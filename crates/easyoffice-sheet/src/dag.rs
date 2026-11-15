//! Dependency pre-pass for the sheet engine.
//!
//! Formulas are evaluated bottom-up (post-order of an explicit-stack DFS) so that dependency
//! chains deeper than the recursion budget still evaluate and cycles are detected without
//! recursion. This module owns that pass, the cached recalculation plan (the post-order of the
//! last full recalculation, dropped on every edit), and the per-column formula index that lets
//! range dependencies skip empty and literal cells.

use crate::memo::{DetSet, DfsState, FastMap};
use crate::{
    CellRange, EvalContext, EvaluatedCellValue, Expr, FormulaError, MemoTable,
    PackedCellCoordinate, SheetEngine, WorkbookEngine,
};
use std::collections::HashMap;

/// Ranges at most this large are walked cell by cell; larger ones use the column index.
const SMALL_RANGE_CELLS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct PackedDfsFrame(pub u64);

impl PackedDfsFrame {
    #[inline]
    pub const fn new(slot: u16, coord: PackedCellCoordinate, expanded: bool) -> Self {
        let sheet_bits = (slot as u64) << 48;
        let row_bits = ((coord.row() as u64) & 0x000F_FFFF) << 28;
        let col_bits = ((coord.col() as u64) & 0x07FF_FFFF) << 1;
        let exp_bits = expanded as u64;
        Self(sheet_bits | row_bits | col_bits | exp_bits)
    }

    #[inline]
    pub const fn slot(self) -> u16 {
        (self.0 >> 48) as u16
    }

    #[inline]
    pub fn coord(self) -> PackedCellCoordinate {
        let row = ((self.0 >> 28) & 0x000F_FFFF) as u32;
        let col = ((self.0 >> 1) & 0x07FF_FFFF) as u32;
        PackedCellCoordinate((u64::from(row) << 32) | u64::from(col))
    }

    #[inline]
    pub const fn expanded(self) -> bool {
        (self.0 & 1) != 0
    }

    #[inline]
    pub fn unpack(self) -> (u16, PackedCellCoordinate, bool) {
        (self.slot(), self.coord(), self.expanded())
    }
}

const _: () = assert!(std::mem::size_of::<PackedDfsFrame>() == 8);

type Frame = PackedDfsFrame;

#[inline]
pub(crate) fn is_formula(entry: Option<&Result<Expr, FormulaError>>) -> bool {
    matches!(
        entry,
        Some(Ok(expr)) if !matches!(expr, Expr::Number(_) | Expr::Boolean(_))
    )
}

pub(crate) fn expr_contains_subtotal(expr: &Expr) -> bool {
    match expr {
        Expr::FuncCall(f) => {
            f.name.eq_ignore_ascii_case("SUBTOTAL")
                || f.name.eq_ignore_ascii_case("AGGREGATE")
                || f.args.iter().any(expr_contains_subtotal)
        }
        Expr::UnaryPlus(inner) | Expr::UnaryMinus(inner) | Expr::PostfixPercent(inner) => {
            expr_contains_subtotal(inner)
        }
        Expr::Binary { lhs, rhs, .. } => expr_contains_subtotal(lhs) || expr_contains_subtotal(rhs),
        // A call of a lambda (a defined name's expanded body is a shared tree, an immediate lambda is the callee itself)
        // and the arguments it is given (PD-232).
        Expr::Call(call) => {
            expr_contains_subtotal(&call.callee) || call.args.iter().any(expr_contains_subtotal)
        }
        Expr::Shared(shared) => expr_contains_subtotal(&shared.expr),
        _ => false,
    }
}

/// Formula cells grouped by column, each column's rows sorted ascending.
#[derive(Debug, Clone, Default)]
pub(crate) struct FormulaIndex {
    columns: Vec<(u32, Vec<u32>)>,
}

impl FormulaIndex {
    pub(crate) fn build(sheet: &SheetEngine) -> Self {
        let mut pairs: Vec<(u32, u32)> = sheet
            .compiled_asts
            .iter()
            .filter(|(_, entry)| is_formula(Some(entry)))
            .map(|(coord, _)| (coord.col(), coord.row()))
            .collect();
        pairs.sort_unstable();
        let mut columns: Vec<(u32, Vec<u32>)> = Vec::new();
        for (col, row) in pairs {
            match columns.last_mut() {
                Some((c, rows)) if *c == col => rows.push(row),
                _ => columns.push((col, vec![row])),
            }
        }
        Self { columns }
    }

