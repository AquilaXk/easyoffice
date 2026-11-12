//! Bond and security functions: `PRICE`, `YIELD`, `ACCRINT`, `ACCRINTM`, `DISC`, `PRICEDISC`,
//! `PRICEMAT`, `RECEIVED`, `YIELDDISC`, `YIELDMAT`, `TBILLEQ`, `TBILLPRICE`, `TBILLYIELD` (PD-201).
//!
//! Reference implementation: `packages/engine-formula/src/functions/financial/{price,yield,
//! accrint,accrintm,disc,pricedisc,pricemat,received,yielddisc,yieldmat,tbilleq,tbillprice,
//! tbillyield}/index.ts` and `src/basics/financial.ts` (`calculatePrice`, `getResultByGuessIterF`)
//! of the Univer formula engine, commit `5a06a7842c8e13fa0646d0733dc3662cf4b330b9`, Apache-2.0,
//! Copyright 2023-present DreamNum Co., Ltd. See `THIRD_PARTY_NOTICES.md`.
//!
//! The yield solvers stop after [`YIELD_MAX_STEPS`] secant steps and the guess search after
//! [`GUESS_BRACKET_STEPS`] widenings; a solver that does not converge gives `#NUM!`.

use crate::functions_coupon::{
    add_months, check_coupon_span, check_frequency_basis, coupon_days, coupon_days_from_start,
    coupon_days_in, coupon_schedule, days_by_basis, days_in_year, year_days, Schedule,
};
use crate::functions_fin_args::ArgRule;
use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

type Fallible<T> = Result<T, FormulaError>;

/// Secant steps of `YIELD`.
const YIELD_MAX_STEPS: usize = 100;
/// Newton steps of the odd-coupon yield solvers before the bracket search starts.
pub(crate) const NEWTON_MAX_STEPS: usize = 100;
/// Widenings and bisections of the bracket search.
pub(crate) const GUESS_BRACKET_STEPS: usize = 60;
const SOLVER_EPS: f64 = 1e-7;

fn num(v: f64) -> Fallible<EvaluatedCellValue> {
    Ok(EvaluatedCellValue::Number(v))
}

/// Newton iteration with a numeric derivative from `guess`; a run that diverges or does not
/// settle falls back to bracketing and bisection (`getResultByGuessIterF`).
pub(crate) fn solve_by_guess(guess: f64, f: &dyn Fn(f64) -> Fallible<f64>) -> Fallible<f64> {
    let (mut x, mut eps, mut steps) = (guess, 1.0, 0);
    while eps > SOLVER_EPS && steps < NEWTON_MAX_STEPS {
        let slope = (f(x + SOLVER_EPS)? - f(x - SOLVER_EPS)?) / (2.0 * SOLVER_EPS);
        let next = x - f(x)? / slope;
        steps += 1;
        eps = (next - x).abs();
        x = next;
    }
    if x.is_nan() || x.is_infinite() || steps == NEWTON_MAX_STEPS {
        return bracket_and_bisect(guess, f);
    }
    Ok(x)
}

fn bracket_and_bisect(guess: f64, f: &dyn Fn(f64) -> Fallible<f64>) -> Fallible<f64> {
    const LOWEST: f64 = -1.0;
    const WIDEN: f64 = 1.6;
    if guess <= LOWEST || guess >= f64::MAX {
        return Err(FormulaError::Num);
    }
    let mut low = if guess - 0.01 <= LOWEST {
        LOWEST + SOLVER_EPS
    } else {
        guess - 0.01
    };
    let mut high = guess + 0.01;
    let (mut begin, mut end) = (low, high);
    for i in 0..GUESS_BRACKET_STEPS {
        begin = if low <= LOWEST {
            LOWEST + SOLVER_EPS
        } else {
            low
        };
        end = high;
        let product = f(begin)? * f(end)?;
        if product <= 0.0 {
            break;
        }
        if product.is_nan() || i == GUESS_BRACKET_STEPS - 1 {
            return Err(FormulaError::Num);
        }
        low = begin + WIDEN * (begin - end);
        high = end + WIDEN * (end - begin);
    }
    let mut f_begin = f(begin)?;
    if f_begin.abs() < SOLVER_EPS || f(end)?.abs() < SOLVER_EPS {
        return Err(FormulaError::Num);
    }
    let mut steps = 0;
    loop {
        let mid = begin + (end - begin) / 2.0;
        let f_mid = f(mid)?;
        if f_begin * f_mid < 0.0 {
            end = mid;
        } else {
            begin = mid;
        }
        f_begin = f(begin)?;
        steps += 1;
        if f_mid.abs() <= SOLVER_EPS || steps >= GUESS_BRACKET_STEPS {
            return Ok(mid);
        }
    }
}

