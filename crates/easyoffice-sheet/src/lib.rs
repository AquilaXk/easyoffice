use easyoffice_formats::EasyOfficeError;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

mod agg_stream;
pub mod coercion;
mod criteria;
mod dag;
pub mod functions;
pub mod functions_complex;
mod functions_date_fin;
mod functions_dist;
mod functions_eng;
mod functions_fin;
mod functions_lookup;
mod functions_lookup_text;
mod functions_math;
mod memo;
mod parser;
mod recalc;
mod rules;
mod special;
mod text_format;
pub mod types;
pub mod workbook;

pub use coercion::*;
pub use functions::*;
pub use functions_complex::{format_complex, parse_complex};
pub use memo::{CellMap, CoordHasher, FastMap, MemoTable};
pub use parser::{compile_formula_expr, BinaryOp, Expr};
pub use recalc::{
    Budget, CellChange, ChangedCell, Footprint, RecalcError, RecalcOutcome, RecalcSession,
    RuleUpdates,
};
pub use rules::{
    decode_visuals, encode_visuals, validate_input, Bar, CellVisual, Icon, PackedVisuals,
    RuleRuntime, Verdict,
};
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
    pub(crate) dense_numbers: Vec<f64>,
    /// Cached dependency order of the last full recalculation; dropped on every edit.
    pub(crate) plan: OnceLock<dag::RecalcPlan>,
    /// Cached per-column index of formula cells for range dependency walks; dropped on edit.
    pub(crate) formula_index: OnceLock<dag::FormulaIndex>,
    /// Cached per-column index of all populated cells for sparse range queries; dropped on edit.
    pub(crate) cell_index: OnceLock<dag::CellIndex>,
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
            dense_numbers: Vec::new(),
            plan: OnceLock::new(),
            formula_index: OnceLock::new(),
            cell_index: OnceLock::new(),
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

    fn compile_cell_entry(raw: &str) -> Option<Result<Expr, FormulaError>> {
        let trimmed = raw.trim();
        if let Some(formula_body) = crate::parser::strip_formula_prefix(trimmed) {
            let expr_src = formula_body.trim();
            if expr_src.is_empty() {
                None
            } else {
                Some(compile_formula_expr(expr_src))
            }
        } else if crate::coercion::is_valid_decimal_syntax(trimmed) {
            if let Some(num) = crate::coercion::parse_strict_number(trimmed) {
                Some(Ok(Expr::Number(num)))
            } else {
                Some(Err(FormulaError::Num))
            }
        } else if trimmed.eq_ignore_ascii_case("TRUE") {
            Some(Ok(Expr::Boolean(true)))
        } else if trimmed.eq_ignore_ascii_case("FALSE") {
            Some(Ok(Expr::Boolean(false)))
        } else {
            None
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

    fn store_cell(&mut self, coord: PackedCellCoordinate, val: String) {
        if val.is_empty() {
            self.cells.remove(&coord);
            self.compiled_asts.remove(&coord);
            self.subtotal_coords.remove(&coord);
            self.update_dense_number(coord, None);
        } else {
            let compiled = Self::compile_cell_entry(&val);
            self.update_dense_number(coord, compiled.as_ref());
            if let Some(comp) = compiled {
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
        self.update_dense_number(coord, Some(&comp));
        self.compiled_asts.insert(coord, comp);
        self.subtotal_coords.remove(&coord);
        self.cells.insert(coord, format!("{num}"));
    }

    pub(crate) fn rebuild_compiled_asts(&mut self) {
        self.invalidate_caches();
        if self.sheet_id == 0 {
            self.sheet_id = compute_sheet_id(&self.name);
        }
        self.compiled_asts.clear();
        self.subtotal_coords.clear();
        self.dense_numbers.clear();
        let coords_and_compiled: Vec<(PackedCellCoordinate, Option<Result<Expr, FormulaError>>)> =
            self.cells
                .iter()
                .map(|(&coord, raw)| (coord, Self::compile_cell_entry(raw)))
                .collect();
        for (coord, compiled) in coords_and_compiled {
            self.update_dense_number(coord, compiled.as_ref());
            if let Some(comp) = compiled {
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
        ctx.steps = 0;
        self.eval_coord_ctx(coord, &mut ctx, 0)
    }

    /// Evaluates all populated cells in the worksheet in a single shared DAG memoization pass
    /// using pre-compiled ASTs and the inline `VisitingTracker` bitset.
    ///
    /// The dependency order is computed once and cached until the next edit, so repeated
    /// recalculations of an unchanged sheet only evaluate formulas (no graph walk).
    pub fn recalculate_all(&self) -> HashMap<PackedCellCoordinate, EvaluatedCellValue> {
        let mut ctx = EvalContext::standalone_id(self.resolved_sheet_id());
        let mut results = HashMap::with_capacity(self.cells.len());
        let built;
        let plan = match self.plan.get().filter(|p| p.matches(self)) {
            Some(plan) => {
                plan.replay(self, &mut ctx);
                plan
            }
            None => {
                built = dag::RecalcPlan::build(self, &mut ctx);
                // Another thread may have stored a plan first; either one is valid.
                let _ = self.plan.set(built.clone());
                &built
            }
        };
        for &coord in &plan.coords {
            ctx.steps = 0;
            let val = self.eval_coord_ctx(coord, &mut ctx, 0);
            results.insert(coord, val);
        }
        results
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
        let mut result = self.eval_expr_ctx(expr, ctx, depth + 1);
        ctx.current_coord = prev_coord;
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
            match compile_formula_expr(expr_src) {
                Ok(expr) => self.eval_expr_ctx(&expr, ctx, depth),
                Err(e) => EvaluatedCellValue::Error(e),
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

    pub(crate) fn apply_binary_op(
        op: BinaryOp,
        l_val: EvaluatedCellValue,
        r_val: EvaluatedCellValue,
    ) -> EvaluatedCellValue {
        match op {
            BinaryOp::Concat => {
                let ls = l_val.to_display_string();
                let rs = r_val.to_display_string();
                if ls.len().saturating_add(rs.len()) > Self::MAX_CELL_TEXT_CHARS
                    && ls.chars().count().saturating_add(rs.chars().count())
                        > Self::MAX_CELL_TEXT_CHARS
                {
                    EvaluatedCellValue::Error(FormulaError::Value)
                } else {
                    EvaluatedCellValue::Text(format!("{ls}{rs}"))
                }
            }
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Pow => {
                let ln = match l_val {
                    EvaluatedCellValue::Empty => 0.0,
                    _ => match l_val.as_number() {
                        Some(v) => v,
                        None => return EvaluatedCellValue::Error(FormulaError::Value),
                    },
                };
                let rn = match r_val {
                    EvaluatedCellValue::Empty => 0.0,
                    _ => match r_val.as_number() {
                        Some(v) => v,
                        None => return EvaluatedCellValue::Error(FormulaError::Value),
                    },
                };
                let res = match op {
                    BinaryOp::Add => ln + rn,
                    BinaryOp::Sub => ln - rn,
                    BinaryOp::Mul => ln * rn,
                    BinaryOp::Div => {
                        if rn == 0.0 {
                            return EvaluatedCellValue::Error(FormulaError::DivByZero);
                        }
                        ln / rn
                    }
                    BinaryOp::Pow => ln.powf(rn),
                    _ => unreachable!(),
                };
                if res.is_finite() {
                    EvaluatedCellValue::Number(res)
                } else {
                    EvaluatedCellValue::Error(FormulaError::Num)
                }
            }
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge => {
                let Some(ordering) = Self::compare_excel_values(&l_val, &r_val) else {
                    return EvaluatedCellValue::Error(FormulaError::Value);
                };
                let flag = match op {
                    BinaryOp::Eq => ordering.is_eq(),
                    BinaryOp::Ne => ordering.is_ne(),
                    BinaryOp::Lt => ordering.is_lt(),
                    BinaryOp::Le => ordering.is_le(),
                    BinaryOp::Gt => ordering.is_gt(),
                    BinaryOp::Ge => ordering.is_ge(),
                    _ => unreachable!(),
                };
                EvaluatedCellValue::Boolean(flag)
            }
        }
    }

    pub(crate) fn compare_excel_values(
        l_val: &EvaluatedCellValue,
        r_val: &EvaluatedCellValue,
    ) -> Option<std::cmp::Ordering> {
        use EvaluatedCellValue as V;
        match (l_val, r_val) {
            (V::Error(_), _) | (_, V::Error(_)) => None,
            (V::Empty, V::Empty) => Some(std::cmp::Ordering::Equal),
            (V::Empty, V::Number(b)) => 0.0f64.partial_cmp(b),
            (V::Number(a), V::Empty) => a.partial_cmp(&0.0),
            (V::Empty, V::Text(b)) => Some("".cmp(b.as_str())),
            (V::Text(a), V::Empty) => Some(a.as_str().cmp("")),
            (V::Empty, V::Boolean(b)) => Some(false.cmp(b)),
            (V::Boolean(a), V::Empty) => Some(a.cmp(&false)),
            (V::Number(a), V::Number(b)) => a.partial_cmp(b),
            (V::Text(a), V::Text(b)) if a.is_ascii() && b.is_ascii() => Some(
                a.bytes()
                    .map(|c| c.to_ascii_lowercase())
                    .cmp(b.bytes().map(|c| c.to_ascii_lowercase())),
            ),
            (V::Text(a), V::Text(b)) => Some(
                a.chars()
                    .flat_map(char::to_lowercase)
                    .cmp(b.chars().flat_map(char::to_lowercase)),
            ),
            (V::Boolean(a), V::Boolean(b)) => Some(a.cmp(b)),
            (V::Number(_), _) | (V::Text(_), V::Boolean(_)) => Some(std::cmp::Ordering::Less),
            (_, V::Number(_)) | (V::Boolean(_), V::Text(_)) => Some(std::cmp::Ordering::Greater),
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
            name.eq_ignore_ascii_case("SUBTOTAL") || args.iter().any(expr_contains_subtotal)
        }
        Expr::UnaryPlus(inner) | Expr::UnaryMinus(inner) | Expr::PostfixPercent(inner) => {
            expr_contains_subtotal(inner)
        }
        Expr::Binary { lhs, rhs, .. } => expr_contains_subtotal(lhs) || expr_contains_subtotal(rhs),
        _ => false,
    }
}
