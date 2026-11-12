//! What a worksheet remembers about how it is shown: the frozen rows and columns, the colour of its
//! tab and the outline (row and column groups) with the groups the user folded (PD-208).
//!
//! 참조 구현: Univer `packages/core/src/sheets/typedef.ts` lines 450-468 (`IFreeze`: `xSplit` and
//! `ySplit` count the frozen columns and rows) and `packages/sheets/src/commands/commands/set-frozen.command.ts`
//! lines 44-58 (the counts are non-negative integers below the sheet size), `set-tab-color.command.ts`
//! (the tab colour is one value per sheet that undo restores) and `packages/core/src/types/enum/group-control-toggle-position.ts`
//! (`BEFORE`/`AFTER`: the summary row or column of a group sits before or after it), Apache-2.0,
//! Copyright 2023-present DreamNum Co., Ltd., commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9. The
//! reference has no group levels, so the levels follow ECMA-376 18.3.1.73 (`outlineLevel`, 0 to 7)
//! and 18.3.1.31 (`outlinePr`: `summaryBelow`, `summaryRight`).
//!
//! Modifications: a group's folded state is the set of rows or columns that folding hid
//! ([`Outline::collapsed_rows`]), kept apart from the rows a user hid (`SheetStyling::hidden_rows`)
//! and the rows a filter hid, so each kind is shown or hidden on its own. A file stores one `hidden`
//! flag per row plus a `collapsed` flag on the summary row; [`Outline::split_collapsed`] tells the two
//! kinds apart and [`Outline::summary_marks`] writes the flags back. Every size is bounded.

use crate::xlsx_layout::{add_run, intersect_runs, normalize_runs, run_total, subtract_runs};

/// Deepest group level (ECMA-376 `outlineLevel` is 0 to 7).
pub const MAX_OUTLINE_LEVEL: u8 = 7;
/// Most rows a frozen pane may hold. A taller pane leaves no room to scroll.
pub const MAX_FREEZE_ROWS: u32 = 1_000;
/// Most columns a frozen pane may hold.
pub const MAX_FREEZE_COLS: u32 = 100;
/// Rows (summed over all runs) one sheet may put in groups, and the same bound for folded rows.
pub const MAX_OUTLINE_ROWS: u64 = 100_000;
/// Runs of equal level one axis may carry.
pub const MAX_OUTLINE_RUNS: usize = 100_000;

/// Zero-based inclusive run of rows or columns that share one group level (1 to 7).
pub type LevelRun = (u32, u32, u8);

/// Rows and columns kept in place while the rest scroll. At least one count is above zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Freeze {
    pub rows: u32,
    pub cols: u32,
}

impl Freeze {
    /// The pane for the counts, or `None` when both are zero or one is over its bound.
    pub fn new(rows: u32, cols: u32) -> Option<Self> {
        (rows <= MAX_FREEZE_ROWS && cols <= MAX_FREEZE_COLS && rows + cols > 0)
            .then_some(Self { rows, cols })
    }
}

/// Row and column groups of one sheet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outline {
    /// Rows and columns in a group, as sorted, disjoint runs of equal level.
    pub rows: Vec<LevelRun>,
    pub cols: Vec<LevelRun>,
    /// Rows and columns the user folded: sorted, disjoint runs, each inside a group.
    pub collapsed_rows: Vec<(u32, u32)>,
    pub collapsed_cols: Vec<(u32, u32)>,
    /// The summary row sits above its group, the summary column to its left (the files' default is
    /// below and to the right).
    pub summary_above: bool,
    pub summary_left: bool,
}

/// Level of `index` in sorted `runs`, 0 outside every group.
pub fn level_at(runs: &[LevelRun], index: u32) -> u8 {
    let after = runs.partition_point(|&(s, _, _)| s <= index);
    match after.checked_sub(1).map(|i| runs[i]) {
        Some((_, e, level)) if e >= index => level,
        _ => 0,
    }
}

/// Rows or columns in `runs`.
pub fn level_total(runs: &[LevelRun]) -> u64 {
    runs.iter().map(|&(s, e, _)| u64::from(e - s) + 1).sum()
}

/// The runs of `runs` as plain index runs.
fn plain_runs(runs: &[LevelRun]) -> Vec<(u32, u32)> {
    let mut out = Vec::with_capacity(runs.len());
    for &(s, e, _) in runs {
        add_run(&mut out, s, e);
    }
    out
}

/// Maximal stretches of consecutive indexes whose level is at least `k`.
pub(crate) fn segments(runs: &[LevelRun], k: u8) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for &(s, e, _) in runs.iter().filter(|r| r.2 >= k) {
        match out.last_mut() {
            Some(last) if last.1 + 1 == s => last.1 = e,
            _ => out.push((s, e)),
        }
    }
    out
}

