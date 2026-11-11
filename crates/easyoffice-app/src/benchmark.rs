use easyoffice_formats::{
    adler32, decode_utf16le_swar, decode_utf16le_vec_baseline, deflate_zlib,
    encode_utf16le_scalar_baseline, encode_utf16le_swar, inflate_zlib,
    unescape_xml_five_pass_baseline, unescape_xml_single_pass, CsvCodec, IeeeCrc32, XmlSanitizer,
};
use easyoffice_sheet::{
    wildcard_match, wildcard_match_alloc_baseline, CellRange, EvaluatedCellValue,
    PackedCellCoordinate, SheetEngine,
};
use std::hint::black_box;
use std::time::Instant;

use crate::benchmark_baselines::{
    deflate_zlib_scalar_baseline, inflate_zlib_scalar_baseline,
    recalculate_all_uncompiled_baseline, scalar_vec_range_agg_baseline,
};
use crate::benchmark_real::{RealPathResult, RELEASE_GATING};

/// Single benchmark suite result following the 7-point `benchmark-checklist` contract.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkSuiteResult {
    pub suite_id: &'static str,
    pub title: &'static str,
    pub workload_description: &'static str,
    pub iterations: usize,
    pub baseline_p50_us: u64,
    pub baseline_p95_us: u64,
    pub baseline_min_us: u64,
    pub baseline_max_us: u64,
    pub optimized_p50_us: u64,
    pub optimized_p95_us: u64,
    pub optimized_min_us: u64,
    pub optimized_max_us: u64,
    pub speedup_factor: f64,
    pub equivalence_verified: bool,
    pub digest_hex: String,
}

/// Complete deterministic benchmark report across all 4 core engine subsystems.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineBenchmarkReport {
    pub iterations: usize,
    pub all_equivalent: bool,
    pub suites: Vec<BenchmarkSuiteResult>,
    /// Real-workload cases (#124); empty until `run_real_path_benchmarks` results are attached.
    pub real_paths: Vec<RealPathResult>,
}

impl EngineBenchmarkReport {
    pub fn summary_line(&self) -> String {
        let parts: Vec<String> = self
            .suites
            .iter()
            .map(|s| {
                format!(
                    "{}:{}us->{}us({:.2}x)",
                    s.suite_id, s.baseline_p50_us, s.optimized_p50_us, s.speedup_factor
                )
            })
            .collect();
        let real: Vec<String> = self
            .real_paths
            .iter()
            .map(RealPathResult::summary)
            .collect();
        format!(
            "bench_rounds={} eq={} [{}] real[{}]",
            self.iterations,
            self.all_equivalent,
            parts.join(", "),
            real.join(", ")
        )
    }

    pub fn to_json(&self) -> String {
        let suites_json: Vec<String> = self
            .suites
            .iter()
            .map(|s| {
                let (id, title, wl, it) = (s.suite_id, s.title, s.workload_description, s.iterations);
                let (bp50, bp95, bmin, bmax) = (s.baseline_p50_us, s.baseline_p95_us, s.baseline_min_us, s.baseline_max_us);
                let (op50, op95, omin, omax) = (s.optimized_p50_us, s.optimized_p95_us, s.optimized_min_us, s.optimized_max_us);
                let (spd, eq, dig) = (s.speedup_factor, s.equivalence_verified, &s.digest_hex);
                format!(
                    "{{\"suiteId\":\"{id}\",\"title\":\"{title}\",\"workload\":\"{wl}\",\"iterations\":{it},\"baselineP50Us\":{bp50},\"baselineP95Us\":{bp95},\"baselineMinUs\":{bmin},\"baselineMaxUs\":{bmax},\"optimizedP50Us\":{op50},\"optimizedP95Us\":{op95},\"optimizedMinUs\":{omin},\"optimizedMaxUs\":{omax},\"speedupFactor\":{spd:.2},\"equivalenceVerified\":{eq},\"digestHex\":\"{dig}\"}}"
                )
            })
            .collect();
        let real_json: Vec<String> = self
            .real_paths
            .iter()
            .map(RealPathResult::to_json)
            .collect();
        let profile = if RELEASE_GATING { "release" } else { "debug" };
        format!(
            "{{\"status\":\"ok\",\"iterations\":{},\"allEquivalent\":{},\"buildProfile\":\"{}\",\"timingGated\":{},\"suites\":[{}],\"realPaths\":[{}]}}",
            self.iterations,
            self.all_equivalent,
            profile,
            RELEASE_GATING,
            suites_json.join(","),
            real_json.join(",")
        )
    }
}

