//! What a card shows of a document the shell cannot draw a page of (PD-179): the first paragraphs,
//! the first rows and columns of the first sheet, or the first slide, taken from the structured
//! content the engine reads for the editors (`UniversalFormatConverter::decode_rich`, which applies
//! its own archive and XML budgets). The input is checked first: the size the window opens, and
//! for a ZIP package what its central directory declares the parts to inflate to.

use super::bounded::RenderError;
use super::card::{self, Align, Block, Content, GRID_COLUMNS, GRID_ROWS};
use super::render::CARD_SIDE_PX;
use super::{zip, ZIP_MAGIC};
use crate::save_thumbnail::render_png_sized;
use easyoffice_formats::{
    DecodedContent, DecodedDocument, OdsUnpacked, RichAlignment, RichBlock, RichListKind,
    UniversalFormatConverter,
};
use easyoffice_sheet::{
    EvaluatedCellValue, PackedCellCoordinate, WorkbookEngine, WorkbookEvalTarget, MAX_EVAL_TIME,
};
use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;
use usvg::fontdb::Database;

/// Most cells of a whole workbook for which the formulas of the grid are evaluated. A larger one
/// shows the formula text: the formulas read other cells, so the whole workbook is loaded into the
/// formula engine, which is not worth it for a card.
const MAX_FORMULA_WORKBOOK_CELLS: usize = 200_000;
/// Most blocks taken from a document: a page holds far fewer.
const MAX_BLOCKS: usize = 64;
/// Most bullets taken from a slide.
const MAX_BULLETS: usize = 12;

/// Largest sum of the sizes a package declares for its parts. The archive reader's own ceiling is
/// 512 MiB, which is more than a card is worth; a package of a window's 10 MiB that declares more
/// than this inflates 12 times or more.
pub const MAX_DECLARED_PACKAGE_BYTES: u64 = 128 * 1_048_576;

/// The card of `bytes` (a file with the extension `extension`) as a PNG whose longer side is
/// [`CARD_SIDE_PX`].
pub fn card_png(
    bytes: &[u8],
    extension: &str,
    fonts: &Arc<Database>,
) -> Result<Vec<u8>, RenderError> {
    let svg = card::svg(&content_of(bytes, extension)?);
    render_png_sized(&svg, fonts, CARD_SIDE_PX).map_err(|_| RenderError::Unreadable)
}

/// The content of the card for `bytes`, a file with the extension `extension`.
pub fn content_of(bytes: &[u8], extension: &str) -> Result<Content, RenderError> {
    if bytes.starts_with(&ZIP_MAGIC)
        && zip::declared_total(&mut Cursor::new(bytes))
            .is_none_or(|total| total > MAX_DECLARED_PACKAGE_BYTES)
    {
        return Err(RenderError::TooLarge);
    }
    let decoded = UniversalFormatConverter::decode_rich(bytes, Some(extension))
        .map_err(|_| RenderError::Unreadable)?;
    Ok(content_from(decoded))
}

pub(super) fn content_from(decoded: DecodedDocument) -> Content {
    match decoded.content {
        DecodedContent::Document(document) => Content::Page(
            document
                .blocks
                .iter()
                .take(MAX_BLOCKS)
                .filter_map(block)
                .collect(),
        ),
        DecodedContent::Workbook(sheets) => Content::Grid(grid(&sheets)),
        DecodedContent::Slides(slides) => {
            let (title, bullets, _) = slides.into_iter().next().unwrap_or_default();
            Content::Slide {
                title,
                lines: bullets.into_iter().take(MAX_BULLETS).collect(),
            }
        }
    }
}

fn block(block: &RichBlock) -> Option<Block> {
    match block {
        RichBlock::Paragraph(paragraph) => {
            let text = paragraph.plain_text();
            if text.trim().is_empty() {
                return None;
            }
            Some(Block::Paragraph {
                text,
                heading: paragraph.heading_level,
                bullet: paragraph.list == Some(RichListKind::Bullet),
                align: match paragraph.alignment {
                    Some(RichAlignment::Center) => Align::Center,
                    Some(RichAlignment::Right) => Align::Right,
                    _ => Align::Left,
                },
            })
        }
        RichBlock::Table(table) => Some(Block::Table(
            table
                .rows
                .iter()
                .take(card::TABLE_MAX_ROWS)
                .map(|row| row.iter().take(card::TABLE_MAX_COLUMNS).cloned().collect())
                .collect(),
        )),
        RichBlock::Image(_) => None,
    }
}

/// The cells of the first [`GRID_ROWS`] rows and [`GRID_COLUMNS`] columns of the first sheet, as
/// shown: a formula is replaced by its value.
pub(super) fn grid(sheets: &[OdsUnpacked]) -> Vec<Vec<String>> {
    grid_within(sheets, MAX_EVAL_TIME)
}

/// [`grid`] with `limit` as the time the formulas may take: a card whose formulas do not finish
/// shows the formula text.
pub(super) fn grid_within(sheets: &[OdsUnpacked], limit: Duration) -> Vec<Vec<String>> {
    let Some((name, cells)) = sheets.first() else {
        return Vec::new();
    };
    let mut rows = vec![vec![String::new(); GRID_COLUMNS]; GRID_ROWS];
    // The cells are sorted by row, then column.
    let window = cells
        .iter()
        .take_while(|(row, _, _)| (*row as usize) < GRID_ROWS)
        .filter(|(_, column, _)| (*column as usize) < GRID_COLUMNS);
    let mut formulas: Vec<(usize, usize, String)> = Vec::new();
    for (row, column, raw) in window {
        let (row, column) = (*row as usize, *column as usize);
        if raw.starts_with('=') {
            if let Ok(coord) = PackedCellCoordinate::new(row as u32, column as u32) {
                formulas.push((row, column, coord.to_a1()));
            }
        }
        rows[row][column] = raw.clone();
    }
    if !formulas.is_empty() {
        evaluate_into(&mut rows, sheets, name, &formulas, limit);
    }
    rows
}

fn evaluate_into(
    rows: &mut [Vec<String>],
    sheets: &[OdsUnpacked],
    name: &str,
    formulas: &[(usize, usize, String)],
    limit: Duration,
) {
    if sheets.iter().map(|(_, cells)| cells.len()).sum::<usize>() > MAX_FORMULA_WORKBOOK_CELLS {
        return;
    }
    let mut workbook = WorkbookEngine::new();
    for (sheet_name, cells) in sheets {
        let Ok(sheet) = workbook.try_add_sheet(sheet_name.clone()) else {
            return;
        };
        for (row, column, raw) in cells {
            if let Ok(coord) = PackedCellCoordinate::new(*row, *column) {
                sheet.set_cell_coord(coord, raw.clone());
            }
        }
    }
    let targets: Vec<WorkbookEvalTarget<'_>> = formulas
        .iter()
        .map(|(_, _, a1)| WorkbookEvalTarget {
            sheet: name,
            expr: a1.as_str(),
            anchor: None,
        })
        .collect();
    let batch = workbook.evaluate_targets_within(&targets, limit);
    if batch.timed_out {
        // The values are not all there: the card keeps the formula text, as for a larger workbook.
        return;
    }
    for ((row, column, _), value) in formulas.iter().zip(batch.values) {
        rows[*row][*column] = match value {
            EvaluatedCellValue::Empty => String::new(),
            shown => shown.to_display_string(),
        };
    }
}
