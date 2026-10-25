use easyoffice_formats::{
    adler32, decode_utf16le_swar, decode_utf16le_vec_baseline, deflate_zlib, encode_utf16le_swar,
    inflate_raw, inflate_zlib, unescape_xml_five_pass_baseline, unescape_xml_single_pass, CsvCodec,
    EasyOfficeError, IeeeCrc32, XmlSanitizer,
};
use easyoffice_sheet::{
    compile_formula_expr, wildcard_match, wildcard_match_alloc_baseline, BinaryOp, CellRange,
    EvaluatedCellValue, Expr, FormulaError, PackedCellCoordinate, SheetEngine,
};
use std::collections::{HashMap, HashSet};
use std::hint::black_box;
use std::time::Instant;

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
        format!(
            "bench_rounds={} eq={} [{}]",
            self.iterations,
            self.all_equivalent,
            parts.join(", ")
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
        format!(
            "{{\"status\":\"ok\",\"iterations\":{},\"allEquivalent\":{},\"suites\":[{}]}}",
            self.iterations,
            self.all_equivalent,
            suites_json.join(",")
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

fn adler32_byte_by_byte_baseline(bytes: &[u8]) -> u32 {
    let mut s1: u32 = 1;
    let mut s2: u32 = 0;
    for &b in bytes {
        s1 = (s1 + u32::from(b)) % 65_521;
        s2 = (s2 + s1) % 65_521;
    }
    (s2 << 16) | s1
}

const BASE_LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const BASE_LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const BASE_DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const BASE_DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

fn write_bits_scalar(out: &mut Vec<u8>, bit_buf: &mut u32, bit_count: &mut u32, val: u32, n: u32) {
    for i in 0..n {
        let bit = (val >> i) & 1;
        *bit_buf |= bit << *bit_count;
        *bit_count += 1;
        if *bit_count == 8 {
            out.push(*bit_buf as u8);
            *bit_buf = 0;
            *bit_count = 0;
        }
    }
}

fn write_huffman_scalar(
    out: &mut Vec<u8>,
    bit_buf: &mut u32,
    bit_count: &mut u32,
    code: u16,
    len: u8,
) {
    let mut rev = 0u32;
    for i in 0..len {
        rev = (rev << 1) | u32::from((code >> i) & 1);
    }
    write_bits_scalar(out, bit_buf, bit_count, rev, u32::from(len));
}

fn write_fixed_sym_scalar(out: &mut Vec<u8>, bit_buf: &mut u32, bit_count: &mut u32, sym: u16) {
    match sym {
        0..=143 => write_huffman_scalar(out, bit_buf, bit_count, 0x30 + sym, 8),
        144..=255 => write_huffman_scalar(out, bit_buf, bit_count, 0x190 + (sym - 144), 9),
        256..=279 => write_huffman_scalar(out, bit_buf, bit_count, sym - 256, 7),
        280..=287 => write_huffman_scalar(out, bit_buf, bit_count, 0xC0 + (sym - 280), 8),
        _ => {}
    }
}

fn deflate_zlib_scalar_baseline(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() / 2 + 24);
    out.extend_from_slice(&[0x78, 0x9C]);
    let mut bit_buf = 0u32;
    let mut bit_count = 0u32;
    write_bits_scalar(&mut out, &mut bit_buf, &mut bit_count, 1, 1);
    write_bits_scalar(&mut out, &mut bit_buf, &mut bit_count, 1, 2);

    let mut head = [-1i32; 4096];
    let mut prev = vec![-1i32; input.len()];
    let mut pos = 0usize;

    while pos < input.len() {
        let mut best_len = 0usize;
        let mut best_dist = 0usize;
        if pos + 3 <= input.len() {
            let h = (((input[pos] as usize) << 8)
                ^ ((input[pos + 1] as usize) << 4)
                ^ (input[pos + 2] as usize))
                & 4095;
            let mut cand = head[h];
            prev[pos] = cand;
            head[h] = pos as i32;
            let max_len = (input.len() - pos).min(258);
            let min_pos = pos.saturating_sub(32_768);
            let mut steps = 0usize;
            while cand >= (min_pos as i32) && steps < 64 {
                let c_pos = cand as usize;
                let dist = pos - c_pos;
                if dist > 0 && dist <= 32_768 {
                    let mut m_len = 0usize;
                    while m_len < max_len && input[c_pos + m_len] == input[pos + m_len] {
                        m_len += 1;
                    }
                    if m_len >= 3 && m_len > best_len {
                        best_len = m_len;
                        best_dist = dist;
                        if best_len == max_len {
                            break;
                        }
                    }
                }
                cand = prev[c_pos];
                steps += 1;
            }
        }

        if best_len >= 3 {
            let mut l_idx = 0usize;
            if best_len == 258 {
                l_idx = 28;
            } else {
                while l_idx + 1 < 28 && (BASE_LEN_BASE[l_idx + 1] as usize) <= best_len {
                    l_idx += 1;
                }
            }
            write_fixed_sym_scalar(&mut out, &mut bit_buf, &mut bit_count, 257 + (l_idx as u16));
            let l_extra = u32::from(BASE_LEN_EXTRA[l_idx]);
            if l_extra > 0 {
                write_bits_scalar(
                    &mut out,
                    &mut bit_buf,
                    &mut bit_count,
                    (best_len - (BASE_LEN_BASE[l_idx] as usize)) as u32,
                    l_extra,
                );
            }

            let mut d_idx = 0usize;
            while d_idx + 1 < 30 && (BASE_DIST_BASE[d_idx + 1] as usize) <= best_dist {
                d_idx += 1;
            }
            write_huffman_scalar(&mut out, &mut bit_buf, &mut bit_count, d_idx as u16, 5);
            let d_extra = u32::from(BASE_DIST_EXTRA[d_idx]);
            if d_extra > 0 {
                write_bits_scalar(
                    &mut out,
                    &mut bit_buf,
                    &mut bit_count,
                    (best_dist - (BASE_DIST_BASE[d_idx] as usize)) as u32,
                    d_extra,
                );
            }

            for step in 1..best_len {
                let adv = pos + step;
                if adv + 3 <= input.len() {
                    let h = (((input[adv] as usize) << 8)
                        ^ ((input[adv + 1] as usize) << 4)
                        ^ (input[adv + 2] as usize))
                        & 4095;
                    prev[adv] = head[h];
                    head[h] = adv as i32;
                }
            }
            pos += best_len;
        } else {
            write_fixed_sym_scalar(
                &mut out,
                &mut bit_buf,
                &mut bit_count,
                u16::from(input[pos]),
            );
            pos += 1;
        }
    }

    write_fixed_sym_scalar(&mut out, &mut bit_buf, &mut bit_count, 256);
    if bit_count > 0 {
        out.push((bit_buf & 0xFF) as u8);
    }
    let checksum = adler32_byte_by_byte_baseline(input);
    out.extend_from_slice(&checksum.to_be_bytes());
    out
}

fn inflate_zlib_scalar_baseline(
    input: &[u8],
    max_output_bytes: usize,
) -> Result<Vec<u8>, EasyOfficeError> {
    if input.len() < 6 {
        return Err(EasyOfficeError::CorruptedContainer(
            "zlib 스트림 길이 부족".to_string(),
        ));
    }
    let decoded = inflate_raw(&input[2..input.len() - 4], max_output_bytes)?;
    let expected = u32::from_be_bytes(input[input.len() - 4..].try_into().unwrap());
    let actual = adler32_byte_by_byte_baseline(&decoded);
    if expected != actual {
        return Err(EasyOfficeError::CorruptedContainer(
            "Adler-32 불일치".to_string(),
        ));
    }
    Ok(decoded)
}

fn eval_cell_uncompiled_baseline(
    sheet: &SheetEngine,
    coord: PackedCellCoordinate,
    memo: &mut HashMap<(String, PackedCellCoordinate), EvaluatedCellValue>,
    visiting: &mut HashSet<(String, PackedCellCoordinate)>,
    depth: usize,
) -> EvaluatedCellValue {
    if depth > SheetEngine::MAX_RECURSION_DEPTH {
        return EvaluatedCellValue::Error(FormulaError::Cycle);
    }
    let key = (sheet.name.to_ascii_lowercase(), coord);
    if let Some(cached) = memo.get(&key) {
        return cached.clone();
    }
    if !visiting.insert(key.clone()) {
        return EvaluatedCellValue::Error(FormulaError::Cycle);
    }
    let res = match sheet.raw_cells().get(&coord) {
        None => EvaluatedCellValue::Empty,
        Some(raw) => {
            let trimmed = raw.trim();
            if let Some(formula_body) = trimmed.strip_prefix('=') {
                let expr_src = formula_body.trim();
                if expr_src.is_empty() {
                    EvaluatedCellValue::Empty
                } else {
                    match compile_formula_expr(expr_src) {
                        Ok(expr) => {
                            eval_expr_uncompiled_baseline(sheet, &expr, memo, visiting, depth + 1)
                        }
                        Err(e) => EvaluatedCellValue::Error(e),
                    }
                }
            } else if let Ok(num) = trimmed.parse::<f64>() {
                if num.is_finite() {
                    EvaluatedCellValue::Number(num)
                } else {
                    EvaluatedCellValue::Error(FormulaError::Num)
                }
            } else if trimmed.eq_ignore_ascii_case("TRUE") {
                EvaluatedCellValue::Boolean(true)
            } else if trimmed.eq_ignore_ascii_case("FALSE") {
                EvaluatedCellValue::Boolean(false)
            } else {
                EvaluatedCellValue::Text(raw.clone())
            }
        }
    };
    visiting.remove(&key);
    memo.insert(key, res.clone());
    res
}

fn eval_expr_uncompiled_baseline(
    sheet: &SheetEngine,
    expr: &Expr,
    memo: &mut HashMap<(String, PackedCellCoordinate), EvaluatedCellValue>,
    visiting: &mut HashSet<(String, PackedCellCoordinate)>,
    depth: usize,
) -> EvaluatedCellValue {
    if depth > SheetEngine::MAX_RECURSION_DEPTH {
        return EvaluatedCellValue::Error(FormulaError::Cycle);
    }
    match expr {
        Expr::Number(n) => EvaluatedCellValue::Number(*n),
        Expr::Text(s) => EvaluatedCellValue::Text(s.clone()),
        Expr::Boolean(b) => EvaluatedCellValue::Boolean(*b),
        Expr::ErrorLiteral(e) => EvaluatedCellValue::Error(*e),
        Expr::Cell(coord) => {
            eval_cell_uncompiled_baseline(sheet, *coord, memo, visiting, depth + 1)
        }
        Expr::UnaryPlus(inner) => {
            eval_expr_uncompiled_baseline(sheet, inner, memo, visiting, depth + 1)
        }
        Expr::UnaryMinus(inner) => {
            match eval_expr_uncompiled_baseline(sheet, inner, memo, visiting, depth + 1).as_number()
            {
                Some(n) => EvaluatedCellValue::Number(-n),
                None => EvaluatedCellValue::Error(FormulaError::Value),
            }
        }
        Expr::PostfixPercent(inner) => {
            match eval_expr_uncompiled_baseline(sheet, inner, memo, visiting, depth + 1).as_number()
            {
                Some(n) => EvaluatedCellValue::Number(n / 100.0),
                None => EvaluatedCellValue::Error(FormulaError::Value),
            }
        }
        Expr::Binary { op, lhs, rhs } => {
            let lv = eval_expr_uncompiled_baseline(sheet, lhs, memo, visiting, depth + 1);
            if let EvaluatedCellValue::Error(e) = lv {
                return EvaluatedCellValue::Error(e);
            }
            let rv = eval_expr_uncompiled_baseline(sheet, rhs, memo, visiting, depth + 1);
            if let EvaluatedCellValue::Error(e) = rv {
                return EvaluatedCellValue::Error(e);
            }
            let ln = lv.as_number().unwrap_or(0.0);
            let rn = rv.as_number().unwrap_or(0.0);
            match op {
                BinaryOp::Add => EvaluatedCellValue::Number(ln + rn),
                BinaryOp::Sub => EvaluatedCellValue::Number(ln - rn),
                BinaryOp::Mul => EvaluatedCellValue::Number(ln * rn),
                BinaryOp::Div => {
                    if rn == 0.0 {
                        EvaluatedCellValue::Error(FormulaError::DivByZero)
                    } else {
                        EvaluatedCellValue::Number(ln / rn)
                    }
                }
                BinaryOp::Pow => EvaluatedCellValue::Number(ln.powf(rn)),
                BinaryOp::Gt => EvaluatedCellValue::Boolean(ln > rn),
                BinaryOp::Ge => EvaluatedCellValue::Boolean(ln >= rn),
                BinaryOp::Lt => EvaluatedCellValue::Boolean(ln < rn),
                BinaryOp::Le => EvaluatedCellValue::Boolean(ln <= rn),
                BinaryOp::Eq => EvaluatedCellValue::Boolean((ln - rn).abs() < 1e-12),
                BinaryOp::Ne => EvaluatedCellValue::Boolean((ln - rn).abs() >= 1e-12),
                BinaryOp::Concat => EvaluatedCellValue::Text(format!(
                    "{}{}",
                    lv.to_display_string(),
                    rv.to_display_string()
                )),
            }
        }
        Expr::FuncCall { name, args } => {
            if name == "IF" && args.len() >= 2 {
                let cond =
                    eval_expr_uncompiled_baseline(sheet, &args[0], memo, visiting, depth + 1)
                        .as_bool()
                        .unwrap_or(false);
                if cond {
                    eval_expr_uncompiled_baseline(sheet, &args[1], memo, visiting, depth + 1)
                } else if args.len() >= 3 {
                    eval_expr_uncompiled_baseline(sheet, &args[2], memo, visiting, depth + 1)
                } else {
                    EvaluatedCellValue::Boolean(false)
                }
            } else if name == "SUM" {
                let mut sum = 0.0;
                for arg in args {
                    if let Expr::Range(r) = arg {
                        if let Ok(coords) = r.expand() {
                            for c in coords {
                                if let Some(n) = eval_cell_uncompiled_baseline(
                                    sheet,
                                    c,
                                    memo,
                                    visiting,
                                    depth + 1,
                                )
                                .as_number()
                                {
                                    sum += n;
                                }
                            }
                        }
                    } else if let Some(n) =
                        eval_expr_uncompiled_baseline(sheet, arg, memo, visiting, depth + 1)
                            .as_number()
                    {
                        sum += n;
                    }
                }
                EvaluatedCellValue::Number(sum)
            } else {
                EvaluatedCellValue::Empty
            }
        }
        _ => EvaluatedCellValue::Error(FormulaError::Value),
    }
}

fn recalculate_all_uncompiled_baseline(
    sheet: &SheetEngine,
) -> HashMap<PackedCellCoordinate, EvaluatedCellValue> {
    let n = sheet.raw_cells().len();
    let mut memo: HashMap<(String, PackedCellCoordinate), EvaluatedCellValue> =
        HashMap::with_capacity(n);
    let mut visiting: HashSet<(String, PackedCellCoordinate)> = HashSet::new();
    for &coord in sheet.raw_cells().keys() {
        eval_cell_uncompiled_baseline(sheet, coord, &mut memo, &mut visiting, 0);
    }
    let key_prefix = sheet.name.to_ascii_lowercase();
    let mut results = HashMap::with_capacity(n);
    for &coord in sheet.raw_cells().keys() {
        if let Some(val) = memo.get(&(key_prefix.clone(), coord)) {
            results.insert(coord, val.clone());
        }
    }
    results
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
            b ^= IeeeCrc32::checksum(black_box(&crc_payload));
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

        if zlib_bytes_a != zlib_bytes_b || dec_a != ooxml_payload || dec_b != ooxml_payload {
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
        let u_a = decode_utf16le_vec_baseline(black_box(&utf16_bytes)).unwrap_or_default();
        let x_a = unescape_xml_five_pass_baseline(black_box(&escaped_xml));
        let dt_a = t0.elapsed().as_micros() as u64;

        let t1 = Instant::now();
        let u_b = decode_utf16le_swar(black_box(&utf16_bytes)).unwrap_or_default();
        let x_b = unescape_xml_single_pass(black_box(&escaped_xml));
        let dt_b = t1.elapsed().as_micros() as u64;

        if u_a != u_b || x_a != x_b || u_b != sample_text {
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
            "Slicing-by-8 IEEE 802.3 CRC-32",
            "1 MiB (64 KiB x 16) ZIP/OOXML container stream",
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
    ];

    let all_equivalent = suites.iter().all(|s| s.equivalence_verified);
    EngineBenchmarkReport {
        iterations: rounds,
        all_equivalent,
        suites,
    }
}

fn expand_range_values_scalar_baseline(
    precompiled_cells: &HashMap<PackedCellCoordinate, EvaluatedCellValue>,
    range: &CellRange,
) -> Vec<EvaluatedCellValue> {
    let coords = range.expand().unwrap();
    coords
        .into_iter()
        .map(|c| {
            precompiled_cells
                .get(&c)
                .cloned()
                .unwrap_or(EvaluatedCellValue::Empty)
        })
        .collect()
}

fn matches_criteria_scalar_baseline(cell: &EvaluatedCellValue, crit: &EvaluatedCellValue) -> bool {
    let crit_str = crit.to_display_string();
    let trimmed = crit_str.trim();
    for op in [">=", "<=", "<>", ">", "<", "="] {
        if let Some(rhs) = trimmed.strip_prefix(op) {
            if let Ok(rhs_num) = rhs.trim().parse::<f64>() {
                let Some(lhs_num) = cell.as_number() else {
                    return op == "<>";
                };
                return match op {
                    ">=" => lhs_num >= rhs_num - 1e-9,
                    "<=" => lhs_num <= rhs_num + 1e-9,
                    "<>" => (lhs_num - rhs_num).abs() >= 1e-9,
                    ">" => lhs_num > rhs_num + 1e-9,
                    "<" => lhs_num < rhs_num - 1e-9,
                    _ => (lhs_num - rhs_num).abs() < 1e-9,
                };
            }
        }
    }
    false
}

fn scalar_vec_range_agg_baseline(
    precompiled_cells: &HashMap<PackedCellCoordinate, EvaluatedCellValue>,
    range: &CellRange,
    left_range: &CellRange,
    right_range: &CellRange,
    col_a_range: &CellRange,
    col_h_range: &CellRange,
) -> [f64; 11] {
    let sum_vals = expand_range_values_scalar_baseline(precompiled_cells, range);
    let sum_nums: Vec<f64> = sum_vals
        .iter()
        .filter_map(|v| match v {
            EvaluatedCellValue::Number(n) => Some(*n),
            _ => None,
        })
        .collect();
    let sum: f64 = sum_nums.iter().sum();

    let avg_vals = expand_range_values_scalar_baseline(precompiled_cells, range);
    let avg_nums: Vec<f64> = avg_vals
        .iter()
        .filter_map(|v| match v {
            EvaluatedCellValue::Number(n) => Some(*n),
            _ => None,
        })
        .collect();
    let avg: f64 = avg_nums.iter().sum::<f64>() / (avg_nums.len().max(1) as f64);

    let min_vals = expand_range_values_scalar_baseline(precompiled_cells, range);
    let min_nums: Vec<f64> = min_vals
        .iter()
        .filter_map(|v| match v {
            EvaluatedCellValue::Number(n) => Some(*n),
            _ => None,
        })
        .collect();
    let min_v: f64 = min_nums.iter().copied().fold(f64::INFINITY, f64::min);

    let max_vals = expand_range_values_scalar_baseline(precompiled_cells, range);
    let max_nums: Vec<f64> = max_vals
        .iter()
        .filter_map(|v| match v {
            EvaluatedCellValue::Number(n) => Some(*n),
            _ => None,
        })
        .collect();
    let max_v: f64 = max_nums.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    let cnt_vals = expand_range_values_scalar_baseline(precompiled_cells, range);
    let cnt_nums: Vec<f64> = cnt_vals
        .iter()
        .filter_map(|v| match v {
            EvaluatedCellValue::Number(n) => Some(*n),
            _ => None,
        })
        .collect();
    let cnt: f64 = cnt_nums.len() as f64;

    let crit_gt_1000 = EvaluatedCellValue::Text(">1000".to_string());
    let sumif_vals = expand_range_values_scalar_baseline(precompiled_cells, range);
    let sumif_target_vals = expand_range_values_scalar_baseline(precompiled_cells, range);
    let sumif_nums: Vec<f64> = sumif_vals
        .iter()
        .zip(sumif_target_vals.iter())
        .filter_map(|(rv, tv)| {
            if matches_criteria_scalar_baseline(rv, &crit_gt_1000) {
                tv.as_number()
            } else {
                None
            }
        })
        .collect();
    let sumif_v: f64 = sumif_nums.iter().sum();

    let crit_ge_500 = EvaluatedCellValue::Text(">=500".to_string());
    let countif_vals = expand_range_values_scalar_baseline(precompiled_cells, range);
    let countif_matches: Vec<bool> = countif_vals
        .iter()
        .filter(|rv| matches_criteria_scalar_baseline(rv, &crit_ge_500))
        .map(|_| true)
        .collect();
    let countif_v: f64 = countif_matches.len() as f64;

    let sp_left = expand_range_values_scalar_baseline(precompiled_cells, left_range);
    let sp_right = expand_range_values_scalar_baseline(precompiled_cells, right_range);
    let sp_prods: Vec<f64> = sp_left
        .iter()
        .zip(sp_right.iter())
        .map(|(a, b)| a.as_number().unwrap_or(0.0) * b.as_number().unwrap_or(0.0))
        .collect();
    let sumprod_v: f64 = sp_prods.iter().sum();

    let stdev_vals = expand_range_values_scalar_baseline(precompiled_cells, range);
    let stdev_nums: Vec<f64> = stdev_vals
        .iter()
        .filter_map(|v| match v {
            EvaluatedCellValue::Number(n) => Some(*n),
            _ => None,
        })
        .collect();
    let stdev_mean = stdev_nums.iter().sum::<f64>() / (stdev_nums.len().max(1) as f64);
    let stdev_var = stdev_nums
        .iter()
        .map(|x| (x - stdev_mean).powi(2))
        .sum::<f64>()
        / ((stdev_nums.len().saturating_sub(1)).max(1) as f64);
    let stdev_v = stdev_var.sqrt();

    let lookup_a = expand_range_values_scalar_baseline(precompiled_cells, col_a_range);
    let lookup_h = expand_range_values_scalar_baseline(precompiled_cells, col_h_range);
    let mut xlookup_v = 0.0;
    for (av, hv) in lookup_a.iter().zip(lookup_h.iter()) {
        if let Some(n) = av.as_number() {
            if (n - 1501.0).abs() < 1e-9 {
                xlookup_v = hv.as_number().unwrap_or(0.0);
                break;
            }
        }
    }

    let match_a = expand_range_values_scalar_baseline(precompiled_cells, col_a_range);
    let mut match_v = 0.0;
    for (idx, av) in match_a.iter().enumerate() {
        if let Some(n) = av.as_number() {
            if (n - 1501.0).abs() < 1e-9 {
                match_v = (idx + 1) as f64;
                break;
            }
        }
    }

    [
        sum, avg, min_v, max_v, cnt, sumif_v, countif_v, sumprod_v, stdev_v, xlookup_v, match_v,
    ]
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