    /// Adds or removes one formula cell, keeping each column's rows sorted.
    pub(crate) fn set_formula(&mut self, coord: PackedCellCoordinate, present: bool) {
        let (col, row) = (coord.col(), coord.row());
        match self.columns.binary_search_by_key(&col, |(c, _)| *c) {
            Ok(i) => {
                let rows = &mut self.columns[i].1;
                match (rows.binary_search(&row), present) {
                    (Err(pos), true) => rows.insert(pos, row),
                    (Ok(pos), false) => {
                        rows.remove(pos);
                        if rows.is_empty() {
                            self.columns.remove(i);
                        }
                    }
                    _ => {}
                }
            }
            Err(i) if present => self.columns.insert(i, (col, vec![row])),
            Err(_) => {}
        }
    }

    /// Formula cells inside the rectangle, appended to `out` in no particular order.
    pub(crate) fn collect(
        &self,
        min_r: u32,
        max_r: u32,
        min_c: u32,
        max_c: u32,
        out: &mut Vec<PackedCellCoordinate>,
    ) {
        let first = self.columns.partition_point(|(c, _)| *c < min_c);
        for (col, rows) in &self.columns[first..] {
            if *col > max_c {
                break;
            }
            let lo = rows.partition_point(|&r| r < min_r);
            let hi = rows.partition_point(|&r| r <= max_r);
            out.extend(
                rows[lo..hi]
                    .iter()
                    .map(|&r| PackedCellCoordinate((u64::from(r) << 32) | u64::from(*col))),
            );
        }
    }

    /// Count of formula cells inside the rectangle.
    #[inline]
    pub(crate) fn count_in_range(&self, min_r: u32, max_r: u32, min_c: u32, max_c: u32) -> usize {
        let first = self.columns.partition_point(|(c, _)| *c < min_c);
        let mut count = 0usize;
        for (col, rows) in &self.columns[first..] {
            if *col > max_c {
                break;
            }
            let lo = rows.partition_point(|&r| r < min_r);
            let hi = rows.partition_point(|&r| r <= max_r);
            count += hi - lo;
        }
        count
    }
}

/// Populated cells indexed by column (`FastMap<u32, Vec<u32>>` with sorted `row` indices per `col`)
/// plus a pre-sorted `Vec<PackedCellCoordinate>` of all populated coordinates.
#[derive(Debug, Clone, Default)]
pub(crate) struct CellIndex {
    pub(crate) by_col: FastMap<u32, Vec<u32>>,
    pub(crate) sorted_coords: Vec<PackedCellCoordinate>,
    /// Column -> (row / 64) -> 64-row bitmask (bit `row % 64` is 1 if populated).
    pub(crate) col_masks: FastMap<u32, std::collections::BTreeMap<u32, u64>>,
}

impl CellIndex {
    pub(crate) fn build(sheet: &SheetEngine) -> Self {
        let mut sorted_coords: Vec<PackedCellCoordinate> = sheet
            .cells
            .keys()
            .chain(sheet.spill.filled())
            .copied()
            .collect();
        sorted_coords.sort_unstable();
        let mut by_col: FastMap<u32, Vec<u32>> = FastMap::default();
        let mut col_masks: FastMap<u32, std::collections::BTreeMap<u32, u64>> = FastMap::default();
        for &coord in &sorted_coords {
            let (r, c) = (coord.row(), coord.col());
            by_col.entry(c).or_default().push(r);
            *col_masks.entry(c).or_default().entry(r / 64).or_default() |= 1u64 << (r % 64);
        }
        Self {
            by_col,
            sorted_coords,
            col_masks,
        }
    }

