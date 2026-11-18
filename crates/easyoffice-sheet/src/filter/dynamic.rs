//! Compiled dynamic date filter conditions (ECMA-376 18.3.2.5 `dynamicFilter`, 18.18.26
//! `ST_DynamicFilterType`, PD-238).
//!
//! Reference implementation: shares the day, week (Sunday to Saturday) and month arithmetic of
//! `crate::rules::period` (adapted from Univer `packages/sheets-conditional-formatting`, Apache-2.0,
//! Copyright 2023-present DreamNum Co., Ltd., commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9; see
//! `THIRD_PARTY_NOTICES.md`) and extends it to the full ECMA-376 `ST_DynamicFilterType` date set
//! (`thisQuarter` / `lastQuarter` / `nextQuarter`, `thisYear` / `lastYear` / `nextYear`,
//! `yearToDate`, `Q1`..`Q4`, `M1`..`M12`).
//!
//! Low-level optimization: every relative period is resolved once per column into an inclusive
//! integer day span `[lo, hi]` clamped to `1..=2_958_465` (zero `civil_from_serial` division per
//! row), and every recurring quarter or month across all years (`Q1`..`Q4`, `M1`..`M12`) is packed
//! into a 12-bit `u16` month mask tested in `O(1)`.

use super::dates::{serial_day, FIRST_DAY_SERIAL, LAST_DAY_SERIAL};
use crate::functions_date_fin::{civil_from_serial, serial_from_civil};
use crate::rules::period::{month_start, utc_today as rules_utc_today, week_start};
use easyoffice_formats::DynamicDate;

/// Today's whole-day serial in UTC (used when a caller or open-file split provides no local date).
#[inline]
pub fn utc_today() -> i64 {
    rules_utc_today()
}

/// A column's dynamic date condition compiled once for a given `today`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynamicPlan {
    Never,
    Span { lo: i64, hi: i64 },
    Months(u16),
}

fn quarter_span(today: i64, step: i64) -> (i64, i64) {
    let (y, m, _) = civil_from_serial(today);
    let q0 = (m - 1) / 3 * 3;
    let first_m = y * 12 + q0 + step * 3;
    let next_m = first_m + 3;
    let lo = serial_from_civil(first_m.div_euclid(12), first_m.rem_euclid(12) + 1, 1);
    let hi = serial_from_civil(next_m.div_euclid(12), next_m.rem_euclid(12) + 1, 1) - 1;
    (lo, hi)
}

fn year_span(year: i64) -> (i64, i64) {
    (
        serial_from_civil(year, 1, 1),
        serial_from_civil(year, 12, 31),
    )
}

fn clamped_span(lo: i64, hi: i64) -> DynamicPlan {
    let lo = lo.max(FIRST_DAY_SERIAL);
    let hi = hi.min(LAST_DAY_SERIAL);
    if lo <= hi {
        DynamicPlan::Span { lo, hi }
    } else {
        DynamicPlan::Never
    }
}

