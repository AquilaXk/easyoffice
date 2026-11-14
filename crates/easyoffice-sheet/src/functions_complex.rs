//! Complex-number (`COMPLEX`, `IM*`), Gaussian error (`ERF`, `ERFC`), step (`DELTA`, `GESTEP`),
//! standard normal (`PHI`, `GAUSS`, `NORM.S.DIST`, `NORMSDIST`, `Z.TEST`, `ZTEST`), and
//! financial schedule/rate (`FVSCHEDULE`, `INTRATE`) spreadsheet functions.
//!
//! Reference implementation: `FormulaJS` (`@formulajs/formulajs`, MIT License,
//! Copyright (c) Sutoiku, Inc., `src/engineering.js`, `src/statistical.js`, `src/financial.js`),
//! with W. J. Cody's rational Chebyshev approximation for `erf`/`erfc` and a zero-regex
//! byte-slice complex number parser.

use crate::coercion::{format_general_number, parse_strict_number, parse_value_text};
use crate::functions_date_fin::day_count_and_year_basis;
use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

#[inline]
fn check_arity(args: &[Expr], min: usize, max: usize) -> Result<(), FormulaError> {
    if !(min..=max).contains(&args.len()) {
        Err(FormulaError::Value)
    } else {
        Ok(())
    }
}

#[inline]
fn finite_num(x: f64) -> Result<EvaluatedCellValue, FormulaError> {
    if x.is_finite() {
        Ok(EvaluatedCellValue::Number(if x == 0.0 { 0.0 } else { x }))
    } else {
        Err(FormulaError::Num)
    }
}

pub(crate) use crate::functions_dist::{erf_cody, erfc_cody, std_normal_cdf, std_normal_pdf};

/// Parses a complex number string (`"3.5"`, `"-2"`, `"i"`, `"-i"`, `"+j"`, `"4i"`, `"-2.5j"`,
/// `"a+bi"`, `"a-bi"`, `"1.5e-2+3i"`) without regular expressions.
pub fn parse_complex(s: &str) -> Result<(f64, f64, char), FormulaError> {
    let (re, im, suffix_opt) = parse_complex_opt(s)?;
    Ok((re, im, suffix_opt.unwrap_or('i')))
}

fn parse_complex_opt(s: &str) -> Result<(f64, f64, Option<char>), FormulaError> {
    if s.len() > SheetEngine::MAX_CELL_TEXT_CHARS {
        return Err(FormulaError::Num);
    }
    let s = s.trim();
    if s.is_empty() {
        return Err(FormulaError::Num);
    }
    let bytes = s.as_bytes();
    let last = bytes[bytes.len() - 1];
    if last != b'i' && last != b'j' {
        if s.len() != bytes.len() || bytes.iter().any(u8::is_ascii_whitespace) {
            return Err(FormulaError::Num);
        }
        let re = parse_strict_number(s).ok_or(FormulaError::Num)?;
        return Ok((if re == 0.0 { 0.0 } else { re }, 0.0, None));
    }
    let suffix = last as char;
    let body = &s[..s.len() - 1];
    if body
        .bytes()
        .any(|b| b.is_ascii_whitespace() || matches!(b, b'i' | b'j' | b'I' | b'J'))
    {
        return Err(FormulaError::Num);
    }
    if body.is_empty() || body == "+" {
        return Ok((0.0, 1.0, Some(suffix)));
    }
    if body == "-" {
        return Ok((0.0, -1.0, Some(suffix)));
    }
    let b_slice = body.as_bytes();
    let mut split_idx: Option<usize> = None;
    for idx in 1..b_slice.len() {
        let ch = b_slice[idx];
        if (ch == b'+' || ch == b'-') && !matches!(b_slice[idx - 1], b'e' | b'E') {
            if split_idx.is_some() {
                return Err(FormulaError::Num);
            }
            split_idx = Some(idx);
        }
    }
    let Some(split) = split_idx else {
        let im = parse_strict_number(body).ok_or(FormulaError::Num)?;
        return Ok((0.0, if im == 0.0 { 0.0 } else { im }, Some(suffix)));
    };
    let re_part = &body[..split];
    let im_part = &body[split..];
    let re = parse_strict_number(re_part).ok_or(FormulaError::Num)?;
    let im = if im_part == "+" {
        1.0
    } else if im_part == "-" {
        -1.0
    } else {
        parse_strict_number(im_part).ok_or(FormulaError::Num)?
    };
    Ok((
        if re == 0.0 { 0.0 } else { re },
        if im == 0.0 { 0.0 } else { im },
        Some(suffix),
    ))
}

