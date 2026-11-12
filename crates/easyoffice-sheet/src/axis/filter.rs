//! The auto filter follows a row or column edit: its range grows, shifts, shrinks or goes away,
//! its column conditions move with their columns and the rows it hides move with their rows.
//!
//! 참조 구현: Univer `packages/sheets-filter/src/controllers/sheets-filter.controller.ts` lines
//! 268-315 (`handleInsertColCommand`: an insertion inside the range widens it, one before it
//! shifts it, the conditions of the columns after the insertion move), 353-426
//! (`handleRemoveColCommand`: the conditions of deleted columns are removed, the range shrinks,
//! a range left without columns is removed) 316-352 and 427-518 (`_handleInsertRowCommand`,
//! `_handleRemoveRowCommand`: the range follows the rows, a range whose header row is deleted, or
//! that loses all its rows, is removed), Apache-2.0, Copyright 2023-present DreamNum Co., Ltd.,
//! commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9. The span arithmetic is `AxisEdit::shift_span`
//! (see the header of `axis/mod.rs`). Rows the filter hides move like the user's hidden rows;
//! inserted rows are shown.

use super::{shift_runs, Axis, AxisEdit, EditKind};
use easyoffice_formats::{FilterColumn, SheetFilter, MAX_FILTER_ROWS};

/// The filter after an edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterShift {
    /// `None` when the edit removes the filter.
    pub filter: Option<SheetFilter>,
    /// The edit removed the condition of a column, so the rows that column hid are shown again
    /// only when the caller evaluates the remaining conditions.
    pub reapply: bool,
}

