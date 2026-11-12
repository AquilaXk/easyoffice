//! Row and column insertion and deletion: how a position, a span and a reference move.
//!
//! 참조 구현: Univer (https://github.com/dream-num/univer, Apache-2.0, Copyright 2023-present
//! DreamNum Co., Ltd., commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9).
//! - `packages/sheets/src/services/ref-range/util.ts` lines 630-690 (`handleBaseInsertRange`):
//!   a span that starts at or after the insertion shifts, a span that straddles it grows.
//! - the same file, lines 469-517 (`handleBaseRemoveRange`): a span inside the deleted rows
//!   disappears, a span that overlaps the deleted rows shrinks or starts at the deletion, a span
//!   after them shifts back; lines 1023-1059 (`runRefRangeMutations`) turn an empty result into
//!   "deleted".
//! - `packages/sheets-formula/src/controllers/utils/ref-range-formula.ts` lines 361-403
//!   (`handleInsertDelete`): the same rule applies to every formula reference.
//!
//! Modifications: the reference works on rotated rectangles; this port keeps one axis as a
//! `(start, end)` pair and applies the edit to it, the other axis never changes. A span that ends
//! on the last row or column (a whole-column or whole-row reference) keeps ending there, and a
//! shift past the edge of the sheet deletes the span instead of growing it. The reference's
//! operator list, selection handling and mutation objects are not used.

mod apply;
mod formula;
mod layout;

pub use apply::{AxisBudget, AxisEditReport};
pub use formula::rewrite_formula_refs;
pub use layout::{shift_boxes, shift_runs, shift_widths};

use crate::PackedCellCoordinate;
use easyoffice_formats::EasyOfficeError;

/// Most rows one insertion or deletion may cover.
pub const MAX_ROWS_PER_EDIT: u32 = 10_000;
/// Most columns one insertion or deletion may cover.
pub const MAX_COLS_PER_EDIT: u32 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Row,
    Col,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    Insert,
    Delete,
}

/// `count` rows or columns inserted before, or deleted from, zero-based index `at`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxisEdit {
    pub axis: Axis,
    pub kind: EditKind,
    pub at: u32,
    pub count: u32,
}

impl AxisEdit {
    pub fn new(axis: Axis, kind: EditKind, at: u32, count: u32) -> Result<Self, EasyOfficeError> {
        let (limit, per_edit, unit) = match axis {
            Axis::Row => (PackedCellCoordinate::MAX_ROWS, MAX_ROWS_PER_EDIT, "행"),
            Axis::Col => (PackedCellCoordinate::MAX_COLS, MAX_COLS_PER_EDIT, "열"),
        };
        if count == 0 || count > per_edit {
            return Err(EasyOfficeError::InvalidArgument(format!(
                "한 번에 {unit} 1~{per_edit}개까지 삽입하거나 삭제할 수 있습니다."
            )));
        }
        if at >= limit || (kind == EditKind::Delete && count > limit - at) {
            return Err(EasyOfficeError::InvalidArgument(format!(
                "{unit} 위치가 시트 범위를 벗어났습니다."
            )));
        }
        Ok(Self {
            axis,
            kind,
            at,
            count,
        })
    }

    /// Number of rows or columns on the edited axis.
    pub const fn limit(&self) -> u32 {
        match self.axis {
            Axis::Row => PackedCellCoordinate::MAX_ROWS,
            Axis::Col => PackedCellCoordinate::MAX_COLS,
        }
    }

    /// Last index that survives a deletion's shift (`at + count - 1` for a deletion).
    const fn deleted_end(&self) -> u32 {
        self.at + self.count - 1
    }

    /// Where the cell at `i` is after the edit; `None` when the edit deletes it (or pushes it off
    /// the sheet).
    #[inline]
    pub fn map_index(&self, i: u32) -> Option<u32> {
        if i < self.at {
            return Some(i);
        }
        match self.kind {
            EditKind::Insert => i.checked_add(self.count).filter(|&n| n < self.limit()),
            EditKind::Delete => (i > self.deleted_end()).then(|| i - self.count),
        }
    }

    /// Where the cell now at `i` was before the edit; `None` for an inserted cell.
    #[inline]
    pub fn unmap_index(&self, i: u32) -> Option<u32> {
        if i < self.at {
            return Some(i);
        }
        match self.kind {
            EditKind::Insert => (i >= self.at + self.count).then(|| i - self.count),
            EditKind::Delete => i.checked_add(self.count).filter(|&n| n < self.limit()),
        }
    }

    /// `coord` after the edit.
    #[inline]
    pub fn map_coord(&self, coord: PackedCellCoordinate) -> Option<PackedCellCoordinate> {
        let (row, col) = (coord.row(), coord.col());
        let (row, col) = match self.axis {
            Axis::Row => (self.map_index(row)?, col),
            Axis::Col => (row, self.map_index(col)?),
        };
        Some(PackedCellCoordinate(
            (u64::from(row) << 32) | u64::from(col),
        ))
    }

    /// `coord` before the edit.
    #[inline]
    pub fn unmap_coord(&self, coord: PackedCellCoordinate) -> Option<PackedCellCoordinate> {
        let (row, col) = (coord.row(), coord.col());
        let (row, col) = match self.axis {
            Axis::Row => (self.unmap_index(row)?, col),
            Axis::Col => (row, self.unmap_index(col)?),
        };
        Some(PackedCellCoordinate(
            (u64::from(row) << 32) | u64::from(col),
        ))
    }

    /// The span `start..=end` on the edited axis after the edit; `None` when the edit deletes
    /// all of it. Whether the result differs from the input is `!= Some((start, end))`.
    pub fn shift_span(&self, start: u32, end: u32) -> Option<(u32, u32)> {
        let last = self.limit() - 1;
        let n = self.count;
        let at = self.at;
        if start == 0 && end == last {
            return Some((0, last));
        }
        match self.kind {
            EditKind::Insert => {
                if end < at {
                    Some((start, end))
                } else if start >= at {
                    let new_start = start.checked_add(n).filter(|&s| s <= last)?;
                    Some((new_start, keep_edge(end, n, last)))
                } else {
                    Some((start, keep_edge(end, n, last)))
                }
            }
            EditKind::Delete => {
                let de = self.deleted_end();
                if end < at {
                    Some((start, end))
                } else if start > de {
                    Some((start - n, if end == last { last } else { end - n }))
                } else if start >= at && end <= de {
                    None
                } else if start < at {
                    // Overlaps the front of the deleted rows, or straddles all of them.
                    let new_end = if end > de {
                        if end == last {
                            last
                        } else {
                            end - n
                        }
                    } else {
                        at - 1
                    };
                    Some((start, new_end))
                } else {
                    // Starts inside the deleted rows and runs past them.
                    Some((at, if end == last { last } else { end - n }))
                }
            }
        }
    }
}

/// `end + count`, but a span that ends on the last row or column keeps ending there.
#[inline]
fn keep_edge(end: u32, count: u32, last: u32) -> u32 {
    if end == last {
        last
    } else {
        (end + count).min(last)
    }
}

#[cfg(test)]
mod tests;
