use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

impl SheetEngine {
    pub(crate) fn eval_multi_criteria(
        &self,
        criteria_pairs: &[Expr],
        expected_len: usize,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Vec<bool>, FormulaError> {
        let mut mask = vec![true; expected_len];
        for pair in criteria_pairs.chunks(2) {
            let r_vals = self.eval_range_vec(&pair[0], ctx, depth)?;
            if r_vals.len() != expected_len {
                return Err(FormulaError::Value);
            }
            let crit = self.eval_arg_val(&pair[1], ctx, depth)?;
            for (m, rv) in mask.iter_mut().zip(r_vals.iter()) {
                if *m && !matches_criteria(rv, &crit) {
                    *m = false;
                }
            }
        }
        Ok(mask)
    }

    pub(crate) fn eval_date_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        match name {
            "DATE" => {
                if args.len() != 3 {
                    return Err(FormulaError::Value);
                }
                let mut y = self.eval_arg_num(&args[0], ctx, depth)?.trunc() as i64;
                if (0..1900).contains(&y) {
                    y += 1900;
                }
                let m = self.eval_arg_num(&args[1], ctx, depth)?.trunc() as i64;
                let d = self.eval_arg_num(&args[2], ctx, depth)?.trunc() as i64;
                let adj_y = y + (m - 1).div_euclid(12);
                let adj_m = (m - 1).rem_euclid(12) + 1;
                let serial = serial_from_civil(adj_y, adj_m, 1) + (d - 1);
                Ok(EvaluatedCellValue::Number(serial as f64))
            }
            "YEAR" | "MONTH" | "DAY" => {
                if args.len() != 1 {
                    return Err(FormulaError::Value);
                }
                let serial = self.eval_arg_num(&args[0], ctx, depth)?.floor() as i64;
                let (y, m, d) = civil_from_serial(serial);
                let v = match name {
                    "YEAR" => y,
                    "MONTH" => m,
                    _ => d,
                };
                Ok(EvaluatedCellValue::Number(v as f64))
            }
            "TIME" => {
                if args.len() != 3 {
                    return Err(FormulaError::Value);
                }
                let h = self.eval_arg_num(&args[0], ctx, depth)?.trunc() as i64;
                let m = self.eval_arg_num(&args[1], ctx, depth)?.trunc() as i64;
                let s = self.eval_arg_num(&args[2], ctx, depth)?.trunc() as i64;
                let total = h * 3600 + m * 60 + s;
                if total < 0 {
                    return Err(FormulaError::Num);
                }
                Ok(EvaluatedCellValue::Number(
                    ((total % 86_400) as f64) / 86_400.0,
                ))
            }
            "HOUR" | "MINUTE" | "SECOND" => {
                if args.len() != 1 {
                    return Err(FormulaError::Value);
                }
                let n = self.eval_arg_num(&args[0], ctx, depth)?;
                if n < 0.0 {
                    return Err(FormulaError::Num);
                }
                let secs = (n.fract() * 86_400.0).round() as i64 % 86_400;
                let v = match name {
                    "HOUR" => secs / 3600,
                    "MINUTE" => (secs % 3600) / 60,
                    _ => secs % 60,
                };
                Ok(EvaluatedCellValue::Number(v as f64))
            }
            "DAYS" => {
                if args.len() != 2 {
                    return Err(FormulaError::Value);
                }
                let end_d = self.eval_arg_num(&args[0], ctx, depth)?.trunc();
                let start_d = self.eval_arg_num(&args[1], ctx, depth)?.trunc();
                Ok(EvaluatedCellValue::Number(end_d - start_d))
            }
            "EDATE" => {
                if args.len() != 2 {
                    return Err(FormulaError::Value);
                }
                let start_s = self.eval_arg_num(&args[0], ctx, depth)?.floor() as i64;
                let months = self.eval_arg_num(&args[1], ctx, depth)?.trunc() as i64;
                let (y, m, d) = civil_from_serial(start_s);
                let total_m = (y * 12 + (m - 1)) + months;
                let ny = total_m.div_euclid(12);
                let nm = total_m.rem_euclid(12) + 1;
                let nd = d.min(days_in_month(ny, nm));
                Ok(EvaluatedCellValue::Number(
                    serial_from_civil(ny, nm, nd) as f64
                ))
            }
            _ => Err(FormulaError::Name),
        }
    }