/// `filter` after `edit`.
pub fn shift_filter(filter: &SheetFilter, edit: &AxisEdit) -> FilterShift {
    let gone = FilterShift {
        filter: None,
        reapply: false,
    };
    let range = filter.range;
    let mut out = filter.clone();
    let mut reapply = false;
    match edit.axis {
        Axis::Row => {
            let header_deleted = edit.kind == EditKind::Delete
                && (edit.at..edit.at + edit.count).contains(&range.row0);
            let Some((start, end)) = edit
                .shift_span(range.row0, range.row1)
                .filter(|_| !header_deleted)
            else {
                return gone;
            };
            if end - start >= MAX_FILTER_ROWS {
                return gone;
            }
            (out.range.row0, out.range.row1) = (start, end);
            out.hidden = shift_runs(&filter.hidden, edit);
        }
        Axis::Col => {
            let Some((start, end)) = edit.shift_span(range.col0, range.col1) else {
                return gone;
            };
            (out.range.col0, out.range.col1) = (start, end);
            out.columns = filter
                .columns
                .iter()
                .filter_map(|c| {
                    let col = edit
                        .map_index(c.col)
                        .filter(|col| (start..=end).contains(col));
                    reapply |= col.is_none();
                    Some(FilterColumn {
                        col: col?,
                        criteria: c.criteria.clone(),
                    })
                })
                .collect();
        }
    }
    FilterShift {
        filter: Some(out),
        reapply,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use easyoffice_formats::{CellBox, CustomCondition, FilterCriteria};

    fn edit(axis: Axis, kind: EditKind, at1: u32, count: u32) -> AxisEdit {
        AxisEdit::new(axis, kind, at1 - 1, count).unwrap()
    }

    fn sample() -> SheetFilter {
        let on = |col| FilterColumn {
            col,
            criteria: FilterCriteria::Custom {
                and: false,
                first: CustomCondition {
                    op: None,
                    text: format!("c{col}"),
                },
                second: None,
            },
        };
        // B3:E12 (rows 3..12 are 2..11 zero-based, columns B..E are 1..4); header row 3.
        let mut f = SheetFilter::new(CellBox::parse("B3:E12").unwrap());
        f.columns = vec![on(1), on(3)];
        f.hidden = vec![(4, 5), (9, 9)];
        f
    }

    fn shifted(f: &SheetFilter, e: AxisEdit) -> SheetFilter {
        shift_filter(f, &e).filter.expect("filter kept")
    }

    #[test]
    fn row_insertions_shift_or_widen_the_range_and_move_the_hidden_rows() {
        let f = sample();
        // Before the header: everything moves down.
        let before = shifted(&f, edit(Axis::Row, EditKind::Insert, 2, 2));
        assert_eq!(before.range.to_a1(), "B5:E14");
        assert_eq!(before.hidden, vec![(6, 7), (11, 11)]);
        // Inside: the range widens, the hidden run is cut and the new rows are shown.
        let inside = shifted(&f, edit(Axis::Row, EditKind::Insert, 6, 1));
        assert_eq!(inside.range.to_a1(), "B3:E13");
        assert_eq!(inside.hidden, vec![(4, 4), (6, 6), (10, 10)]);
        // Below the range: nothing changes.
        assert_eq!(shifted(&f, edit(Axis::Row, EditKind::Insert, 13, 4)), f);
    }

    #[test]
    fn row_deletions_shrink_move_or_remove_the_filter() {
        let f = sample();
        let above = shifted(&f, edit(Axis::Row, EditKind::Delete, 1, 2));
        assert_eq!(above.range.to_a1(), "B1:E10");
        assert_eq!(above.hidden, vec![(2, 3), (7, 7)]);
        let inside = shifted(&f, edit(Axis::Row, EditKind::Delete, 5, 2));
        assert_eq!(inside.range.to_a1(), "B3:E10");
        assert_eq!(inside.hidden, vec![(7, 7)]);
        let tail = shifted(&f, edit(Axis::Row, EditKind::Delete, 11, 5));
        assert_eq!(tail.range.to_a1(), "B3:E10");
        // The header row going away, or every row, removes the filter.
        for e in [
            edit(Axis::Row, EditKind::Delete, 3, 1),
            edit(Axis::Row, EditKind::Delete, 1, 5),
            edit(Axis::Row, EditKind::Delete, 3, 10),
        ] {
            assert_eq!(shift_filter(&f, &e).filter, None, "{e:?}");
        }
        // A deletion above that reaches into the range leaves the part that survives.
        assert_eq!(
            shift_filter(&f, &edit(Axis::Row, EditKind::Delete, 1, 2)).reapply,
            false
        );
    }

    #[test]
    fn column_edits_move_the_conditions_with_their_columns() {
        let f = sample();
        let before = shifted(&f, edit(Axis::Col, EditKind::Insert, 1, 2));
        assert_eq!(before.range.to_a1(), "D3:G12");
        assert_eq!(
            before.columns.iter().map(|c| c.col).collect::<Vec<_>>(),
            vec![3, 5]
        );
        // An insertion between the conditions widens the range and moves only the later one.
        let between = shifted(&f, edit(Axis::Col, EditKind::Insert, 3, 1));
        assert_eq!(between.range.to_a1(), "B3:F12");
        assert_eq!(
            between.columns.iter().map(|c| c.col).collect::<Vec<_>>(),
            vec![1, 4]
        );
        assert!(!shift_filter(&f, &edit(Axis::Col, EditKind::Insert, 3, 1)).reapply);
        // Deleting a column with a condition drops it and asks for the rows to be re-evaluated.
        let deleted = shift_filter(&f, &edit(Axis::Col, EditKind::Delete, 2, 1));
        assert!(deleted.reapply);
        let kept = deleted.filter.unwrap();
        assert_eq!(kept.range.to_a1(), "B3:D12");
        assert_eq!(
            kept.columns.iter().map(|c| c.col).collect::<Vec<_>>(),
            vec![2]
        );
        // Every column of the range deleted: the filter goes away.
        assert_eq!(
            shift_filter(&f, &edit(Axis::Col, EditKind::Delete, 2, 4)).filter,
            None
        );
        // Columns right of the range do not matter.
        assert_eq!(shifted(&f, edit(Axis::Col, EditKind::Insert, 6, 2)), f);
    }

    #[test]
    fn a_range_pushed_over_the_row_bound_is_removed() {
        let mut f = SheetFilter::new(CellBox {
            row0: 0,
            col0: 0,
            row1: MAX_FILTER_ROWS - 1,
            col1: 1,
        });
        f.columns = Vec::new();
        assert!(shift_filter(&f, &edit(Axis::Row, EditKind::Insert, 5, 1))
            .filter
            .is_none());
        assert!(shift_filter(&f, &edit(Axis::Row, EditKind::Insert, 1, 1))
            .filter
            .is_some());
    }
}
