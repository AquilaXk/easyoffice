//! Table block of the rich model: a text grid plus the merged regions drawn over it.
//!
//! The grid keeps one string per cell. A merged region is anchored at its top-left cell, which
//! holds the text; the other cells of the region stay in the grid (normally empty) so every
//! consumer that only looks at text keeps working. Regions are validated against the grid:
//! one-cell regions, spans past the grid, spans above [`MAX_MERGE_SPAN`] and regions that overlap
//! an earlier region are dropped and counted. Overlap checks use a row-major bit grid, so the
//! work is proportional to the number of cells, and a work budget stops hostile inputs that
//! declare many large regions.

use crate::decode_budget::DecodeBudget;
use crate::rich_image::RichImage;
pub use crate::rich_table_cells::*;
pub use crate::rich_table_style::*;
use crate::EasyOfficeError;

/// Largest row or column span of one region (15 bits, so a cell state fits in a `u32`).
pub const MAX_MERGE_SPAN: u32 = 0x7FFF;
/// Maximum hyperlinks kept on one table.
pub const MAX_TABLE_LINKS: usize = 50_000;
const MAX_CELL_URL_BYTES: usize = 4_096;
/// Largest grid (rows times widest row) that regions are validated against.
const MAX_GRID_CELLS: usize = 4_000_000;
/// Largest grid of cells at explicit addresses.
const MAX_PLACED_GRID_CELLS: usize = 1 << 20;
/// Widest row a reader pads out for a column span.
pub(crate) const MAX_PADDED_COLUMNS: usize = 1_024;
/// Bit-grid words that validation may touch before the remaining regions are dropped.
const MAX_VALIDATION_WORDS: usize = 1 << 24;
const WORD_BITS: usize = 64;
const SPAN_BITS: u32 = 15;
const COVERED_FLAG: u32 = 1 << 31;

/// A merged region: its top-left cell and its size in cells (both at least 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellMerge {
    pub row: u32,
    pub col: u32,
    pub row_span: u32,
    pub col_span: u32,
}

/// Hyperlink covering the UTF-8 byte range `start..end` inside `rows[row][col]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableCellLink {
    pub row: u32,
    pub col: u32,
    pub start: u32,
    pub end: u32,
    pub url: String,
}

/// A table: rows of final cell text, merged regions over them, hyperlink spans, column widths,
/// per-cell shading/borders, and inline cell pictures.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RichTable {
    pub rows: Vec<Vec<String>>,
    pub merges: Vec<CellMerge>,
    pub links: Vec<TableCellLink>,
    pub col_widths_twips: Vec<u32>,
    pub cell_styles: Vec<TableCellStyleEntry>,
    pub cell_images: Vec<TableCellImage>,
    pub cell_paragraphs: Vec<TableCellParagraphs>,
}

impl From<Vec<Vec<String>>> for RichTable {
    fn from(rows: Vec<Vec<String>>) -> Self {
        Self::new(rows)
    }
}

impl RichTable {
    pub fn new(rows: Vec<Vec<String>>) -> Self {
        Self {
            rows,
            merges: Vec::new(),
            links: Vec::new(),
            col_widths_twips: Vec::new(),
            cell_styles: Vec::new(),
            cell_images: Vec::new(),
            cell_paragraphs: Vec::new(),
        }
    }

    /// Builds a table from cell text and candidate regions. Returns the table with the valid
    /// regions and the number of regions that were dropped.
    pub fn with_merges(rows: Vec<Vec<String>>, merges: Vec<CellMerge>) -> (Self, usize) {
        let (merges, dropped) = validate_merges(&rows, &merges);
        (
            Self {
                rows,
                merges,
                links: Vec::new(),
                col_widths_twips: Vec::new(),
                cell_styles: Vec::new(),
                cell_images: Vec::new(),
                cell_paragraphs: Vec::new(),
            },
            dropped,
        )
    }

