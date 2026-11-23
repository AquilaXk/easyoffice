//! Odd-period bond functions: `ODDFPRICE`, `ODDFYIELD`, `ODDLPRICE`, `ODDLYIELD` (PD-201, PD-323).
//!
//! Reference implementation: `packages/engine-formula/src/functions/financial/{oddfprice,oddfyield,
//! oddlprice,oddlyield}/index.ts` and `src/basics/financial.ts` of the Univer formula engine, commit
//! `5a06a7842c8e13fa0646d0733dc3662cf4b330b9`, Apache-2.0, Copyright 2023-present DreamNum Co., Ltd.;
//! `coup_number` and `compute_oddfprice` (`base/src/functions/financial/financial_bonds.rs` lines
//! 476-616) of IronCalc, commit `f93662006b8fb1c401dc870bbda26c5d60ac6037`, MIT OR Apache-2.0,
//! Copyright 2023 EqualTo GmbH, Copyright 2023-present IronCalc contributors, Copyright 2026-present
//! IronCalc GmbH, itself derived from `ExcelFinancialFunctions` (`oddbonds.fs`), MIT License,
//! Copyright (c) 2013 Luca Bolognese. See `THIRD_PARTY_NOTICES.md`.
//!
//! The last-period functions keep the earlier reference's date-object walk (`shift_months_raw` /
//! `set_year_raw`), while first-period odd bonds use 1900-clamped month arithmetic via
//! [`checked_add_months_1900`]. Every walk is bounded by `check_coupon_span` and
//! [`MAX_COUPON_PERIODS`].

use crate::functions_bond::solve_by_guess;
use crate::functions_coupon::{
    check_coupon_span, check_frequency_basis, checked_add_months_1900, coupon_days,
    coupon_schedule, days_by_basis, is_last_day_of_month_1900, month_parts, set_year_raw,
    shift_months_raw, year_of, MAX_COUPON_PERIODS,
};
use crate::functions_fin_args::ArgRule;
use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

type Fallible<T> = Result<T, FormulaError>;

/// Days from `start` to `end`, 0 unless `start` is before `end`.
fn positive_days(start: i64, end: i64, basis: i64) -> f64 {
    if start < end {
        days_by_basis(start, end, basis).0
    } else {
        0.0
    }
}

/// Whole quasi-coupon periods strictly between `settlement` and `first_coupon` (`coup_number`
/// with `is_whole_number = true`).
fn coup_number_whole(first_coupon: i64, settlement: i64, months: i64) -> Fallible<f64> {
    let end_of_month_temp = is_last_day_of_month_1900(first_coupon);
    let (_, month, day, days_in_first_month) = month_parts(first_coupon);
    let end_of_month = if !end_of_month_temp && month != 2 && day > 28 && day < days_in_first_month
    {
        is_last_day_of_month_1900(settlement)
    } else {
        end_of_month_temp
    };
    let start_date = checked_add_months_1900(settlement, 0, end_of_month)?;
    let mut coupons = if settlement < start_date { 1.0 } else { 0.0 };
    let mut front = checked_add_months_1900(start_date, months, end_of_month)?;
    let mut steps = 0_i64;
    while front < first_coupon {
        steps += 1;
        if steps > MAX_COUPON_PERIODS + 2 {
            return Err(FormulaError::Num);
        }
        coupons += 1.0;
        front = checked_add_months_1900(front, months, end_of_month)?;
    }
    Ok(coupons)
}

/// A bond with an odd first period, reduced to what the price depends on besides the yield: the
/// price is `redemption / g^redeem_exp + coupon * first_weight / g^first_exp
/// + sum(j = 1..=annuity_count) coupon / g^(j + annuity_offset) - coupon * accrued` with
/// `g = 1 + yield / frequency` (a short first period and a long one differ only in these terms).
struct OddFirst {
    coupon: f64,
    redemption: f64,
    frequency: f64,
    redeem_exp: f64,
    first_weight: f64,
    first_exp: f64,
    annuity_count: i64,
    annuity_offset: f64,
    accrued: f64,
}

/// The dates and rates of `ODDFPRICE` and `ODDFYIELD`.
struct OddFirstInput {
    settlement: i64,
    maturity: i64,
    issue: i64,
    first: i64,
    rate: f64,
    redemption: f64,
    frequency: i64,
    basis: i64,
}

