//! Dependency pre-pass for the sheet engine.
//!
//! Formulas are evaluated bottom-up (post-order of an explicit-stack DFS) so that dependency
//! chains deeper than the recursion budget still evaluate and cycles are detected without
//! recursion. This module owns that pass, the cached recalculation plan (the post-order of the
//! last full recalculation, dropped on every edit), and the per-column formula index that lets
//! range dependencies skip empty and literal cells.

use crate::memo::{DfsState, FastMap};
use crate::{
    CellRange, EvalContext, EvaluatedCellValue, Expr, FormulaError, MemoTable,
    PackedCellCoordinate, SheetEngine, WorkbookEngine,
};
use std::collections::HashMap;

/// Ranges at most this large are walked cell by cell; larger ones use the column index.
const SMALL_RANGE_CELLS: usize = 256;

type Frame = (u16, PackedCellCoordinate, bool);

#[inline]
pub(crate) fn is_formula(entry: Option<&Result<Expr, FormulaError>>) -> bool {
    matches!(
        entry,
        Some(Ok(expr)) if !matches!(expr, Expr::Number(_) | Expr::Boolean(_))
    )
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
}

/// Populated cells indexed by column (`HashMap<u32, Vec<u32>>` with sorted `row` indices per `col`)
/// plus a pre-sorted `Vec<PackedCellCoordinate>` of all populated coordinates.
#[derive(Debug, Clone, Default)]
pub(crate) struct CellIndex {
    pub(crate) by_col: HashMap<u32, Vec<u32>>,
    pub(crate) sorted_coords: Vec<PackedCellCoordinate>,
}

impl CellIndex {
    pub(crate) fn build(sheet: &SheetEngine) -> Self {
        let mut sorted_coords: Vec<PackedCellCoordinate> = sheet.cells.keys().copied().collect();
        sorted_coords.sort_unstable();
        let mut by_col: HashMap<u32, Vec<u32>> = HashMap::new();
        for &coord in &sorted_coords {
            by_col.entry(coord.col()).or_default().push(coord.row());
        }
        Self {
            by_col,
            sorted_coords,
        }
    }

    /// Adds or removes one populated cell in both views of the index.
    pub(crate) fn set_populated(&mut self, coord: PackedCellCoordinate, present: bool) {
        let (col, row) = (coord.col(), coord.row());
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
            if !ctx.memo.contains_at(sid, coord) {
                ctx.steps = 0;
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
    names: FastMap<&'x str, Option<u16>>,
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
            0 if !self.memo.contains_at(sheet_id, dep) => self.stack.push((slot, dep, false)),
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
    scratch: Vec<PackedCellCoordinate>,
}

impl<'x> DepScanner<'x> {
    /// Visits every cell / range dependency of `ast` (explicit stack, no recursion).
    fn scan_dependencies(&mut self, walk: &mut DepWalk<'_>, ast: &'x Expr, site: ScanSite<'x>) {
        let ScanSite { sheet, slot, sid } = site;
        self.expr_stack.clear();
        self.expr_stack.push(ast);
        while let Some(ex) = self.expr_stack.pop() {
            match ex {
                Expr::Cell(dep) => {
                    if is_formula(sheet.compiled_asts.get(dep)) {
                        walk.visit(slot, sid, *dep);
                    }
                }
                Expr::SheetCell {
                    sheet: name,
                    coord: dep,
                } => {
                    if let Some(ds) = self.sheets.resolve(self.root, self.wb, name) {
                        let dep_sheet = self.sheets.get(ds);
                        if is_formula(dep_sheet.compiled_asts.get(dep)) {
                            walk.visit(ds, dep_sheet.resolved_sheet_id(), *dep);
                        }
                    }
                }
                Expr::Range(range) => walk.range(sheet, slot, sid, *range, &mut self.scratch),
                Expr::SheetRange { sheet: name, range } => {
                    if let Some(ds) = self.sheets.resolve(self.root, self.wb, name) {
                        let dep_sheet = self.sheets.get(ds);
                        let dep_sid = dep_sheet.resolved_sheet_id();
                        walk.range(dep_sheet, ds, dep_sid, *range, &mut self.scratch);
                    }
                }
                Expr::UnaryPlus(inner) | Expr::UnaryMinus(inner) | Expr::PostfixPercent(inner) => {
                    self.expr_stack.push(inner);
                }
                Expr::Binary { lhs, rhs, .. } => {
                    self.expr_stack.push(rhs);
                    self.expr_stack.push(lhs);
                }
                Expr::FuncCall { args, .. } => {
                    for a in args.iter().rev() {
                        self.expr_stack.push(a);
                    }
                }
                Expr::Number(_)
                | Expr::Text(_)
                | Expr::Boolean(_)
                | Expr::ErrorLiteral(_)
                | Expr::Name { .. }
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
                names: FastMap::default(),
            },
            expr_stack: Vec::new(),
            scratch: Vec::new(),
        };
        let mut state = DfsState::new(self_sid);
        let mut stack: Vec<Frame> = Vec::with_capacity(roots.len().min(4096));
        for &coord in roots.iter().rev() {
            if is_formula(self.compiled_asts.get(&coord)) && !ctx.memo.contains_at(self_sid, coord)
            {
                stack.push((0, coord, false));
            }
        }
        while let Some((slot, coord, expanded)) = stack.pop() {
            let sheet = scan.sheets.get(slot);
            let sid = sheet.resolved_sheet_id();
            if expanded {
                state.set(sid, coord, 2);
                if let Some(r) = rec.as_mut() {
                    r.order.push(coord);
                }
                if !ctx.memo.contains_at(sid, coord) {
                    ctx.steps = 0;
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
            stack.push((slot, coord, true));
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
