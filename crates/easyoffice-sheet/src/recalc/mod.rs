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

mod dynamic;
mod graph;
mod id_set;
mod interval;
pub(crate) use interval::IntervalIndex;
mod names;
mod rule_session;
mod spill_api;
mod structure;
pub use rule_session::RuleUpdates;

pub use spill_api::SpillArea;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_axis;
#[cfg(test)]
mod tests_bench;
#[cfg(test)]
mod tests_deadline;
#[cfg(test)]
mod tests_names;

use crate::dag::is_formula;
use crate::{
    EvalContext, EvaluatedCellValue, FormulaError, MemoTable, PackedCellCoordinate, WorkbookEngine,
};
use crate::{SpillInfo, MAX_EVAL_TIME};
use easyoffice_formats::EasyOfficeError;
use graph::{cell_key, collect_refs, CellKey, DepGraph, Ref};
use id_set::IdSet;
use std::time::Duration;

/// Bounds on what the dependency graph may hold. A sheet can register far more edges than it has
/// cells (every formula can read thousands of cells), so the graph is metered, not just the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// Reverse edges: (cell, formula) and (range, formula) pairs.
    pub max_edges: usize,
    /// Estimated heap bytes of the graph (see `DepGraph::heap_bytes_estimate`).
    pub max_graph_bytes: usize,
    /// [`Footprint::total_bytes`]: text, graph and syntax trees together.
    pub max_bytes: usize,
    /// The longest one [`RecalcSession::recalculate`] runs; the formulas it has not reached are
    /// not reported and stay dirty (see `deadline`).
    pub max_time: Duration,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_edges: 8_000_000,
            max_graph_bytes: 256 << 20,
            max_bytes: 512 << 20,
            max_time: MAX_EVAL_TIME,
        }
    }
}

/// What a session holds, for callers that meter it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Footprint {
    /// Populated cells over all sheets.
    pub cells: usize,
    /// Bytes of raw cell text.
    pub raw_bytes: usize,
    /// Reverse edges in the dependency graph.
    pub edges: usize,
    /// Estimated heap bytes of the dependency graph.
    pub graph_bytes: usize,
    /// Estimated heap bytes of the compiled formulas (their syntax trees).
    pub ast_bytes: usize,
    /// Estimated heap bytes of the conditional formatting and validation rules (matched cells and
    /// cached range values).
    pub rule_bytes: usize,
    /// Estimated heap bytes of the cells that arrays fill (PD-217).
    pub spill_bytes: usize,
}

impl Footprint {
    /// Text, graph, syntax trees and rules: the figure a memory cap compares against.
    pub fn total_bytes(&self) -> usize {
        self.raw_bytes
            .saturating_add(self.graph_bytes)
            .saturating_add(self.ast_bytes)
            .saturating_add(self.rule_bytes)
            .saturating_add(self.spill_bytes)
    }
}

/// Heap bytes of a compiled formula per byte of its text. Measured on a 100,000-formula sheet of
/// 40-term sums (27 MB of text, about 450 MB of syntax trees) and rounded up.
const AST_BYTES_PER_FORMULA_BYTE: usize = 24;

/// Why an edit was refused.
#[derive(Debug)]
pub enum RecalcError {
    /// The request itself is wrong (unknown sheet, ...).
    Invalid(EasyOfficeError),
    /// The dependency graph outgrew its [`Budget`]. The session is no longer usable: drop it and
    /// evaluate without one.
    OverBudget,
}

impl std::fmt::Display for RecalcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(err) => err.fmt(f),
            Self::OverBudget => f.write_str("의존 그래프가 허용 크기를 넘었습니다."),
        }
    }
}

impl std::error::Error for RecalcError {}

