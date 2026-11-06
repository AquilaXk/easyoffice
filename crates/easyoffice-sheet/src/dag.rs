//! Dependency pre-pass for the sheet engine.
//!
//! Formulas are evaluated bottom-up (post-order of an explicit-stack DFS) so that dependency
//! chains deeper than the recursion budget still evaluate and cycles are detected without
//! recursion. This module owns that pass, the cached recalculation plan (the post-order of the
//! last full recalculation, dropped on every edit), and the per-column formula index that lets
//! range dependencies skip empty and literal cells.

use crate::memo::DfsState;
use crate::{
    CellRange, EvalContext, EvaluatedCellValue, Expr, FormulaError, MemoTable,
    PackedCellCoordinate, SheetEngine, WorkbookEngine,
};

/// Ranges at most this large are walked cell by cell; larger ones use the column index.
const SMALL_RANGE_CELLS: usize = 256;

type Frame = (u16, PackedCellCoordinate, bool);

#[inline]
fn is_formula(entry: Option<&Result<Expr, FormulaError>>) -> bool {
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
    fn build(sheet: &SheetEngine) -> Self {
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

    /// Formula cells inside the rectangle, appended to `out` in no particular order.
    fn collect(
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

impl SheetEngine {
    pub(crate) fn formula_index(&self) -> &FormulaIndex {
        self.formula_index.get_or_init(|| FormulaIndex::build(self))
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
        let mut coords: Vec<PackedCellCoordinate> = sheet.cells.keys().copied().collect();
        coords.sort_unstable();
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
    /// `(name pointer, name length)` of an AST sheet token -> resolved slot.
    names: Vec<(usize, usize, Option<u16>)>,
}

impl<'x> SheetTable<'x> {
    fn get(&self, slot: u16) -> &'x SheetEngine {
        self.slots[usize::from(slot)]
    }

    fn resolve(
        &mut self,
        root: &'x SheetEngine,
        wb: Option<&'x WorkbookEngine>,
        name: &str,
    ) -> Option<u16> {
        let key = (name.as_ptr() as usize, name.len());
        if let Some(&(_, _, slot)) = self.names.iter().find(|n| (n.0, n.1) == key) {
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
        self.names.push((key.0, key.1, slot));
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
        let wb = ctx.workbook;
        let self_sid = self.resolved_sheet_id();
        let mut sheets = SheetTable {
            slots: vec![self],
            names: Vec::new(),
        };
        let mut state = DfsState::new(self_sid);
        let mut stack: Vec<Frame> = Vec::with_capacity(roots.len().min(4096));
        let mut expr_stack: Vec<&Expr> = Vec::new();
        let mut scratch: Vec<PackedCellCoordinate> = Vec::new();
        for &coord in roots.iter().rev() {
            if is_formula(self.compiled_asts.get(&coord)) && !ctx.memo.contains_at(self_sid, coord)
            {
                stack.push((0, coord, false));
            }
        }
        while let Some((slot, coord, expanded)) = stack.pop() {
            let sheet = sheets.get(slot);
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
            expr_stack.clear();
            expr_stack.push(ast);
            while let Some(ex) = expr_stack.pop() {
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
                        if let Some(ds) = sheets.resolve(self, wb, name) {
                            let dep_sheet = sheets.get(ds);
                            if is_formula(dep_sheet.compiled_asts.get(dep)) {
                                walk.visit(ds, dep_sheet.resolved_sheet_id(), *dep);
                            }
                        }
                    }
                    Expr::Range(range) => walk.range(sheet, slot, sid, *range, &mut scratch),
                    Expr::SheetRange { sheet: name, range } => {
                        if let Some(ds) = sheets.resolve(self, wb, name) {
                            let dep_sheet = sheets.get(ds);
                            let dep_sid = dep_sheet.resolved_sheet_id();
                            walk.range(dep_sheet, ds, dep_sid, *range, &mut scratch);
                        }
                    }
                    Expr::UnaryPlus(inner)
                    | Expr::UnaryMinus(inner)
                    | Expr::PostfixPercent(inner) => expr_stack.push(inner),
                    Expr::Binary { lhs, rhs, .. } => {
                        expr_stack.push(rhs);
                        expr_stack.push(lhs);
                    }
                    Expr::FuncCall { args, .. } => {
                        for a in args.iter().rev() {
                            expr_stack.push(a);
                        }
                    }
                    Expr::Number(_)
                    | Expr::Text(_)
                    | Expr::Boolean(_)
                    | Expr::ErrorLiteral(_)
                    | Expr::Missing => {}
                }
            }
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
