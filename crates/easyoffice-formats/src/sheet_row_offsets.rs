//! Where a row starts on the page, in CSS pixels (PD-299): the row geometry that pictures and charts use to turn a
//! two-cell anchor into pixels and pixels into an anchor, in place of a fixed 24 px row.
//!
//! The height of a row is the one rule of `SheetStyling::row_height_px` (its own height, else the default row height a
//! user set, else the grid's 24 px) with hidden rows at 0, the rule the page and the PDF (`pdf_laid::row_height_pt`)
//! share. The page lays rows out at whole pixels (`sheet-row-height.js` `ptToPx`), so each height is rounded the same way
//! here. Only the rows that differ from the base height are kept, as sorted bands with a running total, so a sheet
//! with a million hidden rows costs one band and a position is one binary search.
//!
//! No reference code: the running sum of row heights that spreadsheet grids position objects with is kept per row in the
//! grids this project looked at, which does not fit a sheet of a million rows. The sparse bands, the hidden runs and the
//! inverse lookup are written here, after the column version in `sheet_chart.rs` (`col_left_px`).

use std::cell::OnceCell;
use std::collections::BTreeMap;

use crate::sheet_styling::sheet_chart::{col_left_px, px_to_col_and_emu};
use crate::sheet_styling::sheet_default_col_width::DEFAULT_COL_PX;
use crate::sheet_styling::sheet_row_height::GRID_ROW_PX;
use crate::sheet_styling::xlsx_layout::{normalize_runs, run_contains};
use crate::{OoxmlCodec, SheetStyling};

/// A run of rows `first..=last` that all have `px` height, which is not the base height, and the height the bands
/// before it add up to over the base (`before`, in px).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Band {
    first: u32,
    last: u32,
    px: u32,
    before: i64,
}

/// The top edge of every row of one sheet, in CSS pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowOffsets {
    base: u32,
    bands: Vec<Band>,
}

/// A height in whole pixels, at least 1: the page rounds each row to a pixel and a row that is not hidden is never flat.
fn whole_px(px: f64) -> u32 {
    px.round().max(1.0) as u32
}

impl Default for RowOffsets {
    /// Every row at the grid's 24 px.
    fn default() -> Self {
        Self {
            base: whole_px(GRID_ROW_PX),
            bands: Vec::new(),
        }
    }
}

impl RowOffsets {
    /// The rows of `styling`: their own heights, the default row height and the hidden rows (the rows a user hid, a
    /// folded group hid and the filters hid).
    pub fn of(styling: &SheetStyling) -> Self {
        Self::build(styling, true)
    }

    /// The rows of `styling` with the hidden rows at their height: where a picture that was placed on the cells with the rows
    /// shown lies.
    pub fn natural(styling: &SheetStyling) -> Self {
        Self::build(styling, false)
    }

    fn build(styling: &SheetStyling, with_hidden: bool) -> Self {
        let base = whole_px(styling.base_row_px(GRID_ROW_PX));
        let mut hidden = Vec::new();
        if with_hidden {
            let (user, filtered) = styling.hidden_row_runs();
            hidden = user;
            hidden.extend(filtered);
            normalize_runs(&mut hidden);
        }
        let mut spans: Vec<(u32, u32, u32)> = hidden.iter().map(|&(a, b)| (a, b, 0)).collect();
        for &row in styling.row_heights.keys() {
            let px = whole_px(styling.row_height_px(row, GRID_ROW_PX));
            if px != base && !run_contains(&hidden, row) {
                spans.push((row, row, px));
            }
        }
        spans.sort_unstable();
        let mut bands = Vec::with_capacity(spans.len());
        let mut before = 0i64;
        for (first, last, px) in spans {
            bands.push(Band {
                first,
                last,
                px,
                before,
            });
            before += (i64::from(px) - i64::from(base)) * (i64::from(last) - i64::from(first) + 1);
        }
        Self { base, bands }
    }

