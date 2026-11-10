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

const FRAC_1_SQRT_2PI: f64 = 0.398_942_280_401_432_7;
const FRAC_1_SQRT_PI: f64 = 0.564_189_583_547_756_3;

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

/// W. J. Cody (1969) rational Chebyshev approximation for `erf(x)`, accurate to `< 1e-15`.
#[allow(clippy::excessive_precision)]
fn erf_cody(x: f64) -> f64 {
    if x == 0.0 {
        return 0.0;
    }
    let z = x.abs();
    if z <= 0.46875 {
        const P: [f64; 5] = [
            3.161_123_743_870_565_6e00,
            1.138_641_541_510_501_6e02,
            3.774_852_376_853_02e02,
            3.209_377_589_138_469_5e03,
            1.857_777_061_846_031_5e-1,
        ];
        const Q: [f64; 4] = [
            2.360_129_095_234_412_1e01,
            2.440_246_379_344_441_7e02,
            1.282_616_526_077_372_3e03,
            2.844_236_833_439_171e03,
        ];
        let y = z * z;
        let num = (((P[4] * y + P[0]) * y + P[1]) * y + P[2]) * y + P[3];
        let den = (((y + Q[0]) * y + Q[1]) * y + Q[2]) * y + Q[3];
        x * (num / den)
    } else {
        let c = erfc_cody_pos(z);
        let res = 1.0 - c;
        if x < 0.0 {
            -res
        } else {
            res
        }
    }
}

/// W. J. Cody (1969) rational Chebyshev approximation for `erfc(z)` where `z >= 0`.
#[allow(clippy::excessive_precision)]
fn erfc_cody_pos(z: f64) -> f64 {
    if z > 27.0 {
        return 0.0;
    }
    if z <= 0.46875 {
        return 1.0 - erf_cody(z);
    }
    let z_sq = (z * 16.0).trunc() / 16.0;
    let del = (z - z_sq) * (z + z_sq);
    let exp_term = (-z_sq * z_sq).exp() * (-del).exp();
    if z <= 4.0 {
        const C: [f64; 9] = [
            5.641_884_969_886_701e-1,
            8.883_149_794_388_376e00,
            6.611_919_063_714_163e01,
            2.986_351_381_974_001e02,
            8.819_522_212_417_691e02,
            1.712_047_612_634_070_6e03,
            2.051_078_377_826_071_5e03,
            1.230_339_354_797_997_2e03,
            2.153_115_354_744_038_5e-8,
        ];
        const D: [f64; 8] = [
            1.574_492_611_070_983_5e01,
            1.176_939_508_913_125e02,
            5.371_811_018_620_099e02,
            1.621_389_574_566_690_2e03,
            3.290_799_235_733_459_6e03,
            4.362_619_090_143_247e03,
            3.439_367_674_143_721_6e03,
            1.230_339_354_803_749_5e03,
        ];
        let num = (((((((C[8] * z + C[0]) * z + C[1]) * z + C[2]) * z + C[3]) * z + C[4]) * z
            + C[5])
            * z
            + C[6])
            * z
            + C[7];
        let den = (((((((z + D[0]) * z + D[1]) * z + D[2]) * z + D[3]) * z + D[4]) * z + D[5]) * z
            + D[6])
            * z
            + D[7];
        exp_term * (num / den)
    } else {
        const P: [f64; 6] = [
            3.053_266_349_612_323_4e-1,
            3.603_448_999_498_044_4e-1,
            1.257_817_261_112_292_5e-1,
            1.608_378_514_874_228e-2,
            6.587_491_615_298_378e-4,
            1.631_538_713_730_209_7e-2,
        ];
        const Q: [f64; 5] = [
            2.568_520_192_289_822_4,
            1.872_952_849_923_460_4e00,
            5.279_051_029_514_284e-1,
            6.051_834_131_244_132e-2,
            2.335_204_976_268_691_8e-3,
        ];
        let u = 1.0 / (z * z);
        let num = u * (((((P[5] * u + P[0]) * u + P[1]) * u + P[2]) * u + P[3]) * u + P[4]);
        let den = ((((u + Q[0]) * u + Q[1]) * u + Q[2]) * u + Q[3]) * u + Q[4];
        exp_term * (FRAC_1_SQRT_PI - num / den) / z
    }
}

