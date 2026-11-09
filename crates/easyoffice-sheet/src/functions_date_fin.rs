//! Date, time, and cell-inspection functions, including `WORKDAY.INTL` and `NETWORKDAYS.INTL`
//! backed by a 7-bit `u8` weekend bitmask (`FormulaJS`, MIT License, Copyright (c) Sutoiku, Inc.).

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
/// Default Saturday (bit 5) + Sunday (bit 6) weekend bitmask (`0b110_0000`).
const DEFAULT_WEEKEND_MASK: u8 = (1 << 5) | (1 << 6);

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
fn is_workday_with_mask(serial: i64, weekend_mask: u8) -> bool {
    let weekday_mon0 = (serial - 2).rem_euclid(7) as u8;
    ((weekend_mask >> weekday_mon0) & 1) == 0
}

/// Compiles an Excel weekend code (`1..=7`, `11..=17`) or 7-character `"0000011"` string
/// (Monday..Sunday) into a 7-bit `u8` bitmask where bit `weekday_mon0` is `1` for weekend days.
fn compile_weekend_mask(val: &EvaluatedCellValue) -> Result<u8, FormulaError> {
    match val {
        EvaluatedCellValue::Error(e) => Err(*e),
        EvaluatedCellValue::Empty => Ok(DEFAULT_WEEKEND_MASK),
        EvaluatedCellValue::Boolean(_) => Err(FormulaError::Value),
        EvaluatedCellValue::Number(n) => {
            if !n.is_finite() {
                return Err(FormulaError::Num);
            }
            let code = n.trunc() as i64;
            match code {
                1 => Ok((1 << 5) | (1 << 6)),
                2 => Ok((1 << 6) | (1 << 0)),
                3 => Ok((1 << 0) | (1 << 1)),
                4 => Ok((1 << 1) | (1 << 2)),
                5 => Ok((1 << 2) | (1 << 3)),
                6 => Ok((1 << 3) | (1 << 4)),
                7 => Ok((1 << 4) | (1 << 5)),
                11 => Ok(1 << 6),
                12 => Ok(1 << 0),
                13 => Ok(1 << 1),
                14 => Ok(1 << 2),
                15 => Ok(1 << 3),
                16 => Ok(1 << 4),
                17 => Ok(1 << 5),
                _ => Err(FormulaError::Num),
            }
        }
        EvaluatedCellValue::Text(s) => {
            if s.len() == 7 && s.bytes().all(|b| b == b'0' || b == b'1') {
                let mut mask = 0u8;
                for (i, b) in s.bytes().enumerate() {
                    if b == b'1' {
                        mask |= 1u8 << i;
                    }
                }
                if mask == 0b111_1111 {
                    return Err(FormulaError::Value);
                }
                Ok(mask)
            } else if let Some(n) = crate::coercion::parse_strict_number(s) {
                compile_weekend_mask(&EvaluatedCellValue::Number(n))
            } else {
                Err(FormulaError::Value)
            }
        }
    }
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
            "TIMEVALUE" => self.date_timevalue(args, ctx, depth),
            "WEEKDAY" => self.date_weekday(args, ctx, depth),
            "WEEKNUM" | "ISOWEEKNUM" => self.date_weeknum(name == "ISOWEEKNUM", args, ctx, depth),
            "YEARFRAC" => self.date_yearfrac(args, ctx, depth),
            "DATEDIF" => self.date_datedif(args, ctx, depth),
            "WORKDAY" | "WORKDAY.INTL" => {
                self.date_workday(name == "WORKDAY.INTL", args, ctx, depth)
            }
            "NETWORKDAYS" | "NETWORKDAYS.INTL" => {
                self.date_networkdays(name == "NETWORKDAYS.INTL", args, ctx, depth)
            }
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

    fn date_timevalue(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_date_arity(args, 1, 1)?;
        let EvaluatedCellValue::Text(s) = self.eval_arg_val(&args[0], ctx, depth)? else {
            return Err(FormulaError::Value);
        };
        parse_time_serial(&s)
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

    fn date_weeknum(
        &self,
        is_iso: bool,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_date_arity(args, 1, if is_iso { 1 } else { 2 })?;
        let serial = self.arg_i64(&args[0], SERIAL_BOUNDS_FROM_ONE, true, ctx, depth)?;
        let ret_type = if is_iso {
            21
        } else if let Some(arg) = args.get(1) {
            self.arg_i64(arg, (1.0, 21.0), false, ctx, depth)?
        } else {
            1
        };
        if ret_type == 21 {
            let thu = serial + 3 - (serial - 2).rem_euclid(7);
            let (iso_y, _, _) = civil_from_serial(thu);
            let jan1 = serial_from_civil(iso_y, 1, 1);
            return number(((thu - jan1) / 7 + 1) as f64);
        }
        let week_start = match ret_type {
            1 | 17 => 0,
            2 | 11 => 1,
            12 => 2,
            13 => 3,
            14 => 4,
            15 => 5,
            16 => 6,
            _ => return Err(FormulaError::Num),
        };
        let (y, _, _) = civil_from_serial(serial);
        let jan1 = serial_from_civil(y, 1, 1);
        let jan1_off = ((jan1 - 1).rem_euclid(7) - week_start).rem_euclid(7);
        number(((serial - jan1 + jan1_off) / 7 + 1) as f64)
    }

    fn date_yearfrac(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_date_arity(args, 2, 3)?;
        let s1 = self.arg_i64(&args[0], SERIAL_BOUNDS_FROM_ONE, true, ctx, depth)?;
        let s2 = self.arg_i64(&args[1], SERIAL_BOUNDS_FROM_ONE, true, ctx, depth)?;
        let basis = match args.get(2) {
            Some(arg) if !matches!(arg, Expr::Missing) => {
                self.arg_i64(arg, (0.0, 4.0), false, ctx, depth)?
            }
            _ => 0,
        };
        let (d1, d2) = if s1 <= s2 { (s1, s2) } else { (s2, s1) };
        number(compute_yearfrac(d1, d2, basis))
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
            if !matches!(arg, Expr::Missing) {
                for v in self.eval_range_vec(arg, ctx, depth)? {
                    if let EvaluatedCellValue::Error(e) = v {
                        return Err(e);
                    }
                    if let Some(n) = v.as_number() {
                        holidays.insert(n.floor() as i64);
                    }
                }
            }
        }
        Ok(holidays)
    }

    fn eval_weekend_mask_arg(
        &self,
        args: &[Expr],
        is_intl: bool,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<(u8, usize), FormulaError> {
        if !is_intl {
            return Ok((DEFAULT_WEEKEND_MASK, 2));
        }
        let mask = match args.get(2) {
            None | Some(Expr::Missing) => DEFAULT_WEEKEND_MASK,
            Some(arg) => {
                let v = self.eval_arg_val(arg, ctx, depth)?;
                compile_weekend_mask(&v)?
            }
        };
        Ok((mask, 3))
    }

    fn date_workday(
        &self,
        is_intl: bool,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_date_arity(args, 2, if is_intl { 4 } else { 3 })?;
        let mut cur = self.arg_i64(&args[0], SERIAL_BOUNDS_FROM_ONE, true, ctx, depth)?;
        let rem = self.arg_i64(&args[1], MONTH_BOUNDS, false, ctx, depth)?;
        let (weekend_mask, holiday_idx) = self.eval_weekend_mask_arg(args, is_intl, ctx, depth)?;
        let holidays = self.eval_holidays(args, holiday_idx, ctx, depth)?;
        let step = if rem >= 0 { 1 } else { -1 };
        let mut rem = rem.abs();
        while rem > 0 {
            cur += step;
            if is_workday_with_mask(cur, weekend_mask) && !holidays.contains(&cur) {
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
        is_intl: bool,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_date_arity(args, 2, if is_intl { 4 } else { 3 })?;
        let s1 = self.arg_i64(&args[0], SERIAL_BOUNDS_FROM_ONE, true, ctx, depth)?;
        let s2 = self.arg_i64(&args[1], SERIAL_BOUNDS_FROM_ONE, true, ctx, depth)?;
        let (weekend_mask, holiday_idx) = self.eval_weekend_mask_arg(args, is_intl, ctx, depth)?;
        let holidays = self.eval_holidays(args, holiday_idx, ctx, depth)?;
        let (lo, hi, sign) = if s1 <= s2 {
            (s1, s2, 1i64)
        } else {
            (s2, s1, -1i64)
        };
        if hi - lo > MAX_NETWORKDAYS_SPAN {
            return Err(FormulaError::Num);
        }
        let cnt = (lo..=hi)
            .filter(|d| is_workday_with_mask(*d, weekend_mask) && !holidays.contains(d))
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
            "ERROR.TYPE" => match val {
                EvaluatedCellValue::Error(e) => number(match e {
                    FormulaError::DivByZero => 2.0,
                    FormulaError::Value => 3.0,
                    FormulaError::Ref => 4.0,
                    FormulaError::Name => 5.0,
                    FormulaError::Num | FormulaError::Cycle => 6.0,
                    FormulaError::Na => 7.0,
                }),
                _ => Err(FormulaError::Na),
            },
            _ => Err(FormulaError::Name),
        }
    }
}

fn parse_time_serial(s: &str) -> Option<f64> {
    let mut t = s.trim();
    if let Some((date_part, rest)) = t.split_once(' ') {
        if parse_iso_date_serial(date_part).is_some() {
            t = rest.trim();
        }
    }
    let up = t.to_ascii_uppercase();
    let (body, ampm) = if let Some(b) = up.strip_suffix("AM") {
        (b.trim(), Some(false))
    } else if let Some(b) = up.strip_suffix("PM") {
        (b.trim(), Some(true))
    } else {
        (up.as_str(), None)
    };
    let mut parts = body.split(':');
    let mut h: u32 = parts.next()?.trim().parse().ok()?;
    let m: u32 = parts.next()?.trim().parse().ok()?;
    let sec: f64 = match parts.next() {
        Some(sp) => sp.trim().parse().ok()?,
        None => 0.0,
    };
    if parts.next().is_some() || m >= 60 || !sec.is_finite() || !(0.0..60.0).contains(&sec) {
        return None;
    }
    if let Some(is_pm) = ampm {
        if !(1..=12).contains(&h) {
            return None;
        }
        h = (h % 12) + if is_pm { 12 } else { 0 };
    } else if h >= 24 {
        return None;
    }
    Some((h as f64 * 3600.0 + m as f64 * 60.0 + sec) / SECONDS_PER_DAY as f64)
}

fn compute_yearfrac(d1: i64, d2: i64, basis: i64) -> f64 {
    if d1 == d2 {
        return 0.0;
    }
    let (y1, m1, mut day1) = civil_from_serial(d1);
    let (y2, m2, mut day2) = civil_from_serial(d2);
    match basis {
        0 => {
            if m1 == 2 && day1 == days_in_month(y1, 2) {
                if m2 == 2 && day2 == days_in_month(y2, 2) {
                    day2 = 30;
                }
                day1 = 30;
            }
            if day2 == 31 && day1 >= 30 {
                day2 = 30;
            }
            if day1 == 31 {
                day1 = 30;
            }
            ((y2 - y1) * 360 + (m2 - m1) * 30 + (day2 - day1)) as f64 / 360.0
        }
        1 => {
            let denom = if (y2 - y1) > 1 || (y2 == y1 + 1 && (m2, day2) > (m1, day1)) {
                (y1..=y2)
                    .map(|y| {
                        if days_in_month(y, 2) == 29 {
                            366.0
                        } else {
                            365.0
                        }
                    })
                    .sum::<f64>()
                    / (y2 - y1 + 1) as f64
            } else {
                let feb29_y1 = days_in_month(y1, 2) == 29
                    && serial_from_civil(y1, 2, 29) >= d1
                    && serial_from_civil(y1, 2, 29) <= d2;
                let feb29_y2 = days_in_month(y2, 2) == 29
                    && serial_from_civil(y2, 2, 29) >= d1
                    && serial_from_civil(y2, 2, 29) <= d2;
                if feb29_y1 || feb29_y2 || (y1 == y2 && days_in_month(y1, 2) == 29) {
                    366.0
                } else {
                    365.0
                }
            };
            (d2 - d1) as f64 / denom
        }
        2 => (d2 - d1) as f64 / 360.0,
        3 => (d2 - d1) as f64 / 365.0,
        _ => ((y2 - y1) * 360 + (m2 - m1) * 30 + (day2.min(30) - day1.min(30))) as f64 / 360.0,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workday_intl_and_networkdays_intl_weekend_bitmask() {
        let mut s = SheetEngine::new("DateIntlTest");
        s.set_cell("A1", "=DATE(2026,1,5)").unwrap(); // Monday
        s.set_cell("A2", "=DATE(2026,1,16)").unwrap(); // Friday (next week)
        s.set_cell("H1", "=DATE(2026,1,7)").unwrap();
        assert_eq!(
            s.evaluate_formula_str("=NETWORKDAYS.INTL(A1, A2, 1)"),
            EvaluatedCellValue::Number(10.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=NETWORKDAYS.INTL(A1, A2, 11)"),
            EvaluatedCellValue::Number(11.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=NETWORKDAYS.INTL(A1, A2, \"0000111\", H1)"),
            EvaluatedCellValue::Number(7.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=WORKDAY.INTL(A1, 5, 1)"),
            s.evaluate_formula_str("=DATE(2026,1,12)")
        );
        assert_eq!(
            s.evaluate_formula_str("=WORKDAY.INTL(A1, 5, \"0000111\")"),
            s.evaluate_formula_str("=DATE(2026,1,13)")
        );
        assert_eq!(
            s.evaluate_formula_str("=WORKDAY.INTL(A1, 5, \"1111111\")"),
            EvaluatedCellValue::Error(FormulaError::Value)
        );
    }
}
