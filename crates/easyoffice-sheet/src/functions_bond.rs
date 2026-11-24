//! Bond and security functions: `PRICE`, `YIELD`, `ACCRINT`, `ACCRINTM`, `DISC`, `PRICEDISC`,
//! `PRICEMAT`, `RECEIVED`, `YIELDDISC`, `YIELDMAT`, `TBILLEQ`, `TBILLPRICE`, `TBILLYIELD` (PD-201,
//! PD-323).
//!
//! Reference implementation: `packages/engine-formula/src/functions/financial/{price,yield,
//! accrintm,disc,pricedisc,pricemat,received,yielddisc,yieldmat,tbilleq,tbillprice,tbillyield}`
//! and `src/basics/financial.ts` (`calculatePrice`, `getResultByGuessIterF`) of the Univer formula
//! engine, commit `5a06a7842c8e13fa0646d0733dc3662cf4b330b9`, Apache-2.0, Copyright 2023-present
//! DreamNum Co., Ltd.; `ACCRINT` day-count algorithm (`base/src/functions/financial/accrint.rs`
//! lines 20-268) and bisection fallback (`financial_bonds.rs` lines 749-772) of IronCalc, commit
//! `f93662006b8fb1c401dc870bbda26c5d60ac6037`, MIT OR Apache-2.0, Copyright 2023 EqualTo GmbH,
//! Copyright 2023-present IronCalc contributors, Copyright 2026-present IronCalc GmbH, itself
//! derived from `ExcelFinancialFunctions` (`bonds.fs`, `daycountbasis.fs`), MIT License,
//! Copyright (c) 2013 Luca Bolognese. See `THIRD_PARTY_NOTICES.md`.
//!
//! The yield solvers stop after [`YIELD_MAX_STEPS`] secant steps and the guess search after
//! [`GUESS_BRACKET_STEPS`] widenings plus [`BISECT_MAX_STEPS`] fixed-range bisection steps; a
//! solver that does not converge gives `#NUM!`.

use crate::functions_coupon::{
    check_coupon_span, check_frequency_basis, checked_add_months_1900, coupon_days_from_start,
    coupon_days_in, coupon_schedule, days_30_360_us_psa, days_by_basis, days_in_year,
    is_last_day_of_month_1900, year_days, Schedule, MAX_COUPON_PERIODS,
};
use crate::functions_date_fin::civil_from_serial;
use crate::functions_fin_args::ArgRule;
use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

type Fallible<T> = Result<T, FormulaError>;

/// Secant steps of `YIELD`.
const YIELD_MAX_STEPS: usize = 100;
/// Newton steps of the odd-coupon yield solvers before the bracket search starts.
pub(crate) const NEWTON_MAX_STEPS: usize = 100;
/// Widenings and bisections of the bracket search.
pub(crate) const GUESS_BRACKET_STEPS: usize = 60;
/// Fallback bisection steps over `[-0.9999, 100.0]`.
const BISECT_MAX_STEPS: usize = 200;
const SOLVER_EPS: f64 = 1e-7;

fn num(v: f64) -> Fallible<EvaluatedCellValue> {
    Ok(EvaluatedCellValue::Number(v))
}

/// Newton iteration with a numeric derivative from `guess`; a run that diverges or leaves
/// `(-1, +inf)` falls back to bracketing and bisection (`getResultByGuessIterF` + `solve_yield`).
pub(crate) fn solve_by_guess(guess: f64, f: &dyn Fn(f64) -> Fallible<f64>) -> Fallible<f64> {
    if !guess.is_finite() {
        return Err(FormulaError::Num);
    }
    let (mut x, mut steps) = (guess, 0);
    while steps < NEWTON_MAX_STEPS {
        if x <= -1.0 + SOLVER_EPS {
            break;
        }
        let slope = (f(x + SOLVER_EPS)? - f(x - SOLVER_EPS)?) / (2.0 * SOLVER_EPS);
        if !slope.is_finite() || slope.abs() < 1e-15 {
            break;
        }
        let next = x - f(x)? / slope;
        steps += 1;
        if !next.is_finite() || next <= -1.0 {
            break;
        }
        let eps = (next - x).abs();
        x = next;
        if eps <= SOLVER_EPS {
            return Ok(x);
        }
    }
    bracket_and_bisect(guess, f)
}

