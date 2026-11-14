//! The cells `OFFSET` and `INDIRECT` read while a pass calculates (PD-222). Their reference is
//! computed at run time, so the dependency graph cannot know it before the formula runs; the pass
//! writes down what each formula resolved to, and the session turns that into dependency edges for the
//! next round (the cells an array filled in the round are what makes a reader calculate again).
//!
//! 참조 구현: Univer (Apache-2.0), Copyright 2023-present DreamNum Co., Ltd.,
//! `packages/engine-formula/src/engine/dependency/formula-dependency.ts` (`_getRangeListByFunctionRefNode`
//! runs the reference-returning functions to learn their ranges, `_addDependencyTreeByAddressFunction`
//! adds those ranges as dependencies of the formula; commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9),
//! and IronCalc (MIT OR Apache-2.0), `base/src/evaluation.rs` (`record_seen`: a read is put on record so
//! a later write that contradicts it is noticed; commit 01b39c1a). Changes: the reference runs the
//! functions in a separate analysis step before the calculation and restarts the pass on a
//! contradiction; here the ranges are the ones the real calculation resolved (so a reference that
//! depends on other cells is the one in force), they are kept per formula in one small list, and a
//! formula that resolves to more ranges than the list holds is marked as reading everything. Source
//! and license text: `THIRD_PARTY_NOTICES.md`.

use crate::{FastMap, PackedCellCoordinate, PackedWorkbookCoord};

/// Distinct ranges kept for one formula; a formula past it is calculated again whenever a round filled cells.
pub(crate) const MAX_RECTS_PER_READER: usize = 32;

/// A rectangle on a sheet, as sheet id and zero-based inclusive rows and columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DynRect {
    pub(crate) sheet_id: u16,
    pub(crate) r0: u32,
    pub(crate) r1: u32,
    pub(crate) c0: u32,
    pub(crate) c1: u32,
}

/// What one formula read through `OFFSET` and `INDIRECT`.
#[derive(Debug, Default)]
pub(crate) struct Reads {
    pub(crate) rects: Vec<DynRect>,
    /// More ranges than [`MAX_RECTS_PER_READER`]: the formula is treated as reading anything.
    pub(crate) overflow: bool,
}

/// The reads of the formulas a pass calculated, by the formula's sheet id and cell.
#[derive(Debug, Default)]
pub(crate) struct DynReads {
    by_reader: FastMap<u64, Reads>,
}

impl DynReads {
    /// Notes that the formula at `coord` of the sheet `reader_sheet` read `rect`.
    pub(crate) fn record(
        &mut self,
        reader_sheet: u16,
        reader: PackedCellCoordinate,
        rect: DynRect,
    ) {
        let reads = self
            .by_reader
            .entry(PackedWorkbookCoord::new(reader_sheet, reader).0)
            .or_default();
        if reads.overflow || reads.rects.contains(&rect) {
            return;
        }
        if reads.rects.len() >= MAX_RECTS_PER_READER {
            reads.overflow = true;
            reads.rects = Vec::new();
        } else {
            reads.rects.push(rect);
        }
    }

    pub(crate) fn take(
        &mut self,
        reader_sheet: u16,
        reader: PackedCellCoordinate,
    ) -> Option<Reads> {
        self.by_reader
            .remove(&PackedWorkbookCoord::new(reader_sheet, reader).0)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.by_reader.is_empty()
    }
}