    /// Builds a table from cell text, candidate regions and candidate cell hyperlinks. Returns
    /// `(table, dropped_merges, dropped_links)`.
    pub fn with_merges_and_links(
        rows: Vec<Vec<String>>,
        merges: Vec<CellMerge>,
        links: Vec<TableCellLink>,
    ) -> (Self, usize, usize) {
        let (merges, dropped_merges) = validate_merges(&rows, &merges);
        let (links, dropped_links) = validate_links(&rows, links);
        (
            Self {
                rows,
                merges,
                links,
                col_widths_twips: Vec::new(),
                cell_styles: Vec::new(),
                cell_images: Vec::new(),
                cell_paragraphs: Vec::new(),
            },
            dropped_merges,
            dropped_links,
        )
    }

    /// Validated, non-overlapping cell links sorted by `(row, col, start, end)` for single-pass writers.
    pub(crate) fn validated_links(&self) -> Vec<TableCellLink> {
        if self.links.is_empty() {
            return Vec::new();
        }
        validate_links(&self.rows, self.links.clone()).0
    }

    /// Validated, non-overlapping merged regions within the grid for writers.
    pub fn validated_merges(&self) -> Vec<CellMerge> {
        if self.merges.is_empty() {
            return Vec::new();
        }
        validate_merges(&self.rows, &self.merges).0
    }

    /// Column widths clamped to [`MIN_COL_WIDTH_TWIPS`]..=[`MAX_COL_WIDTH_TWIPS`] when
    /// `self.col_widths_twips.len() == max_cols && max_cols > 0`.
    pub fn validated_col_widths(&self) -> Option<Vec<u32>> {
        let max_cols = self.column_count();
        if max_cols == 0 || self.col_widths_twips.len() != max_cols {
            return None;
        }
        Some(
            self.col_widths_twips
                .iter()
                .map(|&w| w.clamp(MIN_COL_WIDTH_TWIPS, MAX_COL_WIDTH_TWIPS))
                .collect(),
        )
    }

    /// In-bounds, non-default cell styles capped at [`MAX_STYLED_TABLE_CELLS`], sorted by
    /// `((row as u64) << 32) | (col as u64)` and deduplicated (last entry wins).
    pub fn validated_cell_styles(&self) -> Vec<TableCellStyleEntry> {
        if self.cell_styles.is_empty() || self.rows.is_empty() {
            return Vec::new();
        }
        let mut out: Vec<TableCellStyleEntry> = self
            .cell_styles
            .iter()
            .copied()
            .filter(|e| {
                (!e.fill.is_none() || !e.borders.is_unspecified())
                    && usize::try_from(e.row)
                        .ok()
                        .zip(usize::try_from(e.col).ok())
                        .is_some_and(|(r, c)| self.rows.get(r).and_then(|row| row.get(c)).is_some())
            })
            .take(MAX_STYLED_TABLE_CELLS)
            .collect();
        out.sort_by_key(|e| (u64::from(e.row) << 32) | u64::from(e.col));
        let mut write = 0usize;
        for read in 0..out.len() {
            if write > 0
                && out[write - 1].row == out[read].row
                && out[write - 1].col == out[read].col
            {
                out[write - 1] = out[read];
            } else {
                out[write] = out[read];
                write += 1;
            }
        }
        out.truncate(write);
        out
    }

    pub fn validated_cell_paragraphs(&self) -> Vec<TableCellParagraphs> {
        validate_cell_paragraphs(&self.rows, &self.cell_paragraphs)
    }

    /// In-bounds inline cell images, stably sorted by `((row as u64) << 32) | (col as u64)`.
    pub fn validated_cell_images(&self) -> Vec<&TableCellImage> {
        if self.cell_images.is_empty() || self.rows.is_empty() {
            return Vec::new();
        }
        let mut out: Vec<&TableCellImage> = self
            .cell_images
            .iter()
            .filter(|ci| {
                usize::try_from(ci.row)
                    .ok()
                    .zip(usize::try_from(ci.col).ok())
                    .is_some_and(|(r, c)| self.rows.get(r).and_then(|row| row.get(c)).is_some())
            })
            .collect();
        out.sort_by_key(|ci| (u64::from(ci.row) << 32) | u64::from(ci.col));
        out
    }

