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

use crate::sheet_styling::sheet_view::{Outline, MAX_OUTLINE_ROWS};
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

/// The indexes in both sorted, disjoint, non-adjacent run lists.
pub fn intersect_runs(a: &[(u32, u32)], b: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < a.len() && j < b.len() {
        let (start, end) = (a[i].0.max(b[j].0), a[i].1.min(b[j].1));
        if start <= end {
            out.push((start, end));
        }
        if a[i].1 < b[j].1 {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// The indexes of `a` that `b` does not hold, both sorted, disjoint and non-adjacent.
pub(crate) fn subtract_runs(a: &[(u32, u32)], b: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let mut out = Vec::with_capacity(a.len());
    let mut j = 0;
    for &(start, end) in a {
        while j < b.len() && b[j].1 < start {
            j += 1;
        }
        let (mut from, mut k) = (Some(start), j);
        while let Some(at) = from {
            match b.get(k).filter(|cut| cut.0 <= end) {
                None => {
                    out.push((at, end));
                    break;
                }
                Some(&(cut_start, cut_end)) => {
                    if cut_start > at {
                        out.push((at, cut_start - 1));
                    }
                    from = cut_end.checked_add(1).filter(|&next| next <= end);
                    k += 1;
                }
            }
        }
    }
    out
}

/// Whether `index` lies in `runs`.
pub(crate) fn run_contains(runs: &[(u32, u32)], index: u32) -> bool {
    let after = runs.partition_point(|&(s, _)| s <= index);
    after > 0 && runs[after - 1].1 >= index
}

/// What one column of `<cols>` says besides its index.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
struct ColAttrs {
    px: Option<u32>,
    hidden: bool,
    level: u8,
    collapsed: bool,
    unlocked: bool,
}

/// `<cols>` with the custom widths, the hidden and folded columns and the group levels (empty when
/// there is nothing to say). `unlocked_xf` is the `cellXfs` index of the format of an unlocked column.
pub(crate) fn layout_cols_xml(styling: &SheetStyling, unlocked_xf: usize) -> String {
    let outline = &styling.outline;
    if styling.hidden_cols.is_empty() && outline.cols.is_empty() && styling.unlocked_cols.is_empty()
    {
        return cols_xml(&styling.columns);
    }
    // One entry per column that has a width, is hidden or is grouped, merged into runs of equal entries.
    let mut by_col: BTreeMap<u32, ColAttrs> = styling
        .columns
        .iter()
        .map(|(&c, &px)| {
            let attrs = ColAttrs {
                px: Some(px),
                ..ColAttrs::default()
            };
            (c, attrs)
        })
        .collect();
    for &(start, end) in styling.hidden_cols.iter().chain(&outline.collapsed_cols) {
        for col in start..=end {
            by_col.entry(col).or_default().hidden = true;
        }
    }
    for &(start, end, level) in &outline.cols {
        for col in start..=end {
            by_col.entry(col).or_default().level = level;
        }
    }
    for &(start, end) in &styling.unlocked_cols {
        for col in start..=end {
            by_col.entry(col).or_default().unlocked = true;
        }
    }
    let marks = Outline::summary_marks(
        &outline.cols,
        &outline.collapsed_cols,
        &outline.open_cols,
        outline.summary_left,
    );
    for col in marks {
        by_col.entry(col).or_default().collapsed = true;
    }
    let mut runs: Vec<(u32, u32, ColAttrs)> = Vec::new();
    for (&col, &attrs) in &by_col {
        match runs.last_mut() {
            Some((_, end, last)) if *end + 1 == col && *last == attrs => *end = col,
            _ => runs.push((col, col, attrs)),
        }
    }
    let mut xml = String::from("<cols>");
    for (start, end, attrs) in runs {
        let _ = write!(
            xml,
            "<col min=\"{}\" max=\"{}\" width=\"",
            start + 1,
            end + 1
        );
        match attrs.px {
            Some(px) => xml.push_str(&px_to_xlsx_width(px).to_string()),
            None => xml.push_str(DEFAULT_HIDDEN_WIDTH),
        }
        xml.push('"');
        if attrs.hidden {
            xml.push_str(" hidden=\"1\"");
        }
        if attrs.px.is_some() {
            xml.push_str(" customWidth=\"1\"");
        }
        if attrs.level > 0 {
            let _ = write!(xml, " outlineLevel=\"{}\"", attrs.level);
        }
        if attrs.collapsed {
            xml.push_str(" collapsed=\"1\"");
        }
        if attrs.unlocked && unlocked_xf > 0 {
            let _ = write!(xml, " style=\"{unlocked_xf}\"");
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
/// A file does not tell the rows a user hid from the rows a folded group hid, so a reader holds
/// both kinds at once.
pub(crate) fn has_room(styling: &SheetStyling) -> bool {
    styling.merges.len() < MAX_MERGES
        && run_total(&styling.hidden_rows) < MAX_HIDDEN_ROWS + MAX_OUTLINE_ROWS
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
    fn run_lists_intersect_and_subtract() {
        let a = [(1, 5), (8, 9), (12, 20)];
        let b = [(3, 8), (15, 15), (30, 31)];
        assert_eq!(intersect_runs(&a, &b), vec![(3, 5), (8, 8), (15, 15)]);
        assert_eq!(
            subtract_runs(&a, &b),
            vec![(1, 2), (9, 9), (12, 14), (16, 20)]
        );
        assert_eq!(subtract_runs(&a, &[]), a.to_vec());
        assert_eq!(subtract_runs(&a, &[(0, 100)]), vec![]);
        assert_eq!(intersect_runs(&a, &[]), vec![]);
        assert_eq!(
            subtract_runs(&[(0, u32::MAX)], &[(5, u32::MAX)]),
            vec![(0, 4)]
        );
    }

    #[test]
    fn hidden_columns_keep_their_widths_in_the_cols_element() {
        let mut styling = SheetStyling {
            columns: [(0, 120), (2, 80)].into(),
            hidden_cols: vec![(1, 2)],
            ..SheetStyling::default()
        };
        let xml = layout_cols_xml(&styling, 0);
        assert!(xml.contains(r#"<col min="1" max="1""#));
        assert!(
            xml.contains(r#"min="2" max="2" width="8.43" hidden="1"/>"#),
            "{xml}"
        );
        assert!(
            xml.contains(r#"min="3" max="3""#) && xml.contains(r#"hidden="1" customWidth="1""#)
        );
        styling.hidden_cols.clear();
        assert_eq!(layout_cols_xml(&styling, 0), cols_xml(&styling.columns));
    }

    #[test]
    fn grouped_and_folded_columns_carry_level_and_summary_mark() {
        let styling = SheetStyling {
            outline: Outline {
                cols: vec![(1, 2, 1), (3, 3, 2)],
                collapsed_cols: vec![(1, 3)],
                ..Outline::default()
            },
            ..SheetStyling::default()
        };
        let xml = layout_cols_xml(&styling, 0);
        assert!(
            xml.contains(r#"<col min="2" max="3" width="8.43" hidden="1" outlineLevel="1"/>"#),
            "{xml}"
        );
        assert!(
            xml.contains(r#"<col min="4" max="4" width="8.43" hidden="1" outlineLevel="2"/>"#)
                && xml.contains(r#"<col min="5" max="5" width="8.43" collapsed="1"/>"#),
            "{xml}"
        );
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
