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
}

impl SpillTable {
    pub(crate) fn is_empty(&self) -> bool {
        self.areas.is_empty()
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
                    out.extend(edit.map_index(row).map(|moved| (moved, first, last)));
                } else {
                    out.extend(
                        shift_runs(&[(first, last)], edit)
                            .into_iter()
                            .map(|(a, b)| (row, a, b)),
                    );
                }
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

    /// Whether every cell of the rectangle may be changed.
    fn covers(&self, (r0, c0, r1, c1): (u32, u32, u32, u32)) -> bool {
        let whole_rows = (r0..=r1).all(|r| in_runs(&self.unlocked_rows, r));
        let whole_cols = (c0..=c1).all(|c| in_runs(&self.unlocked_cols, c));
        if self.locked_cells.is_empty() && (whole_rows || whole_cols) {
            return true;
        }
        (r0..=r1).all(|r| (c0..=c1).all(|c| self.is_unlocked(r, c)))
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
            });
            return first;
        }
        let children = grid.data.len() - 1;
        let fits = rect_end(anchor, rows, cols).is_some() && children <= budget;
        let free = fits && self.area_is_free(anchor, rows, cols, sheet_id, ctx);
        let area = Area {
            rows,
            cols,
            blocked: !free,
        };
        if !free {
            ctx.pending.push(Pending {
                sheet_id,
                anchor,
                area,
                grid: None,
            });
            return V::Error(FormulaError::Spill);
        }
        ctx.spill_budget = Some(budget - children);
        for dr in 0..rows {
            for dc in 0..cols {
                if dr + dc > 0 {
                    ctx.claims.insert(
                        PackedWorkbookCoord::new(sheet_id, at(anchor, dr, dc)).0,
                        anchor,
                    );
                }
            }
        }
        ctx.fresh.insert(
            PackedWorkbookCoord::new(sheet_id, anchor).0,
            Arc::clone(&grid),
        );
        ctx.pending.push(Pending {
            sheet_id,
            anchor,
            area,
            grid: Some(grid),
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
        let blocked_by = |cell: PackedCellCoordinate| {
            self.cells.contains_key(&cell)
                || self.spill.owner.get(&cell).is_some_and(|&o| o != anchor)
        };
        if self
            .cell_index()
            .range_coords(anchor.row(), last_row, anchor.col(), last_col)
            .into_iter()
            .any(|cell| cell != anchor && blocked_by(cell))
        {
            return false;
        }
        if ctx.claims.is_empty() {
            return true;
        }
        for dr in 0..rows {
            for dc in 0..cols {
                let key = PackedWorkbookCoord::new(sheet_id, at(anchor, dr, dc)).0;
                if ctx.claims.get(&key).is_some_and(|&o| o != anchor) {
                    return false;
                }
            }
        }
        true
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
}

/// Applies the placements a pass decided to the sheets and the memo.
pub(crate) fn publish(
    wb: &mut WorkbookEngine,
    memo: &mut MemoTable,
    pending: Vec<Pending>,
) -> Published {
    let mut out = Published::default();
    for p in pending {
        let Some(idx) = wb
            .sheets()
            .iter()
            .position(|s| s.resolved_sheet_id() == p.sheet_id)
        else {
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
                let was = memo.get_at(p.sheet_id, cell).cloned();
                if was.as_ref() != Some(&value) {
                    out.before.push((idx, cell, was));
                    out.touched.push((idx, cell));
                    changed = true;
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
    out.infos.push(SpillInfo {
        sheet: usize::from(idx),
        anchor,
        rows: 0,
        cols: 0,
        blocked: false,
    });
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