impl From<EasyOfficeError> for RecalcError {
    fn from(err: EasyOfficeError) -> Self {
        Self::Invalid(err)
    }
}

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
    /// The deadline ([`Budget::max_time`]) passed: the formulas the pass had not reached are not in
    /// `changed` (the caller keeps the value it shows) and stay dirty for the next call.
    pub timed_out: bool,
    /// Formulas the deadline left dirty (0 unless `timed_out`).
    pub pending: usize,
    /// The anchors of arrays whose area changed in this recalculation (PD-217). The cells they
    /// fill are in `changed` with their values.
    pub spills: Vec<SpillInfo>,
}

/// The value each cell had before the recalculation, once per cell.
#[derive(Default)]
struct Firsts {
    list: Vec<((u16, PackedCellCoordinate), Option<EvaluatedCellValue>)>,
    /// Built when a cell can come twice (a second pass, a cell an anchor filled).
    index: Option<std::collections::HashSet<(u16, PackedCellCoordinate)>>,
}

impl Firsts {
    fn index(&mut self) -> &mut std::collections::HashSet<(u16, PackedCellCoordinate)> {
        let list = &self.list;
        self.index
            .get_or_insert_with(|| list.iter().map(|(k, _)| *k).collect())
    }

    fn note(&mut self, key: (u16, PackedCellCoordinate), old: Option<EvaluatedCellValue>) {
        if self.index().insert(key) {
            self.list.push((key, old));
        }
    }

    /// The cells of one pass: unique inside the pass, so a pass after the first checks them.
    fn extend(&mut self, pass: Vec<((u16, PackedCellCoordinate), Option<EvaluatedCellValue>)>) {
        if self.list.is_empty() && self.index.is_none() {
            self.list = pass;
        } else {
            for (key, old) in pass {
                self.note(key, old);
            }
        }
    }

    /// In sheet, then row-major order.
    fn into_sorted(mut self) -> Vec<((u16, PackedCellCoordinate), Option<EvaluatedCellValue>)> {
        self.list
            .sort_unstable_by_key(|&((sheet, coord), _)| cell_key(sheet, coord));
        self.list
    }
}

/// One pass of [`RecalcSession::recalculate`].
struct Round {
    /// The formula cells the pass computed, with the value each had before.
    before: Vec<((u16, PackedCellCoordinate), Option<EvaluatedCellValue>)>,
    order_len: usize,
    unfinished: Vec<u32>,
    pending: Vec<crate::spill::Pending>,
    timed_out: bool,
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
    /// The last pass ran out of time: the next one only finishes the dirty formulas (it neither
    /// dirties the volatile ones nor recomputes a circular workbook whole, or it could never finish).
    resuming: bool,
    rng_state: u64,
    /// Total bytes of raw cell text held by the workbook.
    raw_bytes: usize,
    /// Bytes of raw text of the formula cells (the part that becomes syntax trees).
    formula_bytes: usize,
    budget: Budget,
    /// The graph outgrew `budget`; every further edit is refused.
    over_budget: bool,
    refs: Vec<Ref>,
    queue: Vec<u32>,
    /// Estimated heap bytes of the defined names and of what they add to the formulas (see `names`).
    name_bytes: usize,
    /// Rules per sheet index (see `rule_session`).
    rules: std::collections::HashMap<u16, crate::rules::RuleRuntime>,
    /// What staged edits already changed about areas (an anchor that went away): reported by the next
    /// recalculation.
    released: crate::spill::Published,
    /// A dynamic circular reference was met in the pass in progress.
    dynamic_circular_pass: bool,
    /// What formulas read through `OFFSET` and `INDIRECT` (see `dynamic`).
    dynamic: dynamic::DynamicReads,
}

impl RecalcSession {
    /// Builds the dependency graph of `workbook`; every formula starts dirty, so the first
    /// [`recalculate`](Self::recalculate) computes all of them.
    pub fn new(workbook: WorkbookEngine) -> Result<Self, RecalcError> {
        Self::with_budget(workbook, Budget::default())
    }

