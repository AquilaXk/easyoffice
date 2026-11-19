use super::{
    BitGrid, CellMerge, RichTable, TableCellLink, MAX_PADDED_COLUMNS, MAX_PLACED_GRID_CELLS,
    MAX_TABLE_LINKS,
};
use crate::decode_budget::DecodeBudget;
use crate::rich_image::RichImage;
use crate::rich_table_style::{
    CellFill, TableCellBorders, TableCellImage, TableCellStyleEntry, MAX_COL_WIDTH_TWIPS,
    MAX_STYLED_TABLE_CELLS, MIN_COL_WIDTH_TWIPS,
};
use crate::{EasyOfficeError, MAX_MERGE_SPAN};

/// A cell at an explicit grid address (HWPX `cellAddr`/`cellSpan`).
#[derive(Debug)]
pub(crate) struct PlacedCell {
    pub row: u32,
    pub col: u32,
    pub row_span: u32,
    pub col_span: u32,
    pub text: String,
    pub links: Vec<(u32, u32, String)>,
    pub width_twips: Option<u32>,
    pub style: (CellFill, TableCellBorders),
    pub images: Vec<RichImage>,
}

/// Lays addressed cells out as a grid. Cells at a taken address and cells beyond the size limits
/// are dropped and counted; a span that does not fit is reduced to one cell. The grid is as large
/// as the farthest accepted cell or region.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn table_from_placed(
    cells: Vec<PlacedCell>,
    budget: &mut DecodeBudget,
) -> Result<(RichTable, usize), EasyOfficeError> {
    table_from_placed_with_layout(cells, false, budget)
}

pub(crate) fn table_from_placed_with_layout(
    cells: Vec<PlacedCell>,
    fixed_widths: bool,
    budget: &mut DecodeBudget,
) -> Result<(RichTable, usize), EasyOfficeError> {
    let mut dropped = 0usize;
    let (mut rows, mut cols) = (0usize, 0usize);
    let mut accepted: Vec<PlacedCell> = Vec::with_capacity(cells.len());
    for mut cell in cells {
        let (r, c) = (cell.row as usize, cell.col as usize);
        let fits = |rs: u32, cs: u32| {
            let (row_end, col_end) = (r.saturating_add(rs as usize), c.saturating_add(cs as usize));
            let area = rows
                .max(row_end)
                .checked_mul(cols.max(col_end))
                .is_some_and(|n| n <= MAX_PLACED_GRID_CELLS);
            area && col_end <= MAX_PADDED_COLUMNS
        };
        let spans_ok = (1..=MAX_MERGE_SPAN).contains(&cell.row_span)
            && (1..=MAX_MERGE_SPAN).contains(&cell.col_span);
        if !(spans_ok && fits(cell.row_span, cell.col_span)) {
            if cell.row_span != 1 || cell.col_span != 1 {
                dropped += 1;
            }
            (cell.row_span, cell.col_span) = (1, 1);
            if !fits(1, 1) {
                dropped += 1;
                continue;
            }
        }
        rows = rows.max(r + cell.row_span as usize);
        cols = cols.max(c + cell.col_span as usize);
        accepted.push(cell);
    }
    let header_cost = rows.saturating_mul(std::mem::size_of::<Vec<String>>());
    let slots_cost = rows
        .saturating_mul(cols)
        .saturating_mul(std::mem::size_of::<String>());
    budget.charge(
        header_cost.saturating_add(slots_cost),
        "HWPX 표 그리드 슬롯 선차감",
    )?;
    let mut grid = vec![vec![String::new(); cols]; rows];
    let mut anchored = BitGrid::new(rows, cols);
    let mut merges = Vec::new();
    let mut cell_links: Vec<TableCellLink> = Vec::new();
    let mut cell_styles: Vec<TableCellStyleEntry> = Vec::new();
    let mut cell_images: Vec<TableCellImage> = Vec::new();
    let mut col_widths: Vec<Option<u32>> = vec![None; cols];
    let mut span_widths: Vec<(usize, usize, u32)> = Vec::new();
    for cell in accepted {
        let (r, c) = (cell.row as usize, cell.col as usize);
        if anchored.any_set(r, c, c + 1) {
            dropped += 1;
            continue;
        }
        anchored.set(r, c, c + 1);
        grid[r][c] = cell.text;
        for (start, end, url) in cell.links {
            if cell_links.len() < MAX_TABLE_LINKS {
                cell_links.push(TableCellLink {
                    row: cell.row,
                    col: cell.col,
                    start,
                    end,
                    url,
                });
            }
        }
        if (!cell.style.0.is_none() || !cell.style.1.is_unspecified())
            && cell_styles.len() < MAX_STYLED_TABLE_CELLS
        {
            cell_styles.push(TableCellStyleEntry {
                row: cell.row,
                col: cell.col,
                fill: cell.style.0,
                borders: cell.style.1,
            });
        }
        for image in cell.images {
            cell_images.push(TableCellImage {
                row: cell.row,
                col: cell.col,
                image,
            });
        }
        if let Some(w) = cell.width_twips {
            if cell.col_span == 1 {
                if col_widths.get(c).is_some_and(Option::is_none) {
                    col_widths[c] = Some(w);
                }
            } else {
                span_widths.push((c, cell.col_span as usize, w));
            }
        }
        merges.push(CellMerge {
            row: cell.row,
            col: cell.col,
            row_span: cell.row_span,
            col_span: cell.col_span,
        });
    }
    for (c, span, total_w) in span_widths {
        if c + span > cols {
            continue;
        }
        let mut unknown = Vec::new();
        let mut known_sum = 0u32;
        for (offset, slot) in col_widths[c..c + span].iter().enumerate() {
            match slot {
                Some(w) => known_sum = known_sum.saturating_add(*w),
                None => unknown.push(c + offset),
            }
        }
        if !unknown.is_empty() && total_w > known_sum {
            let each = (total_w - known_sum) / unknown.len() as u32;
            if each > 0 {
                for idx in unknown {
                    col_widths[idx] = Some(each);
                }
            }
        }
    }
    let (mut table, rejected, _) = RichTable::with_merges_and_links(grid, merges, cell_links);
    if cols > 0 && col_widths.iter().all(Option::is_some) {
        let widths: Vec<u32> = col_widths.into_iter().flatten().collect();
        let valid_range = widths
            .iter()
            .all(|w| (MIN_COL_WIDTH_TWIPS..=MAX_COL_WIDTH_TWIPS).contains(w));
        let non_uniform = widths.windows(2).any(|w| w[0] != w[1]);
        if valid_range && (fixed_widths || non_uniform) {
            table.col_widths_twips = widths;
        }
    }
    table.cell_styles = cell_styles;
    table.cell_styles = table.validated_cell_styles();
    cell_images.sort_by_key(|ci| (u64::from(ci.row) << 32) | u64::from(ci.col));
    table.cell_images = cell_images;
    Ok((table, dropped + rejected))
}
