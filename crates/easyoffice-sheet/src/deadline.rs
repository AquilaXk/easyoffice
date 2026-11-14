//! Wall-clock deadline of an evaluation pass (PD-215).
//!
//! One cell stops at [`MAX_EVAL_STEPS`](crate::MAX_EVAL_STEPS), but a workbook of many such cells has
//! no bound of its own. An [`EvalContext`] can carry a deadline. The evaluator already counts a
//! step for every cell and expression node; each time that count reaches a multiple of
//! [`POLL_STEPS`] the clock is read (never once per cell), and starting a root cell counts
//! [`ROOT_TICKS`] on a second counter that is read the same way, so a pass of cheap cells reads the
//! clock too. When the deadline has passed the context is *expired*: the step counter is set to its
//! maximum, so the checks every evaluation already makes end the pass with `#VALUE!` without an
//! extra branch on the hot path, and no value computed while expired is stored in the memo (it may
//! rest on an aborted cell). Values stored before the deadline are complete and stay.

use crate::{EvalContext, EvaluatedCellValue, FormulaError, SheetEngine, WorkbookEngine};
use crate::{PackedCellCoordinate, WorkbookBatchEvaluation, WorkbookEvalTarget};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// The longest a recalculation or evaluation pass runs before it stops and leaves the cells it has
/// not reached as `#VALUE!` (the time a row or column edit may spend rewriting formulas, `AxisBudget`).
pub const MAX_EVAL_TIME: Duration = Duration::from_secs(5);

/// Steps between two clock reads (a power of two; a clock read costs as much as a few cells).
const POLL_STEPS: usize = 1024;
/// The mask of [`POLL_STEPS`] the evaluator tests the step counter with.
pub(crate) const POLL_STEPS_MASK: usize = POLL_STEPS - 1;
/// What starting a root cell counts on the tick counter, so a pass of cells that take few steps
/// each reads the clock about every 16 cells.
const ROOT_TICKS: u32 = 64;

/// The deadline and the tick counter of one evaluation pass.
#[derive(Debug, Default)]
pub(crate) struct Clock {
    deadline: Option<Instant>,
    ticks: u32,
    expired: bool,
}

impl EvalContext<'_> {
    /// Sets the deadline `limit` from now (a limit too large for the clock means none, a zero limit
    /// is already past).
    pub(crate) fn with_time_limit(mut self, limit: Duration) -> Self {
        self.clock.deadline = Instant::now().checked_add(limit);
        if limit.is_zero() {
            self.expire();
        }
        self
    }

    fn expire(&mut self) {
        self.clock.expired = true;
        self.steps = usize::MAX;
        self.array_bytes = crate::array::MAX_ARRAY_BYTES;
    }

    /// The same context with no deadline and a fresh step budget (for cells that hold constants).
    pub(crate) fn without_deadline(mut self) -> Self {
        self.clock = Clock::default();
        self.steps = 0;
        self.array_bytes = 0;
        self
    }

    /// The deadline has passed: nothing computed from now on is valid.
    #[inline]
    pub(crate) fn expired(&self) -> bool {
        self.clock.expired
    }

    /// Charges `n` evaluation steps, polls the deadline across [`POLL_STEPS`] boundaries, and
    /// fails closed with `#VALUE!` when [`MAX_EVAL_STEPS`](crate::MAX_EVAL_STEPS) or the deadline
    /// is exceeded.
    #[inline]
    pub(crate) fn charge_steps(&mut self, n: usize) -> Result<(), FormulaError> {
        let before = self.steps;
        let next = before.saturating_add(n);
        self.steps = next;
        if (before ^ next) >= POLL_STEPS {
            self.poll_deadline();
        }
        if self.steps > crate::MAX_EVAL_STEPS {
            return Err(FormulaError::Value);
        }
        Ok(())
    }

    /// Reserves `rows * cols` dynamic-array cell slots against the per-root 128 MiB array budget
    /// (`#SPILL!` fail-closed on overflow or exhaustion).
    #[inline]
    pub(crate) fn charge_array_cells(
        &mut self,
        rows: usize,
        cols: usize,
    ) -> Result<(), FormulaError> {
        let bytes = crate::array::array_bytes_for(rows, cols)?;
        let next = self
            .array_bytes
            .checked_add(bytes)
            .ok_or(FormulaError::Spill)?;
        if next > crate::array::MAX_ARRAY_BYTES {
            return Err(FormulaError::Spill);
        }
        self.array_bytes = next;
        Ok(())
    }

    /// Counts `n` root ticks and reads the clock when a multiple of [`POLL_STEPS`] is crossed.
    #[inline]
    fn tick(&mut self, n: u32) {
        let before = self.clock.ticks;
        self.clock.ticks = before.wrapping_add(n);
        if (before ^ self.clock.ticks) as usize >= POLL_STEPS {
            self.poll_deadline();
        }
    }

    /// Reads the clock and expires the context when the deadline has passed.
    #[cold]
    pub(crate) fn poll_deadline(&mut self) {
        if self.clock.deadline.is_some_and(|d| Instant::now() >= d) {
            self.expire();
        }
    }

    /// Starts a root cell: a fresh step and array budget unless the deadline has passed.
    #[inline]
    pub(crate) fn reset_steps(&mut self) {
        self.tick(ROOT_TICKS);
        if self.clock.expired {
            self.steps = usize::MAX;
            self.array_bytes = crate::array::MAX_ARRAY_BYTES;
        } else {
            self.steps = 0;
            self.array_bytes = 0;
        }
    }
}