    /// Adds or removes one populated cell in both views of the index.
    pub(crate) fn set_populated(&mut self, coord: PackedCellCoordinate, present: bool) {
        let (col, row) = (coord.col(), coord.row());
        let block_idx = row / 64;
        let bit = 1u64 << (row % 64);

        if present {
            let mask_entry = self
                .col_masks
                .entry(col)
                .or_default()
                .entry(block_idx)
                .or_default();
            if (*mask_entry & bit) != 0 {
                return;
            }
            *mask_entry |= bit;
        } else if let Some(blocks) = self.col_masks.get_mut(&col) {
            if let Some(word) = blocks.get_mut(&block_idx) {
                if (*word & bit) == 0 {
                    return;
                }
                *word &= !bit;
                if *word == 0 {
                    blocks.remove(&block_idx);
                    if blocks.is_empty() {
                        self.col_masks.remove(&col);
                    }
                }
            } else {
                return;
            }
        } else {
            return;
        }

        match (self.sorted_coords.binary_search(&coord), present) {
            (Err(pos), true) => {
                self.sorted_coords.insert(pos, coord);
                let rows = self.by_col.entry(col).or_default();
                if let Err(at) = rows.binary_search(&row) {
                    rows.insert(at, row);
                }
            }
            (Ok(pos), false) => {
                self.sorted_coords.remove(pos);
                if let Some(rows) = self.by_col.get_mut(&col) {
                    if let Ok(at) = rows.binary_search(&row) {
                        rows.remove(at);
                    }
                    if rows.is_empty() {
                        self.by_col.remove(&col);
                    }
                }
            }
            _ => {}
        }
    }

    /// Populated coordinates inside `[min_r..=max_r, min_c..=max_c]` in row-major order.
    pub(crate) fn range_coords(
        &self,
        min_r: u32,
        max_r: u32,
        min_c: u32,
        max_c: u32,
    ) -> Vec<PackedCellCoordinate> {
        if min_r > max_r || min_c > max_c || self.sorted_coords.is_empty() {
            return Vec::new();
        }
        let min_key = PackedCellCoordinate((u64::from(min_r) << 32) | u64::from(min_c));
        let max_key = PackedCellCoordinate((u64::from(max_r) << 32) | u64::from(max_c));
        let lo_sorted = self.sorted_coords.partition_point(|&c| c < min_key);
        let hi_sorted = self.sorted_coords.partition_point(|&c| c <= max_key);
        if lo_sorted >= hi_sorted {
            return Vec::new();
        }
        let col_span = (max_c - min_c + 1) as usize;
        if hi_sorted - lo_sorted <= col_span.saturating_mul(16) {
            return self.sorted_coords[lo_sorted..hi_sorted]
                .iter()
                .copied()
                .filter(|c| (min_c..=max_c).contains(&c.col()))
                .collect();
        }
        let mut out = Vec::new();
        if col_span <= self.by_col.len().saturating_mul(4) {
            for col in min_c..=max_c {
                if let Some(rows) = self.by_col.get(&col) {
                    let lo = rows.partition_point(|&r| r < min_r);
                    let hi = rows.partition_point(|&r| r <= max_r);
                    out.extend(
                        rows[lo..hi]
                            .iter()
                            .map(|&r| PackedCellCoordinate((u64::from(r) << 32) | u64::from(col))),
                    );
                }
            }
        } else {
            for (&col, rows) in &self.by_col {
                if (min_c..=max_c).contains(&col) {
                    let lo = rows.partition_point(|&r| r < min_r);
                    let hi = rows.partition_point(|&r| r <= max_r);
                    out.extend(
                        rows[lo..hi]
                            .iter()
                            .map(|&r| PackedCellCoordinate((u64::from(r) << 32) | u64::from(col))),
                    );
                }
            }
        }
        if out.len() > 1 && min_c < max_c {
            out.sort_unstable();
        }
        out
    }

