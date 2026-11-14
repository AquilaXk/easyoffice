//! The auto filter of a sheet: which rows a filter's conditions hide, the values a column offers
//! to choose from and how the rows the file hides split into the filter's and the user's (PD-198).
//!
//! 참조 구현: Univer `packages/sheets-filter/src/models/filter-model.ts` lines 31-467 (`FilterModel`,
//! `FilterColumn.calc`, line 395: a column's condition is evaluated over the data rows below the header, a
//! row already hidden by another column is not evaluated again, and the rows hidden by the other
//! columns are the ones a column's value list leaves out) and
//! `packages/sheets-filter-ui/src/worker/generate-filter-values.service.ts` lines 93-160
//! (`getFilterByValueItems`: one entry per distinct text with its count, empty cells counted
//! apart), Apache-2.0, Copyright 2023-present DreamNum Co., Ltd., commit
//! 5a06a7842c8e13fa0646d0733dc3662cf4b330b9.
//!
//! Modifications: the hidden rows are a bit per row (`RowBits`) and the columns are evaluated one
//! after the other into it, distinct texts are counted in one pass with an optional search, and
//! the list is bounded. Nothing here reads a cell: the caller passes the displayed text of the
//! rows below the header, column by column, with the numbers, date flags and colours the criteria
//! beyond text need (`CellAux`). The reference's worker plumbing is not used. The colour filter and
//! the date tree are in `cell_aux`, `dates`; the top and average criteria it has no counterpart for are in
//! `stats` (PD-226).

mod bits;
mod cell_aux;
mod condition;
mod dates;
mod runs;
mod stats;
mod wildcard;

pub use bits::RowBits;
pub use cell_aux::{color_list, CellAux, ColorList, NO_COLOR};
pub use condition::{number_of, ColumnMatcher};
pub use dates::{date_tree, tree_rows, DateKeys, DateTree, Moment, MonthNode};
pub use runs::{intersect_runs, subtract_runs};
pub use wildcard::{Wildcard, MAX_PATTERN_CHARS};

use condition::TextHasher;
use easyoffice_formats::{EasyOfficeError, SheetFilter, MAX_FILTER_TEXT_CHARS};
use std::collections::HashMap;
use std::hash::BuildHasherDefault;

/// The displayed text of one column of the data rows: `texts[i]` is the row
/// `range.row0 + 1 + i`. `aux` holds the numbers, date flags and colours of the same rows for the
/// criteria that read them; a column judged by text alone has none.
#[derive(Debug, Clone, Copy)]
pub struct ColumnText<'a> {
    pub col: u32,
    pub texts: &'a [String],
    pub aux: Option<&'a CellAux>,
}

impl<'a> ColumnText<'a> {
    pub const fn text(col: u32, texts: &'a [String]) -> Self {
        Self {
            col,
            texts,
            aux: None,
        }
    }
}

fn invalid(what: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(format!("필터 요청이 올바르지 않습니다: {what}"))
}

fn data_rows(filter: &SheetFilter) -> usize {
    (filter.range.row1 - filter.range.row0) as usize
}

fn column<'a>(
    columns: &[ColumnText<'a>],
    col: u32,
    rows: usize,
) -> Result<ColumnText<'a>, EasyOfficeError> {
    let found = columns
        .iter()
        .find(|c| c.col == col)
        .ok_or_else(|| invalid("열 값이 없습니다"))?;
    if found.texts.len() != rows {
        return Err(invalid("열 값의 행 수가 필터 범위와 다릅니다"));
    }
    Ok(*found)
}