    /// The top edge of the zero-based `row`.
    pub fn top(&self, row: u32) -> u64 {
        let plain = i64::from(row) * i64::from(self.base);
        let extra = match self.bands.partition_point(|b| b.first < row).checked_sub(1) {
            None => 0,
            Some(i) => {
                let band = &self.bands[i];
                let rows = i64::from(band.last.min(row - 1)) - i64::from(band.first) + 1;
                band.before + (i64::from(band.px) - i64::from(self.base)) * rows
            }
        };
        (plain + extra).max(0) as u64
    }

    /// The height of the zero-based `row`.
    pub fn height(&self, row: u32) -> u64 {
        let in_band = self
            .bands
            .partition_point(|b| b.first <= row)
            .checked_sub(1)
            .map(|i| &self.bands[i])
            .filter(|b| b.last >= row);
        u64::from(in_band.map_or(self.base, |b| b.px))
    }

    /// The row that holds the vertical offset `target_px` and the pixels of that row above it: the last row that
    /// starts at or above it, so a hidden row, which starts where the next row does, is passed over. An offset below
    /// the last row is the last row's start.
    pub fn locate(&self, target_px: u64) -> (u32, u64) {
        let last = OoxmlCodec::MAX_XLSX_ROWS - 1;
        let (mut lo, mut hi) = (0u32, last);
        while lo < hi {
            let mid = lo + (hi - lo).div_ceil(2);
            if self.top(mid) <= target_px {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        let off = target_px - self.top(lo);
        (lo, if off < self.height(lo) { off } else { 0 })
    }
}

static NO_COLUMNS: BTreeMap<u32, u32> = BTreeMap::new();

/// The column widths and row heights of one sheet, which place a picture or a chart. The rows are worked out the first
/// time a position needs them, so a caller that only has anchors with cells of their own (the usual case) pays nothing.
pub struct SheetGrid<'a> {
    pub columns: &'a BTreeMap<u32, u32>,
    /// The width of the columns that are not in `columns` (`SheetStyling::default_col_px`, PD-305).
    pub default_col_px: u32,
    styling: Option<&'a SheetStyling>,
    rows: OnceCell<RowOffsets>,
    natural: OnceCell<Box<SheetGrid<'a>>>,
}

impl<'a> SheetGrid<'a> {
    /// The grid of `styling`.
    pub fn of(styling: &'a SheetStyling) -> Self {
        Self {
            columns: &styling.columns,
            default_col_px: styling.default_col_px(),
            styling: Some(styling),
            rows: OnceCell::new(),
            natural: OnceCell::new(),
        }
    }

    /// A grid made of its parts, for a caller that does not have the whole sheet at hand (a reader that is still
    /// filling the sheet, a request that carries the layout of its own).
    pub fn new(columns: &'a BTreeMap<u32, u32>, default_col_px: u32, rows: RowOffsets) -> Self {
        Self {
            columns,
            default_col_px,
            styling: None,
            rows: OnceCell::from(rows),
            natural: OnceCell::new(),
        }
    }

    /// The same columns with the hidden rows at their height. A grid that was made of parts has no sheet to tell which
    /// rows are hidden and is its own natural grid.
    pub fn natural(&self) -> &SheetGrid<'a> {
        let Some(styling) = self.styling else {
            return self;
        };
        self.natural.get_or_init(|| {
            Box::new(SheetGrid::new(
                self.columns,
                self.default_col_px,
                RowOffsets::natural(styling),
            ))
        })
    }

    /// A sheet with the default column width and the grid's row height everywhere.
    pub fn plain<'b>() -> SheetGrid<'b> {
        SheetGrid::new(&NO_COLUMNS, DEFAULT_COL_PX, RowOffsets::default())
    }

    /// The x-coordinate in pixels of the left edge of the zero-based `col`.
    pub fn col_left(&self, col: u32) -> u64 {
        col_left_px(col, self.columns, self.default_col_px)
    }

    /// The column at the horizontal offset `target_px` and the EMU the offset lies right of the column's left edge.
    pub(crate) fn col_at(&self, target_px: u64) -> (u32, u32) {
        px_to_col_and_emu(target_px, self.columns, self.default_col_px)
    }

