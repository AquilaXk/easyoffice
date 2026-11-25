//! Where the array a formula calculated goes (PD-217).
//!
//! A formula whose result is an array is an *anchor*: its own cell shows the first value and the
//! values after it fill the cells to the right and below (its *area*). The cells of an area are not
//! cells of the sheet: they are values of the anchor's formula, kept in the memo (so every reader
//! sees them as it sees any formula cell) and listed in the sheet's [`SpillTable`] and its populated
//! cell index (so ranges that read only populated cells see them too).
//!
//! A pass of evaluation only *decides* a placement ([`SheetEngine::place_array`]): the area is
//! blocked when a cell of it holds something or another formula's area has it, when it leaves the
//! sheet, or when the workbook's spill budget would be passed. The sheets change after the pass
//! ([`publish`]), and what the change touched is what the next pass recalculates, until nothing does.
//! A reader that ran before the area it reads was published is therefore calculated again, so no
//! order of evaluation has to be known in advance.

use crate::array::Grid;
use crate::axis::{shift_runs, Axis, AxisEdit};
use crate::memo::CellMap;
use crate::{
    EvalContext, EvaluatedCellValue as V, FormulaError, MemoTable, PackedCellCoordinate,
    PackedWorkbookCoord, SheetEngine, WorkbookEngine,
};
use std::sync::Arc;

/// Cells that anchors of one workbook may fill (not counting the anchors).
pub const MAX_SPILL_CELLS: usize = 2_097_152;

/// Estimated heap bytes per filled cell: the owner entry and the memoized value.
pub(crate) const SPILL_CELL_BYTES: usize = 96;

/// An area this large rebuilds the sheet's cell index instead of patching it per cell.
const BULK_CELLS: usize = 256;

/// Rounds of "publish, then recalculate what the publishing touched" before the areas are taken
/// as circular (a ring of formulas that each fill the cell another one reads).
pub const MAX_SPILL_ROUNDS: usize = 64;

/// The size an anchor's array has and whether it could be placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Area {
    pub(crate) rows: u32,
    pub(crate) cols: u32,
    /// The array could not be placed: the anchor shows `#SPILL!` and the size is what it asked for.
    pub(crate) blocked: bool,
}

/// The areas of one sheet.
#[derive(Debug, Clone, Default)]
pub(crate) struct SpillTable {
    areas: CellMap<Area>,
    owner: CellMap<PackedCellCoordinate>,
    active_links: crate::DetSet<PackedCellCoordinate>,
}

impl SpillTable {
    pub(crate) fn is_empty(&self) -> bool {
        self.areas.is_empty() && self.active_links.is_empty()
    }

    #[inline]
    pub(crate) fn has_active_link(&self, coord: PackedCellCoordinate) -> bool {
        self.active_links.contains(&coord)
    }

    #[inline]
    pub(crate) fn remove_active_link(&mut self, coord: PackedCellCoordinate) -> bool {
        self.active_links.remove(&coord)
    }

    pub(crate) fn active_links(&self) -> impl Iterator<Item = PackedCellCoordinate> + '_ {
        self.active_links.iter().copied()
    }

    /// Rows and columns of the placed area of the anchor at `anchor`.
    pub(crate) fn area(&self, anchor: PackedCellCoordinate) -> Option<(usize, usize)> {
        self.areas
            .get(&anchor)
            .filter(|a| !a.blocked)
            .map(|a| (a.rows as usize, a.cols as usize))
    }

    pub(crate) fn owner_of(&self, cell: PackedCellCoordinate) -> Option<PackedCellCoordinate> {
        self.owner.get(&cell).copied()
    }

    /// The cells that anchors fill.
    pub(crate) fn filled(&self) -> impl Iterator<Item = &PackedCellCoordinate> {
        self.owner.keys()
    }

    pub(crate) fn filled_count(&self) -> usize {
        self.owner.len()
    }

    /// The anchors whose array could not be placed, with the size they asked for.
    pub(crate) fn blocked(&self) -> impl Iterator<Item = (PackedCellCoordinate, Area)> + '_ {
        self.areas
            .iter()
            .filter(|(_, a)| a.blocked)
            .map(|(&c, &a)| (c, a))
    }

    pub(crate) fn anchors_with_area(
        &self,
    ) -> impl Iterator<Item = (PackedCellCoordinate, Area)> + '_ {
        self.areas.iter().map(|(&c, &a)| (c, a))
    }

    pub(crate) fn area_entry(&self, anchor: PackedCellCoordinate) -> Option<Area> {
        self.areas.get(&anchor).copied()
    }
}