/// Formats `(re, im, suffix)` in standard spreadsheet complex notation (`a+bi`, `a-bi`, `bi`, `a`).
pub fn format_complex(re: f64, im: f64, suffix: char) -> String {
    let re = if re.abs() < 1e-15 { 0.0 } else { re };
    let im = if im.abs() < 1e-15 { 0.0 } else { im };
    if im == 0.0 {
        return format_general_number(re);
    }
    if re == 0.0 {
        if im == 1.0 {
            return suffix.to_string();
        }
        if im == -1.0 {
            return format!("-{suffix}");
        }
        let im_str = format_general_number(im);
        return format!("{im_str}{suffix}");
    }
    let re_str = format_general_number(re);
    if im == 1.0 {
        format!("{re_str}+{suffix}")
    } else if im == -1.0 {
        format!("{re_str}-{suffix}")
    } else if im > 0.0 {
        let im_str = format_general_number(im);
        format!("{re_str}+{im_str}{suffix}")
    } else {
        let abs_im_str = format_general_number(im.abs());
        format!("{re_str}-{abs_im_str}{suffix}")
    }
}

#[inline]
pub(crate) fn format_complex_res(
    re: f64,
    im: f64,
    suffix: char,
) -> Result<EvaluatedCellValue, FormulaError> {
    if !re.is_finite() || !im.is_finite() {
        return Err(FormulaError::Num);
    }
    Ok(EvaluatedCellValue::Text(format_complex(re, im, suffix)))
}

#[inline]
fn merge_suffix(a: Option<char>, b: Option<char>) -> Result<Option<char>, FormulaError> {
    match (a, b) {
        (Some(sa), Some(sb)) if sa != sb => Err(FormulaError::Num),
        (Some(s), _) | (_, Some(s)) => Ok(Some(s)),
        (None, None) => Ok(None),
    }
}

#[inline]
fn coerce_cell_to_complex(
    val: &EvaluatedCellValue,
) -> Result<Option<(f64, f64, Option<char>)>, FormulaError> {
    match val {
        EvaluatedCellValue::Error(e) => Err(*e),
        EvaluatedCellValue::Empty => Ok(None),
        EvaluatedCellValue::Boolean(_) => Err(FormulaError::Value),
        EvaluatedCellValue::Number(n) => {
            if !n.is_finite() {
                return Err(FormulaError::Num);
            }
            Ok(Some((if *n == 0.0 { 0.0 } else { *n }, 0.0, None)))
        }
        EvaluatedCellValue::Text(s) => parse_complex_opt(s).map(Some),
    }
}

/// A blank is 0, finite numbers pass (negative zero as zero), numeric text is parsed.
fn strict_num_value(val: EvaluatedCellValue) -> Result<f64, FormulaError> {
    match val {
        EvaluatedCellValue::Boolean(_) => Err(FormulaError::Value),
        EvaluatedCellValue::Empty => Ok(0.0),
        EvaluatedCellValue::Number(n) if n.is_finite() => Ok(if n == 0.0 { 0.0 } else { n }),
        EvaluatedCellValue::Number(_) => Err(FormulaError::Num),
        EvaluatedCellValue::Text(s) => parse_value_text(&s).ok_or(FormulaError::Value),
        EvaluatedCellValue::Error(e) => Err(e),
    }
}

