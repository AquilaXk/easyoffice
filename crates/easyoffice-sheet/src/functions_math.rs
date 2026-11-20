//! 참조 구현: IronCalc `base/src/functions/math_and_trigonometry/mathematical.rs` (commit
//! f93662006b8fb1c401dc870bbda26c5d60ac6037, `MIT OR Apache-2.0`, Copyright 2023 EqualTo GmbH,
//! Copyright 2023-present IronCalc contributors, Copyright 2026-present IronCalc GmbH): `Rounding`,
//! `round_to_digits`, `round_result` (lines 1310-1362) for ROUND, ROUNDUP, ROUNDDOWN and TRUNC;
//! `fn_floor` and `fn_ceiling` (lines 702-767) as `ceiling_floor`; `fn_mround` (lines 930-978).
use crate::sig_digits::{to_precision, EXCEL_PRECISION};
use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

type CellResult = Result<EvaluatedCellValue, FormulaError>;

const MAX_FACTORIAL_ARG: f64 = 170.0;
const LOG_BASE_EPSILON: f64 = 1e-15;

fn check_math_arity(args: &[Expr], min: usize, max: usize) -> Result<(), FormulaError> {
    if !(min..=max).contains(&args.len()) {
        Err(FormulaError::Value)
    } else {
        Ok(())
    }
}

fn number_result(v: f64) -> CellResult {
    Ok(EvaluatedCellValue::Number(v))
}

/// Normalizes negative zero to positive zero.
#[inline]
fn clean_zero(v: f64) -> f64 {
    if v == 0.0 {
        0.0
    } else {
        v
    }
}

/// How ROUND, ROUNDUP, ROUNDDOWN and TRUNC round.
#[derive(Clone, Copy)]
enum Rounding {
    Nearest,
    AwayFromZero,
    TowardZero,
}

/// `value` (already at 15 significant digits) rounded at `digits` decimals (an integer, negative
/// for tens, hundreds, ...). The scaled value is taken at 15 significant digits before rounding,
/// so binary noise does not cross a rounding boundary: 1.005 * 100 is 100.49999999999999 in binary
/// and ROUND(1.005, 2) is 1.01.
///
/// Returns `None` when the result is not a finite number (ROUNDUP of a nonzero number to a
/// multiple of 10^309 or more).
fn round_to_digits(value: f64, digits: f64, mode: Rounding) -> Option<f64> {
    let scale = 10f64.powf(digits);
    if scale == 0.0 {
        // Every finite number is less than half a unit at this position.
        return match mode {
            Rounding::AwayFromZero if value != 0.0 => None,
            _ => Some(0.0),
        };
    }
    let scaled = value * scale;
    if !scaled.is_finite() {
        // The position is past the last digit a number this large has.
        return Some(value);
    }
    let scaled = to_precision(scaled, EXCEL_PRECISION);
    let rounded = match mode {
        Rounding::Nearest => scaled.round(),
        Rounding::AwayFromZero if value > 0.0 => scaled.ceil(),
        Rounding::AwayFromZero => scaled.floor(),
        Rounding::TowardZero => scaled.trunc(),
    };
    let result = rounded / scale;
    result.is_finite().then_some(result)
}

fn round_result(value: f64, digits: f64, mode: Rounding) -> Result<f64, FormulaError> {
    round_to_digits(value, digits, mode).ok_or(FormulaError::Num)
}

/// CEILING / FLOOR core with significance `sig`; the ratio is taken at 15 significant digits.
fn ceiling_floor(is_ceiling: bool, num: f64, sig: f64) -> CellResult {
    if sig == 0.0 {
        return if !is_ceiling && num != 0.0 {
            Err(FormulaError::DivByZero)
        } else {
            number_result(0.0)
        };
    }
    if num > 0.0 && sig < 0.0 {
        return Err(FormulaError::Num);
    }
    let ratio = to_precision(num / sig, EXCEL_PRECISION);
    number_result(if is_ceiling {
        ratio.ceil() * sig
    } else {
        ratio.floor() * sig
    })
}

