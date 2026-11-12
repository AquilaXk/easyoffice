//! Coupon schedule and day counts (`COUPDAYBS`, `COUPDAYS`, `COUPDAYSNC`, `COUPNCD`, `COUPNUM`,
//! `COUPPCD`, `DURATION`, `MDURATION`) and the helpers the bond functions share (PD-201).
//!
//! Reference implementation: `packages/engine-formula/src/basics/financial.ts` (coupon schedule,
//! `calculateCoup*`, `calculateDuration`, `getDateSerialNumberByMonths`, `getCouponsNumber`),
//! `src/basics/date.ts` (`getTwoDateDaysByBasis` and the 30/360, actual and European counts) and
//! `src/functions/financial/{coupdaybs,coupdays,coupdaysnc,coupncd,coupnum,couppcd,duration,mduration}`
//! of the Univer formula engine, commit `5a06a7842c8e13fa0646d0733dc3662cf4b330b9`, Apache-2.0,
//! Copyright 2023-present DreamNum Co., Ltd. See `THIRD_PARTY_NOTICES.md`.
//!
//! Differences from the original: only the 1900 date system exists (no 1904 branches), dates are
//! whole serial numbers (`i64`, the fraction is dropped on input as the spreadsheet does), the
//! coupon walks are bounded by [`MAX_COUPON_PERIODS`] (over the limit the result is `#NUM!`), and a
//! month step keeps the reference's date-object behaviour (a day past the end of the month rolls
//! into the next month) through [`add_months_js`].

use crate::functions_date_fin::{civil_from_serial, serial_from_civil};
use crate::functions_fin_args::ArgRule;
use crate::{EvalContext, EvaluatedCellValue, Expr, FormulaError, SheetEngine};

/// Coupon periods one call may walk (250 years of quarterly coupons). The yield solvers price a
/// bond a few hundred times, so this bounds the work of one cell; no real bond has more.
pub(crate) const MAX_COUPON_PERIODS: i64 = 1_000;

type Fallible<T> = Result<T, FormulaError>;