impl SheetEngine {
    pub(crate) fn eval_strict_num_arg(
        &self,
        arg: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<f64, FormulaError> {
        match self.eval_arg_val(arg, ctx, depth)? {
            EvaluatedCellValue::Boolean(_) => Err(FormulaError::Value),
            other => strict_num_value(other),
        }
    }

    /// The number argument of a distribution or statistics function: like
    /// [`Self::eval_strict_num_arg`], but a logical value is 1 or 0 (PD-205). Logical values inside
    /// a range that an aggregate reads are skipped by the aggregate, not by this reader.
    pub(crate) fn eval_stat_num_arg(
        &self,
        arg: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<f64, FormulaError> {
        match self.eval_arg_val(arg, ctx, depth)? {
            EvaluatedCellValue::Boolean(b) => Ok(f64::from(u8::from(b))),
            other => strict_num_value(other),
        }
    }

    fn eval_complex_arg_opt(
        &self,
        arg: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<(f64, f64, Option<char>), FormulaError> {
        let val = self.eval_arg_val(arg, ctx, depth)?;
        Ok(coerce_cell_to_complex(&val)?.unwrap_or((0.0, 0.0, None)))
    }

    pub(crate) fn eval_complex_arg(
        &self,
        arg: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<(f64, f64, char), FormulaError> {
        let (re, im, s_opt) = self.eval_complex_arg_opt(arg, ctx, depth)?;
        Ok((re, im, s_opt.unwrap_or('i')))
    }

    pub(crate) fn eval_complex_stat_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        match name {
            "DELTA" => {
                check_arity(args, 1, 2)?;
                let n1 = self.eval_strict_num_arg(&args[0], ctx, depth)?;
                let n2 = match args.get(1) {
                    None | Some(Expr::Missing) => 0.0,
                    Some(a) => self.eval_strict_num_arg(a, ctx, depth)?,
                };
                Ok(EvaluatedCellValue::Number(if (n1 - n2).abs() == 0.0 {
                    1.0
                } else {
                    0.0
                }))
            }
            "GESTEP" => {
                check_arity(args, 1, 2)?;
                let n = self.eval_strict_num_arg(&args[0], ctx, depth)?;
                let step = match args.get(1) {
                    None | Some(Expr::Missing) => 0.0,
                    Some(a) => self.eval_strict_num_arg(a, ctx, depth)?,
                };
                Ok(EvaluatedCellValue::Number(if n >= step {
                    1.0
                } else {
                    0.0
                }))
            }
            "ERF" => {
                check_arity(args, 1, 2)?;
                let lo = self.eval_strict_num_arg(&args[0], ctx, depth)?;
                if let Some(arg1) = args.get(1) {
                    if !matches!(arg1, Expr::Missing) {
                        let hi = self.eval_strict_num_arg(arg1, ctx, depth)?;
                        return finite_num(erf_cody(hi) - erf_cody(lo));
                    }
                }
                finite_num(erf_cody(lo))
            }
            "ERF.PRECISE" => {
                check_arity(args, 1, 1)?;
                let x = self.eval_strict_num_arg(&args[0], ctx, depth)?;
                finite_num(erf_cody(x))
            }
            "ERFC" | "ERFC.PRECISE" => {
                check_arity(args, 1, 1)?;
                let x = self.eval_strict_num_arg(&args[0], ctx, depth)?;
                finite_num(erfc_cody(x))
            }
            "PHI" => {
                check_arity(args, 1, 1)?;
                let x = self.eval_stat_num_arg(&args[0], ctx, depth)?;
                finite_num(std_normal_pdf(x))
            }
            "GAUSS" => {
                check_arity(args, 1, 1)?;
                let z = self.eval_stat_num_arg(&args[0], ctx, depth)?;
                finite_num(0.5 * erf_cody(z * std::f64::consts::FRAC_1_SQRT_2))
            }
            "NORM.S.DIST" => {
                check_arity(args, 1, 2)?;
                let z = self.eval_stat_num_arg(&args[0], ctx, depth)?;
                let cumulative = match args.get(1) {
                    None | Some(Expr::Missing) => true,
                    Some(a) => self
                        .eval_arg_val(a, ctx, depth)?
                        .as_bool()
                        .ok_or(FormulaError::Value)?,
                };
                finite_num(if cumulative {
                    std_normal_cdf(z)
                } else {
                    std_normal_pdf(z)
                })
            }
            "NORMSDIST" => {
                check_arity(args, 1, 1)?;
                let z = self.eval_stat_num_arg(&args[0], ctx, depth)?;
                finite_num(std_normal_cdf(z))
            }
            "Z.TEST" | "ZTEST" => self.eval_z_test(args, ctx, depth),
            "FVSCHEDULE" => self.eval_fvschedule(args, ctx, depth),
            "INTRATE" => self.eval_intrate(args, ctx, depth),
            "COMPLEX" => self.eval_complex_ctor(args, ctx, depth),
            "IMREAL" => {
                check_arity(args, 1, 1)?;
                let (re, _, _) = self.eval_complex_arg(&args[0], ctx, depth)?;
                finite_num(re)
            }
            "IMAGINARY" => {
                check_arity(args, 1, 1)?;
                let (_, im, _) = self.eval_complex_arg(&args[0], ctx, depth)?;
                finite_num(im)
            }
            "IMABS" => {
                check_arity(args, 1, 1)?;
                let (re, im, _) = self.eval_complex_arg(&args[0], ctx, depth)?;
                finite_num(re.hypot(im))
            }
            "IMARGUMENT" => {
                check_arity(args, 1, 1)?;
                let (re, im, _) = self.eval_complex_arg(&args[0], ctx, depth)?;
                if re == 0.0 && im == 0.0 {
                    return Err(FormulaError::DivByZero);
                }
                finite_num(im.atan2(re))
            }
            "IMCONJUGATE" => {
                check_arity(args, 1, 1)?;
                let (re, im, suffix) = self.eval_complex_arg(&args[0], ctx, depth)?;
                format_complex_res(re, -im, suffix)
            }
            "IMSUM" => self.eval_imsum(args, ctx, depth),
            "IMSUB" => {
                check_arity(args, 2, 2)?;
                let (r1, i1, s1) = self.eval_complex_arg_opt(&args[0], ctx, depth)?;
                let (r2, i2, s2) = self.eval_complex_arg_opt(&args[1], ctx, depth)?;
                let suffix = merge_suffix(s1, s2)?.unwrap_or('i');
                format_complex_res(r1 - r2, i1 - i2, suffix)
            }
            "IMPRODUCT" => self.eval_improduct(args, ctx, depth),
            "IMDIV" => {
                check_arity(args, 2, 2)?;
                let (a, b, s1) = self.eval_complex_arg_opt(&args[0], ctx, depth)?;
                let (c, d, s2) = self.eval_complex_arg_opt(&args[1], ctx, depth)?;
                let suffix = merge_suffix(s1, s2)?.unwrap_or('i');
                if c == 0.0 && d == 0.0 {
                    return Err(FormulaError::Num);
                }
                let (re, im) = if c.abs() >= d.abs() {
                    let r = d / c;
                    let denom = c + d * r;
                    if denom == 0.0 {
                        return Err(FormulaError::Num);
                    }
                    ((a + b * r) / denom, (b - a * r) / denom)
                } else {
                    let r = c / d;
                    let denom = d + c * r;
                    if denom == 0.0 {
                        return Err(FormulaError::Num);
                    }
                    ((a * r + b) / denom, (b * r - a) / denom)
                };
                format_complex_res(re, im, suffix)
            }
            "IMPOWER" => {
                check_arity(args, 2, 2)?;
                let (re, im, suffix) = self.eval_complex_arg(&args[0], ctx, depth)?;
                let p = self.eval_strict_num_arg(&args[1], ctx, depth)?;
                let r = re.hypot(im);
                if r == 0.0 {
                    return if p <= 0.0 {
                        Err(FormulaError::Num)
                    } else {
                        Ok(EvaluatedCellValue::Text("0".to_string()))
                    };
                }
                let rp = r.powf(p);
                let angle = p * im.atan2(re);
                format_complex_res(rp * angle.cos(), rp * angle.sin(), suffix)
            }
            "IMSQRT" => {
                check_arity(args, 1, 1)?;
                let (re, im, suffix) = self.eval_complex_arg(&args[0], ctx, depth)?;
                if re == 0.0 && im == 0.0 {
                    return Ok(EvaluatedCellValue::Text("0".to_string()));
                }
                let r = re.hypot(im);
                let s_re = (0.5 * (r + re)).max(0.0).sqrt();
                let s_im_abs = (0.5 * (r - re)).max(0.0).sqrt();
                let s_im = if im < 0.0 { -s_im_abs } else { s_im_abs };
                format_complex_res(s_re, s_im, suffix)
            }
            "IMEXP" => {
                check_arity(args, 1, 1)?;
                let (a, b, suffix) = self.eval_complex_arg(&args[0], ctx, depth)?;
                let ea = a.exp();
                format_complex_res(ea * b.cos(), ea * b.sin(), suffix)
            }
            "IMLN" | "IMLOG10" | "IMLOG2" => {
                check_arity(args, 1, 1)?;
                let (a, b, suffix) = self.eval_complex_arg(&args[0], ctx, depth)?;
                let r = a.hypot(b);
                if r == 0.0 {
                    return Err(FormulaError::Num);
                }
                let ln_r = r.ln();
                let theta = b.atan2(a);
                let scale = match name {
                    "IMLOG10" => 1.0 / std::f64::consts::LN_10,
                    "IMLOG2" => 1.0 / std::f64::consts::LN_2,
                    _ => 1.0,
                };
                format_complex_res(ln_r * scale, theta * scale, suffix)
            }
            "IMSIN" => {
                check_arity(args, 1, 1)?;
                let (a, b, suffix) = self.eval_complex_arg(&args[0], ctx, depth)?;
                format_complex_res(a.sin() * b.cosh(), a.cos() * b.sinh(), suffix)
            }
            "IMCOS" => {
                check_arity(args, 1, 1)?;
                let (a, b, suffix) = self.eval_complex_arg(&args[0], ctx, depth)?;
                format_complex_res(a.cos() * b.cosh(), -a.sin() * b.sinh(), suffix)
            }
            _ => Err(FormulaError::Name),
        }
    }

    fn eval_z_test(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 2, 3)?;
        let mut count = 0usize;
        let mut mean = 0.0f64;
        let mut m2 = 0.0f64;
        self.visit_args_streaming(std::slice::from_ref(&args[0]), ctx, depth, |v| {
            if let EvaluatedCellValue::Number(x) = v {
                count += 1;
                let delta = x - mean;
                mean += delta / (count as f64);
                m2 += delta * (x - mean);
            }
            Ok(())
        })?;
        let x = self.eval_stat_num_arg(&args[1], ctx, depth)?;
        let sigma_opt = match args.get(2) {
            None | Some(Expr::Missing) => None,
            Some(a) => Some(self.eval_stat_num_arg(a, ctx, depth)?),
        };
        if count == 0 {
            return Err(FormulaError::Na);
        }
        let sd = match sigma_opt {
            Some(s) => {
                if s == 0.0 {
                    return Err(FormulaError::DivByZero);
                }
                if s < 0.0 {
                    return Err(FormulaError::Num);
                }
                s
            }
            None => {
                if count < 2 {
                    return Err(FormulaError::DivByZero);
                }
                let s = (m2 / ((count - 1) as f64)).sqrt();
                if s == 0.0 {
                    return Err(FormulaError::DivByZero);
                }
                s
            }
        };
        let z = (mean - x) / (sd / (count as f64).sqrt());
        finite_num(0.5 * erfc_cody(z * std::f64::consts::FRAC_1_SQRT_2))
    }

    fn eval_fvschedule(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 2, 2)?;
        let mut fv = self.eval_strict_num_arg(&args[0], ctx, depth)?;
        if let Some(spec) = self.extract_streaming_range_ctx(&args[1], ctx, depth)? {
            let (ref sheet_opt, min_r, min_c, rows, cols, transposed) = spec;
            let total = rows.saturating_mul(cols);
            let target_sheet = self.resolve_sheet_ref(sheet_opt.as_deref(), ctx);
            let max_r = min_r + (rows as u32).saturating_sub(1);
            let max_c = min_c + (cols as u32).saturating_sub(1);
            let pop_count = target_sheet.map_or(total, |ts| {
                ts.cell_index().count_in_range(min_r, max_r, min_c, max_c)
            });
            let mut apply = |v: EvaluatedCellValue| -> Result<(), FormulaError> {
                match v {
                    EvaluatedCellValue::Number(r) => fv *= 1.0 + r,
                    EvaluatedCellValue::Empty => {}
                    EvaluatedCellValue::Text(_) | EvaluatedCellValue::Boolean(_) => {
                        return Err(FormulaError::Value);
                    }
                    EvaluatedCellValue::Error(e) => return Err(e),
                }
                Ok(())
            };
            if !transposed
                && (pop_count == 0 || (total > 64 && total > pop_count.saturating_mul(2)))
            {
                if pop_count > 0 {
                    for coord in target_sheet
                        .unwrap()
                        .cell_index()
                        .range_coords(min_r, max_r, min_c, max_c)
                    {
                        apply(self.eval_sheet_coord(sheet_opt.as_deref(), coord, ctx, depth))?;
                    }
                }
            } else {
                for dr in 0..rows {
                    for dc in 0..cols {
                        apply(self.eval_streaming_cell(&spec, dr, dc, ctx, depth)?)?;
                    }
                }
            }
        } else {
            let r = self.eval_strict_num_arg(&args[1], ctx, depth)?;
            fv *= 1.0 + r;
        }
        finite_num(fv)
    }

    fn eval_intrate(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 4, 5)?;
        let settlement = self.eval_strict_num_arg(&args[0], ctx, depth)?.trunc() as i64;
        let maturity = self.eval_strict_num_arg(&args[1], ctx, depth)?.trunc() as i64;
        let investment = self.eval_strict_num_arg(&args[2], ctx, depth)?;
        let redemption = self.eval_strict_num_arg(&args[3], ctx, depth)?;
        let basis = match args.get(4) {
            None | Some(Expr::Missing) => 0,
            Some(a) => self.eval_strict_num_arg(a, ctx, depth)?.trunc() as i64,
        };
        if settlement <= 0
            || maturity <= 0
            || settlement >= maturity
            || investment <= 0.0
            || redemption <= 0.0
            || !(0..=4).contains(&basis)
        {
            return Err(FormulaError::Num);
        }
        let (days, year_basis) = day_count_and_year_basis(settlement, maturity, basis);
        if days <= 0.0 {
            return Err(FormulaError::Num);
        }
        finite_num(((redemption - investment) / investment) * (year_basis / days))
    }