/// The row or column that sums up the stretch `first..=last`.
fn summary_of(first: u32, last: u32, above: bool) -> Option<u32> {
    if above {
        first.checked_sub(1)
    } else {
        last.checked_add(1)
    }
}

/// Adds `index` with `level` to the sorted runs a scan is building. A row that is out of order, a
/// level over the bound and a list over `max_total` rows are dropped.
pub(crate) fn push_level(
    runs: &mut Vec<LevelRun>,
    total: &mut u64,
    max_total: u64,
    (first, last): (u32, u32),
    level: u8,
) {
    let count = u64::from(last - first) + 1;
    if level == 0 || level > MAX_OUTLINE_LEVEL || *total + count > max_total {
        return;
    }
    match runs.last_mut() {
        Some(prev) if first <= prev.1 => return,
        Some(prev) if prev.1 + 1 == first && prev.2 == level => prev.1 = last,
        _ => {
            if runs.len() >= MAX_OUTLINE_RUNS {
                return;
            }
            runs.push((first, last, level));
        }
    }
    *total += count;
}

fn runs_within(runs: &[LevelRun], limit: u32) -> bool {
    runs.windows(2).all(|p| p[0].1 < p[1].0)
        && runs
            .iter()
            .all(|&(s, e, level)| s <= e && e < limit && (1..=MAX_OUTLINE_LEVEL).contains(&level))
}

fn collapsed_within(collapsed: &[(u32, u32)], levels: &[LevelRun], limit: u32) -> bool {
    collapsed.windows(2).all(|p| p[0].1 + 1 < p[1].0)
        && collapsed.iter().all(|&(s, e)| s <= e && e < limit)
        && subtract_runs(collapsed, &plain_runs(levels)).is_empty()
}

