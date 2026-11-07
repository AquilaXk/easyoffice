use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

impl SheetEngine {
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
                let mut y = checked_f64_to_i64(
                    self.eval_arg_num(&args[0], ctx, depth)?,
                    0.0,
                    9_999.999_999,
                )?;
                if (0..1900).contains(&y) {
                    y += 1900;
                }
                let m = checked_f64_to_i64(
                    self.eval_arg_num(&args[1], ctx, depth)?,
                    -1_000_000.0,
                    1_000_000.0,
                )?;
                let d = checked_f64_to_i64(
                    self.eval_arg_num(&args[2], ctx, depth)?,
                    -100_000_000.0,
                    100_000_000.0,
                )?;
                let total_m = y
                    .checked_mul(12)
                    .and_then(|v| v.checked_add(m - 1))
                    .ok_or(FormulaError::Num)?;
                let adj_y = total_m.div_euclid(12);
                let adj_m = total_m.rem_euclid(12) + 1;
                let serial = serial_from_civil(adj_y, adj_m, 1)
                    .checked_add(d - 1)
                    .ok_or(FormulaError::Num)?;
                if !(0..=2_958_465).contains(&serial) {
                    return Err(FormulaError::Num);
                }
                Ok(EvaluatedCellValue::Number(serial as f64))
            }
            "YEAR" | "MONTH" | "DAY" => {
                if args.len() != 1 {
                    return Err(FormulaError::Value);
                }
                let raw = self.eval_arg_num(&args[0], ctx, depth)?;
                let serial = checked_f64_to_i64(raw.floor(), 0.0, 2_958_465.0)?;
                let (y, m, d) = if serial == 0 {
                    (1900, 1, 0)
                } else {
                    civil_from_serial(serial)
                };
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
                let h = checked_f64_to_i64(
                    self.eval_arg_num(&args[0], ctx, depth)?,
                    -1_000_000_000.0,
                    1_000_000_000.0,
                )?;
                let m = checked_f64_to_i64(
                    self.eval_arg_num(&args[1], ctx, depth)?,
                    -1_000_000_000.0,
                    1_000_000_000.0,
                )?;
                let s = checked_f64_to_i64(
                    self.eval_arg_num(&args[2], ctx, depth)?,
                    -1_000_000_000.0,
                    1_000_000_000.0,
                )?;
                let total = h
                    .checked_mul(3600)
                    .and_then(|v| m.checked_mul(60).and_then(|m60| v.checked_add(m60)))
                    .and_then(|v| v.checked_add(s))
                    .ok_or(FormulaError::Num)?;
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
                if !n.is_finite() || !(0.0..=1e15).contains(&n) {
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
                let end_d = checked_f64_to_i64(
                    self.eval_arg_num(&args[0], ctx, depth)?,
                    -100_000_000.0,
                    100_000_000.0,
                )?;
                let start_d = checked_f64_to_i64(
                    self.eval_arg_num(&args[1], ctx, depth)?,
                    -100_000_000.0,
                    100_000_000.0,
                )?;
                Ok(EvaluatedCellValue::Number((end_d - start_d) as f64))
            }
            "EDATE" | "EOMONTH" => {
                if args.len() != 2 {
                    return Err(FormulaError::Value);
                }
                let start_raw = self.eval_arg_num(&args[0], ctx, depth)?;
                let start_s = checked_f64_to_i64(start_raw.floor(), 1.0, 2_958_465.0)?;
                let months = checked_f64_to_i64(
                    self.eval_arg_num(&args[1], ctx, depth)?,
                    -1_000_000.0,
                    1_000_000.0,
                )?;
                let (y, m, d) = civil_from_serial(start_s);
                let total_m = y
                    .checked_mul(12)
                    .and_then(|v| v.checked_add(m - 1))
                    .and_then(|v| v.checked_add(months))
                    .ok_or(FormulaError::Num)?;
                let ny = total_m.div_euclid(12);
                let nm = total_m.rem_euclid(12) + 1;
                let max_d = days_in_month(ny, nm);
                let nd = if name == "EOMONTH" {
                    max_d
                } else {
                    d.min(max_d)
                };
                let serial = serial_from_civil(ny, nm, nd);
                if !(1..=2_958_465).contains(&serial) {
                    return Err(FormulaError::Num);
                }
                Ok(EvaluatedCellValue::Number(serial as f64))
            }
            "TODAY" | "NOW" => {
                if !args.is_empty() {
                    return Err(FormulaError::Value);
                }
                let secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(1_704_067_200);
                let days = 25_569.0 + ((secs / 86_400) as f64);
                Ok(EvaluatedCellValue::Number(if name == "TODAY" {
                    days
                } else {
                    days + ((secs % 86_400) as f64) / 86_400.0
                }))
            }
            "DATEVALUE" => {
                if args.len() != 1 {
                    return Err(FormulaError::Value);
                }
                let EvaluatedCellValue::Text(s) = self.eval_arg_val(&args[0], ctx, depth)? else {
                    return Err(FormulaError::Value);
                };
                parse_iso_date_serial(&s)
                    .map(EvaluatedCellValue::Number)
                    .ok_or(FormulaError::Value)
            }
            "WEEKDAY" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(FormulaError::Value);
                }
                let serial = checked_f64_to_i64(
                    self.eval_arg_num(&args[0], ctx, depth)?.floor(),
                    0.0,
                    2_958_465.0,
                )?;
                let ret_type = if args.len() == 2 {
                    checked_f64_to_i64(self.eval_arg_num(&args[1], ctx, depth)?, 1.0, 3.0)?
                } else {
                    1
                };
                let w = match ret_type {
                    1 => (serial - 1).rem_euclid(7) + 1,
                    2 => (serial - 2).rem_euclid(7) + 1,
                    3 => (serial - 2).rem_euclid(7),
                    _ => return Err(FormulaError::Num),
                };
                Ok(EvaluatedCellValue::Number(w as f64))
            }
            "DATEDIF" => {
                if args.len() != 3 {
                    return Err(FormulaError::Value);
                }
                let s1 = checked_f64_to_i64(
                    self.eval_arg_num(&args[0], ctx, depth)?.floor(),
                    0.0,
                    2_958_465.0,
                )?;
                let s2 = checked_f64_to_i64(
                    self.eval_arg_num(&args[1], ctx, depth)?.floor(),
                    0.0,
                    2_958_465.0,
                )?;
                if s1 > s2 {
                    return Err(FormulaError::Num);
                }
                let EvaluatedCellValue::Text(u_raw) = self.eval_arg_val(&args[2], ctx, depth)?
                else {
                    return Err(FormulaError::Value);
                };
                let (y1, m1, d1) = civil_from_serial(s1);
                let (y2, m2, d2) = civil_from_serial(s2);
                let diff = match u_raw.trim().to_ascii_uppercase().as_str() {
                    "D" => s2 - s1,
                    "M" => (y2 - y1) * 12 + (m2 - m1) - i64::from(d2 < d1),
                    "Y" => (y2 - y1) - i64::from((m2, d2) < (m1, d1)),
                    "YM" => ((m2 - m1) - i64::from(d2 < d1)).rem_euclid(12),
                    "MD" => {
                        if d2 >= d1 {
                            d2 - d1
                        } else {
                            let (py, pm) = if m2 == 1 { (y2 - 1, 12) } else { (y2, m2 - 1) };
                            days_in_month(py, pm) - d1 + d2
                        }
                    }
                    "YD" => {
                        let mut cand = serial_from_civil(y2, m1, d1);
                        if cand > s2 {
                            cand = serial_from_civil(y2 - 1, m1, d1);
                        }
                        s2 - cand
                    }
                    _ => return Err(FormulaError::Num),
                };
                Ok(EvaluatedCellValue::Number(diff as f64))
            }
            "WORKDAY" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(FormulaError::Value);
                }
                let mut cur = checked_f64_to_i64(
                    self.eval_arg_num(&args[0], ctx, depth)?.floor(),
                    1.0,
                    2_958_465.0,
                )?;
                let mut rem = checked_f64_to_i64(
                    self.eval_arg_num(&args[1], ctx, depth)?,
                    -1_000_000.0,
                    1_000_000.0,
                )?;
                let mut holidays = std::collections::HashSet::new();
                if args.len() == 3 {
                    for v in self.eval_range_vec(&args[2], ctx, depth)? {
                        if let Some(n) = v.as_number() {
                            holidays.insert(n.floor() as i64);
                        }
                    }
                }
                let step = if rem >= 0 { 1 } else { -1 };
                rem = rem.abs();
                while rem > 0 {
                    cur += step;
                    let wd = (cur - 2).rem_euclid(7) + 1;
                    if wd <= 5 && !holidays.contains(&cur) {
                        rem -= 1;
                    }
                }
                if !(1..=2_958_465).contains(&cur) {
                    return Err(FormulaError::Num);
                }
                Ok(EvaluatedCellValue::Number(cur as f64))
            }
            "NETWORKDAYS" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(FormulaError::Value);
                }
                let s1 = checked_f64_to_i64(
                    self.eval_arg_num(&args[0], ctx, depth)?.floor(),
                    1.0,
                    2_958_465.0,
                )?;
                let s2 = checked_f64_to_i64(
                    self.eval_arg_num(&args[1], ctx, depth)?.floor(),
                    1.0,
                    2_958_465.0,
                )?;
                let mut holidays = std::collections::HashSet::new();
                if args.len() == 3 {
                    for v in self.eval_range_vec(&args[2], ctx, depth)? {
                        if let Some(n) = v.as_number() {
                            holidays.insert(n.floor() as i64);
                        }
                    }
                }
                let (lo, hi, sign) = if s1 <= s2 {
                    (s1, s2, 1i64)
                } else {
                    (s2, s1, -1i64)
                };
                if hi - lo > 1_000_000 {
                    return Err(FormulaError::Num);
                }
                let mut cnt = 0i64;
                for d in lo..=hi {
                    let wd = (d - 2).rem_euclid(7) + 1;
                    if wd <= 5 && !holidays.contains(&d) {
                        cnt += 1;
                    }
                }
                Ok(EvaluatedCellValue::Number((cnt * sign) as f64))
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
                    calc_fv(rate, per - 1.0, pmt, pv, t)? * rate
                        / (if t != 0.0 { 1.0 + rate } else { 1.0 })
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
                let mut npv = 0.0;
                let mut t = 0i32;
                for arg in &args[1..] {
                    if let Some(spec) = self.extract_streaming_range_ctx(arg, ctx, depth)? {
                        let (_, _, _, rows, cols, _) = spec;
                        for dr in 0..rows {
                            for dc in 0..cols {
                                let v = self.eval_streaming_cell(&spec, dr, dc, ctx, depth)?;
                                if let EvaluatedCellValue::Number(n) = v {
                                    t += 1;
                                    npv += n / (1.0 + rate).powi(t);
                                }
                            }
                        }
                    } else {
                        let v = self.eval_arg_val(arg, ctx, depth)?;
                        if !matches!(v, EvaluatedCellValue::Empty) {
                            let n = v.as_number().ok_or(FormulaError::Value)?;
                            t += 1;
                            npv += n / (1.0 + rate).powi(t);
                        }
                    }
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
                if nper <= 0.0 {
                    return Err(FormulaError::Num);
                }
                let scale = pv.abs().max(fv.abs()).max(pmt.abs() * nper).max(1.0);
                let f_tol = 1e-9 * scale;
                for _ in 0..100 {
                    if r <= -0.999_999 || !r.is_finite() {
                        break;
                    }
                    let Ok(fv_r) = calc_fv(r, nper, pmt, pv, t) else {
                        break;
                    };
                    let f = fv_r - fv;
                    if f.abs() <= f_tol {
                        return Ok(EvaluatedCellValue::Number(r));
                    }
                    let eps = 1e-7 * r.abs().max(1.0);
                    let Ok(fv_eps) = calc_fv(r + eps, nper, pmt, pv, t) else {
                        break;
                    };
                    let df = (fv_eps - fv - f) / eps;
                    if !df.is_finite() || df.abs() < 1e-15 {
                        break;
                    }
                    let step = f / df;
                    r -= step;
                    if step.abs() <= 1e-12 * r.abs().max(1e-6) {
                        if let Ok(fv_chk) = calc_fv(r, nper, pmt, pv, t) {
                            if (fv_chk - fv).abs() <= 1e-6 * scale {
                                return Ok(EvaluatedCellValue::Number(r));
                            }
                        }
                    }
                }
                let mut lo = -0.99_f64;
                let mut hi = 10.0_f64;
                if let (Ok(fv_lo), Ok(fv_hi)) =
                    (calc_fv(lo, nper, pmt, pv, t), calc_fv(hi, nper, pmt, pv, t))
                {
                    let mut f_lo = fv_lo - fv;
                    let f_hi = fv_hi - fv;
                    if f_lo.is_finite() && f_hi.is_finite() && f_lo * f_hi <= 0.0 {
                        for _ in 0..100 {
                            let mid = 0.5 * (lo + hi);
                            let Ok(fv_mid) = calc_fv(mid, nper, pmt, pv, t) else {
                                break;
                            };
                            let f_mid = fv_mid - fv;
                            if f_mid.abs() <= f_tol
                                || (hi - lo).abs() <= 1e-12 * mid.abs().max(1e-6)
                            {
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
            "SYD" => {
                if args.len() != 4 {
                    return Err(FormulaError::Value);
                }
                let cost = self.eval_arg_num(&args[0], ctx, depth)?;
                let salvage = self.eval_arg_num(&args[1], ctx, depth)?;
                let life = self.eval_arg_num(&args[2], ctx, depth)?;
                let per = self.eval_arg_num(&args[3], ctx, depth)?;
                if life <= 0.0 || per <= 0.0 || per > life {
                    return Err(FormulaError::Num);
                }
                Ok(EvaluatedCellValue::Number(
                    (cost - salvage) * (life - per + 1.0) * 2.0 / (life * (life + 1.0)),
                ))
            }
            "EFFECT" => {
                if args.len() != 2 {
                    return Err(FormulaError::Value);
                }
                let nominal = self.eval_arg_num(&args[0], ctx, depth)?;
                let npery = self.eval_arg_num(&args[1], ctx, depth)?.trunc();
                if nominal <= 0.0 || npery < 1.0 {
                    return Err(FormulaError::Num);
                }
                Ok(EvaluatedCellValue::Number(
                    (1.0 + nominal / npery).powf(npery) - 1.0,
                ))
            }
            "NOMINAL" => {
                if args.len() != 2 {
                    return Err(FormulaError::Value);
                }
                let effect = self.eval_arg_num(&args[0], ctx, depth)?;
                let npery = self.eval_arg_num(&args[1], ctx, depth)?.trunc();
                if effect <= 0.0 || npery < 1.0 {
                    return Err(FormulaError::Num);
                }
                Ok(EvaluatedCellValue::Number(
                    ((effect + 1.0).powf(1.0 / npery) - 1.0) * npery,
                ))
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
