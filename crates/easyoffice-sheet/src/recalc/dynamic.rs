//! What a session knows about the cells `OFFSET` and `INDIRECT` read (PD-222). The static graph has no
//! edge for a reference that is computed while the formula runs, so after every round the ranges the
//! round's formulas resolved to (see `dyn_reads`) become edges of a second graph, and the cells a round
//! publishes (the ones arrays fill) dirty the formulas that read them for the next round.

use super::graph::{DepGraph, Ref};
use super::id_set::IdSet;
use crate::dyn_reads::{DynReads, DynRect, Reads};
use crate::{FastMap, PackedCellCoordinate};

/// Edges of the dynamic graph; a formula that would pass it is treated as reading anything.
const MAX_DYNAMIC_EDGES: usize = 262_144;

#[derive(Debug, Default, Clone)]
pub(super) struct DynamicReads {
    edges: DepGraph,
    held: FastMap<u32, Vec<Ref>>,
    /// Formulas that read more than the edges can say: calculated again whenever a round filled cells.
    anything: IdSet,
}

fn as_ref(rect: &DynRect, sheet_of: &impl Fn(u16) -> Option<u16>) -> Option<Ref> {
    let sheet = sheet_of(rect.sheet_id)?;
    Some(if rect.r0 == rect.r1 && rect.c0 == rect.c1 {
        Ref::Cell {
            sheet,
            coord: PackedCellCoordinate(u64::from(rect.r0) << 32 | u64::from(rect.c0)),
        }
    } else {
        Ref::Rect {
            sheet,
            r0: rect.r0,
            r1: rect.r1,
            c0: rect.c0,
            c1: rect.c1,
        }
    })
}

impl DynamicReads {
    /// Nothing is held: no formula read through `OFFSET` or `INDIRECT` yet.
    pub(super) fn is_idle(&self) -> bool {
        self.held.is_empty() && self.anything.is_empty()
    }

    pub(super) fn edge_count(&self) -> usize {
        self.edges.edge_count()
    }

    pub(super) fn heap_bytes_estimate(&self) -> usize {
        self.edges.heap_bytes_estimate()
    }

    /// The reads of the formula `vertex` in the pass that just ran, replacing what it read before.
    pub(super) fn replace(
        &mut self,
        vertex: u32,
        reads: Option<Reads>,
        sheet_of: &impl Fn(u16) -> Option<u16>,
    ) {
        let refs: Vec<Ref> = reads.as_ref().map_or_else(Vec::new, |r| {
            r.rects.iter().filter_map(|r| as_ref(r, sheet_of)).collect()
        });
        let overflow = reads.is_some_and(|r| r.overflow);
        // The same references as the last pass (a volatile formula that reads the same cells): nothing to move.
        if !overflow
            && !self.anything.contains(vertex)
            && self
                .held
                .get(&vertex)
                .map_or(refs.is_empty(), |old| *old == refs)
        {
            return;
        }
        self.forget(vertex);
        if overflow || self.edges.edge_count() + refs.len() > MAX_DYNAMIC_EDGES {
            self.anything.insert(vertex);
        } else if !refs.is_empty() {
            self.edges.register(vertex, &refs);
            self.held.insert(vertex, refs);
        }
    }

    /// The formula `vertex` is gone or is calculated from scratch.
    pub(super) fn forget(&mut self, vertex: u32) {
        if let Some(old) = self.held.remove(&vertex) {
            self.edges.unregister(vertex, &old);
        }
        self.anything.remove(vertex);
    }

    /// Adds to `dirty` the formulas that read one of the cells `seeds` through `OFFSET` or `INDIRECT`.
    pub(super) fn mark_readers(
        &self,
        seeds: &[(u16, PackedCellCoordinate)],
        dirty: &mut IdSet,
        is_live: &impl Fn(u32) -> bool,
    ) {
        if seeds.is_empty() || self.is_idle() {
            return;
        }
        let mut found = IdSet::default();
        self.edges.mark_dependents(seeds, &mut found);
        self.anything.for_each(|v| {
            found.insert(v);
        });
        // A formula that left the graph is never calculated (its id may be free or reused).
        found.for_each(|v| {
            if is_live(v) {
                dirty.insert(v);
            }
        });
    }

    /// Takes the reads of every vertex in `vertices` out of `reads`.
    pub(super) fn absorb(
        &mut self,
        mut reads: DynReads,
        vertices: impl Iterator<Item = (u32, u16, PackedCellCoordinate)>,
        sheet_id: &impl Fn(u16) -> u16,
        sheet_of: &impl Fn(u16) -> Option<u16>,
    ) {
        if reads.is_empty() && self.is_idle() {
            return;
        }
        for (vertex, sheet, coord) in vertices {
            self.replace(vertex, reads.take(sheet_id(sheet), coord), sheet_of);
        }
    }
}
