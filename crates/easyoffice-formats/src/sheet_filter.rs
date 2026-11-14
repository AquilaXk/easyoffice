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
//! Modifications: columns are keyed by absolute zero-based column (the file stores the offset from
//! the range's first column), conditions own their text, the colour and dynamic filters of the
//! reference are not modelled, and the rows the filter hides are kept beside the model as sorted
//! runs (`hidden`), separate from the rows a user hid (`SheetStyling::hidden_rows`). Every size is
//! bounded.

use crate::CellBox;

/// Rows (header row included) a filter range may cover.
pub const MAX_FILTER_ROWS: u32 = 200_000;
/// Columns with a condition in one filter.
pub const MAX_FILTER_COLUMNS: usize = 256;
/// Values one column's list may select.
pub const MAX_FILTER_VALUES: usize = 10_000;
/// Characters of a condition's text or of one selected value (a longer cell text is not listed).
pub const MAX_FILTER_TEXT_CHARS: usize = 255;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterCriteria {
    /// The listed texts are shown; `blank` also shows the empty cells.
    Values { blank: bool, values: Vec<String> },
    /// One condition, or two joined by AND (`and`) or OR.
    Custom {
        and: bool,
        first: CustomCondition,
        second: Option<CustomCondition>,
    },
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
        FilterCriteria::Values { values, .. } => {
            values.len() <= MAX_FILTER_VALUES
                && values
                    .iter()
                    .all(|v| v.chars().count() <= MAX_FILTER_TEXT_CHARS)
        }
        FilterCriteria::Custom { first, second, .. } => {
            text_ok(first) && second.as_ref().is_none_or(text_ok)
        }
    }
}
