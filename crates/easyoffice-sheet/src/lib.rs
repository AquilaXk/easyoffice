use easyoffice_formats::EasyOfficeError;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

mod agg_stream;
mod array;
mod array_class;
mod array_eval;
pub mod axis;
mod byte_width;
pub mod coercion;
mod compile;
mod criteria;
mod dag;
mod deadline;
pub mod filter;
#[cfg(test)]
mod function_coverage_tests;
pub mod functions;
mod functions_aggregate;
mod functions_array_gen;
mod functions_array_shape;
mod functions_bessel;
mod functions_bond;
pub mod functions_complex;
mod functions_convert;
mod functions_coupon;
mod functions_date_fin;
mod functions_db;
mod functions_depr;
mod functions_dist;
mod functions_dist_more;
mod functions_eng;
mod functions_fin;
mod functions_fin_args;
mod functions_imtrig;
mod functions_lambda;
mod functions_lookup;
mod functions_lookup_ref;
mod functions_lookup_text;
mod functions_math;
mod functions_math_trig;
mod functions_matrix;
mod functions_odd;
mod functions_regression;
mod functions_spill;
mod functions_stat_a;
mod functions_stat_tests;
mod functions_text_bytes;
mod functions_trend;
#[cfg(test)]
mod functions_unit_tests;
mod lexer;
mod memo;
pub mod names;
#[cfg(test)]
mod names_tests;
mod operators;
mod parser;
mod parser_array;
mod pivot;
mod recalc;
mod rules;
mod sheet_sort;
mod sort_keys;
mod sort_refs;
mod special;
mod spill;
mod text_format;
pub mod types;
pub mod workbook;

pub use axis::{
    follow_sheet_change_links, follow_sheet_change_names, quote_sheet_name, rewrite_sheet_names,
    shift_boxes, shift_filter, shift_levels, shift_link_targets, shift_names, shift_outline,
    shift_runs, shift_widths, Axis, AxisBudget, AxisEdit, AxisEditReport, EditKind, FilterShift,
    SheetChange,
};
pub use coercion::*;
pub use deadline::MAX_EVAL_TIME;
pub use functions::*;
pub use functions_complex::{format_complex, parse_complex};
pub use memo::{CellMap, CoordHasher, FastMap, MemoTable};
pub use names::{validate_names, NameIssue, NameTable};
pub use parser::{compile_formula_expr, BinaryOp, Expr, SharedExpr};
pub use pivot::{PivotError, PivotSpec};
pub use recalc::{
    Budget, CellChange, ChangedCell, Footprint, RecalcError, RecalcOutcome, RecalcSession,
    RuleUpdates, SpillArea,
};
pub use rules::{
    decode_visuals, encode_visuals, follow_sheet_change, shift_rules, validate_input, Bar,
    CellVisual, Icon, PackedVisuals, RuleRuntime, Verdict,
};
pub use sheet_sort::{sorted_formula, SortKey};
pub use sort_keys::{sort_order, SortColumn, MAX_SORT_KEYS};
pub use spill::{SheetLock, SpillInfo, MAX_SPILL_CELLS, MAX_SPILL_ROUNDS};
pub use types::*;
pub use workbook::*;

pub const MAX_CELL_TEXT_CHARS: usize = 32_767;
pub const MAX_EVAL_STEPS: usize = 2_097_152;

static EMPTY_VALUE: EvaluatedCellValue = EvaluatedCellValue::Empty;

/// Outcome of evaluating one cell: a temporary value, or "already stored in the memo table".
pub(crate) enum CellEval {
    Owned(EvaluatedCellValue),
    Memoized,
}

pub(crate) struct EvalContext<'a> {
    pub workbook: Option<&'a WorkbookEngine>,
    pub memo: MemoTable,
    pub current_coord: Option<PackedCellCoordinate>,
    pub(crate) steps: usize,
    pub(crate) rng_state: u64,
    pub(crate) circular: bool,
    pub(crate) clock: deadline::Clock,
    /// Arrays that stand for temporary ranges while a function of the scalar engine runs on them
    /// (see `array_eval`).
    pub(crate) temp: Vec<Arc<array::Grid>>,
    /// Calls of `LAMBDA` values in progress.
    pub(crate) lambda_depth: usize,
    /// Cells anchors may still fill; `None` when areas are not placed (a calculation outside a
    /// session): an array then shows its first value only.
    pub(crate) spill_budget: Option<usize>,
    /// What the pass decided for the anchors it calculated (see `spill`).
    pub(crate) pending: Vec<spill::Pending>,
    /// Cells the pass has given to anchors, by sheet and cell, to the anchor.
    pub(crate) claims: FastMap<u64, PackedCellCoordinate>,
    /// The arrays this pass placed, by sheet and anchor: a `A1#` read in the same pass takes the array itself.
    pub(crate) fresh: FastMap<u64, Arc<array::Grid>>,
}

