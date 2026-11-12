//! Row sort of a sheet engine: orders the rows of a range by several key columns and rewrites the
//! formulas that point into the moved rows (see `sort_keys` for the comparison rules).

use crate::sort_keys::{sort_order, SortColumn, MAX_SORT_KEYS};
use crate::sort_refs::{adjust_formula_row_refs, SortedRows};
use crate::{EvalContext, EvaluatedCellValue, PackedCellCoordinate, SheetEngine};
use easyoffice_formats::EasyOfficeError;

/// One sort key: a column (0-based) and its direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortKey {
    pub col: u32,
    pub ascending: bool,
}

impl SortKey {
    pub const fn asc(col: u32) -> Self {
        Self {
            col,
            ascending: true,
        }
    }

    pub const fn desc(col: u32) -> Self {
        Self {
            col,
            ascending: false,
        }
    }
}

fn invalid(message: &str) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(message.to_string())
}

fn push_value(column: &mut SortColumn, value: &EvaluatedCellValue) {
    match value {
        EvaluatedCellValue::Empty => column.push_empty(),
        EvaluatedCellValue::Number(n) => column.push_number(*n),
        EvaluatedCellValue::Text(t) => column.push_text(t),
        other => column.push_text(&other.to_display_string()),
    }
}

impl SheetEngine {
    /// Sorts the rows `start_row..=end_row` (0-indexed) by `keys`, first key first. Every
    /// populated column of those rows moves with its row; a row that ties on every key keeps its
    /// place. Blank keys end up last in both directions.
    pub fn sort_rows(
        &mut self,
        start_row: u32,
        end_row: u32,
        keys: &[SortKey],
        case_sensitive: bool,
    ) -> Result<(), EasyOfficeError> {
        if start_row > end_row {
            return Err(invalid("정렬 시작 행이 끝 행보다 클 수 없습니다."));
        }
        if keys.is_empty() || keys.len() > MAX_SORT_KEYS {
            return Err(invalid("정렬 기준은 1개 이상 64개 이하여야 합니다."));
        }
        if end_row >= PackedCellCoordinate::MAX_ROWS
            || keys.iter().any(|k| k.col >= PackedCellCoordinate::MAX_COLS)
        {
            return Err(invalid(
                "정렬 행 또는 열 인덱스가 허용 범위를 초과했습니다.",
            ));
        }

        let rows = (end_row - start_row + 1) as usize;
        let mut ctx = EvalContext::standalone_id(self.resolved_sheet_id());
        let mut columns = Vec::with_capacity(keys.len());
        for key in keys {
            let mut column = SortColumn::new(key.ascending, case_sensitive, rows);
            for r in start_row..=end_row {
                let coord = PackedCellCoordinate::new(r, key.col)?;
                if self.cells.contains_key(&coord) {
                    push_value(&mut column, &self.eval_coord_ctx(coord, &mut ctx, 0));
                } else {
                    column.push_empty();
                }
            }
            columns.push(column);
        }
        let order = sort_order(&columns);

        // Take the cells of the range out (no copies), grouped by source row.
        let mut moved: Vec<(u32, u32, String)> = Vec::new();
        let in_range: Vec<PackedCellCoordinate> = self
            .cells
            .keys()
            .copied()
            .filter(|c| (start_row..=end_row).contains(&c.row()))
            .collect();
        for coord in in_range {
            if let Some(raw) = self.cells.remove(&coord) {
                moved.push((coord.row(), coord.col(), raw));
            }
        }
        moved.sort_unstable_by_key(|&(r, c, _)| (r, c));
        let mut starts = vec![0usize; rows + 1];
        for (r, _, _) in &moved {
            starts[(r - start_row) as usize + 1] += 1;
        }
        for i in 0..rows {
            starts[i + 1] += starts[i];
        }

        for (offset, &source) in order.iter().enumerate() {
            let target_row = start_row + offset as u32;
            let source_row = start_row + source;
            let from = starts[source as usize];
            let to = starts[source as usize + 1];
            for (_, col, raw) in &mut moved[from..to] {
                let text = std::mem::take(raw);
                let text = if target_row == source_row {
                    text
                } else {
                    adjust_formula_row_refs(
                        &text,
                        &self.name,
                        SortedRows::new(start_row, end_row, source_row, target_row),
                    )
                };
                self.cells
                    .insert(PackedCellCoordinate::new(target_row, *col)?, text);
            }
        }
        self.rebuild_compiled_asts();
        Ok(())
    }
}