    pub(crate) fn eval_financial_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        match name {
            "PMT" => {
                if args.len() < 3 || args.len() > 5 {
                    return Err(FormulaError::Value);
                }
                let rate = self.eval_arg_num(&args[0], ctx, depth)?;
                let nper = self.eval_arg_num(&args[1], ctx, depth)?;
                let pv = self.eval_arg_num(&args[2], ctx, depth)?;
                let fv = if args.len() >= 4 {
                    self.eval_arg_num(&args[3], ctx, depth)?
                } else {
                    0.0
                };
                let t = if args.len() == 5 {
                    self.eval_arg_num(&args[4], ctx, depth)?
                } else {
                    0.0
                };
                Ok(EvaluatedCellValue::Number(calc_pmt(rate, nper, pv, fv, t)?))
            }
            "IPMT" | "PPMT" => {
                if args.len() < 4 || args.len() > 6 {
                    return Err(FormulaError::Value);
                }
                let rate = self.eval_arg_num(&args[0], ctx, depth)?;
                let per = self.eval_arg_num(&args[1], ctx, depth)?;
                let nper = self.eval_arg_num(&args[2], ctx, depth)?;
                let pv = self.eval_arg_num(&args[3], ctx, depth)?;
                let fv = if args.len() >= 5 {
                    self.eval_arg_num(&args[4], ctx, depth)?
                } else {
                    0.0
                };
                let t = if args.len() == 6 {
                    self.eval_arg_num(&args[5], ctx, depth)?
                } else {
                    0.0
                };
                if per < 1.0 || per > nper {
                    return Err(FormulaError::Num);
                }
                let pmt = calc_pmt(rate, nper, pv, fv, t)?;
                let ipmt = if (per - 1.0).abs() < 1e-9 && t != 0.0 {
                    0.0
                } else {
                    let p = if t != 0.0 { per - 2.0 } else { per - 1.0 };
                    calc_fv(rate, p, pmt, pv, t)? * rate / (if t != 0.0 { 1.0 + rate } else { 1.0 })
                };
                Ok(EvaluatedCellValue::Number(if name == "IPMT" {
                    ipmt
                } else {
                    pmt - ipmt
                }))
            }
            "FV" => {
                if args.len() < 3 || args.len() > 5 {
                    return Err(FormulaError::Value);
                }
                let rate = self.eval_arg_num(&args[0], ctx, depth)?;
                let nper = self.eval_arg_num(&args[1], ctx, depth)?;
                let pmt = self.eval_arg_num(&args[2], ctx, depth)?;
                let pv = if args.len() >= 4 {
                    self.eval_arg_num(&args[3], ctx, depth)?
                } else {
                    0.0
                };
                let t = if args.len() == 5 {
                    self.eval_arg_num(&args[4], ctx, depth)?
                } else {
                    0.0
                };
                Ok(EvaluatedCellValue::Number(calc_fv(rate, nper, pmt, pv, t)?))
            }
            "PV" => {
                if args.len() < 3 || args.len() > 5 {
                    return Err(FormulaError::Value);
                }
                let rate = self.eval_arg_num(&args[0], ctx, depth)?;
                let nper = self.eval_arg_num(&args[1], ctx, depth)?;
                let pmt = self.eval_arg_num(&args[2], ctx, depth)?;
                let fv = if args.len() >= 4 {
                    self.eval_arg_num(&args[3], ctx, depth)?
                } else {
                    0.0
                };
                let t = if args.len() == 5 {
                    self.eval_arg_num(&args[4], ctx, depth)?
                } else {
                    0.0
                };
                if rate.abs() < 1e-12 {
                    return Ok(EvaluatedCellValue::Number(-fv - pmt * nper));
                }
                let factor = (1.0 + rate).powf(nper);
                let type_adj = if t != 0.0 { 1.0 + rate } else { 1.0 };
                let pv = (-fv - pmt * type_adj * (factor - 1.0) / rate) / factor;
                Ok(EvaluatedCellValue::Number(pv))
            }
            "NPV" => {
                if args.len() < 2 {
                    return Err(FormulaError::Value);
                }
                let rate = self.eval_arg_num(&args[0], ctx, depth)?;
                if rate <= -1.0 {
                    return Err(FormulaError::DivByZero);
                }
                let cfs = self.expand_args_ctx(&args[1..], ctx, depth)?;
                let mut npv = 0.0;
                for (t, v) in cfs
                    .iter()
                    .filter_map(EvaluatedCellValue::as_number)
                    .enumerate()
                {
                    npv += v / (1.0 + rate).powi((t + 1) as i32);
                }
                Ok(EvaluatedCellValue::Number(npv))
            }
            "IRR" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(FormulaError::Value);
                }
                let vals = self.eval_range_vec(&args[0], ctx, depth)?;
                let cfs: Vec<f64> = vals
                    .iter()
                    .filter_map(EvaluatedCellValue::as_number)
                    .collect();
                let max_cf = cfs.iter().map(|v| v.abs()).fold(0.0_f64, f64::max);
                if cfs.len() < 2
                    || max_cf == 0.0
                    || !cfs.iter().any(|&v| v > 0.0)
                    || !cfs.iter().any(|&v| v < 0.0)
                {
                    return Err(FormulaError::Num);
                }
                let scaled_cfs: Vec<f64> = cfs.iter().map(|&v| v / max_cf).collect();
                let eval_npv = |rate: f64| -> f64 {
                    scaled_cfs
                        .iter()
                        .enumerate()
                        .map(|(t, &cf)| cf / (1.0 + rate).powi(t as i32))
                        .sum()
                };
                let mut r = if args.len() == 2 {
                    self.eval_arg_num(&args[1], ctx, depth)?
                } else {
                    0.1
                };
                for _ in 0..100 {
                    if r <= -0.999_999 || !r.is_finite() {
                        break;
                    }
                    let mut f = 0.0;
                    let mut df = 0.0;
                    for (t, &cf) in scaled_cfs.iter().enumerate() {
                        let denom = (1.0 + r).powi(t as i32);
                        f += cf / denom;
                        if t > 0 {
                            df -= (t as f64) * cf / (1.0 + r).powi((t + 1) as i32);
                        }
                    }
                    if f.abs() < 1e-10 {
                        return Ok(EvaluatedCellValue::Number(r));
                    }
                    if df.abs() < 1e-15 {
                        break;
                    }
                    r -= f / df;
                }
                let mut lo = -0.99_f64;
                let mut hi = 10.0_f64;
                let mut f_lo = eval_npv(lo);
                let f_hi = eval_npv(hi);
                if f_lo.is_finite() && f_hi.is_finite() && f_lo * f_hi <= 0.0 {
                    for _ in 0..100 {
                        let mid = 0.5 * (lo + hi);
                        let f_mid = eval_npv(mid);
                        if f_mid.abs() < 1e-11 || (hi - lo).abs() < 1e-12 {
                            return Ok(EvaluatedCellValue::Number(mid));
                        }
                        if f_lo * f_mid <= 0.0 {
                            hi = mid;
                        } else {
                            lo = mid;
                            f_lo = f_mid;
                        }
                    }
                    return Ok(EvaluatedCellValue::Number(0.5 * (lo + hi)));
                }
                Err(FormulaError::Num)
            }
            "RATE" => {
                if args.len() < 3 || args.len() > 6 {
                    return Err(FormulaError::Value);
                }
                let nper = self.eval_arg_num(&args[0], ctx, depth)?;
                let pmt = self.eval_arg_num(&args[1], ctx, depth)?;
                let pv = self.eval_arg_num(&args[2], ctx, depth)?;
                let fv = if args.len() >= 4 {
                    self.eval_arg_num(&args[3], ctx, depth)?
                } else {
                    0.0
                };
                let t = if args.len() >= 5 {
                    self.eval_arg_num(&args[4], ctx, depth)?
                } else {
                    0.0
                };
                let mut r = if args.len() == 6 {
                    self.eval_arg_num(&args[5], ctx, depth)?
                } else {
                    0.1
                };
                for _ in 0..100 {
                    let f = calc_fv(r, nper, pmt, pv, t)? - fv;
                    if f.abs() < 1e-7 {
                        return Ok(EvaluatedCellValue::Number(r));
                    }
                    let eps = 1e-6;
                    let df = (calc_fv(r + eps, nper, pmt, pv, t)? - fv - f) / eps;
                    if df.abs() < 1e-15 {
                        break;
                    }
                    r -= f / df;
                }
                Err(FormulaError::Num)
            }
            "NPER" => {
                if args.len() < 3 || args.len() > 5 {
                    return Err(FormulaError::Value);
                }
                let rate = self.eval_arg_num(&args[0], ctx, depth)?;
                let pmt = self.eval_arg_num(&args[1], ctx, depth)?;
                let pv = self.eval_arg_num(&args[2], ctx, depth)?;
                let fv = if args.len() >= 4 {
                    self.eval_arg_num(&args[3], ctx, depth)?
                } else {
                    0.0
                };
                let t = if args.len() == 5 {
                    self.eval_arg_num(&args[4], ctx, depth)?
                } else {
                    0.0
                };
                if rate.abs() < 1e-12 {
                    if pmt.abs() < 1e-12 {
                        return Err(FormulaError::DivByZero);
                    }
                    return Ok(EvaluatedCellValue::Number((-pv - fv) / pmt));
                }
                let pmt_adj = pmt * (if t != 0.0 { 1.0 + rate } else { 1.0 });
                let num = pmt_adj - fv * rate;
                let den = pv * rate + pmt_adj;
                if den == 0.0 || num / den <= 0.0 || 1.0 + rate <= 0.0 {
                    return Err(FormulaError::Num);
                }
                Ok(EvaluatedCellValue::Number(
                    (num / den).ln() / (1.0 + rate).ln(),
                ))
            }
            "SLN" => {
                if args.len() != 3 {
                    return Err(FormulaError::Value);
                }
                let cost = self.eval_arg_num(&args[0], ctx, depth)?;
                let salvage = self.eval_arg_num(&args[1], ctx, depth)?;
                let life = self.eval_arg_num(&args[2], ctx, depth)?;
                if life.abs() < 1e-15 {
                    return Err(FormulaError::DivByZero);
                }
                Ok(EvaluatedCellValue::Number((cost - salvage) / life))
            }
            "DB" => {
                if args.len() < 4 || args.len() > 5 {
                    return Err(FormulaError::Value);
                }
                let cost = self.eval_arg_num(&args[0], ctx, depth)?;
                let salvage = self.eval_arg_num(&args[1], ctx, depth)?;
                let life = self.eval_arg_num(&args[2], ctx, depth)?;
                let period = self.eval_arg_num(&args[3], ctx, depth)?.trunc() as usize;
                let month = if args.len() == 5 {
                    self.eval_arg_num(&args[4], ctx, depth)?.trunc() as usize
                } else {
                    12
                };
                if cost <= 0.0
                    || salvage < 0.0
                    || life <= 0.0
                    || !(1..=10_000).contains(&period)
                    || !(1..=12).contains(&month)
                {
                    return Err(FormulaError::Num);
                }
                let rate = ((1.0 - (salvage / cost).powf(1.0 / life)) * 1000.0).round() / 1000.0;
                let mut total_dep = 0.0;
                let mut dep = 0.0;
                for p in 1..=period {
                    dep = if p == 1 {
                        cost * rate * (month as f64) / 12.0
                    } else if p == (life.trunc() as usize) + 1 && month < 12 {
                        (cost - total_dep) * rate * ((12 - month) as f64) / 12.0
                    } else {
                        (cost - total_dep) * rate
                    };
                    total_dep += dep;
                }
                Ok(EvaluatedCellValue::Number(dep))
            }
            "DDB" => {
                if args.len() < 4 || args.len() > 5 {
                    return Err(FormulaError::Value);
                }
                let cost = self.eval_arg_num(&args[0], ctx, depth)?;
                let salvage = self.eval_arg_num(&args[1], ctx, depth)?;
                let life = self.eval_arg_num(&args[2], ctx, depth)?;
                let period = self.eval_arg_num(&args[3], ctx, depth)?.trunc() as usize;
                let factor = if args.len() == 5 {
                    self.eval_arg_num(&args[4], ctx, depth)?
                } else {
                    2.0
                };
                if cost < 0.0
                    || salvage < 0.0
                    || life <= 0.0
                    || !(1..=10_000).contains(&period)
                    || factor <= 0.0
                {
                    return Err(FormulaError::Num);
                }
                let mut book = cost;
                let mut dep = 0.0;
                for _ in 1..=period {
                    dep = (book * (factor / life)).min((book - salvage).max(0.0));
                    book -= dep;
                }
                Ok(EvaluatedCellValue::Number(dep))
            }
            _ => Err(FormulaError::Name),
        }
    }

    pub(crate) fn eval_info_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if args.len() != 1 {
            return Err(FormulaError::Value);
        }
        let val = self.eval_expr_ctx(&args[0], ctx, depth + 1);
        match name {
            "ISBLANK" => Ok(EvaluatedCellValue::Boolean(matches!(
                val,
                EvaluatedCellValue::Empty
            ))),
            "ISNUMBER" => Ok(EvaluatedCellValue::Boolean(matches!(
                val,
                EvaluatedCellValue::Number(_)
            ))),
            "ISTEXT" => Ok(EvaluatedCellValue::Boolean(matches!(
                val,
                EvaluatedCellValue::Text(_)
            ))),
            "ISLOGICAL" => Ok(EvaluatedCellValue::Boolean(matches!(
                val,
                EvaluatedCellValue::Boolean(_)
            ))),
            "ISERR" => Ok(EvaluatedCellValue::Boolean(matches!(
                val,
                EvaluatedCellValue::Error(e) if e != FormulaError::Na
            ))),
            "ISERROR" => Ok(EvaluatedCellValue::Boolean(matches!(
                val,
                EvaluatedCellValue::Error(_)
            ))),
            "ISNA" => Ok(EvaluatedCellValue::Boolean(matches!(
                val,
                EvaluatedCellValue::Error(FormulaError::Na)
            ))),
            "ISEVEN" | "ISODD" => {
                if let EvaluatedCellValue::Error(e) = val {
                    return Err(e);
                }
                if matches!(val, EvaluatedCellValue::Boolean(_)) {
                    return Err(FormulaError::Value);
                }
                let n = val.as_number().ok_or(FormulaError::Value)?.trunc().abs() as i64;
                let is_even = n % 2 == 0;
                Ok(EvaluatedCellValue::Boolean(if name == "ISEVEN" {
                    is_even
                } else {
                    !is_even
                }))
            }
            "TYPE" => {
                let code = match val {
                    EvaluatedCellValue::Empty | EvaluatedCellValue::Number(_) => 1.0,
                    EvaluatedCellValue::Text(_) => 2.0,
                    EvaluatedCellValue::Boolean(_) => 4.0,
                    EvaluatedCellValue::Error(_) => 16.0,
                };
                Ok(EvaluatedCellValue::Number(code))
            }
            _ => Err(FormulaError::Name),
        }
    }
}

