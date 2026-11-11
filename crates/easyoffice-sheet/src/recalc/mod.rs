//! Incremental recalculation of a workbook that stays open between edits.
//!
//! A [`RecalcSession`] owns the workbook, the memoized value of every formula cell, and a reverse
//! dependency graph. An edit marks the formulas that transitively read the changed cell dirty,
//! drops only their memoized values, and evaluates only those (in dependency order, with the same
//! pre-pass the full recalculation uses), so the cost of an edit follows the size of what it
//! affects instead of the size of the workbook. The result of a recalculation is the list of
//! formula cells whose value changed.
//!
//! Invariants: the memo holds a value only for formula cells that are up to date; a formula cell
//! with no memoized value is dirty. Volatile formulas (`RAND`, `NOW`, `OFFSET`, `INDIRECT`, ...)
//! are dirty on every pass, and while a circular reference exists every formula is (see
//! [`RecalcSession::recalculate`]). The sheet set is fixed for the life of a session.

mod graph;
mod id_set;
mod interval;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_bench;

use crate::dag::is_formula;
use crate::{
    EvalContext, EvaluatedCellValue, FormulaError, MemoTable, PackedCellCoordinate, WorkbookEngine,
};
use easyoffice_formats::EasyOfficeError;
use graph::{cell_key, collect_refs, CellKey, DepGraph, Ref};
use id_set::IdSet;

/// A batch this large rebuilds a sheet's cell indexes once instead of patching them per cell.
const BULK_CHANGE_THRESHOLD: usize = 64;

/// One cell edit: the sheet, the cell, and its new raw text (`""` clears the cell).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellChange {
    pub sheet: String,
    pub coord: PackedCellCoordinate,
    pub raw: String,
}

/// A formula cell whose value is new or differs from the previous recalculation.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangedCell {
    /// Index of the sheet in workbook order (see [`RecalcSession::sheet_name`]).
    pub sheet: usize,
    pub coord: PackedCellCoordinate,
    pub value: EvaluatedCellValue,
}

/// Result of one [`RecalcSession::recalculate`].
#[derive(Debug, Clone, PartialEq)]
pub struct RecalcOutcome {
    pub changed: Vec<ChangedCell>,
    /// A circular reference exists somewhere in the workbook (same meaning as the full path).
    pub circular: bool,
    /// Formula cells that were evaluated (the dirty set).
    pub recomputed: usize,
}

pub struct RecalcSession {
    wb: WorkbookEngine,
    graph: DepGraph,
    memo: MemoTable,
    /// Formula vertices without an up-to-date memoized value.
    dirty: IdSet,
    /// Formula vertices whose value is the circular-reference error.
    cyclic: IdSet,
    /// Cells edited since the last recalculation (the seeds of dirty propagation).
    edited: Vec<(u16, PackedCellCoordinate)>,
    /// A dynamic (run-time) circular reference was met in the last pass.
    dynamic_circular: bool,
    rng_state: u64,
    /// Total bytes of raw cell text held by the workbook.
    raw_bytes: usize,
    refs: Vec<Ref>,
    queue: Vec<u32>,
}

impl RecalcSession {
    /// Builds the dependency graph of `workbook`; every formula starts dirty, so the first
    /// [`recalculate`](Self::recalculate) computes all of them.
    pub fn new(workbook: WorkbookEngine) -> Result<Self, EasyOfficeError> {
        if workbook.sheet_count() > usize::from(u16::MAX) {
            return Err(EasyOfficeError::InvalidArgument(
                "시트가 너무 많아 증분 재계산 세션을 만들 수 없습니다.".to_string(),
            ));
        }
        let primary_id = workbook
            .sheets()
            .first()
            .map_or(1, |sheet| sheet.resolved_sheet_id());
        let mut session = Self {
            graph: DepGraph::default(),
            memo: MemoTable::new(primary_id),
            dirty: IdSet::default(),
            cyclic: IdSet::default(),
            edited: Vec::new(),
            dynamic_circular: false,
            rng_state: EvalContext::standalone_id(primary_id).rng_state,
            raw_bytes: 0,
            refs: Vec::new(),
            queue: Vec::new(),
            wb: WorkbookEngine::new(),
        };
        for idx in 0..workbook.sheet_count() {
            let sheet = &workbook.sheets()[idx];
            session.raw_bytes += sheet.cells.values().map(String::len).sum::<usize>();
            for (&coord, entry) in &sheet.compiled_asts {
                if let (true, Ok(ast)) = (is_formula(Some(entry)), entry) {
                    let sheet_idx = idx as u16;
                    let vertex = session.graph.add_vertex(sheet_idx, coord);
                    let volatile = collect_refs(&workbook, sheet_idx, ast, &mut session.refs);
                    session.graph.register(vertex, &session.refs);
                    session.graph.set_volatile(vertex, volatile);
                    session.dirty.insert(vertex);
                }
            }
        }
        session.wb = workbook;
        Ok(session)
    }