/// Which cells of a protected sheet may be changed (PD-217): an array cannot be placed over a cell
/// that may not, as it cannot be typed into. Unlocked cells are lists of `(row, first column, last
/// column)`, sorted; rows and columns are runs `(first, last)`, zero-based.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SheetLock {
    pub unlocked_cells: Vec<(u32, u32, u32)>,
    pub locked_cells: Vec<(u32, u32, u32)>,
    pub unlocked_rows: Vec<(u32, u32)>,
    pub unlocked_cols: Vec<(u32, u32)>,
}

fn in_cells(cells: &[(u32, u32, u32)], row: u32, col: u32) -> bool {
    let from = cells.partition_point(|&(r, _, _)| r < row);
    cells[from..]
        .iter()
        .take_while(|&&(r, _, _)| r == row)
        .any(|&(_, c0, c1)| (c0..=c1).contains(&col))
}

fn in_runs(runs: &[(u32, u32)], at: u32) -> bool {
    runs.iter().any(|&(a, b)| (a..=b).contains(&at))
}

fn covered_in_runs(runs: &[(u32, u32)], lo: u32, hi: u32) -> u64 {
    if lo > hi || runs.is_empty() {
        return 0;
    }
    let mut intervals = Vec::with_capacity(runs.len());
    for &(a, b) in runs {
        let a_clamped = a.max(lo);
        let b_clamped = b.min(hi);
        if a_clamped <= b_clamped {
            intervals.push((a_clamped, b_clamped));
        }
    }
    if intervals.is_empty() {
        return 0;
    }
    intervals.sort_unstable_by_key(|&(a, _)| a);
    let mut total = 0u64;
    let (mut cur_a, mut cur_b) = intervals[0];
    for &(a, b) in &intervals[1..] {
        if a <= cur_b.saturating_add(1) {
            cur_b = cur_b.max(b);
        } else {
            total += u64::from(cur_b - cur_a + 1);
            cur_a = a;
            cur_b = b;
        }
    }
    total += u64::from(cur_b - cur_a + 1);
    total
}

impl SheetLock {
    /// Whether the cell may be changed.
    pub fn is_unlocked(&self, row: u32, col: u32) -> bool {
        !in_cells(&self.locked_cells, row, col)
            && (in_cells(&self.unlocked_cells, row, col)
                || in_runs(&self.unlocked_rows, row)
                || in_runs(&self.unlocked_cols, col))
    }

    /// The lock after the rows or columns of the sheet move for `edit`: a cell keeps its mark, a
    /// deleted cell loses it, and an inserted cell has none (it stays locked on a protected sheet).
    pub(crate) fn shifted(&self, edit: &AxisEdit) -> SheetLock {
        let by_rows = edit.axis == Axis::Row;
        let cells = |list: &[(u32, u32, u32)]| -> Vec<(u32, u32, u32)> {
            let mut out = Vec::with_capacity(list.len());
            for &(row, first, last) in list {
                if by_rows {
                    if let Some((lo, hi)) = edit.cols {
                        if first < lo {
                            out.push((row, first, last.min(lo - 1)));
                        }
                        let mid0 = first.max(lo);
                        let mid1 = last.min(hi);
                        if mid0 <= mid1 {
                            out.extend(edit.map_index(row).map(|moved| (moved, mid0, mid1)));
                        }
                        if last > hi {
                            out.push((row, first.max(hi + 1), last));
                        }
                    } else {
                        out.extend(edit.map_index(row).map(|moved| (moved, first, last)));
                    }
                } else {
                    out.extend(
                        shift_runs(&[(first, last)], edit)
                            .into_iter()
                            .map(|(a, b)| (row, a, b)),
                    );
                }
            }
            if by_rows && edit.cols.is_some() {
                out.sort_unstable();
            }
            out
        };
        let runs = |list: &[(u32, u32)], axis: Axis| {
            if edit.axis == axis {
                shift_runs(list, edit)
            } else {
                list.to_vec()
            }
        };
        SheetLock {
            unlocked_cells: cells(&self.unlocked_cells),
            locked_cells: cells(&self.locked_cells),
            unlocked_rows: runs(&self.unlocked_rows, Axis::Row),
            unlocked_cols: runs(&self.unlocked_cols, Axis::Col),
        }
    }

