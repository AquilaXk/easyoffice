//! Hyperbolic, reciprocal and logarithm functions of complex numbers: `IMSINH`, `IMCOSH`, `IMTAN`,
//! `IMTANH`, `IMSEC`, `IMSECH`, `IMCSC`, `IMCSCH`, `IMCOT`, `IMCOTH`, `IMLOG` (PD-201).
//!
//! Reference implementation: `packages/engine-formula/src/basics/complex.ts` (`Sinh`, `Cosh`,
//! `Tan`, `Tanh`, `Sec`, `Sech`, `Csc`, `Csch`, `Cot`, `Coth`, `Log`) and
//! `src/functions/engineering/{imsinh,imcosh,imtan,imtanh,imsec,imsech,imcsc,imcsch,imcot,imcoth,
//! imlog}/index.ts` of the Univer formula engine, commit `5a06a7842c8e13fa0646d0733dc3662cf4b330b9`,
//! Apache-2.0, Copyright 2023-present DreamNum Co., Ltd. See `THIRD_PARTY_NOTICES.md`.
//!
//! The reference computes in decimal arithmetic and rounds the parts to 15 significant digits;
//! here the parts are `f64` and the shared complex formatter rounds them the same way. The
//! imaginary unit is read from the argument (`i` or `j`), as the other `IM*` functions do.

use crate::functions_complex::format_complex_res;
use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

type Fallible<T> = Result<T, FormulaError>;

/// `(re, im)` of the function `name` at `a + b i`; `None` is the reference's zero result for
/// arguments whose hyperbolic parts overflow (`IMSECH`, `IMCSCH`). Overflowing arguments of the
/// other hyperbolic functions are `#NUM!`.
fn evaluate(name: &str, a: f64, b: f64) -> Fallible<Option<(f64, f64)>> {
    let overflow = !a.sinh().is_finite() || !a.cosh().is_finite();
    let overflow_double = !(2.0 * a).sinh().is_finite() || !(2.0 * a).cosh().is_finite();
    let part = |re: f64, im: f64| Ok(Some((re, im)));
    match name {
        "IMSINH" | "IMCOSH" | "IMTANH" | "IMCOTH" if overflow => Err(FormulaError::Num),
        "IMSECH" if overflow_double => Ok(None),
        "IMCSCH" if overflow => Ok(None),
        "IMSINH" if b != 0.0 => part(a.sinh() * b.cos(), a.cosh() * b.sin()),
        "IMSINH" => part(a.sinh(), 0.0),
        "IMCOSH" if b != 0.0 => part(a.cosh() * b.cos(), a.sinh() * b.sin()),
        "IMCOSH" => part(a.cosh(), 0.0),
        "IMTAN" if b != 0.0 => {
            let den = (2.0 * a).cos() + (2.0 * b).cosh();
            part((2.0 * a).sin() / den, (2.0 * b).sinh() / den)
        }
        "IMTAN" => part(a.tan(), 0.0),
        "IMTANH" if b != 0.0 => {
            let den = (2.0 * a).cosh() + (2.0 * b).cos();
            part((2.0 * a).sinh() / den, (2.0 * b).sin() / den)
        }
        "IMTANH" => part(a.tanh(), 0.0),
        "IMSEC" if b != 0.0 => {
            let den = (2.0 * b).cosh() + (2.0 * a).cos();
            part(
                2.0 * a.cos() * b.cosh() / den,
                2.0 * a.sin() * b.sinh() / den,
            )
        }
        "IMSEC" => part(1.0 / a.cos(), 0.0),
        "IMSECH" if b != 0.0 => {
            let den = (2.0 * a).cosh() + (2.0 * b).cos();
            part(
                2.0 * a.cosh() * b.cos() / den,
                -2.0 * a.sinh() * b.sin() / den,
            )
        }
        "IMSECH" => part(1.0 / a.cosh(), 0.0),
        "IMCSC" if b != 0.0 => {
            let den = (2.0 * b).cosh() - (2.0 * a).cos();
            part(
                2.0 * a.sin() * b.cosh() / den,
                -2.0 * a.cos() * b.sinh() / den,
            )
        }
        "IMCSC" => part(1.0 / a.sin(), 0.0),
        "IMCSCH" if b != 0.0 => {
            let den = (2.0 * a).cosh() - (2.0 * b).cos();
            part(
                2.0 * a.sinh() * b.cos() / den,
                -2.0 * a.cosh() * b.sin() / den,
            )
        }
        "IMCSCH" => part(1.0 / a.sinh(), 0.0),
        "IMCOT" if b != 0.0 => {
            let den = (2.0 * b).cosh() - (2.0 * a).cos();
            part((2.0 * a).sin() / den, -(2.0 * b).sinh() / den)
        }
        "IMCOT" => part(1.0 / a.tan(), 0.0),
        "IMCOTH" if b != 0.0 => {
            let den = (2.0 * a).cosh() - (2.0 * b).cos();
            part((2.0 * a).sinh() / den, -(2.0 * b).sin() / den)
        }
        "IMCOTH" => part(1.0 / a.tanh(), 0.0),
        _ => Err(FormulaError::Name),
    }
}

impl SheetEngine {
    pub(crate) fn eval_im_trig_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<EvaluatedCellValue> {
        if name == "IMLOG" {
            return self.eval_imlog(args, ctx, depth);
        }
        if args.len() != 1 {
            return Err(FormulaError::Value);
        }
        let (a, b, suffix) = self.eval_complex_arg(&args[0], ctx, depth)?;
        match evaluate(name, a, b)? {
            Some((re, im)) => format_complex_res(re, im, suffix),
            None => Ok(EvaluatedCellValue::Number(0.0)),
        }
    }

    /// `IMLOG(inumber, [base])`: the natural logarithm divided by `ln(base)` (default base 10).
    fn eval_imlog(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<EvaluatedCellValue> {
        if !(1..=2).contains(&args.len()) {
            return Err(FormulaError::Value);
        }
        let (a, b, suffix) = self.eval_complex_arg(&args[0], ctx, depth)?;
        let base = match args.get(1) {
            Some(arg) => self.eval_strict_num_arg(arg, ctx, depth)?,
            None => 10.0,
        };
        if (a == 0.0 && b == 0.0) || base <= 0.0 {
            return Err(FormulaError::Num);
        }
        let ln_base = base.ln();
        if ln_base == 0.0 {
            return Err(FormulaError::Num);
        }
        let ln_abs = a.hypot(b).ln();
        let argument = b.atan2(a);
        format_complex_res(ln_abs / ln_base, argument / ln_base, suffix)
    }
}
