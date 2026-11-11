//! Scalar and uncompiled baselines the benchmark suites time the production code against.
//! Each one is a deliberately plain version of an optimised path, kept so the A/B rounds in
//! `benchmark.rs` have something to compare and cross-check.

use easyoffice_formats::{inflate_raw, EasyOfficeError};
use easyoffice_sheet::{
    compile_formula_expr, BinaryOp, CellRange, EvaluatedCellValue, Expr, FormulaError,
    PackedCellCoordinate, SheetEngine,
};
use std::collections::{HashMap, HashSet};

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

pub(crate) fn write_bits_scalar(
    out: &mut Vec<u8>,
    bit_buf: &mut u32,
    bit_count: &mut u32,
    val: u32,
    n: u32,
) {
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

pub(crate) fn write_huffman_scalar(
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

pub(crate) fn write_fixed_sym_scalar(
    out: &mut Vec<u8>,
    bit_buf: &mut u32,
    bit_count: &mut u32,
    sym: u16,
) {
    match sym {
        0..=143 => write_huffman_scalar(out, bit_buf, bit_count, 0x30 + sym, 8),
        144..=255 => write_huffman_scalar(out, bit_buf, bit_count, 0x190 + (sym - 144), 9),
        256..=279 => write_huffman_scalar(out, bit_buf, bit_count, sym - 256, 7),
        280..=287 => write_huffman_scalar(out, bit_buf, bit_count, 0xC0 + (sym - 280), 8),
        _ => {}
    }
}

pub(crate) fn deflate_zlib_scalar_baseline(input: &[u8]) -> Vec<u8> {
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
                if (1..=32_768).contains(&dist) {
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

pub(crate) fn inflate_zlib_scalar_baseline(
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

pub(crate) fn eval_cell_uncompiled_baseline(
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

pub(crate) fn eval_expr_uncompiled_baseline(
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

pub(crate) fn recalculate_all_uncompiled_baseline(
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

pub(crate) fn expand_range_values_scalar_baseline(
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

pub(crate) fn matches_criteria_scalar_baseline(
    cell: &EvaluatedCellValue,
    crit: &EvaluatedCellValue,
) -> bool {
    let crit_str = crit.to_display_string();
    let trimmed = crit_str.trim();
    for op in [">=", "<=", "<>", ">", "<", "="] {
        if let Some(rhs) = trimmed.strip_prefix(op) {
            if let Ok(rhs_num) = rhs.trim().parse::<f64>() {
                let Some(lhs_num) = cell.as_number() else {
                    return op == "<>";
                };
                return match op {
                    ">=" => lhs_num >= rhs_num - 1e-12,
                    "<=" => lhs_num <= rhs_num + 1e-12,
                    "<>" => (lhs_num - rhs_num).abs() >= 1e-12,
                    ">" => lhs_num > rhs_num,
                    "<" => lhs_num < rhs_num,
                    _ => (lhs_num - rhs_num).abs() < 1e-12,
                };
            }
        }
    }
    false
}

pub(crate) fn scalar_vec_range_agg_baseline(
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