/// The data rows (relative to the row below the header) the conditions of every column except
/// `skip` hide. The conditions that compare a cell with its column (the largest numbers, the
/// average) are judged last, column by column, over the rows the other conditions left visible.
pub fn hidden_bits(
    filter: &SheetFilter,
    columns: &[ColumnText<'_>],
    skip: Option<u32>,
) -> Result<RowBits, EasyOfficeError> {
    let rows = data_rows(filter);
    let mut hidden = RowBits::new(rows);
    let mut scratch: Vec<char> = Vec::new();
    let no_aux = CellAux::default();
    let mut statistics = Vec::new();
    for entry in filter.columns.iter().filter(|c| Some(c.col) != skip) {
        let cells = column(columns, entry.col, rows)?;
        let matcher = ColumnMatcher::compile(&entry.criteria)
            .ok_or_else(|| invalid("조건 문자열이 너무 깁니다"))?;
        if matcher.is_statistic() {
            statistics.push((&entry.criteria, cells));
            continue;
        }
        let aux = cells.aux.unwrap_or(&no_aux);
        for (i, text) in cells.texts.iter().enumerate() {
            // A row another column already hides is not tested again.
            if !hidden.get(i) && !matcher.accepts_at(i, text, aux, &mut scratch) {
                hidden.set(i);
            }
        }
    }
    for (criteria, cells) in statistics {
        stats::hide_by_statistic(criteria, cells.aux.unwrap_or(&no_aux), rows, &mut hidden);
    }
    Ok(hidden)
}

/// The rows (sheet row numbers) the conditions hide, as sorted runs.
pub fn hidden_runs(
    filter: &SheetFilter,
    columns: &[ColumnText<'_>],
) -> Result<Vec<(u32, u32)>, EasyOfficeError> {
    Ok(hidden_bits(filter, columns, None)?.runs(filter.range.row0 + 1))
}

/// One distinct text of a column and how many rows show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueItem {
    pub text: String,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ValueList {
    /// Numbers first (ascending), then the other texts without regard to case.
    pub items: Vec<ValueItem>,
    /// Rows with an empty cell (zero while a search is given).
    pub blank: u32,
    /// More distinct texts than `limit`: the list holds the first ones met.
    pub more: bool,
}

const NO_MATCH: u32 = u32::MAX;

/// The distinct texts of `texts` among the rows `excluded` does not hide, at most `limit` of
/// them. With a `query`, only texts that contain it (letter case ignored) are listed and
/// counted. A text longer than the bound a condition may hold is not listed.
pub fn value_list(texts: &[String], excluded: &RowBits, query: &str, limit: usize) -> ValueList {
    let needle = query.to_lowercase();
    let mut index: HashMap<&str, u32, BuildHasherDefault<TextHasher>> = HashMap::default();
    let mut list = ValueList::default();
    for (i, text) in texts.iter().enumerate() {
        if excluded.get(i) {
            continue;
        }
        if text.is_empty() {
            list.blank += 1;
            continue;
        }
        if let Some(&at) = index.get(text.as_str()) {
            if at != NO_MATCH {
                list.items[at as usize].count += 1;
            }
            continue;
        }
        let too_long =
            text.len() > MAX_FILTER_TEXT_CHARS && text.chars().count() > MAX_FILTER_TEXT_CHARS;
        let wanted = !too_long && (needle.is_empty() || text.to_lowercase().contains(&needle));
        if !wanted {
            index.insert(text, NO_MATCH);
        } else if list.items.len() >= limit {
            list.more = true;
            index.insert(text, NO_MATCH);
        } else {
            index.insert(text, list.items.len() as u32);
            list.items.push(ValueItem {
                text: text.clone(),
                count: 1,
            });
        }
    }
    if !needle.is_empty() {
        list.blank = 0;
    }
    sort_items(&mut list.items);
    list
}

fn sort_items(items: &mut [ValueItem]) {
    let mut keyed: Vec<(Option<f64>, String, ValueItem)> = items
        .iter()
        .map(|item| {
            let folded = item.text.to_lowercase();
            (number_of(&item.text), folded, item.clone())
        })
        .collect();
    keyed.sort_by(|a, b| match (a.0, b.0) {
        (Some(x), Some(y)) => x.total_cmp(&y).then_with(|| a.2.text.cmp(&b.2.text)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.1.cmp(&b.1).then_with(|| a.2.text.cmp(&b.2.text)),
    });
    for (slot, (_, _, item)) in items.iter_mut().zip(keyed) {
        *slot = item;
    }
}

/// Splits the hidden rows a file carries into the filter's and the user's: a hidden row below the
/// header that the conditions also hide belongs to the filter, every other hidden row stays the
/// user's. Rows the conditions would hide that the file shows stay shown (the file is the cache a
/// filter is only re-applied from on request). `assume` marks the data rows whose cells the
/// caller could not read (a formula whose value is not known): the filter is taken to hide them
/// when the file does.
pub fn split_hidden_rows(
    filter: &mut SheetFilter,
    hidden_rows: &mut Vec<(u32, u32)>,
    columns: &[ColumnText<'_>],
    assume: &RowBits,
) -> Result<(), EasyOfficeError> {
    if !filter.has_criteria() {
        return Ok(());
    }
    let mut bits = hidden_bits(filter, columns, None)?;
    bits.union_with(assume);
    let computed = bits.runs(filter.range.row0 + 1);
    filter.hidden = intersect_runs(&computed, hidden_rows);
    *hidden_rows = subtract_runs(hidden_rows, &filter.hidden);
    Ok(())
}

#[cfg(test)]
mod tests;
