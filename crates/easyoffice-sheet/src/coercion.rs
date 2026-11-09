use crate::functions_date_fin::ymd_to_excel_serial;
use crate::parser::Expr;
use crate::range_refs::{CompiledCriteria, StreamingRangeSpec};
use crate::types::{EvaluatedCellValue, FormulaError, PackedCellCoordinate};
use crate::{EvalContext, SheetEngine};

#[inline]
pub fn is_valid_decimal_syntax(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut i = 0;
    if bytes[i] == b'+' || bytes[i] == b'-' {
        i += 1;
    }
    if i >= bytes.len() {
        return false;
    }
    let mut has_digits = false;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        has_digits = true;
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            has_digits = true;
            i += 1;
        }
    }
    if !has_digits {
        return false;
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        i += 1;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        let exp_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == exp_start {
            return false;
        }
    }
    i == bytes.len()
}

#[inline]
pub fn parse_strict_number(s: &str) -> Option<f64> {
    let trimmed = s.trim();
    if !is_valid_decimal_syntax(trimmed) {
        return None;
    }
    let n = trimmed.parse::<f64>().ok()?;
    if !n.is_finite() {
        return None;
    }
    Some(if n == 0.0 { 0.0 } else { n })
}

pub fn parse_iso_date_serial(s: &str) -> Option<f64> {
    let trimmed = s.trim();
    if trimmed.len() < 8 || trimmed.len() > 10 {
        return None;
    }
    let sep = if trimmed.contains('-') {
        '-'
    } else if trimmed.contains('/') {
        '/'
    } else {
        return None;
    };
    let mut parts = trimmed.split(sep);
    let y_str = parts.next()?;
    let m_str = parts.next()?;
    let d_str = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if y_str.len() != 4
        || !(1..=2).contains(&m_str.len())
        || !(1..=2).contains(&d_str.len())
        || !y_str.bytes().all(|b| b.is_ascii_digit())
        || !m_str.bytes().all(|b| b.is_ascii_digit())
        || !d_str.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let y: i64 = y_str.parse().ok()?;
    let m: i64 = m_str.parse().ok()?;
    let d: i64 = d_str.parse().ok()?;
    if !(1900..=9999).contains(&y) || !(1..=12).contains(&m) || d < 1 {
        return None;
    }
    let is_leap = (y == 1900) || (y % 4 == 0 && (y % 100 != 0 || y % 400 == 0));
    let max_d = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap {
                29
            } else {
                28
            }
        }
        _ => return None,
    };
    if d > max_d {
        return None;
    }
    let serial = ymd_to_excel_serial(y, m, d).ok()?;
    Some(serial as f64)
}

pub fn parse_value_text(s: &str) -> Option<f64> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(n) = parse_strict_number(trimmed) {
        return Some(n);
    }
    if let Some(serial) = parse_iso_date_serial(trimmed) {
        return Some(serial);
    }
    let mut work = trimmed;
    let mut is_neg_paren = false;
    if work.starts_with('(') && work.ends_with(')') && work.len() >= 3 {
        is_neg_paren = true;
        work = work[1..work.len() - 1].trim();
    }
    let mut is_percent = false;
    if let Some(stripped) = work.strip_suffix('%') {
        is_percent = true;
        work = stripped.trim();
    }
    for prefix in ["$", "₩", "€", "¥", "£"] {
        if let Some(stripped) = work.strip_prefix(prefix) {
            work = stripped.trim();
            break;
        }
    }
    let mut val = if !work.as_bytes().contains(&b',') {
        parse_strict_number(work)?
    } else if work.len() <= 64 {
        let mut buf = [0u8; 64];
        let mut len = 0usize;
        for &b in work.as_bytes() {
            if b != b',' {
                buf[len] = b;
                len += 1;
            }
        }
        let degrouped = std::str::from_utf8(&buf[..len]).ok()?;
        parse_strict_number(degrouped)?
    } else {
        let degrouped = work.replace(',', "");
        parse_strict_number(&degrouped)?
    };
    if is_neg_paren {
        val = -val.abs();
    }
    if is_percent {
        val /= 100.0;
    }
    if !val.is_finite() {
        return None;
    }
    Some(if val == 0.0 { 0.0 } else { val })
}

