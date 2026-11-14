//! Row and column groups follow the same edit as the cells (PD-208).
//!
//! 참조 구현: the span rule is Univer's (`axis/mod.rs` header). A group that an insertion lies
//! strictly inside grows, one that starts at or after the insertion shifts, as the hidden-row runs
//! of `layout.rs` do. The reference has no groups, so the level of inserted rows follows what a
//! spreadsheet program does: they take the smaller level of the rows around them, which keeps rows
//! added inside a group in the group and rows added at its edge outside it.
//!
//! Modifications: the rows (columns) a folded group hid stay hidden when rows are inserted inside
//! them, since the new rows lie in the group; a folded row that no longer lies in a group is shown.

use super::{Axis, AxisEdit, EditKind};
use easyoffice_formats::{intersect_runs, level_at, normalize_runs, LevelRun, Outline};

/// Level runs after `edit`: deleted rows leave their group, surviving runs shift, and inserted rows
/// take the smaller level of their neighbours.
pub fn shift_levels(runs: &[LevelRun], edit: &AxisEdit) -> Vec<LevelRun> {
    let mut moved: Vec<LevelRun> = runs
        .iter()
        .filter_map(|&(s, e, level)| {
            let (ns, ne) = edit.shift_span(s, e)?;
            Some((ns, ne, level))
        })
        .collect();
    if edit.kind == EditKind::Insert && edit.at > 0 {
        let (above, below) = (level_at(runs, edit.at - 1), level_at(runs, edit.at));
        let level = above.min(below);
        let end = edit.at + edit.count - 1;
        // Rows inserted inside one run are already part of it.
        if level > 0 && level_at(&moved, edit.at) == 0 && end < edit.limit() {
            moved.push((edit.at, end, level));
            moved.sort_unstable();
        }
    }
    let mut merged: Vec<LevelRun> = Vec::with_capacity(moved.len());
    for run in moved {
        match merged.last_mut() {
            Some(last) if last.1 + 1 == run.0 && last.2 == run.2 => last.1 = run.1,
            _ => merged.push(run),
        }
    }
    merged
}

/// The groups of `outline` after `edit`.
pub fn shift_outline(outline: &Outline, edit: &AxisEdit) -> Outline {
    let mut moved = outline.clone();
    let (levels, folded, open) = match edit.axis {
        Axis::Row => (
            &mut moved.rows,
            &mut moved.collapsed_rows,
            &mut moved.open_rows,
        ),
        Axis::Col => (
            &mut moved.cols,
            &mut moved.collapsed_cols,
            &mut moved.open_cols,
        ),
    };
    *levels = shift_levels(levels, edit);
    let mut runs: Vec<(u32, u32)> = folded
        .iter()
        .filter_map(|&(s, e)| edit.shift_span(s, e))
        .collect();
    normalize_runs(&mut runs);
    let mut grouped: Vec<(u32, u32)> = levels.iter().map(|&(s, e, _)| (s, e)).collect();
    normalize_runs(&mut grouped);
    *folded = intersect_runs(&runs, &grouped);
    // The groups left open inside folded rows move with their rows; one the edit cut, joined or took out of the
    // folded rows is folded with them.
    let shifted: Vec<LevelRun> = open
        .iter()
        .filter_map(|&(s, e, level)| {
            let (ns, ne) = edit.shift_span(s, e)?;
            Some((ns, ne, level))
        })
        .collect();
    *open = Outline::settle_open(levels, folded, shifted);
    moved
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(axis: Axis, kind: EditKind, at: u32, count: u32) -> AxisEdit {
        AxisEdit::new(axis, kind, at, count).unwrap()
    }

    #[test]
    fn rows_inserted_inside_a_group_join_it_and_at_its_edge_stay_outside() {
        let runs = vec![(2, 5, 1)];
        let ins = |at, n| shift_levels(&runs, &edit(Axis::Row, EditKind::Insert, at, n));
        assert_eq!(ins(3, 2), vec![(2, 7, 1)]);
        assert_eq!(ins(2, 1), vec![(3, 6, 1)]);
        assert_eq!(ins(6, 1), vec![(2, 5, 1)]);
        assert_eq!(ins(0, 3), vec![(5, 8, 1)]);
    }

    #[test]
    fn rows_between_two_levels_take_the_smaller_one() {
        let runs = vec![(2, 3, 1), (4, 5, 2)];
        let moved = shift_levels(&runs, &edit(Axis::Row, EditKind::Insert, 4, 2));
        assert_eq!(moved, vec![(2, 5, 1), (6, 7, 2)]);
    }

    #[test]
    fn deleting_rows_shrinks_and_removes_groups() {
        let runs = vec![(2, 5, 1), (8, 9, 2)];
        let del = |at, n| shift_levels(&runs, &edit(Axis::Row, EditKind::Delete, at, n));
        assert_eq!(del(3, 2), vec![(2, 3, 1), (6, 7, 2)]);
        assert_eq!(del(8, 2), vec![(2, 5, 1)]);
        assert_eq!(del(0, 2), vec![(0, 3, 1), (6, 7, 2)]);
        assert_eq!(del(4, 4), vec![(2, 3, 1), (4, 5, 2)]);
    }

    #[test]
    fn an_inner_group_left_open_inside_a_folded_one_moves_with_its_rows() {
        let outline = Outline {
            rows: vec![(2, 3, 1), (4, 5, 2), (6, 8, 1)],
            collapsed_rows: vec![(2, 8)],
            open_rows: vec![(4, 5, 2)],
            ..Outline::default()
        };
        let before = shift_outline(&outline, &edit(Axis::Row, EditKind::Insert, 0, 2));
        assert_eq!(
            (before.collapsed_rows, before.open_rows),
            (vec![(4, 10)], vec![(6, 7, 2)])
        );
        let inside = shift_outline(&outline, &edit(Axis::Row, EditKind::Insert, 5, 1));
        assert_eq!(inside.open_rows, vec![(4, 6, 2)]);
        // The edit that takes the inner group out leaves nothing to remember.
        let gone = shift_outline(&outline, &edit(Axis::Row, EditKind::Delete, 4, 2));
        assert!(gone.open_rows.is_empty());
        let cols = shift_outline(&outline, &edit(Axis::Col, EditKind::Insert, 0, 1));
        assert_eq!(cols.open_rows, outline.open_rows);
    }

    #[test]
    fn a_row_edit_leaves_the_columns_and_folded_rows_follow_their_group() {
        let outline = Outline {
            rows: vec![(2, 5, 1)],
            collapsed_rows: vec![(2, 5)],
            cols: vec![(1, 2, 1)],
            ..Outline::default()
        };
        let inside = shift_outline(&outline, &edit(Axis::Row, EditKind::Insert, 3, 2));
        assert_eq!(inside.rows, vec![(2, 7, 1)]);
        assert_eq!(inside.collapsed_rows, vec![(2, 7)]);
        assert_eq!(inside.cols, outline.cols);
        let below = shift_outline(&outline, &edit(Axis::Row, EditKind::Delete, 2, 4));
        assert!(below.rows.is_empty() && below.collapsed_rows.is_empty());
        let cols = shift_outline(&outline, &edit(Axis::Col, EditKind::Insert, 0, 1));
        assert_eq!(cols.cols, vec![(2, 3, 1)]);
        assert_eq!(cols.collapsed_rows, outline.collapsed_rows);
    }
}
