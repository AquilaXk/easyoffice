//! Applies a row or column edit to a workbook: the edited sheet's cells move, and every formula
//! of every sheet that points at the edited sheet is rewritten.
//!
//! The work is staged: all new formula text is computed first (and checked against a time
//! budget and the sheet edge), and the workbook changes only when that succeeded, so a refused
//! edit leaves it exactly as it was.

use super::{formula::rewrite_formula_refs, AxisEdit, EditKind};
use crate::{PackedCellCoordinate, SheetEngine, WorkbookEngine};
use easyoffice_formats::EasyOfficeError;
use std::time::{Duration, Instant};

/// Formulas scanned between two looks at the clock.
const CLOCK_STRIDE: usize = 1_024;

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
    /// an insertion would push a cell off the sheet, or the rewrite outruns `budget`.
    pub fn apply_axis_edit(
        &mut self,
        sheet: &str,
        edit: AxisEdit,
        budget: Duration,
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
                if scanned % CLOCK_STRIDE == 0 && started.elapsed() > budget {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "수식이 너무 많아 제한 시간 안에 참조를 옮기지 못했습니다.".to_string(),
                    ));
                }
                if let Some(new) =
                    rewrite_formula_refs(raw, &engine.name, &target_name, &edit, false)
                {
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
