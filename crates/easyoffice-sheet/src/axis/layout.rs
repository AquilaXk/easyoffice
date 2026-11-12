//! The layout of a sheet that is tied to positions (merged ranges, hidden rows and columns,
//! custom column widths, the ranges of rules) follows the same edit as the cells.
//!
//! 참조 구현: the span rule is Univer's (`axis/mod.rs` header). How merges, row visibility and widths
//! follow an edit is modelled on `packages/sheets/src/controllers/merge-cell.controller.ts`
//! lines 529-590 (`_handleInsertRowCommand`: an insertion strictly inside a merge widens it, one
//! before it shifts it) and 723-790 (`_handleRemoveRowCommand`: a deletion narrows the merges it
//! overlaps, a merge left with one cell is dropped), and `packages/sheets/src/commands/commands/insert-row-col.command.ts`
//! lines 120-140 (inserted rows are visible: `hd: BooleanNumber.FALSE`), Apache-2.0, Copyright
//! 2023-present DreamNum Co., Ltd., commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9. A merge that
//! loses all its cells disappears; inserted rows and columns have the default width.

use super::{Axis, AxisEdit};
use easyoffice_formats::CellBox;
use std::collections::BTreeMap;

fn span_of(b: &CellBox, axis: Axis) -> (u32, u32) {
    match axis {
        Axis::Row => (b.row0, b.row1),
        Axis::Col => (b.col0, b.col1),
    }
}

/// The boxes after `edit`: a box that loses all of its rows or columns disappears. With
/// `drop_single`, a box left with one cell disappears too (merged ranges).
pub fn shift_boxes(boxes: &[CellBox], edit: &AxisEdit, drop_single: bool) -> Vec<CellBox> {
    boxes
        .iter()
        .filter_map(|b| {
            let (s, e) = span_of(b, edit.axis);
            let (ns, ne) = edit.shift_span(s, e)?;
            let mut out = *b;
            match edit.axis {
                Axis::Row => (out.row0, out.row1) = (ns, ne),
                Axis::Col => (out.col0, out.col1) = (ns, ne),
            }
            (!drop_single || out.area() > 1).then_some(out)
        })
        .collect()
}

/// Sorted, non-overlapping inclusive runs of hidden rows (or columns) after `edit`. A run that
/// the insertion cuts in two stays two runs: the inserted rows are visible.
pub fn shift_runs(runs: &[(u32, u32)], edit: &AxisEdit) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::with_capacity(runs.len() + 1);
    let mut push = |s: u32, e: u32| match out.last_mut() {
        Some(last) if last.1 + 1 >= s => last.1 = last.1.max(e),
        _ => out.push((s, e)),
    };
    for &(s, e) in runs {
        if edit.kind == super::EditKind::Insert && s < edit.at && e >= edit.at {
            push(s, edit.at - 1);
            let limit = edit.limit() - 1;
            if let Some(start) = (edit.at + edit.count <= limit).then_some(edit.at + edit.count) {
                push(start, (e + edit.count).min(limit));
            }
        } else if let Some((ns, ne)) = edit.shift_span(s, e) {
            push(ns, ne);
        }
    }
    out
}

/// Custom column widths (zero-based column to pixels) after a column edit; a row edit leaves
/// them as they are.
pub fn shift_widths(widths: &BTreeMap<u32, u32>, edit: &AxisEdit) -> BTreeMap<u32, u32> {
    if edit.axis == Axis::Row {
        return widths.clone();
    }
    widths
        .iter()
        .filter_map(|(&col, &px)| Some((edit.map_index(col)?, px)))
        .collect()
}
