//! Large-workload suites and the release performance gate (#135). The suites interleave A/B
//! rounds (flipping the order each round to cancel cache and clock bias); the gate only enforces
//! ratios in release builds, and with loose floors so shared CI runners do not flake.

use crate::benchmark::{
    recalculate_all_uncompiled_baseline, BenchmarkSuiteResult, EngineBenchmarkReport,
};
use crate::benchmark_real::RELEASE_GATING;
use easyoffice_formats::IeeeCrc32;
use easyoffice_sheet::{EvaluatedCellValue, PackedCellCoordinate, SheetEngine};
use std::collections::HashMap;
use std::hint::black_box;
use std::time::Instant;

const CRC_LARGE_BYTES: usize = 1 << 20;
const RECALC_ROWS: u32 = 10_000;

fn percentile_stats(mut samples: Vec<u64>) -> (u64, u64, u64, u64) {
    samples.sort_unstable();
    let n = samples.len().max(1);
    let at = |i: usize| samples.get(i.min(n - 1)).copied().unwrap_or(1).max(1);
    (
        at(n / 2),
        at((n * 95).saturating_sub(1) / 100),
        at(0),
        at(n - 1),
    )
}

#[allow(clippy::too_many_arguments)]
fn suite(
    suite_id: &'static str,
    title: &'static str,
    workload_description: &'static str,
    rounds: usize,
    base: Vec<u64>,
    opt: Vec<u64>,
    equivalence_verified: bool,
    digest_hex: String,
) -> BenchmarkSuiteResult {
    let (bp50, bp95, bmin, bmax) = percentile_stats(base);
    let (op50, op95, omin, omax) = percentile_stats(opt);
    BenchmarkSuiteResult {
        suite_id,
        title,
        workload_description,
        iterations: rounds,
        baseline_p50_us: bp50,
        baseline_p95_us: bp95,
        baseline_min_us: bmin,
        baseline_max_us: bmax,
        optimized_p50_us: op50,
        optimized_p95_us: op95,
        optimized_min_us: omin,
        optimized_max_us: omax,
        speedup_factor: ((bp50 as f64) / (op50 as f64) * 100.0).round() / 100.0,
        equivalence_verified,
        digest_hex,
    }
}