    /// Checks whether any populated cell or external spill cell exists inside `[min_r..=max_r, min_c..=max_c]`
    /// excluding `anchor`, with zero heap allocations.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn has_spill_collision(
        &self,
        min_r: u32,
        max_r: u32,
        min_c: u32,
        max_c: u32,
        anchor: PackedCellCoordinate,
        cells: &crate::memo::CellMap<String>,
        spill_owner: &crate::memo::CellMap<PackedCellCoordinate>,
    ) -> bool {
        if min_r > max_r || min_c > max_c || self.sorted_coords.is_empty() {
            return false;
        }
        let col_span = (max_c - min_c + 1) as usize;
        let blocked = |coord: PackedCellCoordinate| {
            coord != anchor
                && (cells.contains_key(&coord)
                    || spill_owner.get(&coord).is_some_and(|&o| o != anchor))
        };
        if col_span <= self.by_col.len().saturating_mul(4) {
            for col in min_c..=max_c {
                if let Some(rows) = self.by_col.get(&col) {
                    let lo = rows.partition_point(|&r| r < min_r);
                    let hi = rows.partition_point(|&r| r <= max_r);
                    for &r in &rows[lo..hi] {
                        let coord = PackedCellCoordinate((u64::from(r) << 32) | u64::from(col));
                        if blocked(coord) {
                            return true;
                        }
                    }
                }
            }
        } else {
            for (&col, rows) in &self.by_col {
                if (min_c..=max_c).contains(&col) {
                    let lo = rows.partition_point(|&r| r < min_r);
                    let hi = rows.partition_point(|&r| r <= max_r);
                    for &r in &rows[lo..hi] {
                        let coord = PackedCellCoordinate((u64::from(r) << 32) | u64::from(col));
                        if blocked(coord) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Sorted populated row indices within `[min_r..=max_r]` in column `col`.
    pub(crate) fn col_rows_slice(&self, col: u32, min_r: u32, max_r: u32) -> &[u32] {
        if min_r > max_r {
            return &[];
        }
        let Some(rows) = self.by_col.get(&col) else {
            return &[];
        };
        let lo = rows.partition_point(|&r| r < min_r);
        let hi = rows.partition_point(|&r| r <= max_r);
        &rows[lo..hi]
    }

    /// Sorted populated coordinates within `row` and columns `[min_c..=max_c]`.
    pub(crate) fn row_coords_slice(
        &self,
        row: u32,
        min_c: u32,
        max_c: u32,
    ) -> &[PackedCellCoordinate] {
        if min_c > max_c || self.sorted_coords.is_empty() {
            return &[];
        }
        let min_key = PackedCellCoordinate((u64::from(row) << 32) | u64::from(min_c));
        let max_key = PackedCellCoordinate((u64::from(row) << 32) | u64::from(max_c));
        let lo = self.sorted_coords.partition_point(|&c| c < min_key);
        let hi = self.sorted_coords.partition_point(|&c| c <= max_key);
        &self.sorted_coords[lo..hi]
    }

    /// Exact count of populated cells in column `col` inside `[min_r..=max_r]` using 64-row
    /// word bitmasks in $O(B)$ time ($B \approx \text{populated blocks}$) via `count_ones`.
    #[inline]
    pub(crate) fn count_col_range(&self, col: u32, min_r: u32, max_r: u32) -> usize {
        if min_r > max_r {
            return 0;
        }
        let Some(blocks) = self.col_masks.get(&col) else {
            return 0;
        };
        let start_blk = min_r / 64;
        let end_blk = max_r / 64;
        let mut count = 0usize;
        for (&blk, &word) in blocks.range(start_blk..=end_blk) {
            let mut active = word;
            if blk == start_blk {
                let mask = !((1u64 << (min_r % 64)) - 1);
                active &= mask;
            }
            if blk == end_blk {
                let end_mod = max_r % 64;
                let mask = if end_mod == 63 {
                    !0u64
                } else {
                    (1u64 << (end_mod + 1)) - 1
                };
                active &= mask;
            }
            count += active.count_ones() as usize;
        }
        count
    }

    /// Iterates populated rows in column `col` inside `[min_r..=max_r]` using bitmask
    /// `(word & mask).trailing_zeros()` scans without vector allocation.
    #[allow(dead_code)]
    #[inline]
    pub(crate) fn for_each_col_row(
        &self,
        col: u32,
        min_r: u32,
        max_r: u32,
        mut f: impl FnMut(u32),
    ) {
        if min_r > max_r {
            return;
        }
        let Some(blocks) = self.col_masks.get(&col) else {
            return;
        };
        let start_blk = min_r / 64;
        let end_blk = max_r / 64;
        for (&blk, &word) in blocks.range(start_blk..=end_blk) {
            let mut active = word;
            if blk == start_blk {
                let mask = !((1u64 << (min_r % 64)) - 1);
                active &= mask;
            }
            if blk == end_blk {
                let end_mod = max_r % 64;
                let mask = if end_mod == 63 {
                    !0u64
                } else {
                    (1u64 << (end_mod + 1)) - 1
                };
                active &= mask;
            }
            let base_row = blk * 64;
            while active != 0 {
                let tz = active.trailing_zeros();
                f(base_row + tz);
                active &= active - 1;
            }
        }
    }

    /// Exact count of populated coordinates inside `[min_r..=max_r, min_c..=max_c]` without
    /// allocating a coordinate vector ($O(\log N)$ for single rows or single columns).
    pub(crate) fn count_in_range(&self, min_r: u32, max_r: u32, min_c: u32, max_c: u32) -> usize {
        if min_r > max_r || min_c > max_c || self.sorted_coords.is_empty() {
            return 0;
        }
        if min_c == max_c {
            return self.count_col_range(min_c, min_r, max_r);
        }
        if min_r == max_r {
            return self.row_coords_slice(min_r, min_c, max_c).len();
        }
        let min_key = PackedCellCoordinate((u64::from(min_r) << 32) | u64::from(min_c));
        let max_key = PackedCellCoordinate((u64::from(max_r) << 32) | u64::from(max_c));
        let lo_sorted = self.sorted_coords.partition_point(|&c| c < min_key);
        let hi_sorted = self.sorted_coords.partition_point(|&c| c <= max_key);
        if lo_sorted >= hi_sorted {
            return 0;
        }
        let col_span = (max_c - min_c + 1) as usize;
        if hi_sorted - lo_sorted <= col_span.saturating_mul(16) {
            return self.sorted_coords[lo_sorted..hi_sorted]
                .iter()
                .filter(|c| (min_c..=max_c).contains(&c.col()))
                .count();
        }
        let mut count = 0usize;
        if col_span <= self.by_col.len().saturating_mul(4) {
            for col in min_c..=max_c {
                if let Some(rows) = self.by_col.get(&col) {
                    let lo = rows.partition_point(|&r| r < min_r);
                    let hi = rows.partition_point(|&r| r <= max_r);
                    count += hi - lo;
                }
            }
        } else {
            for (&col, rows) in &self.by_col {
                if (min_c..=max_c).contains(&col) {
                    let lo = rows.partition_point(|&r| r < min_r);
                    let hi = rows.partition_point(|&r| r <= max_r);
                    count += hi - lo;
                }
            }
        }
        count
    }
}

impl SheetEngine {
    pub(crate) fn formula_index(&self) -> &FormulaIndex {
        self.formula_index.get_or_init(|| FormulaIndex::build(self))
    }

    pub(crate) fn cell_index(&self) -> &CellIndex {
        self.cell_index.get_or_init(|| CellIndex::build(self))
    }
}

/// Post-order of the formula cells of the last full recalculation plus every cell coordinate in
/// evaluation-result order. Valid until the sheet is edited or renamed.
#[derive(Debug, Clone)]
pub(crate) struct RecalcPlan {
    name: String,
    cells_len: usize,
    pub(crate) coords: Vec<PackedCellCoordinate>,
    order: Vec<PackedCellCoordinate>,
    cycle_owners: Vec<PackedCellCoordinate>,
}

#[derive(Default)]
pub(crate) struct PlanRecorder {
    order: Vec<PackedCellCoordinate>,
    cycle_owners: Vec<PackedCellCoordinate>,
}

impl RecalcPlan {
    pub(crate) fn matches(&self, sheet: &SheetEngine) -> bool {
        self.cells_len == sheet.cells.len() && self.name == sheet.name
    }

    /// Runs the pre-pass for every populated cell (recording the order) on a fresh context.
    pub(crate) fn build(sheet: &SheetEngine, ctx: &mut EvalContext<'_>) -> Self {
        let coords = sheet.cell_index().sorted_coords.clone();
        let mut rec = PlanRecorder::default();
        sheet.pre_evaluate_dag(&coords, ctx, Some(&mut rec));
        Self {
            name: sheet.name.clone(),
            cells_len: sheet.cells.len(),
            coords,
            order: rec.order,
            cycle_owners: rec.cycle_owners,
        }
    }

    /// Re-applies the recorded order on a fresh context: same evaluation sequence as `build`,
    /// without walking the dependency graph again.
    pub(crate) fn replay(&self, sheet: &SheetEngine, ctx: &mut EvalContext<'_>) {
        let sid = sheet.resolved_sheet_id();
        for &owner in &self.cycle_owners {
            ctx.circular = true;
            ctx.memo
                .insert_at(sid, owner, EvaluatedCellValue::Error(FormulaError::Cycle));
        }
        for &coord in &self.order {
            if ctx.expired() {
                break;
            }
            if !ctx.memo.contains_at(sid, coord) {
                ctx.reset_steps();
                sheet.with_coord_value(coord, ctx, 0, |_| ());
            }
        }
    }
}

/// Sheets reached by the pass, addressed by a small slot number instead of a cloned name.
struct SheetTable<'x> {
    slots: Vec<&'x SheetEngine>,
    /// AST sheet token text -> resolved slot, so a name is looked up in the workbook once per
    /// spelling instead of once per reference.
    names: HashMap<&'x str, Option<u16>>,
}

impl<'x> SheetTable<'x> {
    fn get(&self, slot: u16) -> &'x SheetEngine {
        self.slots[usize::from(slot)]
    }

    fn resolve(
        &mut self,
        root: &'x SheetEngine,
        wb: Option<&'x WorkbookEngine>,
        name: &'x str,
    ) -> Option<u16> {
        if let Some(&slot) = self.names.get(name) {
            return slot;
        }
        let target = if WorkbookEngine::is_same_sheet_identity(name, &root.name) {
            Some(root)
        } else {
            wb.and_then(|w| w.sheet(name))
        };
        let slot = target.and_then(|sheet| {
            let pos = self
                .slots
                .iter()
                .position(|s| std::ptr::eq(*s, sheet))
                .unwrap_or_else(|| {
                    self.slots.push(sheet);
                    self.slots.len() - 1
                });
            u16::try_from(pos).ok()
        });
        self.names.insert(name, slot);
        slot
    }
}

struct DepWalk<'a> {
    memo: &'a MemoTable,
    state: &'a DfsState,
    stack: &'a mut Vec<Frame>,
    cycle: &'a mut bool,
}

impl DepWalk<'_> {
    #[inline]
    fn visit(&mut self, slot: u16, sheet_id: u16, dep: PackedCellCoordinate) {
        match self.state.get(sheet_id, dep) {
            1 => *self.cycle = true,
            0 if !self.memo.contains_at(sheet_id, dep) => {
                self.stack.push(PackedDfsFrame::new(slot, dep, false))
            }
            _ => {}
        }
    }

    fn range(
        &mut self,
        sheet: &SheetEngine,
        slot: u16,
        sheet_id: u16,
        range: CellRange,
        scratch: &mut Vec<PackedCellCoordinate>,
    ) {
        let min_r = range.start.row().min(range.end.row());
        let max_r = range.start.row().max(range.end.row());
        let min_c = range.start.col().min(range.end.col());
        let max_c = range.start.col().max(range.end.col());
        if range.cell_count() <= SMALL_RANGE_CELLS {
            for r in min_r..=max_r {
                for c in min_c..=max_c {
                    let Ok(dep) = PackedCellCoordinate::new(r, c) else {
                        continue;
                    };
                    if is_formula(sheet.compiled_asts.get(&dep)) {
                        self.visit(slot, sheet_id, dep);
                    }
                }
            }
            return;
        }
        scratch.clear();
        sheet
            .formula_index()
            .collect(min_r, max_r, min_c, max_c, scratch);
        scratch.sort_unstable();
        for &dep in scratch.iter() {
            self.visit(slot, sheet_id, dep);
        }
    }
}

