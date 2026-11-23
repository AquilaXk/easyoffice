//! Coupon schedule and day counts (`COUPDAYBS`, `COUPDAYS`, `COUPDAYSNC`, `COUPNCD`, `COUPNUM`,
//! `COUPPCD`, `DURATION`, `MDURATION`) and the helpers the bond functions share (PD-201, PD-323).
//!
//! Reference implementation: `packages/engine-formula/src/basics/financial.ts` (day counts,
//! `calculateDuration`) and `src/basics/date.ts` (`getTwoDateDaysByBasis` and the 30/360, actual
//! and European counts) of the Univer formula engine, commit
//! `5a06a7842c8e13fa0646d0733dc3662cf4b330b9`, Apache-2.0, Copyright 2023-present DreamNum Co., Ltd.;
//! calendar month arithmetic (`base/src/functions/financial/calendar.rs` lines 182-221) and
//! maturity-anchored coupon schedule / actual `COUPDAYS` / US 30/360 `COUPDAYSNC`
//! (`base/src/functions/financial/financial_bonds.rs` lines 34-58, 111-130, 205-250, 433-457,
//! 1463-1480) of IronCalc, commit `f93662006b8fb1c401dc870bbda26c5d60ac6037`, MIT OR Apache-2.0,
//! Copyright 2023 EqualTo GmbH, Copyright 2023-present IronCalc contributors, Copyright 2026-present
//! IronCalc GmbH, itself derived from `ExcelFinancialFunctions` (`bonds.fs`, `oddbonds.fs`,
//! `daycountbasis.fs`), MIT License, Copyright (c) 2013 Luca Bolognese. See `THIRD_PARTY_NOTICES.md`.
//!
//! Differences from the original: dates are whole serial numbers (`i64`, the fraction is dropped
//! on input as the spreadsheet does), all coupon walks are bounded by [`MAX_COUPON_PERIODS`]
//! (over the limit the result is `#NUM!`), regular coupon schedules clamp month ends with
//! [`unchecked_add_months_1900`] / [`is_last_day_of_month_1900`], while last-period odd bonds keep
//! [`shift_months_raw`] / [`set_year_raw`].

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

