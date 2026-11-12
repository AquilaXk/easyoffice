//! Hidden rows and columns and merged ranges of a worksheet (`hidden` on `row` and `col`,
//! `mergeCells`), kept beside the cell values and styles in [`SheetStyling`].
//!
//! 참조 구현: Univer `packages/sheets/src/commands/commands/set-row-visible.command.ts` and
//! `set-col-visible.command.ts` (`SetSpecificRowsVisibleCommand`, `SetSpecificColsVisibleCommand`:
//! visibility is a property of whole rows and columns, kept as ranges) and
//! `add-worksheet-merge.command.ts` lines 117-145 and 177-260 (merging keeps one cell's content
//! and removes the merges it overlaps), Apache-2.0, Copyright 2023-present DreamNum Co., Ltd.,
//! commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9. The file format side (`<row hidden>`,
//! `<col hidden>`, `<mergeCells>`) follows ECMA-376 18.3.1; the reference stores no file format.

use crate::sheet_styling::xlsx_styles::{cols_xml, px_to_xlsx_width};
use crate::{CellBox, SheetStyling};
use std::collections::BTreeMap;
use std::fmt::Write;

/// Merged ranges one sheet may carry.
pub const MAX_MERGES: usize = 100_000;
/// Hidden rows (summed over all runs) one sheet may carry.
pub const MAX_HIDDEN_ROWS: u64 = 100_000;

/// Hidden width of a column that has no custom width (`width` is required next to `hidden`).
const DEFAULT_HIDDEN_WIDTH: &str = "8.43";

/// Rows or columns in `runs`.
pub fn run_total(runs: &[(u32, u32)]) -> u64 {
    runs.iter().map(|&(s, e)| u64::from(e - s) + 1).sum()
}

/// Adds `start..=end` to sorted, disjoint, non-adjacent `runs`.
pub(crate) fn add_run(runs: &mut Vec<(u32, u32)>, start: u32, end: u32) {
    match runs.last_mut() {
        None => runs.push((start, end)),
        Some(last) if start > last.1 && start - last.1 > 1 => runs.push((start, end)),
        Some(last) if start >= last.0 && start <= last.1.saturating_add(1) => {
            last.1 = last.1.max(end);
        }
        Some(_) => {
            runs.push((start, end));
            normalize_runs(runs);
        }
    }
}

/// Sorts `runs` and joins overlapping or touching ones.
pub fn normalize_runs(runs: &mut Vec<(u32, u32)>) {
    runs.sort_unstable();
    let mut merged: Vec<(u32, u32)> = Vec::with_capacity(runs.len());
    for &(s, e) in runs.iter() {
        match merged.last_mut() {
            Some(last) if s <= last.1.saturating_add(1) => last.1 = last.1.max(e),
            _ => merged.push((s, e)),
        }
    }
    *runs = merged;
}

/// Whether `index` lies in `runs`.
pub(crate) fn run_contains(runs: &[(u32, u32)], index: u32) -> bool {
    let after = runs.partition_point(|&(s, _)| s <= index);
    after > 0 && runs[after - 1].1 >= index
}

/// `<cols>` with the custom widths and the hidden columns (empty when there are none).
pub(crate) fn layout_cols_xml(columns: &BTreeMap<u32, u32>, hidden: &[(u32, u32)]) -> String {
    if hidden.is_empty() {
        return cols_xml(columns);
    }
    // One entry per column that has a width or is hidden, merged into runs of equal entries.
    let mut by_col: BTreeMap<u32, (Option<u32>, bool)> = columns
        .iter()
        .map(|(&c, &px)| (c, (Some(px), false)))
        .collect();
    for &(start, end) in hidden {
        for col in start..=end {
            by_col.entry(col).or_insert((None, true)).1 = true;
        }
    }
    let mut runs: Vec<(u32, u32, Option<u32>, bool)> = Vec::new();
    for (&col, &(px, hide)) in &by_col {
        match runs.last_mut() {
            Some((_, end, w, h)) if *end + 1 == col && *w == px && *h == hide => *end = col,
            _ => runs.push((col, col, px, hide)),
        }
    }
    let mut xml = String::from("<cols>");
    for (start, end, px, hide) in runs {
        let _ = write!(
            xml,
            "<col min=\"{}\" max=\"{}\" width=\"",
            start + 1,
            end + 1
        );
        match px {
            Some(px) => xml.push_str(&px_to_xlsx_width(px).to_string()),
            None => xml.push_str(DEFAULT_HIDDEN_WIDTH),
        }
        xml.push('"');
        if hide {
            xml.push_str(" hidden=\"1\"");
        }
        if px.is_some() {
            xml.push_str(" customWidth=\"1\"");
        }
        xml.push_str("/>");
    }
    xml.push_str("</cols>");
    xml
}

