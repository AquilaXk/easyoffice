//! What a session says about arrays and how a request without a session uses one (PD-217): a
//! workbook with array formulas is calculated by a session that lives for the request, because the
//! cells an array fills exist only after a pass and the passes that follow.

use super::{Budget, RecalcOutcome, RecalcSession};
use crate::{
    EvalContext, EvaluatedCellValue, FormulaError, PackedCellCoordinate, SheetEngine,
    WorkbookEngine,
};
use std::collections::HashMap;
use std::time::Duration;

/// One array that spills: its sheet, anchor, size and the values of its area, row by row.
#[derive(Debug, Clone, PartialEq)]
pub struct SpillArea {
    pub sheet: String,
    pub anchor: PackedCellCoordinate,
    pub rows: usize,
    pub cols: usize,
    /// The array could not be placed: the anchor shows `#SPILL!` and the values are not on the sheet.
    pub blocked: bool,
    pub values: Vec<EvaluatedCellValue>,
}

impl RecalcSession {
    /// Whether the formula at `coord` is one the last [`recalculate`](Self::recalculate) did not reach.
    pub fn is_unreached(&self, sheet: &str, coord: PackedCellCoordinate) -> bool {
        self.wb
            .find_sheet_index(sheet)
            .and_then(|idx| self.graph.vertex_at(idx as u16, coord))
            .is_some_and(|v| self.dirty.contains(v))
    }

    /// Tells the session which cells of `sheet` may be changed (`None`: the sheet is not protected). An array that
    /// is placed stays where it is until its formula is calculated again; an array that could not be placed is
    /// tried again by the next [`recalculate`](Self::recalculate).
    pub fn set_sheet_lock(
        &mut self,
        sheet: &str,
        lock: Option<crate::SheetLock>,
    ) -> Result<(), super::RecalcError> {
        let idx = self.sheet_index(sheet)?;
        let engine = &mut self.wb.sheets_mut()[usize::from(idx)];
        engine.set_spill_lock(lock);
        let anchors: Vec<PackedCellCoordinate> =
            engine.spill.blocked().map(|(anchor, _)| anchor).collect();
        for coord in anchors {
            if let Some(v) = self.graph.vertex_at(idx, coord) {
                self.dirty.insert(v);
            }
        }
        Ok(())
    }

    /// Calculates formula text (`=...`) as if it stood in `anchor`, reading the session's values.
    pub fn evaluate_text(
        &mut self,
        sheet: &str,
        text: &str,
        anchor: Option<PackedCellCoordinate>,
    ) -> EvaluatedCellValue {
        self.evaluate_text_within(sheet, text, anchor, self.budget.max_time)
            .0
    }

    /// [`evaluate_text`](Self::evaluate_text) that stops after `limit`; the flag says the time ran out
    /// and the value is not one to show.
    pub(crate) fn evaluate_text_within(
        &mut self,
        sheet: &str,
        text: &str,
        anchor: Option<PackedCellCoordinate>,
        limit: Duration,
    ) -> (EvaluatedCellValue, bool) {
        let Some(idx) = self.wb.find_sheet_index(sheet) else {
            return (EvaluatedCellValue::Error(FormulaError::Ref), false);
        };
        let engine = &self.wb.sheets()[idx];
        let mut ctx =
            EvalContext::with_workbook(&self.wb, engine.resolved_sheet_id()).with_time_limit(limit);
        ctx.memo = std::mem::replace(&mut self.memo, crate::MemoTable::new(1));
        ctx.rng_state = self.rng_state;
        ctx.current_coord = anchor;
        ctx.reset_steps();
        let value = engine.eval_raw_string_ctx(text, &mut ctx, 0);
        self.rng_state = ctx.rng_state;
        let expired = ctx.expired();
        self.memo = ctx.memo;
        (value, expired)
    }

    /// The value of every populated cell of `sheet`, the cells arrays fill included.
    pub fn sheet_values(&self, sheet: &str) -> HashMap<PackedCellCoordinate, EvaluatedCellValue> {
        let Some(idx) = self.wb.find_sheet_index(sheet) else {
            return HashMap::new();
        };
        let engine = &self.wb.sheets()[idx];
        let mut ctx = EvalContext::with_workbook(&self.wb, engine.resolved_sheet_id());
        ctx.memo = std::mem::replace(
            &mut ctx.memo,
            crate::MemoTable::new(engine.resolved_sheet_id()),
        );
        let coords = engine.cell_index().sorted_coords.clone();
        let mut out = HashMap::with_capacity(coords.len());
        for coord in coords {
            let value = match self.memo.get_at(engine.resolved_sheet_id(), coord) {
                Some(v) => v.clone(),
                None => {
                    ctx.reset_steps();
                    engine.eval_coord_ctx(coord, &mut ctx, 0)
                }
            };
            out.insert(coord, value);
        }
        out
    }

    /// Every array of the workbook that is bigger than one cell, in sheet and row-major order.
    pub fn spill_areas(&self) -> Vec<SpillArea> {
        let mut out = Vec::new();
        for engine in self.wb.sheets() {
            let sid = engine.resolved_sheet_id();
            let mut anchors: Vec<_> = engine
                .spill
                .anchors_with_area()
                .filter(|(_, a)| a.rows * a.cols > 1)
                .collect();
            anchors.sort_unstable_by_key(|&(c, _)| c);
            for (anchor, area) in anchors {
                let (rows, cols) = (area.rows as usize, area.cols as usize);
                let mut values = Vec::with_capacity(if area.blocked { 0 } else { rows * cols });
                if !area.blocked {
                    for r in 0..rows {
                        for c in 0..cols {
                            let at = PackedCellCoordinate(
                                (u64::from(anchor.row() + r as u32) << 32)
                                    | u64::from(anchor.col() + c as u32),
                            );
                            values.push(
                                self.memo
                                    .get_at(sid, at)
                                    .cloned()
                                    .unwrap_or(EvaluatedCellValue::Empty),
                            );
                        }
                    }
                }
                out.push(SpillArea {
                    sheet: engine.name.clone(),
                    anchor,
                    rows,
                    cols,
                    blocked: area.blocked,
                    values,
                });
            }
        }
        out
    }
}

/// What a request without a session gets from a session built for it.
pub(crate) struct Settled {
    pub(crate) session: RecalcSession,
    pub(crate) outcome: RecalcOutcome,
}

impl WorkbookEngine {
    /// Whether any formula of the workbook is one the array evaluator calculates.
    pub fn has_array_formulas(&self) -> bool {
        self.sheets().iter().any(|s| !s.array_formulas.is_empty())
    }

    /// A session over a copy of the workbook, calculated within `limit`; `None` when the workbook
    /// is over the session's budget (its arrays then show their first value).
    pub(crate) fn settle(&self, limit: Duration) -> Option<Settled> {
        let budget = Budget {
            max_time: limit,
            ..Budget::default()
        };
        let mut session = RecalcSession::with_budget(self.clone(), budget).ok()?;
        let outcome = session.recalculate();
        Some(Settled { session, outcome })
    }
}

impl SheetEngine {
    /// A workbook of one sheet with this sheet's cells (the standalone sheet has no workbook, but an
    /// array needs a session).
    pub(crate) fn as_workbook(&self) -> WorkbookEngine {
        let mut wb = WorkbookEngine::new();
        let sheet = wb.add_sheet(self.name.clone());
        for (&coord, raw) in &self.cells {
            sheet.set_cell_coord(coord, raw.clone());
        }
        wb
    }
}