#[inline]
fn erfc_cody(x: f64) -> f64 {
    if x == 0.0 {
        1.0
    } else if x > 0.0 {
        erfc_cody_pos(x)
    } else if x < -0.46875 {
        2.0 - erfc_cody_pos(-x)
    } else {
        1.0 - erf_cody(x)
    }
}

#[inline]
fn std_normal_pdf(x: f64) -> f64 {
    FRAC_1_SQRT_2PI * (-0.5 * x * x).exp()
}

#[inline]
fn std_normal_cdf(z: f64) -> f64 {
    0.5 * erfc_cody(-z * std::f64::consts::FRAC_1_SQRT_2)
}

/// Parses a complex number string (`"3.5"`, `"-2"`, `"i"`, `"-i"`, `"+j"`, `"4i"`, `"-2.5j"`,
/// `"a+bi"`, `"a-bi"`, `"1.5e-2+3i"`) without regular expressions.
pub fn parse_complex(s: &str) -> Result<(f64, f64, char), FormulaError> {
    let (re, im, suffix_opt) = parse_complex_opt(s)?;
    Ok((re, im, suffix_opt.unwrap_or('i')))
}

fn parse_complex_opt(s: &str) -> Result<(f64, f64, Option<char>), FormulaError> {
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
fn format_complex_res(re: f64, im: f64, suffix: char) -> Result<EvaluatedCellValue, FormulaError> {
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

impl SheetEngine {
    fn eval_strict_num_arg(
        &self,
        arg: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<f64, FormulaError> {
        let val = self.eval_arg_val(arg, ctx, depth)?;
        match val {
            EvaluatedCellValue::Boolean(_) => Err(FormulaError::Value),
            EvaluatedCellValue::Empty => Ok(0.0),
            EvaluatedCellValue::Number(n) => {
                if n.is_finite() {
                    Ok(if n == 0.0 { 0.0 } else { n })
                } else {
                    Err(FormulaError::Num)
                }
            }
            EvaluatedCellValue::Text(s) => parse_value_text(&s).ok_or(FormulaError::Value),
            EvaluatedCellValue::Error(e) => Err(e),
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

    fn eval_complex_arg(
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
                let x = self.eval_strict_num_arg(&args[0], ctx, depth)?;
                finite_num(std_normal_pdf(x))
            }
            "GAUSS" => {
                check_arity(args, 1, 1)?;
                let z = self.eval_strict_num_arg(&args[0], ctx, depth)?;
                finite_num(0.5 * erf_cody(z * std::f64::consts::FRAC_1_SQRT_2))
            }
            "NORM.S.DIST" => {
                check_arity(args, 1, 2)?;
                let z = self.eval_strict_num_arg(&args[0], ctx, depth)?;
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
                let z = self.eval_strict_num_arg(&args[0], ctx, depth)?;
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
                let denom = c * c + d * d;
                if denom == 0.0 {
                    return Err(FormulaError::Num);
                }
                format_complex_res((a * c + b * d) / denom, (b * c - a * d) / denom, suffix)
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
        let x = self.eval_strict_num_arg(&args[1], ctx, depth)?;
        let sigma_opt = match args.get(2) {
            None | Some(Expr::Missing) => None,
            Some(a) => Some(self.eval_strict_num_arg(a, ctx, depth)?),
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
            let (_, _, _, rows, cols, _) = spec;
            for dr in 0..rows {
                for dc in 0..cols {
                    match self.eval_streaming_cell(&spec, dr, dc, ctx, depth)? {
                        EvaluatedCellValue::Number(r) => fv *= 1.0 + r,
                        EvaluatedCellValue::Empty => {}
                        EvaluatedCellValue::Text(_) | EvaluatedCellValue::Boolean(_) => {
                            return Err(FormulaError::Value);
                        }
                        EvaluatedCellValue::Error(e) => return Err(e),
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