#[inline]
pub fn coerce_to_number(val: &EvaluatedCellValue) -> Result<f64, FormulaError> {
    match val {
        EvaluatedCellValue::Number(n) => {
            if n.is_finite() {
                Ok(if *n == 0.0 { 0.0 } else { *n })
            } else {
                Err(FormulaError::Num)
            }
        }
        EvaluatedCellValue::Boolean(b) => Ok(if *b { 1.0 } else { 0.0 }),
        EvaluatedCellValue::Empty => Ok(0.0),
        EvaluatedCellValue::Text(s) => parse_value_text(s).ok_or(FormulaError::Value),
        EvaluatedCellValue::Error(e) => Err(*e),
    }
}

#[inline]
pub fn coerce_to_bool(val: &EvaluatedCellValue) -> Result<bool, FormulaError> {
    match val {
        EvaluatedCellValue::Boolean(b) => Ok(*b),
        EvaluatedCellValue::Number(n) => {
            if n.is_finite() {
                Ok(*n != 0.0)
            } else {
                Err(FormulaError::Num)
            }
        }
        EvaluatedCellValue::Empty => Ok(false),
        EvaluatedCellValue::Text(s) => {
            let t = s.trim();
            if t.eq_ignore_ascii_case("TRUE") {
                Ok(true)
            } else if t.eq_ignore_ascii_case("FALSE") {
                Ok(false)
            } else {
                Err(FormulaError::Value)
            }
        }
        EvaluatedCellValue::Error(e) => Err(*e),
    }
}

#[inline]
pub fn sanitize_evaluated_value(val: EvaluatedCellValue) -> EvaluatedCellValue {
    match val {
        EvaluatedCellValue::Number(n) => {
            if !n.is_finite() {
                EvaluatedCellValue::Error(FormulaError::Num)
            } else if n == 0.0 {
                EvaluatedCellValue::Number(0.0)
            } else {
                EvaluatedCellValue::Number(n)
            }
        }
        other => other,
    }
}

pub fn format_general_number(n: f64) -> String {
    if !n.is_finite() {
        return FormulaError::Num.code().to_string();
    }
    if n == 0.0 {
        return "0".to_string();
    }
    let sign = if n.is_sign_negative() { "-" } else { "" };
    let abs_n = n.abs();
    let sci = format!("{abs_n:.14e}");
    let Some((mantissa_str, exp_str)) = sci.split_once('e') else {
        return format!("{n}");
    };
    let Ok(exp) = exp_str.parse::<i32>() else {
        return format!("{n}");
    };

    let mut digits = String::with_capacity(16);
    for ch in mantissa_str.chars() {
        if ch != '.' {
            digits.push(ch);
        }
    }
    while digits.len() > 1 && digits.ends_with('0') {
        digits.pop();
    }

    if !(-9..=14).contains(&exp) {
        let mantissa_fmt = if digits.len() == 1 {
            digits
        } else {
            format!("{}.{}", &digits[..1], &digits[1..])
        };
        let exp_sign = if exp >= 0 { '+' } else { '-' };
        let exp_abs = exp.unsigned_abs();
        return format!("{sign}{mantissa_fmt}E{exp_sign}{exp_abs}");
    }

    let k = digits.len() as i32;
    let dec_pos = exp + 1;
    if dec_pos >= k {
        let mut out = String::with_capacity(sign.len() + dec_pos as usize);
        out.push_str(sign);
        out.push_str(&digits);
        for _ in 0..(dec_pos - k) {
            out.push('0');
        }
        out
    } else if dec_pos > 0 {
        let split = dec_pos as usize;
        format!("{sign}{}.{}", &digits[..split], &digits[split..])
    } else {
        let leading_zeros = (-dec_pos) as usize;
        let mut out = String::with_capacity(sign.len() + 2 + leading_zeros + digits.len());
        out.push_str(sign);
        out.push_str("0.");
        for _ in 0..leading_zeros {
            out.push('0');
        }
        out.push_str(&digits);
        out
    }
}