fn compute_stats(mut samples_us: Vec<u64>) -> (u64, u64, u64, u64) {
    samples_us.sort_unstable();
    let n = samples_us.len();
    if n == 0 {
        return (1, 1, 1, 1);
    }
    let min_v = samples_us[0].max(1);
    let max_v = samples_us[n - 1].max(1);
    let p50_v = samples_us[n / 2].max(1);
    let p95_idx = ((n * 95).saturating_sub(1) / 100).min(n - 1);
    let p95_v = samples_us[p95_idx].max(1);
    (p50_v, p95_v, min_v, max_v)
}

/// Runs all 4 deterministic benchmarks across `iterations` interleaved A/B rounds.
pub fn run_deterministic_benchmarks(iterations: usize) -> EngineBenchmarkReport {
    let rounds = iterations.clamp(3, 25);

    let mut crc_payload = Vec::with_capacity(65_536);
    while crc_payload.len() < 65_536 {
        crc_payload.extend_from_slice(
            b"PK\x03\x04<w:p><w:r><w:t>EasyOffice Slicing-by-8 IEEE 802.3 CRC-32 Benchmark</w:t></w:r></w:p>",
        );
    }
    crc_payload.truncate(65_536);

    let mut crc_base_samples = Vec::with_capacity(rounds);
    let mut crc_opt_samples = Vec::with_capacity(rounds);
    let mut crc_eq = true;
    let mut crc_digest = 0u32;

    for r in 0..=rounds {
        let t0 = Instant::now();
        let mut a = 0u32;
        for _ in 0..16 {
            a ^= IeeeCrc32::checksum_byte_by_byte_baseline(black_box(&crc_payload));
        }
        let dt_a = t0.elapsed().as_micros() as u64;

        let t1 = Instant::now();
        let mut b = 0u32;
        for _ in 0..16 {
            b ^= IeeeCrc32::checksum_scalar(black_box(&crc_payload));
        }
        let dt_b = t1.elapsed().as_micros() as u64;

        if a != b {
            crc_eq = false;
        }
        crc_digest = b;
        if r > 0 {
            crc_base_samples.push(dt_a.max(1));
            crc_opt_samples.push(dt_b.max(1));
        }
    }

    let mut ooxml_payload = Vec::with_capacity(32_768);
    while ooxml_payload.len() < 32_768 {
        ooxml_payload.extend_from_slice(
            b"<w:p><w:pPr><w:pStyle w:val=\"BodyText\"/></w:pPr><w:r><w:t>Standard ECMA-376 OOXML &amp; KS X 6101 HWPX deterministic compression</w:t></w:r></w:p>",
        );
    }
    ooxml_payload.truncate(32_768);

    let mut def_base_samples = Vec::with_capacity(rounds);
    let mut def_opt_samples = Vec::with_capacity(rounds);
    let mut def_eq = true;
    let mut def_digest = 0u32;

    for r in 0..=rounds {
        let t0 = Instant::now();
        let zlib_bytes_a = deflate_zlib_scalar_baseline(black_box(&ooxml_payload));
        let dec_a =
            inflate_zlib_scalar_baseline(black_box(&zlib_bytes_a), 65_536).unwrap_or_default();
        let dt_a = t0.elapsed().as_micros() as u64;

        let t1 = Instant::now();
        let zlib_bytes_b = deflate_zlib(black_box(&ooxml_payload));
        let dec_b = inflate_zlib(black_box(&zlib_bytes_b), 65_536).unwrap_or_default();
        let dt_b = t1.elapsed().as_micros() as u64;

        // The baseline is the original fixed-Huffman compressor; the production compressor now
        // emits dynamic Huffman blocks, so the streams differ. Equivalence means both streams
        // decode (with either decoder) to the input and the production stream is not larger.
        if zlib_bytes_b.len() > zlib_bytes_a.len()
            || dec_a != ooxml_payload
            || dec_b != ooxml_payload
            || inflate_zlib(&zlib_bytes_a, 65_536).unwrap_or_default() != ooxml_payload
        {
            def_eq = false;
        }
        def_digest = adler32(&dec_b);
        if r > 0 {
            def_base_samples.push(dt_a.max(1));
            def_opt_samples.push(dt_b.max(1));
        }
    }

    let sample_text =
        "EasyOffice Desktop Suite 행정안전부 표준 전자문서 호환성 검증 A&B <C> \"D\" 'E' "
            .repeat(220);
    let utf16_bytes = encode_utf16le_swar(&sample_text);
    let escaped_xml = easyoffice_formats::XmlSanitizer::escape(&sample_text);

    let mut utf_base_samples = Vec::with_capacity(rounds);
    let mut utf_opt_samples = Vec::with_capacity(rounds);
    let mut utf_eq = true;
    let mut utf_digest = 0u32;

    for r in 0..=rounds {
        let t0 = Instant::now();
        let enc_a = encode_utf16le_scalar_baseline(black_box(&sample_text));
        let u_a = decode_utf16le_vec_baseline(black_box(&enc_a)).unwrap_or_default();
        let x_a = unescape_xml_five_pass_baseline(black_box(&escaped_xml));
        let dt_a = t0.elapsed().as_micros() as u64;

        let t1 = Instant::now();
        let enc_b = encode_utf16le_swar(black_box(&sample_text));
        let u_b = decode_utf16le_swar(black_box(&enc_b)).unwrap_or_default();
        let x_b = unescape_xml_single_pass(black_box(&escaped_xml));
        let dt_b = t1.elapsed().as_micros() as u64;

        if enc_a != enc_b || enc_b != utf16_bytes || u_a != u_b || x_a != x_b || u_b != sample_text
        {
            utf_eq = false;
        }
        utf_digest = IeeeCrc32::checksum(x_b.as_bytes());
        if r > 0 {
            utf_base_samples.push(dt_a.max(1));
            utf_opt_samples.push(dt_b.max(1));
        }
    }

    let mut sheet = SheetEngine::new("BenchSheet");
    let mut coords = Vec::with_capacity(200);
    for r in 1..=50 {
        for (col, raw) in [
            ("A", format!("{}", r * 10)),
            ("B", format!("=A{r}*1.15+10")),
            ("C", format!("=IF(B{r}>100, A{r}+B{r}, B{r}*2)")),
            ("D", format!("=SUM(A1:C{r})/3")),
        ] {
            let a1 = format!("{col}{r}");
            sheet.set_cell(&a1, raw).unwrap();
            coords.push((PackedCellCoordinate::parse_a1(&a1).unwrap(), a1));
        }
    }

    let target_coord = coords.last().unwrap().0;
    let mut sheet_base_samples = Vec::with_capacity(rounds);
    let mut sheet_opt_samples = Vec::with_capacity(rounds);
    let mut sheet_eq = true;
    let mut sheet_digest = 0u64;

    for r in 0..=rounds {
        let t0 = Instant::now();
        let mut last_a = EvaluatedCellValue::Empty;
        for _ in 0..4 {
            let map_a = recalculate_all_uncompiled_baseline(black_box(&sheet));
            if let Some(v) = map_a.get(&target_coord) {
                last_a = v.clone();
            }
        }
        let dt_a = t0.elapsed().as_micros() as u64;

        let t1 = Instant::now();
        let mut last_b = EvaluatedCellValue::Empty;
        for _ in 0..4 {
            let map_b = black_box(&sheet).recalculate_all();
            if let Some(v) = map_b.get(&target_coord) {
                last_b = v.clone();
            }
        }
        let dt_b = t1.elapsed().as_micros() as u64;

        if last_a != last_b {
            sheet_eq = false;
        }
        if let Some(n) = last_b.as_number() {
            sheet_digest = n.to_bits();
        }
        if r > 0 {
            sheet_base_samples.push(dt_a.max(1));
            sheet_opt_samples.push(dt_b.max(1));
        }
    }

    let build = |id, title, desc, base: Vec<u64>, opt: Vec<u64>, eq, digest| {
        let (bp50, bp95, bmin, bmax) = compute_stats(base);
        let (op50, op95, omin, omax) = compute_stats(opt);
        let speedup = ((bp50 as f64) / (op50.max(1) as f64) * 100.0).round() / 100.0;
        BenchmarkSuiteResult {
            suite_id: id,
            title,
            workload_description: desc,
            iterations: rounds,
            baseline_p50_us: bp50,
            baseline_p95_us: bp95,
            baseline_min_us: bmin,
            baseline_max_us: bmax,
            optimized_p50_us: op50,
            optimized_p95_us: op95,
            optimized_min_us: omin,
            optimized_max_us: omax,
            speedup_factor: speedup,
            equivalence_verified: eq,
            digest_hex: digest,
        }
    };

    let suites = vec![
        build(
            "crc32_slicing_by_8",
            "Scalar slicing-by-16 IEEE 802.3 CRC-32",
            "1 MiB (64 KiB x 16) ZIP/OOXML container stream, scalar path only",
            crc_base_samples,
            crc_opt_samples,
            crc_eq,
            format!("{crc_digest:08x}"),
        ),
        build(
            "deflate_zlib_roundtrip",
            "RFC 1951/1950 DEFLATE + Adler-32",
            "32 KiB ECMA-376 OOXML XML compression + 9-bit LUT inflate",
            def_base_samples,
            def_opt_samples,
            def_eq,
            format!("{def_digest:08x}"),
        ),
        build(
            "utf16_swar_and_xml_unescape",
            "64-Bit SWAR UTF-16LE & Single-Pass XML",
            "16 KiB bilingual Korean/ASCII UTF-16LE + XML entity stream",
            utf_base_samples,
            utf_opt_samples,
            utf_eq,
            format!("{utf_digest:08x}"),
        ),
        build(
            "sheet_dag_bitset_eval",
            "Pre-Compiled AST + Bitset DAG Sheet",
            "200 interdependent cells x 4 recalculation passes (800 evals)",
            sheet_base_samples,
            sheet_opt_samples,
            sheet_eq,
            format!("{sheet_digest:016x}"),
        ),
        bench_swar_csv_xml_and_zero_alloc_range_agg(rounds),
        crate::benchmark_large::bench_crc32_hardware(rounds),
        crate::benchmark_large::bench_sheet_recalc_10k(rounds),
    ];

    let all_equivalent = suites.iter().all(|s| s.equivalence_verified);
    EngineBenchmarkReport {
        iterations: rounds,
        all_equivalent,
        suites,
        real_paths: Vec::new(),
    }
}

