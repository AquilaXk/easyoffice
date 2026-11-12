//! Argument reading shared by the bond, security and depreciation functions (PD-201).
//!
//! The reference implementation reads the arguments of these functions in three passes: a
//! cell or range that is an error, a multi-cell array or a logical value decides first (by
//! position), then the date arguments are parsed, then the numeric ones. [`FinArgs`] keeps that
//! order, so a call with several bad arguments reports the same error as the reference.
//! Reference implementation: `packages/engine-formula/src/engine/utils/check-variant-error.ts` and
//! `src/basics/date.ts` (`getDateSerialNumberByObject`) of the Univer formula engine, commit
//! `5a06a7842c8e13fa0646d0733dc3662cf4b330b9`, Apache-2.0, Copyright 2023-present DreamNum Co.,
//! Ltd. See `THIRD_PARTY_NOTICES.md`.

use crate::coercion::coerce_to_number;
use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

/// Largest serial number of the 1900 date system (9999-12-31).
pub(crate) const MAX_SERIAL: f64 = 2_958_465.0;

/// How a call treats logical values and blanks among its arguments.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ArgRule {
    /// A logical value is `#VALUE!`, a blank reads as 0.
    Strict,
    /// As `Strict`, but a blank among the required arguments is `#N/A` (the bond functions).
    BlankNa,
    /// A logical value reads as 1 or 0, a blank as 0.
    BoolOk,
}

/// The evaluated arguments of one call, past the first pass (no errors, no logical values).
pub(crate) struct FinArgs {
    vals: Vec<EvaluatedCellValue>,
}

impl FinArgs {
    /// Argument `i` as a number: numbers, numeric text and dates in text form; a blank is 0.
    pub(crate) fn num(&self, i: usize) -> Result<f64, FormulaError> {
        coerce_to_number(&self.vals[i])
    }

    /// Argument `i` as a number, or `default` when the argument is missing or blank.
    pub(crate) fn num_or(&self, i: usize, default: f64) -> Result<f64, FormulaError> {
        match self.vals.get(i) {
            None | Some(EvaluatedCellValue::Empty) => Ok(default),
            Some(_) => self.num(i),
        }
    }

    /// Argument `i` as a whole number (rounded down); used for frequency and basis.
    pub(crate) fn int_or(&self, i: usize, default: f64) -> Result<f64, FormulaError> {
        self.num_or(i, default).map(f64::floor)
    }

    /// Argument `i` as text (numbers in their general format).
    pub(crate) fn text(&self, i: usize) -> String {
        self.vals[i].to_display_string()
    }

    /// Argument `i` as a date serial number: 0 to 2,958,465, fraction dropped.
    pub(crate) fn date(&self, i: usize) -> Result<i64, FormulaError> {
        let serial = self.num(i)?;
        if !(0.0..=MAX_SERIAL).contains(&serial) {
            return Err(FormulaError::Num);
        }
        Ok(serial.floor() as i64)
    }
}

impl SheetEngine {
    /// Evaluates every argument in order; the first error value, multi-cell range or logical
    /// value ends the call with its error (`#VALUE!` for the last two), unless `rule` says
    /// otherwise. A blank optional argument takes its default.
    pub(crate) fn fin_args(
        &self,
        args: &[Expr],
        (min, max): (usize, usize),
        rule: ArgRule,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<FinArgs, FormulaError> {
        if !(min..=max).contains(&args.len()) {
            return Err(FormulaError::Value);
        }
        let mut vals = Vec::with_capacity(args.len());
        for (i, arg) in args.iter().enumerate() {
            match self.eval_expr_ctx(arg, ctx, depth) {
                EvaluatedCellValue::Error(e) => return Err(e),
                EvaluatedCellValue::Boolean(_) if rule != ArgRule::BoolOk => {
                    return Err(FormulaError::Value)
                }
                EvaluatedCellValue::Empty if rule == ArgRule::BlankNa && i < min => {
                    return Err(FormulaError::Na)
                }
                other => vals.push(other),
            }
        }
        Ok(FinArgs { vals })
    }
}
