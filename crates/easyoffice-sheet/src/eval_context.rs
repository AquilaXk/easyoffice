//! Evaluation context tracking limits, memo table, random state, and transient overlays.

use crate::array::Grid;
use crate::deadline::Clock;
use crate::dyn_reads::DynReads;
use crate::lookup_index::LookupCache;
use crate::memo::MemoTable;
use crate::spill::Pending;
use crate::{EvaluatedCellValue, FastMap, PackedCellCoordinate, WorkbookEngine};
use std::sync::Arc;

/// A claimed spill rectangle placed in the current pass (PD-217).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClaimedSpillRect {
    pub(crate) sheet_id: u16,
    pub(crate) r0: u32,
    pub(crate) c0: u32,
    pub(crate) r1: u32,
    pub(crate) c1: u32,
    pub(crate) owner: PackedCellCoordinate,
}

impl ClaimedSpillRect {
    #[inline]
    pub(crate) fn overlaps(
        &self,
        sheet_id: u16,
        r0: u32,
        c0: u32,
        r1: u32,
        c1: u32,
        owner: PackedCellCoordinate,
    ) -> bool {
        if self.sheet_id != sheet_id || self.owner == owner {
            return false;
        }
        self.r0 <= r1 && r0 <= self.r1 && self.c0 <= c1 && c0 <= self.c1
    }
}

/// Row-striped spatial set of claimed spill rectangles in the current pass (PD-217).
/// Uses zero-allocation linear scan up to 16 claims, switching to 64-row stripe buckets
/// (plus an overflow list for tall rectangles) so `K` spills on one sheet check overlap in O(1).
#[derive(Debug, Default)]
pub(crate) struct ClaimedSpillSet {
    rects: Vec<ClaimedSpillRect>,
    stripes: FastMap<u64, Vec<u32>>,
    tall: Vec<u32>,
}

const CLAIM_LINEAR_CAP: usize = 16;
const CLAIM_STRIPE_SHIFT: u32 = 6;
const CLAIM_MAX_STRIPES: u32 = 128;

impl ClaimedSpillSet {
    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    #[inline]
    fn index_rect(
        stripes: &mut FastMap<u64, Vec<u32>>,
        tall: &mut Vec<u32>,
        idx: u32,
        rect: &ClaimedSpillRect,
    ) {
        let s0 = rect.r0 >> CLAIM_STRIPE_SHIFT;
        let s1 = rect.r1 >> CLAIM_STRIPE_SHIFT;
        if s1 - s0 > CLAIM_MAX_STRIPES {
            tall.push(idx);
        } else {
            let prefix = u64::from(rect.sheet_id) << 32;
            for s in s0..=s1 {
                stripes.entry(prefix | u64::from(s)).or_default().push(idx);
            }
        }
    }

    pub(crate) fn push(&mut self, rect: ClaimedSpillRect) {
        let idx = self.rects.len() as u32;
        self.rects.push(rect);
        if self.rects.len() == CLAIM_LINEAR_CAP + 1 {
            for (i, r) in self.rects.iter().enumerate() {
                Self::index_rect(&mut self.stripes, &mut self.tall, i as u32, r);
            }
        } else if self.rects.len() > CLAIM_LINEAR_CAP {
            Self::index_rect(&mut self.stripes, &mut self.tall, idx, &rect);
        }
    }

    pub(crate) fn has_overlap(
        &self,
        sheet_id: u16,
        r0: u32,
        c0: u32,
        r1: u32,
        c1: u32,
        owner: PackedCellCoordinate,
    ) -> bool {
        if self.rects.len() <= CLAIM_LINEAR_CAP {
            return self
                .rects
                .iter()
                .any(|c| c.overlaps(sheet_id, r0, c0, r1, c1, owner));
        }
        let s0 = r0 >> CLAIM_STRIPE_SHIFT;
        let s1 = r1 >> CLAIM_STRIPE_SHIFT;
        if s1 - s0 > CLAIM_MAX_STRIPES {
            return self
                .rects
                .iter()
                .any(|c| c.overlaps(sheet_id, r0, c0, r1, c1, owner));
        }
        if self
            .tall
            .iter()
            .any(|&i| self.rects[i as usize].overlaps(sheet_id, r0, c0, r1, c1, owner))
        {
            return true;
        }
        let prefix = u64::from(sheet_id) << 32;
        for s in s0..=s1 {
            if let Some(bucket) = self.stripes.get(&(prefix | u64::from(s))) {
                if bucket
                    .iter()
                    .any(|&i| self.rects[i as usize].overlaps(sheet_id, r0, c0, r1, c1, owner))
                {
                    return true;
                }
            }
        }
        false
    }
}

