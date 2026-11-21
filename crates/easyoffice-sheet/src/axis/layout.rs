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
use easyoffice_formats::{CellBox, RowHeights};
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
            if edit.axis == Axis::Row && !edit.covers_cols(b.col0, b.col1) {
                return (!drop_single || b.area() > 1).then_some(*b);
            }
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

/// Sorted, non-overlapping inclusive runs of whole-sheet rows (or columns) after `edit`.
/// A column-bounded row shift leaves whole-sheet row runs unchanged.
pub fn shift_runs(runs: &[(u32, u32)], edit: &AxisEdit) -> Vec<(u32, u32)> {
    if edit.cols.is_some() {
        return runs.to_vec();
    }
    shift_runs_on_axis(runs, edit)
}

/// Sorted, non-overlapping inclusive runs after `edit` on its axis, used for ranges already known
/// to lie inside `edit.cols` (such as a table's or filter's own hidden rows).
pub(super) fn shift_runs_on_axis(runs: &[(u32, u32)], edit: &AxisEdit) -> Vec<(u32, u32)> {
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

/// Row heights (zero-based row to height) after a row edit. A deleted row loses its height, the rows after it move,
/// and an inserted row takes the height a user set on the row above it, as the spreadsheet program's insert copies the
/// format of the row above (PD-277); a height that follows the content is not copied. A column edit and a row edit
/// bounded to some columns leave them as they are.
pub fn shift_row_heights(heights: &RowHeights, edit: &AxisEdit) -> RowHeights {
    if edit.axis == Axis::Col || edit.cols.is_some() {
        return heights.clone();
    }
    let mut out: RowHeights = heights
        .iter()
        .filter_map(|(&row, &height)| Some((edit.map_index(row)?, height)))
        .collect();
    let above = edit
        .at
        .checked_sub(1)
        .and_then(|row| heights.get(&row))
        .filter(|height| height.custom);
    if let (super::EditKind::Insert, Some(&height)) = (edit.kind, above) {
        let end = edit.at.saturating_add(edit.count).min(edit.limit());
        out.extend((edit.at..end).map(|row| (row, height)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::axis::EditKind;

    fn heights(rows: &[(u32, &str, bool)]) -> RowHeights {
        rows.iter()
            .map(|&(row, pt, custom)| {
                (
                    row,
                    easyoffice_formats::RowHeight::parse(pt, custom).unwrap(),
                )
            })
            .collect()
    }

    fn edit(axis: Axis, kind: EditKind, at: u32, count: u32) -> AxisEdit {
        AxisEdit::new(axis, kind, at, count).unwrap()
    }

    #[test]
    fn rows_move_with_an_insertion_and_the_new_rows_copy_the_row_above() {
        let before = heights(&[(1, "30", true), (4, "40", true), (6, "20", false)]);
        let after = shift_row_heights(&before, &edit(Axis::Row, EditKind::Insert, 2, 2));
        assert_eq!(
            after,
            heights(&[
                (1, "30", true),
                (2, "30", true),
                (3, "30", true),
                (6, "40", true),
                (8, "20", false)
            ])
        );
        // An inserted row below a row that follows its content has no height of its own.
        let auto_above = heights(&[(1, "30", false)]);
        assert_eq!(
            shift_row_heights(&auto_above, &edit(Axis::Row, EditKind::Insert, 2, 1)),
            auto_above
        );
        // At the first row there is no row above.
        assert_eq!(
            shift_row_heights(
                &heights(&[(0, "30", true)]),
                &edit(Axis::Row, EditKind::Insert, 0, 1)
            ),
            heights(&[(1, "30", true)])
        );
    }

    #[test]
    fn a_deletion_drops_the_heights_of_its_rows_and_closes_the_gap() {
        let before = heights(&[
            (1, "30", true),
            (2, "35", true),
            (3, "40", true),
            (5, "45", false),
        ]);
        let after = shift_row_heights(&before, &edit(Axis::Row, EditKind::Delete, 2, 2));
        assert_eq!(after, heights(&[(1, "30", true), (3, "45", false)]));
    }

    #[test]
    fn a_column_edit_and_a_column_bounded_row_edit_keep_them() {
        let before = heights(&[(1, "30", true), (4, "40", true)]);
        assert_eq!(
            shift_row_heights(&before, &edit(Axis::Col, EditKind::Insert, 0, 3)),
            before
        );
        let bounded = edit(Axis::Row, EditKind::Delete, 0, 1)
            .with_cols(1, 2)
            .unwrap();
        assert_eq!(shift_row_heights(&before, &bounded), before);
    }
}