pub(crate) fn matches_criteria(cell: &EvaluatedCellValue, crit: &EvaluatedCellValue) -> bool {
    if let EvaluatedCellValue::Number(cn) = crit {
        return cell.as_number().is_some_and(|v| (v - cn).abs() < 1e-9);
    }
    let crit_str = crit.to_display_string();
    let trimmed = crit_str.trim();
    for op in [">=", "<=", "<>", ">", "<", "="] {
        if let Some(rhs) = trimmed.strip_prefix(op) {
            let rhs_trim = rhs.trim();
            if let Ok(rhs_num) = rhs_trim.parse::<f64>() {
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
            let lhs_s = cell.to_display_string().to_lowercase();
            let rhs_s = rhs_trim.to_lowercase();
            return match op {
                "<>" => lhs_s != rhs_s,
                "=" => lhs_s == rhs_s,
                ">=" => lhs_s >= rhs_s,
                "<=" => lhs_s <= rhs_s,
                ">" => lhs_s > rhs_s,
                _ => lhs_s < rhs_s,
            };
        }
    }
    if let Ok(rhs_num) = trimmed.parse::<f64>() {
        if let Some(lhs_num) = cell.as_number() {
            return (lhs_num - rhs_num).abs() < 1e-9;
        }
    }
    match cell {
        EvaluatedCellValue::Text(s) => wildcard_match(s, trimmed),
        EvaluatedCellValue::Empty => wildcard_match("", trimmed),
        EvaluatedCellValue::Boolean(b) => {
            wildcard_match(if *b { "TRUE" } else { "FALSE" }, trimmed)
        }
        EvaluatedCellValue::Error(e) => wildcard_match(e.code(), trimmed),
        EvaluatedCellValue::Number(_) => wildcard_match(&cell.to_display_string(), trimmed),
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

/// Zero-allocation two-pointer backtracking wildcard matcher (`*`, `?`, `~*`, `~?`, `~~`)
/// operating directly on UTF-8 `&str` slices with case-insensitive comparison.
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

/// Baseline 2D heap-allocated DP wildcard matcher retained for 1:1 benchmark verification.
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

fn serial_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let yr = y - if m <= 2 { 1 } else { 0 };
    let era = if yr >= 0 { yr } else { yr - 399 } / 400;
    let yoe = yr - era * 400;
    let mp = m + if m > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468 + 25_569
}

fn civil_from_serial(serial: i64) -> (i64, i64, i64) {
    let z = serial - 25_569 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + if m <= 2 { 1 } else { 0 }, m, d)
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

fn calc_pmt(rate: f64, nper: f64, pv: f64, fv: f64, t: f64) -> Result<f64, FormulaError> {
    if nper.abs() < 1e-12 {
        return Err(FormulaError::DivByZero);
    }
    if rate.abs() < 1e-12 {
        return Ok((-pv - fv) / nper);
    }
    let factor = (1.0 + rate).powf(nper);
    let denom = (if t != 0.0 { 1.0 + rate } else { 1.0 }) * (factor - 1.0);
    if denom.abs() < 1e-15 {
        return Err(FormulaError::DivByZero);
    }
    Ok(-rate * (pv * factor + fv) / denom)
}

fn calc_fv(rate: f64, nper: f64, pmt: f64, pv: f64, t: f64) -> Result<f64, FormulaError> {
    if rate.abs() < 1e-12 {
        return Ok(-pv - pmt * nper);
    }
    let factor = (1.0 + rate).powf(nper);
    let type_adj = if t != 0.0 { 1.0 + rate } else { 1.0 };
    Ok(-pv * factor - pmt * type_adj * (factor - 1.0) / rate)
}