impl<'a> EvalContext<'a> {
    #[inline]
    pub fn standalone_id(sheet_id: u16) -> Self {
        Self {
            workbook: None,
            memo: MemoTable::new(sheet_id),
            current_coord: None,
            steps: 0,
            rng_state: 0x5DEE_CE66_D1A4_F87B,
            circular: false,
            clock: deadline::Clock::default(),
            temp: Vec::new(),
            lambda_depth: 0,
            spill_budget: None,
            pending: Vec::new(),
            claims: FastMap::default(),
            fresh: FastMap::default(),
        }
    }

    #[inline]
    pub fn with_workbook(workbook: &'a WorkbookEngine, sheet_id: u16) -> Self {
        Self {
            workbook: Some(workbook),
            memo: MemoTable::new(sheet_id),
            current_coord: None,
            steps: 0,
            rng_state: 0x5DEE_CE66_D1A4_F87B,
            circular: false,
            clock: deadline::Clock::default(),
            temp: Vec::new(),
            lambda_depth: 0,
            spill_budget: None,
            pending: Vec::new(),
            claims: FastMap::default(),
            fresh: FastMap::default(),
        }
    }

    /// The value at `coord` of temporary array number `index`.
    pub(crate) fn temp_cell(
        &self,
        index: usize,
        coord: PackedCellCoordinate,
    ) -> EvaluatedCellValue {
        match self.temp.get(index) {
            Some(grid)
                if (coord.row() as usize) < grid.rows && (coord.col() as usize) < grid.cols =>
            {
                grid.get(coord.row() as usize, coord.col() as usize).clone()
            }
            _ => EvaluatedCellValue::Empty,
        }
    }

    #[inline]
    pub fn next_rand_f64(&mut self) -> f64 {
        self.rng_state = self
            .rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.rng_state >> 11) as f64) * (1.0 / ((1u64 << 53) as f64))
    }
}

/// Spreadsheet calculation engine with $O(V + E)$ cycle detection, pre-compiled ASTs, and DAG memoization.
#[derive(Debug, Clone, Default)]
pub struct SheetEngine {
    pub name: String,
    pub(crate) sheet_id: u16,
    pub(crate) has_workbook_assigned_id: bool,
    pub(crate) cells: CellMap<String>,
    pub(crate) compiled_asts: CellMap<Result<Expr, FormulaError>>,
    pub(crate) subtotal_coords: HashSet<PackedCellCoordinate>,
    /// Formulas the array evaluator calculates (see `array_class::needs_array`).
    pub(crate) array_formulas: HashSet<PackedCellCoordinate>,
    /// The areas anchors fill in this sheet (PD-217).
    pub(crate) spill: spill::SpillTable,
    /// The cells of a protected sheet that may be changed (PD-217).
    pub(crate) spill_lock: Option<Arc<spill::SheetLock>>,
    pub(crate) dense_numbers: Vec<f64>,
    /// Cached dependency order of the last full recalculation; dropped on every edit.
    pub(crate) plan: OnceLock<dag::RecalcPlan>,
    /// Cached per-column index of formula cells for range dependency walks; dropped on edit.
    pub(crate) formula_index: OnceLock<dag::FormulaIndex>,
    /// Cached per-column index of all populated cells for sparse range queries; dropped on edit.
    pub(crate) cell_index: OnceLock<dag::CellIndex>,
    /// The workbook's defined names, shared by its sheets (PD-214).
    pub(crate) names: Arc<NameTable>,
    /// The formulas that use a defined name, with the names (hashes) they use, so a change of a
    /// definition compiles only those again.
    pub(crate) name_uses: CellMap<compile::NameUse>,
    /// Nodes the formulas of the whole workbook hold because they use names (shared by its sheets, see
    /// `names::MAX_WORKBOOK_NAME_NODES`).
    pub(crate) name_budget: Arc<std::sync::atomic::AtomicUsize>,
}

impl SheetEngine {
    pub const MAX_RECURSION_DEPTH: usize = 1024;
    pub const MAX_CELL_TEXT_CHARS: usize = MAX_CELL_TEXT_CHARS;
    pub const MAX_EVAL_STEPS: usize = MAX_EVAL_STEPS;
    const DENSE_ROWS: usize = 16_384;
    const DENSE_COLS: usize = 16;

