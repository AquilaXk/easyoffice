//! Date groups of a column: which of a column's dates and times a checked node of the date tree
//! (year, month, day, hour, minute, second) covers, and the tree a column offers (PD-226).
//!
//! 참조 구현: Univer `packages/sheets-filter-ui/src/worker/generate-filter-values.service.ts` lines
//! 206-262 (`getFilterTreeByValueItems`: a cell whose number format shows a year, month or day part
//! becomes a leaf of a year, month, day tree, each node counting the cells below it, a month's
//! children sorted by day), Apache-2.0, Copyright 2023-present DreamNum Co., Ltd., commit
//! 5a06a7842c8e13fa0646d0733dc3662cf4b330b9.
//!
//! Modifications: the cell's number is split into its calendar fields by the engine's own date
//! arithmetic instead of formatting it to text and splitting the text, the tree is a sorted map
//! with a bound on its days, a group is judged by one hash lookup per unit of a packed key, and the
//! checked state lives in the file's `dateGroupItem` list instead of the checked leaves' texts.

use super::RowBits;
use crate::functions_date_fin::civil_from_serial;
use easyoffice_formats::{DateGroup, DateUnit, MAX_FILTER_DATE_GROUPS};
use std::collections::{BTreeMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

const SECONDS_PER_DAY: f64 = 86_400.0;
/// Serial numbers a date cell may have (the engine's date range, 1900-01-01 to 9999-12-31).
const FIRST_SERIAL: f64 = 1.0;
const LAST_SERIAL: f64 = 2_958_465.0;

/// The calendar fields of a date and time serial number, `None` outside the date range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moment {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl Moment {
    /// The fields of `serial`; the time of day rounds to the nearest second (a day's last half
    /// second belongs to the next day).
    pub fn of_serial(serial: f64) -> Option<Self> {
        if !serial.is_finite() || !(FIRST_SERIAL..LAST_SERIAL + 1.0).contains(&serial) {
            return None;
        }
        let mut days = serial.floor();
        let mut secs = ((serial - days) * SECONDS_PER_DAY).round();
        if secs >= SECONDS_PER_DAY {
            days += 1.0;
            secs = 0.0;
        }
        if days > LAST_SERIAL {
            return None;
        }
        let (year, month, day) = civil_from_serial(days as i64);
        let secs = secs as u32;
        Some(Self {
            year: u16::try_from(year).ok()?,
            month: month as u8,
            day: day as u8,
            hour: (secs / 3600) as u8,
            minute: (secs / 60 % 60) as u8,
            second: (secs % 60) as u8,
        })
    }

    /// The key of the group of `unit` this moment is in.
    #[inline]
    fn key(&self, unit: DateUnit) -> u64 {
        let fields = [
            u64::from(self.year),
            u64::from(self.month),
            u64::from(self.day),
            u64::from(self.hour),
            u64::from(self.minute),
            u64::from(self.second),
        ];
        // Year 14 bits, month 4, day 5, hour 5, minute 6, second 6: 40 bits under the unit.
        const WIDTH: [u32; 6] = [14, 4, 5, 5, 6, 6];
        let mut key = unit as u64;
        for (at, field) in fields.iter().enumerate() {
            key = key << WIDTH[at] | if at <= unit as usize { *field } else { 0 };
        }
        key
    }
}

#[derive(Default)]
struct KeyHasher(u64);

