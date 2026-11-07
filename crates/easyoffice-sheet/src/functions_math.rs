use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

type CellResult = Result<EvaluatedCellValue, FormulaError>;

const MAX_ROUND_DIGITS: f64 = 300.0;
const MIN_ROUND_DIGITS: f64 = -309.0;
const MAX_FACTORIAL_ARG: f64 = 170.0;
const ROUND_NUDGE_ULPS: f64 = 2.0;
const SNAP_ULPS: f64 = 4.0;
const LOG_BASE_EPSILON: f64 = 1e-15;

fn check_math_arity(args: &[Expr], min: usize, max: usize) -> Result<(), FormulaError> {
    if args.len() < min || args.len() > max {
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum RoundMode {
    Nearest,
    Up,
    Down,
}

/// Rounds `scaled` (already multiplied by the digit factor) in the given mode.
fn round_scaled(mode: RoundMode, scaled: f64, num: f64, exp: i32) -> f64 {
    let abs_s = scaled.abs();
    let near_integer = exp > 0 && (abs_s - abs_s.round()).abs() <= abs_s * f64::EPSILON * SNAP_ULPS;
    match mode {
        RoundMode::Nearest if exp > 0 => {
            let nudge = abs_s * f64::EPSILON * ROUND_NUDGE_ULPS;
            (scaled + nudge.copysign(scaled)).round()
        }
        RoundMode::Nearest => scaled.round(),
        RoundMode::Up => if near_integer {
            abs_s.round()
        } else {
            abs_s.ceil()
        }
        .copysign(num),
        RoundMode::Down => if near_integer {
            abs_s.round()
        } else {
            abs_s.floor()
        }
        .copysign(num),
    }
}

/// ROUND / ROUNDUP / ROUNDDOWN / TRUNC core.
fn round_digits(mode: RoundMode, num: f64, digits: f64) -> Result<f64, FormulaError> {
    let d_trunc = digits.trunc();
    if d_trunc >= MAX_ROUND_DIGITS {
        return Ok(num);
    }
    let underflow = || {
        if mode == RoundMode::Up {
            Err(FormulaError::Num)
        } else {
            Ok(0.0)
        }
    };
    if d_trunc <= MIN_ROUND_DIGITS {
        return underflow();
    }
    let exp = d_trunc as i32;
    let factor = 10f64.powi(exp);
    if !factor.is_finite() || factor == 0.0 {
        return if exp > 0 { Ok(num) } else { underflow() };
    }
    let scaled = num * factor;
    if !scaled.is_finite() {
        return Ok(num);
    }
    let res = round_scaled(mode, scaled, num, exp) / factor;
    if !res.is_finite() {
        return Err(FormulaError::Num);
    }
    Ok(clean_zero(res))
}

/// CEILING / FLOOR core with significance `sig`.
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
    let q = num / sig;
    let q_adj = if (q - q.round()).abs() <= q.abs() * f64::EPSILON * SNAP_ULPS {
        q.round()
    } else {
        q
    };
    number_result(if is_ceiling {
        q_adj.ceil() * sig
    } else {
        q_adj.floor() * sig
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
            "ABS" | "SIGN" | "INT" | "SQRT" | "FACT" => self.math_unary(name, args, ctx, depth),
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
            "EXP" | "LN" | "LOG10" | "SIN" | "COS" | "TAN" => {
                self.math_transcendental(name, args, ctx, depth)
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
            "ROUND" => RoundMode::Nearest,
            "ROUNDUP" => RoundMode::Up,
            _ => RoundMode::Down,
        };
        number_result(round_digits(mode, num, digits)?)
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
        let q = num / mult;
        let nudge = q.abs() * f64::EPSILON * ROUND_NUDGE_ULPS;
        number_result(clean_zero((q + nudge.copysign(q)).round() * mult))
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
        let res = match name {
            "EXP" => n.exp(),
            "LN" => n.ln(),
            "LOG10" => n.log10(),
            "SIN" => n.sin(),
            "COS" => n.cos(),
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
}