    pub fn new(name: impl Into<String>) -> Self {
        let raw = name.into();
        let name_str = if raw.trim().is_empty() {
            "Sheet1".to_string()
        } else {
            raw
        };
        let sheet_id = compute_sheet_id(&name_str);
        Self {
            name: name_str,
            sheet_id,
            has_workbook_assigned_id: false,
            cells: CellMap::default(),
            compiled_asts: CellMap::default(),
            subtotal_coords: HashSet::new(),
            array_formulas: HashSet::new(),
            spill: spill::SpillTable::default(),
            spill_lock: None,
            dense_numbers: Vec::new(),
            plan: OnceLock::new(),
            formula_index: OnceLock::new(),
            cell_index: OnceLock::new(),
            names: Arc::default(),
            name_uses: CellMap::default(),
            name_budget: Arc::default(),
        }
    }

    pub fn set_name(&mut self, name: impl Into<String>) {
        let raw = name.into();
        self.name = if raw.trim().is_empty() {
            "Sheet1".to_string()
        } else {
            raw
        };
        if !self.has_workbook_assigned_id {
            self.sheet_id = compute_sheet_id(&self.name);
        }
    }

    #[inline]
    pub fn raw_cells(&self) -> &CellMap<String> {
        &self.cells
    }

    /// Drops the cached recalculation plan (it covers the cell set, so any edit invalidates it).
    #[inline]
    pub(crate) fn invalidate_plan(&mut self) {
        if self.plan.get().is_some() {
            self.plan = OnceLock::new();
        }
    }

    /// Drops the cached recalculation plan, formula index, and cell index (called by every mutator
    /// that does not keep the indexes in step itself).
    #[inline]
    pub(crate) fn invalidate_caches(&mut self) {
        self.invalidate_plan();
        if self.formula_index.get().is_some() {
            self.formula_index = OnceLock::new();
        }
        if self.cell_index.get().is_some() {
            self.cell_index = OnceLock::new();
        }
    }

    #[inline]
    pub(crate) fn resolved_sheet_id(&self) -> u16 {
        if self.sheet_id != 0 {
            self.sheet_id
        } else {
            compute_sheet_id(&self.name)
        }
    }

    #[inline]
    fn update_dense_number(
        &mut self,
        coord: PackedCellCoordinate,
        compiled: Option<&Result<Expr, FormulaError>>,
    ) {
        let r = coord.row() as usize;
        let c = coord.col() as usize;
        if r < Self::DENSE_ROWS && c < Self::DENSE_COLS {
            let idx = (r << 4) | c;
            if let Some(Ok(Expr::Number(n))) = compiled {
                if self.dense_numbers.len() <= idx {
                    // Grow in 256-row steps so sparse sheets stay small.
                    let rows = (((r >> 8) + 1) << 8).min(Self::DENSE_ROWS);
                    self.dense_numbers.resize(rows * Self::DENSE_COLS, f64::NAN);
                }
                self.dense_numbers[idx] = *n;
            } else if idx < self.dense_numbers.len() {
                self.dense_numbers[idx] = f64::NAN;
            }
        }
    }

    pub fn set_cell(
        &mut self,
        a1: &str,
        raw_value: impl Into<String>,
    ) -> Result<(), EasyOfficeError> {
        let coord = PackedCellCoordinate::parse_a1(a1)?;
        self.set_cell_coord(coord, raw_value);
        Ok(())
    }

    pub fn set_cell_coord(&mut self, coord: PackedCellCoordinate, raw_value: impl Into<String>) {
        self.invalidate_plan();
        let was_populated = self.cells.contains_key(&coord);
        let was_formula = dag::is_formula(self.compiled_asts.get(&coord));
        self.store_cell(coord, raw_value.into());
        // Indexes that exist are patched instead of rebuilt: an edit that does not add or remove a
        // populated / formula cell leaves them untouched.
        let is_populated = self.cells.contains_key(&coord);
        let is_formula = dag::is_formula(self.compiled_asts.get(&coord));
        if was_populated != is_populated {
            if let Some(index) = self.cell_index.get_mut() {
                index.set_populated(coord, is_populated);
            }
        }
        if was_formula != is_formula {
            if let Some(index) = self.formula_index.get_mut() {
                index.set_formula(coord, is_formula);
            }
        }
    }

    /// Drops the cached indexes; the next evaluation rebuilds them in one sorted pass, which is
    /// cheaper than patching them cell by cell when many cells change at once.
    pub(crate) fn drop_indexes(&mut self) {
        self.formula_index = OnceLock::new();
        self.cell_index = OnceLock::new();
    }

    /// Records whether the formula compiled for `coord` is one the array evaluator calculates.
    fn mark_array_formula(
        &mut self,
        coord: PackedCellCoordinate,
        entry: Option<&Result<Expr, FormulaError>>,
    ) {
        if matches!(entry, Some(Ok(expr)) if array_class::needs_array(expr)) {
            self.array_formulas.insert(coord);
        } else {
            self.array_formulas.remove(&coord);
        }
    }

