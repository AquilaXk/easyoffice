//! Depreciation and loan interest functions: `VDB`, `AMORLINC`, `ISPMT` (PD-201) and `AMORDEGRC`.
//!
//! Reference implementation: `packages/engine-formula/src/functions/financial/{vdb,amorlinc,ispmt}/
//! index.ts` and `src/basics/financial.ts` (`calculateDDB`) of the Univer formula engine, commit
//! `5a06a7842c8e13fa0646d0733dc3662cf4b330b9`, Apache-2.0, Copyright 2023-present DreamNum Co.,
//! Ltd. See `THIRD_PARTY_NOTICES.md`.
//!
//! `AMORDEGRC` is IronCalc `base/src/functions/financial/financial_depreciation.rs` (`amor_degrc` 276-333,
//! `amor_first_depr_linc` 177-212, `amor_depr_coeff`, `amor_round`, `fn_amordegrc` 494-574; 기준 커밋
//! f93662006b8fb1c401dc870bbda26c5d60ac6037, MIT OR Apache-2.0, Copyright 2023 EqualTo GmbH,
//! 2023-present IronCalc contributors, 2026-present IronCalc GmbH) on this engine's day counts.
//!
//! `VDB` walks one period at a time, so the number of periods it covers is limited to
//! [`MAX_VDB_PERIODS`] (over it the result is `#NUM!`).

use crate::functions_coupon::{days_by_basis, is_leap, year_month_day};
use crate::functions_fin_args::ArgRule;
use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

type Fallible<T> = Result<T, FormulaError>;

/// Periods `VDB` may walk (the limit `DB` and `DDB` use).
const MAX_VDB_PERIODS: f64 = 10_000.0;

fn num(v: f64) -> Fallible<EvaluatedCellValue> {
    Ok(EvaluatedCellValue::Number(v))
}

/// Days of the calendar year the AMOR functions divide the first period by.
fn amor_days_in_year(date: i64, basis: i64) -> f64 {
    match basis {
        1 if is_leap(year_month_day(date).0) => 366.0,
        1 | 3 => 365.0,
        _ => 360.0,
    }
}

/// A late February day of a leap year counted as 28 February on the actual-day bases.
fn amor_fix_29_february(date: i64, basis: i64) -> i64 {
    let (y, m, d) = year_month_day(date);
    if (basis == 1 || basis == 3) && is_leap(y) && m == 2 && d >= 28 {
        date - (d - 28)
    } else {
        date
    }
}

/// Depreciation of the first (possibly part) period and the asset life it gives.
fn amor_first_depr(
    (cost, salvage, rate, life): (f64, f64, f64, f64),
    (purchased, first_period): (i64, i64),
    basis: i64,
) -> (f64, f64) {
    let days_in_year = amor_days_in_year(purchased, basis);
    let (purchased, first_period) = (
        amor_fix_29_february(purchased, basis),
        amor_fix_29_february(first_period, basis),
    );
    let first_len = days_by_basis(purchased, first_period, basis).0;
    let first_temp = first_len / days_in_year * rate * cost;
    let first = if first_temp == 0.0 {
        cost * rate
    } else {
        first_temp
    };
    let asset_life = if first_temp == 0.0 { life } else { life + 1.0 };
    (first.min(cost - salvage), asset_life)
}

/// The depreciation coefficient that grows with the asset life.
fn amor_depr_coeff(asset_life: f64) -> f64 {
    if (3.0..=4.0).contains(&asset_life) {
        1.5
    } else if (5.0..=6.0).contains(&asset_life) {
        2.0
    } else if asset_life > 6.0 {
        2.5
    } else {
        1.0
    }
}

/// Rounded at 13 significant digits first, then to the nearest whole number.
fn amor_round(x: f64) -> f64 {
    ((x * 1e13).round() / 1e13).round()
}

