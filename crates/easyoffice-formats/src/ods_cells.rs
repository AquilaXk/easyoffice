//! ODS table cells as `(row, col, text)` within the row, column and cell budgets, read through
//! the one-pass grid walker.

use crate::odf_hwpx_codecs::{ods_budget_error, OdsMultiUnpacked};
use crate::ods_grid::{table_name, walk_ods, OdsCell, OdsSink};
use crate::sheet_styling::ods_styles::{parse_automatic_styles, StyleSink};
use crate::sheet_styling::SheetStyling;
use crate::xml_stream::Attrs;
use crate::{EasyOfficeError, OdfCodec};

struct TextSink {
    sheets: OdsMultiUnpacked,
    name: String,
    cells: Vec<(u32, u32, String)>,
    current_row: u32,
    total_cells: usize,
}

impl TextSink {
    /// Expands one row (with column repeats) at `target_row` into the cells of the table.
    fn emit_row(
        &mut self,
        cells: &[OdsCell<'_>],
        last_populated: usize,
        target_row: u32,
    ) -> Result<(), EasyOfficeError> {
        let mut current_col: u32 = 0;
        for (index, cell) in cells.iter().enumerate() {
            let next_col = current_col
                .checked_add(cell.col_repeat)
                .ok_or_else(|| ods_budget_error("ODS 열 반복 횟수가 허용 범위를 초과했습니다."))?;
            if index <= last_populated && next_col > OdfCodec::MAX_COLS + 1 {
                return Err(ods_budget_error(
                    "ODS 열 인덱스가 허용 범위를 초과했습니다.",
                ));
            }
            if let Some(text) = &cell.text {
                for offset in 0..cell.col_repeat {
                    let r = cell.explicit_row.unwrap_or(target_row);
                    let c = cell
                        .explicit_col
                        .map(|ec| ec.saturating_add(offset))
                        .unwrap_or(current_col + offset);
                    if r > OdfCodec::MAX_ROWS || c > OdfCodec::MAX_COLS {
                        return Err(ods_budget_error("ODS 셀 좌표가 허용 범위를 초과했습니다."));
                    }
                    self.total_cells += 1;
                    if self.total_cells > OdfCodec::MAX_CELLS {
                        return Err(ods_budget_error(
                            "ODS 셀 개수가 허용 예산(200,000)을 초과했습니다.",
                        ));
                    }
                    self.cells.push((r, c, text.clone()));
                }
            }
            current_col = next_col;
        }
        Ok(())
    }
}

impl OdsSink for TextSink {
    fn table(&mut self, attrs: &Attrs<'_>) {
        self.name = table_name(attrs).unwrap_or_else(|| format!("Sheet{}", self.sheets.len() + 1));
        self.cells = Vec::new();
        self.current_row = 0;
    }

    fn row(&mut self, repeat: u32, cells: &[OdsCell<'_>]) -> Result<(), EasyOfficeError> {
        if repeat > OdfCodec::MAX_ROWS + 1 {
            return Err(ods_budget_error(
                "ODS 행 반복 횟수가 허용 범위를 초과했습니다.",
            ));
        }
        let mut last_populated = None;
        for (index, cell) in cells.iter().enumerate() {
            if cell.col_repeat > OdfCodec::MAX_COLS + 1 {
                return Err(ods_budget_error(
                    "ODS 열 반복 횟수가 허용 범위를 초과했습니다.",
                ));
            }
            if cell.text.is_some() {
                last_populated = Some(index);
            }
        }
        let Some(last_populated) = last_populated else {
            self.current_row = self.current_row.saturating_add(repeat);
            return Ok(());
        };
        let next_row = self
            .current_row
            .checked_add(repeat)
            .ok_or_else(|| ods_budget_error("ODS 행 반복 횟수가 허용 범위를 초과했습니다."))?;
        if next_row > OdfCodec::MAX_ROWS + 1 {
            return Err(ods_budget_error(
                "ODS 행 인덱스가 허용 범위를 초과했습니다.",
            ));
        }
        for offset in 0..repeat {
            let target_row = self.current_row + offset;
            if target_row > OdfCodec::MAX_ROWS {
                return Err(ods_budget_error("ODS 행 좌표가 허용 범위를 초과했습니다."));
            }
            self.emit_row(cells, last_populated, target_row)?;
        }
        self.current_row = next_row;
        Ok(())
    }

    fn table_end(&mut self) {
        let name = std::mem::take(&mut self.name);
        self.sheets.push((name, std::mem::take(&mut self.cells)));
    }
}

impl TextSink {
    fn new() -> Self {
        Self {
            sheets: Vec::new(),
            name: String::new(),
            cells: Vec::new(),
            current_row: 0,
            total_cells: 0,
        }
    }

    /// The sheets read; a document without a table has one empty `Sheet1`.
    fn into_sheets(self) -> OdsMultiUnpacked {
        if self.sheets.is_empty() {
            return vec![("Sheet1".to_string(), Vec::new())];
        }
        self.sheets
    }
}

/// Reads every table of `content_xml`.
pub(crate) fn read_ods_sheets(content_xml: &str) -> Result<OdsMultiUnpacked, EasyOfficeError> {
    let mut sink = TextSink::new();
    walk_ods(content_xml, true, &mut sink)?;
    Ok(sink.into_sheets())
}

/// The tables and their styling from one walk of `content_xml`.
pub(crate) fn read_ods_sheets_with_styling(
    content_xml: &str,
) -> Result<(OdsMultiUnpacked, Vec<(String, SheetStyling)>), EasyOfficeError> {
    let styles = parse_automatic_styles(content_xml);
    let mut sinks = (TextSink::new(), StyleSink::new(&styles));
    walk_ods(content_xml, true, &mut sinks)?;
    Ok((sinks.0.into_sheets(), sinks.1.into_tables()))
}
