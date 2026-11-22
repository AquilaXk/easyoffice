//! Runtime workbook calculation options (`INFO("RECALC")` and bounded iterative circular-reference
//! evaluation, PD-309).
//!
//! Mirrors the thread-local guard pattern of `date_system.rs`: callers enter the workbook's active
//! calculation mode and iteration parameters around evaluation and drop the guard afterward. When
//! iteration is disabled (the default), cyclic references fall through cleanly to `#NUM!` with zero
//! overhead on acyclic paths.

use std::cell::Cell;
use std::collections::HashMap;

use crate::dag::{is_formula, DepScanner, DepWalk, Frame, PackedDfsFrame, ScanSite, SheetTable};
use crate::memo::DfsState;
use crate::{
    DetSet, EvalContext, EvaluatedCellValue, Expr, MemoTable, PackedCellCoordinate, SheetEngine,
};

const MAX_ITERATE_COUNT: u16 = 32_767;
const DEFAULT_ITERATE_COUNT: u16 = 100;
const MAX_ITERATE_DELTA: f64 = 1e100;
const MAX_ITER_TOTAL_STEPS: usize = 16_777_216;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RuntimeCalcState {
    manual: bool,
    iterate: bool,
    iterate_count: u16,
    iterate_delta_bits: u64,
}

impl Default for RuntimeCalcState {
    fn default() -> Self {
        Self {
            manual: false,
            iterate: false,
            iterate_count: DEFAULT_ITERATE_COUNT,
            iterate_delta_bits: 0.001f64.to_bits(),
        }
    }
}

impl RuntimeCalcState {
    fn from_args(manual: bool, iterate: Option<(u16, f64)>) -> Self {
        match iterate {
            Some((count, delta)) => {
                let bounded_count = count.clamp(1, MAX_ITERATE_COUNT);
                let bounded_delta =
                    if delta.is_finite() && (0.0..=MAX_ITERATE_DELTA).contains(&delta) {
                        if delta == 0.0 {
                            0.0
                        } else {
                            delta
                        }
                    } else {
                        0.001
                    };
                Self {
                    manual,
                    iterate: true,
                    iterate_count: bounded_count,
                    iterate_delta_bits: bounded_delta.to_bits(),
                }
            }
            None => Self {
                manual,
                ..Self::default()
            },
        }
    }
}

thread_local! {
    static CALC_STATE: Cell<RuntimeCalcState> = const {
        Cell::new(RuntimeCalcState {
            manual: false,
            iterate: false,
            iterate_count: DEFAULT_ITERATE_COUNT,
            // 0.001f64.to_bits() == 0x3F50_624D_D2F1_A9FC
            iterate_delta_bits: 0x3F50_624D_D2F1_A9FC,
        })
    };
}

/// Keeps workbook calculation options in effect on this thread until dropped.
#[must_use = "the calculation options are only in effect while the guard lives"]
pub struct CalcOptionsGuard {
    previous: RuntimeCalcState,
}

impl Drop for CalcOptionsGuard {
    fn drop(&mut self) {
        CALC_STATE.with(|cell| cell.set(self.previous));
    }
}

/// Applies `manual` recalc reporting (`INFO("RECALC")`) and optional iterative calculation
/// `(max_iterations, max_change)` for the lifetime of the returned guard.
pub fn enter_calc_options(manual: bool, iterate: Option<(u16, f64)>) -> CalcOptionsGuard {
    let next = RuntimeCalcState::from_args(manual, iterate);
    CalcOptionsGuard {
        previous: CALC_STATE.with(|cell| cell.replace(next)),
    }
}

/// Whether this thread calculates in manual calculation mode (`INFO("RECALC") == "Manual"`).
#[inline]
pub fn in_manual_calc() -> bool {
    CALC_STATE.with(|cell| cell.get().manual)
}

/// Active iterative calculation parameters `(max_iterations, max_change)`, if enabled.
#[inline]
pub fn in_iteration() -> Option<(u16, f64)> {
    let st = CALC_STATE.with(Cell::get);
    st.iterate
        .then(|| (st.iterate_count, f64::from_bits(st.iterate_delta_bits)))
}

#[inline]
pub(crate) fn runtime_calc_signature() -> (bool, Option<(u16, u64)>) {
    let st = CALC_STATE.with(Cell::get);
    (
        st.manual,
        st.iterate
            .then_some((st.iterate_count, st.iterate_delta_bits)),
    )
}

/// Value returned when an iterative evaluation encounters an active cycle back-edge.
pub(crate) fn iter_fallback(
    ctx: &mut EvalContext<'_>,
    sheet_id: u16,
    coord: PackedCellCoordinate,
) -> Option<EvaluatedCellValue> {
    in_iteration()?;
    ctx.iter_back_edge = true;
    Some(
        ctx.iter_values
            .as_ref()
            .and_then(|table| table.get_at(sheet_id, coord))
            .cloned()
            .unwrap_or(EvaluatedCellValue::Number(0.0)),
    )
}