pub(crate) fn is_leap(y: i64) -> bool {
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

/// `(year, month, day)` of a serial number.
pub(crate) fn year_month_day(serial: i64) -> (i64, i64, i64) {
    let Ymd { y, m, d } = ymd(serial);
    (y, m, d)
}

fn ymd(serial: i64) -> Ymd {
    if !crate::date_system::in_date_1904() && serial == 0 {
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
    if !crate::date_system::in_date_1904() && p.y == 1900 && p.m == 1 && p.d == 0 {
        0
    } else {
        serial_from_civil(p.y, p.m, p.d)
    }
}

/// Whether `serial` is the last day of its month in the active displayed calendar (so serial 60,
/// fictional Feb 29 1900, is month-end whereas serial 59, Feb 28 1900, is not).
pub(crate) fn is_last_day_of_month_1900(serial: i64) -> bool {
    if !crate::date_system::in_date_1904() {
        if serial <= 0 {
            return false;
        }
        if serial == 60 {
            return true;
        }
        if serial == 59 {
            return false;
        }
    }
    let (y, m, d) = civil_from_serial(serial);
    d == days_in_month(y, m)
}

/// Moves `serial` by `months` using Gregorian month lengths (`EDATE` clamping, or snapping to
/// month-end when `return_last` is set). May yield negative serials before epoch `0`.
pub(crate) fn unchecked_add_months_1900(
    serial: i64,
    months: i64,
    return_last: bool,
) -> Fallible<i64> {
    let (y, m, d) = civil_from_serial(serial);
    let index = y
        .checked_mul(12)
        .and_then(|v| v.checked_add(m - 1))
        .and_then(|v| v.checked_add(months))
        .ok_or(FormulaError::Num)?;
    let (ny, nm) = (index.div_euclid(12), index.rem_euclid(12) + 1);
    if !(-9999..=9999).contains(&ny) {
        return Err(FormulaError::Num);
    }
    let last = days_in_month(ny, nm);
    let nd = if return_last { last } else { d.min(last) };
    let moved = serial_from_civil(ny, nm, nd);
    if crate::date_system::to_1900(moved) > 2_958_465 {
        return Err(FormulaError::Num);
    }
    Ok(moved)
}

/// Checked variant of [`unchecked_add_months_1900`] rejecting dates before serial `0`.
pub(crate) fn checked_add_months_1900(
    serial: i64,
    months: i64,
    return_last: bool,
) -> Fallible<i64> {
    let moved = unchecked_add_months_1900(serial, months, return_last)?;
    if moved < 0 {
        Err(FormulaError::Num)
    } else {
        Ok(moved)
    }
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

/// US (NASD) 30/360 day count using Gregorian Feb-end checks; `modify_both` applies the
/// `ModifyBothDates` variant (`COUPDAYSNC` period length and `ACCRINT` basis 0 denominator).
pub(crate) fn days_30_360_us_psa(start: i64, end: i64, modify_both: bool) -> f64 {
    let (sy, sm, mut sd) = civil_from_serial(start);
    let (ey, em, mut ed) = civil_from_serial(end);
    let start_feb_end = sm == 2 && sd == days_in_month(sy, 2);
    let end_feb_end = em == 2 && ed == days_in_month(ey, 2);
    if end_feb_end && (start_feb_end || modify_both) {
        ed = 30;
    }
    if ed == 31 && (sd >= 30 || modify_both) {
        ed = 30;
    }
    if sd == 31 || start_feb_end {
        sd = 30;
    }
    ((ey - sy) * 360 + (em - sm) * 30 + (ed - sd)) as f64
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
    let (sy, sm, _) = civil_from_serial(settlement);
    let (my, mm, _) = civil_from_serial(maturity);
    let period = 12 / frequency;
    if ((my - sy) * 12 + mm - sm) / period > MAX_COUPON_PERIODS {
        return Err(FormulaError::Num);
    }
    let return_last = is_last_day_of_month_1900(maturity);
    let mut periods = 0_i64;
    let mut front = maturity;
    let mut trailing = maturity;
    while front > settlement {
        if periods >= MAX_COUPON_PERIODS {
            return Err(FormulaError::Num);
        }
        trailing = front;
        periods += 1;
        front = unchecked_add_months_1900(maturity, -period * periods, return_last)?;
    }
    Ok(Schedule {
        previous: front,
        next: trailing,
        count: periods,
    })
}

pub(crate) fn coupon_days_from_start(sched: &Schedule, settlement: i64, basis: i64) -> f64 {
    days_by_basis(sched.previous.max(0), settlement, basis).0
}

/// Actual/actual coupon period days for `COUPDAYS` (`basis == 1`): steps back from `maturity`
/// without EOM snapping while strictly after `settlement`, then returns `NCD - PCD`.
fn coupdays_actual(settlement: i64, maturity: i64, frequency: i64) -> Fallible<f64> {
    check_coupon_span(settlement, maturity, frequency)?;
    let period = 12 / frequency;
    let mut date = maturity;
    let mut steps = 0_i64;
    loop {
        steps += 1;
        if steps > MAX_COUPON_PERIODS + 2 {
            return Err(FormulaError::Num);
        }
        let earlier = unchecked_add_months_1900(date, -period, false)?;
        if earlier <= settlement {
            break;
        }
        date = earlier;
    }
    let sched = coupon_schedule(settlement, date, frequency)?;
    Ok((sched.next - sched.previous) as f64)
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
            "COUPDAYBS" => num(coupon_days_from_start(&sched, settlement, basis)),
            "COUPDAYSNC" => {
                if basis == 0 {
                    let prev = sched.previous.max(0);
                    num(days_30_360_us_psa(prev, sched.next, true)
                        - days_30_360_us_psa(prev, settlement, false))
                } else {
                    num(days_by_basis(settlement, sched.next, basis).0)
                }
            }
            "COUPDAYS" => {
                if basis == 1 && settlement > 0 {
                    num(coupdays_actual(settlement, maturity, frequency)?)
                } else {
                    num(coupon_days(settlement, maturity, frequency, basis)?)
                }
            }
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
