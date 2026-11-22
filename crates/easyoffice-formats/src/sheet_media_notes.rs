// 참조 구현: jmcnamara/rust_xlsxwriter (MIT OR Apache-2.0, Copyright 2022-2026 John McNamara, src/image.rs, src/note.rs; see THIRD_PARTY_NOTICES.md)

//! Spreadsheet floating image and cell note models with packed `u64` cell indexing and
//! fail-closed resource budgets (`PD-256`).

use crate::cell_type::unpack_cell_pos;
use crate::rich_image::{RichImage, MAX_ALT_TEXT_BYTES, MAX_IMAGE_BYTES};
use crate::sheet_styling::sheet_anchor_edit::AnchorCells;
use crate::sheet_styling::sheet_chart::{
    col_left_px, emu_to_px_rounded, px_to_col_and_emu, px_to_row_and_emu, ChartAnchor,
};
use crate::sheet_styling::sheet_row_offsets::SheetGrid;
use crate::{EasyOfficeError, OoxmlCodec, SheetStyling};

pub const MAX_SHEET_IMAGES_PER_SHEET: usize = 64;
pub const MAX_WORKBOOK_SHEET_IMAGES: usize = 256;
pub const MAX_WORKBOOK_SHEET_IMAGE_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_SHEET_IMAGE_ID_BYTES: usize = 256;
pub const MAX_SHEET_IMAGE_NAME_BYTES: usize = 256;
pub const MAX_SHEET_IMAGE_DIM_PX: u32 = 8_192;

pub const MAX_NOTES_PER_SHEET: usize = 4_096;
pub const MAX_WORKBOOK_NOTES: usize = 16_384;
pub const MAX_NOTE_AUTHOR_BYTES: usize = 256;
pub const MAX_SHEET_NOTE_TEXT_BYTES: usize = 32_768;

/// Floating PNG/JPEG picture anchored onto a worksheet (`SheetStyling::images`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetFloatingImage {
    pub id: String,
    pub name: String,
    pub alt: String,
    pub image: RichImage,
    pub anchor: ChartAnchor,
}

impl SheetFloatingImage {
    pub fn within_bounds(&self) -> bool {
        !self.id.trim().is_empty()
            && self.id.len() <= MAX_SHEET_IMAGE_ID_BYTES
            && self.name.len() <= MAX_SHEET_IMAGE_NAME_BYTES
            && self.alt.len() <= MAX_ALT_TEXT_BYTES
            && self.image.bytes().len() <= MAX_IMAGE_BYTES
            && image_anchor_within_bounds(&self.anchor)
    }
}

/// Cell note (legacy spreadsheet comment) keyed by [`crate::pack_cell_pos`] in `SheetStyling::notes`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SheetCellNote {
    pub author: String,
    pub text: String,
    pub visible: bool,
}

impl SheetCellNote {
    pub fn within_bounds(&self) -> bool {
        self.author.len() <= MAX_NOTE_AUTHOR_BYTES && self.text.len() <= MAX_SHEET_NOTE_TEXT_BYTES
    }
}

/// Checks image anchor bounds without applying chart minimum dimension floors (`180x140`).
pub fn image_anchor_within_bounds(anchor: &ChartAnchor) -> bool {
    (1..=MAX_SHEET_IMAGE_DIM_PX).contains(&anchor.width_px)
        && (1..=MAX_SHEET_IMAGE_DIM_PX).contains(&anchor.height_px)
        && anchor.from_col < OoxmlCodec::MAX_XLSX_COLS
        && anchor.to_col < OoxmlCodec::MAX_XLSX_COLS
        && anchor.from_row < OoxmlCodec::MAX_XLSX_ROWS
        && anchor.to_row < OoxmlCodec::MAX_XLSX_ROWS
}

/// Recomputes pixel coordinates from two-cell grid anchors for floating images (`1..=8192 px`).
pub fn sync_image_px_from_grid(anchor: &mut ChartAnchor, grid: &SheetGrid<'_>) {
    let (columns, rows) = (grid.columns, grid.rows());
    let x0 =
        col_left_px(anchor.from_col, columns).saturating_add(emu_to_px_rounded(anchor.col_off_emu));
    let y0 = rows
        .top(anchor.from_row)
        .saturating_add(emu_to_px_rounded(anchor.row_off_emu));
    let x1 = col_left_px(anchor.to_col, columns)
        .saturating_add(emu_to_px_rounded(anchor.to_col_off_emu));
    let y1 = rows
        .top(anchor.to_row)
        .saturating_add(emu_to_px_rounded(anchor.to_row_off_emu));
    anchor.left_px = x0.min(u64::from(u32::MAX)) as u32;
    anchor.top_px = y0.min(u64::from(u32::MAX)) as u32;
    if x1 > x0 {
        anchor.width_px = ((x1 - x0).min(u64::from(MAX_SHEET_IMAGE_DIM_PX)) as u32).max(1);
    }
    if y1 > y0 {
        anchor.height_px = ((y1 - y0).min(u64::from(MAX_SHEET_IMAGE_DIM_PX)) as u32).max(1);
    }
}