    /// Width of the widest row.
    pub fn column_count(&self) -> usize {
        self.rows.iter().map(Vec::len).max().unwrap_or(0)
    }

    /// Cell state lookup for writers, or `None` when the table has no valid merged region. The
    /// regions are validated again, so a table edited after construction cannot corrupt output.
    pub fn span_map(&self) -> Option<SpanMap> {
        if self.merges.is_empty() {
            return None;
        }
        let (merges, _) = validate_merges(&self.rows, &self.merges);
        SpanMap::build(self.column_count(), self.rows.len(), &merges)
    }
}

/// Validates a cell hyperlink address: accepts [`crate::safe_link_target`] (`http`, `https`,
/// `mailto`, `#_Toc<N>`) plus `tel:` and `ftp://` within [`MAX_CELL_URL_BYTES`] and free of
/// whitespace/control characters; rejects everything else (`javascript:`, `file:`, `data:`, ...).
pub(crate) fn safe_cell_link_url(raw: &str) -> Option<&str> {
    if let Some(ok) = crate::rich_document::safe_link_target(raw) {
        return Some(ok);
    }
    let url = raw.trim();
    let bad_char = |c: char| c <= ' ' || ('\u{7f}'..='\u{9f}').contains(&c);
    if url.is_empty() || url.len() > MAX_CELL_URL_BYTES || url.chars().any(bad_char) {
        return None;
    }
    let (scheme, rest) = url.split_once(':')?;
    let valid = match scheme.to_ascii_lowercase().as_str() {
        "tel" => !rest.is_empty() && !rest.starts_with(['/', '?']),
        "ftp" => rest
            .strip_prefix("//")
            .is_some_and(|r| !r.is_empty() && !r.starts_with(['/', '?', '#'])),
        _ => false,
    };
    valid.then_some(url)
}

fn validate_links(rows: &[Vec<String>], links: Vec<TableCellLink>) -> (Vec<TableCellLink>, usize) {
    let total = links.len();
    let mut candidates = Vec::with_capacity(total.min(MAX_TABLE_LINKS));
    for mut link in links {
        if candidates.len() >= MAX_TABLE_LINKS {
            break;
        }
        let (Ok(r), Ok(c), Ok(s), Ok(e)) = (
            usize::try_from(link.row),
            usize::try_from(link.col),
            usize::try_from(link.start),
            usize::try_from(link.end),
        ) else {
            continue;
        };
        let Some(cell) = rows.get(r).and_then(|row| row.get(c)) else {
            continue;
        };
        if s >= e || e > cell.len() || !cell.is_char_boundary(s) || !cell.is_char_boundary(e) {
            continue;
        }
        let Some(clean_url) = safe_cell_link_url(&link.url) else {
            continue;
        };
        if clean_url.len() != link.url.len() {
            link.url = clean_url.to_string();
        }
        candidates.push(link);
    }
    candidates.sort_by_key(|l| (l.row, l.col, l.start, l.end));
    let mut kept: Vec<TableCellLink> = Vec::with_capacity(candidates.len());
    for link in candidates {
        if let Some(prev) = kept.last() {
            if prev.row == link.row && prev.col == link.col && link.start < prev.end {
                continue;
            }
        }
        kept.push(link);
    }
    let dropped = total.saturating_sub(kept.len());
    (kept, dropped)
}

/// What a grid position is in a merged table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellSpan {
    Plain,
    /// Top-left cell of a region.
    Anchor {
        row_span: u32,
        col_span: u32,
    },
    /// Another cell of a region, `row_offset`/`col_offset` below and right of its anchor.
    Covered {
        row_offset: u32,
        col_offset: u32,
    },
}

/// Per-cell state of a table, one packed `u32` per cell in row-major order.
#[derive(Debug, Clone)]
pub struct SpanMap {
    cols: usize,
    slots: Vec<u32>,
}