    /// [`new`](Self::new) with an explicit graph [`Budget`]; a workbook that already exceeds it is
    /// refused.
    pub fn with_budget(workbook: WorkbookEngine, budget: Budget) -> Result<Self, RecalcError> {
        if workbook.sheet_count() > usize::from(u16::MAX) {
            return Err(EasyOfficeError::InvalidArgument(
                "시트가 너무 많아 증분 재계산 세션을 만들 수 없습니다.".to_string(),
            )
            .into());
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
            resuming: false,
            rng_state: EvalContext::standalone_id(primary_id).rng_state,
            raw_bytes: 0,
            formula_bytes: 0,
            budget,
            over_budget: false,
            refs: Vec::new(),
            queue: Vec::new(),
            rules: std::collections::HashMap::new(),
            released: crate::spill::Published::default(),
            dynamic_circular_pass: false,
            dynamic: dynamic::DynamicReads::default(),
            // The workbook is put in place after the loop, so its names are counted from the argument.
            name_bytes: workbook.name_bytes(),
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
                    session.formula_bytes += sheet.cells.get(&coord).map_or(0, String::len);
                    if session.exceeds_budget() {
                        return Err(RecalcError::OverBudget);
                    }
                } else {
                    // `=5`, `=TRUE`, a parse error: no vertex, but the first pass reports them.
                    session.edited.push((idx as u16, coord));
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

    /// The sheets' names in the order the session was opened with.
    pub fn sheet_names(&self) -> impl Iterator<Item = &str> {
        self.wb.sheets().iter().map(|s| s.name.as_str())
    }

    /// Cells, text and graph the session holds (what a caller meters).
    pub fn footprint(&self) -> Footprint {
        Footprint {
            cells: self.wb.sheets().iter().map(|s| s.cells.len()).sum(),
            raw_bytes: self.raw_bytes,
            edges: self.graph.edge_count() + self.dynamic.edge_count(),
            graph_bytes: self
                .graph
                .heap_bytes_estimate()
                .saturating_add(self.dynamic.heap_bytes_estimate()),
            ast_bytes: self.ast_bytes(),
            rule_bytes: self
                .rules
                .values()
                .map(crate::rules::RuleRuntime::heap_bytes_estimate)
                .sum(),
            spill_bytes: self
                .wb
                .sheets()
                .iter()
                .map(|s| s.spill.filled_count())
                .sum::<usize>()
                .saturating_mul(crate::spill::SPILL_CELL_BYTES),
        }
    }

    /// Estimated heap bytes of the syntax trees, with what the defined names add.
    fn ast_bytes(&self) -> usize {
        self.formula_bytes
            .saturating_mul(AST_BYTES_PER_FORMULA_BYTE)
            .saturating_add(self.name_ast_bytes())
    }

    fn exceeds_budget(&self) -> bool {
        let graph_bytes = self.graph.heap_bytes_estimate();
        let ast_bytes = self.ast_bytes();
        self.graph.edge_count() > self.budget.max_edges
            || graph_bytes > self.budget.max_graph_bytes
            || self
                .raw_bytes
                .saturating_add(graph_bytes)
                .saturating_add(ast_bytes)
                > self.budget.max_bytes
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
    ) -> Result<(), RecalcError> {
        if self.over_budget {
            return Err(RecalcError::OverBudget);
        }
        let idx = self.sheet_index(sheet)?;
        self.stage(idx, coord, raw)
    }

    /// Stages a batch of edits. A large batch rebuilds the cell indexes once instead of patching
    /// them per edit.
    pub fn apply(&mut self, changes: &[CellChange]) -> Result<(), RecalcError> {
        if self.over_budget {
            return Err(RecalcError::OverBudget);
        }
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
            self.stage(idx, change.coord, &change.raw)?;
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

    /// Takes a formula cell out of the graph, with what it read through `OFFSET` and `INDIRECT`: its id may be
    /// reused, so nothing may stay behind under it. Every path that removes a vertex goes through here.
    fn release_vertex(graph: &mut DepGraph, dynamic: &mut dynamic::DynamicReads, vertex: u32) {
        graph.remove_vertex(vertex);
        dynamic.forget(vertex);
    }

    fn stage(
        &mut self,
        sheet: u16,
        coord: PackedCellCoordinate,
        raw: &str,
    ) -> Result<(), RecalcError> {
        let engine = &self.wb.sheets()[usize::from(sheet)];
        let old_raw = engine.cells.get(&coord).map_or("", String::as_str);
        if old_raw == raw {
            return Ok(());
        }
        self.raw_bytes = self.raw_bytes - old_raw.len() + raw.len();
        if let Some(vertex) = self.graph.vertex_at(sheet, coord) {
            if let Some(Ok(ast)) = engine.compiled_asts.get(&coord) {
                collect_refs(&self.wb, sheet, ast, &mut self.refs);
                self.graph.unregister(vertex, &self.refs);
            }
            self.formula_bytes -= old_raw.len();
            Self::release_vertex(&mut self.graph, &mut self.dynamic, vertex);
            self.dirty.remove(vertex);
            self.cyclic.remove(vertex);
            self.memo.remove_at(engine.resolved_sheet_id(), coord);
        }
        if self.wb.sheets()[usize::from(sheet)]
            .spill
            .area_entry(coord)
            .is_some()
        {
            // The formula that filled an area is replaced: the area goes with it.
            crate::spill::release_anchor(
                &mut self.wb.sheets_mut()[usize::from(sheet)],
                sheet,
                &mut self.memo,
                coord,
                &mut self.released,
            );
        }
        self.wb.sheets_mut()[usize::from(sheet)].set_cell_coord(coord, raw);
        self.name_bytes = self.wb.name_bytes();
        let engine = &self.wb.sheets()[usize::from(sheet)];
        if let Some(entry) = engine.compiled_asts.get(&coord) {
            if let (true, Ok(ast)) = (is_formula(Some(entry)), entry) {
                let vertex = self.graph.add_vertex(sheet, coord);
                let volatile = collect_refs(&self.wb, sheet, ast, &mut self.refs);
                self.graph.register(vertex, &self.refs);
                self.graph.set_volatile(vertex, volatile);
                self.dirty.insert(vertex);
                self.formula_bytes += raw.len();
            }
        }
        self.edited.push((sheet, coord));
        if self.exceeds_budget() {
            self.over_budget = true;
            return Err(RecalcError::OverBudget);
        }
        Ok(())
    }

    /// Recomputes every formula that depends on a staged edit (and every volatile formula) and
    /// reports the formula cells whose value changed.
    ///
    /// Arrays that formulas calculate fill cells nobody wrote (PD-217). A pass decides where they
    /// go; the sheets change after it, and what the change touched (a filled cell, an anchor)
    /// is recalculated by the next pass, until a pass touches nothing. The cells filled are
    /// reported in `changed` like formula cells.
    pub fn recalculate(&mut self) -> RecalcOutcome {
        let dirty = &mut self.dirty;
        if self.resuming {
            // Only what the last pass left dirty (and what edits since touched).
        } else if self.cyclic.is_empty() && !self.dynamic_circular {
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
        let edited_all = std::mem::take(&mut self.edited);
        let mut seeds = edited_all.clone();
        let mut firsts = Firsts::default();
        for (sheet, coord, old) in std::mem::take(&mut self.released.before) {
            firsts.note((sheet, coord), old);
        }
        seeds.append(&mut self.released.touched);
        let mut infos: Vec<SpillInfo> = std::mem::take(&mut self.released.infos);
        let mut freed: Vec<(u16, PackedCellCoordinate)> = std::mem::take(&mut self.released.freed);
        let mut unfinished: Vec<u32>;
        let mut computed = 0usize;
        let mut timed_out = false;
        let mut unsettled = false;
        let mut round = 0usize;
        let mut first_round = true;
        loop {
            self.mark_spill_dirty(&seeds, &freed, first_round.then_some(&edited_all[..]));
            self.dynamic
                .mark_readers(&seeds, &mut self.dirty, &|v| self.graph.is_live(v));
            self.graph
                .expand_dirty(&seeds, &mut self.dirty, &mut self.queue);
            let result = self.run_round();
            firsts.extend(result.before);
            computed += result.order_len - result.unfinished.len();
            unfinished = result.unfinished;
            timed_out |= result.timed_out;
            let published = crate::spill::publish(&mut self.wb, &mut self.memo, result.pending);
            for (sheet, coord, old) in published.before {
                firsts.note((sheet, coord), old);
            }
            infos.extend(published.infos);
            seeds = published.touched;
            freed = published.freed;
            first_round = false;
            round += 1;
            if !unfinished.is_empty() || seeds.is_empty() {
                break;
            }
            if round >= crate::spill::MAX_SPILL_ROUNDS {
                unsettled = true;
                break;
            }
        }

        if unsettled {
            // Arrays that keep moving what each other reads are a circular reference: they show the
            // circular-reference error and give their cells up; what reads them is calculated once more.
            let rings: Vec<(u16, PackedCellCoordinate)> = seeds
                .iter()
                .copied()
                .filter(|&(sheet, coord)| {
                    self.wb.sheets()[usize::from(sheet)]
                        .spill
                        .area_entry(coord)
                        .is_some()
                })
                .collect();
            let mut out = crate::spill::Published::default();
            let mut poisoned = Vec::new();
            for &(sheet, anchor) in &rings {
                let sid = self.wb.sheets()[usize::from(sheet)].resolved_sheet_id();
                crate::spill::release_anchor(
                    &mut self.wb.sheets_mut()[usize::from(sheet)],
                    sheet,
                    &mut self.memo,
                    anchor,
                    &mut out,
                );
                self.memo
                    .insert_at(sid, anchor, EvaluatedCellValue::Error(FormulaError::Cycle));
                if let Some(v) = self.graph.vertex_at(sheet, anchor) {
                    self.cyclic.insert(v);
                    poisoned.push(v);
                }
            }
            for (sheet, coord, old) in out.before {
                firsts.note((sheet, coord), old);
            }
            infos.extend(out.infos);
            let mut readers = out.touched;
            readers.extend(rings.iter().copied());
            self.dynamic
                .mark_readers(&readers, &mut self.dirty, &|v| self.graph.is_live(v));
            self.graph
                .expand_dirty(&readers, &mut self.dirty, &mut self.queue);
            for &v in &poisoned {
                self.dirty.remove(v);
            }
            let result = self.run_round();
            firsts.extend(result.before);
            computed += result.order_len - result.unfinished.len();
            unfinished = result.unfinished;
            let published = crate::spill::publish(&mut self.wb, &mut self.memo, result.pending);
            for (sheet, coord, old) in published.before {
                firsts.note((sheet, coord), old);
            }
            infos.extend(published.infos);
        }

        let mut changed = Vec::new();
        let sheets = self.wb.sheets();
        // Formulas the deadline cut off stay dirty and keep the value the caller last saw (none for
        // a formula that is new), so they are neither reported nor shown as errors.
        let unfinished_sites: std::collections::HashSet<(u16, PackedCellCoordinate)> =
            unfinished.iter().map(|&v| self.graph.site(v)).collect();
        for ((sheet, coord), old) in firsts.into_sorted() {
            if unfinished_sites.contains(&(sheet, coord)) {
                continue;
            }
            let sid = sheets[usize::from(sheet)].resolved_sheet_id();
            let held = self.memo.get_at(sid, coord);
            if held.is_none() && old.is_none() && self.graph.vertex_at(sheet, coord).is_none() {
                continue;
            }
            let value = held.cloned().unwrap_or(EvaluatedCellValue::Empty);
            if matches!(value, EvaluatedCellValue::Error(FormulaError::Cycle)) {
                if let Some(v) = self.graph.vertex_at(sheet, coord) {
                    self.cyclic.insert(v);
                }
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
        let mut edited_all = edited_all;
        edited_all.sort_unstable();
        edited_all.dedup();
        // These cells hold constants, so they are read with a fresh clock.
        let mut ctx = EvalContext::with_workbook(
            &self.wb,
            sheets.first().map_or(1, |s| s.resolved_sheet_id()),
        )
        .without_deadline();
        ctx.memo = std::mem::replace(&mut self.memo, MemoTable::new(1));
        ctx.rng_state = self.rng_state;
        for &(sheet, coord) in &edited_all {
            let engine = &sheets[usize::from(sheet)];
            let is_formula_text = engine
                .cells
                .get(&coord)
                .is_some_and(|raw| crate::parser::strip_formula_prefix(raw.trim()).is_some());
            if is_formula_text && self.graph.vertex_at(sheet, coord).is_none() {
                ctx.reset_steps();
                changed.push(ChangedCell {
                    sheet: usize::from(sheet),
                    coord,
                    value: engine.eval_coord_ctx(coord, &mut ctx, 0),
                });
            }
        }
        self.dynamic_circular = self.dynamic_circular_pass || ctx.circular;
        self.rng_state = ctx.rng_state;
        self.memo = ctx.memo;
        self.dirty.clear();
        for &v in &unfinished {
            self.dirty.insert(v);
        }
        self.resuming = !unfinished.is_empty();
        let mut seen_anchor = std::collections::HashSet::new();
        let mut spills: Vec<SpillInfo> = Vec::new();
        for info in infos.into_iter().rev() {
            if seen_anchor.insert((info.sheet, info.anchor)) {
                spills.push(info);
            }
        }
        spills.reverse();
        RecalcOutcome {
            changed,
            circular: !self.cyclic.is_empty() || self.dynamic_circular || unsettled,
            recomputed: computed,
            pending: unfinished.len(),
            timed_out: timed_out && !unfinished.is_empty(),
            spills,
        }
    }

    /// Evaluates the dirty formulas once: the formulas in canonical order, with the pre-pass of the
    /// full path. What a pass decided about arrays is returned, not yet applied.
    fn run_round(&mut self) -> Round {
        // Canonical order: sheet, then row-major, the order the full path hands its targets in.
        let mut order: Vec<(CellKey, u32)> = Vec::with_capacity(self.dirty.len());
        self.dirty.for_each(|v| {
            let (sheet, coord) = self.graph.site(v);
            order.push((cell_key(sheet, coord), v));
        });
        order.sort_unstable();

        let sheets = self.wb.sheets();
        let mut before: Vec<((u16, PackedCellCoordinate), Option<EvaluatedCellValue>)> =
            Vec::with_capacity(order.len());
        let olds: Vec<Option<EvaluatedCellValue>> = order
            .iter()
            .map(|&(_, v)| {
                let (sheet, coord) = self.graph.site(v);
                self.cyclic.remove(v);
                self.memo
                    .remove_at(sheets[usize::from(sheet)].resolved_sheet_id(), coord)
            })
            .collect();
        for (&(_, v), old) in order.iter().zip(&olds) {
            before.push((self.graph.site(v), old.clone()));
        }

        let primary_id = sheets.first().map_or(1, |s| s.resolved_sheet_id());
        let mut ctx =
            EvalContext::with_workbook(&self.wb, primary_id).with_time_limit(self.budget.max_time);
        ctx.memo = std::mem::replace(&mut self.memo, MemoTable::new(primary_id));
        ctx.rng_state = self.rng_state;
        ctx.dyn_reads = Some(crate::dyn_reads::DynReads::default());
        ctx.spill_budget = Some(
            crate::spill::MAX_SPILL_CELLS
                .saturating_sub(sheets.iter().map(|s| s.spill.filled_count()).sum::<usize>()),
        );
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

        let mut unfinished = Vec::new();
        for (&(_, v), old) in order.iter().zip(olds) {
            let (sheet, coord) = self.graph.site(v);
            let sid = sheets[usize::from(sheet)].resolved_sheet_id();
            if ctx.memo.get_at(sid, coord).is_some() {
                continue;
            }
            if ctx.expired() {
                unfinished.push(v);
                if let Some(old) = old {
                    if matches!(old, EvaluatedCellValue::Error(FormulaError::Cycle)) {
                        self.cyclic.insert(v);
                    }
                    ctx.memo.insert_at(sid, coord, old);
                }
            }
        }
        // A placement decided for an anchor the deadline cut off rests on aborted cells; the anchors the
        // pass finished keep theirs (they are not calculated again).
        let mut pending = std::mem::take(&mut ctx.pending);
        if ctx.expired() {
            let cut: std::collections::HashSet<(u16, PackedCellCoordinate)> =
                unfinished.iter().map(|&v| self.graph.site(v)).collect();
            pending.retain(|p| {
                !sheets
                    .iter()
                    .position(|s| s.resolved_sheet_id() == p.sheet_id)
                    .is_some_and(|idx| cut.contains(&(idx as u16, p.anchor)))
            });
        }
        self.dynamic_circular_pass = ctx.circular;
        self.rng_state = ctx.rng_state;
        // What the formulas that finished read through OFFSET and INDIRECT: the next round recalculates them
        // when the cells a round fills are among it.
        let unreached: std::collections::HashSet<u32> = unfinished.iter().copied().collect();
        self.dynamic.absorb(
            ctx.dyn_reads.take().unwrap_or_default(),
            order
                .iter()
                .filter(|(_, v)| !unreached.contains(v))
                .map(|&(_, v)| {
                    let (sheet, coord) = self.graph.site(v);
                    (v, sheet, coord)
                }),
            &|sheet| sheets[usize::from(sheet)].resolved_sheet_id(),
            &|id| {
                sheets
                    .iter()
                    .position(|s| s.resolved_sheet_id() == id)
                    .map(|i| i as u16)
            },
        );
        let timed_out = ctx.expired();
        self.memo = ctx.memo;
        self.dirty.clear();
        for &v in &unfinished {
            self.dirty.insert(v);
        }
        Round {
            before,
            order_len: order.len(),
            unfinished,
            pending,
            timed_out,
        }
    }

    /// Dirties the anchors a change of the cells `seeds` and `edited` concerns: the anchor whose area a
    /// cell was written into, and an anchor that could not be placed when a cell its area needs went.
    fn mark_spill_dirty(
        &mut self,
        seeds: &[(u16, PackedCellCoordinate)],
        freed: &[(u16, PackedCellCoordinate)],
        edited: Option<&[(u16, PackedCellCoordinate)]>,
    ) {
        let sheets = self.wb.sheets();
        if sheets.iter().all(|s| s.spill.is_empty()) {
            return;
        }
        let touched = edited
            .unwrap_or(&[])
            .iter()
            .chain(freed.iter())
            .chain(seeds.iter());
        for &(sheet, coord) in edited.unwrap_or(&[]) {
            if let Some(anchor) = sheets[usize::from(sheet)].spill.owner_of(coord) {
                if let Some(v) = self.graph.vertex_at(sheet, anchor) {
                    self.dirty.insert(v);
                }
            }
        }
        let cells: Vec<(u16, PackedCellCoordinate)> = touched.copied().collect();
        for (idx, sheet) in sheets.iter().enumerate() {
            let blocked: Vec<_> = sheet.spill.blocked().collect();
            if blocked.is_empty() {
                continue;
            }
            let mine: Vec<PackedCellCoordinate> = cells
                .iter()
                .filter(|(s, _)| usize::from(*s) == idx)
                .map(|&(_, c)| c)
                .collect();
            let everything = blocked.len().saturating_mul(mine.len()) > 4_000_000;
            for (anchor, area) in blocked {
                let hit = everything
                    || mine.iter().any(|c| {
                        c.row() >= anchor.row()
                            && c.col() >= anchor.col()
                            && c.row() - anchor.row() < area.rows
                            && c.col() - anchor.col() < area.cols
                    });
                if hit {
                    if let Some(v) = self.graph.vertex_at(idx as u16, anchor) {
                        self.dirty.insert(v);
                    }
                }
            }
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
