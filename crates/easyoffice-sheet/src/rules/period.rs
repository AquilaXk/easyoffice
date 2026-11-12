//! The day range of a "dates occurring" rule (yesterday, this week, last month, ...).
//!
//! Reference implementation: the periods are the ones of the Univer conditional formatting
//! package (`packages/sheets-conditional-formatting/src/base/const.ts` `CFTimePeriodOperator`;
//! `models/calculate-unit-v2/highlight-cell-calculate-unit.ts` lines 149-240, the `timePeriod`
//! branch of `getRuleConfig`), Apache-2.0, Copyright 2023-present DreamNum Co., Ltd., commit
//! 5a06a7842c8e13fa0646d0733dc3662cf4b330b9. See `THIRD_PARTY_NOTICES.md`.
//!
//! Differences from the original: days are spreadsheet serial numbers (a cell matches when the
//! whole-day part of its number lies in the range) instead of millisecond timestamps, weeks run
//! Sunday to Saturday as in the file formats, and "today" is passed in (the page sends the local
//! date), not read from a clock inside the rule.

use crate::functions_date_fin::{civil_from_serial, serial_from_civil};
use easyoffice_formats::TimePeriod;

/// Excel serial of 1970-01-01.
const UNIX_EPOCH_SERIAL: i64 = 25_569;
const SECONDS_PER_DAY: i64 = 86_400;

/// The serial of today's date in UTC (the default when the page sends none).
pub(crate) fn utc_today() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    UNIX_EPOCH_SERIAL + secs / SECONDS_PER_DAY
}

/// First day of the month that is `step` months from the month of `day`.
fn month_start(day: i64, step: i64) -> i64 {
    let (y, m, _) = civil_from_serial(day);
    let index = y * 12 + (m - 1) + step;
    serial_from_civil(index.div_euclid(12), index.rem_euclid(12) + 1, 1)
}

/// Sunday of the week of `day`.
fn week_start(day: i64) -> i64 {
    // Serial 1 is a Sunday, so serials 1, 8, 15, ... start weeks.
    day - (day - 1).rem_euclid(7)
}

/// First and last serial (inclusive) of `period` when today is `today`.
pub(crate) fn period_days(period: TimePeriod, today: i64) -> (i64, i64) {
    let week = |step: i64| {
        let start = week_start(today) + 7 * step;
        (start, start + 6)
    };
    let month = |step: i64| (month_start(today, step), month_start(today, step + 1) - 1);
    match period {
        TimePeriod::Yesterday => (today - 1, today - 1),
        TimePeriod::Today => (today, today),
        TimePeriod::Tomorrow => (today + 1, today + 1),
        TimePeriod::Last7Days => (today - 6, today),
        TimePeriod::LastWeek => week(-1),
        TimePeriod::ThisWeek => week(0),
        TimePeriod::NextWeek => week(1),
        TimePeriod::LastMonth => month(-1),
        TimePeriod::ThisMonth => month(0),
        TimePeriod::NextMonth => month(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i64, m: i64, d: i64) -> i64 {
        serial_from_civil(y, m, d)
    }

    fn range(period: TimePeriod, today: (i64, i64, i64)) -> ((i64, i64, i64), (i64, i64, i64)) {
        let (a, b) = period_days(period, day(today.0, today.1, today.2));
        (civil_from_serial(a), civil_from_serial(b))
    }

    #[test]
    fn days_and_the_last_seven_days() {
        let today = (2026, 11, 12);
        assert_eq!(
            range(TimePeriod::Today, today),
            ((2026, 11, 12), (2026, 11, 12))
        );
        assert_eq!(
            range(TimePeriod::Yesterday, today),
            ((2026, 11, 11), (2026, 11, 11))
        );
        assert_eq!(
            range(TimePeriod::Tomorrow, today),
            ((2026, 11, 13), (2026, 11, 13))
        );
        assert_eq!(
            range(TimePeriod::Last7Days, today),
            ((2026, 11, 6), (2026, 11, 12))
        );
        // Across a month and a year edge.
        assert_eq!(
            range(TimePeriod::Yesterday, (2026, 1, 1)),
            ((2025, 12, 31), (2025, 12, 31))
        );
        assert_eq!(
            range(TimePeriod::Tomorrow, (2024, 2, 28)),
            ((2024, 2, 29), (2024, 2, 29))
        );
    }

    #[test]
    fn weeks_run_sunday_to_saturday() {
        // 2026-11-12 is a Thursday: its week is Sunday 11-08 to Saturday 11-14.
        let today = (2026, 11, 12);
        assert_eq!(
            range(TimePeriod::ThisWeek, today),
            ((2026, 11, 8), (2026, 11, 14))
        );
        assert_eq!(
            range(TimePeriod::LastWeek, today),
            ((2026, 11, 1), (2026, 11, 7))
        );
        assert_eq!(
            range(TimePeriod::NextWeek, today),
            ((2026, 11, 15), (2026, 11, 21))
        );
        // A Sunday is the first day of its own week, a Saturday the last.
        assert_eq!(
            range(TimePeriod::ThisWeek, (2026, 11, 8)),
            ((2026, 11, 8), (2026, 11, 14))
        );
        assert_eq!(
            range(TimePeriod::ThisWeek, (2026, 11, 14)),
            ((2026, 11, 8), (2026, 11, 14))
        );
        // A week over a year edge.
        assert_eq!(
            range(TimePeriod::ThisWeek, (2026, 12, 31)),
            ((2026, 12, 27), (2027, 1, 2))
        );
    }

    #[test]
    fn months_follow_the_calendar_including_year_edges_and_leap_days() {
        assert_eq!(
            range(TimePeriod::ThisMonth, (2026, 11, 12)),
            ((2026, 11, 1), (2026, 11, 30))
        );
        assert_eq!(
            range(TimePeriod::LastMonth, (2026, 11, 12)),
            ((2026, 10, 1), (2026, 10, 31))
        );
        assert_eq!(
            range(TimePeriod::NextMonth, (2026, 11, 12)),
            ((2026, 12, 1), (2026, 12, 31))
        );
        assert_eq!(
            range(TimePeriod::NextMonth, (2026, 12, 15)),
            ((2027, 1, 1), (2027, 1, 31))
        );
        assert_eq!(
            range(TimePeriod::LastMonth, (2026, 1, 15)),
            ((2025, 12, 1), (2025, 12, 31))
        );
        assert_eq!(
            range(TimePeriod::ThisMonth, (2024, 2, 10)),
            ((2024, 2, 1), (2024, 2, 29))
        );
        assert_eq!(
            range(TimePeriod::ThisMonth, (2026, 2, 10)),
            ((2026, 2, 1), (2026, 2, 28))
        );
    }
}