    pub fn workbook(&self) -> &WorkbookEngine {
        &self.wb
    }

    pub fn sheet_name(&self, sheet: usize) -> Option<&str> {
        self.wb.sheets().get(sheet).map(|s| s.name.as_str())
    }

    /// Populated cells and bytes of raw cell text the session holds (what a caller meters).
    pub fn footprint(&self) -> (usize, usize) {
        let cells = self.wb.sheets().iter().map(|s| s.cells.len()).sum();
        (cells, self.raw_bytes)
    }

    /// Number of formula cells and of distinct range references tracked by the graph.
    pub fn graph_size(&self) -> (usize, usize) {
        (self.graph.vertex_count(), self.graph.range_count())
    }

    /// Stages one edit; it takes effect at the next [`recalculate`](Self::recalculate).
    pub fn set_cell(
        &mut self,
        sheet: &str,
        coord: PackedCellCoordinate,
        raw: &str,
    ) -> Result<(), EasyOfficeError> {
        let idx = self.sheet_index(sheet)?;
        self.stage(idx, coord, raw);
        Ok(())
    }

    /// Stages a batch of edits. A large batch rebuilds the cell indexes once instead of patching
    /// them per edit.
    pub fn apply(&mut self, changes: &[CellChange]) -> Result<(), EasyOfficeError> {
        let mut resolved = Vec::with_capacity(changes.len());
        for change in changes {
            resolved.push(self.sheet_index(&change.sheet)?);
        }
        if changes.len() > BULK_CHANGE_THRESHOLD {
            for &idx in &resolved {
                self.wb.sheets_mut()[usize::from(idx)].drop_indexes();
            }
        }
        for (change, idx) in changes.iter().zip(resolved) {
            self.stage(idx, change.coord, &change.raw);
        }
        Ok(())
    }

    fn sheet_index(&self, name: &str) -> Result<u16, EasyOfficeError> {
        self.wb
            .find_sheet_index(name)
            .map(|idx| idx as u16)
            .ok_or_else(|| {
                EasyOfficeError::InvalidArgument(format!("시트를 찾을 수 없습니다: {name}"))
            })
    }

    fn stage(&mut self, sheet: u16, coord: PackedCellCoordinate, raw: &str) {
        let engine = &self.wb.sheets()[usize::from(sheet)];
        let old_raw = engine.cells.get(&coord).map_or("", String::as_str);
        if old_raw == raw {
            return;
        }
        self.raw_bytes = self.raw_bytes - old_raw.len() + raw.len();
        if let Some(vertex) = self.graph.vertex_at(sheet, coord) {
            if let Some(Ok(ast)) = engine.compiled_asts.get(&coord) {
                collect_refs(&self.wb, sheet, ast, &mut self.refs);
                self.graph.unregister(vertex, &self.refs);
            }
            self.graph.remove_vertex(vertex);
            self.dirty.remove(vertex);
            self.cyclic.remove(vertex);
            self.memo.remove_at(engine.resolved_sheet_id(), coord);
        }
        self.wb.sheets_mut()[usize::from(sheet)].set_cell_coord(coord, raw);
        let engine = &self.wb.sheets()[usize::from(sheet)];
        if let Some(entry) = engine.compiled_asts.get(&coord) {
            if let (true, Ok(ast)) = (is_formula(Some(entry)), entry) {
                let vertex = self.graph.add_vertex(sheet, coord);
                let volatile = collect_refs(&self.wb, sheet, ast, &mut self.refs);
                self.graph.register(vertex, &self.refs);
                self.graph.set_volatile(vertex, volatile);
                self.dirty.insert(vertex);
            }
        }
        self.edited.push((sheet, coord));
    }