fn pseudo_random_bytes(len: usize) -> Vec<u8> {
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

/// Byte-at-a-time table CRC versus the runtime-selected (hardware when available) CRC on a
/// 1 MiB buffer, plus the 4 KiB / 64 KiB scalar-vs-reference equivalence.
pub fn bench_crc32_hardware(rounds: usize) -> BenchmarkSuiteResult {
    let payload = pseudo_random_bytes(CRC_LARGE_BYTES);
    let mut eq = [4_096usize, 65_536, CRC_LARGE_BYTES].iter().all(|&n| {
        IeeeCrc32::checksum(&payload[..n])
            == IeeeCrc32::checksum_byte_by_byte_baseline(&payload[..n])
    });
    let (mut base, mut opt) = (Vec::new(), Vec::new());
    let mut digest = 0u32;
    for round in 0..=rounds {
        let run_base = || {
            let t = Instant::now();
            let v = IeeeCrc32::checksum_byte_by_byte_baseline(black_box(&payload));
            (t.elapsed().as_micros() as u64, v)
        };
        let run_opt = || {
            let t = Instant::now();
            let v = IeeeCrc32::checksum(black_box(&payload));
            (t.elapsed().as_micros() as u64, v)
        };
        let ((dt_a, a), (dt_b, b)) = if round % 2 == 0 {
            let a = run_base();
            (a, run_opt())
        } else {
            let b = run_opt();
            (run_base(), b)
        };
        eq &= a == b;
        digest = b;
        if round > 0 {
            base.push(dt_a.max(1));
            opt.push(dt_b.max(1));
        }
    }
    suite(
        "crc32_hardware_1mib",
        "Runtime-detected CRC-32 (PCLMULQDQ / CRC32 instruction, scalar fallback)",
        "1 MiB pseudo-random buffer, byte-wise table baseline vs dispatch path",
        rounds,
        base,
        opt,
        eq,
        format!("{}:{digest:08x}", IeeeCrc32::hardware_path_name()),
    )
}

pub(crate) fn build_recalc_sheet(rows: u32) -> SheetEngine {
    let mut sheet = SheetEngine::new("Recalc10k");
    for r in 1..=rows {
        let lo = r.saturating_sub(9).max(1);
        sheet
            .set_cell(&format!("A{r}"), format!("{}", r * 10))
            .unwrap();
        sheet
            .set_cell(&format!("B{r}"), format!("=A{r}*1.15+10"))
            .unwrap();
        sheet
            .set_cell(
                &format!("C{r}"),
                format!("=IF(B{r}>100, A{r}+B{r}, B{r}*2)"),
            )
            .unwrap();
        sheet
            .set_cell(&format!("D{r}"), format!("=SUM(A{r}:C{r})/3"))
            .unwrap();
        sheet
            .set_cell(&format!("E{r}"), format!("=SUM(D{lo}:D{r})"))
            .unwrap();
    }
    sheet
}

fn values_close(a: &EvaluatedCellValue, b: &EvaluatedCellValue) -> bool {
    match (a, b) {
        (EvaluatedCellValue::Number(x), EvaluatedCellValue::Number(y)) => {
            (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1.0)
        }
        _ => a == b,
    }
}

fn maps_close(
    a: &HashMap<PackedCellCoordinate, EvaluatedCellValue>,
    b: &HashMap<PackedCellCoordinate, EvaluatedCellValue>,
) -> bool {
    a.len() == b.len()
        && a.iter()
            .all(|(k, v)| b.get(k).is_some_and(|w| values_close(v, w)))
}

/// 10,000-row workbook recalculation: per-evaluation compile baseline vs the slot-memo engine.
pub fn bench_sheet_recalc_10k(rounds: usize) -> BenchmarkSuiteResult {
    let rounds = rounds.min(5);
    let sheet = build_recalc_sheet(RECALC_ROWS);
    let (mut base, mut opt) = (Vec::new(), Vec::new());
    let mut eq = true;
    let mut digest = 0u64;
    for round in 0..=rounds {
        let run_base = || {
            let t = Instant::now();
            let m = recalculate_all_uncompiled_baseline(black_box(&sheet));
            (t.elapsed().as_micros() as u64, m)
        };
        let run_opt = || {
            let t = Instant::now();
            let m = black_box(&sheet).recalculate_all();
            (t.elapsed().as_micros() as u64, m)
        };
        let ((dt_a, map_a), (dt_b, map_b)) = if round % 2 == 0 {
            let a = run_base();
            (a, run_opt())
        } else {
            let b = run_opt();
            (run_base(), b)
        };
        eq &= maps_close(&map_a, &map_b);
        let last = PackedCellCoordinate::parse_a1(&format!("E{RECALC_ROWS}")).unwrap();
        if let Some(n) = map_b.get(&last).and_then(EvaluatedCellValue::as_number) {
            digest = n.to_bits();
        }
        if round > 0 {
            base.push(dt_a.max(1));
            opt.push(dt_b.max(1));
        }
    }
    suite(
        "sheet_recalc_10k_rows",
        "10,000-row recalculation (slot memo + cached plan)",
        "10,000 rows x 5 columns (50,000 cells, windowed SUM ranges), one full recalculation",
        rounds,
        base,
        opt,
        eq,
        format!("{digest:016x}"),
    )
}

/// Loose release-only floors: far below the measured speedups so runner noise cannot flip them.
pub fn performance_gate(report: &EngineBenchmarkReport) -> Result<(), String> {
    if !report.all_equivalent {
        return Err("동등성 검증 실패".to_string());
    }
    if !RELEASE_GATING {
        return Ok(());
    }
    let floors = [
        ("crc32_slicing_by_8", 2.0),
        ("crc32_hardware_1mib", 2.0),
        ("sheet_dag_bitset_eval", 1.1),
        ("sheet_recalc_10k_rows", 2.0),
    ];
    for (id, floor) in floors {
        let s = report
            .suites
            .iter()
            .find(|s| s.suite_id == id)
            .ok_or_else(|| format!("{id}: 벤치마크 스위트 누락"))?;
        if s.speedup_factor < floor {
            return Err(format!(
                "{id}: 속도 향상 {:.2}배가 느슨한 하한 {floor:.1}배 미만",
                s.speedup_factor
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_suites_are_equivalent_and_report_their_ids() {
        let crc = bench_crc32_hardware(3);
        let recalc = bench_sheet_recalc_10k(3);
        eprintln!("{crc:?}\n{recalc:?}");
        assert!(crc.equivalence_verified && recalc.equivalence_verified);
        assert_eq!(crc.suite_id, "crc32_hardware_1mib");
        assert_eq!(recalc.suite_id, "sheet_recalc_10k_rows");
    }

    #[test]
    fn gate_never_rejects_ratios_in_debug_builds_but_always_rejects_inequivalence() {
        let mut report = EngineBenchmarkReport {
            iterations: 3,
            all_equivalent: true,
            suites: Vec::new(),
            real_paths: Vec::new(),
        };
        if !RELEASE_GATING {
            assert!(performance_gate(&report).is_ok());
        }
        report.all_equivalent = false;
        assert!(performance_gate(&report).is_err());
    }
}
