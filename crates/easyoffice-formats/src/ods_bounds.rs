//! The bounds an ODS writer checks before it writes (the grid and the budgets of the styling it
//! carries).

use crate::odf_hwpx_codecs::ods_budget_error;
use crate::{EasyOfficeError, OdfCodec, OdsUnpacked, SheetStyling};

impl OdfCodec {
    /// Rejects cell / style coordinates and counts above the ODS budgets.
    pub(crate) fn validate_ods_bounds(
        sheets: &[OdsUnpacked],
        stylings: &[SheetStyling],
    ) -> Result<(), EasyOfficeError> {
        let mut total_cells = 0usize;
        for (_, cells) in sheets {
            total_cells = total_cells.saturating_add(cells.len());
            if total_cells > Self::MAX_CELLS {
                return Err(ods_budget_error("ODS 셀 개수가 허용 예산을 초과했습니다."));
            }
            if cells
                .iter()
                .any(|(row, col, _)| *row > Self::MAX_ROWS || *col > Self::MAX_COLS)
            {
                return Err(ods_budget_error("ODS 셀 좌표가 허용 범위를 초과했습니다."));
            }
        }
        if !crate::workbook_tables_within_bounds(stylings) {
            return Err(ods_budget_error(
                "ODS 표 개수·열 개수·범위가 허용 범위를 초과했습니다.",
            ));
        }
        for styling in stylings {
            let bad_cell = styling
                .cells
                .keys()
                .any(|&(r, c)| r > Self::MAX_ROWS || c > Self::MAX_COLS);
            if !crate::sheet_styling::sheet_table::ods_tables::filter_within_bounds(
                styling,
                (Self::MAX_ROWS, Self::MAX_COLS),
            ) {
                return Err(ods_budget_error(
                    "ODS 자동 필터가 허용 범위를 초과했습니다.",
                ));
            }
            if !styling.view_within_bounds(Self::MAX_ROWS + 1, Self::MAX_COLS + 1) {
                return Err(ods_budget_error(
                    "ODS 틀 고정·탭 색·그룹·쪽 설정 값이 허용 범위를 초과했습니다.",
                ));
            }
            if bad_cell || styling.columns.keys().any(|&c| c > Self::MAX_COLS) {
                return Err(ods_budget_error(
                    "ODS 서식 좌표가 허용 범위를 초과했습니다.",
                ));
            }
        }
        Ok(())
    }
}
