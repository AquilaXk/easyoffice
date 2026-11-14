//! The auto filter of a worksheet: its range, the condition of each column and the rows it hides
//! (PD-198).
//!
//! 참조 구현: Univer `packages/sheets-filter/src/models/types.ts` lines 22-136 (`IAutoFilter`,
//! `IFilterColumn`, `IFilters`, `ICustomFilters`, `ICustomFilter`, `CustomFilterOperator`),
//! Apache-2.0, Copyright 2023-present DreamNum Co., Ltd., commit
//! 5a06a7842c8e13fa0646d0733dc3662cf4b330b9. The shapes follow ECMA-376 18.3.2 as the reference
//! does: a column holds either a list of values (`filters`, with `blank`) or one or two custom
//! conditions joined by AND or OR, and text conditions are wildcard patterns (`*`, `?`, `~`).
//!
//! The colour filter (`IColorFilters`, `packages/sheets-filter/src/models/types.ts` lines 71-79 and
//! `filter-model.ts` lines 496-524: a cell stands for its fill or its text colour, "no colour" is a
//! value of its own) is the `Color` criterion, and the date tree of the reference's value list
//! (`packages/sheets-filter-ui/src/worker/generate-filter-values.service.ts` lines 206-260: year,
//! month, day) is the `dates` list of `Values`, in the file's `dateGroupItem` form (ECMA-376 18.3.2.4).
//! The top-ten (18.3.2.10) and above/below-average (18.3.2.5 `dynamicFilter`) criteria are not in the
//! reference's model (its `IDynamicFilter` is declared and unused).
//!
//! Modifications: columns are keyed by absolute zero-based column (the file stores the offset from
//! the range's first column), conditions own their text, a column holds one colour (as the file
//! does) instead of a list, date groups carry the unit they stand for, and the rows the filter hides
//! are kept beside the model as sorted runs (`hidden`), separate from the rows a user hid
//! (`SheetStyling::hidden_rows`). Every size is bounded.

use crate::CellBox;

/// Rows (header row included) a filter range may cover.
pub const MAX_FILTER_ROWS: u32 = 200_000;
/// Columns with a condition in one filter.
pub const MAX_FILTER_COLUMNS: usize = 256;
/// Values one column's list may select.
pub const MAX_FILTER_VALUES: usize = 10_000;
/// Characters of a condition's text or of one selected value (a longer cell text is not listed).
pub const MAX_FILTER_TEXT_CHARS: usize = 255;
/// Date groups one column's list may select.
pub const MAX_FILTER_DATE_GROUPS: usize = 10_000;
/// Items a top or bottom filter may ask for, and the most percent it may ask for.
pub const MAX_TOP_ITEMS: u32 = 1_000_000;
pub const MAX_TOP_PERCENT: u32 = 100;
/// The first and last year a date group may name (the serial numbers of a sheet stop at 9999).
pub const MIN_DATE_YEAR: u16 = 1900;
pub const MAX_DATE_YEAR: u16 = 9999;

/// How a custom condition compares; `None` on [`CustomCondition`] is the text match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomOp {
    Equal,
    NotEqual,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
}

impl CustomOp {
    /// The `operator` attribute value of `customFilter` (ECMA-376 18.18.31).
    pub const fn name(self) -> &'static str {
        match self {
            Self::Equal => "equal",
            Self::NotEqual => "notEqual",
            Self::GreaterThan => "greaterThan",
            Self::GreaterThanOrEqual => "greaterThanOrEqual",
            Self::LessThan => "lessThan",
            Self::LessThanOrEqual => "lessThanOrEqual",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "equal" => Self::Equal,
            "notEqual" => Self::NotEqual,
            "greaterThan" => Self::GreaterThan,
            "greaterThanOrEqual" => Self::GreaterThanOrEqual,
            "lessThan" => Self::LessThan,
            "lessThanOrEqual" => Self::LessThanOrEqual,
            _ => return None,
        })
    }
}

/// One custom condition. Without an operator the text is a wildcard pattern the whole cell text
/// must match, and `NotEqual` with a pattern excludes the matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomCondition {
    pub op: Option<CustomOp>,
    pub text: String,
}

/// How far a date group reaches (ECMA-376 18.18.17 `ST_DateTimeGrouping`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DateUnit {
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
}

impl DateUnit {
    /// The `dateTimeGrouping` attribute value.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Year => "year",
            Self::Month => "month",
            Self::Day => "day",
            Self::Hour => "hour",
            Self::Minute => "minute",
            Self::Second => "second",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "year" => Self::Year,
            "month" => Self::Month,
            "day" => Self::Day,
            "hour" => Self::Hour,
            "minute" => Self::Minute,
            "second" => Self::Second,
            _ => return None,
        })
    }
}

/// One checked node of the date tree: every date and time that agrees with it down to `unit`
/// (`dateGroupItem`). The fields below the unit are zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateGroup {
    pub unit: DateUnit,
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl DateGroup {
    /// Whether the fields are inside the calendar for the unit (a day is checked against the month
    /// and year it names, so no group names a day that does not exist).
    pub fn is_valid(&self) -> bool {
        let unit = self.unit;
        let month_ok = unit < DateUnit::Month || (1..=12).contains(&self.month);
        let day_ok =
            unit < DateUnit::Day || (1..=days_in_month(self.year, self.month)).contains(&self.day);
        (MIN_DATE_YEAR..=MAX_DATE_YEAR).contains(&self.year)
            && month_ok
            && day_ok
            && (unit < DateUnit::Hour || self.hour < 24)
            && (unit < DateUnit::Minute || self.minute < 60)
            && (unit < DateUnit::Second || self.second < 60)
    }
}