/// The sheet whose formula is being scanned.
struct ScanSite<'x> {
    sheet: &'x SheetEngine,
    slot: u16,
    sid: u16,
}

/// Reusable buffers and sheet resolution for the dependency scan.
struct DepScanner<'x> {
    root: &'x SheetEngine,
    wb: Option<&'x WorkbookEngine>,
    sheets: SheetTable<'x>,
    expr_stack: Vec<&'x Expr>,
    /// Shared trees (see [`Expr::Shared`]) already scanned for the formula at hand.
    seen_shared: DetSet<*const crate::SharedExpr>,
    scratch: Vec<PackedCellCoordinate>,
}

impl<'x> DepScanner<'x> {
    /// Visits every cell / range dependency of `ast` (explicit stack, no recursion).
    fn scan_dependencies(&mut self, walk: &mut DepWalk<'_>, ast: &'x Expr, site: ScanSite<'x>) {
        let ScanSite { sheet, slot, sid } = site;
        self.expr_stack.clear();
        self.seen_shared.clear();
        self.expr_stack.push(ast);
        while let Some(ex) = self.expr_stack.pop() {
            match ex {
                Expr::Cell(dep) => {
                    if is_formula(sheet.compiled_asts.get(dep)) {
                        walk.visit(slot, sid, *dep);
                    }
                }
                Expr::SheetCell(sc) => {
                    let name = &sc.sheet;
                    let dep = &sc.coord;
                    if let Some(ds) = self.sheets.resolve(self.root, self.wb, name) {
                        let dep_sheet = self.sheets.get(ds);
                        if is_formula(dep_sheet.compiled_asts.get(dep)) {
                            walk.visit(ds, dep_sheet.resolved_sheet_id(), *dep);
                        }
                    }
                }
                Expr::SpillRef(sp) => match &sp.sheet {
                    None => {
                        let coord = &sp.coord;
                        if is_formula(sheet.compiled_asts.get(coord)) {
                            walk.visit(slot, sid, *coord);
                        }
                    }
                    Some(name) => {
                        let coord = &sp.coord;
                        if let Some(ds) = self.sheets.resolve(self.root, self.wb, name) {
                            let dep_sheet = self.sheets.get(ds);
                            if is_formula(dep_sheet.compiled_asts.get(coord)) {
                                walk.visit(ds, dep_sheet.resolved_sheet_id(), *coord);
                            }
                        }
                    }
                },
                Expr::Call(c) => {
                    for a in c.args.iter().rev() {
                        self.expr_stack.push(a);
                    }
                    self.expr_stack.push(&c.callee);
                }
                Expr::Range(range) => walk.range(sheet, slot, sid, *range, &mut self.scratch),
                Expr::SheetRange(sr) => {
                    let name = &sr.sheet;
                    let range = &sr.range;
                    if let Some(ds) = self.sheets.resolve(self.root, self.wb, name) {
                        let dep_sheet = self.sheets.get(ds);
                        let dep_sid = dep_sheet.resolved_sheet_id();
                        walk.range(dep_sheet, ds, dep_sid, *range, &mut self.scratch);
                    }
                }
                Expr::Shared(shared) => {
                    if self.seen_shared.insert(std::sync::Arc::as_ptr(shared)) {
                        self.expr_stack.push(&shared.expr);
                    }
                }
                Expr::UnaryPlus(inner) | Expr::UnaryMinus(inner) | Expr::PostfixPercent(inner) => {
                    self.expr_stack.push(inner);
                }
                Expr::Binary { lhs, rhs, .. } => {
                    self.expr_stack.push(rhs);
                    self.expr_stack.push(lhs);
                }
                Expr::FuncCall(f) => {
                    for a in f.args.iter().rev() {
                        self.expr_stack.push(a);
                    }
                }
                Expr::Number(_)
                | Expr::Text(_)
                | Expr::Boolean(_)
                | Expr::ErrorLiteral(_)
                | Expr::Name(_)
                | Expr::Array(_)
                | Expr::Var(_)
                | Expr::Missing => {}
            }
        }
    }
}