    pub fn rows(&self) -> &RowOffsets {
        self.rows
            .get_or_init(|| self.styling.map(RowOffsets::of).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RowHeight;

    fn styling(
        heights: &[(u32, f64)],
        default: Option<f64>,
        hidden: &[(u32, u32)],
    ) -> SheetStyling {
        let mut st = SheetStyling::default();
        for &(row, pt) in heights {
            st.row_heights
                .insert(row, RowHeight::from_points(pt, true).unwrap());
        }
        st.default_row_height = default.and_then(|pt| RowHeight::from_points(pt, true));
        st.hidden_rows = hidden.to_vec();
        st
    }

    #[test]
    fn plain_rows_are_22_px() {
        let rows = RowOffsets::of(&SheetStyling::default());
        assert_eq!((rows.top(0), rows.top(1), rows.top(10)), (0, 22, 220));
        assert_eq!(rows.locate(46), (2, 2));
        assert_eq!(RowOffsets::default(), rows);
    }

    #[test]
    fn own_heights_default_height_and_hidden_rows_move_the_rows_below() {
        // 15 pt = 20 px default; row 1 is 30 pt = 40 px; row 3 is hidden; row 4 is 60 pt = 80 px.
        let rows = RowOffsets::of(&styling(&[(1, 30.0), (4, 60.0)], Some(15.0), &[(3, 3)]));
        let tops: Vec<u64> = (0..7).map(|r| rows.top(r)).collect();
        assert_eq!(tops, [0, 20, 60, 80, 80, 160, 180]);
        assert_eq!(rows.height(3), 0);
        assert_eq!(rows.height(4), 80);
        // The top of the hidden row 3 is the top of row 4, so a position there is row 4.
        assert_eq!(rows.locate(80), (4, 0));
        assert_eq!(rows.locate(79), (2, 19));
        assert_eq!(rows.locate(165), (5, 5));
    }

    #[test]
    fn a_million_hidden_rows_are_one_band() {
        let rows = RowOffsets::of(&styling(&[], None, &[(10, 1_000_000)]));
        assert_eq!(rows.bands.len(), 1);
        assert_eq!(rows.top(10), 220);
        assert_eq!(rows.top(1_000_001), 220);
        assert_eq!(rows.top(1_000_002), 242);
        assert_eq!(rows.locate(220), (1_000_001, 0));
    }

    #[test]
    fn a_position_below_the_last_row_is_the_last_row() {
        let rows = RowOffsets::default();
        let last = OoxmlCodec::MAX_XLSX_ROWS - 1;
        assert_eq!(rows.locate(u64::MAX / 4), (last, 0));
    }

    #[test]
    fn the_pdf_and_the_anchors_read_one_height_per_row() {
        use crate::pdf_laid::{row_height_pt, PDF_PT_PER_PX};
        // Whole-pixel heights (a multiple of 0.75 pt), so rounding does not part the two.
        let st = styling(
            &[(1, 30.0), (4, 60.0), (6, 3.0)],
            Some(15.0),
            &[(3, 3), (8, 9)],
        );
        let rows = RowOffsets::of(&st);
        for row in 0..12 {
            let pt = row_height_pt(&st, row, 1.0);
            assert_eq!(pt, rows.height(row) as f64 * PDF_PT_PER_PX, "row {row}");
        }
    }

    #[test]
    fn every_row_round_trips_through_its_top() {
        let rows = RowOffsets::of(&styling(
            &[(0, 12.0), (2, 90.5), (7, 7.5), (8, 400.0)],
            Some(20.0),
            &[(4, 6), (9, 9)],
        ));
        for row in 0..14 {
            if rows.height(row) == 0 {
                continue;
            }
            assert_eq!(rows.locate(rows.top(row)), (row, 0), "row {row}");
            let mid = rows.height(row) / 2;
            assert_eq!(rows.locate(rows.top(row) + mid), (row, mid), "row {row}");
        }
    }
}