fn amor_degrc(
    (cost, salvage, period, rate): (f64, f64, f64, f64),
    dates: (i64, i64),
    basis: i64,
) -> f64 {
    let life = (1.0 / rate).ceil();
    if cost == salvage || period > life {
        return 0.0;
    }
    let depr_rate = rate * amor_depr_coeff(life);
    let (first_linc, asset_life) = amor_first_depr((cost, salvage, depr_rate, life), dates, basis);
    let first = amor_round(first_linc);
    if period == 0.0 {
        return first;
    }
    let near = |x: f64, y: f64| (x - y).abs() < 0.0001;
    let (mut counted, mut depr, mut rate_now, mut remain) = (1.0, 0.0, depr_rate, cost - first);
    while counted <= period {
        counted += 1.0;
        let last_but_one = near(asset_life - counted, 2.0);
        let temp = if last_but_one {
            remain * 0.5
        } else {
            rate_now * remain
        };
        if last_but_one {
            rate_now = 1.0;
        }
        depr = if remain < salvage {
            (remain - salvage).max(0.0)
        } else {
            temp
        };
        remain -= depr;
    }
    amor_round(depr)
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
            "AMORDEGRC" => self.fin_amordegrc(args, ctx, depth),
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

    fn fin_amordegrc(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<EvaluatedCellValue> {
        let a = self.fin_args(args, (6, 7), ArgRule::Strict, ctx, depth)?;
        let (purchased, first_period) = (a.date(1)?, a.date(2)?);
        let (cost, salvage, period, rate) = (a.num(0)?, a.num(3)?, a.num(4)?, a.num(5)?);
        let basis = a.int_or(6, 0.0)?;
        let life = 1.0 / rate;
        if cost < 0.0
            || salvage < 0.0
            || salvage > cost
            || period < 0.0
            || rate <= 0.0
            || purchased >= first_period
            || ![0.0, 1.0, 3.0, 4.0].contains(&basis)
            || (0.0..=3.0).contains(&life)
            || (4.0..5.0).contains(&life)
        {
            return Err(FormulaError::Num);
        }
        num(amor_degrc(
            (cost, salvage, period, rate),
            (purchased, first_period),
            basis as i64,
        ))
    }

    pub(crate) fn fin_sln(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if args.len() != 3 {
            return Err(FormulaError::Value);
        }
        let cost = self.eval_arg_num(&args[0], ctx, depth)?;
        let salvage = self.eval_arg_num(&args[1], ctx, depth)?;
        let life = self.eval_arg_num(&args[2], ctx, depth)?;
        if life.abs() < 1e-12 {
            return Err(FormulaError::DivByZero);
        }
        num((cost - salvage) / life)
    }

    pub(crate) fn fin_db(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if args.len() < 4 || args.len() > 5 {
            return Err(FormulaError::Value);
        }
        let cost = self.eval_arg_num(&args[0], ctx, depth)?;
        let salvage = self.eval_arg_num(&args[1], ctx, depth)?;
        let life = self.eval_arg_num(&args[2], ctx, depth)?;
        let period = self.eval_arg_num(&args[3], ctx, depth)?.trunc() as usize;
        let month = match args.get(4) {
            Some(a) => self.eval_arg_num(a, ctx, depth)?.trunc() as usize,
            None => 12,
        };
        if cost <= 0.0
            || salvage < 0.0
            || life <= 0.0
            || !(1..=100_000).contains(&period)
            || !(1..=12).contains(&month)
        {
            return Err(FormulaError::Num);
        }
        let rate = ((1.0 - (salvage / cost).powf(1.0 / life)) * 1000.0).round() / 1000.0;
        let months = 12.0;
        let mut total_dep = 0.0;
        let mut dep = 0.0;
        for p in 1..=period {
            dep = if p == 1 {
                cost * rate * (month as f64) / months
            } else if p == (life.trunc() as usize) + 1 && month < 12 {
                (cost - total_dep) * rate * ((12 - month) as f64) / months
            } else {
                (cost - total_dep) * rate
            };
            total_dep += dep;
        }
        num(dep)
    }

    pub(crate) fn fin_ddb(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
        if args.len() < 4 || args.len() > 5 {
            return Err(FormulaError::Value);
        }
        let cost = self.eval_arg_num(&args[0], ctx, depth)?;
        let salvage = self.eval_arg_num(&args[1], ctx, depth)?;
        let life = self.eval_arg_num(&args[2], ctx, depth)?;
        let period = self.eval_arg_num(&args[3], ctx, depth)?.trunc() as usize;
        let factor = match args.get(4) {
            Some(a) => self.eval_arg_num(a, ctx, depth)?,
            None => 2.0,
        };
        if cost < 0.0
            || salvage < 0.0
            || life <= 0.0
            || !(1..=100_000).contains(&period)
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
        num(dep)
    }

    pub(crate) fn fin_syd(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<EvaluatedCellValue, FormulaError> {
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
        num((cost - salvage) * (life - per + 1.0) * 2.0 / (life * (life + 1.0)))
    }
}