pub fn format_fixed_number(num: f64, decimals: i32, use_commas: bool) -> String {
    if !num.is_finite() {
        return FormulaError::Num.code().to_string();
    }
    let rounded_val = if decimals < 0 {
        let factor = 10f64.powi(-decimals);
        if factor.is_finite() && factor > 0.0 {
            let q = num / factor;
            let nudge = q.abs() * f64::EPSILON * 2.0;
            (q + nudge.copysign(q)).round() * factor
        } else {
            0.0
        }
    } else {
        let factor = 10f64.powi(decimals.min(20));
        if factor.is_finite() && (num * factor).is_finite() {
            let q = num * factor;
            let nudge = q.abs() * f64::EPSILON * 2.0;
            (q + nudge.copysign(q)).round() / factor
        } else {
            num
        }
    };
    let prec = decimals.clamp(0, 20) as usize;
    let abs_v = if rounded_val == 0.0 {
        0.0
    } else {
        rounded_val.abs()
    };
    let sign = if rounded_val < 0.0 && abs_v != 0.0 {
        "-"
    } else {
        ""
    };
    let raw = format!("{abs_v:.prec$}");
    if !use_commas {
        return format!("{sign}{raw}");
    }
    let (int_part, frac_opt) = match raw.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (raw.as_str(), None),
    };
    let mut grouped = String::with_capacity(int_part.len() + int_part.len() / 3);
    for (idx, ch) in int_part.chars().enumerate() {
        if idx > 0 && (int_part.len() - idx) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    match frac_opt {
        Some(f) => format!("{sign}{grouped}.{f}"),
        None => format!("{sign}{grouped}"),
    }
}

pub use crate::text_format::{
    format_number_with_code, format_text_pattern, format_text_value_with_code,
};

/// Inputs of a multi-criteria aggregate (`COUNTIFS`, `SUMIFS`, ...).
struct MultiCritJob<'a> {
    specs: Vec<StreamingRangeSpec<'a>>,
    crits: Vec<CompiledCriteria>,
    target_spec: Option<StreamingRangeSpec<'a>>,
    r_count: usize,
    c_count: usize,
}

/// Running totals of matching cells.
#[derive(Default)]
struct CritTotals {
    sum: f64,
    count: usize,
    min_val: Option<f64>,
    max_val: Option<f64>,
}

impl SheetEngine {
    pub(crate) fn eval_multi_criteria_streaming(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        let (target_spec, crit_args) = if name == "COUNTIFS" {
            if args.len() < 2 || (args.len() & 1) != 0 {
                return Err(FormulaError::Value);
            }
            (None, args)
        } else {
            if args.len() < 3 || (args.len() & 1) == 0 {
                return Err(FormulaError::Value);
            }
            let Some(ts) = self.extract_streaming_range_ctx(&args[0], ctx, depth)? else {
                return Err(FormulaError::Value);
            };
            (Some(ts), &args[1..])
        };

        let pair_count = crit_args.len() / 2;
        let mut specs = Vec::with_capacity(pair_count);
        let mut crit_vals = Vec::with_capacity(pair_count);
        let mut expected_dims: Option<(usize, usize)> = target_spec.as_ref().map(|ts| (ts.3, ts.4));

        for pair in crit_args.chunks(2) {
            let Some(sp) = self.extract_streaming_range_ctx(&pair[0], ctx, depth)? else {
                return Err(FormulaError::Value);
            };
            let dims = (sp.3, sp.4);
            if let Some(exp) = expected_dims {
                if dims != exp {
                    return Err(FormulaError::Value);
                }
            } else {
                expected_dims = Some(dims);
            }
            specs.push(sp);
            crit_vals.push(self.eval_arg_val(&pair[1], ctx, depth)?);
        }

        let (r_count, c_count) = expected_dims.ok_or(FormulaError::Value)?;
        let job = MultiCritJob {
            specs,
            crits: crit_vals.iter().map(CompiledCriteria::compile).collect(),
            target_spec,
            r_count,
            c_count,
        };
        let totals = self.run_multi_criteria(&job, ctx, depth)?;
        let (sum, count) = (totals.sum, totals.count);
        match name {
            "COUNTIFS" => Ok(EvaluatedCellValue::Number(count as f64)),
            "AVERAGEIFS" => {
                if count == 0 {
                    Err(FormulaError::DivByZero)
                } else {
                    Ok(EvaluatedCellValue::Number(sum / (count as f64)))
                }
            }
            "MINIFS" => Ok(EvaluatedCellValue::Number(totals.min_val.unwrap_or(0.0))),
            "MAXIFS" => Ok(EvaluatedCellValue::Number(totals.max_val.unwrap_or(0.0))),
            _ => Ok(EvaluatedCellValue::Number(sum)),
        }
    }