impl SheetEngine {
    pub(crate) fn pre_evaluate_dag_iterative(
        &self,
        roots: &[PackedCellCoordinate],
        ctx: &mut EvalContext<'_>,
    ) {
        self.pre_evaluate_dag(roots, ctx, None);
    }

    pub(crate) fn pre_evaluate_dag(
        &self,
        roots: &[PackedCellCoordinate],
        ctx: &mut EvalContext<'_>,
        mut rec: Option<&mut PlanRecorder>,
    ) {
        let self_sid = self.resolved_sheet_id();
        let mut scan = DepScanner {
            root: self,
            wb: ctx.workbook,
            sheets: SheetTable {
                slots: vec![self],
                names: HashMap::new(),
            },
            expr_stack: Vec::new(),
            seen_shared: DetSet::default(),
            scratch: Vec::new(),
        };
        let mut state = DfsState::new(self_sid);
        let mut stack: Vec<Frame> = Vec::with_capacity(roots.len().min(4096));
        for &coord in roots.iter().rev() {
            if is_formula(self.compiled_asts.get(&coord)) && !ctx.memo.contains_at(self_sid, coord)
            {
                stack.push(PackedDfsFrame::new(0, coord, false));
            }
        }
        while let Some(frame) = stack.pop() {
            let (slot, coord, expanded) = frame.unpack();
            if ctx.expired() {
                break;
            }
            let sheet = scan.sheets.get(slot);
            let sid = sheet.resolved_sheet_id();
            if expanded {
                state.set(sid, coord, 2);
                if let Some(r) = rec.as_mut() {
                    r.order.push(coord);
                }
                if !ctx.memo.contains_at(sid, coord) {
                    ctx.reset_steps();
                    sheet.with_coord_value(coord, ctx, 0, |_| ());
                }
                continue;
            }
            if ctx.memo.contains_at(sid, coord) || state.get(sid, coord) != 0 {
                continue;
            }
            state.set(sid, coord, 1);
            let Some(Ok(ast)) = sheet.compiled_asts.get(&coord) else {
                state.set(sid, coord, 2);
                continue;
            };
            if matches!(ast, Expr::Number(_) | Expr::Boolean(_)) {
                state.set(sid, coord, 2);
                continue;
            }
            stack.push(PackedDfsFrame::new(slot, coord, true));
            let mut cycle = false;
            let mut walk = DepWalk {
                memo: &ctx.memo,
                state: &state,
                stack: &mut stack,
                cycle: &mut cycle,
            };
            let site = ScanSite { sheet, slot, sid };
            scan.scan_dependencies(&mut walk, ast, site);
            if cycle {
                ctx.circular = true;
                ctx.memo
                    .insert_at(sid, coord, EvaluatedCellValue::Error(FormulaError::Cycle));
                if let Some(r) = rec.as_mut() {
                    r.cycle_owners.push(coord);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_coord_number_preserves_cell_and_formula_indexes() {
        let mut sheet = SheetEngine::new("S");
        sheet.set_cell("A1", "10").unwrap();
        sheet.set_cell("B1", "=A1*2").unwrap();
        sheet.set_cell("D1", "5").unwrap();
        let c_ptr = sheet.cell_index() as *const CellIndex;
        let f_ptr = sheet.formula_index() as *const FormulaIndex;
        let d1 = PackedCellCoordinate::parse_a1("D1").unwrap();

        sheet.set_coord_number(d1, 42.0);
        assert!(
            sheet.cell_index.get().is_some(),
            "cell_index dropped on overwrite"
        );
        assert!(
            sheet.formula_index.get().is_some(),
            "formula_index dropped on overwrite"
        );
        assert!(std::ptr::eq(sheet.cell_index.get().unwrap(), c_ptr));
        assert!(std::ptr::eq(sheet.formula_index.get().unwrap(), f_ptr));

        // Goal seek repeatedly updates D1 via set_coord_number without dropping indexes.
        sheet.set_cell("E1", "=D1*3").unwrap();
        let _ = sheet.cell_index();
        let _ = sheet.formula_index();
        let solved = sheet.goal_seek("E1", 60.0, "D1").unwrap();
        assert!((solved - 20.0).abs() < 1e-4);
        assert!(sheet.cell_index.get().is_some());
        assert!(sheet.formula_index.get().is_some());

        // New cell via set_coord_number patches CellIndex in place.
        let c5 = PackedCellCoordinate::parse_a1("C5").unwrap();
        sheet.set_coord_number(c5, 7.0);
        assert!(sheet.cell_index.get().is_some());
        let rebuilt_cells = CellIndex::build(&sheet);
        let mut got: Vec<_> = sheet.cell_index().sorted_coords.clone();
        got.sort_unstable();
        assert_eq!(got, rebuilt_cells.sorted_coords);

        // Replacing a formula cell with a number removes it from FormulaIndex in place.
        let b1 = PackedCellCoordinate::parse_a1("B1").unwrap();
        sheet.set_coord_number(b1, 9.0);
        assert!(sheet.formula_index.get().is_some());
        assert!(sheet.cell_index.get().is_some());
        let mut got_f = Vec::new();
        sheet.formula_index().collect(0, 100, 0, 100, &mut got_f);
        let mut want_f = Vec::new();
        FormulaIndex::build(&sheet).collect(0, 100, 0, 100, &mut want_f);
        got_f.sort_unstable();
        want_f.sort_unstable();
        assert_eq!(got_f, want_f);
        assert_eq!(sheet.evaluate_cell("B1"), EvaluatedCellValue::Number(9.0));
    }

    #[test]
    fn col_masks_and_word_scans() {
        let mut sheet = SheetEngine::new("MaskS");
        for r in [0, 5, 63, 64, 65, 127, 128, 500, 1000] {
            let coord = PackedCellCoordinate::new(r, 0).unwrap();
            sheet.set_coord_number(coord, r as f64);
        }
        let idx = sheet.cell_index();
        assert_eq!(idx.count_col_range(0, 0, 1000), 9);
        assert_eq!(idx.count_col_range(0, 1, 63), 2);
        assert_eq!(idx.count_col_range(0, 64, 127), 3);

        let mut scanned = Vec::new();
        idx.for_each_col_row(0, 0, 1000, |r| scanned.push(r));
        assert_eq!(scanned, vec![0, 5, 63, 64, 65, 127, 128, 500, 1000]);
    }
}