/// What a bond price depends on besides the rate and the yield: the coupons left (`n`), the days
/// in the coupon period (`e`) and the days since the last coupon (`a`). The yield solver prices
/// the same bond many times, so these are computed once.
pub(crate) struct BondTerms {
    n: i64,
    e: f64,
    a: f64,
    frequency: f64,
}

impl BondTerms {
    pub(crate) fn new(sched: &Schedule, settlement: i64, frequency: i64, basis: i64) -> Self {
        Self {
            n: sched.count,
            e: coupon_days_in(sched, settlement, frequency, basis),
            a: coupon_days_from_start(sched, settlement, basis),
            frequency: frequency as f64,
        }
    }

    /// Price per 100 of face value (`calculatePrice`).
    pub(crate) fn price(&self, rate: f64, yld: f64, redemption: f64) -> f64 {
        let (n, e, a, f) = (self.n, self.e, self.a, self.frequency);
        let coupon = 100.0 * rate / f;
        if n == 1 {
            return (coupon + redemption) / (yld / f * (e - a) / e + 1.0) - coupon * a / e;
        }
        let growth = 1.0 + yld / f;
        let first = (e - a) / e;
        let mut price = redemption / growth.powf(n as f64 - 1.0 + first);
        for k in 1..=n {
            price += coupon / growth.powf(k as f64 - 1.0 + first);
        }
        price - coupon * a / e
    }
}

impl SheetEngine {
    pub(crate) fn eval_bond_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<EvaluatedCellValue> {
        match name {
            "PRICE" | "YIELD" => self.fin_price_yield(name == "YIELD", args, ctx, depth),
            "ACCRINT" => self.fin_accrint(args, ctx, depth),
            "ACCRINTM" => self.fin_accrintm(args, ctx, depth),
            "DISC" | "PRICEDISC" | "RECEIVED" | "YIELDDISC" => {
                self.fin_discount_family(name, args, ctx, depth)
            }
            "PRICEMAT" | "YIELDMAT" => self.fin_at_maturity(name == "YIELDMAT", args, ctx, depth),
            "TBILLEQ" | "TBILLPRICE" | "TBILLYIELD" => self.fin_tbill(name, args, ctx, depth),
            _ => Err(FormulaError::Name),
        }
    }