pub fn bench_swar_csv_xml_and_zero_alloc_range_agg(iterations: usize) -> BenchmarkSuiteResult {
    let rounds = iterations.clamp(3, 25);

    let mut xml_doc = String::with_capacity(16_384);
    xml_doc.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document><w:body>");
    while xml_doc.len() < 16_300 {
        xml_doc.push_str(
            "<w:p><w:r><w:t>기획조정실 분기별 예산집행 및 디지털행정혁신과 성과 보고서 표준 문단</w:t></w:r></w:p>",
        );
    }
    xml_doc.push_str("</w:body></w:document>");

    let raw_escape_input =
        "기획조정실 A&B <예산집행> \"정상\" '승인' 디지털행정혁신과 2026 분기별 실적 요약 "
            .repeat(96);

    let mut csv_lines = Vec::with_capacity(200);
    for r in 1..=200 {
        csv_lines.push(format!(
            "기획조정실_{r:03},{},{},{},{},\"예산집행, 분기 \"\"승인\"\"\",98.5,정상",
            r * 100,
            r * 120,
            r * 140,
            r * 160
        ));
    }
    let csv_doc = csv_lines.join("\r\n");

    let mut agg_sheet = SheetEngine::new("RangeAggBench");
    let col_letters = ["A", "B", "C", "D", "E", "F", "G", "H"];
    for r in 1..=200u32 {
        for (c_idx, col) in col_letters.iter().enumerate() {
            let val = (r * 10 + (c_idx as u32 + 1)) as f64;
            agg_sheet
                .set_cell(&format!("{col}{r}"), format!("{val}"))
                .unwrap();
        }
    }
    agg_sheet.set_cell("I1", "=SUM(A1:H200)").unwrap();
    agg_sheet.set_cell("I2", "=AVERAGE(A1:H200)").unwrap();
    agg_sheet.set_cell("I3", "=MIN(A1:H200)").unwrap();
    agg_sheet.set_cell("I4", "=MAX(A1:H200)").unwrap();
    agg_sheet.set_cell("I5", "=COUNT(A1:H200)").unwrap();
    agg_sheet
        .set_cell("I6", "=SUMIF(A1:H200, \">1000\")")
        .unwrap();
    agg_sheet
        .set_cell("I7", "=COUNTIF(A1:H200, \">=500\")")
        .unwrap();
    agg_sheet
        .set_cell("I8", "=SUMPRODUCT(A1:D200, E1:H200)")
        .unwrap();
    agg_sheet.set_cell("I9", "=STDEV(A1:H200)").unwrap();
    agg_sheet
        .set_cell("I10", "=XLOOKUP(1501, A1:A200, H1:H200, 0)")
        .unwrap();
    agg_sheet
        .set_cell("I11", "=MATCH(1501, A1:A200, 0)")
        .unwrap();
    let agg_range = CellRange::parse("A1:H200").unwrap();
    let left_range = CellRange::parse("A1:D200").unwrap();
    let right_range = CellRange::parse("E1:H200").unwrap();
    let col_a_range = CellRange::parse("A1:A200").unwrap();
    let col_h_range = CellRange::parse("H1:H200").unwrap();
    let precompiled_cells = agg_sheet.recalculate_all();

    let wildcard_pairs = [
        (
            "행정안전부_디지털정부실_2026_보고서",
            "행정*디지털*2026*보고서",
        ),
        ("기획조정실_예산_A1", "기획조정실_예산_?1"),
        ("특수문자*포함_텍스트", "특수문자~*포함_*"),
        ("물음표?이스케이프", "물음표~?이스케이프"),
    ];

    let mut base_samples = Vec::with_capacity(rounds);
    let mut opt_samples = Vec::with_capacity(rounds);
    let mut eq = true;
    let mut digest = 0u32;

    for r in 0..=rounds {
        let t0 = Instant::now();
        let xml_ok_a = XmlSanitizer::validate_safe_xml_scalar_baseline(black_box(&xml_doc)).is_ok();
        let esc_a = XmlSanitizer::escape_scalar_baseline(black_box(&raw_escape_input));
        let unesc_a = unescape_xml_five_pass_baseline(black_box(&esc_a));
        let csv_a = CsvCodec::parse_scalar_baseline(black_box(&csv_doc), ',').unwrap_or_default();
        let mut csv_fields_a = 0usize;
        let mut csv_bytes_a = 0usize;
        for row in &csv_a {
            for field in row {
                csv_fields_a += 1;
                csv_bytes_a += field.len();
            }
        }
        let agg_a = scalar_vec_range_agg_baseline(
            black_box(&precompiled_cells),
            black_box(&agg_range),
            black_box(&left_range),
            black_box(&right_range),
            black_box(&col_a_range),
            black_box(&col_h_range),
        );
        let mut wc_hits_a = 0usize;
        for _ in 0..32 {
            for &(txt, pat) in &wildcard_pairs {
                if wildcard_match_alloc_baseline(black_box(txt), black_box(pat)) {
                    wc_hits_a += 1;
                }
            }
        }
        let dt_a = t0.elapsed().as_micros() as u64;

        let t1 = Instant::now();
        let xml_ok_b = XmlSanitizer::validate_safe_xml(black_box(&xml_doc)).is_ok();
        let esc_b = XmlSanitizer::escape(black_box(&raw_escape_input));
        let unesc_b = unescape_xml_single_pass(black_box(&esc_b));
        let mut csv_rows_b = 0usize;
        let mut csv_fields_b = 0usize;
        let mut csv_bytes_b = 0usize;
        let csv_ok_b = CsvCodec::visit_fields_swar(black_box(&csv_doc), ',', |r_idx, _c_idx, f| {
            csv_rows_b = csv_rows_b.max(r_idx + 1);
            csv_fields_b += 1;
            csv_bytes_b += f.len();
        })
        .is_ok();
        let agg_b = [
            agg_sheet.evaluate_cell("I1").as_number().unwrap_or(0.0),
            agg_sheet.evaluate_cell("I2").as_number().unwrap_or(0.0),
            agg_sheet.evaluate_cell("I3").as_number().unwrap_or(0.0),
            agg_sheet.evaluate_cell("I4").as_number().unwrap_or(0.0),
            agg_sheet.evaluate_cell("I5").as_number().unwrap_or(0.0),
            agg_sheet.evaluate_cell("I6").as_number().unwrap_or(0.0),
            agg_sheet.evaluate_cell("I7").as_number().unwrap_or(0.0),
            agg_sheet.evaluate_cell("I8").as_number().unwrap_or(0.0),
            agg_sheet.evaluate_cell("I9").as_number().unwrap_or(0.0),
            agg_sheet.evaluate_cell("I10").as_number().unwrap_or(0.0),
            agg_sheet.evaluate_cell("I11").as_number().unwrap_or(0.0),
        ];
        let mut wc_hits_b = 0usize;
        for _ in 0..32 {
            for &(txt, pat) in &wildcard_pairs {
                if wildcard_match(black_box(txt), black_box(pat)) {
                    wc_hits_b += 1;
                }
            }
        }
        let dt_b = t1.elapsed().as_micros() as u64;

        let agg_match = agg_a
            .iter()
            .zip(agg_b.iter())
            .all(|(a, b)| (a - b).abs() < 1e-6);
        let csv_match = csv_ok_b
            && csv_a.len() == csv_rows_b
            && csv_fields_a == csv_fields_b
            && csv_bytes_a == csv_bytes_b;
        if !xml_ok_a
            || !xml_ok_b
            || esc_a != esc_b
            || unesc_a != unesc_b
            || unesc_b != raw_escape_input
            || !csv_match
            || !agg_match
            || wc_hits_a != wc_hits_b
            || wc_hits_b != 128
        {
            eq = false;
        }
        digest = IeeeCrc32::checksum(unesc_b.as_bytes())
            ^ (csv_bytes_b as u32)
            ^ (agg_b[0] as u32)
            ^ (agg_b[7] as u32)
            ^ (agg_b[8] as u32)
            ^ (agg_b[9] as u32)
            ^ (agg_b[10] as u32)
            ^ (wc_hits_b as u32);
        if r > 0 {
            base_samples.push(dt_a.max(1));
            opt_samples.push(dt_b.max(1));
        }
    }

    let (bp50, bp95, bmin, bmax) = compute_stats(base_samples);
    let (op50, op95, omin, omax) = compute_stats(opt_samples);
    let speedup = ((bp50 as f64) / (op50.max(1) as f64) * 100.0).round() / 100.0;
    BenchmarkSuiteResult {
        suite_id: "swar_csv_xml_and_zero_alloc_range_agg",
        title: "64-Bit SWAR XML/CSV & Zero-Alloc Range Agg",
        workload_description:
            "16 KiB XML + 200x8 CSV + 1,600-cell streaming SUM/AVG/MIN/MAX/COUNT/SUMIF/COUNTIF/SUMPRODUCT/STDEV/XLOOKUP/MATCH + Wildcard",
        iterations: rounds,
        baseline_p50_us: bp50,
        baseline_p95_us: bp95,
        baseline_min_us: bmin,
        baseline_max_us: bmax,
        optimized_p50_us: op50,
        optimized_p95_us: op95,
        optimized_min_us: omin,
        optimized_max_us: omax,
        speedup_factor: speedup,
        equivalence_verified: eq,
        digest_hex: format!("{digest:08x}"),
    }
}
