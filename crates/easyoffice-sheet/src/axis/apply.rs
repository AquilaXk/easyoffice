//! Applies a row or column edit to a workbook: the edited sheet's cells move, and every formula
//! of every sheet that points at the edited sheet is rewritten.
//!
//! The work is staged: all new formula text is computed first (and checked against a time
//! budget and the sheet edge), and the workbook changes only when that succeeded, so a refused
//! edit leaves it exactly as it was.

use super::{formula::rewrite_formula_refs, rewrite_sheet_names, AxisEdit, EditKind, SheetChange};
use crate::{PackedCellCoordinate, SheetEngine, WorkbookEngine};
use easyoffice_formats::EasyOfficeError;
use std::time::{Duration, Instant};

/// Formulas scanned between two looks at the clock.
const CLOCK_STRIDE: usize = 1_024;

/// Limits of one edit: the time the formula rewrite may take and the most formulas it may change
/// (the page receives every changed formula text).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxisBudget {
    pub time: Duration,
    pub max_rewrites: usize,
}

impl Default for AxisBudget {
    fn default() -> Self {
        Self {
            time: Duration::from_secs(5),
            max_rewrites: 500_000,
        }
    }
}

impl AxisBudget {
    pub const fn within(time: Duration) -> Self {
        Self {
            time,
            max_rewrites: 500_000,
        }
    }
}

/// What an applied edit did to the workbook.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AxisEditReport {
    /// Formula cells whose text changed: sheet index and the cell's new position.
    pub rewritten: Vec<(usize, PackedCellCoordinate)>,
    /// Cells of the edited sheet that a deletion removed.
    pub removed_cells: usize,
}

impl SheetEngine {
    /// Moves the cells (with their compiled formulas) for `edit`; returns how many a deletion
    /// removed.
    fn move_axis(&mut self, edit: &AxisEdit) -> usize {
        let cells = std::mem::take(&mut self.cells);
        let mut asts = std::mem::take(&mut self.compiled_asts);
        let mut subtotals = std::mem::take(&mut self.subtotal_coords);
        self.dense_numbers.clear();
        self.invalidate_caches();
        let mut removed = 0;
        for (coord, raw) in cells {
            let Some(moved) = edit.map_coord(coord) else {
                removed += 1;
                continue;
            };
            if let Some(ast) = asts.remove(&coord) {
                self.update_dense_number(moved, Some(&ast));
                self.compiled_asts.insert(moved, ast);
            }
            if subtotals.remove(&coord) {
                self.subtotal_coords.insert(moved);
            }
            self.cells.insert(moved, raw);
        }
        removed
    }
}

impl WorkbookEngine {
    /// Inserts or deletes rows or columns of `sheet`. Formulas that point at the sheet follow the
    /// move (see [`rewrite_formula_refs`]). Fails, changing nothing, when the sheet is unknown,
    /// an insertion would push a cell off the sheet, or the rewrite outruns `budget` (time or the
    /// number of formulas it changes).
    pub fn apply_axis_edit(
        &mut self,
        sheet: &str,
        edit: AxisEdit,
        budget: AxisBudget,
    ) -> Result<AxisEditReport, EasyOfficeError> {
        let target = self.find_sheet_index(sheet).ok_or_else(|| {
            EasyOfficeError::InvalidArgument(format!("시트를 찾을 수 없습니다: {sheet}"))
        })?;
        let started = Instant::now();
        let target_name = self.sheets()[target].name.clone();
        let mut rewrites: Vec<(usize, PackedCellCoordinate, String)> = Vec::new();
        let mut scanned = 0usize;
        for (si, engine) in self.sheets().iter().enumerate() {
            for (&coord, raw) in &engine.cells {
                if si == target && edit.map_coord(coord).is_none() {
                    if edit.kind == EditKind::Insert {
                        return Err(EasyOfficeError::InvalidArgument(
                            "시트 끝으로 밀려나는 셀이 있어 삽입할 수 없습니다.".to_string(),
                        ));
                    }
                    continue;
                }
                if crate::parser::strip_formula_prefix(raw).is_none() {
                    continue;
                }
                scanned += 1;
                if scanned % CLOCK_STRIDE == 0 && started.elapsed() > budget.time {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "수식이 너무 많아 제한 시간 안에 참조를 옮기지 못했습니다.".to_string(),
                    ));
                }
                if let Some(new) =
                    rewrite_formula_refs(raw, &engine.name, &target_name, &edit, false)
                {
                    if rewrites.len() >= budget.max_rewrites {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(
                            "바뀌는 수식이 너무 많아 행·열을 삽입하거나 삭제할 수 없습니다."
                                .to_string(),
                        ));
                    }
                    rewrites.push((si, coord, new));
                }
            }
        }

        let removed_cells = self.sheets_mut()[target].move_axis(&edit);
        let mut rewritten = Vec::with_capacity(rewrites.len());
        for (si, coord, new) in rewrites {
            let at = if si == target {
                edit.map_coord(coord).unwrap_or(coord)
            } else {
                coord
            };
            self.sheets_mut()[si].set_cell_coord(at, new);
            rewritten.push((si, at));
        }
        Ok(AxisEditReport {
            rewritten,
            removed_cells,
        })
    }
}

impl WorkbookEngine {
    /// The new text of every formula that names the sheet `change` renames or deletes, as sheet
    /// index, cell and text. The workbook itself does not change (the caller renames or removes
    /// the sheet). The formulas of a deleted sheet go with it and are left out. Fails when the
    /// scan outruns `budget` (time or the number of formulas it changes).
    pub fn plan_sheet_rewrites(
        &self,
        change: SheetChange<'_>,
        budget: AxisBudget,
    ) -> Result<Vec<(usize, PackedCellCoordinate, String)>, EasyOfficeError> {
        let started = Instant::now();
        let mut plan = Vec::new();
        let mut scanned = 0usize;
        for (si, engine) in self.sheets().iter().enumerate() {
            let deleted = matches!(change, SheetChange::Delete { name } if engine.name.eq_ignore_ascii_case(name));
            if deleted {
                continue;
            }
            for (&coord, raw) in &engine.cells {
                if crate::parser::strip_formula_prefix(raw).is_none() {
                    continue;
                }
                scanned += 1;
                if scanned % CLOCK_STRIDE == 0 && started.elapsed() > budget.time {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "수식이 너무 많아 제한 시간 안에 시트 이름을 옮기지 못했습니다."
                            .to_string(),
                    ));
                }
                let Some(new) = rewrite_sheet_names(raw, change) else {
                    continue;
                };
                if plan.len() >= budget.max_rewrites {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "바뀌는 수식이 너무 많아 시트를 바꿀 수 없습니다.".to_string(),
                    ));
                }
                plan.push((si, coord, new));
            }
        }
        Ok(plan)
    }
}