#[inline]
fn record_iter_delta(
    old_val: Option<&EvaluatedCellValue>,
    new_val: Option<&EvaluatedCellValue>,
    max_diff: &mut f64,
    changed_non_num: &mut bool,
) {
    match (old_val, new_val) {
        (Some(EvaluatedCellValue::Number(a)), Some(EvaluatedCellValue::Number(b))) => {
            *max_diff = max_diff.max((b - a).abs());
        }
        (None, Some(EvaluatedCellValue::Number(b))) => {
            *max_diff = max_diff.max(b.abs());
        }
        (Some(a), Some(b)) if a == b => {}
        (None, None) => {}
        _ => {
            *changed_non_num = true;
        }
    }
}

/// Post-order dependency collection followed by bounded Gauss-Seidel passes when iterative
/// circular calculation is enabled.
pub(crate) fn pre_evaluate_iterative(
    root: &SheetEngine,
    roots: &[PackedCellCoordinate],
    ctx: &mut EvalContext<'_>,
    max_iters: u16,
    max_delta: f64,
) {
    let self_sid = root.resolved_sheet_id();
    let mut scan = DepScanner {
        root,
        wb: ctx.workbook,
        sheets: SheetTable {
            slots: vec![root],
            names: HashMap::new(),
        },
        expr_stack: Vec::new(),
        seen_shared: DetSet::default(),
        scratch: Vec::new(),
    };
    let mut state = DfsState::new(self_sid);
    let mut stack: Vec<Frame> = Vec::with_capacity(roots.len().min(4096));
    let mut order: Vec<(u16, u16, PackedCellCoordinate)> = Vec::new();
    let mut saw_static_cycle = false;

    for &coord in roots.iter().rev() {
        if is_formula(root.compiled_asts.get(&coord)) && !ctx.memo.contains_at(self_sid, coord) {
            stack.push(PackedDfsFrame::new(0, coord, false));
        }
    }
    while let Some(frame) = stack.pop() {
        if ctx.expired() {
            break;
        }
        let (slot, coord, expanded) = frame.unpack();
        let sheet = scan.sheets.get(slot);
        let sid = sheet.resolved_sheet_id();
        if expanded {
            state.set(sid, coord, 2);
            order.push((slot, sid, coord));
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
            saw_static_cycle = true;
        }
    }

    if order.is_empty() || ctx.expired() {
        return;
    }

    let prev_touched = ctx.iter_touched.take();
    ctx.iter_touched = Some(Vec::new());
    let mut pass = 0u16;
    let mut total_steps = 0usize;
    let mut extra_touched: Vec<(u16, PackedCellCoordinate)> = Vec::new();

    loop {
        ctx.iter_back_edge = false;
        if let Some(touched) = ctx.iter_touched.as_mut() {
            touched.clear();
        }
        for &(slot, sid, coord) in &order {
            if ctx.expired() {
                break;
            }
            let target_sheet = scan.sheets.get(slot);
            if !ctx.memo.contains_at(sid, coord) {
                ctx.reset_steps();
                target_sheet.with_coord_value(coord, ctx, 0, |_| ());
                total_steps = total_steps.saturating_add(ctx.steps.max(1));
            }
        }
        if ctx.expired() {
            break;
        }
        let has_cycle = saw_static_cycle || ctx.iter_back_edge;
        if has_cycle {
            ctx.iterated = true;
        }
        if !has_cycle {
            break;
        }

        if let Some(touched) = ctx.iter_touched.as_ref() {
            extra_touched.extend(touched.iter().copied());
            extra_touched.sort_unstable();
            extra_touched.dedup();
        }

        let mut max_diff = 0.0f64;
        let mut changed_non_num = false;
        for &(_, sid, coord) in &order {
            let new_val = ctx.memo.get_at(sid, coord);
            let old_val = ctx.iter_values.as_ref().and_then(|t| t.get_at(sid, coord));
            record_iter_delta(old_val, new_val, &mut max_diff, &mut changed_non_num);
        }
        for &(sid, coord) in &extra_touched {
            let new_val = ctx.memo.get_at(sid, coord);
            let old_val = ctx.iter_values.as_ref().and_then(|t| t.get_at(sid, coord));
            record_iter_delta(old_val, new_val, &mut max_diff, &mut changed_non_num);
        }

        if !changed_non_num && max_diff <= max_delta {
            break;
        }
        if pass + 1 >= max_iters || total_steps >= MAX_ITER_TOTAL_STEPS {
            break;
        }

        let mut iter_table = ctx
            .iter_values
            .take()
            .unwrap_or_else(|| MemoTable::new(self_sid));
        for &(_, sid, coord) in &order {
            if let Some(val) = ctx.memo.remove_at(sid, coord) {
                iter_table.insert_at(sid, coord, val);
            }
        }
        for &(sid, coord) in &extra_touched {
            if let Some(val) = ctx.memo.remove_at(sid, coord) {
                iter_table.insert_at(sid, coord, val);
            }
        }
        ctx.iter_values = Some(iter_table);
        pass += 1;
    }

    ctx.iter_touched = prev_touched;
}