    /// Recomputes every formula that depends on a staged edit (and every volatile formula) and
    /// reports the formula cells whose value changed.
    pub fn recalculate(&mut self) -> RecalcOutcome {
        let dirty = &mut self.dirty;
        if self.cyclic.is_empty() && !self.dynamic_circular {
            self.graph.for_each_volatile(|v| {
                dirty.insert(v);
            });
        } else {
            // Which cell of a circular reference carries the error depends on the order the cells
            // are entered in, so a workbook that has one is recomputed whole, in the same order as
            // the full path, instead of trusting an owner chosen by an earlier partial pass.
            self.graph.for_each_vertex(|v| {
                dirty.insert(v);
            });
        }
        self.graph
            .expand_dirty(&self.edited, &mut self.dirty, &mut self.queue);

        // Canonical order: sheet, then row-major, the order the full path hands its targets in.
        let mut order: Vec<(CellKey, u32)> = Vec::with_capacity(self.dirty.len());
        self.dirty.for_each(|v| {
            let (sheet, coord) = self.graph.site(v);
            order.push((cell_key(sheet, coord), v));
        });
        order.sort_unstable();

        let sheets = self.wb.sheets();
        let before: Vec<Option<EvaluatedCellValue>> = order
            .iter()
            .map(|&(_, v)| {
                let (sheet, coord) = self.graph.site(v);
                self.cyclic.remove(v);
                self.memo
                    .remove_at(sheets[usize::from(sheet)].resolved_sheet_id(), coord)
            })
            .collect();

        let primary_id = sheets.first().map_or(1, |s| s.resolved_sheet_id());
        let mut ctx = EvalContext::with_workbook(&self.wb, primary_id);
        ctx.memo = std::mem::replace(&mut self.memo, MemoTable::new(primary_id));
        ctx.rng_state = self.rng_state;
        let mut roots: Vec<PackedCellCoordinate> = Vec::new();
        let mut start = 0;
        while start < order.len() {
            let sheet = self.graph.site(order[start].1).0;
            roots.clear();
            let mut end = start;
            while end < order.len() && self.graph.site(order[end].1).0 == sheet {
                roots.push(self.graph.site(order[end].1).1);
                end += 1;
            }
            sheets[usize::from(sheet)].pre_evaluate_dag(&roots, &mut ctx, None);
            start = end;
        }

        let mut changed = Vec::new();
        for (&(_, v), old) in order.iter().zip(before) {
            let (sheet, coord) = self.graph.site(v);
            let sid = sheets[usize::from(sheet)].resolved_sheet_id();
            let value = ctx
                .memo
                .get_at(sid, coord)
                .cloned()
                .unwrap_or(EvaluatedCellValue::Empty);
            if matches!(value, EvaluatedCellValue::Error(FormulaError::Cycle)) {
                self.cyclic.insert(v);
            }
            if old.as_ref() != Some(&value) {
                changed.push(ChangedCell {
                    sheet: usize::from(sheet),
                    coord,
                    value,
                });
            }
        }
        // Edited cells that hold formula text but are not vertices (`=5`, `=TRUE`, a parse error)
        // have a constant value; report it so the caller never has to guess.
        self.edited.sort_unstable();
        self.edited.dedup();
        for &(sheet, coord) in &self.edited {
            let engine = &sheets[usize::from(sheet)];
            let is_formula_text = engine
                .cells
                .get(&coord)
                .is_some_and(|raw| crate::parser::strip_formula_prefix(raw.trim()).is_some());
            if is_formula_text && self.graph.vertex_at(sheet, coord).is_none() {
                ctx.steps = 0;
                changed.push(ChangedCell {
                    sheet: usize::from(sheet),
                    coord,
                    value: engine.eval_coord_ctx(coord, &mut ctx, 0),
                });
            }
        }
        self.dynamic_circular = ctx.circular;
        self.rng_state = ctx.rng_state;
        self.memo = ctx.memo;
        self.edited.clear();
        self.dirty.clear();
        RecalcOutcome {
            changed,
            circular: !self.cyclic.is_empty() || self.dynamic_circular,
            recomputed: order.len(),
        }
    }

    /// Current value of a cell: the memoized value of a formula cell (call after
    /// [`recalculate`](Self::recalculate)), or the stored literal.
    pub fn cell_value(&self, sheet: &str, coord: PackedCellCoordinate) -> EvaluatedCellValue {
        let Some(idx) = self.wb.find_sheet_index(sheet) else {
            return EvaluatedCellValue::Error(FormulaError::Ref);
        };
        let engine = &self.wb.sheets()[idx];
        if let Some(value) = self.memo.get_at(engine.resolved_sheet_id(), coord) {
            return value.clone();
        }
        let mut ctx = EvalContext::with_workbook(&self.wb, engine.resolved_sheet_id());
        engine.eval_coord_ctx(coord, &mut ctx, 0)
    }
}
