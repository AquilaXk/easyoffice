//! Conditional formatting and data validation inside a [`RecalcSession`] (PD-171).
//!
//! The session already holds the workbook and the up-to-date formula values, so a rule evaluates
//! against them without a second workbook. After an edit batch the caller passes the edits and the
//! recalculation outcome; only the sheets whose rules the change can reach are re-evaluated, and
//! only cells whose appearance changed are reported.

use super::{CellChange, RecalcOutcome, RecalcSession};
use crate::rules::{CellVisual, RuleRuntime, SheetCells, Verdict};
use crate::{validate_input, EvalContext, EvaluatedCellValue, MemoTable, PackedCellCoordinate};
use easyoffice_formats::{EasyOfficeError, SheetRules};

/// What an edit batch changed in the conditional formatting of the sheets.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct RuleUpdates {
    /// Sheet index and the cells whose appearance changed (see [`CellVisual::is_none`]).
    pub changed: Vec<(usize, Vec<CellVisual>)>,
    /// Sheets whose rules were removed for matching too many cells.
    pub dropped: Vec<usize>,
}

impl RecalcSession {
    fn with_rule_cells<R>(
        &mut self,
        sheet: usize,
        f: impl FnOnce(&mut SheetCells<'_, '_>) -> R,
    ) -> R {
        let primary_id = self
            .wb
            .sheets()
            .first()
            .map_or(1, |s| s.resolved_sheet_id());
        let memo = std::mem::replace(&mut self.memo, MemoTable::new(primary_id));
        let mut ctx =
            EvalContext::with_workbook(&self.wb, primary_id).with_time_limit(self.budget.max_time);
        ctx.memo = memo;
        // A formula the last recalculation did not reach reads as the value the page shows (empty
        // for a new one): a rule never calculates it, which would repeat the work the deadline cut off.
        let blank = self.dirty_without_value();
        for &(sid, coord) in &blank {
            ctx.memo.insert_at(sid, coord, EvaluatedCellValue::Empty);
        }
        let out = {
            let mut cells = SheetCells {
                sheet: &self.wb.sheets()[sheet],
                ctx: &mut ctx,
            };
            f(&mut cells)
        };
        self.memo = ctx.memo;
        self.forget(&blank);
        out
    }

    /// The dirty formulas that hold no value (the memo keeps the last value of one the deadline cut
    /// off), with their sheet id.
    fn dirty_without_value(&self) -> Vec<(u16, PackedCellCoordinate)> {
        let mut blank = Vec::new();
        self.dirty.for_each(|v| {
            let (sheet, coord) = self.graph.site(v);
            let sid = self.wb.sheets()[usize::from(sheet)].resolved_sheet_id();
            if !self.memo.contains_at(sid, coord) {
                blank.push((sid, coord));
            }
        });
        blank
    }

    /// Takes back what a rule left in the memo for formulas that have no value yet: the next
    /// recalculation reports them in full.
    fn forget(&mut self, blank: &[(u16, PackedCellCoordinate)]) {
        for &(sid, coord) in blank {
            self.memo.remove_at(sid, coord);
        }
    }

    /// Replaces the rules of `sheet` and returns the appearance of every formatted cell. Rules
    /// that match more cells than the budget allows are refused.
    pub fn set_rules(
        &mut self,
        sheet: &str,
        rules: SheetRules,
    ) -> Result<Vec<CellVisual>, EasyOfficeError> {
        let idx = self.sheet_index(sheet)?;
        let mut runtime = RuleRuntime::new(rules);
        let visuals = self.with_rule_cells(usize::from(idx), |cells| runtime.evaluate(cells));
        if runtime.is_over_budget() {
            self.rules.remove(&idx);
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "조건부 서식 규칙이 한 번에 칠하는 셀이 너무 많습니다.".to_string(),
            ));
        }
        self.rules.insert(idx, runtime);
        Ok(visuals)
    }

    /// Sheets (by index) that have conditional formatting rules.
    pub fn rule_sheets(&self) -> impl Iterator<Item = u16> + '_ {
        self.rules
            .iter()
            .filter(|(_, r)| r.has_formatting())
            .map(|(&i, _)| i)
    }

    /// Appearance changes after `edits` and the recalculation `outcome`, per sheet index. A sheet
    /// whose rules outgrew the budget is listed in `dropped` and loses its rules.
    pub fn rule_updates(&mut self, edits: &[CellChange], outcome: &RecalcOutcome) -> RuleUpdates {
        let mut touched: Vec<Vec<PackedCellCoordinate>> = vec![Vec::new(); self.wb.sheet_count()];
        for edit in edits {
            if let Some(i) = self.wb.find_sheet_index(&edit.sheet) {
                touched[i].push(edit.coord);
            }
        }
        for c in &outcome.changed {
            touched[c.sheet].push(c.coord);
        }
        let any = touched.iter().any(|t| !t.is_empty());
        let mut out = RuleUpdates::default();
        let sheets: Vec<u16> = self.rule_sheets().collect();
        for idx in sheets {
            let Some(mut runtime) = self.rules.remove(&idx) else {
                continue;
            };
            let here = &touched[usize::from(idx)];
            let visuals =
                self.with_rule_cells(usize::from(idx), |cells| runtime.update(cells, here, any));
            if runtime.is_over_budget() {
                out.dropped.push(usize::from(idx));
                continue;
            }
            self.rules.insert(idx, runtime);
            if !visuals.is_empty() {
                out.changed.push((usize::from(idx), visuals));
            }
        }
        out
    }

    /// Whether `input` may be entered in `coord` of `sheet` (see [`validate_input`]).
    pub fn validate(
        &mut self,
        sheet: &str,
        coord: PackedCellCoordinate,
        input: &str,
    ) -> Result<Verdict, EasyOfficeError> {
        let idx = usize::from(self.sheet_index(sheet)?);
        let Some(runtime) = self.rules.get(&(idx as u16)) else {
            return Ok(Verdict::Valid);
        };
        let rules = runtime.validations.clone();
        let primary_id = self
            .wb
            .sheets()
            .first()
            .map_or(1, |s| s.resolved_sheet_id());
        let blank = self.dirty_without_value();
        let memo = std::mem::replace(&mut self.memo, MemoTable::new(primary_id));
        let (verdict, memo) = validate_input(&mut self.wb, idx, &rules, coord, input, memo);
        self.memo = memo;
        self.forget(&blank);
        Ok(verdict)
    }
}