    /// Whether every cell of the rectangle may be changed in O(K log K) run-interval checks.
    fn covers(&self, (r0, c0, r1, c1): (u32, u32, u32, u32)) -> bool {
        if r0 > r1 || c0 > c1 {
            return true;
        }
        // 1. Any explicitly locked cell inside the rectangle immediately violates full coverage.
        if !self.locked_cells.is_empty() {
            let hit_locked = self
                .locked_cells
                .iter()
                .any(|&(lr, lc0, lc1)| (r0..=r1).contains(&lr) && lc0 <= c1 && c0 <= lc1);
            if hit_locked {
                return false;
            }
        }

        let total_rows = u64::from(r1 - r0 + 1);
        let covered_rows = covered_in_runs(&self.unlocked_rows, r0, r1);
        let rem_rows = total_rows.saturating_sub(covered_rows);

        let total_cols = u64::from(c1 - c0 + 1);
        let covered_cols = covered_in_runs(&self.unlocked_cols, c0, c1);
        let rem_cols = total_cols.saturating_sub(covered_cols);

        if rem_rows == 0 || rem_cols == 0 {
            return true;
        }

        let clipped_cols: Vec<(u32, u32)> = self
            .unlocked_cols
            .iter()
            .filter_map(|&(a, b)| {
                let lo = a.max(c0);
                let hi = b.min(c1);
                (lo <= hi).then_some((lo, hi))
            })
            .collect();

        // Group unlocked_cells by row clipped to c0..=c1, merging with clipped_cols per row.
        let from = self.unlocked_cells.partition_point(|&(r, _, _)| r < r0);
        let mut checked_rows = 0u64;
        let mut row_intervals: Vec<(u32, u32)> = Vec::new();
        let mut cur_row: Option<u32> = None;

        for &(r, c0_cell, c1_cell) in &self.unlocked_cells[from..] {
            if r > r1 {
                break;
            }
            if in_runs(&self.unlocked_rows, r) {
                continue;
            }
            let overlap_c0 = c0_cell.max(c0);
            let overlap_c1 = c1_cell.min(c1);
            if overlap_c0 <= overlap_c1 {
                if cur_row != Some(r) {
                    if cur_row.is_some() {
                        row_intervals.extend_from_slice(&clipped_cols);
                        if covered_in_runs(&row_intervals, c0, c1) < total_cols {
                            return false;
                        }
                        row_intervals.clear();
                        checked_rows += 1;
                    }
                    cur_row = Some(r);
                }
                row_intervals.push((overlap_c0, overlap_c1));
            }
        }
        if cur_row.is_some() {
            row_intervals.extend_from_slice(&clipped_cols);
            if covered_in_runs(&row_intervals, c0, c1) < total_cols {
                return false;
            }
            checked_rows += 1;
        }

        checked_rows == rem_rows
    }
}

/// What a pass decided for one anchor.
#[derive(Debug)]
pub(crate) struct Pending {
    pub(crate) sheet_id: u16,
    pub(crate) anchor: PackedCellCoordinate,
    pub(crate) area: Area,
    /// The values of a placed array bigger than one cell.
    pub(crate) grid: Option<Arc<Grid>>,
    /// Whether `(0, 0)` of the formula's grid carries a live `HYPERLINK` target (PD-236).
    pub(crate) anchor_link: bool,
}

/// An anchor whose area changed in a recalculation: `rows` and `cols` are 0 when it no longer
/// spills, `blocked` when it shows `#SPILL!`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpillInfo {
    /// Index of the sheet in workbook order.
    pub sheet: usize,
    pub anchor: PackedCellCoordinate,
    pub rows: usize,
    pub cols: usize,
    pub blocked: bool,
}