impl DynamicPlan {
    pub fn compile(kind: DynamicDate, today: i64) -> Self {
        let today = today.clamp(FIRST_DAY_SERIAL, LAST_DAY_SERIAL);
        let week = |step: i64| {
            let start = week_start(today) + 7 * step;
            clamped_span(start, start + 6)
        };
        let month =
            |step: i64| clamped_span(month_start(today, step), month_start(today, step + 1) - 1);
        let quarter = |step: i64| {
            let (lo, hi) = quarter_span(today, step);
            clamped_span(lo, hi)
        };
        match kind {
            DynamicDate::Today => clamped_span(today, today),
            DynamicDate::Yesterday => clamped_span(today - 1, today - 1),
            DynamicDate::Tomorrow => clamped_span(today + 1, today + 1),
            DynamicDate::ThisWeek => week(0),
            DynamicDate::LastWeek => week(-1),
            DynamicDate::NextWeek => week(1),
            DynamicDate::ThisMonth => month(0),
            DynamicDate::LastMonth => month(-1),
            DynamicDate::NextMonth => month(1),
            DynamicDate::ThisQuarter => quarter(0),
            DynamicDate::LastQuarter => quarter(-1),
            DynamicDate::NextQuarter => quarter(1),
            DynamicDate::ThisYear => {
                let (y, _, _) = civil_from_serial(today);
                let (lo, hi) = year_span(y);
                clamped_span(lo, hi)
            }
            DynamicDate::LastYear => {
                let (y, _, _) = civil_from_serial(today);
                let (lo, hi) = year_span(y - 1);
                clamped_span(lo, hi)
            }
            DynamicDate::NextYear => {
                let (y, _, _) = civil_from_serial(today);
                let (lo, hi) = year_span(y + 1);
                clamped_span(lo, hi)
            }
            DynamicDate::YearToDate => {
                let (y, _, _) = civil_from_serial(today);
                clamped_span(serial_from_civil(y, 1, 1), today)
            }
            DynamicDate::Q1 => Self::Months(0b0000_0000_0111),
            DynamicDate::Q2 => Self::Months(0b0000_0011_1000),
            DynamicDate::Q3 => Self::Months(0b0001_1100_0000),
            DynamicDate::Q4 => Self::Months(0b1110_0000_0000),
            DynamicDate::M1 => Self::Months(1 << 0),
            DynamicDate::M2 => Self::Months(1 << 1),
            DynamicDate::M3 => Self::Months(1 << 2),
            DynamicDate::M4 => Self::Months(1 << 3),
            DynamicDate::M5 => Self::Months(1 << 4),
            DynamicDate::M6 => Self::Months(1 << 5),
            DynamicDate::M7 => Self::Months(1 << 6),
            DynamicDate::M8 => Self::Months(1 << 7),
            DynamicDate::M9 => Self::Months(1 << 8),
            DynamicDate::M10 => Self::Months(1 << 9),
            DynamicDate::M11 => Self::Months(1 << 10),
            DynamicDate::M12 => Self::Months(1 << 11),
        }
    }