impl SpanMap {
    fn build(cols: usize, rows: usize, merges: &[CellMerge]) -> Option<Self> {
        if merges.is_empty() {
            return None;
        }
        let mut slots = vec![0u32; rows.checked_mul(cols)?];
        for m in merges {
            let (r0, c0) = (m.row as usize, m.col as usize);
            for dr in 0..m.row_span {
                for dc in 0..m.col_span {
                    let at = (r0 + dr as usize) * cols + c0 + dc as usize;
                    slots[at] = if dr == 0 && dc == 0 {
                        m.row_span << SPAN_BITS | m.col_span
                    } else {
                        COVERED_FLAG | dr << SPAN_BITS | dc
                    };
                }
            }
        }
        Some(Self { cols, slots })
    }

    /// State of the cell at `(row, col)`; positions outside the grid are plain.
    pub fn at(&self, row: usize, col: usize) -> CellSpan {
        let slot = if col < self.cols {
            self.slots.get(row * self.cols + col).copied().unwrap_or(0)
        } else {
            0
        };
        let mask = MAX_MERGE_SPAN;
        match slot {
            0 => CellSpan::Plain,
            s if s & COVERED_FLAG != 0 => CellSpan::Covered {
                row_offset: s >> SPAN_BITS & mask,
                col_offset: s & mask,
            },
            s => CellSpan::Anchor {
                row_span: s >> SPAN_BITS & mask,
                col_span: s & mask,
            },
        }
    }
}

/// Row-major bit grid.
pub(super) struct BitGrid {
    words_per_row: usize,
    words: Vec<u64>,
}

impl BitGrid {
    pub(super) fn new(rows: usize, cols: usize) -> Self {
        let words_per_row = cols.div_ceil(WORD_BITS);
        Self {
            words_per_row,
            words: vec![0; rows * words_per_row],
        }
    }

    /// Mask of the bits `lo..hi` inside word `word`.
    fn mask(word: usize, lo: usize, hi: usize) -> u64 {
        let start = lo.max(word * WORD_BITS);
        let end = hi.min((word + 1) * WORD_BITS);
        if start >= end {
            return 0;
        }
        let width = end - start;
        let ones = if width == WORD_BITS {
            u64::MAX
        } else {
            (1u64 << width) - 1
        };
        ones << (start - word * WORD_BITS)
    }

    fn row_words(&self, row: usize, lo: usize, hi: usize) -> std::ops::Range<usize> {
        let base = row * self.words_per_row;
        base + lo / WORD_BITS..base + hi.div_ceil(WORD_BITS)
    }

    pub(super) fn any_set(&self, row: usize, lo: usize, hi: usize) -> bool {
        let base = row * self.words_per_row;
        self.row_words(row, lo, hi)
            .any(|i| self.words[i] & Self::mask(i - base, lo, hi) != 0)
    }

    pub(super) fn set(&mut self, row: usize, lo: usize, hi: usize) {
        let base = row * self.words_per_row;
        for i in self.row_words(row, lo, hi) {
            self.words[i] |= Self::mask(i - base, lo, hi);
        }
    }
}

/// Keeps the regions that fit the grid and do not overlap an earlier kept region. Returns them
/// sorted by position and the number of dropped regions (one-cell regions are ignored silently).
fn validate_merges(rows: &[Vec<String>], merges: &[CellMerge]) -> (Vec<CellMerge>, usize) {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let fits = rows
        .len()
        .checked_mul(cols)
        .is_some_and(|n| n <= MAX_GRID_CELLS);
    let candidates = merges
        .iter()
        .filter(|m| m.row_span > 1 || m.col_span > 1)
        .count();
    if !fits {
        return (Vec::new(), candidates);
    }
    let mut taken = BitGrid::new(rows.len(), cols);
    let mut kept = Vec::new();
    let mut budget = MAX_VALIDATION_WORDS;
    for m in merges.iter().filter(|m| m.row_span > 1 || m.col_span > 1) {
        let Some(region) = region_bounds(rows, m) else {
            continue;
        };
        let words =
            (region.row_end - region.row) * (region.col_end - region.col).div_ceil(WORD_BITS);
        budget = match budget.checked_sub(words.max(1)) {
            Some(left) => left,
            None => break,
        };
        let free =
            (region.row..region.row_end).all(|r| !taken.any_set(r, region.col, region.col_end));
        if !free {
            continue;
        }
        for r in region.row..region.row_end {
            taken.set(r, region.col, region.col_end);
        }
        kept.push(*m);
    }
    kept.sort_by_key(|m| (m.row, m.col));
    let dropped = candidates - kept.len();
    (kept, dropped)
}