impl Outline {
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.cols.is_empty()
    }

    /// Whether every run is sorted, disjoint, within the sheet (`max_row`, `max_col` are the sheet's
    /// sizes) and under the bounds, and every folded row or column lies in a group.
    pub fn within_bounds(&self, max_row: u32, max_col: u32) -> bool {
        self.rows.len() <= MAX_OUTLINE_RUNS
            && self.cols.len() <= MAX_OUTLINE_RUNS
            && level_total(&self.rows) <= MAX_OUTLINE_ROWS
            && level_total(&self.cols) <= u64::from(max_col)
            && runs_within(&self.rows, max_row)
            && runs_within(&self.cols, max_col)
            && run_total(&self.collapsed_rows) <= MAX_OUTLINE_ROWS
            && collapsed_within(&self.collapsed_rows, &self.rows, max_row)
            && collapsed_within(&self.collapsed_cols, &self.cols, max_col)
    }

    /// The deepest level of the rows (columns with `cols`).
    pub fn max_level(&self, cols: bool) -> u8 {
        let runs = if cols { &self.cols } else { &self.rows };
        runs.iter().map(|r| r.2).max().unwrap_or(0)
    }

    /// Of the rows (columns) a file marks hidden, `hidden` loses the ones a folded group hid and
    /// they are returned. A hidden row belongs to a folded group when the stretch of rows at or
    /// above one of its levels is summed up by a row that carries `collapsed` (`marks`, sorted).
    pub fn split_collapsed(
        levels: &[LevelRun],
        hidden: &mut Vec<(u32, u32)>,
        marks: &[u32],
        above: bool,
    ) -> Vec<(u32, u32)> {
        let mut folded: Vec<(u32, u32)> = Vec::new();
        for k in 1..=MAX_OUTLINE_LEVEL {
            for (first, last) in segments(levels, k) {
                let marked = summary_of(first, last, above)
                    .is_some_and(|row| marks.binary_search(&row).is_ok());
                if marked {
                    folded.push((first, last));
                }
            }
        }
        normalize_runs(&mut folded);
        let collapsed = intersect_runs(&folded, hidden);
        *hidden = subtract_runs(hidden, &collapsed);
        collapsed
    }

    /// The rows (columns) a file marks `collapsed`: the summary of every stretch of grouped rows
    /// that is folded as a whole, sorted. The inverse of [`Self::split_collapsed`].
    pub fn summary_marks(levels: &[LevelRun], collapsed: &[(u32, u32)], above: bool) -> Vec<u32> {
        let mut marks = Vec::new();
        for k in 1..=MAX_OUTLINE_LEVEL {
            for (first, last) in segments(levels, k) {
                let folded = intersect_runs(collapsed, &[(first, last)]) == [(first, last)];
                if let Some(row) = summary_of(first, last, above).filter(|_| folded) {
                    marks.push(row);
                }
            }
        }
        marks.sort_unstable();
        marks.dedup();
        marks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freeze_needs_a_pane_inside_the_bounds() {
        assert!(Freeze::new(0, 0).is_none());
        assert_eq!(Freeze::new(2, 0), Some(Freeze { rows: 2, cols: 0 }));
        assert!(Freeze::new(MAX_FREEZE_ROWS + 1, 0).is_none());
        assert!(Freeze::new(0, MAX_FREEZE_COLS + 1).is_none());
        assert!(Freeze::new(u32::MAX, 1).is_none());
    }

    #[test]
    fn levels_are_looked_up_by_position() {
        let runs: Vec<LevelRun> = vec![(2, 5, 1), (3, 4, 2)];
        // Overlap is invalid, the lookup of a valid list is what matters.
        let valid: Vec<LevelRun> = vec![(2, 2, 1), (3, 4, 2), (5, 5, 1)];
        assert_eq!(level_at(&valid, 1), 0);
        assert_eq!(level_at(&valid, 3), 2);
        assert_eq!(level_at(&valid, 5), 1);
        assert_eq!(level_at(&valid, 6), 0);
        assert!(!runs_within(&runs, 100));
        assert!(runs_within(&valid, 100));
        assert!(!runs_within(&[(0, 1, 8)], 100) && !runs_within(&[(0, 1, 0)], 100));
    }

    #[test]
    fn a_nested_group_folds_on_its_own_or_with_the_outer_one() {
        // Rows 1..=9 level 1, rows 3..=5 level 2; summary below.
        let levels: Vec<LevelRun> = vec![(1, 2, 1), (3, 5, 2), (6, 9, 1)];
        // Inner group folded: its summary row is 6.
        let marks = Outline::summary_marks(&levels, &[(3, 5)], false);
        assert_eq!(marks, vec![6]);
        let mut hidden = vec![(3, 5)];
        let folded = Outline::split_collapsed(&levels, &mut hidden, &marks, false);
        assert_eq!((folded, hidden), (vec![(3, 5)], vec![]));
        // Outer group folded as a whole: both summaries are marked.
        let marks = Outline::summary_marks(&levels, &[(1, 9)], false);
        assert_eq!(marks, vec![6, 10]);
        let mut hidden = vec![(1, 9)];
        let folded = Outline::split_collapsed(&levels, &mut hidden, &marks, false);
        assert_eq!((folded, hidden), (vec![(1, 9)], vec![]));
    }

    #[test]
    fn a_hidden_row_in_a_group_without_a_mark_stays_hidden() {
        let levels: Vec<LevelRun> = vec![(1, 4, 1)];
        let mut hidden = vec![(2, 2), (20, 21)];
        let folded = Outline::split_collapsed(&levels, &mut hidden, &[], false);
        assert!(folded.is_empty());
        assert_eq!(hidden, vec![(2, 2), (20, 21)]);
        // A mark on the wrong side does not fold the group.
        let folded = Outline::split_collapsed(&levels, &mut hidden, &[0], false);
        assert!(folded.is_empty());
    }

    #[test]
    fn a_summary_above_the_group_is_the_row_before_it() {
        let levels: Vec<LevelRun> = vec![(5, 8, 1)];
        assert_eq!(Outline::summary_marks(&levels, &[(5, 8)], true), vec![4]);
        assert!(Outline::summary_marks(&[(0, 3, 1)], &[(0, 3)], true).is_empty());
        let mut hidden = vec![(5, 8)];
        let folded = Outline::split_collapsed(&levels, &mut hidden, &[4], true);
        assert_eq!(folded, vec![(5, 8)]);
    }

    #[test]
    fn folded_rows_must_lie_in_a_group() {
        let outline = Outline {
            rows: vec![(2, 5, 1)],
            collapsed_rows: vec![(2, 5)],
            ..Outline::default()
        };
        assert!(outline.within_bounds(100, 10));
        let stray = Outline {
            collapsed_rows: vec![(2, 6)],
            ..outline.clone()
        };
        assert!(!stray.within_bounds(100, 10));
        let past = Outline {
            rows: vec![(2, 200, 1)],
            ..Outline::default()
        };
        assert!(!past.within_bounds(100, 10));
    }

    #[test]
    fn levels_are_collected_in_order_and_bounded() {
        let (mut runs, mut total) = (Vec::new(), 0);
        push_level(&mut runs, &mut total, 10, (1, 1), 1);
        push_level(&mut runs, &mut total, 10, (2, 3), 1);
        push_level(&mut runs, &mut total, 10, (4, 4), 2);
        push_level(&mut runs, &mut total, 10, (3, 3), 1);
        push_level(&mut runs, &mut total, 10, (9, 9), 8);
        push_level(&mut runs, &mut total, 10, (9, 9), 0);
        assert_eq!(runs, vec![(1, 3, 1), (4, 4, 2)]);
        assert_eq!(total, 4);
        push_level(&mut runs, &mut total, 5, (10, 10), 1);
        assert_eq!((runs.len(), total), (3, 5));
        push_level(&mut runs, &mut total, 5, (11, 12), 1);
        assert_eq!((runs.len(), total), (3, 5));
    }
}