impl OddFirst {
    /// The terms of `calculateOddFPrice` that do not depend on the yield.
    fn new(input: &OddFirstInput) -> Fallible<Self> {
        let OddFirstInput {
            settlement,
            maturity,
            issue,
            first,
            rate,
            redemption,
            frequency,
            basis,
        } = *input;
        let f = frequency as f64;
        let coupon = 100.0 * rate / f;
        let dfc = positive_days(issue, first, basis);
        let e = coupon_days(settlement, first, frequency, basis)?;
        let to_first = coupon_schedule(settlement, first, frequency)?;
        let make =
            |redeem_exp, first_weight, first_exp, annuity_count, annuity_offset, accrued| Self {
                coupon,
                redemption,
                frequency: f,
                redeem_exp,
                first_weight,
                first_exp,
                annuity_count,
                annuity_offset,
                accrued,
            };
        if dfc < e {
            let n = coupon_schedule(settlement, maturity, frequency)?.count;
            let ratio = positive_days(settlement, first, basis) / e;
            return Ok(make(
                n as f64 - 1.0 + ratio,
                dfc / e,
                ratio,
                n - 1,
                ratio,
                positive_days(issue, settlement, basis) / e,
            ));
        }
        let first_to_mat = coupon_schedule(first, maturity, frequency)?;
        if first_to_mat.previous < 0 {
            return Err(FormulaError::Num);
        }
        let n = first_to_mat.count;
        let nq = coup_number_whole(first, settlement, 12 / frequency)?;
        let dsc = if basis == 2 || basis == 3 {
            positive_days(settlement, to_first.next, basis)
        } else {
            if to_first.previous < 0 {
                return Err(FormulaError::Num);
            }
            e - days_by_basis(to_first.previous, settlement, basis).0
        };
        let ratio = dsc / e;
        let mut late = first;
        let (mut dc_sum, mut a_sum) = (0.0, 0.0);
        let nc = coupon_schedule(issue, first, frequency)?;
        if nc.previous < 0 {
            return Err(FormulaError::Num);
        }
        for index in (1..=nc.count).rev() {
            let early = checked_add_months_1900(late, -12 / frequency, false)?;
            let nl = if basis == 1 {
                positive_days(early, late, basis)
            } else {
                e
            };
            let dc = if index > 1 {
                nl
            } else {
                positive_days(issue, late, basis)
            };
            dc_sum += dc / nl;
            a_sum += positive_days(issue.max(early), settlement.min(late), basis) / nl;
            late = early;
        }
        Ok(make(
            n as f64 + nq + ratio,
            dc_sum,
            nq + ratio,
            n,
            nq + ratio,
            a_sum,
        ))
    }

    /// Price per 100 of face value at yield `yld`.
    fn price(&self, yld: f64) -> f64 {
        let growth = 1.0 + yld / self.frequency;
        let mut price = self.redemption / growth.powf(self.redeem_exp)
            + self.coupon * self.first_weight / growth.powf(self.first_exp);
        for j in 1..=self.annuity_count {
            price += self.coupon / growth.powf(j as f64 + self.annuity_offset);
        }
        price - self.coupon * self.accrued
    }
}

/// Coupon periods (and parts) counted the way the last-period functions do.
struct OddLast {
    settlement: i64,
    maturity: i64,
    last_interest: i64,
    rate: f64,
    redemption: f64,
    frequency: i64,
    basis: i64,
}

impl OddLast {
    fn coupon_date(&self) -> i64 {
        let step = 12 / self.frequency;
        let mut date = set_year_raw(self.last_interest, year_of(self.maturity));
        if date > self.maturity {
            date = set_year_raw(date, year_of(date) - 1);
        }
        while date < self.maturity {
            date = shift_months_raw(date, step);
        }
        date
    }

    /// Fraction of coupon periods between `start` and `end`, counted from `coupon`.
    fn fraction(&self, start: i64, end: i64, coupon: i64) -> Fallible<f64> {
        let step = 12 / self.frequency;
        let (frequency, basis) = (self.frequency, self.basis);
        let mut date = set_year_raw(coupon, year_of(start));
        if date < start {
            date = set_year_raw(date, year_of(date) + 1);
        }
        while date > start {
            date = shift_months_raw(date, -step);
        }
        let early = date;
        let late = shift_months_raw(date, step);
        if late >= end {
            return Ok(
                days_by_basis(start, end, basis).0 / coupon_days(early, late, frequency, basis)?
            );
        }
        let mut result =
            days_by_basis(start, late, basis).0 / coupon_days(early, late, frequency, basis)?;
        let mut early_walk = late;
        let mut late_walk = shift_months_raw(late, step);
        while late_walk < end {
            early_walk = shift_months_raw(early_walk, step);
            late_walk = shift_months_raw(late_walk, step);
            result += 1.0;
        }
        result += days_by_basis(early_walk, end, basis).0
            / coupon_days(early_walk, late_walk, frequency, basis)?;
        Ok(result)
    }