/// `<mergeCells>` (empty when there are none).
pub(crate) fn merge_cells_xml(merges: &[CellBox]) -> String {
    if merges.is_empty() {
        return String::new();
    }
    let mut xml = format!("<mergeCells count=\"{}\">", merges.len());
    for merge in merges {
        let _ = write!(xml, "<mergeCell ref=\"{}\"/>", merge.to_a1());
    }
    xml.push_str("</mergeCells>");
    xml
}

/// A merged range read from `ref`; only a rectangle of cells (not `A:C` or `2:5`) with more than
/// one cell counts.
pub(crate) fn parse_merge(reference: &str) -> Option<CellBox> {
    let is_cell = |end: &str| {
        end.bytes().any(|b| b.is_ascii_alphabetic()) && end.bytes().any(|b| b.is_ascii_digit())
    };
    reference
        .split(':')
        .all(is_cell)
        .then(|| CellBox::parse(reference))
        .flatten()
        .filter(|b| b.area() > 1)
}

/// Whether `styling` still has room for another merge or hidden row (readers stop at the bounds).
pub(crate) fn has_room(styling: &SheetStyling) -> bool {
    styling.merges.len() < MAX_MERGES && run_total(&styling.hidden_rows) < MAX_HIDDEN_ROWS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_stay_sorted_and_merge_when_touching() {
        let mut runs = Vec::new();
        for i in [3, 4, 5, 9, 10, 20] {
            add_run(&mut runs, i, i);
        }
        assert_eq!(runs, vec![(3, 5), (9, 10), (20, 20)]);
        add_run(&mut runs, 6, 8);
        assert_eq!(runs, vec![(3, 10), (20, 20)]);
        add_run(&mut runs, 1, 2);
        assert_eq!(runs, vec![(1, 10), (20, 20)]);
        assert_eq!(run_total(&runs), 11);
        assert!(run_contains(&runs, 10) && run_contains(&runs, 20));
        assert!(!run_contains(&runs, 11) && !run_contains(&runs, 0));
    }

    #[test]
    fn hidden_columns_keep_their_widths_in_the_cols_element() {
        let widths: BTreeMap<u32, u32> = [(0, 120), (2, 80)].into();
        let xml = layout_cols_xml(&widths, &[(1, 2)]);
        assert!(xml.contains(r#"<col min="1" max="1""#));
        assert!(
            xml.contains(r#"min="2" max="2" width="8.43" hidden="1"/>"#),
            "{xml}"
        );
        assert!(
            xml.contains(r#"min="3" max="3""#) && xml.contains(r#"hidden="1" customWidth="1""#)
        );
        assert_eq!(layout_cols_xml(&widths, &[]), cols_xml(&widths));
    }

    #[test]
    fn merges_are_written_as_references() {
        let merges = [
            CellBox::parse("A1:B2").unwrap(),
            CellBox::parse("D4:D6").unwrap(),
        ];
        assert_eq!(
            merge_cells_xml(&merges),
            r#"<mergeCells count="2"><mergeCell ref="A1:B2"/><mergeCell ref="D4:D6"/></mergeCells>"#
        );
        assert_eq!(merge_cells_xml(&[]), "");
        assert!(parse_merge("C3").is_none());
        assert!(parse_merge("C3:C3").is_none());
        assert!(parse_merge("A:C").is_none() && parse_merge("2:5").is_none());
        assert_eq!(parse_merge("C3:D3").unwrap().to_a1(), "C3:D3");
    }
}