/// The cells of the area `rows` by `cols` at `anchor`, the anchor itself first; `None` past the sheet.
fn rect_end(anchor: PackedCellCoordinate, rows: u32, cols: u32) -> Option<(u32, u32)> {
    let last_row = anchor.row().checked_add(rows)?.checked_sub(1)?;
    let last_col = anchor.col().checked_add(cols)?.checked_sub(1)?;
    (last_row < PackedCellCoordinate::MAX_ROWS && last_col < PackedCellCoordinate::MAX_COLS)
        .then_some((last_row, last_col))
}

fn at(anchor: PackedCellCoordinate, r: u32, c: u32) -> PackedCellCoordinate {
    PackedCellCoordinate((u64::from(anchor.row() + r) << 32) | u64::from(anchor.col() + c))
}

impl SheetEngine {
    /// Decides where the array `grid` of the formula at `anchor` goes and returns the anchor's value:
    /// its first value, or `#SPILL!` when the array cannot be placed. Outside a session
    /// (`ctx.spill_budget` is `None`) nothing is placed and the value is the first one.
    pub(crate) fn place_array(
        &self,
        anchor: PackedCellCoordinate,
        grid: Arc<Grid>,
        ctx: &mut EvalContext<'_>,
    ) -> V {
        let first = match grid.top_left() {
            V::Empty => V::Number(0.0),
            v => v,
        };
        let Some(budget) = ctx.spill_budget else {
            return first;
        };
        let sheet_id = self.resolved_sheet_id();
        let (rows, cols) = (grid.rows as u32, grid.cols as u32);
        let anchor_link = grid.link_id(0, 0) != 0 && !matches!(first, V::Error(_));
        if grid.is_scalar() {
            let area = Area {
                rows: 1,
                cols: 1,
                blocked: false,
            };
            ctx.pending.push(Pending {
                sheet_id,
                anchor,
                area,
                grid: None,
                anchor_link,
            });
            return first;
        }
        let children = grid.data.len() - 1;
        let candidate_cost = children.min(budget).max(1);
        ctx.steps = ctx.steps.saturating_add(children.min(1024));
        if ctx.expired() {
            return V::Error(FormulaError::Spill);
        }
        let fits = rect_end(anchor, rows, cols).is_some() && children <= budget;
        let free = fits && self.area_is_free(anchor, rows, cols, sheet_id, ctx);
        let area = Area {
            rows,
            cols,
            blocked: !free,
        };
        if !free {
            ctx.spill_budget = Some(budget.saturating_sub(candidate_cost));
            ctx.pending.push(Pending {
                sheet_id,
                anchor,
                area,
                grid: None,
                anchor_link: false,
            });
            return V::Error(FormulaError::Spill);
        }
        ctx.spill_budget = Some(budget - children);
        let (last_row, last_col) = rect_end(anchor, rows, cols).unwrap();
        ctx.claims.push(crate::eval_context::ClaimedSpillRect {
            sheet_id,
            r0: anchor.row(),
            c0: anchor.col(),
            r1: last_row,
            c1: last_col,
            owner: anchor,
        });
        ctx.fresh.insert(
            PackedWorkbookCoord::new(sheet_id, anchor).0,
            Arc::clone(&grid),
        );
        ctx.pending.push(Pending {
            sheet_id,
            anchor,
            area,
            grid: Some(grid),
            anchor_link,
        });
        first
    }

