use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

const MAX_SERIAL: i64 = 2_958_465;
const MAX_SERIAL_F: f64 = 2_958_465.0;
const SECONDS_PER_DAY: i64 = 86_400;
const MONTHS_PER_YEAR: i64 = 12;
const EXCEL_EPOCH_DAYS: f64 = 25_569.0;
const FALLBACK_UNIX_SECS: u64 = 1_704_067_200;
const MAX_NETWORKDAYS_SPAN: i64 = 1_000_000;
const MONTH_BOUNDS: (f64, f64) = (-1_000_000.0, 1_000_000.0);
const DAY_BOUNDS: (f64, f64) = (-100_000_000.0, 100_000_000.0);
const TIME_BOUNDS: (f64, f64) = (-1_000_000_000.0, 1_000_000_000.0);
const SERIAL_BOUNDS_FROM_ZERO: (f64, f64) = (0.0, MAX_SERIAL_F);
const SERIAL_BOUNDS_FROM_ONE: (f64, f64) = (1.0, MAX_SERIAL_F);

fn check_date_arity(args: &[Expr], min: usize, max: usize) -> Result<(), FormulaError> {
    if args.len() < min || args.len() > max {
        Err(FormulaError::Value)
    } else {
        Ok(())
    }
}

fn number(v: f64) -> Result<EvaluatedCellValue, FormulaError> {
    Ok(EvaluatedCellValue::Number(v))
}

#[inline]
fn is_weekday(serial: i64) -> bool {
    (serial - 2).rem_euclid(7) < 5
}

/// Shifts a year/month pair by `months`, returning the normalized `(year, month)`.
fn shift_year_month(year: i64, month: i64, months: i64) -> Result<(i64, i64), FormulaError> {
    let total_m = year
        .checked_mul(MONTHS_PER_YEAR)
        .and_then(|v| v.checked_add(month - 1))
        .and_then(|v| v.checked_add(months))
        .ok_or(FormulaError::Num)?;
    Ok((
        total_m.div_euclid(MONTHS_PER_YEAR),
        total_m.rem_euclid(MONTHS_PER_YEAR) + 1,
    ))
}

/// Difference between two serials for a DATEDIF unit, or `None` for an unknown unit.
fn datedif_diff(unit: &str, s1: i64, s2: i64) -> Option<i64> {
    let (y1, m1, d1) = civil_from_serial(s1);
    let (y2, m2, d2) = civil_from_serial(s2);
    let diff = match unit {
        "D" => s2 - s1,
        "M" => (y2 - y1) * 12 + (m2 - m1) - i64::from(d2 < d1),
        "Y" => (y2 - y1) - i64::from((m2, d2) < (m1, d1)),
        "YM" => ((m2 - m1) - i64::from(d2 < d1)).rem_euclid(12),
        "MD" if d2 >= d1 => d2 - d1,
        "MD" => {
            let (py, pm) = if m2 == 1 { (y2 - 1, 12) } else { (y2, m2 - 1) };
            days_in_month(py, pm) - d1 + d2
        }
        "YD" => {
            let mut cand = serial_from_civil(y2, m1, d1);
            if cand > s2 {
                cand = serial_from_civil(y2 - 1, m1, d1);
            }
            s2 - cand
        }
        _ => return None,
    };
    Some(diff)
}

fn date_today_now(is_today: bool, args: &[Expr]) -> Result<EvaluatedCellValue, FormulaError> {
    check_date_arity(args, 0, 0)?;
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(FALLBACK_UNIX_SECS);
    let days = EXCEL_EPOCH_DAYS + ((secs / SECONDS_PER_DAY as u64) as f64);
    number(if is_today {
        days
    } else {
        days + ((secs % SECONDS_PER_DAY as u64) as f64) / SECONDS_PER_DAY as f64
    })
}