fn bisect_range(
    mut begin: f64,
    mut end: f64,
    max_steps: usize,
    tol: f64,
    f: &dyn Fn(f64) -> Fallible<f64>,
) -> Fallible<f64> {
    let mut f_begin = f(begin)?;
    if f_begin.abs() <= SOLVER_EPS {
        return Ok(begin);
    }
    let f_end = f(end)?;
    if f_end.abs() <= SOLVER_EPS {
        return Ok(end);
    }
    if !f_begin.is_finite() || !f_end.is_finite() || f_begin * f_end > 0.0 {
        return Err(FormulaError::Num);
    }
    for _ in 0..max_steps {
        let mid = begin + (end - begin) / 2.0;
        let f_mid = f(mid)?;
        if !f_mid.is_finite() {
            return Err(FormulaError::Num);
        }
        if f_mid.abs() <= SOLVER_EPS || (end - begin).abs() < tol {
            return Ok(mid);
        }
        if f_begin * f_mid < 0.0 {
            end = mid;
        } else {
            begin = mid;
            f_begin = f_mid;
        }
    }
    Ok(begin + (end - begin) / 2.0)
}

fn bisect_wide(f: &dyn Fn(f64) -> Fallible<f64>) -> Fallible<f64> {
    let hi = 100.0_f64;
    let f_hi = f(hi)?;
    for &lo in &[-0.9999_f64, -0.99, -0.95, -0.9, -0.5, 0.0] {
        let f_lo = f(lo)?;
        if f_lo.is_finite() && f_hi.is_finite() && f_lo * f_hi <= 0.0 {
            return bisect_range(lo, hi, BISECT_MAX_STEPS, 1e-12, f);
        }
    }
    Err(FormulaError::Num)
}

