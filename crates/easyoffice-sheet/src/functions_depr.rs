//! Depreciation and loan interest functions: `VDB`, `AMORLINC`, `ISPMT` (PD-201).
//!
//! Reference implementation: `packages/engine-formula/src/functions/financial/{vdb,amorlinc,ispmt}/
//! index.ts` and `src/basics/financial.ts` (`calculateDDB`) of the Univer formula engine, commit
//! `5a06a7842c8e13fa0646d0733dc3662cf4b330b9`, Apache-2.0, Copyright 2023-present DreamNum Co.,
//! Ltd. See `THIRD_PARTY_NOTICES.md`.
//!
//! `VDB` walks one period at a time, so the number of periods it covers is limited to
//! [`MAX_VDB_PERIODS`] (over it the result is `#NUM!`).

use crate::functions_coupon::days_by_basis;
use crate::functions_fin_args::ArgRule;
use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

type Fallible<T> = Result<T, FormulaError>;

/// Periods `VDB` may walk (the limit `DB` and `DDB` use).
const MAX_VDB_PERIODS: f64 = 10_000.0;

fn num(v: f64) -> Fallible<EvaluatedCellValue> {
    Ok(EvaluatedCellValue::Number(v))
}

/// Double-declining depreciation of one period (`calculateDDB`).
fn ddb_period(cost: f64, salvage: f64, life: f64, period: f64, factor: f64) -> f64 {
    let mut rate = factor / life;
    let old_cost = if rate >= 1.0 {
        rate = 1.0;
        if period == 1.0 {
            cost
        } else {
            0.0
        }
    } else {
        cost * (1.0 - rate).powf(period - 1.0)
    };
    let new_cost = cost * (1.0 - rate).powf(period);
    let depreciation = if new_cost < salvage {
        old_cost - salvage
    } else {
        old_cost - new_cost
    };
    depreciation.max(0.0)
}

/// Depreciation over `remaining..end` that switches to straight line when that is larger.
fn switching_depreciation(
    cost: f64,
    salvage: f64,
    life: f64,
    remaining: f64,
    end_period: f64,
    factor: f64,
) -> f64 {
    let end = end_period.ceil();
    let (mut total, mut rest, mut straight, mut switched) = (0.0, cost - salvage, 0.0, false);
    let mut i = 1.0;
    while i <= end {
        let mut part;
        if switched {
            part = straight;
        } else {
            let declining = ddb_period(cost, salvage, life, i, factor);
            straight = rest / (remaining - (i - 1.0));
            if straight > declining {
                part = straight;
                switched = true;
            } else {
                part = declining;
                rest -= declining;
            }
        }
        if i == end {
            part *= end_period + 1.0 - end;
        }
        total += part;
        i += 1.0;
    }
    total
}

fn variable_declining_balance(
    cost: f64,
    salvage: f64,
    life: f64,
    (start, end): (f64, f64),
    factor: f64,
    no_switch: bool,
) -> f64 {
    let first = start.floor();
    let last = end.ceil();
    if cost < salvage {
        if start >= 1.0 || no_switch {
            return 0.0;
        }
        let gap = (cost - salvage).abs();
        let span = gap * (end - start);
        return -(if span > gap { gap } else { span });
    }
    if no_switch {
        let mut total = 0.0;
        let mut i = first + 1.0;
        while i <= last {
            let mut part = ddb_period(cost, salvage, life, i, factor);
            if i == first + 1.0 {
                part *= end.min(first + 1.0) - start;
            } else if i == last {
                part *= end + 1.0 - last;
            }
            total += part;
            i += 1.0;
        }
        return total;
    }
    let after_start = cost - switching_depreciation(cost, salvage, life, life, start, factor);
    switching_depreciation(
        after_start,
        salvage,
        life,
        life - start,
        end - start,
        factor,
    )
}

impl SheetEngine {
    pub(crate) fn eval_depr_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<EvaluatedCellValue> {
        match name {
            "VDB" => self.fin_vdb(args, ctx, depth),
            "AMORLINC" => self.fin_amorlinc(args, ctx, depth),
            "ISPMT" => {
                let a = self.fin_args(args, (4, 4), ArgRule::BoolOk, ctx, depth)?;
                let (rate, per, nper, pv) = (a.num(0)?, a.num(1)?, a.num(2)?, a.num(3)?);
                if nper == 0.0 {
                    return Err(FormulaError::DivByZero);
                }
                num(pv * rate * (per / nper - 1.0))
            }
            _ => Err(FormulaError::Name),
        }
    }

    fn fin_vdb(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<EvaluatedCellValue> {
        let a = self.fin_args(args, (5, 7), ArgRule::BoolOk, ctx, depth)?;
        let (cost, salvage, life) = (a.num(0)?, a.num(1)?, a.num(2)?);
        let (start, end) = (a.num(3)?, a.num(4)?);
        let factor = a.num_or(5, 2.0)?;
        let no_switch = a.num_or(6, 0.0)? != 0.0;
        if cost < 0.0
            || salvage < 0.0
            || life < 0.0
            || start < 0.0
            || end < 0.0
            || end > life
            || start > end
            || factor < 0.0
        {
            return Err(FormulaError::Num);
        }
        if life == 0.0 && start == 0.0 && end == 0.0 {
            return Err(FormulaError::DivByZero);
        }
        if end.ceil() > MAX_VDB_PERIODS || life.is_nan() {
            return Err(FormulaError::Num);
        }
        num(variable_declining_balance(
            cost,
            salvage,
            life,
            (start, end),
            factor,
            no_switch,
        ))
    }

    fn fin_amorlinc(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<EvaluatedCellValue> {
        let a = self.fin_args(args, (6, 7), ArgRule::Strict, ctx, depth)?;
        let (purchased, first_period) = (a.date(1)?, a.date(2)?);
        let (cost, salvage, period, rate) = (a.num(0)?, a.num(3)?, a.num(4)?, a.num(5)?);
        let basis = a.int_or(6, 0.0)?;
        if cost <= 0.0
            || salvage < 0.0
            || cost < salvage
            || purchased > first_period
            || period < 0.0
            || rate <= 0.0
            || ![0.0, 1.0, 3.0, 4.0].contains(&basis)
        {
            return Err(FormulaError::Num);
        }
        let period = if period > 1.0 {
            period.floor()
        } else {
            period.ceil()
        };
        let total = cost - salvage;
        let yearly = cost * rate;
        let (days, year_days) = days_by_basis(purchased, first_period, basis as i64);
        let first_fraction = days / year_days;
        let life = (total / yearly - first_fraction).ceil();
        if life < 0.0 {
            return num(0.0);
        }
        num(if period == 0.0 {
            yearly * first_fraction
        } else if period == life {
            total - yearly * (first_fraction + period - 1.0)
        } else if period > life {
            0.0
        } else {
            yearly
        })
    }
}