    fn fin_price_yield(
        &self,
        is_yield: bool,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<EvaluatedCellValue> {
        let a = self.fin_args(args, (6, 7), ArgRule::BlankNa, ctx, depth)?;
        let (settlement, maturity) = (a.date(0)?, a.date(1)?);
        let (rate, second, redemption) = (a.num(2)?, a.num(3)?, a.num(4)?);
        let (frequency, basis) = check_frequency_basis(a.int_or(5, 0.0)?, a.int_or(6, 0.0)?)?;
        let second_bad = if is_yield {
            second <= 0.0
        } else {
            second < 0.0
        };
        if rate < 0.0
            || second_bad
            || redemption <= 0.0
            || settlement >= maturity
            || settlement <= 0
        {
            return Err(FormulaError::Num);
        }
        let sched = coupon_schedule(settlement, maturity, frequency)?;
        if sched.previous < 0 {
            return Err(FormulaError::Num);
        }
        let terms = BondTerms::new(&sched, settlement, frequency, basis);
        if !is_yield {
            return num(terms.price(rate, second, redemption));
        }
        if sched.count > 1 {
            let price_at = |yld: f64| terms.price(rate, yld, redemption);
            let mut yld = if rate == 0.0 { 0.01 } else { rate };
            let mut eps = price_at(yld) - second;
            for _ in 0..YIELD_MAX_STEPS {
                if eps.abs() <= SOLVER_EPS {
                    break;
                }
                let price = price_at(1.01 * yld);
                yld += -eps / (price - second - eps) * yld * 0.01;
                eps = price_at(yld) - second;
            }
            return num(yld);
        }
        let f = frequency as f64;
        let dsr = days_by_basis(settlement, maturity, basis).0;
        let temp = second / 100.0 + terms.a / terms.e * rate / f;
        num(((redemption / 100.0 + rate / f) - temp) / temp * f * terms.e / dsr)
    }

    fn fin_accrint(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<EvaluatedCellValue> {
        if !(6..=8).contains(&args.len()) {
            return Err(FormulaError::Value);
        }
        let a = self.fin_args(
            &args[..args.len().min(7)],
            (6, 7),
            ArgRule::Strict,
            ctx,
            depth,
        )?;
        let calc_method = match args.get(7) {
            Some(arg) => self.eval_arg_num(arg, ctx, depth)? != 0.0,
            None => true,
        };
        let (issue, first, settlement) = (a.date(0)?, a.date(1)?, a.date(2)?);
        let (rate, par) = (a.num(3)?, a.num(4)?);
        let (frequency, basis) = check_frequency_basis(a.int_or(5, 0.0)?, a.int_or(6, 0.0)?)?;
        if rate <= 0.0 || par <= 0.0 || issue >= settlement || issue == 0 {
            return Err(FormulaError::Num);
        }
        let accrued = accrued_periods(issue, first, settlement, frequency, basis, calc_method)?;
        num(par * rate / frequency as f64 * accrued)
    }

    fn fin_accrintm(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<EvaluatedCellValue> {
        let a = self.fin_args(args, (4, 5), ArgRule::Strict, ctx, depth)?;
        let (issue, settlement) = (a.date(0)?, a.date(1)?);
        let (rate, par, basis) = (a.num(2)?, a.num(3)?, a.int_or(4, 0.0)?);
        if rate <= 0.0 || par <= 0.0 || !(0.0..=4.0).contains(&basis) || issue > settlement {
            return Err(FormulaError::Num);
        }
        if issue == settlement {
            return num(0.0);
        }
        let (days, year) = days_by_basis(issue, settlement, basis as i64);
        num(par * rate * days / year)
    }

    /// `DISC`, `PRICEDISC`, `RECEIVED` and `YIELDDISC`: (settlement, maturity, value, value, basis).
    fn fin_discount_family(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<EvaluatedCellValue> {
        let a = self.fin_args(
            args,
            (4, 5),
            if name == "DISC" {
                ArgRule::Strict
            } else {
                ArgRule::BlankNa
            },
            ctx,
            depth,
        )?;
        let (settlement, maturity) = (a.date(0)?, a.date(1)?);
        let (first, second, basis) = (a.num(2)?, a.num(3)?, a.int_or(4, 0.0)?);
        if first <= 0.0 || second <= 0.0 || !(0.0..=4.0).contains(&basis) || settlement >= maturity
        {
            return Err(FormulaError::Num);
        }
        let (dsm, year) = days_by_basis(settlement, maturity, basis as i64);
        match name {
            "DISC" => num(((second - first) / second) * (year / dsm)),
            "PRICEDISC" => num(second - first * second * dsm / year),
            "YIELDDISC" => num(((second / first) - 1.0) / (dsm / year)),
            _ => {
                let received = first / (1.0 - (second * dsm / year));
                if received < 0.0 {
                    Err(FormulaError::Num)
                } else {
                    num(received)
                }
            }
        }
    }

    /// `PRICEMAT` and `YIELDMAT`: (settlement, maturity, issue, rate, yield or price, basis).
    fn fin_at_maturity(
        &self,
        is_yield: bool,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<EvaluatedCellValue> {
        let a = self.fin_args(args, (5, 6), ArgRule::BlankNa, ctx, depth)?;
        let (settlement, maturity, issue) = (a.date(0)?, a.date(1)?, a.date(2)?);
        let (rate, last, basis) = (a.num(3)?, a.num(4)?, a.int_or(5, 0.0)?);
        let last_bad = if is_yield { last <= 0.0 } else { last < 0.0 };
        if rate < 0.0
            || last_bad
            || !(0.0..=4.0).contains(&basis)
            || !(maturity > settlement && settlement > issue)
        {
            return Err(FormulaError::Num);
        }
        let basis = basis as i64;
        let b = year_days(settlement, basis);
        let dsm = days_by_basis(settlement, maturity, basis).0;
        let dim = days_by_basis(issue, maturity, basis).0;
        let accrued = days_by_basis(issue, settlement, basis).0;
        num(if is_yield {
            ((1.0 + dim / b * rate) / (last / 100.0 + accrued / b * rate) - 1.0) / (dsm / b)
        } else {
            (100.0 + dim / b * rate * 100.0) / (1.0 + dsm / b * last) - accrued / b * rate * 100.0
        })
    }

    /// `TBILLEQ`, `TBILLPRICE`, `TBILLYIELD`: (settlement, maturity, discount or price).
    fn fin_tbill(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<EvaluatedCellValue> {
        let a = self.fin_args(args, (3, 3), ArgRule::BlankNa, ctx, depth)?;
        let (settlement, maturity) = (a.date(0)?, a.date(1)?);
        let value = a.num(2)?;
        if value <= 0.0 || settlement >= maturity {
            return Err(FormulaError::Num);
        }
        let dsm = (maturity - settlement) as f64;
        let year = days_in_year(settlement);
        if dsm > year {
            return Err(FormulaError::Num);
        }
        let price = 100.0 * (1.0 - value * dsm / 360.0);
        match name {
            "TBILLPRICE" if price < 0.0 => Err(FormulaError::Num),
            "TBILLPRICE" => num(price),
            "TBILLYIELD" => num((100.0 - value) / value * 360.0 / dsm),
            _ => {
                let mut eq = 365.0 * value / (360.0 - value * dsm);
                if dsm > 182.0 {
                    let fraction = if settlement == 0 {
                        dsm / year
                    } else {
                        dsm / 365.0
                    };
                    eq = (-fraction
                        + (fraction * fraction - (fraction * 2.0 - 1.0) * (1.0 - 100.0 / price))
                            .sqrt())
                        / (fraction - 0.5);
                    if eq.is_nan() {
                        return Err(FormulaError::Num);
                    }
                }
                if eq < 0.0 {
                    Err(FormulaError::Num)
                } else {
                    num(eq)
                }
            }
        }
    }
}

/// Accrued interest in coupon periods from the issue date to `settlement` (`ACCRINT`).
fn accrued_periods(
    issue: i64,
    first: i64,
    settlement: i64,
    frequency: i64,
    basis: i64,
    calc_method: bool,
) -> Fallible<f64> {
    if coupon_schedule(issue, first, frequency)?.previous <= 0 {
        return Ok(0.0);
    }
    let previous = coupon_schedule(settlement, first, frequency)?.previous;
    let months = 12 / frequency;
    let first_is_month_end = crate::functions_coupon::is_month_end_serial(first);
    let walk_back = |from: i64| add_months(from, -months, first_is_month_end);
    check_coupon_span(issue, first.max(settlement), frequency)?;
    let mut coupon = walk_back(first);
    if settlement > first && calc_method {
        coupon = first;
        while coupon < settlement {
            coupon = add_months(coupon, months, first_is_month_end);
        }
    }
    let mut from = issue.max(coupon);
    let mut days = days_by_basis(from, settlement, basis).0;
    if previous >= issue {
        days = days_by_basis(from, settlement, if basis == 0 { 0 } else { 4 }).0;
    }
    if settlement < from {
        days = -days;
    }
    let mut period_days = coupon_days(coupon, first, frequency, basis)?;
    let mut accrued = days / period_days;
    let mut start = coupon;
    while start > issue {
        let end = start;
        start = walk_back(start);
        from = issue.max(start);
        let to_end = days_by_basis(from, end, basis).0;
        if basis == 0 {
            days = if end >= from || issue <= start {
                to_end
            } else {
                -to_end
            };
            period_days = coupon_days(start, end, frequency, basis)?;
        } else {
            days = if end < from { -to_end } else { to_end };
            period_days = if basis == 3 {
                365.0 / frequency as f64
            } else {
                let span = days_by_basis(start, end, basis).0;
                if end < start {
                    -span
                } else {
                    span
                }
            };
        }
        accrued += if issue <= start {
            f64::from(u8::from(calc_method))
        } else {
            days / period_days
        };
    }
    Ok(accrued)
}