    /// No cell of the area but the anchor holds a value, belongs to another anchor's area or was
    /// claimed by another anchor in this pass.
    fn area_is_free(
        &self,
        anchor: PackedCellCoordinate,
        rows: u32,
        cols: u32,
        sheet_id: u16,
        ctx: &EvalContext<'_>,
    ) -> bool {
        let Some((last_row, last_col)) = rect_end(anchor, rows, cols) else {
            return false;
        };
        let area = (anchor.row(), anchor.col(), last_row, last_col);
        if [&self.spill_merges, &self.spill_tables]
            .into_iter()
            .any(|blockers| blockers.as_ref().is_some_and(|b| b.touches(area)))
        {
            return false;
        }
        if let Some(lock) = &self.spill_lock {
            // The anchor's cell is the formula's own; the cells after it must be ones that may be changed.
            let rows = (anchor.row(), last_row);
            let cols = (anchor.col(), last_col);
            let rest_ok = |r0, c0, r1, c1| r0 > r1 || c0 > c1 || lock.covers((r0, c0, r1, c1));
            let ok = rest_ok(rows.0, cols.0 + 1, rows.1, cols.1)
                && rest_ok(rows.0 + 1, cols.0, rows.1, cols.1);
            if !ok {
                return false;
            }
        }
        if self.cell_index().has_spill_collision(
            anchor.row(),
            last_row,
            anchor.col(),
            last_col,
            anchor,
            &self.cells,
            &self.spill.owner,
        ) {
            return false;
        }
        if ctx.claims.is_empty() {
            return true;
        }
        !ctx.claims.has_overlap(
            sheet_id,
            anchor.row(),
            anchor.col(),
            last_row,
            last_col,
            anchor,
        )
    }
}

/// What publishing changed.
#[derive(Debug, Default)]
pub(crate) struct Published {
    /// Cells whose value or presence changed, and anchors whose area or values changed: the next
    /// pass recalculates what reads them.
    pub(crate) touched: Vec<(u16, PackedCellCoordinate)>,
    /// The value a filled cell had before (`None`: it was not filled), once per cell.
    pub(crate) before: Vec<(u16, PackedCellCoordinate, Option<V>)>,
    pub(crate) infos: Vec<SpillInfo>,
    /// The cells an anchor gave up (a blocked anchor that asked for them may fit now).
    pub(crate) freed: Vec<(u16, PackedCellCoordinate)>,
    /// Cells whose `HYPERLINK` flag toggled even if their display value stayed equal (PD-236).
    pub(crate) link_toggled: Vec<(u16, PackedCellCoordinate)>,
}

/// Applies the placements a pass decided to the sheets and the memo.
pub(crate) fn publish(
    wb: &mut WorkbookEngine,
    memo: &mut MemoTable,
    pending: Vec<Pending>,
) -> Published {
    let mut out = Published::default();
    for p in pending {
        let Some(idx) = wb.find_sheet_index_by_id(p.sheet_id).or_else(|| {
            wb.sheets()
                .iter()
                .position(|s| s.resolved_sheet_id() == p.sheet_id)
        }) else {
            continue;
        };
        publish_one(&mut wb.sheets_mut()[idx], idx as u16, memo, &p, &mut out);
    }
    out
}