/// Recomputes two-cell grid anchors from pixel coordinates for floating images.
pub fn sync_image_grid_from_px(anchor: &mut ChartAnchor, grid: &SheetGrid<'_>) {
    let (columns, rows) = (grid.columns, grid.rows());
    let w = u64::from(anchor.width_px.clamp(1, MAX_SHEET_IMAGE_DIM_PX));
    let h = u64::from(anchor.height_px.clamp(1, MAX_SHEET_IMAGE_DIM_PX));
    let left = u64::from(anchor.left_px);
    let top = u64::from(anchor.top_px);
    let (fc, fce) = px_to_col_and_emu(left, columns);
    let (fr, fre) = px_to_row_and_emu(top, rows);
    let (tc, tce) = px_to_col_and_emu(left.saturating_add(w), columns);
    let (tr, tre) = px_to_row_and_emu(top.saturating_add(h), rows);
    anchor.from_col = fc;
    anchor.col_off_emu = fce;
    anchor.from_row = fr;
    anchor.row_off_emu = fre;
    anchor.to_col = tc;
    anchor.to_col_off_emu = tce;
    anchor.to_row = tr;
    anchor.to_row_off_emu = tre;
}

/// Returns effective two-cell DrawingML coordinates for a floating image anchor.
pub fn effective_image_two_cell(anchor: &ChartAnchor, grid: &SheetGrid<'_>) -> AnchorCells {
    let mut copy = *anchor;
    copy.settle(grid, sync_image_grid_from_px);
    copy.cells()
}

/// Validates workbook and per-sheet floating image budgets.
pub fn validate_workbook_sheet_images(stylings: &[SheetStyling]) -> Result<(), EasyOfficeError> {
    let mut total_count = 0usize;
    let mut total_bytes = 0usize;
    for st in stylings {
        if st.images.len() > MAX_SHEET_IMAGES_PER_SHEET {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "시트 그림 수({})가 허용 상한({MAX_SHEET_IMAGES_PER_SHEET})을 초과했습니다.",
                st.images.len()
            )));
        }
        total_count = total_count.saturating_add(st.images.len());
        if total_count > MAX_WORKBOOK_SHEET_IMAGES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "통합 문서 전체 그림 수({total_count})가 허용 상한({MAX_WORKBOOK_SHEET_IMAGES})을 초과했습니다."
            )));
        }
        for img in &st.images {
            total_bytes = total_bytes.saturating_add(img.image.bytes().len());
            if total_bytes > MAX_WORKBOOK_SHEET_IMAGE_BYTES {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "통합 문서 그림 총 크기({total_bytes}바이트)가 허용 상한({MAX_WORKBOOK_SHEET_IMAGE_BYTES}바이트)을 초과했습니다."
                )));
            }
            if !img.within_bounds() {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "시트 그림 '{}'의 메타데이터 또는 크기 예산이 허용 상한을 초과했습니다.",
                    img.id
                )));
            }
        }
    }
    Ok(())
}

/// Validates workbook and per-sheet cell note budgets and cell coordinates.
pub fn validate_workbook_notes(stylings: &[SheetStyling]) -> Result<(), EasyOfficeError> {
    let mut total_notes = 0usize;
    for st in stylings {
        if st.notes.len() > MAX_NOTES_PER_SHEET {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "시트 메모 수({})가 허용 상한({MAX_NOTES_PER_SHEET})을 초과했습니다.",
                st.notes.len()
            )));
        }
        total_notes = total_notes.saturating_add(st.notes.len());
        if total_notes > MAX_WORKBOOK_NOTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "통합 문서 전체 메모 수({total_notes})가 허용 상한({MAX_WORKBOOK_NOTES})을 초과했습니다."
            )));
        }
        for (&packed, note) in &st.notes {
            let (row, col) = unpack_cell_pos(packed);
            if row >= OoxmlCodec::MAX_XLSX_ROWS || col >= OoxmlCodec::MAX_XLSX_COLS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "시트 메모 좌표({row}, {col})가 XLSX 허용 범위를 초과했습니다."
                )));
            }
            if !note.within_bounds() {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "시트 메모 작성자 또는 본문 길이가 허용 상한을 초과했습니다.".to_string(),
                ));
            }
        }
    }
    Ok(())
}