pub(crate) struct EvalContext<'a> {
    pub workbook: Option<&'a WorkbookEngine>,
    pub memo: MemoTable,
    pub current_coord: Option<PackedCellCoordinate>,
    pub(crate) steps: usize,
    pub(crate) array_bytes: usize,
    pub(crate) rng_state: u64,
    pub(crate) circular: bool,
    pub(crate) clock: Clock,
    /// Arrays that stand for temporary ranges while a function of the scalar engine runs on them.
    pub(crate) temp: Vec<Arc<Grid>>,
    /// Calls of `LAMBDA` values in progress.
    pub(crate) lambda_depth: usize,
    /// Cells anchors may still fill; `None` when areas are not placed (a calculation outside a session).
    pub(crate) spill_budget: Option<usize>,
    /// What the pass decided for the anchors it calculated.
    pub(crate) pending: Vec<Pending>,
    /// Rectangles the pass has given to anchors.
    pub(crate) claims: ClaimedSpillSet,
    /// The arrays this pass placed, by sheet and anchor: a `A1#` read in the same pass takes the array itself.
    pub(crate) fresh: FastMap<u64, Arc<Grid>>,
    /// What the formulas of this pass read through `OFFSET` and `INDIRECT`; `None` when nobody collects it.
    pub(crate) dyn_reads: Option<DynReads>,
    pub(crate) lookup_cache: LookupCache,
    /// Virtual single-cell override for zero-mutation evaluations.
    pub(crate) cell_overlay: Option<(u16, PackedCellCoordinate, EvaluatedCellValue)>,
}

impl<'a> EvalContext<'a> {
    #[inline]
    pub fn standalone_id(sheet_id: u16) -> Self {
        Self {
            workbook: None,
            memo: MemoTable::new(sheet_id),
            current_coord: None,
            steps: 0,
            array_bytes: 0,
            rng_state: 0x5DEE_CE66_D1A4_F87B,
            circular: false,
            clock: Clock::default(),
            temp: Vec::new(),
            lambda_depth: 0,
            spill_budget: None,
            pending: Vec::new(),
            claims: ClaimedSpillSet::default(),
            fresh: FastMap::default(),
            dyn_reads: None,
            lookup_cache: LookupCache::default(),
            cell_overlay: None,
        }
    }

    #[inline]
    pub fn with_workbook(workbook: &'a WorkbookEngine, sheet_id: u16) -> Self {
        Self {
            workbook: Some(workbook),
            memo: MemoTable::new(sheet_id),
            current_coord: None,
            steps: 0,
            array_bytes: 0,
            rng_state: 0x5DEE_CE66_D1A4_F87B,
            circular: false,
            clock: Clock::default(),
            temp: Vec::new(),
            lambda_depth: 0,
            spill_budget: None,
            pending: Vec::new(),
            claims: ClaimedSpillSet::default(),
            fresh: FastMap::default(),
            dyn_reads: None,
            lookup_cache: LookupCache::default(),
            cell_overlay: None,
        }
    }

    /// The value at `coord` of temporary array number `index`.
    pub(crate) fn temp_cell(
        &self,
        index: usize,
        coord: PackedCellCoordinate,
    ) -> EvaluatedCellValue {
        match self.temp.get(index) {
            Some(grid)
                if (coord.row() as usize) < grid.rows && (coord.col() as usize) < grid.cols =>
            {
                grid.get(coord.row() as usize, coord.col() as usize).clone()
            }
            _ => EvaluatedCellValue::Empty,
        }
    }

    #[inline]
    pub fn next_rand_f64(&mut self) -> f64 {
        self.rng_state = self
            .rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.rng_state >> 11) as f64) * (1.0 / ((1u64 << 53) as f64))
    }
}