fn publish_one(
    sheet: &mut SheetEngine,
    idx: u16,
    memo: &mut MemoTable,
    p: &Pending,
    out: &mut Published,
) {
    let anchor = p.anchor;
    let next_anchor_link = !p.area.blocked && p.anchor_link;
    let prev_anchor_link = if next_anchor_link {
        !sheet.spill.active_links.insert(anchor)
    } else {
        sheet.spill.active_links.remove(&anchor)
    };
    if prev_anchor_link != next_anchor_link {
        out.link_toggled.push((idx, anchor));
    }
    let old = sheet.spill.areas.get(&anchor).copied();
    // Many cells at once: the populated-cell index is rebuilt in one sorted pass instead of patched cell by cell.
    let cells = |a: Option<Area>| a.map_or(0, |a| a.rows as usize * a.cols as usize);
    if cells(old).max(cells(Some(p.area))) > BULK_CELLS {
        sheet.drop_indexes();
    }
    let new_extent = if p.area.blocked {
        (1, 1)
    } else {
        (p.area.rows, p.area.cols)
    };
    let mut shape = old != Some(p.area);
    let mut changed = false;
    if let Some(old) = old.filter(|o| !o.blocked) {
        for dr in 0..old.rows {
            for dc in 0..old.cols {
                if (dr + dc > 0) && (dr >= new_extent.0 || dc >= new_extent.1) {
                    let cell = at(anchor, dr, dc);
                    sheet.spill.owner.remove(&cell);
                    if sheet.spill.active_links.remove(&cell) {
                        out.link_toggled.push((idx, cell));
                    }
                    let kept = sheet.cells.contains_key(&cell);
                    if let Some(index) = sheet.cell_index.get_mut() {
                        index.set_populated(cell, kept);
                    }
                    let was = memo.remove_at(p.sheet_id, cell);
                    out.before.push((idx, cell, was));
                    out.touched.push((idx, cell));
                    out.freed.push((idx, cell));
                    shape = true;
                }
            }
        }
    }
    if let Some(grid) = &p.grid {
        for dr in 0..p.area.rows {
            for dc in 0..p.area.cols {
                if dr + dc == 0 {
                    continue;
                }
                let cell = at(anchor, dr, dc);
                let value = match grid.get(dr as usize, dc as usize) {
                    V::Empty => V::Number(0.0),
                    v => v.clone(),
                };
                if sheet.spill.owner.insert(cell, anchor).is_none() {
                    if let Some(index) = sheet.cell_index.get_mut() {
                        index.set_populated(cell, true);
                    }
                }
                let next_link =
                    grid.link_id(dr as usize, dc as usize) != 0 && !matches!(value, V::Error(_));
                let prev_link = if next_link {
                    !sheet.spill.active_links.insert(cell)
                } else {
                    sheet.spill.active_links.remove(&cell)
                };
                let was = memo.get_at(p.sheet_id, cell).cloned();
                if was.as_ref() != Some(&value) || prev_link != next_link {
                    out.before.push((idx, cell, was.clone()));
                    if prev_link != next_link {
                        out.link_toggled.push((idx, cell));
                    }
                    if was.as_ref() != Some(&value) {
                        out.touched.push((idx, cell));
                        changed = true;
                    }
                }
                memo.insert_at(p.sheet_id, cell, value);
            }
        }
    }
    sheet.spill.areas.insert(anchor, p.area);
    if shape || changed {
        out.touched.push((idx, anchor));
    }
    let single = p.area.rows * p.area.cols == 1 && !p.area.blocked;
    if shape && !(old.is_none() && single) {
        let (rows, cols) = if single {
            (0, 0)
        } else {
            (p.area.rows as usize, p.area.cols as usize)
        };
        out.infos.push(SpillInfo {
            sheet: usize::from(idx),
            anchor,
            rows,
            cols,
            blocked: p.area.blocked,
        });
    }
}

/// Drops the area of the anchor at `anchor` (its formula went away): the filled cells are
/// forgotten and reported as touched.
pub(crate) fn release_anchor(
    sheet: &mut SheetEngine,
    idx: u16,
    memo: &mut MemoTable,
    anchor: PackedCellCoordinate,
    out: &mut Published,
) {
    let had_link = sheet.spill.active_links.remove(&anchor);
    if had_link {
        out.link_toggled.push((idx, anchor));
    }
    let Some(old) = sheet.spill.areas.remove(&anchor) else {
        return;
    };
    if old.rows as usize * old.cols as usize > BULK_CELLS {
        sheet.drop_indexes();
    }
    let sid = sheet.resolved_sheet_id();
    if !old.blocked {
        for dr in 0..old.rows {
            for dc in 0..old.cols {
                if dr + dc > 0 {
                    let cell = at(anchor, dr, dc);
                    if sheet.spill.owner.get(&cell) == Some(&anchor) {
                        sheet.spill.owner.remove(&cell);
                        if sheet.spill.active_links.remove(&cell) {
                            out.link_toggled.push((idx, cell));
                        }
                        let kept = sheet.cells.contains_key(&cell);
                        if let Some(index) = sheet.cell_index.get_mut() {
                            index.set_populated(cell, kept);
                        }
                        let was = memo.remove_at(sid, cell);
                        out.before.push((idx, cell, was));
                        out.touched.push((idx, cell));
                        out.freed.push((idx, cell));
                    }
                }
            }
        }
    }
    if old.rows * old.cols > 1 || old.blocked {
        out.infos.push(SpillInfo {
            sheet: usize::from(idx),
            anchor,
            rows: 0,
            cols: 0,
            blocked: false,
        });
    }
}