/// A calendar date; `d` is 0 for serial 0 (the spreadsheet's "January 0, 1900").
#[derive(Clone, Copy, PartialEq, PartialOrd)]
struct Ymd {
    y: i64,
    m: i64,
    d: i64,
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Days of month `m` (1 to 12) of year `y`.
pub(crate) fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        2 if is_leap(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn ymd(serial: i64) -> Ymd {
    if serial == 0 {
        return Ymd {
            y: 1900,
            m: 1,
            d: 0,
        };
    }
    let (y, m, d) = civil_from_serial(serial);
    Ymd { y, m, d }
}

fn serial_of(p: Ymd) -> i64 {
    if p.y == 1900 && p.m == 1 && p.d == 0 {
        0
    } else {
        serial_from_civil(p.y, p.m, p.d)
    }
}

fn is_month_end(p: Ymd) -> bool {
    p.d == days_in_month(p.y, p.m)
}

pub(crate) fn is_month_end_serial(serial: i64) -> bool {
    is_month_end(ymd(serial))
}

/// Date-object month step without the month-end rule: a day past the end of the target month
/// rolls into the following month (`setUTCMonth(getUTCMonth() + months)`).
pub(crate) fn shift_months_raw(serial: i64, months: i64) -> i64 {
    let p = ymd(serial);
    let index = p.y * 12 + p.m - 1 + months;
    serial_of(Ymd {
        y: index.div_euclid(12),
        m: index.rem_euclid(12) + 1,
        d: 1,
    }) + p.d
        - 1
}

/// Date-object year set (`setUTCFullYear`): February 29 of a common year rolls into March.
pub(crate) fn set_year_raw(serial: i64, year: i64) -> i64 {
    let p = ymd(serial);
    serial_of(Ymd {
        y: year,
        m: p.m,
        d: 1,
    }) + p.d
        - 1
}

/// `(year, month, day, days in the month)` of a serial number.
pub(crate) fn month_parts(serial: i64) -> (i64, i64, i64, i64) {
    let p = ymd(serial);
    (p.y, p.m, p.d, days_in_month(p.y, p.m))
}

pub(crate) fn year_of(serial: i64) -> i64 {
    ymd(serial).y
}

/// Rejects a stretch of dates that would take more than [`MAX_COUPON_PERIODS`] coupon steps.
pub(crate) fn check_coupon_span(start: i64, end: i64, frequency: i64) -> Fallible<()> {
    // a month has at least 28 days
    if (end - start) / 28 / (12 / frequency) > MAX_COUPON_PERIODS {
        return Err(FormulaError::Num);
    }
    Ok(())
}

/// `(days, days in the year)` between two serial numbers under `basis` (0 to 4); the day count is
/// never negative. Basis 0 is 30/360 (US), 1 actual/actual, 2 actual/360, 3 actual/365, 4 30E/360.
pub(crate) fn days_by_basis(start: i64, end: i64, basis: i64) -> (f64, f64) {
    match basis {
        0 => days_30_360(start, end, true),
        1 => days_actual(start, end),
        2 => ((end - start).abs() as f64, 360.0),
        4 => days_30_360(start, end, false),
        _ => ((end - start).abs() as f64, 365.0),
    }
}

fn days_30_360(start: i64, end: i64, us: bool) -> (f64, f64) {
    let (s, mut e) = (ymd(start), ymd(end));
    let mut sd = s.d;
    if us {
        let start_feb_end = s.m == 2 && s.d == days_in_month(s.y, 2);
        if start_feb_end || sd == 31 {
            sd = 30;
        }
        if e.m == 2 && e.d == days_in_month(e.y, 2) && start_feb_end {
            e.d = 30;
        } else if e.d == 31 {
            if sd < 30 {
                e = ymd(end + 1);
            } else {
                e.d = 30;
            }
        }
    } else {
        sd = sd.min(30);
        e.d = e.d.min(30);
    }
    let forward = end >= start;
    let years = (e.y - s.y) * 360;
    let start_month = if forward { 30 - sd } else { -sd };
    let end_month = if forward { e.d } else { e.d - 30 };
    let middle = (if forward {
        e.m - s.m - 1
    } else {
        e.m - s.m + 1
    }) * 30;
    (
        (years + start_month + end_month + middle).abs() as f64,
        360.0,
    )
}

fn days_actual(start: i64, end: i64) -> (f64, f64) {
    let (s, e) = (ymd(start), ymd(end));
    let total = (end - start).abs() as f64;
    // the same day and month in a later year: a whole number of years of their real length
    if end >= start && e.y > s.y && s.m == e.m && s.d == e.d {
        return (total, total / (e.y - s.y) as f64);
    }
    let (first_year, last_year) = if e.y < s.y { (e.y, s.y) } else { (s.y, e.y) };
    let mut first = serial_from_civil(first_year, 1, 1);
    let last = serial_from_civil(last_year, 12, 31);
    // the spreadsheet counts 1900 as 365 days, but its serial numbers span 366
    if first_year == 1900 {
        first += 1;
    }
    let years = (e.y - s.y).abs() + 1;
    (total, (last - first + 1) as f64 / years as f64)
}

/// Days in the calendar year of `serial`; the spreadsheet counts 1900 as a leap year.
pub(crate) fn days_in_year(serial: i64) -> f64 {
    let y = ymd(serial).y;
    if is_leap(y) || y == 1900 {
        366.0
    } else {
        365.0
    }
}

/// Days in the year of `serial` under `basis` (the "B" of the bond formulas).
pub(crate) fn year_days(serial: i64, basis: i64) -> f64 {
    match basis {
        1 => days_in_year(serial),
        3 => 365.0,
        _ => 360.0,
    }
}

/// The reference's date-object month step: from a month end the result is a month end,
/// otherwise a day past the end of the target month rolls into the following month.
pub(crate) fn add_months_js(serial: i64, months: i64) -> i64 {
    let p = ymd(serial);
    let index = p.y * 12 + p.m - 1 + months;
    let (y, m) = (index.div_euclid(12), index.rem_euclid(12) + 1);
    if is_month_end(p) {
        return serial_of(Ymd {
            y,
            m,
            d: days_in_month(y, m),
        });
    }
    serial_of(Ymd { y, m, d: 1 }) + p.d - 1
}

/// [`add_months_js`], then optionally the last day of the resulting month.
pub(crate) fn add_months(serial: i64, months: i64, last_day: bool) -> i64 {
    let moved = add_months_js(serial, months);
    if !last_day {
        return moved;
    }
    let p = ymd(moved);
    serial_of(Ymd {
        d: days_in_month(p.y, p.m),
        ..p
    })
}

/// A coupon date that remembers whether the maturity sits on a month end.
#[derive(Clone, Copy)]
struct Coupon {
    at: Ymd,
    month_end: bool,
}

impl Coupon {
    fn of(serial: i64) -> Self {
        let at = ymd(serial);
        Self {
            at,
            month_end: is_month_end(at),
        }
    }

    /// `months` later (negative: earlier); the maturity's day and month-end rule is kept.
    fn shifted(self, months: i64) -> Self {
        let index = self.at.y * 12 + self.at.m - 1 + months;
        let (y, m) = (index.div_euclid(12), index.rem_euclid(12) + 1);
        let max_day = days_in_month(y, m);
        let d = if self.month_end {
            max_day
        } else {
            self.at.d.min(max_day)
        };
        Self {
            at: Ymd { y, m, d },
            month_end: self.month_end,
        }
    }
}

/// The coupon dates around the settlement date and the coupons left to pay.
pub(crate) struct Schedule {
    pub(crate) previous: i64,
    pub(crate) next: i64,
    pub(crate) count: i64,
}

pub(crate) fn coupon_schedule(
    settlement: i64,
    maturity: i64,
    frequency: i64,
) -> Fallible<Schedule> {
    let s = ymd(settlement);
    let m = ymd(maturity);
    let period = 12 / frequency;
    if ((m.y - s.y) * 12 + m.m - s.m) / period > MAX_COUPON_PERIODS {
        return Err(FormulaError::Num);
    }
    let mut candidate = Coupon {
        at: Ymd { y: s.y, ..m },
        month_end: is_month_end(m),
    };
    if candidate.at < s {
        candidate.at.y += 1;
    }
    while candidate.at > s {
        candidate = candidate.shifted(-period);
    }
    let previous = serial_of(candidate.at);
    let next = serial_of(candidate.shifted(period).at);
    let mut count = 0;
    let mut walk = Coupon::of(maturity);
    while walk.at > s {
        walk = walk.shifted(-period);
        count += 1;
    }
    Ok(Schedule {
        previous,
        next,
        count,
    })
}

pub(crate) fn coupon_days_from_start(sched: &Schedule, settlement: i64, basis: i64) -> f64 {
    days_by_basis(sched.previous, settlement, basis).0
}

/// Days in the coupon period (the "E" of the bond formulas) for a schedule already at hand.
pub(crate) fn coupon_days_in(sched: &Schedule, settlement: i64, frequency: i64, basis: i64) -> f64 {
    match basis {
        1 if sched.previous < 0 && frequency == 1 => 365.0,
        1 if settlement == 0 && frequency == 2 && sched.previous < 0 => 184.0,
        1 => (sched.next - sched.previous) as f64,
        3 => 365.0 / frequency as f64,
        _ => 360.0 / frequency as f64,
    }
}

/// Days in the coupon period (the "E" of the bond formulas).
pub(crate) fn coupon_days(
    settlement: i64,
    maturity: i64,
    frequency: i64,
    basis: i64,
) -> Fallible<f64> {
    if basis == 1 {
        let sched = coupon_schedule(settlement, maturity, frequency)?;
        return Ok(coupon_days_in(&sched, settlement, frequency, basis));
    }
    Ok(coupon_days_in(
        &Schedule {
            previous: 0,
            next: 0,
            count: 0,
        },
        settlement,
        frequency,
        basis,
    ))
}

/// Macaulay duration in years (`DURATION`).
fn duration(
    settlement: i64,
    maturity: i64,
    coupon: f64,
    yld: f64,
    frequency: i64,
    basis: i64,
) -> Fallible<f64> {
    let sched = coupon_schedule(settlement, maturity, frequency)?;
    let from_start = coupon_days_from_start(&sched, settlement, basis);
    let period_days = coupon_days_in(&sched, settlement, frequency, basis);
    let f = frequency as f64;
    let offset = (period_days - from_start) / period_days - 1.0;
    let growth = yld / f + 1.0;
    let payment = coupon * 100.0 / f;
    let (mut weighted, mut total) = (0.0, 0.0);
    for i in 1..=sched.count {
        let t = i as f64 + offset;
        let pv = payment / growth.powf(t);
        weighted += t * pv;
        total += pv;
    }
    let t = sched.count as f64 + offset;
    let pv = 100.0 / growth.powf(t);
    weighted += t * pv;
    total += pv;
    Ok(weighted / total / f)
}

/// Frequency (1, 2 or 4) and basis (0 to 4) checks shared by the coupon functions.
pub(crate) fn check_frequency_basis(frequency: f64, basis: f64) -> Fallible<(i64, i64)> {
    if ![1.0, 2.0, 4.0].contains(&frequency) || !(0.0..=4.0).contains(&basis) {
        return Err(FormulaError::Num);
    }
    Ok((frequency as i64, basis as i64))
}

fn num(v: f64) -> Fallible<EvaluatedCellValue> {
    Ok(EvaluatedCellValue::Number(v))
}

impl SheetEngine {
    pub(crate) fn eval_coupon_func(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<EvaluatedCellValue> {
        if matches!(name, "DURATION" | "MDURATION") {
            return self.fin_duration(name == "MDURATION", args, ctx, depth);
        }
        let a = self.fin_args(args, (3, 4), ArgRule::Strict, ctx, depth)?;
        let (settlement, maturity) = (a.date(0)?, a.date(1)?);
        let (frequency, basis) = check_frequency_basis(a.int_or(2, 0.0)?, a.int_or(3, 0.0)?)?;
        if settlement >= maturity {
            return Err(FormulaError::Num);
        }
        let sched = coupon_schedule(settlement, maturity, frequency)?;
        match name {
            "COUPNCD" => num(sched.next as f64),
            "COUPPCD" => num(sched.previous.max(0) as f64),
            "COUPNUM" if sched.previous < 0 => Err(FormulaError::Num),
            "COUPNUM" => num(sched.count as f64),
            "COUPDAYBS" => {
                let days = coupon_days_from_start(&sched, settlement, basis);
                num(if settlement == 0 && days == 1.0 {
                    0.0
                } else {
                    days
                })
            }
            "COUPDAYSNC" => num(days_by_basis(settlement, sched.next, basis).0),
            "COUPDAYS" => num(coupon_days(settlement, maturity, frequency, basis)?),
            _ => Err(FormulaError::Name),
        }
    }

    fn fin_duration(
        &self,
        modified: bool,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Fallible<EvaluatedCellValue> {
        let a = self.fin_args(args, (5, 6), ArgRule::BlankNa, ctx, depth)?;
        let (settlement, maturity) = (a.date(0)?, a.date(1)?);
        let (coupon, yld) = (a.num(2)?, a.num(3)?);
        let (frequency, basis) = check_frequency_basis(a.int_or(4, 0.0)?, a.int_or(5, 0.0)?)?;
        if coupon < 0.0 || yld < 0.0 || settlement >= maturity || settlement <= 0 || maturity <= 366
        {
            return Err(FormulaError::Num);
        }
        let years = duration(settlement, maturity, coupon, yld, frequency, basis)?;
        num(if modified {
            years / (1.0 + yld / frequency as f64)
        } else {
            years
        })
    }
}