fn average_a_value(
    sum: &mut f64,
    count: &mut usize,
    v: EvaluatedCellValue,
) -> Result<(), FormulaError> {
    match v {
        EvaluatedCellValue::Empty => {}
        EvaluatedCellValue::Number(n) => {
            *sum += n;
            *count += 1;
        }
        EvaluatedCellValue::Boolean(b) => {
            *sum += if b { 1.0 } else { 0.0 };
            *count += 1;
        }
        EvaluatedCellValue::Text(_) => *count += 1,
        EvaluatedCellValue::Error(e) => return Err(e),
    }
    Ok(())
}

fn gcd_u64(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

fn lcm_u64(a: u64, b: u64) -> Option<u64> {
    if a == 0 || b == 0 {
        Some(0)
    } else {
        (a / gcd_u64(a, b)).checked_mul(b)
    }
}

const ROMAN_TABLE: [(u32, &str); 13] = [
    (1000, "M"),
    (900, "CM"),
    (500, "D"),
    (400, "CD"),
    (100, "C"),
    (90, "XC"),
    (50, "L"),
    (40, "XL"),
    (10, "X"),
    (9, "IX"),
    (5, "V"),
    (4, "IV"),
    (1, "I"),
];

fn roman_value(b: u8) -> Option<i64> {
    match b.to_ascii_uppercase() {
        b'I' => Some(1),
        b'V' => Some(5),
        b'X' => Some(10),
        b'L' => Some(50),
        b'C' => Some(100),
        b'D' => Some(500),
        b'M' => Some(1000),
        _ => None,
    }
}

impl SheetEngine {
    /// Math, trigonometry and scalar-conversion functions.
    pub(crate) fn eval_math_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        match name {
            "ABS" | "SIGN" | "INT" | "SQRT" | "FACT" | "EVEN" | "ODD" | "FACTDOUBLE" | "SQRTPI"
            | "DEGREES" | "RADIANS" => self.math_unary(name, args, ctx, depth),
            "ROUND" | "ROUNDUP" | "ROUNDDOWN" | "TRUNC" => self.math_round(name, args, ctx, depth),
            "MROUND" => self.math_mround(args, ctx, depth),
            "RANDBETWEEN" => self.math_randbetween(args, ctx, depth),
            "CEILING" | "FLOOR" => self.math_ceiling_floor(name == "CEILING", args, ctx, depth),
            "MOD" | "QUOTIENT" => {
                check_math_arity(args, 2, 2)?;
                let num = self.eval_arg_num(&args[0], ctx, depth)?;
                let div = self.eval_arg_num(&args[1], ctx, depth)?;
                if div == 0.0 {
                    return Err(FormulaError::DivByZero);
                }
                number_result(if name == "MOD" {
                    num - div * (num / div).floor()
                } else {
                    (num / div).trunc()
                })
            }
            "POWER" => {
                check_math_arity(args, 2, 2)?;
                let res = self
                    .eval_arg_num(&args[0], ctx, depth)?
                    .powf(self.eval_arg_num(&args[1], ctx, depth)?);
                if !res.is_finite() {
                    return Err(FormulaError::Num);
                }
                number_result(res)
            }
            "EXP" | "LN" | "LOG10" | "SIN" | "COS" | "TAN" | "ASIN" | "ACOS" | "ATAN" | "SINH"
            | "COSH" | "TANH" => self.math_transcendental(name, args, ctx, depth),
            "ATAN2" => {
                check_math_arity(args, 2, 2)?;
                let x = self.eval_arg_num(&args[0], ctx, depth)?;
                let y = self.eval_arg_num(&args[1], ctx, depth)?;
                if x == 0.0 && y == 0.0 {
                    return Err(FormulaError::DivByZero);
                }
                number_result(y.atan2(x))
            }
            "LOG" => self.math_log(args, ctx, depth),
            "PI" | "RAND" => {
                check_math_arity(args, 0, 0)?;
                number_result(if name == "PI" {
                    std::f64::consts::PI
                } else {
                    ctx.next_rand_f64()
                })
            }
            "NA" => {
                check_math_arity(args, 0, 0)?;
                Err(FormulaError::Na)
            }
            "N" => self.math_n(args, ctx, depth),
            "T" => {
                check_math_arity(args, 1, 1)?;
                match self.eval_expr_ctx(&args[0], ctx, depth) {
                    EvaluatedCellValue::Text(s) => Ok(EvaluatedCellValue::Text(s)),
                    EvaluatedCellValue::Error(e) => Err(e),
                    _ => Ok(EvaluatedCellValue::Text(String::new())),
                }
            }
            "AVERAGEA" => self.math_average_a(args, ctx, depth),
            "SUMSQ" => self.math_sumsq(args, ctx, depth),
            "GCD" | "LCM" => self.math_gcd_lcm(name == "LCM", args, ctx, depth),
            "COMBIN" | "COMBINA" | "PERMUT" | "PERMUTATIONA" => {
                self.math_combin_permut(name, args, ctx, depth)
            }
            "BASE" => self.math_base(args, ctx, depth),
            "DECIMAL" => self.math_decimal(args, ctx, depth),
            "ROMAN" => self.math_roman(args, ctx, depth),
            "ARABIC" => self.math_arabic(args, ctx, depth),
            _ => Err(FormulaError::Name),
        }
    }

    fn math_unary(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        check_math_arity(args, 1, 1)?;
        let n = self.eval_arg_num(&args[0], ctx, depth)?;
        match name {
            "ABS" => number_result(n.abs()),
            "INT" => number_result(n.floor()),
            "SIGN" => number_result(if n > 0.0 {
                1.0
            } else if n < 0.0 {
                -1.0
            } else {
                0.0
            }),
            "SQRT" if n < 0.0 => Err(FormulaError::Num),
            "SQRT" => number_result(n.sqrt()),
            "SQRTPI" if n < 0.0 => Err(FormulaError::Num),
            "SQRTPI" => number_result((n * std::f64::consts::PI).sqrt()),
            "DEGREES" => number_result(n.to_degrees()),
            "RADIANS" => number_result(n.to_radians()),
            "EVEN" => {
                if n == 0.0 {
                    return number_result(0.0);
                }
                let c = n.abs().ceil();
                let even = if (c as i64) % 2 == 0 { c } else { c + 1.0 };
                number_result(clean_zero(even.copysign(n)))
            }
            "ODD" => {
                if n == 0.0 {
                    return number_result(1.0);
                }
                let c = n.abs().ceil();
                let odd = if (c as i64) % 2 != 0 { c } else { c + 1.0 };
                number_result(odd.copysign(n))
            }
            "FACTDOUBLE" => {
                let n = n.trunc();
                if !(-1.0..=300.0).contains(&n) {
                    return Err(FormulaError::Num);
                }
                let mut k = n.max(0.0) as u32;
                let mut acc = 1.0f64;
                while k >= 2 {
                    acc *= k as f64;
                    k -= 2;
                }
                if !acc.is_finite() {
                    return Err(FormulaError::Num);
                }
                number_result(acc)
            }
            _ => {
                let n = n.trunc();
                if !(0.0..=MAX_FACTORIAL_ARG).contains(&n) {
                    return Err(FormulaError::Num);
                }
                number_result((2..=(n as u32)).fold(1.0f64, |acc, i| acc * i as f64))
            }
        }
    }

    fn math_round(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        if args.is_empty() || args.len() > 2 || (name != "TRUNC" && args.len() != 2) {
            return Err(FormulaError::Value);
        }
        let num = self.eval_arg_num(&args[0], ctx, depth)?;
        if num == 0.0 {
            return number_result(0.0);
        }
        let digits = match args.get(1) {
            Some(arg) => self.eval_arg_num(arg, ctx, depth)?,
            None => 0.0,
        };
        let mode = match name {
            "ROUND" => Rounding::Nearest,
            "ROUNDUP" => Rounding::AwayFromZero,
            _ => Rounding::TowardZero,
        };
        number_result(clean_zero(round_result(
            to_precision(num, EXCEL_PRECISION),
            digits.trunc(),
            mode,
        )?))
    }

    fn math_mround(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_math_arity(args, 2, 2)?;
        let num = self.eval_arg_num(&args[0], ctx, depth)?;
        let mult = self.eval_arg_num(&args[1], ctx, depth)?;
        if mult == 0.0 || num == 0.0 {
            return number_result(0.0);
        }
        if (num > 0.0 && mult < 0.0) || (num < 0.0 && mult > 0.0) {
            return Err(FormulaError::Num);
        }
        number_result(clean_zero((num / mult).round() * mult))
    }

    fn math_randbetween(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        check_math_arity(args, 2, 2)?;
        let lo = self.eval_arg_num(&args[0], ctx, depth)?.ceil();
        let hi = self.eval_arg_num(&args[1], ctx, depth)?.floor();
        if lo > hi {
            return Err(FormulaError::Num);
        }
        let span = (hi - lo) + 1.0;
        let r = (lo + (ctx.next_rand_f64() * span).floor()).min(hi);
        number_result(clean_zero(r))
    }

    fn math_ceiling_floor(
        &self,
        is_ceiling: bool,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        check_math_arity(args, 1, 2)?;
        let num = self.eval_arg_num(&args[0], ctx, depth)?;
        let sig = match args.get(1) {
            Some(arg) => self.eval_arg_num(arg, ctx, depth)?,
            None if num < 0.0 => -1.0,
            None => 1.0,
        };
        ceiling_floor(is_ceiling, num, sig)
    }

    fn math_transcendental(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        check_math_arity(args, 1, 1)?;
        let n = self.eval_arg_num(&args[0], ctx, depth)?;
        if (name == "LN" || name == "LOG10") && n <= 0.0 {
            return Err(FormulaError::Num);
        }
        if (name == "ASIN" || name == "ACOS") && !(-1.0..=1.0).contains(&n) {
            return Err(FormulaError::Num);
        }
        let res = match name {
            "EXP" => n.exp(),
            "LN" => n.ln(),
            "LOG10" => n.log10(),
            "SIN" => n.sin(),
            "COS" => n.cos(),
            "ASIN" => n.asin(),
            "ACOS" => n.acos(),
            "ATAN" => n.atan(),
            "SINH" => n.sinh(),
            "COSH" => n.cosh(),
            "TANH" => n.tanh(),
            _ => n.tan(),
        };
        if !res.is_finite() {
            return Err(FormulaError::Num);
        }
        number_result(res)
    }

    fn math_log(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_math_arity(args, 1, 2)?;
        let n = self.eval_arg_num(&args[0], ctx, depth)?;
        let base = match args.get(1) {
            Some(arg) => self.eval_arg_num(arg, ctx, depth)?,
            None => 10.0,
        };
        if n <= 0.0 || base <= 0.0 || (base - 1.0).abs() < LOG_BASE_EPSILON {
            return Err(FormulaError::Num);
        }
        number_result(n.log(base))
    }

    fn math_n(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_math_arity(args, 1, 1)?;
        match self.eval_expr_ctx(&args[0], ctx, depth) {
            EvaluatedCellValue::Number(n) => number_result(n),
            EvaluatedCellValue::Boolean(b) => number_result(if b { 1.0 } else { 0.0 }),
            EvaluatedCellValue::Error(e) => Err(e),
            _ => number_result(0.0),
        }
    }

    fn math_average_a(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        let mut sum = 0.0;
        let mut count = 0usize;
        self.visit_args_streaming(args, ctx, depth, |v| {
            average_a_value(&mut sum, &mut count, v)
        })?;
        if count == 0 {
            return Err(FormulaError::DivByZero);
        }
        number_result(sum / (count as f64))
    }

    fn math_sumsq(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_math_arity(args, 1, usize::MAX)?;
        let mut sum = 0.0f64;
        self.visit_args_streaming(args, ctx, depth, |v| {
            if let EvaluatedCellValue::Number(x) = v {
                sum += x * x;
            }
            Ok(())
        })?;
        number_result(sum)
    }

    fn math_gcd_lcm(
        &self,
        is_lcm: bool,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        check_math_arity(args, 1, usize::MAX)?;
        let mut acc: Option<u64> = None;
        self.visit_args_streaming(args, ctx, depth, |v| {
            let x = match v {
                EvaluatedCellValue::Empty => return Ok(()),
                EvaluatedCellValue::Number(n) => n,
                _ => return Err(FormulaError::Value),
            };
            if !(0.0..9_007_199_254_740_992.0).contains(&x) {
                return Err(FormulaError::Num);
            }
            let n = x.trunc() as u64;
            acc = Some(match acc {
                None => n,
                Some(prev) if is_lcm => lcm_u64(prev, n).ok_or(FormulaError::Num)?,
                Some(prev) => gcd_u64(prev, n),
            });
            Ok(())
        })?;
        number_result(acc.unwrap_or(0) as f64)
    }

    fn math_combin_permut(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        check_math_arity(args, 2, 2)?;
        let n_raw = self.eval_arg_num(&args[0], ctx, depth)?.trunc();
        let k_raw = self.eval_arg_num(&args[1], ctx, depth)?.trunc();
        if !(0.0..=1e9).contains(&n_raw) || !(0.0..=1e9).contains(&k_raw) {
            return Err(FormulaError::Num);
        }
        let n = n_raw as u64;
        let k = k_raw as u64;
        let res = match name {
            "PERMUTATIONA" => {
                if k == 0 {
                    1.0
                } else {
                    (n as f64).powf(k as f64)
                }
            }
            "COMBINA" => {
                if k == 0 {
                    1.0
                } else if n == 0 {
                    0.0
                } else {
                    let m = n + k - 1;
                    let c = k.min(m - k);
                    if c >= 515 {
                        return Err(FormulaError::Num);
                    }
                    let mut acc = 1.0f64;
                    for i in 1..=c {
                        acc = acc * ((m - c + i) as f64) / (i as f64);
                        if !acc.is_finite() {
                            return Err(FormulaError::Num);
                        }
                    }
                    acc.round()
                }
            }
            "PERMUT" => {
                if k > n || k > 170 {
                    return Err(FormulaError::Num);
                }
                let mut acc = 1.0f64;
                for i in (n - k + 1)..=n {
                    acc *= i as f64;
                    if !acc.is_finite() {
                        return Err(FormulaError::Num);
                    }
                }
                acc
            }
            _ => {
                if k > n {
                    return Err(FormulaError::Num);
                }
                let c = k.min(n - k);
                if c >= 515 {
                    return Err(FormulaError::Num);
                }
                let mut acc = 1.0f64;
                for i in 1..=c {
                    acc = acc * ((n - c + i) as f64) / (i as f64);
                    if !acc.is_finite() {
                        return Err(FormulaError::Num);
                    }
                }
                acc.round()
            }
        };
        if !res.is_finite() {
            return Err(FormulaError::Num);
        }
        number_result(res)
    }

    fn math_base(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_math_arity(args, 2, 3)?;
        let raw = self.eval_arg_num(&args[0], ctx, depth)?;
        let num = raw.trunc();
        let radix = self.eval_arg_num(&args[1], ctx, depth)?.trunc();
        // A negative number is `#NUM!` even when it truncates to 0 (`BASE(-0.5,2)`).
        if raw < 0.0
            || !(0.0..9_007_199_254_740_992.0).contains(&num)
            || !(2.0..=36.0).contains(&radix)
        {
            return Err(FormulaError::Num);
        }
        let min_len = match args.get(2) {
            Some(arg) => {
                let l = self.eval_arg_num(arg, ctx, depth)?.trunc();
                if !(0.0..=255.0).contains(&l) {
                    return Err(FormulaError::Num);
                }
                l as usize
            }
            None => 0,
        };
        let r = radix as u64;
        let mut n = num as u64;
        let mut buf = [0u8; 64];
        let mut len = 0usize;
        loop {
            let d = (n % r) as u8;
            buf[len] = if d < 10 { b'0' + d } else { b'A' + (d - 10) };
            len += 1;
            n /= r;
            if n == 0 {
                break;
            }
        }
        let mut out = String::with_capacity(len.max(min_len));
        for _ in len..min_len {
            out.push('0');
        }
        for &b in buf[..len].iter().rev() {
            out.push(char::from(b));
        }
        Ok(EvaluatedCellValue::Text(out))
    }

    fn math_decimal(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_math_arity(args, 2, 2)?;
        let s = self.eval_arg_val(&args[0], ctx, depth)?.to_display_string();
        let radix = self.eval_arg_num(&args[1], ctx, depth)?.trunc();
        if !(2.0..=36.0).contains(&radix) {
            return Err(FormulaError::Num);
        }
        // The text is at most 255 characters (`#VALUE!` past that); an empty text is 0.
        if s.chars().count() > 255 {
            return Err(FormulaError::Value);
        }
        if s.trim().is_empty() {
            return number_result(0.0);
        }
        let parsed = u128::from_str_radix(s.trim(), radix as u32).map_err(|_| FormulaError::Num)?;
        number_result(parsed as f64)
    }

    fn math_roman(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_math_arity(args, 1, 2)?;
        let n = self.eval_arg_num(&args[0], ctx, depth)?.trunc();
        if let Some(arg) = args.get(1) {
            let _ = self.eval_arg_val(arg, ctx, depth)?;
        }
        if !(0.0..=3999.0).contains(&n) {
            return Err(FormulaError::Value);
        }
        let mut rem = n as u32;
        let mut out = String::new();
        for (val, sym) in ROMAN_TABLE {
            while rem >= val {
                out.push_str(sym);
                rem -= val;
            }
        }
        Ok(EvaluatedCellValue::Text(out))
    }

    fn math_arabic(&self, args: &[Expr], ctx: &mut EvalContext<'_>, depth: usize) -> CellResult {
        check_math_arity(args, 1, 1)?;
        let raw = self.eval_arg_val(&args[0], ctx, depth)?.to_display_string();
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return number_result(0.0);
        }
        let (neg, body) = match trimmed.strip_prefix('-') {
            Some(rest) => (true, rest.trim()),
            None => (false, trimmed),
        };
        if neg && body.is_empty() {
            return Err(FormulaError::Value);
        }
        let mut total = 0i64;
        let mut prev = 0i64;
        for &b in body.as_bytes().iter().rev() {
            let v = roman_value(b).ok_or(FormulaError::Value)?;
            if v < prev {
                total -= v;
            } else {
                total += v;
                prev = v;
            }
        }
        number_result((if neg { -total } else { total }) as f64)
    }

    /// Paired-array statistical and linear regression functions (`CORREL`, `PEARSON`,
    /// `COVARIANCE.P`, `COVARIANCE.S`, `SLOPE`, `INTERCEPT`, `RSQ`, `FORECAST`, `FORECAST.LINEAR`;
    /// reference implementation: MIT License, Copyright (c) Sutoiku, Inc.; see `THIRD_PARTY_NOTICES.md`).
    pub(crate) fn eval_stat_regression(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellResult {
        let is_forecast = matches!(name, "FORECAST" | "FORECAST.LINEAR");
        let expected_arity = if is_forecast { 3 } else { 2 };
        check_math_arity(args, expected_arity, expected_arity)?;
        let forecast_x = if is_forecast {
            Some(self.eval_arg_num(&args[0], ctx, depth)?)
        } else {
            None
        };
        let (y_arg, x_arg) = if is_forecast {
            (&args[1], &args[2])
        } else {
            (&args[0], &args[1])
        };
        let ys = self.eval_range_vec(y_arg, ctx, depth)?;
        let xs = self.eval_range_vec(x_arg, ctx, depth)?;
        if ys.len() != xs.len() || ys.is_empty() {
            return Err(FormulaError::Na);
        }
        let mut pairs = Vec::with_capacity(ys.len());
        for (yv, xv) in ys.iter().zip(xs.iter()) {
            if let EvaluatedCellValue::Error(e) = yv {
                return Err(*e);
            }
            if let EvaluatedCellValue::Error(e) = xv {
                return Err(*e);
            }
            if let (EvaluatedCellValue::Number(y), EvaluatedCellValue::Number(x)) = (yv, xv) {
                pairs.push((*y, *x));
            }
        }
        let n = pairs.len();
        if n == 0 || (name == "STEYX" && n < 3) || (name != "COVARIANCE.P" && n < 2) {
            return Err(FormulaError::DivByZero);
        }
        let nf = n as f64;
        let mean_y = pairs.iter().map(|&(y, _)| y).sum::<f64>() / nf;
        let mean_x = pairs.iter().map(|&(_, x)| x).sum::<f64>() / nf;
        let mut s_yy = 0.0;
        let mut s_xx = 0.0;
        let mut s_xy = 0.0;
        for &(y, x) in &pairs {
            let dy = y - mean_y;
            let dx = x - mean_x;
            s_yy += dy * dy;
            s_xx += dx * dx;
            s_xy += dx * dy;
        }
        match name {
            "COVARIANCE.P" => number_result(s_xy / nf),
            "COVARIANCE.S" => number_result(s_xy / ((n - 1) as f64)),
            "CORREL" | "PEARSON" => {
                if s_xx == 0.0 || s_yy == 0.0 {
                    return Err(FormulaError::DivByZero);
                }
                number_result(s_xy / (s_xx * s_yy).sqrt())
            }
            "RSQ" => {
                if s_xx == 0.0 || s_yy == 0.0 {
                    return Err(FormulaError::DivByZero);
                }
                number_result((s_xy * s_xy) / (s_xx * s_yy))
            }
            "STEYX" => {
                if s_xx == 0.0 {
                    return Err(FormulaError::DivByZero);
                }
                let residual_ss = (s_yy - (s_xy * s_xy) / s_xx).max(0.0);
                number_result((residual_ss / ((n - 2) as f64)).sqrt())
            }
            "SLOPE" | "INTERCEPT" | "FORECAST" | "FORECAST.LINEAR" => {
                if s_xx == 0.0 {
                    return Err(FormulaError::DivByZero);
                }
                let slope = s_xy / s_xx;
                let intercept = mean_y - slope * mean_x;
                match name {
                    "SLOPE" => number_result(slope),
                    "INTERCEPT" => number_result(intercept),
                    _ => number_result(intercept + slope * forecast_x.unwrap_or(0.0)),
                }
            }
            _ => Err(FormulaError::Name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correl_covariance_slope_intercept_rsq_forecast_parity() {
        let mut s = SheetEngine::new("StatRegTest");
        for (i, (y, x)) in [
            ("6", "20"),
            ("7", "28"),
            ("9", "31"),
            ("15", "38"),
            ("21", "40"),
        ]
        .into_iter()
        .enumerate()
        {
            let r = i + 1;
            s.set_cell(&format!("A{r}"), y).unwrap();
            s.set_cell(&format!("B{r}"), x).unwrap();
        }
        let EvaluatedCellValue::Number(correl) = s.evaluate_formula_str("=CORREL(A1:A5, B1:B5)")
        else {
            panic!("expected CORREL number");
        };
        let EvaluatedCellValue::Number(pearson) = s.evaluate_formula_str("=PEARSON(A1:A5, B1:B5)")
        else {
            panic!("expected PEARSON number");
        };
        assert!((correl - pearson).abs() < 1e-12);
        let expected_correl = 183.8 / (159.2_f64 * 259.2).sqrt();
        assert!((correl - expected_correl).abs() < 1e-12, "correl={correl}");

        let EvaluatedCellValue::Number(rsq) = s.evaluate_formula_str("=RSQ(A1:A5, B1:B5)") else {
            panic!("expected RSQ number");
        };
        assert!((rsq - correl * correl).abs() < 1e-12);

        let EvaluatedCellValue::Number(cov_p) =
            s.evaluate_formula_str("=COVARIANCE.P(A1:A5, B1:B5)")
        else {
            panic!("expected COVARIANCE.P number");
        };
        let EvaluatedCellValue::Number(cov_s) =
            s.evaluate_formula_str("=COVARIANCE.S(A1:A5, B1:B5)")
        else {
            panic!("expected COVARIANCE.S number");
        };
        assert!((cov_p * 5.0 - cov_s * 4.0).abs() < 1e-12);

        let EvaluatedCellValue::Number(slope) = s.evaluate_formula_str("=SLOPE(A1:A5, B1:B5)")
        else {
            panic!("expected SLOPE number");
        };
        let EvaluatedCellValue::Number(intercept) =
            s.evaluate_formula_str("=INTERCEPT(A1:A5, B1:B5)")
        else {
            panic!("expected INTERCEPT number");
        };
        let EvaluatedCellValue::Number(fc) = s.evaluate_formula_str("=FORECAST(30, A1:A5, B1:B5)")
        else {
            panic!("expected FORECAST number");
        };
        let EvaluatedCellValue::Number(fc_lin) =
            s.evaluate_formula_str("=FORECAST.LINEAR(30, A1:A5, B1:B5)")
        else {
            panic!("expected FORECAST.LINEAR number");
        };
        assert!((fc - (intercept + slope * 30.0)).abs() < 1e-12);
        assert!((fc - fc_lin).abs() < 1e-12);
        assert!((fc - 10.607253).abs() < 1e-5, "fc={fc}");
    }
}