    fn store_cell(&mut self, coord: PackedCellCoordinate, val: String) {
        if val.is_empty() {
            self.cells.remove(&coord);
            self.compiled_asts.remove(&coord);
            self.subtotal_coords.remove(&coord);
            self.array_formulas.remove(&coord);
            self.set_name_uses(coord, None);
            self.update_dense_number(coord, None);
        } else {
            let compile::Compiled { entry, uses } = self.compile_cell_entry(&val);
            self.set_name_uses(coord, uses);
            self.update_dense_number(coord, entry.as_ref());
            self.mark_array_formula(coord, entry.as_ref());
            if let Some(comp) = entry {
                if matches!(comp, Ok(ref expr) if expr_contains_subtotal(expr)) {
                    self.subtotal_coords.insert(coord);
                } else {
                    self.subtotal_coords.remove(&coord);
                }
                self.compiled_asts.insert(coord, comp);
            } else {
                self.compiled_asts.remove(&coord);
                self.subtotal_coords.remove(&coord);
            }
            self.cells.insert(coord, val);
        }
    }

    #[inline]
    pub(crate) fn set_coord_number(&mut self, coord: PackedCellCoordinate, num: f64) {
        self.invalidate_caches();
        let comp = Ok(Expr::Number(num));
        self.set_name_uses(coord, None);
        self.update_dense_number(coord, Some(&comp));
        self.compiled_asts.insert(coord, comp);
        self.subtotal_coords.remove(&coord);
        self.array_formulas.remove(&coord);
        self.cells.insert(coord, format!("{num}"));
    }

    pub(crate) fn rebuild_compiled_asts(&mut self) {
        self.invalidate_caches();
        if self.sheet_id == 0 {
            self.sheet_id = compute_sheet_id(&self.name);
        }
        self.compiled_asts.clear();
        self.subtotal_coords.clear();
        self.array_formulas.clear();
        self.dense_numbers.clear();
        for old in self.name_uses.values() {
            self.name_budget
                .fetch_sub(old.nodes as usize, std::sync::atomic::Ordering::Relaxed);
        }
        self.name_uses.clear();
        let compiled: Vec<(PackedCellCoordinate, compile::Compiled)> = self
            .cells
            .iter()
            .map(|(&coord, raw)| (coord, self.compile_cell_entry(raw)))
            .collect();
        for (coord, compile::Compiled { entry, uses }) in compiled {
            self.set_name_uses(coord, uses);
            self.update_dense_number(coord, entry.as_ref());
            self.mark_array_formula(coord, entry.as_ref());
            if let Some(comp) = entry {
                if matches!(comp, Ok(ref expr) if expr_contains_subtotal(expr)) {
                    self.subtotal_coords.insert(coord);
                }
                self.compiled_asts.insert(coord, comp);
            }
        }
    }

    pub fn get_raw_cell(&self, a1: &str) -> Option<&str> {
        let coord = PackedCellCoordinate::parse_a1(a1).ok()?;
        self.cells.get(&coord).map(String::as_str)
    }

    pub fn evaluate_cell(&self, a1: &str) -> EvaluatedCellValue {
        let Ok(coord) = PackedCellCoordinate::parse_a1(a1) else {
            return EvaluatedCellValue::Error(FormulaError::Ref);
        };
        let mut ctx = EvalContext::standalone_id(self.resolved_sheet_id());
        self.pre_evaluate_dag_iterative(std::slice::from_ref(&coord), &mut ctx);
        ctx.reset_steps();
        self.eval_coord_ctx(coord, &mut ctx, 0)
    }

    /// Evaluates all populated cells in the worksheet in a single shared DAG memoization pass
    /// using pre-compiled ASTs and the inline `VisitingTracker` bitset.
    ///
    /// The dependency order is computed once and cached until the next edit, so repeated
    /// recalculations of an unchanged sheet only evaluate formulas (no graph walk).
    pub fn recalculate_all(&self) -> HashMap<PackedCellCoordinate, EvaluatedCellValue> {
        self.recalculate_all_within(None).0
    }

    pub fn evaluate_formula_str(&self, formula: &str) -> EvaluatedCellValue {
        let mut ctx = EvalContext::standalone_id(self.resolved_sheet_id());
        self.eval_raw_string_ctx(formula, &mut ctx, 0)
    }