impl Hasher for KeyHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
        }
    }

    #[inline]
    fn write_u64(&mut self, n: u64) {
        self.0 = (n ^ (n >> 29)).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

/// The checked date groups of one column, ready to test cells.
#[derive(Default)]
pub struct DateKeys {
    keys: HashSet<u64, BuildHasherDefault<KeyHasher>>,
    /// Bit `u` is set when some group has the unit with index `u`.
    units: u8,
}

impl DateKeys {
    pub fn new(groups: &[DateGroup]) -> Self {
        let mut out = Self::default();
        for g in groups {
            let at = Moment {
                year: g.year,
                month: g.month,
                day: g.day,
                hour: g.hour,
                minute: g.minute,
                second: g.second,
            };
            out.keys.insert(at.key(g.unit));
            out.units |= 1 << g.unit as u8;
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Whether a cell holding `serial` is in a checked group.
    #[inline]
    pub fn covers(&self, serial: f64) -> bool {
        if self.keys.is_empty() {
            return false;
        }
        let Some(at) = Moment::of_serial(serial) else {
            return false;
        };
        const UNITS: [DateUnit; 6] = [
            DateUnit::Year,
            DateUnit::Month,
            DateUnit::Day,
            DateUnit::Hour,
            DateUnit::Minute,
            DateUnit::Second,
        ];
        UNITS
            .iter()
            .any(|&unit| self.units & (1 << unit as u8) != 0 && self.keys.contains(&at.key(unit)))
    }
}

/// One node of the date tree a column offers: a year with its months with their days.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DateTree {
    /// `(year, count, months)`, ascending.
    pub years: Vec<(u16, u32, Vec<MonthNode>)>,
    /// More distinct days than the bound: the tree holds the first ones met.
    pub more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthNode {
    pub month: u8,
    pub count: u32,
    /// `(day, count)`, ascending.
    pub days: Vec<(u8, u32)>,
}

/// The tree of the dates among the rows `excluded` leaves visible; `dates` says which rows show as
/// dates and `numbers` holds their serials. At most `MAX_FILTER_DATE_GROUPS` distinct days.
pub fn date_tree(numbers: &[f64], dates: &RowBits, excluded: &RowBits) -> DateTree {
    let mut days: BTreeMap<(u16, u8, u8), u32> = BTreeMap::new();
    let mut more = false;
    for (i, &serial) in numbers.iter().enumerate() {
        if !dates.get(i) || excluded.get(i) {
            continue;
        }
        let Some(at) = Moment::of_serial(serial) else {
            continue;
        };
        let key = (at.year, at.month, at.day);
        if let Some(count) = days.get_mut(&key) {
            *count += 1;
        } else if days.len() >= MAX_FILTER_DATE_GROUPS {
            more = true;
        } else {
            days.insert(key, 1);
        }
    }
    let mut tree = DateTree {
        years: Vec::new(),
        more,
    };
    for ((year, month, day), count) in days {
        if tree.years.last().is_none_or(|y| y.0 != year) {
            tree.years.push((year, 0, Vec::new()));
        }
        let Some(y) = tree.years.last_mut() else {
            continue;
        };
        y.1 += count;
        if y.2.last().is_none_or(|m| m.month != month) {
            y.2.push(MonthNode {
                month,
                count: 0,
                days: Vec::new(),
            });
        }
        if let Some(m) = y.2.last_mut() {
            m.count += count;
            m.days.push((day, count));
        }
    }
    tree
}

/// The rows `date_tree` counts: the date cells with a date part among the rows `excluded` leaves visible.
pub fn tree_rows(numbers: &[f64], dates: &RowBits, excluded: &RowBits) -> RowBits {
    let mut rows = RowBits::new(numbers.len());
    for (i, &serial) in numbers.iter().enumerate() {
        if dates.get(i) && !excluded.get(i) && Moment::of_serial(serial).is_some() {
            rows.set(i);
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(unit: DateUnit, y: u16, mo: u8, d: u8, h: u8, mi: u8, s: u8) -> DateGroup {
        DateGroup {
            unit,
            year: y,
            month: mo,
            day: d,
            hour: h,
            minute: mi,
            second: s,
        }
    }

    #[test]
    fn serials_split_into_calendar_fields() {
        // 45000 is 2023-03-15; .5 is noon; the leap day 1900-02-29 is serial 60.
        let m = Moment::of_serial(45000.5).unwrap();
        assert_eq!(
            (m.year, m.month, m.day, m.hour, m.minute, m.second),
            (2023, 3, 15, 12, 0, 0)
        );
        let m = Moment::of_serial(60.0).unwrap();
        assert_eq!((m.year, m.month, m.day), (1900, 2, 29));
        // The last half second of a day belongs to the next one.
        let m = Moment::of_serial(45000.0 + 86399.6 / 86400.0).unwrap();
        assert_eq!((m.month, m.day, m.hour), (3, 16, 0));
        let last = Moment::of_serial(2_958_465.0).unwrap();
        assert_eq!((last.year, last.month, last.day), (9999, 12, 31));
        for bad in [0.0, 0.5, -3.0, 2_958_466.0, f64::NAN, f64::INFINITY] {
            assert!(Moment::of_serial(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn a_group_covers_everything_down_to_its_unit() {
        let keys = DateKeys::new(&[
            group(DateUnit::Year, 2024, 0, 0, 0, 0, 0),
            group(DateUnit::Month, 2023, 3, 0, 0, 0, 0),
            group(DateUnit::Day, 2022, 12, 25, 0, 0, 0),
            group(DateUnit::Second, 2021, 1, 2, 3, 4, 5),
        ]);
        let serial =
            |y, m, d| f64::from(crate::functions_date_fin::serial_from_civil(y, m, d) as i32);
        assert!(keys.covers(serial(2024, 6, 1) + 0.7));
        assert!(keys.covers(serial(2023, 3, 31)) && !keys.covers(serial(2023, 4, 1)));
        assert!(keys.covers(serial(2022, 12, 25) + 0.99) && !keys.covers(serial(2022, 12, 24)));
        let at = serial(2021, 1, 2) + (3.0 * 3600.0 + 4.0 * 60.0 + 5.0) / 86_400.0;
        assert!(keys.covers(at) && !keys.covers(at + 1.0 / 86_400.0));
        assert!(!keys.covers(f64::NAN) && !keys.covers(0.5));
        assert!(!DateKeys::default().covers(45000.0));
    }

    #[test]
    fn the_tree_counts_days_months_and_years_in_order() {
        let numbers = [45000.0, 45001.0, 45000.5, 45300.0, f64::NAN, 40000.0];
        let mut dates = RowBits::new(6);
        for i in [0, 1, 2, 3, 5] {
            dates.set(i);
        }
        let mut excluded = RowBits::new(6);
        excluded.set(1);
        let tree = date_tree(&numbers, &dates, &excluded);
        assert!(!tree.more);
        let years: Vec<_> = tree.years.iter().map(|y| (y.0, y.1)).collect();
        assert_eq!(years, vec![(2009, 1), (2023, 2), (2024, 1)]);
        let march = &tree.years[1].2[0];
        assert_eq!(
            (march.month, march.count, march.days.clone()),
            (3, 2, vec![(15, 2)])
        );
    }

    #[test]
    fn the_tree_stops_at_the_bound_of_days() {
        let n = MAX_FILTER_DATE_GROUPS + 50;
        let numbers: Vec<f64> = (0..n).map(|i| 36_000.0 + i as f64).collect();
        let mut dates = RowBits::new(n);
        for i in 0..n {
            dates.set(i);
        }
        let tree = date_tree(&numbers, &dates, &RowBits::new(n));
        assert!(tree.more);
        let total: u32 = tree.years.iter().map(|y| y.1).sum();
        assert_eq!(total as usize, MAX_FILTER_DATE_GROUPS);
    }
}
