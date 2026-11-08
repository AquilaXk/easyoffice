//! Table block of the rich model: a text grid plus the merged regions drawn over it.
//!
//! The grid keeps one string per cell. A merged region is anchored at its top-left cell, which
//! holds the text; the other cells of the region stay in the grid (normally empty) so every
//! consumer that only looks at text keeps working. Regions are validated against the grid:
//! one-cell regions, spans past the grid, spans above [`MAX_MERGE_SPAN`] and regions that overlap
//! an earlier region are dropped and counted. Overlap checks use a row-major bit grid, so the
//! work is proportional to the number of cells, and a work budget stops hostile inputs that
//! declare many large regions.

/// Largest row or column span of one region (15 bits, so a cell state fits in a `u32`).
pub const MAX_MERGE_SPAN: u32 = 0x7FFF;
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

/// A table: rows of final cell text and the merged regions over them (sorted by position once
/// built through [`RichTable::with_merges`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RichTable {
    pub rows: Vec<Vec<String>>,
    pub merges: Vec<CellMerge>,
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
        }
    }

    /// Builds a table from cell text and candidate regions. Returns the table with the valid
    /// regions and the number of regions that were dropped.
    pub fn with_merges(rows: Vec<Vec<String>>, merges: Vec<CellMerge>) -> (Self, usize) {
        let (merges, dropped) = validate_merges(&rows, &merges);
        (Self { rows, merges }, dropped)
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
struct BitGrid {
    words_per_row: usize,
    words: Vec<u64>,
}

impl BitGrid {
    fn new(rows: usize, cols: usize) -> Self {
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

    fn any_set(&self, row: usize, lo: usize, hi: usize) -> bool {
        let base = row * self.words_per_row;
        self.row_words(row, lo, hi)
            .any(|i| self.words[i] & Self::mask(i - base, lo, hi) != 0)
    }

    fn set(&mut self, row: usize, lo: usize, hi: usize) {
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

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Pushes a cell and the covered cells of its column span. A span that is too wide is
    /// dropped (the cell is then plain) and counted. Returns the column and the span used.
    fn push_spanning(&mut self, text: String, col_span: u32) -> (usize, u32) {
        let col = self.row.len();
        self.open.remove(&col);
        let mut span = col_span.max(1);
        if span > 1 && !self.fits(col, span) {
            self.dropped += 1;
            span = 1;
        }
        self.row.push(text);
        self.row.extend((1..span).map(|_| String::new()));
        (col, span)
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
    pub fn cell(&mut self, text: String) {
        self.push_spanning(text, 1);
    }

    /// Adds a covered cell (no text of its own); regions are declared by their anchor.
    pub fn covered(&mut self) {
        self.cell(String::new());
    }

    /// Adds the anchor of a region whose covered cells follow as separate cells (ODF). A span
    /// that is too large is dropped (the cell is then plain) and counted.
    pub fn anchor(&mut self, text: String, row_span: u32, col_span: u32) {
        let col = self.row.len();
        self.cell(text);
        if row_span <= 1 && col_span <= 1 {
            return;
        }
        if row_span > MAX_MERGE_SPAN || !self.fits(col, col_span) {
            self.dropped += 1;
            return;
        }
        self.merges
            .push(self.region(col, row_span.max(1), col_span.max(1)));
    }

    /// Adds a cell spanning `col_span` columns followed by its covered cells (OOXML `gridSpan`).
    pub fn spanning(&mut self, text: String, col_span: u32) {
        let (col, span) = self.push_spanning(text, col_span);
        if span > 1 {
            self.merges.push(self.region(col, 1, span));
        }
    }

    /// Adds the first cell of a region that following rows may extend with
    /// [`Self::continue_above`] (OOXML `vMerge="restart"`). Never extended, it is a plain cell.
    pub fn restart(&mut self, text: String, col_span: u32) {
        let (col, span) = self.push_spanning(text, col_span);
        self.merges.push(self.region(col, 1, span));
        self.open.insert(col, self.merges.len() - 1);
    }

    /// Adds a cell that continues the region directly above in the same column (OOXML
    /// `vMerge`), padded to `col_span` cells. Without a matching region above it is a plain
    /// cell.
    pub fn continue_above(&mut self, text: String, col_span: u32) {
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
                self.row.push(text);
                self.row.extend((1..col_span).map(|_| String::new()));
            }
            None => self.spanning(text, col_span),
        }
    }

    /// The table, and the number of regions that were dropped while building or validating.
    pub fn finish(self) -> (RichTable, usize) {
        let (table, dropped) = RichTable::with_merges(self.rows, self.merges);
        (table, dropped + self.dropped)
    }
}

/// A cell at an explicit grid address (HWPX `cellAddr`/`cellSpan`).
#[derive(Debug)]
pub(crate) struct PlacedCell {
    pub row: u32,
    pub col: u32,
    pub row_span: u32,
    pub col_span: u32,
    pub text: String,
}

/// Lays addressed cells out as a grid. Cells at a taken address and cells beyond the size limits
/// are dropped and counted; a span that does not fit is reduced to one cell. The grid is as large
/// as the farthest accepted cell or region.
pub(crate) fn table_from_placed(cells: Vec<PlacedCell>) -> (RichTable, usize) {
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
    let mut grid = vec![vec![String::new(); cols]; rows];
    let mut anchored = BitGrid::new(rows, cols);
    let mut merges = Vec::new();
    for cell in accepted {
        let (r, c) = (cell.row as usize, cell.col as usize);
        if anchored.any_set(r, c, c + 1) {
            dropped += 1;
            continue;
        }
        anchored.set(r, c, c + 1);
        grid[r][c] = cell.text;
        merges.push(CellMerge {
            row: cell.row,
            col: cell.col,
            row_span: cell.row_span,
            col_span: cell.col_span,
        });
    }
    let (table, rejected) = RichTable::with_merges(grid, merges);
    (table, dropped + rejected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(rows: usize, cols: usize) -> Vec<Vec<String>> {
        vec![vec![String::new(); cols]; rows]
    }

    fn merge(row: u32, col: u32, row_span: u32, col_span: u32) -> CellMerge {
        CellMerge {
            row,
            col,
            row_span,
            col_span,
        }
    }

    #[test]
    fn valid_regions_are_kept_sorted() {
        let (t, dropped) =
            RichTable::with_merges(grid(4, 4), vec![merge(2, 2, 2, 2), merge(0, 0, 1, 3)]);
        assert_eq!(dropped, 0);
        assert_eq!(t.merges, vec![merge(0, 0, 1, 3), merge(2, 2, 2, 2)]);
    }

    #[test]
    fn overlapping_out_of_range_and_huge_regions_are_dropped() {
        let (t, dropped) = RichTable::with_merges(
            grid(3, 3),
            vec![
                merge(0, 0, 2, 2),
                merge(1, 1, 2, 2),
                merge(2, 2, 2, 1),
                merge(0, 2, 1, 2),
                merge(0, 0, u32::MAX, u32::MAX),
                merge(u32::MAX, u32::MAX, 2, 2),
                merge(1, 0, 0, 2),
                merge(2, 0, 1, 1),
            ],
        );
        assert_eq!(t.merges, vec![merge(0, 0, 2, 2)]);
        assert_eq!(dropped, 6);
    }

    #[test]
    fn ragged_rows_reject_regions_over_missing_cells() {
        let rows = vec![vec!["a".into(), "b".into()], vec!["c".into()]];
        let (t, dropped) = RichTable::with_merges(rows, vec![merge(0, 0, 2, 2), merge(0, 0, 1, 2)]);
        assert_eq!(t.merges, vec![merge(0, 0, 1, 2)]);
        assert_eq!(dropped, 1);
    }

    #[test]
    fn grid_over_the_limit_drops_every_region() {
        let (t, dropped) = RichTable::with_merges(grid(2_001, 2_001), vec![merge(0, 0, 2, 2)]);
        assert!(t.merges.is_empty());
        assert_eq!(dropped, 1);
    }

    #[test]
    fn many_large_overlapping_regions_stay_within_the_work_budget() {
        let rows = 2_000usize;
        let candidates = vec![merge(0, 0, 2_000, 1_000); 200_000];
        let started = std::time::Instant::now();
        let (t, dropped) = RichTable::with_merges(grid(rows, 1_000), candidates);
        assert_eq!(t.merges.len(), 1);
        assert_eq!(dropped, 199_999);
        assert!(started.elapsed().as_secs() < 5);
    }

    #[test]
    fn span_map_marks_anchor_and_covered_cells() {
        let (t, _) = RichTable::with_merges(grid(3, 3), vec![merge(0, 1, 2, 2)]);
        let map = t.span_map().expect("merged");
        assert_eq!(map.at(0, 0), CellSpan::Plain);
        assert_eq!(
            map.at(0, 1),
            CellSpan::Anchor {
                row_span: 2,
                col_span: 2
            }
        );
        assert_eq!(
            map.at(1, 2),
            CellSpan::Covered {
                row_offset: 1,
                col_offset: 1
            }
        );
        assert_eq!(map.at(2, 2), CellSpan::Plain);
        assert_eq!(map.at(9, 9), CellSpan::Plain);
        assert!(RichTable::new(grid(2, 2)).span_map().is_none());
    }

    #[test]
    fn stale_regions_are_revalidated_by_span_map() {
        let mut t = RichTable::with_merges(grid(3, 3), vec![merge(1, 1, 2, 2)]).0;
        t.rows.truncate(2);
        assert!(t.span_map().is_none());
    }

    #[test]
    fn assembler_pads_column_spans_and_continues_vertical_regions() {
        let mut a = TableAssembler::default();
        a.start_row();
        a.spanning("가".into(), 2);
        a.restart("나".into(), 1);
        a.end_row();
        a.start_row();
        a.cell("x".into());
        a.cell("y".into());
        a.continue_above(String::new(), 1);
        a.end_row();
        let (t, dropped) = a.finish();
        assert_eq!(dropped, 0);
        assert_eq!(t.rows[0].len(), 3);
        assert_eq!(t.merges, vec![merge(0, 0, 1, 2), merge(0, 2, 2, 1)]);
    }

    #[test]
    fn assembler_ignores_orphan_continuations_and_huge_spans() {
        let mut a = TableAssembler::default();
        a.start_row();
        a.continue_above(String::new(), 1);
        a.spanning("a".into(), u32::MAX);
        a.anchor("b".into(), u32::MAX, 1);
        a.end_row();
        let (t, dropped) = a.finish();
        assert_eq!(t.rows[0].len(), 3);
        assert!(t.merges.is_empty());
        assert_eq!(dropped, 2);
    }

    #[test]
    fn placed_cells_make_a_grid_and_drop_duplicates_and_far_cells() {
        let cell = |row, col, row_span, col_span, text: &str| PlacedCell {
            row,
            col,
            row_span,
            col_span,
            text: text.to_string(),
        };
        let (t, dropped) = table_from_placed(vec![
            cell(0, 0, 1, 2, "a"),
            cell(0, 2, 2, 1, "b"),
            cell(1, 0, 1, 1, "c"),
            cell(1, 0, 1, 1, "dup"),
            cell(0, u32::MAX, 1, 1, "far"),
            cell(9_999_999, 0, 1, 1, "tall"),
        ]);
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.rows[0], vec!["a", "", "b"]);
        assert_eq!(t.rows[1][0], "c");
        assert_eq!(t.merges, vec![merge(0, 0, 1, 2), merge(0, 2, 2, 1)]);
        assert_eq!(dropped, 3);
    }
}