    #[inline]
    pub(crate) fn eval_sheet_coord(
        &self,
        sheet_opt: Option<&str>,
        coord: PackedCellCoordinate,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> EvaluatedCellValue {
        match sheet_opt {
            None => self.eval_coord_ctx(coord, ctx, depth),
            Some(target_name) => {
                if let Some(index) = array::virtual_sheet_index(target_name) {
                    return ctx.temp_cell(index, coord);
                }
                if WorkbookEngine::is_same_sheet_identity(target_name, &self.name) {
                    return self.eval_coord_ctx(coord, ctx, depth);
                }
                let Some(wb) = ctx.workbook else {
                    return EvaluatedCellValue::Error(FormulaError::Ref);
                };
                let Some(target_sheet) = wb.sheet(target_name) else {
                    return EvaluatedCellValue::Error(FormulaError::Ref);
                };
                target_sheet.eval_coord_ctx(coord, ctx, depth)
            }
        }
    }

    #[inline]
    pub(crate) fn is_nested_subtotal_cell(
        &self,
        sheet_opt: Option<&str>,
        coord: PackedCellCoordinate,
        ctx: &EvalContext<'_>,
    ) -> bool {
        let target_sheet = match sheet_opt {
            None => self,
            Some(target_name)
                if WorkbookEngine::is_same_sheet_identity(target_name, &self.name) =>
            {
                self
            }
            Some(target_name) => {
                let Some(wb) = ctx.workbook else {
                    return false;
                };
                let Some(ts) = wb.sheet(target_name) else {
                    return false;
                };
                ts
            }
        };
        target_sheet.subtotal_coords.contains(&coord)
    }

    /// Evaluates a cell and hands the value to `f` by reference: memo hits are borrowed from the
    /// memo table instead of cloned, and a freshly computed value is stored without a copy.
    #[inline]
    pub(crate) fn with_coord_value<R>(
        &self,
        coord: PackedCellCoordinate,
        ctx: &mut EvalContext<'_>,
        depth: usize,
        f: impl FnOnce(&EvaluatedCellValue) -> R,
    ) -> R {
        match self.eval_coord_core(coord, ctx, depth) {
            CellEval::Owned(v) => f(&v),
            CellEval::Memoized => f(ctx
                .memo
                .get_at(self.resolved_sheet_id(), coord)
                .unwrap_or(&EMPTY_VALUE)),
        }
    }

    /// Cross-sheet counterpart of [`Self::with_coord_value`] (mirrors `eval_sheet_coord`).
    pub(crate) fn with_sheet_coord_value<R>(
        &self,
        sheet_opt: Option<&str>,
        coord: PackedCellCoordinate,
        ctx: &mut EvalContext<'_>,
        depth: usize,
        f: impl FnOnce(&EvaluatedCellValue) -> R,
    ) -> R {
        if let Some(index) = sheet_opt.and_then(array::virtual_sheet_index) {
            return f(&ctx.temp_cell(index, coord));
        }
        let target = match sheet_opt {
            None => Some(self),
            Some(name) if WorkbookEngine::is_same_sheet_identity(name, &self.name) => Some(self),
            Some(name) => ctx.workbook.and_then(|wb| wb.sheet(name)),
        };
        match target {
            Some(sheet) => sheet.with_coord_value(coord, ctx, depth, f),
            None => f(&EvaluatedCellValue::Error(FormulaError::Ref)),
        }
    }

    #[inline]
    pub(crate) fn eval_coord_ctx(
        &self,
        coord: PackedCellCoordinate,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> EvaluatedCellValue {
        self.with_coord_value(coord, ctx, depth, EvaluatedCellValue::clone)
    }

    fn eval_coord_core(
        &self,
        coord: PackedCellCoordinate,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> CellEval {
        ctx.steps = ctx.steps.saturating_add(1);
        if ctx.steps & deadline::POLL_STEPS_MASK == 0 {
            ctx.poll_deadline();
        }
        if ctx.steps > Self::MAX_EVAL_STEPS {
            return CellEval::Owned(EvaluatedCellValue::Error(FormulaError::Value));
        }
        if depth > Self::MAX_RECURSION_DEPTH {
            return CellEval::Owned(EvaluatedCellValue::Error(FormulaError::Ref));
        }
        let r = coord.row() as usize;
        let c = coord.col() as usize;
        if r < Self::DENSE_ROWS && c < Self::DENSE_COLS {
            if let Some(&n) = self.dense_numbers.get((r << 4) | c) {
                if !n.is_nan() {
                    return CellEval::Owned(EvaluatedCellValue::Number(n));
                }
            }
        }
        let sid = self.resolved_sheet_id();
        // Only formula cells are ever memoized, so a hit needs no further lookups.
        if ctx.memo.contains_at(sid, coord) {
            return CellEval::Memoized;
        }
        let Some(compiled) = self.compiled_asts.get(&coord) else {
            return CellEval::Owned(match self.cells.get(&coord) {
                None => EvaluatedCellValue::Empty,
                Some(raw) => {
                    if crate::parser::strip_formula_prefix(raw).is_some_and(|s| s.trim().is_empty())
                    {
                        EvaluatedCellValue::Empty
                    } else if raw.len() > Self::MAX_CELL_TEXT_CHARS
                        && raw.chars().count() > Self::MAX_CELL_TEXT_CHARS
                    {
                        EvaluatedCellValue::Error(FormulaError::Value)
                    } else {
                        EvaluatedCellValue::Text(raw.clone())
                    }
                }
            });
        };
        let expr = match compiled {
            Ok(Expr::Number(n)) => return CellEval::Owned(EvaluatedCellValue::Number(*n)),
            Ok(Expr::Boolean(b)) => return CellEval::Owned(EvaluatedCellValue::Boolean(*b)),
            Err(e) => return CellEval::Owned(EvaluatedCellValue::Error(*e)),
            Ok(expr) => expr,
        };

        if !ctx.memo.visit_insert(sid, coord) {
            ctx.circular = true;
            return CellEval::Owned(EvaluatedCellValue::Error(FormulaError::Cycle));
        }

        let prev_coord = ctx.current_coord;
        ctx.current_coord = Some(coord);
        let mut result = if !self.array_formulas.is_empty() && self.array_formulas.contains(&coord)
        {
            self.eval_array_formula(expr, Some(coord), ctx, depth + 1)
        } else {
            self.eval_expr_ctx(expr, ctx, depth + 1)
        };
        ctx.current_coord = prev_coord;
        if ctx.expired() {
            // The deadline passed inside this cell: its value may rest on an aborted cell.
            ctx.memo.visit_remove(sid, coord);
            return CellEval::Owned(EvaluatedCellValue::Error(FormulaError::Value));
        }
        result = crate::coercion::sanitize_evaluated_value(result);
        if let EvaluatedCellValue::Text(ref s) = result {
            if s.len() > Self::MAX_CELL_TEXT_CHARS && s.chars().count() > Self::MAX_CELL_TEXT_CHARS
            {
                result = EvaluatedCellValue::Error(FormulaError::Value);
            }
        }

        ctx.memo.visit_remove(sid, coord);
        ctx.memo.insert_at(sid, coord, result);
        CellEval::Memoized
    }

    pub(crate) fn eval_raw_string_ctx(
        &self,
        raw: &str,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> EvaluatedCellValue {
        if raw.is_empty() {
            return EvaluatedCellValue::Empty;
        }
        let trimmed = raw.trim();
        let mut res = if let Some(formula_body) = crate::parser::strip_formula_prefix(trimmed)
            .or_else(|| crate::parser::strip_open_formula_prefix(trimmed))
        {
            let expr_src = formula_body.trim();
            if expr_src.is_empty() {
                return EvaluatedCellValue::Empty;
            }
            match self.compile_formula(expr_src).entry {
                Some(Ok(expr)) if array_class::needs_array(&expr) => {
                    self.eval_array_formula(&expr, None, ctx, depth)
                }
                Some(Ok(expr)) => self.eval_expr_ctx(&expr, ctx, depth),
                Some(Err(e)) => EvaluatedCellValue::Error(e),
                None => EvaluatedCellValue::Empty,
            }
        } else if crate::coercion::is_valid_decimal_syntax(trimmed) {
            if let Some(num) = crate::coercion::parse_strict_number(trimmed) {
                EvaluatedCellValue::Number(num)
            } else {
                EvaluatedCellValue::Error(FormulaError::Num)
            }
        } else if trimmed.eq_ignore_ascii_case("TRUE") {
            EvaluatedCellValue::Boolean(true)
        } else if trimmed.eq_ignore_ascii_case("FALSE") {
            EvaluatedCellValue::Boolean(false)
        } else {
            EvaluatedCellValue::Text(raw.to_string())
        };
        res = crate::coercion::sanitize_evaluated_value(res);
        if let EvaluatedCellValue::Text(ref s) = res {
            if s.len() > Self::MAX_CELL_TEXT_CHARS && s.chars().count() > Self::MAX_CELL_TEXT_CHARS
            {
                res = EvaluatedCellValue::Error(FormulaError::Value);
            }
        }
        res
    }

    pub(crate) fn eval_expr_ctx(
        &self,
        expr: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> EvaluatedCellValue {
        ctx.steps = ctx.steps.saturating_add(1);
        if ctx.steps & deadline::POLL_STEPS_MASK == 0 {
            ctx.poll_deadline();
        }
        if ctx.steps > Self::MAX_EVAL_STEPS {
            return EvaluatedCellValue::Error(FormulaError::Value);
        }
        if depth > Self::MAX_RECURSION_DEPTH {
            return EvaluatedCellValue::Error(FormulaError::Ref);
        }
        let res = match expr {
            Expr::Missing => EvaluatedCellValue::Number(0.0),
            Expr::Number(n) => EvaluatedCellValue::Number(*n),
            Expr::Text(s) => {
                if s.len() > Self::MAX_CELL_TEXT_CHARS
                    && s.chars().count() > Self::MAX_CELL_TEXT_CHARS
                {
                    EvaluatedCellValue::Error(FormulaError::Value)
                } else {
                    EvaluatedCellValue::Text(s.clone())
                }
            }
            Expr::Boolean(b) => EvaluatedCellValue::Boolean(*b),
            Expr::ErrorLiteral(e) => EvaluatedCellValue::Error(*e),
            Expr::Name { .. } | Expr::Var(_) | Expr::Call { .. } => {
                EvaluatedCellValue::Error(FormulaError::Name)
            }
            Expr::Array(lit) => lit
                .data
                .first()
                .cloned()
                .unwrap_or(EvaluatedCellValue::Empty),
            Expr::SpillRef { sheet, coord } => {
                self.eval_sheet_coord(sheet.as_deref(), *coord, ctx, depth)
            }
            Expr::Shared(shared) => self.eval_expr_ctx(&shared.expr, ctx, depth),
            Expr::Cell(coord) => self.eval_coord_ctx(*coord, ctx, depth),
            Expr::SheetCell { sheet, coord } => {
                self.eval_sheet_coord(Some(sheet), *coord, ctx, depth)
            }
            Expr::Range(range) => {
                if range.start == range.end {
                    self.eval_coord_ctx(range.start, ctx, depth)
                } else {
                    EvaluatedCellValue::Error(FormulaError::Value)
                }
            }
            Expr::SheetRange { sheet, range } => {
                if !self.has_sheet_in_ctx(sheet, ctx) {
                    EvaluatedCellValue::Error(FormulaError::Ref)
                } else if range.start == range.end {
                    self.eval_sheet_coord(Some(sheet), range.start, ctx, depth)
                } else {
                    EvaluatedCellValue::Error(FormulaError::Value)
                }
            }
            Expr::UnaryPlus(inner) => self.eval_unary(inner, ctx, depth, |n| n),
            Expr::UnaryMinus(inner) => self.eval_unary(inner, ctx, depth, |n| -n),
            Expr::PostfixPercent(inner) => self.eval_unary(inner, ctx, depth, |n| n / 100.0),
            Expr::Binary { op, lhs, rhs } if !matches!(**lhs, Expr::Binary { .. }) => {
                // Single operator (no left-nested chain): evaluate both sides without a scratch Vec.
                let l_val = self.eval_expr_ctx(lhs, ctx, depth);
                if let EvaluatedCellValue::Error(e) = l_val {
                    return EvaluatedCellValue::Error(e);
                }
                let r_val = self.eval_expr_ctx(rhs, ctx, depth);
                if let EvaluatedCellValue::Error(e) = r_val {
                    return EvaluatedCellValue::Error(e);
                }
                Self::apply_binary_op(*op, l_val, r_val)
            }
            Expr::Binary { .. } => self.eval_binary_chain(expr, ctx, depth),
            Expr::FuncCall { name, args } => self.eval_func_call_expr(name, args, ctx, depth),
        };
        crate::coercion::sanitize_evaluated_value(res)
    }

    /// Unary operator / postfix percent: propagates errors, coerces to a number and maps it.
    fn eval_unary(
        &self,
        inner: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
        map: impl FnOnce(f64) -> f64,
    ) -> EvaluatedCellValue {
        let val = self.eval_expr_ctx(inner, ctx, depth);
        if let EvaluatedCellValue::Error(e) = val {
            return EvaluatedCellValue::Error(e);
        }
        match val.as_number() {
            Some(n) => EvaluatedCellValue::Number(map(n)),
            None => EvaluatedCellValue::Error(FormulaError::Value),
        }
    }

    /// Left-nested binary chains are folded iteratively (no recursion on long chains); the
    /// first 8 operators live on the stack so typical formulas never allocate.
    fn eval_binary_chain(
        &self,
        expr: &Expr,
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> EvaluatedCellValue {
        let mut inline: [Option<(BinaryOp, &Expr)>; 8] = [None; 8];
        let mut spill: Vec<(BinaryOp, &Expr)> = Vec::new();
        let mut count = 0usize;
        let mut cur = expr;
        while let Expr::Binary { op, lhs, rhs } = cur {
            if count < inline.len() {
                inline[count] = Some((*op, &**rhs));
            } else {
                spill.push((*op, &**rhs));
            }
            count += 1;
            cur = lhs;
        }
        let mut l_val = self.eval_expr_ctx(cur, ctx, depth);
        if let EvaluatedCellValue::Error(e) = l_val {
            return EvaluatedCellValue::Error(e);
        }
        for idx in (0..count).rev() {
            let (op, rhs) = if idx >= inline.len() {
                spill[idx - inline.len()]
            } else {
                inline[idx].expect("slot filled while walking the spine")
            };
            let r_val = self.eval_expr_ctx(rhs, ctx, depth);
            if let EvaluatedCellValue::Error(e) = r_val {
                return EvaluatedCellValue::Error(e);
            }
            l_val = Self::apply_binary_op(op, l_val, r_val);
            if let EvaluatedCellValue::Error(e) = l_val {
                return EvaluatedCellValue::Error(e);
            }
        }
        l_val
    }

    fn eval_func_call_expr(
        &self,
        name: &str,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> EvaluatedCellValue {
        match name {
            // Hot path: same semantics as the `IF` arm of the function table, without the
            // multi-stage name dispatch.
            "IF" if (2..=3).contains(&args.len()) => {
                let cond = self.eval_expr_ctx(&args[0], ctx, depth);
                if let EvaluatedCellValue::Error(e) = cond {
                    return EvaluatedCellValue::Error(e);
                }
                match cond.as_bool() {
                    None => EvaluatedCellValue::Error(FormulaError::Value),
                    Some(true) => self.eval_expr_ctx(&args[1], ctx, depth),
                    Some(false) if args.len() == 3 => self.eval_expr_ctx(&args[2], ctx, depth),
                    Some(false) => EvaluatedCellValue::Boolean(false),
                }
            }
            "SUM" | "AVERAGE" | "AVG" | "MIN" | "MAX" | "COUNT" | "COUNTA" | "COUNTBLANK"
            | "PRODUCT" => match self.eval_streaming_range_agg_ctx(name, args, ctx, depth) {
                Ok(v) => v,
                Err(e) => EvaluatedCellValue::Error(e),
            },
            _ => self.eval_func_ctx(name, args, ctx, depth),
        }
    }

    pub(crate) fn expand_args_ctx(
        &self,
        args: &[Expr],
        ctx: &mut EvalContext<'_>,
        depth: usize,
    ) -> Result<Vec<EvaluatedCellValue>, FormulaError> {
        let mut out = Vec::new();
        self.visit_args_streaming(args, ctx, depth, |v| {
            out.push(v);
            Ok(())
        })?;
        Ok(out)
    }

    pub(crate) fn values_match(a: &EvaluatedCellValue, b: &EvaluatedCellValue) -> bool {
        use EvaluatedCellValue as V;
        match (a, b) {
            (V::Number(na), V::Number(nb)) => na == nb,
            (V::Boolean(ba), V::Boolean(bb)) => ba == bb,
            (V::Text(sa), V::Text(sb)) if sa.is_ascii() && sb.is_ascii() => {
                sa.eq_ignore_ascii_case(sb)
            }
            (V::Text(sa), V::Text(sb)) => sa
                .chars()
                .flat_map(char::to_lowercase)
                .eq(sb.chars().flat_map(char::to_lowercase)),
            (V::Empty, V::Empty) => true,
            (V::Empty, V::Text(s)) | (V::Text(s), V::Empty) => s.is_empty(),
            _ => false,
        }
    }

    pub(crate) fn values_match_wildcard(
        cell: &EvaluatedCellValue,
        pattern: &EvaluatedCellValue,
    ) -> bool {
        if let (EvaluatedCellValue::Text(s), EvaluatedCellValue::Text(p)) = (cell, pattern) {
            wildcard_match(s, p)
        } else {
            Self::values_match(cell, pattern)
        }
    }
}

pub(crate) fn expr_contains_subtotal(expr: &Expr) -> bool {
    match expr {
        Expr::FuncCall { name, args } => {
            name.eq_ignore_ascii_case("SUBTOTAL")
                || name.eq_ignore_ascii_case("AGGREGATE")
                || args.iter().any(expr_contains_subtotal)
        }
        Expr::UnaryPlus(inner) | Expr::UnaryMinus(inner) | Expr::PostfixPercent(inner) => {
            expr_contains_subtotal(inner)
        }
        Expr::Binary { lhs, rhs, .. } => expr_contains_subtotal(lhs) || expr_contains_subtotal(rhs),
        Expr::Shared(shared) => expr_contains_subtotal(&shared.expr),
        _ => false,
    }
}