impl SheetEngine {
    pub(crate) fn eval_date_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        match name {
            "DATE" => self.date_date(args, ctx, depth),
            "YEAR" | "MONTH" | "DAY" => self.date_ymd_part(name, args, ctx, depth),
            "TIME" => self.date_time(args, ctx, depth),
            "HOUR" | "MINUTE" | "SECOND" => self.date_time_part(name, args, ctx, depth),
            "DAYS" => self.date_days(args, ctx, depth),
            "EDATE" | "EOMONTH" => self.date_edate(name == "EOMONTH", args, ctx, depth),
            "TODAY" | "NOW" => date_today_now(name == "TODAY", args),
            "DATEVALUE" => self.date_datevalue(args, ctx, depth),
            "WEEKDAY" => self.date_weekday(args, ctx, depth),
            "DATEDIF" => self.date_datedif(args, ctx, depth),
            "WORKDAY" => self.date_workday(args, ctx, depth),
            "NETWORKDAYS" => self.date_networkdays(args, ctx, depth),
            _ => Err(FormulaError::Name),
        }
    }

    /// Evaluates `arg` as a number and range-checks it into an `i64`.
    fn arg_i64(
        &self,
        arg: &Expr,
        bounds: (f64, f64),
        floor: bool,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<i64, FormulaError> {
        let v = self.eval_arg_num(arg, ctx, depth)?;
        checked_f64_to_i64(if floor { v.floor() } else { v }, bounds.0, bounds.1)
    }

    fn date_date(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_date_arity(args, 3, 3)?;
        let mut y = self.arg_i64(&args[0], (0.0, 9_999.999_999), false, ctx, depth)?;
        if (0..1900).contains(&y) {
            y += 1900;
        }
        let m = self.arg_i64(&args[1], MONTH_BOUNDS, false, ctx, depth)?;
        let d = self.arg_i64(&args[2], DAY_BOUNDS, false, ctx, depth)?;
        let (adj_y, adj_m) = shift_year_month(y, 1, m - 1)?;
        let serial = serial_from_civil(adj_y, adj_m, 1)
            .checked_add(d - 1)
            .ok_or(FormulaError::Num)?;
        if !(0..=MAX_SERIAL).contains(&serial) {
            return Err(FormulaError::Num);
        }
        number(serial as f64)
    }

    fn date_ymd_part(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_date_arity(args, 1, 1)?;
        let serial = self.arg_i64(&args[0], SERIAL_BOUNDS_FROM_ZERO, true, ctx, depth)?;
        let (y, m, d) = if serial == 0 {
            (1900, 1, 0)
        } else {
            civil_from_serial(serial)
        };
        number(match name {
            "YEAR" => y,
            "MONTH" => m,
            _ => d,
        } as f64)
    }

    fn date_time(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_date_arity(args, 3, 3)?;
        let h = self.arg_i64(&args[0], TIME_BOUNDS, false, ctx, depth)?;
        let m = self.arg_i64(&args[1], TIME_BOUNDS, false, ctx, depth)?;
        let s = self.arg_i64(&args[2], TIME_BOUNDS, false, ctx, depth)?;
        let total = h
            .checked_mul(3600)
            .and_then(|v| m.checked_mul(60).and_then(|m60| v.checked_add(m60)))
            .and_then(|v| v.checked_add(s))
            .ok_or(FormulaError::Num)?;
        if total < 0 {
            return Err(FormulaError::Num);
        }
        number(((total % SECONDS_PER_DAY) as f64) / SECONDS_PER_DAY as f64)
    }

    fn date_time_part(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_date_arity(args, 1, 1)?;
        let n = self.eval_arg_num(&args[0], ctx, depth)?;
        if !n.is_finite() || !(0.0..=1e15).contains(&n) {
            return Err(FormulaError::Num);
        }
        let secs = (n.fract() * SECONDS_PER_DAY as f64).round() as i64 % SECONDS_PER_DAY;
        number(match name {
            "HOUR" => secs / 3600,
            "MINUTE" => (secs % 3600) / 60,
            _ => secs % 60,
        } as f64)
    }

    fn date_days(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_date_arity(args, 2, 2)?;
        let end_d = self.arg_i64(&args[0], DAY_BOUNDS, false, ctx, depth)?;
        let start_d = self.arg_i64(&args[1], DAY_BOUNDS, false, ctx, depth)?;
        number((end_d - start_d) as f64)
    }

    fn date_edate(
        &self,
        end_of_month: bool,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_date_arity(args, 2, 2)?;
        let start_s = self.arg_i64(&args[0], SERIAL_BOUNDS_FROM_ONE, true, ctx, depth)?;
        let months = self.arg_i64(&args[1], MONTH_BOUNDS, false, ctx, depth)?;
        let (y, m, d) = civil_from_serial(start_s);
        let (ny, nm) = shift_year_month(y, m, months)?;
        let max_d = days_in_month(ny, nm);
        let nd = if end_of_month { max_d } else { d.min(max_d) };
        let serial = serial_from_civil(ny, nm, nd);
        if !(1..=MAX_SERIAL).contains(&serial) {
            return Err(FormulaError::Num);
        }
        number(serial as f64)
    }

    fn date_datevalue(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_date_arity(args, 1, 1)?;
        let EvaluatedCellValue::Text(s) = self.eval_arg_val(&args[0], ctx, depth)? else {
            return Err(FormulaError::Value);
        };
        parse_iso_date_serial(&s)
            .map(EvaluatedCellValue::Number)
            .ok_or(FormulaError::Value)
    }

    fn date_weekday(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_date_arity(args, 1, 2)?;
        let serial = self.arg_i64(&args[0], SERIAL_BOUNDS_FROM_ZERO, true, ctx, depth)?;
        let ret_type = match args.get(1) {
            Some(arg) => self.arg_i64(arg, (1.0, 3.0), false, ctx, depth)?,
            None => 1,
        };
        let w = match ret_type {
            1 => (serial - 1).rem_euclid(7) + 1,
            2 => (serial - 2).rem_euclid(7) + 1,
            3 => (serial - 2).rem_euclid(7),
            _ => return Err(FormulaError::Num),
        };
        number(w as f64)
    }

    fn date_datedif(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_date_arity(args, 3, 3)?;
        let s1 = self.arg_i64(&args[0], SERIAL_BOUNDS_FROM_ZERO, true, ctx, depth)?;
        let s2 = self.arg_i64(&args[1], SERIAL_BOUNDS_FROM_ZERO, true, ctx, depth)?;
        if s1 > s2 {
            return Err(FormulaError::Num);
        }
        let EvaluatedCellValue::Text(unit) = self.eval_arg_val(&args[2], ctx, depth)? else {
            return Err(FormulaError::Value);
        };
        let diff = datedif_diff(unit.trim().to_ascii_uppercase().as_str(), s1, s2)
            .ok_or(FormulaError::Num)?;
        number(diff as f64)
    }

    /// Collects the optional holiday list at `args[idx]` as floored serials.
    fn eval_holidays(
        &self,
        args: &[Expr],
        idx: usize,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<std::collections::HashSet<i64>, FormulaError> {
        let mut holidays = std::collections::HashSet::new();
        if let Some(arg) = args.get(idx) {
            for v in self.eval_range_vec(arg, ctx, depth)? {
                if let Some(n) = v.as_number() {
                    holidays.insert(n.floor() as i64);
                }
            }
        }
        Ok(holidays)
    }

    fn date_workday(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_date_arity(args, 2, 3)?;
        let mut cur = self.arg_i64(&args[0], SERIAL_BOUNDS_FROM_ONE, true, ctx, depth)?;
        let rem = self.arg_i64(&args[1], MONTH_BOUNDS, false, ctx, depth)?;
        let holidays = self.eval_holidays(args, 2, ctx, depth)?;
        let step = if rem >= 0 { 1 } else { -1 };
        let mut rem = rem.abs();
        while rem > 0 {
            cur += step;
            if is_weekday(cur) && !holidays.contains(&cur) {
                rem -= 1;
            }
        }
        if !(1..=MAX_SERIAL).contains(&cur) {
            return Err(FormulaError::Num);
        }
        number(cur as f64)
    }

    fn date_networkdays(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_date_arity(args, 2, 3)?;
        let s1 = self.arg_i64(&args[0], SERIAL_BOUNDS_FROM_ONE, true, ctx, depth)?;
        let s2 = self.arg_i64(&args[1], SERIAL_BOUNDS_FROM_ONE, true, ctx, depth)?;
        let holidays = self.eval_holidays(args, 2, ctx, depth)?;
        let (lo, hi, sign) = if s1 <= s2 {
            (s1, s2, 1i64)
        } else {
            (s2, s1, -1i64)
        };
        if hi - lo > MAX_NETWORKDAYS_SPAN {
            return Err(FormulaError::Num);
        }
        let cnt = (lo..=hi)
            .filter(|d| is_weekday(*d) && !holidays.contains(d))
            .count() as i64;
        number((cnt * sign) as f64)
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
        let val = self.eval_expr_ctx(&args[0], ctx, depth);
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
                let raw_n = val.as_number().ok_or(FormulaError::Value)?;
                let n = checked_f64_to_i64(raw_n.trunc().abs(), 0.0, 9e15)?;
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

pub(crate) fn parse_iso_date_serial(s: &str) -> Option<f64> {
    let trimmed = s.trim();
    let sep = if trimmed.contains('-') {
        '-'
    } else if trimmed.contains('/') {
        '/'
    } else {
        return None;
    };
    let mut parts = trimmed.split(sep);
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let d: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1900..=9999).contains(&y) || !(1..=12).contains(&m) {
        return None;
    }
    if d < 1 || d > days_in_month(y, m) {
        return None;
    }
    Some(serial_from_civil(y, m, d) as f64)
}

pub(crate) fn ymd_to_excel_serial(y: i64, m: i64, d: i64) -> Result<i64, FormulaError> {
    if !(1900..=9999).contains(&y) || !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) {
        return Err(FormulaError::Value);
    }
    Ok(serial_from_civil(y, m, d))
}

#[inline]
fn checked_f64_to_i64(n: f64, min: f64, max: f64) -> Result<i64, FormulaError> {
    if !n.is_finite() || n < min || n > max {
        return Err(FormulaError::Num);
    }
    Ok(n.trunc() as i64)
}

pub(crate) fn serial_from_civil(y: i64, m: i64, d: i64) -> i64 {
    if y == 1900 && m == 2 && d == 29 {
        return 60;
    }
    let yr = y - if m <= 2 { 1 } else { 0 };
    let era = if yr >= 0 { yr } else { yr - 399 } / 400;
    let yoe = yr - era * 400;
    let mp = m + if m > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let raw = era * 146_097 + doe - 719_468 + 25_569;
    if raw <= 60 {
        raw - 1
    } else {
        raw
    }
}

pub(crate) fn civil_from_serial(serial: i64) -> (i64, i64, i64) {
    if serial == 60 {
        return (1900, 2, 29);
    }
    let adj = if serial < 60 { serial + 1 } else { serial };
    let z = adj - 25_569 + 719_468;
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
            if y == 1900 || (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}