    /// `(fAi, fDCi, fDSCi)` of the reference formulas.
    fn fractions(&self) -> Fallible<(f64, f64, f64)> {
        let coupon = self.coupon_date();
        Ok((
            self.fraction(self.last_interest, self.settlement, coupon)?,
            self.fraction(self.last_interest, self.maturity, coupon)?,
            self.fraction(self.settlement, self.maturity, coupon)?,
        ))
    }

    fn price(&self, yld: f64) -> Fallible<f64> {
        let (ai, dci, dsci) = self.fractions()?;
        let f = self.frequency as f64;
        Ok(
            (self.redemption * f + 100.0 * self.rate * (dci - ai * (1.0 + yld * dsci / f)))
                / (yld * dsci + f),
        )
    }

    fn yield_for(&self, price: f64) -> Fallible<f64> {
        let (ai, dci, dsci) = self.fractions()?;
        let f = self.frequency as f64;
        Ok(
            (f * (self.redemption - price) + 100.0 * self.rate * (dci - ai))
                / (dsci * price + 100.0 * self.rate * ai * dsci / f),
        )
    }
}

impl SheetEngine {
    pub(crate) fn eval_odd_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<EvaluatedCellValue> {
        let first_period = matches!(name, "ODDFPRICE" | "ODDFYIELD");
        let is_yield = matches!(name, "ODDFYIELD" | "ODDLYIELD");
        let (min, max) = if first_period { (8, 9) } else { (7, 8) };
        let a = self.fin_args(args, (min, max), ArgRule::BlankNa, ctx, depth)?;
        let (settlement, maturity) = (a.date(0)?, a.date(1)?);
        let (issue, first) = if first_period {
            (a.date(2)?, a.date(3)?)
        } else {
            (0, a.date(2)?)
        };
        let at = if first_period { 4 } else { 3 };
        let (rate, yld_or_price, redemption) = (a.num(at)?, a.num(at + 1)?, a.num(at + 2)?);
        let (frequency, basis) =
            check_frequency_basis(a.int_or(at + 3, 0.0)?, a.int_or(at + 4, 0.0)?)?;
        let value_bad = if is_yield {
            yld_or_price <= 0.0
        } else {
            yld_or_price < 0.0
        };
        if rate < 0.0 || value_bad || redemption <= 0.0 {
            return Err(FormulaError::Num);
        }
        let result = if first_period {
            let ordered = maturity > first && first > settlement && settlement > issue;
            if !ordered {
                return Err(FormulaError::Num);
            }
            check_coupon_span(issue, maturity, frequency)?;
            let first_sched = coupon_schedule(first, maturity, frequency)?;
            if first_sched.previous != first
                || coupon_schedule(issue, maturity, frequency)?.previous < 0
            {
                return Err(FormulaError::Num);
            }
            let odd = OddFirst::new(&OddFirstInput {
                settlement,
                maturity,
                issue,
                first,
                rate,
                redemption,
                frequency,
                basis,
            })?;
            if is_yield {
                let days = days_by_basis(settlement, maturity, basis).0;
                let pr = yld_or_price;
                let guess = (rate * days * 100.0 - (pr - 100.0))
                    / ((pr - 100.0) * 0.25 * (1.0 + 2.0 * days) + days * 100.0);
                solve_by_guess(guess, &|yld| Ok(pr - odd.price(yld)))?
            } else {
                odd.price(yld_or_price)
            }
        } else {
            let last_interest = first;
            let ordered = maturity > settlement
                && settlement > last_interest
                && settlement != 60
                && last_interest > 0;
            if !ordered || coupon_schedule(last_interest, maturity, frequency)?.previous < 0 {
                return Err(FormulaError::Num);
            }
            check_coupon_span(last_interest, maturity, frequency)?;
            let odd = OddLast {
                settlement,
                maturity,
                last_interest,
                rate,
                redemption,
                frequency,
                basis,
            };
            if is_yield {
                odd.yield_for(yld_or_price)?
            } else {
                odd.price(yld_or_price)?
            }
        };
        Ok(EvaluatedCellValue::Number(result))
    }
}