impl SheetEngine {
    /// [`recalculate_all`](Self::recalculate_all) that stops after `limit` (none: never). The cells
    /// the pass did not reach are left out of the map, and the flag says so. A partial dependency
    /// order is not kept for the next call.
    pub fn recalculate_all_within(
        &self,
        limit: Option<Duration>,
    ) -> (HashMap<PackedCellCoordinate, EvaluatedCellValue>, bool) {
        if !self.array_formulas.is_empty() {
            if let Some(settled) = self.as_workbook().settle(limit.unwrap_or(Duration::MAX)) {
                let mut values = settled.session.sheet_values(&self.name);
                values.retain(|&coord, _| !settled.session.is_unreached(&self.name, coord));
                return (values, settled.outcome.timed_out);
            }
        }
        let mut ctx = EvalContext::standalone_id(self.resolved_sheet_id());
        if let Some(limit) = limit {
            ctx = ctx.with_time_limit(limit);
        }
        let mut results = HashMap::with_capacity(self.cells.len());
        let built;
        let plan = match self.plan.get().filter(|p| p.matches(self)) {
            Some(plan) => {
                plan.replay(self, &mut ctx);
                plan
            }
            None => {
                built = crate::dag::RecalcPlan::build(self, &mut ctx);
                if !ctx.expired() {
                    // Another thread may have stored a plan first; either one is valid.
                    let _ = self.plan.set(built.clone());
                }
                &built
            }
        };
        for &coord in &plan.coords {
            ctx.reset_steps();
            let value = if ctx.expired() {
                None
            } else {
                // A value computed while the deadline passed may rest on an aborted cell.
                let val = self.eval_coord_ctx(coord, &mut ctx, 0);
                (!ctx.expired()).then_some(val)
            };
            if let Some(value) = value.or_else(|| self.settled_value(coord, &ctx)) {
                results.insert(coord, value);
            }
        }
        (results, ctx.expired())
    }

    /// The value of `coord` once the deadline has passed: a formula cell the pass finished keeps its
    /// value, a cell that holds a plain value reads as itself, anything else has none.
    pub(crate) fn settled_value(
        &self,
        coord: PackedCellCoordinate,
        ctx: &EvalContext<'_>,
    ) -> Option<EvaluatedCellValue> {
        let sid = self.resolved_sheet_id();
        if crate::dag::is_formula(self.compiled_asts.get(&coord)) {
            return ctx.memo.get_at(sid, coord).cloned();
        }
        let mut calm = EvalContext::standalone_id(sid);
        Some(self.eval_coord_ctx(coord, &mut calm, 0))
    }
}