/// Days of `month` (1-12) of `year` in the proleptic Gregorian calendar, 0 for a month outside it.
pub const fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 0,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterCriteria {
    /// The listed texts and date groups are shown; `blank` also shows the empty cells.
    Values {
        blank: bool,
        values: Vec<String>,
        dates: Vec<DateGroup>,
    },
    /// One condition, or two joined by AND (`and`) or OR.
    Custom {
        and: bool,
        first: CustomCondition,
        second: Option<CustomCondition>,
    },
    /// The `count` largest (or, with `bottom`, smallest) numbers, or that many percent of the
    /// numbers (`top10`). A number tied with the last one shown is shown too.
    Top {
        bottom: bool,
        percent: bool,
        count: u32,
    },
    /// The numbers above (or below) the average of the numbers in the column (`dynamicFilter`).
    Average { above: bool },
    /// The cells whose fill (or, with `font`, text) colour is `color` as `0xRRGGBB`; `None` is the
    /// cells with no colour of their own (`colorFilter`).
    Color { font: bool, color: Option<u32> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterColumn {
    /// Zero-based column in the sheet, inside the filter's range.
    pub col: u32,
    pub criteria: FilterCriteria,
}

/// The auto filter of one sheet. `range` includes the header row; the rows below it are filtered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetFilter {
    pub range: CellBox,
    /// Sorted by column, one entry per column.
    pub columns: Vec<FilterColumn>,
    /// Rows the filter hides, as sorted, disjoint, non-adjacent zero-based inclusive runs.
    pub hidden: Vec<(u32, u32)>,
}

impl SheetFilter {
    pub const fn new(range: CellBox) -> Self {
        Self {
            range,
            columns: Vec::new(),
            hidden: Vec::new(),
        }
    }

    /// Whether the range and the conditions are within the bounds a reader or the page may hand
    /// over.
    pub fn within_bounds(&self) -> bool {
        let r = self.range;
        r.row1 > r.row0
            && r.row1 - r.row0 < MAX_FILTER_ROWS
            && self
                .hidden
                .iter()
                .all(|&(first, last)| first > r.row0 && last <= r.row1)
            && columns_within_bounds(&self.columns, &r)
    }

    pub fn column(&self, col: u32) -> Option<&FilterColumn> {
        self.columns
            .binary_search_by_key(&col, |c| c.col)
            .ok()
            .map(|i| &self.columns[i])
    }

    /// Whether any column has a condition.
    pub fn has_criteria(&self) -> bool {
        !self.columns.is_empty()
    }

    /// Whether `row` is one the filter hides.
    pub fn hides(&self, row: u32) -> bool {
        crate::sheet_styling::xlsx_layout::run_contains(&self.hidden, row)
    }
}

/// Whether the conditions are sorted by column, inside `range`, and within the bounds of their texts.
pub(crate) fn columns_within_bounds(columns: &[FilterColumn], range: &CellBox) -> bool {
    let text_ok = |c: &CustomCondition| c.text.chars().count() <= MAX_FILTER_TEXT_CHARS;
    columns.len() <= MAX_FILTER_COLUMNS
        && columns.windows(2).all(|pair| pair[0].col < pair[1].col)
        && columns.iter().all(|c| {
            (range.col0..=range.col1).contains(&c.col)
                && criteria_within_bounds(&c.criteria, text_ok)
        })
}

fn criteria_within_bounds(c: &FilterCriteria, text_ok: impl Fn(&CustomCondition) -> bool) -> bool {
    match c {
        FilterCriteria::Values { values, dates, .. } => {
            values.len() <= MAX_FILTER_VALUES
                && dates.len() <= MAX_FILTER_DATE_GROUPS
                && values
                    .iter()
                    .all(|v| v.chars().count() <= MAX_FILTER_TEXT_CHARS)
                && dates.iter().all(DateGroup::is_valid)
        }
        FilterCriteria::Custom { first, second, .. } => {
            text_ok(first) && second.as_ref().is_none_or(text_ok)
        }
        FilterCriteria::Top { percent, count, .. } => (1..=if *percent {
            MAX_TOP_PERCENT
        } else {
            MAX_TOP_ITEMS
        })
            .contains(count),
        FilterCriteria::Average { .. } => true,
        FilterCriteria::Color { color, .. } => color.is_none_or(|c| c <= 0xFF_FFFF),
    }
}

/// The `(font, colour)` pairs of the colour filters among `columns`.
pub(crate) fn color_criteria(
    columns: &[FilterColumn],
) -> impl Iterator<Item = (bool, Option<u32>)> + '_ {
    columns.iter().filter_map(|c| match c.criteria {
        FilterCriteria::Color { font, color } => Some((font, color)),
        _ => None,
    })
}

/// Whether `text` is a plain number (the text a numeric condition compares with).
pub(crate) fn plain_number(text: &str) -> bool {
    text.trim().parse::<f64>().is_ok_and(f64::is_finite)
}