struct Bounds {
    row: usize,
    col: usize,
    row_end: usize,
    col_end: usize,
}

/// The region as grid bounds when spans are in range and every spanned row has all its cells.
fn region_bounds(rows: &[Vec<String>], m: &CellMerge) -> Option<Bounds> {
    if m.row_span == 0
        || m.col_span == 0
        || m.row_span > MAX_MERGE_SPAN
        || m.col_span > MAX_MERGE_SPAN
    {
        return None;
    }
    let (row, col) = (usize::try_from(m.row).ok()?, usize::try_from(m.col).ok()?);
    let row_end = row.checked_add(m.row_span as usize)?;
    let col_end = col.checked_add(m.col_span as usize)?;
    let all_cells_exist = rows.get(row..row_end)?.iter().all(|r| r.len() >= col_end);
    all_cells_exist.then_some(Bounds {
        row,
        col,
        row_end,
        col_end,
    })
}

/// Builds the grid and regions of a table row by row, for formats that list covered cells
/// (ODF), pad column spans (OOXML) or continue a region in the next row (OOXML `vMerge`).
/// Spans are never trusted for allocation: padding is limited to [`MAX_PADDED_COLUMNS`].
#[derive(Debug, Default)]
pub(crate) struct TableAssembler {
    rows: Vec<Vec<String>>,
    row: Vec<String>,
    merges: Vec<CellMerge>,
    links: Vec<TableCellLink>,
    col_widths_twips: Vec<u32>,
    cell_styles: Vec<TableCellStyleEntry>,
    cell_images: Vec<TableCellImage>,
    cell_paragraphs: Vec<TableCellParagraphs>,
    /// Region index that a continuing cell in column `c` extends, by column.
    open: std::collections::HashMap<usize, usize>,
    dropped: usize,
}

impl TableAssembler {
    pub fn start_row(&mut self) {
        self.row = Vec::new();
    }

