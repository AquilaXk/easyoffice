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
use crate::recalc::IntervalIndex;
use easyoffice_formats::CellBox;

/// The merged ranges of one sheet: sorted by first row, with the row intervals in an index.
#[derive(Debug, Clone, Default)]
pub struct SpillMerges {
    sorted: Vec<CellBox>,
    /// The row intervals of `sorted` (the id is the position), for the merges that start above a rectangle and reach it.
    rows: IntervalIndex,
}

impl PartialEq for SpillMerges {
    fn eq(&self, other: &Self) -> bool {
        self.sorted == other.sorted
    }
}

impl Eq for SpillMerges {}

impl SpillMerges {
    pub(crate) fn new(mut boxes: Vec<CellBox>) -> Self {
        boxes.sort_unstable_by_key(|b| (b.row0, b.col0));
        let mut rows = IntervalIndex::default();
        for (id, b) in boxes.iter().enumerate() {
            rows.insert(b.row0, b.row1, id as u32);
        }
        Self {
            sorted: boxes,
            rows,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.sorted.is_empty()
    }

    /// The ranges after the rows or columns of the sheet move for `edit`; a merge left with one cell is gone
    /// (`drop_single`), a table of one cell stays.
    pub(crate) fn shifted(&self, edit: &AxisEdit, drop_single: bool) -> Self {
        Self::new(shift_boxes(&self.sorted, edit, drop_single))
    }

    /// Whether a merged range shares a cell with the rectangle `(row0, col0, row1, col1)`: the merges that
    /// cover its first row come from the index, the ones that start inside it from the sorted list.
    pub(crate) fn touches(&self, (row0, col0, row1, col1): (u32, u32, u32, u32)) -> bool {
        let meets = |b: &CellBox| b.col0 <= col1 && b.col1 >= col0;
        let mut hit = false;
        self.rows.stab(row0, |id| {
            hit = hit || meets(&self.sorted[id as usize]);
        });
        hit || {
            let from = self.sorted.partition_point(|b| b.row0 <= row0);
            self.sorted[from..]
                .iter()
                .take_while(|b| b.row0 <= row1)
                .any(meets)
        }
    }
}