    fn eval_complex_ctor(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        check_arity(args, 2, 3)?;
        let re = self.eval_strict_num_arg(&args[0], ctx, depth)?;
        let im = self.eval_strict_num_arg(&args[1], ctx, depth)?;
        let suffix = match args.get(2) {
            None | Some(Expr::Missing) => 'i',
            Some(a) => match self.eval_arg_val(a, ctx, depth)? {
                EvaluatedCellValue::Empty => 'i',
                EvaluatedCellValue::Text(s) if s == "i" => 'i',
                EvaluatedCellValue::Text(s) if s == "j" => 'j',
                _ => return Err(FormulaError::Value),
            },
        };
        format_complex_res(re, im, suffix)
    }

    fn eval_imsum(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if args.is_empty() {
            return Err(FormulaError::Value);
        }
        let mut sum_re = 0.0f64;
        let mut sum_im = 0.0f64;
        let mut suffix_opt: Option<char> = None;
        self.visit_args_streaming(args, ctx, depth, |v| {
            if let Some((re, im, s_opt)) = coerce_cell_to_complex(&v)? {
                suffix_opt = merge_suffix(suffix_opt, s_opt)?;
                sum_re += re;
                sum_im += im;
            }
            Ok(())
        })?;
        format_complex_res(sum_re, sum_im, suffix_opt.unwrap_or('i'))
    }

