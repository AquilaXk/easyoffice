//! Real-workload benchmarks (#124): they time the production conversion, pack/unpack and
//! recalculation paths (including DEFLATE compression) instead of small synthetic inputs, and
//! report both elapsed time and output size.

use easyoffice_formats::{
    DocumentFormat, IeeeCrc32, UniversalDocumentModel, UniversalFormatConverter,
};
use easyoffice_sheet::SheetEngine;
use std::hint::black_box;
use std::time::Instant;

/// One real-path measurement.
#[derive(Debug, Clone, PartialEq)]
pub struct RealPathResult {
    pub case_id: &'static str,
    pub workload: String,
    pub input_bytes: usize,
    pub output_bytes: usize,
    pub elapsed_us: u64,
    pub equivalence_verified: bool,
}

impl RealPathResult {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"caseId\":\"{}\",\"workload\":\"{}\",\"inputBytes\":{},\"outputBytes\":{},\"elapsedUs\":{},\"equivalenceVerified\":{}}}",
            self.case_id,
            self.workload,
            self.input_bytes,
            self.output_bytes,
            self.elapsed_us,
            self.equivalence_verified
        )
    }

    pub fn summary(&self) -> String {
        format!(
            "{}:{}us/{}B",
            self.case_id, self.elapsed_us, self.output_bytes
        )
    }
}

/// Release builds gate on loose bounds; debug builds report the numbers without asserting timing.
pub const RELEASE_GATING: bool = !cfg!(debug_assertions);

const DOC_PARAGRAPHS: usize = 9_700;
const DOC_OUTPUT_CEILING_BYTES: usize = 500_000;
const XLSX_ROWS: u32 = 1_000;
const XLSX_COLS: u32 = 104;
const XLSX_OUTPUT_CEILING_BYTES: usize = 3_000_000;
const RECALC_TARGETS: u32 = 5_000;
const RELEASE_TIME_CEILING_US: u64 = 10_000_000;

fn big_document_model() -> UniversalDocumentModel {
    let words = [
        "기획조정실",
        "예산집행",
        "디지털",
        "행정혁신",
        "report",
        "budget",
        "분기별",
        "성과",
        "표준",
        "문서",
        "Quarterly",
        "alignment",
        "검토",
        "승인",
        "2026",
        "plan",
    ];
    let mut seed = 0x2545_F491u32;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as usize
    };
    let paragraphs = (0..DOC_PARAGRAPHS)
        .map(|i| {
            let mut s = format!("{}. ", i + 1);
            for _ in 0..8 + next() % 6 {
                s.push_str(words[next() % words.len()]);
                s.push(' ');
            }
            s
        })
        .collect();
    UniversalDocumentModel {
        title: "대용량 문서".to_string(),
        paragraphs,
        ..Default::default()
    }
}

fn document_case(
    case_id: &'static str,
    source: &[u8],
    target: DocumentFormat,
    expected: &[String],
) -> RealPathResult {
    let t0 = Instant::now();
    let (_, decoded) = UniversalFormatConverter::decode(black_box(source), Some("docx"))
        .expect("benchmark source decodes");
    let out = UniversalFormatConverter::encode(black_box(&decoded), target)
        .expect("benchmark target encodes");
    let elapsed_us = t0.elapsed().as_micros() as u64;
    let verified = UniversalFormatConverter::decode(&out, Some(target.extension()))
        .map(|(_, m)| m.paragraphs == expected)
        .unwrap_or(false);
    RealPathResult {
        case_id,
        workload: format!(
            "{DOC_PARAGRAPHS}-paragraph document, decode + encode {}",
            target.extension()
        ),
        input_bytes: source.len(),
        output_bytes: out.len(),
        elapsed_us: elapsed_us.max(1),
        equivalence_verified: verified && out.len() < DOC_OUTPUT_CEILING_BYTES,
    }
}

fn xlsx_case() -> RealPathResult {
    let mut cells = Vec::with_capacity((XLSX_ROWS * XLSX_COLS) as usize);
    for r in 0..XLSX_ROWS {
        for c in 0..XLSX_COLS {
            cells.push((r, c, format!("{}", r * 131 + c * 7)));
        }
    }
    let model = UniversalDocumentModel {
        title: "대형 시트".to_string(),
        sheet_cells: cells.clone(),
        ..Default::default()
    };
    let t0 = Instant::now();
    let packed = UniversalFormatConverter::encode(black_box(&model), DocumentFormat::Xlsx)
        .expect("xlsx packs");
    let (_, decoded) =
        UniversalFormatConverter::decode(black_box(&packed), Some("xlsx")).expect("xlsx unpacks");
    let elapsed_us = t0.elapsed().as_micros() as u64;
    let mut got = decoded.sheet_cells;
    got.sort_unstable_by_key(|c| (c.0, c.1));
    RealPathResult {
        case_id: "xlsx_100k_cells_pack_unpack",
        workload: format!("{XLSX_ROWS}x{XLSX_COLS} cells, pack + unpack"),
        input_bytes: cells.iter().map(|c| c.2.len()).sum(),
        output_bytes: packed.len(),
        elapsed_us: elapsed_us.max(1),
        equivalence_verified: got == cells && packed.len() < XLSX_OUTPUT_CEILING_BYTES,
    }
}