    /// Whether the cell at `(dr, dc)` satisfies every criteria pair.
    fn cell_passes_all(
        &self,
        job: &MultiCritJob<'_>,
        dr: usize,
        dc: usize,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<bool, FormulaError> {
        for (sp, cc) in job.specs.iter().zip(job.crits.iter()) {
            let val = self.eval_streaming_cell_allow_error(sp, dr, dc, ctx, depth)?;
            if !cc.matches(&val) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Adds the cell at `(dr, dc)` (which passed all criteria) to the totals.
    fn accumulate_match(
        &self,
        job: &MultiCritJob<'_>,
        dr: usize,
        dc: usize,
        totals: &mut CritTotals,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<(), FormulaError> {
        let Some(ref ts) = job.target_spec else {
            totals.count += 1;
            return Ok(());
        };
        if let EvaluatedCellValue::Number(n) = self.eval_streaming_cell(ts, dr, dc, ctx, depth)? {
            totals.sum += n;
            totals.count += 1;
            totals.min_val = Some(totals.min_val.map_or(n, |v| v.min(n)));
            totals.max_val = Some(totals.max_val.map_or(n, |v| v.max(n)));
        }
        Ok(())
    }

    fn run_multi_criteria(
        &self,
        job: &MultiCritJob<'_>,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<CritTotals, FormulaError> {
        let total = job.r_count.saturating_mul(job.c_count);
        let mut totals = CritTotals::default();
        let sparse_idx = job
            .specs
            .iter()
            .zip(job.crits.iter())
            .position(|(sp, cc)| !sp.5 && !cc.matches(&EvaluatedCellValue::Empty));
        let target_dense = job.target_spec.as_ref().is_some_and(|ts| !ts.5);
        if total > 1024 && (sparse_idx.is_some() || target_dense) {
            let anchor = match sparse_idx {
                Some(idx) => &job.specs[idx],
                None => job.target_spec.as_ref().unwrap(),
            };
            self.multi_criteria_sparse(job, anchor, &mut totals, ctx, depth)?;
        } else if total > 1024 && job.target_spec.is_none() && job.specs.iter().all(|sp| !sp.5) {
            totals.count = self.multi_criteria_count_by_population(job, total, ctx, depth)?;
        } else {
            for dr in 0..job.r_count {
                for dc in 0..job.c_count {
                    if self.cell_passes_all(job, dr, dc, ctx, depth)? {
                        self.accumulate_match(job, dr, dc, &mut totals, ctx, depth)?;
                    }
                }
            }
        }
        Ok(totals)
    }

    /// Walks only the populated cells of the anchor range.
    fn multi_criteria_sparse(
        &self,
        job: &MultiCritJob<'_>,
        anchor: &StreamingRangeSpec<'_>,
        totals: &mut CritTotals,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<(), FormulaError> {
        let Some(anchor_sheet) = self.resolve_sheet_ref(anchor.0.as_deref(), ctx) else {
            return Ok(());
        };
        let max_r = anchor.1 + (job.r_count as u32).saturating_sub(1);
        let max_c = anchor.2 + (job.c_count as u32).saturating_sub(1);
        let mut pop_coords: Vec<PackedCellCoordinate> = anchor_sheet
            .cells
            .keys()
            .copied()
            .filter(|c| {
                c.row() >= anchor.1 && c.row() <= max_r && c.col() >= anchor.2 && c.col() <= max_c
            })
            .collect();
        pop_coords.sort_unstable();
        for coord in pop_coords {
            let dr = (coord.row() - anchor.1) as usize;
            let dc = (coord.col() - anchor.2) as usize;
            if self.cell_passes_all(job, dr, dc, ctx, depth)? {
                self.accumulate_match(job, dr, dc, totals, ctx, depth)?;
            }
        }
        Ok(())
    }

    /// COUNTIFS over mostly-empty ranges: count = total minus populated non-matching cells.
    fn multi_criteria_count_by_population(
        &self,
        job: &MultiCritJob<'_>,
        total: usize,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<usize, FormulaError> {
        let mut pop_offsets: Vec<(usize, usize)> = Vec::new();
        for sp in &job.specs {
            let Some(sh) = self.resolve_sheet_ref(sp.0.as_deref(), ctx) else {
                continue;
            };
            let max_r = sp.1 + (job.r_count as u32).saturating_sub(1);
            let max_c = sp.2 + (job.c_count as u32).saturating_sub(1);
            for &c in sh.cells.keys() {
                if c.row() >= sp.1 && c.row() <= max_r && c.col() >= sp.2 && c.col() <= max_c {
                    pop_offsets.push(((c.row() - sp.1) as usize, (c.col() - sp.2) as usize));
                }
            }
        }
        pop_offsets.sort_unstable();
        pop_offsets.dedup();
        let mut non_matching_pop = 0usize;
        for (dr, dc) in pop_offsets {
            if !self.cell_passes_all(job, dr, dc, ctx, depth)? {
                non_matching_pop += 1;
            }
        }
        Ok(total.saturating_sub(non_matching_pop))
    }

    pub(crate) fn eval_subtotal(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if args.len() < 2 {
            return Err(FormulaError::Value);
        }
        let fn_code = self.eval_arg_num(&args[0], ctx, depth)?.trunc() as i32;
        let base_code = if (101..=111).contains(&fn_code) {
            fn_code - 100
        } else {
            fn_code
        };
        if !(1..=11).contains(&base_code) {
            return Err(FormulaError::Value);
        }

        let mut acc = SubtotalAcc::new();
        for arg in &args[1..] {
            if let Some(spec) = self.extract_streaming_range_ctx(arg, ctx, depth)? {
                self.subtotal_range(&spec, &mut acc, ctx, depth)?;
            } else if !crate::expr_contains_subtotal(arg) {
                acc.fold(self.eval_arg_val(arg, ctx, depth)?)?;
            }
        }
        acc.result(base_code)
    }

    /// Folds every non-nested-subtotal cell of a streamed range into `acc`.
    fn subtotal_range(
        &self,
        spec: &StreamingRangeSpec<'_>,
        acc: &mut SubtotalAcc,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<(), FormulaError> {
        let (ref sheet_opt, min_r, min_c, rows, cols, transposed) = *spec;
        for dr in 0..rows {
            for dc in 0..cols {
                let (r, c) = if transposed {
                    (min_r + dc as u32, min_c + dr as u32)
                } else {
                    (min_r + dr as u32, min_c + dc as u32)
                };
                if ctx.steps > Self::MAX_EVAL_STEPS {
                    return Err(FormulaError::Value);
                }
                let coord = PackedCellCoordinate::new(r, c).map_err(|_| FormulaError::Ref)?;
                if self.is_nested_subtotal_cell(sheet_opt.as_deref(), coord, ctx) {
                    continue;
                }
                acc.fold(self.eval_sheet_coord(sheet_opt.as_deref(), coord, ctx, depth))?;
            }
        }
        Ok(())
    }
}

/// Running statistics for `SUBTOTAL` (Welford mean / variance for the deviation codes).
struct SubtotalAcc {
    sum: f64,
    product: f64,
    num_count: usize,
    non_empty_count: usize,
    min_val: Option<f64>,
    max_val: Option<f64>,
    mean: f64,
    m2: f64,
}

impl SubtotalAcc {
    fn new() -> Self {
        Self {
            sum: 0.0,
            product: 1.0,
            num_count: 0,
            non_empty_count: 0,
            min_val: None,
            max_val: None,
            mean: 0.0,
            m2: 0.0,
        }
    }

    fn fold(&mut self, v: EvaluatedCellValue) -> Result<(), FormulaError> {
        match v {
            EvaluatedCellValue::Error(e) => return Err(e),
            EvaluatedCellValue::Empty => {}
            EvaluatedCellValue::Number(n) => {
                self.non_empty_count += 1;
                self.sum += n;
                self.product *= n;
                self.num_count += 1;
                self.min_val = Some(self.min_val.map_or(n, |m| m.min(n)));
                self.max_val = Some(self.max_val.map_or(n, |m| m.max(n)));
                let delta = n - self.mean;
                self.mean += delta / (self.num_count as f64);
                self.m2 += delta * (n - self.mean);
            }
            EvaluatedCellValue::Boolean(_) | EvaluatedCellValue::Text(_) => {
                self.non_empty_count += 1;
            }
        }
        Ok(())
    }

    /// Variance with divisor `count - 1` (sample) or `count` (population).
    fn variance(&self, sample: bool) -> Result<f64, FormulaError> {
        let min_count = if sample { 2 } else { 1 };
        if self.num_count < min_count {
            return Err(FormulaError::DivByZero);
        }
        let denom = if sample {
            self.num_count - 1
        } else {
            self.num_count
        };
        Ok(self.m2 / (denom as f64))
    }

    fn result(&self, base_code: i32) -> Result<EvaluatedCellValue, FormulaError> {
        let value = match base_code {
            1 if self.num_count == 0 => return Err(FormulaError::DivByZero),
            1 => self.sum / (self.num_count as f64),
            2 => self.num_count as f64,
            3 => self.non_empty_count as f64,
            4 => self.max_val.unwrap_or(0.0),
            5 => self.min_val.unwrap_or(0.0),
            6 if self.num_count == 0 => 0.0,
            6 => self.product,
            7 => self.variance(true)?.sqrt(),
            8 => self.variance(false)?.sqrt(),
            9 => self.sum,
            10 => self.variance(true)?,
            11 => self.variance(false)?,
            _ => return Err(FormulaError::Value),
        };
        Ok(EvaluatedCellValue::Number(value))
    }
}

#[inline]
fn next_pattern_token(pattern: &str, p_idx: usize) -> Option<(char, bool, usize)> {
    let mut chars = pattern[p_idx..].chars();
    let ch = chars.next()?;
    let after_first = p_idx + ch.len_utf8();
    if ch == '~' {
        if let Some(escaped) = chars.next() {
            if matches!(escaped, '*' | '?' | '~') {
                return Some((escaped, true, after_first + escaped.len_utf8()));
            }
        }
    }
    Some((ch, false, after_first))
}

#[inline]
fn chars_eq_case_insensitive(a: char, b: char) -> bool {
    if a == b {
        return true;
    }
    if a.is_ascii() && b.is_ascii() {
        return a.eq_ignore_ascii_case(&b);
    }
    a.to_lowercase().eq(b.to_lowercase())
}

pub fn wildcard_match(text: &str, pattern: &str) -> bool {
    let mut t_idx = 0usize;
    let mut p_idx = 0usize;
    let mut star_p: Option<usize> = None;
    let mut match_t = 0usize;

    while t_idx < text.len() {
        if let Some((p_ch, is_escaped, next_p)) = next_pattern_token(pattern, p_idx) {
            if !is_escaped && p_ch == '*' {
                star_p = Some(next_p);
                match_t = t_idx;
                p_idx = next_p;
                continue;
            }
            let t_ch = text[t_idx..].chars().next().unwrap();
            if (!is_escaped && p_ch == '?') || chars_eq_case_insensitive(p_ch, t_ch) {
                t_idx += t_ch.len_utf8();
                p_idx = next_p;
                continue;
            }
        }
        if let Some(sp) = star_p {
            p_idx = sp;
            let skip_ch = text[match_t..].chars().next().unwrap();
            match_t += skip_ch.len_utf8();
            t_idx = match_t;
        } else {
            return false;
        }
    }

    while let Some((p_ch, is_escaped, next_p)) = next_pattern_token(pattern, p_idx) {
        if !is_escaped && p_ch == '*' {
            p_idx = next_p;
        } else {
            return false;
        }
    }
    true
}

pub fn wildcard_prefix_match(text: &str, pattern: &str) -> bool {
    let trailing_tildes = pattern.chars().rev().take_while(|&c| c == '~').count();
    let pat = if trailing_tildes % 2 == 1 {
        format!("{pattern}~*")
    } else {
        format!("{pattern}*")
    };
    wildcard_match(text, &pat)
}

pub fn wildcard_match_alloc_baseline(text: &str, pattern: &str) -> bool {
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let pat_lower = pattern.to_lowercase();
    let mut p: Vec<(char, bool)> = Vec::with_capacity(pat_lower.len());
    let mut chars = pat_lower.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '~' {
            if let Some(&next) = chars.peek() {
                if matches!(next, '*' | '?' | '~') {
                    p.push((chars.next().unwrap(), true));
                    continue;
                }
            }
        }
        p.push((ch, false));
    }
    let mut dp = vec![vec![false; p.len() + 1]; t.len() + 1];
    dp[0][0] = true;
    for j in 1..=p.len() {
        if !p[j - 1].1 && p[j - 1].0 == '*' {
            dp[0][j] = dp[0][j - 1];
        }
    }
    for i in 1..=t.len() {
        for j in 1..=p.len() {
            let (pch, escaped) = p[j - 1];
            if !escaped && pch == '*' {
                dp[i][j] = dp[i - 1][j] || dp[i][j - 1];
            } else if (!escaped && pch == '?') || pch == t[i - 1] {
                dp[i][j] = dp[i - 1][j - 1];
            }
        }
    }
    dp[t.len()][p.len()]
}
