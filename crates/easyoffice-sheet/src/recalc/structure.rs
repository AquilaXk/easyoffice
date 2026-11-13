//! Row and column insertion and deletion inside a [`RecalcSession`].
//!
//! The edit moves cells and rewrites formula text in the session's workbook (`axis`), after which
//! every position in the dependency graph is new, so the graph is rebuilt in one pass. The values
//! the session held are carried over to the cells' new positions before recalculating, so the
//! outcome lists only the formula cells whose value differs from what the page already shows.

use super::{RecalcError, RecalcOutcome, RecalcSession};
use crate::axis::{AxisBudget, AxisEdit, AxisEditReport, SheetChange};
use crate::PackedCellCoordinate;
use crate::{MemoTable, WorkbookEngine};
impl RecalcSession {
    /// Applies `edit` to `sheet`. Fails without changing the session when the edit is refused
    /// (unknown sheet, a cell would leave the sheet, too many formulas for `budget`).
    /// A rebuilt graph that outgrows the session's budget makes the session unusable and answers
    /// [`RecalcError::OverBudget`].
    pub fn apply_axis_edit(
        &mut self,
        sheet: &str,
        edit: AxisEdit,
        budget: AxisBudget,
    ) -> Result<(AxisEditReport, RecalcOutcome), RecalcError> {
        if self.over_budget {
            return Err(RecalcError::OverBudget);
        }
        let target = self.sheet_index(sheet)?;
        let report = self.wb.apply_axis_edit(sheet, edit, budget)?;
        let mut wb = std::mem::replace(&mut self.wb, WorkbookEngine::new());
        // The areas arrays filled are at the old positions; the rebuilt session places them again.
        for sheet in wb.sheets_mut() {
            sheet.clear_spills();
        }
        let old_memo = std::mem::replace(&mut self.memo, MemoTable::new(1));
        let mut next = match RecalcSession::with_budget(wb, self.budget) {
            Ok(next) => next,
            Err(err) => {
                self.over_budget = true;
                return Err(err);
            }
        };
        let mut sites = Vec::with_capacity(next.graph.vertex_count());
        next.graph
            .for_each_vertex(|v| sites.push(next.graph.site(v)));
        for (sheet_idx, coord) in sites {
            let before = if sheet_idx == target {
                edit.unmap_coord(coord)
            } else {
                Some(coord)
            };
            let sid = next.wb.sheets()[usize::from(sheet_idx)].resolved_sheet_id();
            if let Some(value) = before.and_then(|c| old_memo.get_at(sid, c)) {
                next.memo.insert_at(sid, coord, value.clone());
            }
        }
        next.rng_state = self.rng_state;
        let outcome = next.recalculate();
        *self = next;
        Ok((report, outcome))
    }
}

impl RecalcSession {
    /// The formula texts a rename or deletion of a sheet rewrites (see
    /// [`WorkbookEngine::plan_sheet_rewrites`]); the session is not changed. The page applies the
    /// answer and opens a new session, as the sheets it holds are not the sheets of this one.
    pub fn plan_sheet_rewrites(
        &self,
        change: SheetChange<'_>,
        budget: AxisBudget,
    ) -> Result<Vec<(usize, PackedCellCoordinate, String)>, RecalcError> {
        if self.over_budget {
            return Err(RecalcError::OverBudget);
        }
        self.wb
            .plan_sheet_rewrites(change, budget)
            .map_err(RecalcError::Invalid)
    }
}
