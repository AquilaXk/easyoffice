//! Reciprocal and inverse hyperbolic functions, the significance rounding variants and bankers
//! rounding (PD-205): `ACOSH`, `ASINH`, `ATANH`, `ACOT`, `ACOTH`, `COT`, `COTH`, `CSC`, `CSCH`,
//! `SEC`, `SECH`, `CEILING.MATH`, `CEILING.PRECISE`, `FLOOR.MATH`, `FLOOR.PRECISE`, `ROUNDBANK`,
//! `ISO.CEILING` and `PERCENTOF`.
//!
//! Reference implementation: `packages/engine-formula/src/functions/math/{acosh,asinh,atanh,acot,
//! acoth,cot,coth,csc,csch,sec,sech,ceiling-math,ceiling-precise,floor-math,floor-precise,
//! roundbank}/index.ts` and `src/engine/utils/math-kit.ts` (`ceil`, `floor`) of the Univer formula
//! engine, commit `5a06a7842c8e13fa0646d0733dc3662cf4b330b9`, Apache-2.0, Copyright 2023-present
//! DreamNum Co., Ltd. See `THIRD_PARTY_NOTICES.md`.
//!
//! `ISO.CEILING` and `PERCENTOF` are IronCalc `base/src/functions/math_and_trigonometry/mathematical.rs`
//! (`fn_iso_ceiling` 852-855, `fn_percentof` 1265-1281), 기준 커밋
//! f93662006b8fb1c401dc870bbda26c5d60ac6037, MIT OR Apache-2.0, Copyright 2023 EqualTo GmbH,
//! 2023-present IronCalc contributors, 2026-present IronCalc GmbH.
//!
//! Arguments read like the reference: a blank cell is 0, a logical value is 1 or 0, numeric text is
//! converted and other text is `#VALUE!`. A result that is not a finite number is `#NUM!`.

use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

type Fallible<T> = Result<T, FormulaError>;

/// `|x|` at or above this makes `COT`, `CSC` and `SEC` give `#NUM!` (the reference's `2^27`).
const TRIG_LIMIT: f64 = 134_217_728.0;
/// Digits beyond which `ROUNDBANK` leaves the number alone (above) or gives 0 (below).
const BANK_MAX_DIGITS: f64 = 16.0;
/// Width of the "exactly one half" band `ROUNDBANK` accepts after scaling to 8 decimals.
const BANK_HALF_BAND: f64 = 1e-8;

/// `Math.ceil(x - eps)` with the reference's relative `eps` that absorbs binary representation error.
fn ceil_eps(x: f64) -> f64 {
    (x - f64::EPSILON * x.abs().max(1.0)).ceil()
}

fn floor_eps(x: f64) -> f64 {
    (x + f64::EPSILON * x.abs().max(1.0)).floor()
}

/// The significance rounding of `CEILING.MATH` and `FLOOR.MATH`: toward the larger value, or away
/// from zero for a negative number when `mode` is not 0. The sign of `significance` is ignored.
fn round_to_multiple(up: bool, number: f64, significance: f64, mode: f64) -> f64 {
    let step = |x: f64| if up { ceil_eps(x) } else { floor_eps(x) };
    if number < 0.0 && mode != 0.0 {
        if significance < 0.0 {
            step(number.abs() / significance.abs()) * significance
        } else {
            -step(number.abs() / significance) * significance
        }
    } else if significance < 0.0 {
        -step(number / significance.abs()) * significance
    } else {
        step(number / significance) * significance
    }
}

/// `ROUNDBANK`: halves round to the even neighbor; the scaled value is first fixed to 8 decimals.
fn round_bank(number: f64, digits: f64) -> f64 {
    if !number.is_finite() {
        return number;
    }
    if digits > BANK_MAX_DIGITS || number.abs() >= 4_503_599_627_370_496.0 {
        return number;
    }
    if digits < -BANK_MAX_DIGITS {
        return 0.0;
    }
    let multiplier = 10f64.powf(digits);
    let scaled = round_decimals(number * multiplier, 8);
    let integer = scaled.floor();
    let fraction = scaled - integer;
    let mut result = (scaled + 0.5).floor();
    if fraction > 0.5 - BANK_HALF_BAND && fraction < 0.5 + BANK_HALF_BAND {
        result = if integer % 2.0 == 0.0 {
            integer
        } else {
            integer + 1.0
        };
    }
    if digits == 0.0 {
        result
    } else {
        result / multiplier
    }
}

/// Pure f64 scale/rounding replacing format!("{:.places$}").parse() round-trip.
#[inline]
fn round_decimals(x: f64, places: usize) -> f64 {
    let scale = 10f64.powi(places as i32);
    if !x.is_finite() || x.abs() >= 4_503_599_627_370_496.0 / scale {
        return x;
    }
    (x * scale).round() / scale
}

fn finite(v: f64) -> Fallible<EvaluatedCellValue> {
    if v.is_finite() {
        Ok(EvaluatedCellValue::Number(v))
    } else {
        Err(FormulaError::Num)
    }
}