    #[inline]
    pub fn accepts(self, serial: f64) -> bool {
        let Some(day) = serial_day(serial) else {
            return false;
        };
        match self {
            Self::Never => false,
            Self::Span { lo, hi } => (lo..=hi).contains(&day),
            Self::Months(mask) => {
                let (_, month, _) = civil_from_serial(day);
                (mask & (1u16 << (month - 1))) != 0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i64, m: i64, d: i64) -> i64 {
        serial_from_civil(y, m, d)
    }

    fn span_of(kind: DynamicDate, today: (i64, i64, i64)) -> ((i64, i64, i64), (i64, i64, i64)) {
        let DynamicPlan::Span { lo, hi } =
            DynamicPlan::compile(kind, day(today.0, today.1, today.2))
        else {
            panic!("expected Span for {kind:?}");
        };
        (civil_from_serial(lo), civil_from_serial(hi))
    }

    #[test]
    fn relative_days_weeks_months_quarters_years_and_ytd_follow_calendar() {
        // 2026-11-18 is a Wednesday (week: Sun 11-15 .. Sat 11-21, Q4: Oct 1 .. Dec 31).
        let t = (2026, 11, 18);
        assert_eq!(span_of(DynamicDate::Today, t), (t, t));
        assert_eq!(
            span_of(DynamicDate::Yesterday, t),
            ((2026, 11, 17), (2026, 11, 17))
        );
        assert_eq!(
            span_of(DynamicDate::Tomorrow, t),
            ((2026, 11, 19), (2026, 11, 19))
        );
        assert_eq!(
            span_of(DynamicDate::ThisWeek, t),
            ((2026, 11, 15), (2026, 11, 21))
        );
        assert_eq!(
            span_of(DynamicDate::LastWeek, t),
            ((2026, 11, 8), (2026, 11, 14))
        );
        assert_eq!(
            span_of(DynamicDate::NextWeek, t),
            ((2026, 11, 22), (2026, 11, 28))
        );
        assert_eq!(
            span_of(DynamicDate::ThisMonth, t),
            ((2026, 11, 1), (2026, 11, 30))
        );
        assert_eq!(
            span_of(DynamicDate::LastMonth, t),
            ((2026, 10, 1), (2026, 10, 31))
        );
        assert_eq!(
            span_of(DynamicDate::NextMonth, t),
            ((2026, 12, 1), (2026, 12, 31))
        );
        assert_eq!(
            span_of(DynamicDate::ThisQuarter, t),
            ((2026, 10, 1), (2026, 12, 31))
        );
        assert_eq!(
            span_of(DynamicDate::LastQuarter, t),
            ((2026, 7, 1), (2026, 9, 30))
        );
        assert_eq!(
            span_of(DynamicDate::NextQuarter, t),
            ((2027, 1, 1), (2027, 3, 31))
        );
        assert_eq!(
            span_of(DynamicDate::ThisYear, t),
            ((2026, 1, 1), (2026, 12, 31))
        );
        assert_eq!(
            span_of(DynamicDate::LastYear, t),
            ((2025, 1, 1), (2025, 12, 31))
        );
        assert_eq!(
            span_of(DynamicDate::NextYear, t),
            ((2027, 1, 1), (2027, 12, 31))
        );
        assert_eq!(
            span_of(DynamicDate::YearToDate, t),
            ((2026, 1, 1), (2026, 11, 18))
        );
        // Leap-year Q1 and LastQuarter across year boundary.
        assert_eq!(
            span_of(DynamicDate::LastQuarter, (2024, 2, 29)),
            ((2023, 10, 1), (2023, 12, 31))
        );
        assert_eq!(
            span_of(DynamicDate::ThisQuarter, (2024, 2, 29)),
            ((2024, 1, 1), (2024, 3, 31))
        );
    }

    #[test]
    fn recurring_quarters_and_months_match_every_year_and_clamp_calendar_edges() {
        let q1 = DynamicPlan::compile(DynamicDate::Q1, 45_000);
        assert!(q1.accepts(day(1999, 1, 1) as f64));
        assert!(q1.accepts(day(2024, 2, 29) as f64 + 0.75));
        assert!(q1.accepts(day(2030, 3, 31) as f64));
        assert!(!q1.accepts(day(2030, 4, 1) as f64));

        let quarters = [
            (DynamicDate::Q1, 1..=3),
            (DynamicDate::Q2, 4..=6),
            (DynamicDate::Q3, 7..=9),
            (DynamicDate::Q4, 10..=12),
        ];
        for (qk, months) in quarters {
            let plan = DynamicPlan::compile(qk, 45_000);
            for m in 1..=12 {
                assert_eq!(
                    plan.accepts(day(2025, m, 15) as f64),
                    months.contains(&(m as u8))
                );
            }
        }
        let month_kinds = [
            DynamicDate::M1,
            DynamicDate::M2,
            DynamicDate::M3,
            DynamicDate::M4,
            DynamicDate::M5,
            DynamicDate::M6,
            DynamicDate::M7,
            DynamicDate::M8,
            DynamicDate::M9,
            DynamicDate::M10,
            DynamicDate::M11,
            DynamicDate::M12,
        ];
        for (idx, &mk) in month_kinds.iter().enumerate() {
            let plan = DynamicPlan::compile(mk, 45_000);
            for m in 1..=12 {
                assert_eq!(plan.accepts(day(2021, m, 10) as f64), m == (idx as i64) + 1);
            }
        }

        // Calendar boundaries and half-second rounding.
        assert_eq!(
            DynamicPlan::compile(DynamicDate::Yesterday, FIRST_DAY_SERIAL),
            DynamicPlan::Never
        );
        assert_eq!(
            DynamicPlan::compile(DynamicDate::Tomorrow, LAST_DAY_SERIAL),
            DynamicPlan::Never
        );
        let tomorrow = DynamicPlan::compile(DynamicDate::Tomorrow, 45_000);
        assert!(tomorrow.accepts(45_000.0 + 86_399.6 / 86_400.0));
        assert!(!tomorrow.accepts(f64::NAN) && !tomorrow.accepts(0.5));
    }
}
