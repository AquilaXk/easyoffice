//! What an anchor of a picture or a chart does when the rows and columns under it change (`editAs` of a DrawingML two-cell
//! anchor, PD-299): moves and sizes with the cells (the default), moves with them and keeps its size, or stays where it is.
//! `oneCellAnchor` and `absoluteAnchor` files read as the second and the third kind.

use crate::sheet_styling::sheet_chart::{emu_to_px_rounded, ChartAnchor};
use crate::sheet_styling::sheet_row_offsets::SheetGrid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnchorEdit {
    #[default]
    TwoCell,
    OneCell,
    Absolute,
}

impl AnchorEdit {
    /// The value of an `editAs` attribute. A file is input, so anything but the three values is the default.
    pub fn parse(text: &str) -> Self {
        match text.trim() {
            "oneCell" => Self::OneCell,
            "absolute" => Self::Absolute,
            _ => Self::TwoCell,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TwoCell => "twoCell",
            Self::OneCell => "oneCell",
            Self::Absolute => "absolute",
        }
    }
}

/// `(from_col, from_row, col_off, row_off, to_col, to_row, to_col_off, to_row_off)`, the offsets in EMU.
pub type AnchorCells = (u32, u32, u32, u32, u32, u32, u32, u32);

impl ChartAnchor {
    /// The cells of the anchor.
    pub(crate) fn cells(&self) -> AnchorCells {
        (
            self.from_col,
            self.from_row,
            self.col_off_emu,
            self.row_off_emu,
            self.to_col,
            self.to_row,
            self.to_col_off_emu,
            self.to_row_off_emu,
        )
    }

    /// The top left corner of the anchor's start cell and its offset, in px.
    fn cell_origin_px(&self, grid: &SheetGrid<'_>) -> (u32, u32) {
        let x = grid
            .col_left(self.from_col)
            .saturating_add(emu_to_px_rounded(self.col_off_emu));
        let y = grid
            .rows()
            .top(self.from_row)
            .saturating_add(emu_to_px_rounded(self.row_off_emu));
        let clamp = |v: u64| v.min(u64::from(u32::MAX)) as u32;
        (clamp(x), clamp(y))
    }

    /// The corners of the anchor in px, `(left, top, right, bottom)`, from its cells on `grid`.
    fn corners_px(&self, grid: &SheetGrid<'_>) -> [u64; 4] {
        let x = |col: u32, off: u32| grid.col_left(col).saturating_add(emu_to_px_rounded(off));
        let y = |row: u32, off: u32| grid.rows().top(row).saturating_add(emu_to_px_rounded(off));
        [
            x(self.from_col, self.col_off_emu),
            y(self.from_row, self.row_off_emu),
            x(self.to_col, self.to_col_off_emu),
            y(self.to_row, self.to_row_off_emu),
        ]
    }

    /// Whether the anchor's cells already put its corners where `placed` does, to a pixel, on `grid`.
    fn lies_where(&self, placed: &Self, grid: &SheetGrid<'_>) -> bool {
        let (a, b) = (self.corners_px(grid), placed.corners_px(grid));
        a.iter().zip(b).all(|(p, q)| p.abs_diff(q) <= 1)
    }

    /// Makes the cells say what the anchor means on the rows and columns of `grid`: a two-cell anchor says its own cells
    /// (its size follows them), a one-cell anchor starts on its start cell and keeps its pixel size, an absolute anchor
    /// keeps its pixel position and size. An anchor with no cells of its own has its pixels read.
    /// `grid_from_px` is the version of `sync_grid_from_px` of the kind of anchor (chart or picture).
    ///
    /// The cells of a one-cell or absolute anchor that has some are kept as they are, to the EMU, when they already
    /// describe that size and place, on the rows as they are or with the hidden rows shown (the cells a file gives for a
    /// picture over hidden rows, which are the rows' height when they were shown or hidden). Only a change of the rows, the
    /// columns, the place or the size makes them be worked out again.
    pub(crate) fn settle(
        &mut self,
        grid: &SheetGrid<'_>,
        grid_from_px: impl Fn(&mut Self, &SheetGrid<'_>),
    ) {
        let kept = *self;
        match (self.edit_as, self.has_explicit_grid()) {
            (AnchorEdit::TwoCell, true) => return,
            (_, false) => return grid_from_px(self, grid),
            (_, true) => {}
        }
        for rows in [grid, grid.natural()] {
            *self = kept;
            match kept.edit_as {
                AnchorEdit::OneCell => self.start_on_cell(rows, &grid_from_px),
                _ => grid_from_px(self, rows),
            }
            if kept.lies_where(self, rows) {
                *self = kept;
                return;
            }
        }
        *self = kept;
        match kept.edit_as {
            AnchorEdit::OneCell => self.start_on_cell(grid, grid_from_px),
            _ => grid_from_px(self, grid),
        }
    }

    /// Puts the anchor on its start cell as `grid` has it and ends it a pixel size further (the end cells and the pixel
    /// position are worked out from that).
    pub fn start_on_cell(
        &mut self,
        grid: &SheetGrid<'_>,
        grid_from_px: impl Fn(&mut Self, &SheetGrid<'_>),
    ) {
        let start = (
            self.from_col,
            self.from_row,
            self.col_off_emu,
            self.row_off_emu,
        );
        (self.left_px, self.top_px) = self.cell_origin_px(grid);
        grid_from_px(self, grid);
        (
            self.from_col,
            self.from_row,
            self.col_off_emu,
            self.row_off_emu,
        ) = start;
    }
}