/// The reciprocal functions and the inverse hyperbolic functions of one number.
fn unary(name: &str, x: f64) -> Fallible<EvaluatedCellValue> {
    let reciprocal = |v: f64| {
        if v == 0.0 {
            Err(FormulaError::DivByZero)
        } else {
            finite(1.0 / v)
        }
    };
    match name {
        "ACOSH" => finite(x.acosh()),
        "ASINH" => finite(x.asinh()),
        "ATANH" => finite(x.atanh()),
        "ACOT" => {
            let r = (1.0 / x).atan();
            finite(if x < 0.0 { r + std::f64::consts::PI } else { r })
        }
        "ACOTH" if x.abs() <= 1.0 => Err(FormulaError::Num),
        "ACOTH" => finite(0.5 * ((x + 1.0) / (x - 1.0)).ln()),
        "COT" | "CSC" | "SEC" if x.abs() >= TRIG_LIMIT => Err(FormulaError::Num),
        "COT" if x == 0.0 => Err(FormulaError::DivByZero),
        "COT" => reciprocal(x.tan()),
        "CSC" if x == 0.0 => Err(FormulaError::DivByZero),
        "CSC" => reciprocal(x.sin()),
        "SEC" => reciprocal(x.cos()),
        "COTH" if x == 0.0 => Err(FormulaError::DivByZero),
        "COTH" => reciprocal(x.tanh()),
        "CSCH" if x == 0.0 => Err(FormulaError::DivByZero),
        "CSCH" if !x.sinh().is_finite() => finite(0.0),
        "CSCH" => reciprocal(x.sinh()),
        // SECH
        _ if !x.cosh().is_finite() => finite(0.0),
        _ => reciprocal(x.cosh()),
    }
}

impl SheetEngine {
    /// Evaluates a number argument the way the reference reads it (a blank cell is 0).
    pub(crate) fn eval_arg_num_blank_zero(
        &self,
        arg: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<f64> {
        match self.eval_arg_val(arg, ctx, depth)? {
            EvaluatedCellValue::Empty => Ok(0.0),
            other => other.as_number().ok_or(FormulaError::Value),
        }
    }

    /// An optional number argument: absent or skipped (`f(1,,2)`) gives `default`.
    fn optional_num(
        &self,
        arg: Option<&Expr>,
        default: f64,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<f64> {
        match arg {
            None | Some(Expr::Missing) => Ok(default),
            Some(arg) => self.eval_arg_num_blank_zero(arg, ctx, depth),
        }
    }

    /// `PERCENTOF(subset, total)`: the sum of `subset` as a share of the sum of `total`.
    ///
    pub(crate) fn eval_percentof(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<EvaluatedCellValue> {
        if args.len() != 2 {
            return Err(FormulaError::Value);
        }
        let sum_of = |arg: &Expr, ctx: &mut EvalContext<'_>| {
            self.eval_streaming_range_agg_ctx("SUM", std::slice::from_ref(arg), ctx, depth)
        };
        let part = sum_of(&args[0], ctx)?;
        let total = sum_of(&args[1], ctx)?;
        match (part, total) {
            (EvaluatedCellValue::Number(_), EvaluatedCellValue::Number(0.0)) => {
                Err(FormulaError::DivByZero)
            }
            (EvaluatedCellValue::Number(p), EvaluatedCellValue::Number(t)) => {
                Ok(EvaluatedCellValue::Number(p / t))
            }
            (EvaluatedCellValue::Error(e), _) | (_, EvaluatedCellValue::Error(e)) => Err(e),
            _ => Err(FormulaError::Value),
        }
    }

    pub(crate) fn eval_math_trig_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<EvaluatedCellValue> {
        // `ISO.CEILING` is `CEILING.PRECISE`.
        let name = if name == "ISO.CEILING" {
            "CEILING.PRECISE"
        } else {
            name
        };
        let (min, max) = match name {
            "ROUNDBANK" => (2, 2),
            "CEILING.MATH" | "FLOOR.MATH" => (1, 3),
            "CEILING.PRECISE" | "FLOOR.PRECISE" => (1, 2),
            _ => (1, 1),
        };
        if !(min..=max).contains(&args.len()) {
            return Err(FormulaError::Value);
        }
        let number = self.eval_arg_num_blank_zero(&args[0], ctx, depth)?;
        match name {
            "ROUNDBANK" => {
                let digits = self.eval_arg_num_blank_zero(&args[1], ctx, depth)?;
                finite(round_bank(number, digits.trunc()))
            }
            "CEILING.MATH" | "FLOOR.MATH" | "CEILING.PRECISE" | "FLOOR.PRECISE" => {
                let significance = self.optional_num(args.get(1), 1.0, ctx, depth)?;
                let mode = self.optional_num(args.get(2), 0.0, ctx, depth)?;
                if number == 0.0 || significance == 0.0 {
                    return Ok(EvaluatedCellValue::Number(0.0));
                }
                finite(round_to_multiple(
                    name.starts_with("CEILING"),
                    number,
                    significance,
                    mode,
                ))
            }
            _ => unary(name, number),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundbank_handles_large_floats_without_overflow() {
        assert_eq!(round_bank(1e305, 2.0), 1e305);
        assert_eq!(round_bank(-1e305, 2.0), -1e305);
        assert_eq!(round_decimals(1e305, 8), 1e305);
    }
}