fn recalc_case() -> RealPathResult {
    let mut sheet = SheetEngine::new("Recalc");
    for r in 1..=RECALC_TARGETS {
        sheet
            .set_cell(&format!("A{r}"), format!("{}", r * 3))
            .unwrap();
        sheet
            .set_cell(&format!("B{r}"), format!("=IF(A{r}>100, A{r}*2, A{r}+1)"))
            .unwrap();
    }
    let t0 = Instant::now();
    let results = black_box(&sheet).recalculate_all();
    let elapsed_us = t0.elapsed().as_micros() as u64;
    let check = |r: u32, expected: f64| {
        sheet
            .evaluate_cell(&format!("B{r}"))
            .as_number()
            .is_some_and(|v| (v - expected).abs() < 1e-9)
    };
    RealPathResult {
        case_id: "sheet_recalc_5000_targets",
        workload: format!("{RECALC_TARGETS} formula targets over {RECALC_TARGETS} inputs"),
        input_bytes: 0,
        output_bytes: results.len(),
        elapsed_us: elapsed_us.max(1),
        equivalence_verified: results.len() >= (RECALC_TARGETS * 2) as usize
            && check(1, 4.0)
            && check(RECALC_TARGETS, f64::from(RECALC_TARGETS * 3 * 2)),
    }
}

fn recalc_10k_case() -> RealPathResult {
    let rows = 10_000u32;
    let sheet = crate::benchmark_large::build_recalc_sheet(rows);
    let t0 = Instant::now();
    let results = black_box(&sheet).recalculate_all();
    let elapsed_us = t0.elapsed().as_micros() as u64;
    let expected_d = |r: u32| {
        let a = f64::from(r) * 10.0;
        let b = a * 1.15 + 10.0;
        let c = if b > 100.0 { a + b } else { b * 2.0 };
        (a + b + c) / 3.0
    };
    let expected_e: f64 = (rows - 9..=rows).map(expected_d).sum();
    let e_last = sheet
        .evaluate_cell(&format!("E{rows}"))
        .as_number()
        .is_some_and(|v| (v - expected_e).abs() <= 1e-9 * expected_e.abs());
    RealPathResult {
        case_id: "sheet_recalc_10k_rows",
        workload: format!("{rows} rows x 5 columns with windowed SUM ranges, full recalculation"),
        input_bytes: 0,
        output_bytes: results.len(),
        elapsed_us: elapsed_us.max(1),
        equivalence_verified: results.len() == (rows * 5) as usize && e_last,
    }
}

fn crc_case(case_id: &'static str, bytes: usize, passes: usize) -> RealPathResult {
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let payload: Vec<u8> = (0..bytes)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 16) as u8
        })
        .collect();
    let expected = IeeeCrc32::checksum_byte_by_byte_baseline(&payload);
    let t0 = Instant::now();
    let mut last = 0u32;
    for _ in 0..passes {
        last = IeeeCrc32::checksum(black_box(&payload));
    }
    let elapsed_us = t0.elapsed().as_micros() as u64;
    RealPathResult {
        case_id,
        workload: format!(
            "{bytes} bytes x {passes} passes, CRC-32 ({})",
            IeeeCrc32::hardware_path_name()
        ),
        input_bytes: bytes * passes,
        output_bytes: 4,
        elapsed_us: elapsed_us.max(1),
        equivalence_verified: last == expected,
    }
}

/// Runs the real-path cases once each. Total runtime stays well under 10 s in release builds.
pub fn run_real_path_benchmarks() -> Vec<RealPathResult> {
    let model = big_document_model();
    let source = UniversalFormatConverter::encode(&model, DocumentFormat::Docx)
        .expect("benchmark source docx");
    vec![
        document_case(
            "docx_to_docx_1mb",
            &source,
            DocumentFormat::Docx,
            &model.paragraphs,
        ),
        document_case(
            "docx_to_hwpx_1mb",
            &source,
            DocumentFormat::Hwpx,
            &model.paragraphs,
        ),
        xlsx_case(),
        recalc_case(),
        recalc_10k_case(),
        crc_case("crc32_64kib_x64", 65_536, 64),
        crc_case("crc32_1mib_x8", 1 << 20, 8),
    ]
}

/// Equivalence (and output-size ceilings) always gate; elapsed time is only bounded in release
/// builds, and loosely, so shared CI runners do not flake.
pub fn real_path_gate(results: &[RealPathResult]) -> Result<(), String> {
    for r in results {
        if !r.equivalence_verified {
            return Err(format!("{}: 동등성 또는 출력 크기 상한 위반", r.case_id));
        }
        if RELEASE_GATING && r.elapsed_us > RELEASE_TIME_CEILING_US {
            return Err(format!(
                "{}: {}us가 느슨한 상한을 초과",
                r.case_id, r.elapsed_us
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_paths_report_sizes_and_pass_the_gate() {
        let results = run_real_path_benchmarks();
        for r in &results {
            eprintln!("{}", r.to_json());
        }
        assert_eq!(results.len(), 7);
        real_path_gate(&results).unwrap();
        let docx = &results[0];
        assert!(docx.output_bytes < DOC_OUTPUT_CEILING_BYTES, "{docx:?}");
        assert!(results[1]
            .to_json()
            .contains("\"caseId\":\"docx_to_hwpx_1mb\""));
    }
}