impl WorkbookEngine {
    /// Evaluates many targets (cell addresses or `=` formulas) in one shared memoized pass, so
    /// formula cells referenced by several targets are computed once, and reports whether any
    /// circular reference was encountered. The pass stops after [`MAX_EVAL_TIME`].
    pub fn evaluate_targets(&self, targets: &[WorkbookEvalTarget<'_>]) -> WorkbookBatchEvaluation {
        self.evaluate_targets_within(targets, MAX_EVAL_TIME)
    }

    /// [`evaluate_targets`](Self::evaluate_targets) that stops after `limit`. A target the pass did
    /// not reach has no value (`unreached`, its entry in `values` is empty) and `timed_out` is set; a
    /// target reached before the deadline has its real value.
    pub fn evaluate_targets_within(
        &self,
        targets: &[WorkbookEvalTarget<'_>],
        limit: Duration,
    ) -> WorkbookBatchEvaluation {
        if self.has_array_formulas() {
            if let Some(settled) = self.evaluate_targets_settled(targets, limit) {
                return settled;
            }
        }
        let primary_id = targets
            .first()
            .and_then(|t| self.sheet(t.sheet))
            .map_or(0, SheetEngine::resolved_sheet_id);
        let mut ctx = EvalContext::with_workbook(self, primary_id).with_time_limit(limit);
        if targets.len() > 1 {
            let mut roots_by_sheet: Vec<Vec<PackedCellCoordinate>> =
                vec![Vec::new(); self.sheets().len()];
            for target in targets {
                if let Some(idx) = self.find_sheet_index(target.sheet) {
                    let expr = target.expr.trim();
                    if crate::parser::strip_open_formula_prefix(expr).is_none() {
                        if let Ok(coord) = PackedCellCoordinate::parse_a1(expr) {
                            roots_by_sheet[idx].push(coord);
                        }
                    }
                }
            }
            for (idx, roots) in roots_by_sheet.into_iter().enumerate() {
                if !roots.is_empty() {
                    self.sheets()[idx].pre_evaluate_dag_iterative(&roots, &mut ctx);
                }
            }
        }
        let mut values = Vec::with_capacity(targets.len());
        let mut unreached = Vec::new();
        for (i, target) in targets.iter().enumerate() {
            let value = if ctx.expired() {
                None
            } else {
                Some(self.evaluate_target(target, &mut ctx))
            };
            // A value computed while the deadline passed may rest on an aborted cell: read it again.
            let value = value
                .filter(|_| !ctx.expired())
                .or_else(|| self.settled_target(target, &ctx));
            if value.is_none() {
                unreached.resize(targets.len(), false);
                unreached[i] = true;
            }
            values.push(value.unwrap_or(EvaluatedCellValue::Empty));
        }
        WorkbookBatchEvaluation {
            values,
            circular: ctx.circular,
            timed_out: ctx.expired(),
            unreached,
            spills: Vec::new(),
        }
    }

    /// The targets of a workbook with array formulas: a session built for the request settles the
    /// arrays and the formulas that read them, and the targets are read from it.
    fn evaluate_targets_settled(
        &self,
        targets: &[WorkbookEvalTarget<'_>],
        limit: Duration,
    ) -> Option<WorkbookBatchEvaluation> {
        // One deadline for the whole request: the settling and every formula target share `limit`.
        let start = Instant::now();
        let mut settled = self.settle(limit)?;
        let mut timed_out = settled.outcome.timed_out;
        let mut values = Vec::with_capacity(targets.len());
        let mut unreached = Vec::new();
        for (i, target) in targets.iter().enumerate() {
            let expr = target.expr.trim();
            let value = if crate::parser::strip_open_formula_prefix(expr).is_some() {
                let left = limit.saturating_sub(start.elapsed());
                let (value, expired) =
                    settled
                        .session
                        .evaluate_text_within(target.sheet, expr, target.anchor, left);
                timed_out |= expired;
                (!expired).then_some(value)
            } else {
                match PackedCellCoordinate::parse_a1(expr) {
                    Ok(coord) if settled.session.is_unreached(target.sheet, coord) => None,
                    Ok(coord) => Some(settled.session.cell_value(target.sheet, coord)),
                    Err(_) => Some(EvaluatedCellValue::Error(FormulaError::Ref)),
                }
            };
            if value.is_none() {
                unreached.resize(targets.len(), false);
                unreached[i] = true;
            }
            values.push(value.unwrap_or(EvaluatedCellValue::Empty));
        }
        Some(WorkbookBatchEvaluation {
            values,
            circular: settled.outcome.circular,
            timed_out,
            unreached,
            spills: settled.session.spill_areas(),
        })
    }

    fn evaluate_target(
        &self,
        target: &WorkbookEvalTarget<'_>,
        ctx: &mut EvalContext<'_>,
    ) -> EvaluatedCellValue {
        let Some(sheet) = self.sheet(target.sheet) else {
            return EvaluatedCellValue::Error(FormulaError::Ref);
        };
        ctx.reset_steps();
        let expr = target.expr.trim();
        if crate::parser::strip_open_formula_prefix(expr).is_some() {
            ctx.current_coord = target.anchor;
            let value = sheet.eval_raw_string_ctx(expr, ctx, 0);
            ctx.current_coord = None;
            value
        } else if let Ok(coord) = PackedCellCoordinate::parse_a1(expr) {
            if !ctx.memo.contains_at(sheet.resolved_sheet_id(), coord) {
                sheet.pre_evaluate_dag_iterative(std::slice::from_ref(&coord), ctx);
            }
            ctx.reset_steps();
            sheet.eval_coord_ctx(coord, ctx, 0)
        } else {
            EvaluatedCellValue::Error(FormulaError::Ref)
        }
    }

    /// The value of `target` once the deadline has passed (see `settled_value`); an expression
    /// target has none.
    fn settled_target(
        &self,
        target: &WorkbookEvalTarget<'_>,
        ctx: &EvalContext<'_>,
    ) -> Option<EvaluatedCellValue> {
        let Some(sheet) = self.sheet(target.sheet) else {
            return Some(EvaluatedCellValue::Error(FormulaError::Ref));
        };
        let expr = target.expr.trim();
        if crate::parser::strip_open_formula_prefix(expr).is_some() {
            return None;
        }
        match PackedCellCoordinate::parse_a1(expr) {
            Ok(coord) => sheet.settled_value(coord, ctx),
            Err(_) => Some(EvaluatedCellValue::Error(FormulaError::Ref)),
        }
    }
}