fn bracket_and_bisect(guess: f64, f: &dyn Fn(f64) -> Fallible<f64>) -> Fallible<f64> {
    const LOWEST: f64 = -1.0;
    const WIDEN: f64 = 1.6;
    if guess <= LOWEST || guess >= f64::MAX {
        return bisect_wide(f);
    }
    let mut low = if guess - 0.01 <= LOWEST {
        LOWEST + SOLVER_EPS
    } else {
        guess - 0.01
    };
    let mut high = guess + 0.01;
    let (mut begin, mut end) = (low, high);
    let mut bracketed = false;
    for _ in 0..GUESS_BRACKET_STEPS {
        begin = if low <= LOWEST {
            LOWEST + SOLVER_EPS
        } else {
            low
        };
        end = high;
        let fb = f(begin)?;
        if fb.abs() <= SOLVER_EPS {
            return Ok(begin);
        }
        let fe = f(end)?;
        if fe.abs() <= SOLVER_EPS {
            return Ok(end);
        }
        let product = fb * fe;
        if product.is_finite() && product <= 0.0 {
            bracketed = true;
            break;
        }
        if product.is_nan() {
            break;
        }
        low = begin + WIDEN * (begin - end);
        high = end + WIDEN * (end - begin);
    }
    if bracketed {
        if let Ok(root) = bisect_range(begin, end, GUESS_BRACKET_STEPS, 0.0, f) {
            return Ok(root);
        }
    }
    bisect_wide(f)
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
        if rate <= 0.0 || par <= 0.0 || issue >= settlement || issue <= 0 {
            return Err(FormulaError::Num);
        }
        check_coupon_span(issue, first.max(settlement), frequency)?;
        compute_accrint(
            issue,
            first,
            settlement,
            rate,
            par,
            frequency,
            basis,
            calc_method,
        )
        .and_then(num)
    }

    fn fin_accrintm(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<EvaluatedCellValue> {
        let a = self.fin_args(args, (3, 5), ArgRule::Strict, ctx, depth)?;
        let (issue, settlement) = (a.date(0)?, a.date(1)?);
        // `par` is optional and 1000 when left out; an issue date not before the settlement is `#NUM!`.
        let (rate, par, basis) = (a.num(2)?, a.num_or(3, 1000.0)?, a.int_or(4, 0.0)?);
        if rate <= 0.0 || par <= 0.0 || !(0.0..=4.0).contains(&basis) || issue >= settlement {
            return Err(FormulaError::Num);
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

fn acc_date_diff_360_eu(start: i64, end: i64) -> f64 {
    let (sy, sm, sd0) = civil_from_serial(start);
    let (ey, em, ed0) = civil_from_serial(end);
    ((ey - sy) * 360 + (em - sm) * 30 + (ed0.min(30) - sd0.min(30))) as f64
}

fn acc_find_pcd_ncd(
    start: i64,
    end: i64,
    num_months: i64,
    return_last: bool,
) -> Fallible<(i64, i64)> {
    let mut front = start;
    let mut trailing = end;
    for _ in 0..=(MAX_COUPON_PERIODS + 1) {
        let stop = if num_months > 0 {
            front >= end
        } else {
            front <= end
        };
        if stop {
            return Ok((front, trailing));
        }
        trailing = front;
        front = checked_add_months_1900(front, num_months, return_last)?;
    }
    Err(FormulaError::Num)
}

fn acc_days_numerator(start: i64, end: i64, basis: i64) -> f64 {
    match basis {
        0 => days_30_360_us_psa(start, end, false),
        4 => acc_date_diff_360_eu(start, end),
        _ => (end - start) as f64,
    }
}

fn acc_coup_days(settl: i64, mat: i64, freq: i64, basis: i64) -> Fallible<f64> {
    match basis {
        0 | 2 | 4 => Ok(360.0 / freq as f64),
        3 => Ok(365.0 / freq as f64),
        _ => {
            let num_months = -(12 / freq);
            let (pcd, ncd) =
                acc_find_pcd_ncd(mat, settl, num_months, is_last_day_of_month_1900(mat))?;
            Ok((ncd - pcd) as f64)
        }
    }
}

/// Accrued interest (`ACCRINT`) over quasi-coupon periods anchored on `first_interest`.
#[allow(clippy::too_many_arguments)]
fn compute_accrint(
    issue: i64,
    first_interest: i64,
    settlement: i64,
    rate: f64,
    par: f64,
    frequency: i64,
    basis: i64,
    calc_method: bool,
) -> Fallible<f64> {
    let num_months = 12 / frequency;
    let freq = frequency as f64;
    let end_month_bond = is_last_day_of_month_1900(first_interest);
    let calc = if calc_method { 1.0 } else { 0.0 };

    let reg_pcd = checked_add_months_1900(first_interest, -num_months, end_month_bond)?;
    let pcd = if settlement > first_interest && calc_method {
        acc_find_pcd_ncd(first_interest, settlement, num_months, end_month_bond)?.1
    } else {
        reg_pcd
    };

    let first_date = issue.max(pcd);
    let days0 = acc_days_numerator(first_date, settlement, basis);
    let coup_days0 = acc_coup_days(reg_pcd, first_interest, frequency, basis)?;
    let mut acc = days0 / coup_days0;

    let mut front = pcd;
    let mut steps = 0_i64;
    while front > issue {
        steps += 1;
        if steps > MAX_COUPON_PERIODS + 2 {
            return Err(FormulaError::Num);
        }
        let period_end = front;
        front = checked_add_months_1900(front, -num_months, end_month_bond)?;
        let period_start = front;
        if issue <= period_start {
            acc += calc;
        } else {
            let numerator = acc_days_numerator(issue, period_end, basis);
            let denominator = match basis {
                0 => days_30_360_us_psa(period_start, period_end, true),
                2 => 360.0 / freq,
                3 => 365.0 / freq,
                4 => acc_date_diff_360_eu(period_start, period_end),
                _ => (period_end - period_start) as f64,
            };
            acc += numerator / denominator;
        }
    }

    Ok(par * rate / freq * acc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bracket_and_bisect_accepts_boundary_roots_before_product() {
        let end_tangent = |x: f64| -> Fallible<f64> { Ok((x - 0.06).powi(2) + 1e-10) };
        let root_end = bracket_and_bisect(0.05, &end_tangent).unwrap();
        assert!((root_end - 0.06).abs() < 1e-9);

        let begin_tangent = |x: f64| -> Fallible<f64> { Ok((x - 0.04).powi(2) + 1e-10) };
        let root_begin = bracket_and_bisect(0.05, &begin_tangent).unwrap();
        assert!((root_begin - 0.04).abs() < 1e-9);

        let zero_times_inf = |x: f64| -> Fallible<f64> {
            if (x - 0.06).abs() < 1e-12 {
                Ok(0.0)
            } else if x < 0.06 {
                Ok(f64::INFINITY)
            } else {
                Ok(1.0)
            }
        };
        let root_inf = bracket_and_bisect(0.05, &zero_times_inf).unwrap();
        assert!((root_inf - 0.06).abs() < 1e-9);
    }
}