    fn eval_improduct(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if args.is_empty() {
            return Err(FormulaError::Value);
        }
        let mut prod_re = 1.0f64;
        let mut prod_im = 0.0f64;
        let mut suffix_opt: Option<char> = None;
        let mut has_any = false;
        self.visit_args_streaming(args, ctx, depth, |v| {
            if let Some((re, im, s_opt)) = coerce_cell_to_complex(&v)? {
                suffix_opt = merge_suffix(suffix_opt, s_opt)?;
                let next_re = prod_re * re - prod_im * im;
                let next_im = prod_re * im + prod_im * re;
                prod_re = next_re;
                prod_im = next_im;
                has_any = true;
            }
            Ok(())
        })?;
        if !has_any {
            return Ok(EvaluatedCellValue::Text("0".to_string()));
        }
        format_complex_res(prod_re, prod_im, suffix_opt.unwrap_or('i'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::functions_dist::FRAC_1_SQRT_2PI;

    #[test]
    fn step_error_and_normal_distribution_functions_match_formulajs() {
        let mut s = SheetEngine::new("ComplexStatTest");
        assert_eq!(
            s.evaluate_formula_str("=DELTA(5, 4)"),
            EvaluatedCellValue::Number(0.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=DELTA(5, 5)"),
            EvaluatedCellValue::Number(1.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=DELTA(0)"),
            EvaluatedCellValue::Number(1.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=GESTEP(5, 4)"),
            EvaluatedCellValue::Number(1.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=GESTEP(-1)"),
            EvaluatedCellValue::Number(0.0)
        );

        let EvaluatedCellValue::Number(erf1) = s.evaluate_formula_str("=ERF(1)") else {
            panic!("expected ERF number");
        };
        assert!((erf1 - 0.842_700_792_949_714_9).abs() < 1e-12);
        let EvaluatedCellValue::Number(erf_span) = s.evaluate_formula_str("=ERF(0, 1)") else {
            panic!("expected ERF span number");
        };
        assert!((erf_span - erf1).abs() < 1e-15);
        assert_eq!(
            s.evaluate_formula_str("=ERF.PRECISE(0)"),
            EvaluatedCellValue::Number(0.0)
        );
        let EvaluatedCellValue::Number(erfc1) = s.evaluate_formula_str("=ERFC(1)") else {
            panic!("expected ERFC number");
        };
        assert!((erfc1 - 0.157_299_207_050_285_13).abs() < 1e-12);
        assert_eq!(
            s.evaluate_formula_str("=ERFC.PRECISE(0)"),
            EvaluatedCellValue::Number(1.0)
        );

        let EvaluatedCellValue::Number(phi0) = s.evaluate_formula_str("=PHI(0)") else {
            panic!("expected PHI number");
        };
        assert!((phi0 - FRAC_1_SQRT_2PI).abs() < 1e-15);
        let EvaluatedCellValue::Number(gauss2) = s.evaluate_formula_str("=GAUSS(2)") else {
            panic!("expected GAUSS number");
        };
        assert!((gauss2 - 0.477_249_868_051_820_8).abs() < 1e-12);
        assert_eq!(
            s.evaluate_formula_str("=NORM.S.DIST(0, TRUE)"),
            EvaluatedCellValue::Number(0.5)
        );
        assert_eq!(
            s.evaluate_formula_str("=NORMSDIST(0)"),
            EvaluatedCellValue::Number(0.5)
        );
        let EvaluatedCellValue::Number(nsd_pdf) = s.evaluate_formula_str("=NORM.S.DIST(0, FALSE)")
        else {
            panic!("expected NORM.S.DIST PDF");
        };
        assert!((nsd_pdf - FRAC_1_SQRT_2PI).abs() < 1e-15);

        for (idx, v) in [3, 6, 7, 8, 6, 5, 4, 2, 1, 9].into_iter().enumerate() {
            s.set_cell(&format!("A{}", idx + 1), v.to_string()).unwrap();
        }
        let EvaluatedCellValue::Number(zt) = s.evaluate_formula_str("=Z.TEST(A1:A10, 4)") else {
            panic!("expected Z.TEST number");
        };
        assert!((zt - 0.090_574_196_851_363_8).abs() < 1e-9);
        let EvaluatedCellValue::Number(zt_sig) = s.evaluate_formula_str("=ZTEST(A1:A10, 4, 2)")
        else {
            panic!("expected ZTEST number");
        };
        assert!((zt_sig - 0.040_995_160_500_28).abs() < 1e-9);
    }

    #[test]
    fn fvschedule_intrate_and_all_18_complex_functions_work() {
        let mut s = SheetEngine::new("ComplexAndFinTest");
        s.set_cell("B1", "0.09").unwrap();
        s.set_cell("B2", "0.11").unwrap();
        s.set_cell("B3", "0.10").unwrap();
        let EvaluatedCellValue::Number(fvs) = s.evaluate_formula_str("=FVSCHEDULE(1, B1:B4)")
        else {
            panic!("expected FVSCHEDULE number");
        };
        assert!((fvs - 1.33089).abs() < 1e-10);

        let EvaluatedCellValue::Number(ir0) = s.evaluate_formula_str(
            "=INTRATE(DATE(2008,2,15), DATE(2008,5,15), 1000000, 1014420, 2)",
        ) else {
            panic!("expected INTRATE number");
        };
        assert!((ir0 - 0.05768).abs() < 1e-8);

        assert_eq!(
            s.evaluate_formula_str("=COMPLEX(3, 4)"),
            EvaluatedCellValue::Text("3+4i".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=COMPLEX(0, -1, \"j\")"),
            EvaluatedCellValue::Text("-j".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=IMREAL(\"1.5e-2-3i\")"),
            EvaluatedCellValue::Number(0.015)
        );
        assert_eq!(
            s.evaluate_formula_str("=IMAGINARY(\"-i\")"),
            EvaluatedCellValue::Number(-1.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=IMABS(\"5+12i\")"),
            EvaluatedCellValue::Number(13.0)
        );
        assert_eq!(
            s.evaluate_formula_str("=IMCONJUGATE(\"3+4i\")"),
            EvaluatedCellValue::Text("3-4i".to_string())
        );
        s.set_cell("C1", "3+4i").unwrap();
        s.set_cell("C2", "5-3i").unwrap();
        assert_eq!(
            s.evaluate_formula_str("=IMSUM(C1:C2, \"1+2i\")"),
            EvaluatedCellValue::Text("9+3i".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=IMSUB(\"13+4i\", \"5+3i\")"),
            EvaluatedCellValue::Text("8+i".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=IMPRODUCT(\"3+4i\", \"5-3i\")"),
            EvaluatedCellValue::Text("27+11i".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=IMDIV(\"-238+240i\", \"10+24i\")"),
            EvaluatedCellValue::Text("5+12i".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=IMDIV(\"1e300+1e300i\", \"1e300+1e300i\")"),
            EvaluatedCellValue::Text("1".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=IMDIV(\"1e-300+1e-300i\", \"1e-300+1e-300i\")"),
            EvaluatedCellValue::Text("1".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=IMSQRT(\"3+4i\")"),
            EvaluatedCellValue::Text("2+i".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=IMPOWER(\"2+i\", 2)"),
            EvaluatedCellValue::Text("3+4i".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=IMARGUMENT(\"1+i\")"),
            EvaluatedCellValue::Number(std::f64::consts::FRAC_PI_4)
        );
        assert_eq!(
            s.evaluate_formula_str("=IMLN(\"1\")"),
            EvaluatedCellValue::Text("0".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=IMLOG10(\"100\")"),
            EvaluatedCellValue::Text("2".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=IMLOG2(\"8\")"),
            EvaluatedCellValue::Text("3".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=IMEXP(\"0\")"),
            EvaluatedCellValue::Text("1".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=IMSIN(\"0\")"),
            EvaluatedCellValue::Text("0".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=IMCOS(\"0\")"),
            EvaluatedCellValue::Text("1".to_string())
        );
        assert_eq!(
            s.evaluate_formula_str("=IMSUM(\"3+4i\", \"1+2j\")"),
            EvaluatedCellValue::Error(FormulaError::Num)
        );
    }
}
