//! The bounds an XLSX writer checks before it writes (ECMA-376 limits of the grid and the budgets of the
//! styling it carries).

use crate::{EasyOfficeError, OdsUnpacked, OoxmlCodec, SheetStyling};

impl OoxmlCodec {
    /// Rejects cell / style coordinates outside the ECMA-376 limits.
    pub(crate) fn validate_xlsx_bounds(
        active_sheets: &[OdsUnpacked],
        stylings: &[SheetStyling],
    ) -> Result<(), EasyOfficeError> {
        for (_, cells) in active_sheets {
            for &(row, col, _) in cells {
                if row >= Self::MAX_XLSX_ROWS || col >= Self::MAX_XLSX_COLS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "XLSX 셀 좌표({row}, {col})가 ECMA-376 허용 범위({}x{})를 초과했습니다.",
                        Self::MAX_XLSX_ROWS,
                        Self::MAX_XLSX_COLS
                    )));
                }
            }
        }
        if !crate::workbook_tables_within_bounds(stylings) {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "XLSX 표 개수·열 개수·범위가 허용 범위를 초과했습니다.".to_string(),
            ));
        }
        for styling in stylings {
            let cell = styling
                .cells
                .keys()
                .copied()
                .find(|&(r, c)| r >= Self::MAX_XLSX_ROWS || c >= Self::MAX_XLSX_COLS);
            let link = styling
                .links
                .keys()
                .copied()
                .find(|&(r, c)| r >= Self::MAX_XLSX_ROWS || c >= Self::MAX_XLSX_COLS);
            let column = styling
                .columns
                .keys()
                .find(|&&c| c >= Self::MAX_XLSX_COLS)
                .map(|&c| (0, c));
            let layout = styling
                .merges
                .iter()
                .any(|m| m.row1 >= Self::MAX_XLSX_ROWS || m.col1 >= Self::MAX_XLSX_COLS)
                || styling
                    .hidden_rows
                    .iter()
                    .any(|r| r.1 >= Self::MAX_XLSX_ROWS)
                || styling
                    .hidden_cols
                    .iter()
                    .any(|c| c.1 >= Self::MAX_XLSX_COLS)
                || styling.merges.len() > crate::MAX_MERGES
                || crate::run_total(&styling.hidden_rows) > crate::MAX_HIDDEN_ROWS
                || styling.filter.as_ref().is_some_and(|f| {
                    !f.within_bounds()
                        || f.range.row1 >= Self::MAX_XLSX_ROWS
                        || f.range.col1 >= Self::MAX_XLSX_COLS
                });
            if layout {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "XLSX 병합·숨김 범위가 허용 범위를 초과했습니다.".to_string(),
                ));
            }
            if !styling.view_within_bounds(Self::MAX_XLSX_ROWS, Self::MAX_XLSX_COLS) {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "XLSX 틀 고정·탭 색·그룹·쪽 설정 값이 허용 범위를 초과했습니다.".to_string(),
                ));
            }
            if let Some((row, col)) = cell.or(column).or(link) {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "XLSX 서식 좌표({row}, {col})가 ECMA-376 허용 범위({}x{})를 초과했습니다.",
                    Self::MAX_XLSX_ROWS,
                    Self::MAX_XLSX_COLS
                )));
            }
        }
        Ok(())
    }
}
