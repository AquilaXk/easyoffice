//! Merged ranges a sheet has, for the arrays that spill (PD-222): an array cannot be placed where it
//! would cross a merged range, as it cannot be typed into one (`#SPILL!`, the cause the spreadsheet
//! program lists as "spill into merged cells").
//!
//! 참조 구현: IronCalc (MIT OR Apache-2.0), Copyright (c) IronCalc contributors, `base/src/model.rs`
//! `spill_dynamic_array` (commit 01b39c1a: a cell of the area inside a merged range blocks the array).
//! Changes: the reference asks the worksheet "is this cell merged" for every cell of the area (one
//! lookup per cell); here the merges are kept sorted by first row with the tallest height, so one
//! binary search and a short scan answer "does any merge touch this rectangle" for the whole area, and
//! the merge of the anchor itself counts (the reference skips the anchor's cell). Source and license
//! text: `THIRD_PARTY_NOTICES.md`.

use crate::axis::{shift_boxes, AxisEdit};
use easyoffice_formats::CellBox;

/// The merged ranges of one sheet, sorted by first row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpillMerges {
    sorted: Vec<CellBox>,
    /// The most rows any merge has: a merge that starts before a rectangle can still reach it by this many.
    tallest: u32,
}

impl SpillMerges {
    pub(crate) fn new(mut boxes: Vec<CellBox>) -> Self {
        boxes.sort_unstable_by_key(|b| (b.row0, b.col0));
        let tallest = boxes.iter().map(|b| b.row1 - b.row0 + 1).max().unwrap_or(0);
        Self {
            sorted: boxes,
            tallest,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.sorted.is_empty()
    }

    /// The merges after the rows or columns of the sheet move for `edit`.
    pub(crate) fn shifted(&self, edit: &AxisEdit) -> Self {
        Self::new(shift_boxes(&self.sorted, edit, true))
    }

    /// Whether a merged range shares a cell with the rectangle `(row0, col0, row1, col1)`.
    pub(crate) fn touches(&self, (row0, col0, row1, col1): (u32, u32, u32, u32)) -> bool {
        let from = self
            .sorted
            .partition_point(|b| b.row0 < row0.saturating_sub(self.tallest.saturating_sub(1)));
        self.sorted[from..]
            .iter()
            .take_while(|b| b.row0 <= row1)
            .any(|b| b.row1 >= row0 && b.col0 <= col1 && b.col1 >= col0)
    }
}