impl SheetEngine {
    /// Sets which cells of the (protected) sheet may be changed; `None` for a sheet that is not protected.
    pub fn set_spill_lock(&mut self, lock: Option<SheetLock>) {
        self.spill_lock = lock.map(Arc::new);
    }

    /// Sets the merged ranges of the sheet, which an array may not cross; an empty list for a sheet with none.
    pub fn set_spill_merges(&mut self, merges: Vec<easyoffice_formats::CellBox>) {
        self.spill_merges =
            (!merges.is_empty()).then(|| Arc::new(crate::spill_merges::SpillMerges::new(merges)));
    }

    /// Sets the tables of the sheet, which an array may not be placed in or cross; an empty list for a sheet with none.
    pub fn set_spill_tables(&mut self, tables: Vec<easyoffice_formats::CellBox>) {
        self.spill_tables =
            (!tables.is_empty()).then(|| Arc::new(crate::spill_merges::SpillMerges::new(tables)));
    }

    /// Sets the rows the user and a filter hid (sorted, disjoint, zero-based inclusive runs), which `SUBTOTAL` and `AGGREGATE`
    /// leave out (PD-232). Call it before the cells are calculated; a session uses
    /// [`RecalcSession::set_sheet_hidden_rows`](crate::RecalcSession::set_sheet_hidden_rows). True when the rows changed.
    pub fn set_hidden_rows(&mut self, hidden: Vec<(u32, u32)>, filtered: Vec<(u32, u32)>) -> bool {
        let next = crate::hidden_rows::HiddenRows::new(hidden, filtered);
        let changed = self.hidden_rows != next;
        self.hidden_rows = next;
        changed
    }

    /// Forgets every area of the sheet (the cells they filled stop being cells).
    pub(crate) fn clear_spills(&mut self) {
        if !self.spill.is_empty() {
            self.spill = SpillTable::default();
            self.drop_indexes();
        }
    }

    /// Calculates a formula with the array evaluator and returns the value of its cell: the first
    /// value of the array, placed when the pass places arrays (`anchor` is the formula's cell, `None`
    /// for a formula that is not in a cell).
    pub(crate) fn eval_array_formula(
        &self,
        expr: &crate::Expr,
        anchor: Option<PackedCellCoordinate>,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> V {
        let mark = ctx.temp.len();
        let val = self.eval_val(expr, &crate::array::Env::default(), ctx, depth);
        ctx.temp.truncate(mark);
        let grid = match val {
            Ok(crate::array::Val::Grid(grid)) => grid,
            Ok(
                crate::array::Val::Lambda(_)
                | crate::array::Val::FuncGrid(_)
                | crate::array::Val::Ref(_)
                | crate::array::Val::Omitted,
            ) => return V::Error(FormulaError::Calc),
            Err(e) => return V::Error(e),
        };
        match anchor {
            Some(anchor) => self.place_array(anchor, grid, ctx),
            None => grid.top_left(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheet_lock_covers_large_ranges_instantly() {
        let lock = SheetLock {
            unlocked_cells: vec![(10, 0, 100), (10, 50, 150)], // overlapping
            locked_cells: vec![],
            unlocked_rows: vec![(0, 1_048_575)],
            unlocked_cols: vec![],
        };
        // Covered by whole rows run: instant
        assert!(lock.covers((0, 0, 1_048_575, 100)));

        let lock2 = SheetLock {
            unlocked_cells: vec![(10, 0, 100), (10, 50, 150)],
            locked_cells: vec![],
            unlocked_rows: vec![(0, 5), (7, 10)],
            unlocked_cols: vec![(0, 16_383)],
        };
        // Covered by whole cols run: instant
        assert!(lock2.covers((0, 0, 1_048_575, 16_383)));

        let lock3 = SheetLock {
            unlocked_cells: vec![(6, 0, 100)],
            locked_cells: vec![],
            unlocked_rows: vec![(0, 5), (7, 10)],
            unlocked_cols: vec![],
        };
        // Row 6 not covered outside col 0..100
        assert!(!lock3.covers((0, 0, 10, 200)));
    }
}