    pub fn end_row(&mut self) {
        self.rows.push(std::mem::take(&mut self.row));
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Cells in the open row.
    pub fn row_len(&self) -> usize {
        self.row.len()
    }

    /// Zero-based `(row, col)` of the cell about to be added.
    pub fn next_coord(&self) -> (u32, u32) {
        (self.rows.len() as u32, self.row.len() as u32)
    }

    pub fn push_cell_style(
        &mut self,
        row: u32,
        col: u32,
        fill: CellFill,
        borders: TableCellBorders,
    ) {
        if (fill.is_none() && borders.is_unspecified())
            || self.cell_styles.len() >= MAX_STYLED_TABLE_CELLS
        {
            return;
        }
        self.cell_styles.push(TableCellStyleEntry {
            row,
            col,
            fill,
            borders,
        });
    }

    pub fn push_cell_images(&mut self, row: u32, col: u32, images: Vec<RichImage>) {
        for image in images {
            self.cell_images.push(TableCellImage { row, col, image });
        }
    }

    pub fn cell_paragraphs_len(&self) -> usize {
        self.cell_paragraphs.len()
    }

    pub fn push_cell_paragraphs(&mut self, row: u32, col: u32, paragraphs: Vec<RichCellParagraph>) {
        if !paragraphs.is_empty()
            && cell_para_has_formatting(&paragraphs)
            && self.cell_paragraphs.len() < MAX_CELL_PARAGRAPH_ENTRIES
        {
            self.cell_paragraphs.push(TableCellParagraphs {
                row,
                col,
                paragraphs,
            });
        }
    }

    /// Appends hyperlink spans `(start, end, url)` for `(row, col)`, bounded by [`MAX_TABLE_LINKS`].
    pub fn push_cell_links(&mut self, row: u32, col: u32, spans: &[(u32, u32, String)]) {
        for (start, end, url) in spans {
            if self.links.len() >= MAX_TABLE_LINKS {
                break;
            }
            self.links.push(TableCellLink {
                row,
                col,
                start: *start,
                end: *end,
                url: url.clone(),
            });
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Pushes a cell and the covered cells of its column span. A span that is too wide is
    /// dropped (the cell is then plain) and counted. Returns the column and the span used.
    fn push_spanning(
        &mut self,
        text: String,
        col_span: u32,
        budget: &mut DecodeBudget,
    ) -> Result<(usize, u32), EasyOfficeError> {
        let col = self.row.len();
        self.open.remove(&col);
        let mut span = col_span.max(1);
        if span > 1 && !self.fits(col, span) {
            self.dropped += 1;
            span = 1;
        }
        let padding_slots = (span as usize).saturating_sub(1);
        if padding_slots > 0 {
            budget.charge(
                padding_slots.saturating_mul(std::mem::size_of::<String>()),
                "표 가로 병합 패딩 슬롯",
            )?;
        }
        self.row.push(text);
        self.row.extend((1..span).map(|_| String::new()));
        Ok((col, span))
    }

    fn fits(&self, col: usize, col_span: u32) -> bool {
        col_span <= MAX_MERGE_SPAN && col.saturating_add(col_span as usize) <= MAX_PADDED_COLUMNS
    }

    fn region(&self, col: usize, row_span: u32, col_span: u32) -> CellMerge {
        CellMerge {
            row: self.rows.len() as u32,
            col: col as u32,
            row_span,
            col_span,
        }
    }

    /// Adds a plain cell.
    pub fn cell(&mut self, text: String, budget: &mut DecodeBudget) -> Result<(), EasyOfficeError> {
        self.push_spanning(text, 1, budget)?;
        Ok(())
    }

    /// Adds a covered cell (no text of its own); regions are declared by their anchor.
    pub fn covered(&mut self, budget: &mut DecodeBudget) -> Result<(), EasyOfficeError> {
        self.cell(String::new(), budget)
    }

    /// Adds the anchor of a region whose covered cells follow as separate cells (ODF). A span
    /// that is too large is dropped (the cell is then plain) and counted.
    pub fn anchor(
        &mut self,
        text: String,
        row_span: u32,
        col_span: u32,
        budget: &mut DecodeBudget,
    ) -> Result<(), EasyOfficeError> {
        let col = self.row.len();
        self.cell(text, budget)?;
        if row_span <= 1 && col_span <= 1 {
            return Ok(());
        }
        if row_span > MAX_MERGE_SPAN || !self.fits(col, col_span) {
            self.dropped += 1;
            return Ok(());
        }
        self.merges
            .push(self.region(col, row_span.max(1), col_span.max(1)));
        Ok(())
    }

    /// Adds a cell spanning `col_span` columns followed by its covered cells (OOXML `gridSpan`).
    pub fn spanning(
        &mut self,
        text: String,
        col_span: u32,
        budget: &mut DecodeBudget,
    ) -> Result<(), EasyOfficeError> {
        let (col, span) = self.push_spanning(text, col_span, budget)?;
        if span > 1 {
            self.merges.push(self.region(col, 1, span));
        }
        Ok(())
    }

    /// Adds the first cell of a region that following rows may extend with
    /// [`Self::continue_above`] (OOXML `vMerge="restart"`). Never extended, it is a plain cell.
    pub fn restart(
        &mut self,
        text: String,
        col_span: u32,
        budget: &mut DecodeBudget,
    ) -> Result<(), EasyOfficeError> {
        let (col, span) = self.push_spanning(text, col_span, budget)?;
        self.merges.push(self.region(col, 1, span));
        self.open.insert(col, self.merges.len() - 1);
        Ok(())
    }

    /// Adds a cell that continues the region directly above in the same column (OOXML
    /// `vMerge`), padded to `col_span` cells. Without a matching region above it is a plain
    /// cell.
    pub fn continue_above(
        &mut self,
        text: String,
        col_span: u32,
        budget: &mut DecodeBudget,
    ) -> Result<(), EasyOfficeError> {
        let col = self.row.len();
        let row = self.rows.len() as u32;
        let col_span = col_span.max(1);
        let extend = self.open.get(&col).copied().filter(|&i| {
            let m = &self.merges[i];
            m.row + m.row_span == row && m.col_span == col_span && m.row_span < MAX_MERGE_SPAN
        });
        match extend {
            Some(i) => {
                self.merges[i].row_span += 1;
                let padding_slots = (col_span as usize).saturating_sub(1);
                if padding_slots > 0 {
                    budget.charge(
                        padding_slots.saturating_mul(std::mem::size_of::<String>()),
                        "표 세로 병합 가로 패딩 슬롯",
                    )?;
                }
                self.row.push(text);
                self.row.extend((1..col_span).map(|_| String::new()));
                Ok(())
            }
            None => self.spanning(text, col_span, budget),
        }
    }

    /// The table, and the number of regions that were dropped while building or validating.
    pub fn finish(self, budget: &mut DecodeBudget) -> Result<(RichTable, usize), EasyOfficeError> {
        let header_cost = self
            .rows
            .len()
            .saturating_mul(std::mem::size_of::<Vec<String>>());
        budget.charge(header_cost, "표 행 헤더 비용")?;
        let (mut table, dropped, _) =
            RichTable::with_merges_and_links(self.rows, self.merges, self.links);
        table.col_widths_twips = self.col_widths_twips;
        table.col_widths_twips = table.validated_col_widths().unwrap_or_default();
        table.cell_styles = self.cell_styles;
        table.cell_styles = table.validated_cell_styles();
        table.cell_images = self.cell_images;
        table.cell_images.retain(|ci| {
            usize::try_from(ci.row)
                .ok()
                .zip(usize::try_from(ci.col).ok())
                .is_some_and(|(r, c)| table.rows.get(r).and_then(|row| row.get(c)).is_some())
        });
        table
            .cell_images
            .sort_by_key(|ci| (u64::from(ci.row) << 32) | u64::from(ci.col));
        table.cell_paragraphs = self.cell_paragraphs;
        table.cell_paragraphs = table.validated_cell_paragraphs();
        Ok((table, dropped + self.dropped))
    }
}

/// Joins the paragraphs of one table cell with `\n` (dropping trailing empty paragraphs) and
/// extracts merged UTF-8 byte hyperlink spans `(start, end, url)` in a single pass.
pub(crate) fn join_cell_paragraphs(
    paras: &[Vec<crate::RichRun>],
) -> (String, Vec<(u32, u32, String)>) {
    let mut end_idx = paras.len();
    while end_idx > 0 && paras[end_idx - 1].iter().all(|r| r.text.is_empty()) {
        end_idx -= 1;
    }
    let active = &paras[..end_idx];
    if active.is_empty() {
        return (String::new(), Vec::new());
    }
    let total_bytes: usize = active
        .iter()
        .map(|p| p.iter().map(|r| r.text.len()).sum::<usize>())
        .sum::<usize>()
        + active.len().saturating_sub(1);
    let mut text = String::with_capacity(total_bytes);
    let mut spans: Vec<(u32, u32, String)> = Vec::new();
    for (p_idx, runs) in active.iter().enumerate() {
        if p_idx > 0 {
            text.push('\n');
        }
        for run in runs {
            if run.text.is_empty() {
                continue;
            }
            let start = text.len();
            text.push_str(&run.text);
            let end = text.len();
            if let (Some(url), Ok(s), Ok(e)) = (&run.link, u32::try_from(start), u32::try_from(end))
            {
                if let Some(last) = spans.last_mut().filter(|(_, le, lu)| *le == s && lu == url) {
                    last.1 = e;
                } else {
                    spans.push((s, e, url.clone()));
                }
            }
        }
    }
    (text, spans)
}

#[path = "rich_table_placed.rs"]
mod placed;
pub(crate) use placed::*;

#[cfg(test)]
#[path = "rich_table_tests.rs"]
mod tests;
