//! Reverse dependency graph of a workbook session: which formula cells must be recomputed when a
//! cell changes.
//!
//! Single-cell references are edges `cell -> dependents`; range references are registered once per
//! distinct rectangle in an interval index and answered by a point query, so a `SUM` over a whole
//! column costs one index entry instead of a million edges. Dirty propagation walks the reverse
//! edges breadth first over a bit set of vertex ids.
//!
//! 참조 구현: formualizer (MIT OR Apache-2.0), Copyright (c) 2025-2026 Frank Colson and
//! Formualizer contributors, `crates/formualizer-eval/src/engine/graph/range_deps.rs`
//! (`add_range_deps_from_keys`: range -> stripe registration, `collect_range_dependents_for_rect`:
//! candidate lookup plus exact overlap check) and `graph/mod.rs` (`mark_dirty_many`: transitive
//! dirty closure). Changes: ranges are indexed per column by row interval (tall and narrow ranges)
//! or in one row-interval index with an exact column check (wide ranges) instead of the reference's
//! row/column/block stripes, identical rectangles share one entry, dependents are a sorted vector
//! (a bit set per read cell grows with the highest vertex id), and keys are the packed workbook
//! coordinate. Source and license text: `THIRD_PARTY_NOTICES.md`.

use super::id_set::IdSet;
use super::interval::IntervalIndex;
use crate::{CellRange, Expr, FastMap, PackedCellCoordinate, PackedWorkbookCoord, WorkbookEngine};

/// Packed `(sheet index, row, col)`.
pub(crate) type CellKey = u64;

#[inline]
pub(crate) fn cell_key(sheet: u16, coord: PackedCellCoordinate) -> CellKey {
    PackedWorkbookCoord::new(sheet, coord).0
}

/// Ranges at most this wide are indexed once per column; wider ones share one row index.
const NARROW_MAX_WIDTH: u32 = 16;
/// Functions whose result changes without any referenced cell changing, or whose references are
/// computed at run time. Their cells are recomputed on every pass.
const VOLATILE_FUNCTIONS: [&str; 7] = [
    "RAND",
    "RANDBETWEEN",
    "RANDARRAY",
    "NOW",
    "TODAY",
    "OFFSET",
    "INDIRECT",
];

/// One reference a formula reads: a cell, or a rectangle (`r0 <= r1`, `c0 <= c1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Ref {
    Cell {
        sheet: u16,
        coord: PackedCellCoordinate,
    },
    Rect {
        sheet: u16,
        r0: u32,
        r1: u32,
        c0: u32,
        c1: u32,
    },
}

impl Ref {
    fn rect(sheet: u16, range: CellRange) -> Self {
        let r0 = range.start.row().min(range.end.row());
        let r1 = range.start.row().max(range.end.row());
        let c0 = range.start.col().min(range.end.col());
        let c1 = range.start.col().max(range.end.col());
        if r0 == r1 && c0 == c1 {
            Self::Cell {
                sheet,
                coord: range.start,
            }
        } else {
            Self::Rect {
                sheet,
                r0,
                r1,
                c0,
                c1,
            }
        }
    }
}

/// Collects the sorted, de-duplicated references of `ast` (a formula on `own_sheet`) into `out`
/// and reports whether the formula is volatile. Explicit stack, no recursion.
pub(crate) fn collect_refs(
    wb: &WorkbookEngine,
    own_sheet: u16,
    ast: &Expr,
    out: &mut Vec<Ref>,
) -> bool {
    out.clear();
    let mut volatile = false;
    let mut stack: Vec<&Expr> = vec![ast];
    let mut seen_shared: std::collections::HashSet<*const crate::SharedExpr> =
        std::collections::HashSet::new();
    let resolve = |name: &str| {
        wb.find_sheet_index(name)
            .and_then(|idx| u16::try_from(idx).ok())
    };
    while let Some(expr) = stack.pop() {
        match expr {
            Expr::Cell(coord) => out.push(Ref::Cell {
                sheet: own_sheet,
                coord: *coord,
            }),
            Expr::SheetCell { sheet, coord } => {
                if let Some(idx) = resolve(sheet) {
                    out.push(Ref::Cell {
                        sheet: idx,
                        coord: *coord,
                    });
                }
            }
            Expr::SpillRef { sheet, coord } => {
                let target = match sheet {
                    None => Some(own_sheet),
                    Some(name) => resolve(name),
                };
                if let Some(idx) = target {
                    out.push(Ref::Cell {
                        sheet: idx,
                        coord: *coord,
                    });
                }
            }
            Expr::Call { callee, args } => {
                stack.push(callee);
                stack.extend(args.iter());
            }
            Expr::Range(range) => out.push(Ref::rect(own_sheet, *range)),
            Expr::SheetRange { sheet, range } => {
                if let Some(idx) = resolve(sheet) {
                    out.push(Ref::rect(idx, *range));
                }
            }
            Expr::Shared(shared) => {
                // A shared tree is read once per formula however often it appears inside itself.
                let at = std::sync::Arc::as_ptr(shared);
                if seen_shared.insert(at) {
                    stack.push(&shared.expr);
                }
            }
            Expr::UnaryPlus(inner) | Expr::UnaryMinus(inner) | Expr::PostfixPercent(inner) => {
                stack.push(inner);
            }
            Expr::Binary { lhs, rhs, .. } => {
                stack.push(lhs);
                stack.push(rhs);
            }
            Expr::FuncCall { name, args } => {
                volatile |= VOLATILE_FUNCTIONS.contains(&name.as_str());
                stack.extend(args.iter());
            }
            Expr::Number(_)
            | Expr::Text(_)
            | Expr::Boolean(_)
            | Expr::ErrorLiteral(_)
            | Expr::Name { .. }
            | Expr::Array(_)
            | Expr::Var(_)
            | Expr::Missing => {}
        }
    }
    out.sort_unstable();
    out.dedup();
    volatile
}

/// Set of vertex ids as a sorted vector: 4 bytes per member whatever the ids are (a bit set costs
/// `max id / 8` bytes per set, which a crafted sheet turns into quadratic memory), binary-search
/// membership, and ascending iteration. Inserting into the middle of a very large set moves the
/// tail, which only a cell read by tens of thousands of formulas in random order pays.
#[derive(Debug, Clone, Default)]
struct Dependents(Vec<u32>);

impl Dependents {
    /// Adds `id`; whether it was new.
    fn insert(&mut self, id: u32) -> bool {
        match self.0.binary_search(&id) {
            Ok(_) => false,
            Err(pos) => {
                self.0.insert(pos, id);
                true
            }
        }
    }

    /// Removes `id`; whether it was present.
    fn remove(&mut self, id: u32) -> bool {
        match self.0.binary_search(&id) {
            Ok(pos) => {
                self.0.remove(pos);
                true
            }
            Err(_) => false,
        }
    }

    /// Removes the ids of `ids` (ascending); how many were present. One pass over the set.
    fn remove_sorted(&mut self, ids: &[u32]) -> usize {
        let before = self.0.len();
        let mut next = 0;
        self.0.retain(|&v| {
            while next < ids.len() && ids[next] < v {
                next += 1;
            }
            next >= ids.len() || ids[next] != v
        });
        before - self.0.len()
    }

    /// Adds the ids of `ids` (ascending); how many were new. One merge of two sorted runs.
    fn insert_sorted(&mut self, ids: &[u32]) -> usize {
        let before = self.0.len();
        let mut merged = Vec::with_capacity(before + ids.len());
        let (mut a, mut b) = (self.0.iter().peekable(), ids.iter().peekable());
        loop {
            let next = match (a.peek(), b.peek()) {
                (Some(&&x), Some(&&y)) if x == y => {
                    b.next();
                    a.next().copied()
                }
                (Some(&&x), Some(&&y)) if x < y => a.next().copied(),
                (Some(_), Some(_)) | (None, Some(_)) => b.next().copied(),
                (Some(_), None) => a.next().copied(),
                (None, None) => break,
            };
            merged.extend(next);
        }
        self.0 = merged;
        self.0.len() - before
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[inline]
    fn for_each(&self, mut f: impl FnMut(u32)) {
        self.0.iter().for_each(|&v| f(v));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct RectKey(u64, u64);

impl RectKey {
    fn new(sheet: u16, r0: u32, r1: u32, c0: u32, c1: u32) -> Self {
        Self(
            (u64::from(sheet) << 42) | (u64::from(r0) << 21) | u64::from(r1),
            (u64::from(c0) << 16) | u64::from(c1),
        )
    }
}

#[derive(Debug, Clone)]
struct RangeEntry {
    c0: u32,
    c1: u32,
    deps: Dependents,
}

#[derive(Debug, Default, Clone)]
struct SheetRanges {
    /// Column -> row intervals of the narrow ranges that cover it.
    narrow: FastMap<u32, IntervalIndex>,
    /// Row intervals of the wide ranges (the column span is checked on lookup).
    wide: IntervalIndex,
}

/// Distinct rectangles with the formulas that read them, indexed for point lookups.
#[derive(Debug, Default, Clone)]
struct RangeDeps {
    by_rect: FastMap<RectKey, u32>,
    entries: Vec<Option<RangeEntry>>,
    free: Vec<u32>,
    sheets: Vec<SheetRanges>,
    /// (rectangle, formula) pairs.
    edges: usize,
    /// Interval entries over all column indexes (a rectangle is entered once per column it spans).
    slots: usize,
    /// Per-column interval indexes currently allocated.
    containers: usize,
}

fn sheet_ranges(sheets: &mut Vec<SheetRanges>, sheet: u16) -> &mut SheetRanges {
    let idx = usize::from(sheet);
    if sheets.len() <= idx {
        sheets.resize_with(idx + 1, SheetRanges::default);
    }
    &mut sheets[idx]
}

impl RangeDeps {
    fn add(&mut self, sheet: u16, rect: (u32, u32, u32, u32), dependent: u32) {
        let (r0, r1, c0, c1) = rect;
        let key = RectKey::new(sheet, r0, r1, c0, c1);
        let id = match self.by_rect.get(&key) {
            Some(&id) => id,
            None => {
                let entry = RangeEntry {
                    c0,
                    c1,
                    deps: Dependents::default(),
                };
                let id = match self.free.pop() {
                    Some(id) => {
                        self.entries[id as usize] = Some(entry);
                        id
                    }
                    None => {
                        self.entries.push(Some(entry));
                        (self.entries.len() - 1) as u32
                    }
                };
                self.by_rect.insert(key, id);
                let ranges = sheet_ranges(&mut self.sheets, sheet);
                if c1 - c0 < NARROW_MAX_WIDTH {
                    for col in c0..=c1 {
                        let before = ranges.narrow.len();
                        ranges.narrow.entry(col).or_default().insert(r0, r1, id);
                        self.containers += ranges.narrow.len() - before;
                        self.slots += 1;
                    }
                } else {
                    ranges.wide.insert(r0, r1, id);
                    self.slots += 1;
                }
                id
            }
        };
        if let Some(entry) = self.entries[id as usize].as_mut() {
            self.edges += usize::from(entry.deps.insert(dependent));
        }
    }

    fn remove(&mut self, sheet: u16, rect: (u32, u32, u32, u32), dependent: u32) {
        let (r0, r1, c0, c1) = rect;
        let key = RectKey::new(sheet, r0, r1, c0, c1);
        let Some(&id) = self.by_rect.get(&key) else {
            return;
        };
        let Some(entry) = self.entries[id as usize].as_mut() else {
            return;
        };
        self.edges -= usize::from(entry.deps.remove(dependent));
        if !entry.deps.is_empty() {
            return;
        }
        self.entries[id as usize] = None;
        self.free.push(id);
        self.by_rect.remove(&key);
        let ranges = sheet_ranges(&mut self.sheets, sheet);
        if c1 - c0 < NARROW_MAX_WIDTH {
            for col in c0..=c1 {
                if let Some(index) = ranges.narrow.get_mut(&col) {
                    index.remove(r0, r1, id);
                    self.slots -= 1;
                    if index.is_empty() {
                        ranges.narrow.remove(&col);
                        self.containers -= 1;
                    }
                }
            }
        } else {
            ranges.wide.remove(r0, r1, id);
            self.slots -= 1;
        }
    }

    /// Calls `f` with the dependents of every registered rectangle that contains the cell.
    #[inline]
    fn visit_covering(&self, sheet: u16, row: u32, col: u32, mut f: impl FnMut(&Dependents)) {
        let Some(ranges) = self.sheets.get(usize::from(sheet)) else {
            return;
        };
        if let Some(index) = ranges.narrow.get(&col) {
            index.stab(row, |id| {
                if let Some(entry) = &self.entries[id as usize] {
                    f(&entry.deps);
                }
            });
        }
        ranges.wide.stab(row, |id| {
            if let Some(entry) = &self.entries[id as usize] {
                if entry.c0 <= col && col <= entry.c1 {
                    f(&entry.deps);
                }
            }
        });
    }

    fn len(&self) -> usize {
        self.by_rect.len()
    }
}

/// Formula cells (vertices) and the reverse edges between cells and the formulas that read them.
#[derive(Debug, Default, Clone)]
pub(crate) struct DepGraph {
    vertex_of: FastMap<CellKey, u32>,
    /// Sheet index and coordinate per vertex id (`u16::MAX` marks a free id).
    sheet: Vec<u16>,
    coord: Vec<PackedCellCoordinate>,
    free: Vec<u32>,
    cell_deps: FastMap<CellKey, Dependents>,
    /// (cell, formula) pairs in `cell_deps`.
    cell_edges: usize,
    ranges: RangeDeps,
    volatile: IdSet,
}

impl DepGraph {
    pub(crate) fn vertex_at(&self, sheet: u16, coord: PackedCellCoordinate) -> Option<u32> {
        self.vertex_of.get(&cell_key(sheet, coord)).copied()
    }

    #[inline]
    pub(crate) fn site(&self, vertex: u32) -> (u16, PackedCellCoordinate) {
        (self.sheet[vertex as usize], self.coord[vertex as usize])
    }

    pub(crate) fn vertex_count(&self) -> usize {
        self.vertex_of.len()
    }

    pub(crate) fn range_count(&self) -> usize {
        self.ranges.len()
    }

    /// Reverse edges held: (cell, formula) pairs plus (rectangle, formula) pairs.
    pub(crate) fn edge_count(&self) -> usize {
        self.cell_edges + self.ranges.edges
    }

    /// Estimated heap bytes of the graph. The per-item sizes are conservative averages (map slot,
    /// header and growth slack included); the point is a bound that grows with what the sheet
    /// registers, not a byte-exact count.
    pub(crate) fn heap_bytes_estimate(&self) -> usize {
        const EDGE: usize = 4;
        const CELL_KEY: usize = 80;
        const RECT: usize = 112;
        const INTERVAL_SLOT: usize = 80;
        const INDEX_CONTAINER: usize = 512;
        const VERTEX: usize = 56;
        self.edge_count() * EDGE
            + self.cell_deps.len() * CELL_KEY
            + self.ranges.len() * RECT
            + self.ranges.slots * INTERVAL_SLOT
            + self.ranges.containers * INDEX_CONTAINER
            + self.vertex_of.len() * VERTEX
    }

    /// Adds a formula cell and returns its id (a freed id is reused).
    pub(crate) fn add_vertex(&mut self, sheet: u16, coord: PackedCellCoordinate) -> u32 {
        let id = match self.free.pop() {
            Some(id) => {
                self.sheet[id as usize] = sheet;
                self.coord[id as usize] = coord;
                id
            }
            None => {
                self.sheet.push(sheet);
                self.coord.push(coord);
                (self.sheet.len() - 1) as u32
            }
        };
        self.vertex_of.insert(cell_key(sheet, coord), id);
        id
    }

    /// Removes a formula cell. Its references must already be unregistered.
    pub(crate) fn remove_vertex(&mut self, vertex: u32) {
        let (sheet, coord) = self.site(vertex);
        self.vertex_of.remove(&cell_key(sheet, coord));
        self.sheet[vertex as usize] = u16::MAX;
        self.volatile.remove(vertex);
        self.free.push(vertex);
    }

    pub(crate) fn set_volatile(&mut self, vertex: u32, volatile: bool) {
        if volatile {
            self.volatile.insert(vertex);
        } else {
            self.volatile.remove(vertex);
        }
    }

    /// Visits every live formula vertex in id order.
    pub(crate) fn for_each_vertex(&self, mut f: impl FnMut(u32)) {
        for (id, &sheet) in self.sheet.iter().enumerate() {
            if sheet != u16::MAX {
                f(id as u32);
            }
        }
    }

    pub(crate) fn for_each_volatile(&self, f: impl FnMut(u32)) {
        self.volatile.for_each(f);
    }

    pub(crate) fn register(&mut self, vertex: u32, refs: &[Ref]) {
        for &r in refs {
            match r {
                Ref::Cell { sheet, coord } => {
                    let deps = self.cell_deps.entry(cell_key(sheet, coord)).or_default();
                    self.cell_edges += usize::from(deps.insert(vertex));
                }
                Ref::Rect {
                    sheet,
                    r0,
                    r1,
                    c0,
                    c1,
                } => self.ranges.add(sheet, (r0, r1, c0, c1), vertex),
            }
        }
    }

    /// [`register`](Self::register) for many formulas at once: `pairs` are (reference, formula). A cell read
    /// by thousands of the formulas gets them in one merge instead of one shifted insert each.
    pub(crate) fn register_batch(&mut self, pairs: &mut [(Ref, u32)]) {
        pairs.sort_unstable();
        let mut ids: Vec<u32> = Vec::new();
        for group in pairs.chunk_by(|a, b| a.0 == b.0) {
            match group[0].0 {
                Ref::Cell { sheet, coord } => {
                    ids.clear();
                    ids.extend(group.iter().map(|&(_, v)| v));
                    let deps = self.cell_deps.entry(cell_key(sheet, coord)).or_default();
                    self.cell_edges += deps.insert_sorted(&ids);
                }
                Ref::Rect {
                    sheet,
                    r0,
                    r1,
                    c0,
                    c1,
                } => {
                    for &(_, v) in group {
                        self.ranges.add(sheet, (r0, r1, c0, c1), v);
                    }
                }
            }
        }
    }

    /// [`unregister`](Self::unregister) for many formulas at once (see [`register_batch`](Self::register_batch)).
    pub(crate) fn unregister_batch(&mut self, pairs: &mut [(Ref, u32)]) {
        pairs.sort_unstable();
        let mut ids: Vec<u32> = Vec::new();
        for group in pairs.chunk_by(|a, b| a.0 == b.0) {
            match group[0].0 {
                Ref::Cell { sheet, coord } => {
                    let key = cell_key(sheet, coord);
                    ids.clear();
                    ids.extend(group.iter().map(|&(_, v)| v));
                    if let Some(deps) = self.cell_deps.get_mut(&key) {
                        self.cell_edges -= deps.remove_sorted(&ids);
                        if deps.is_empty() {
                            self.cell_deps.remove(&key);
                        }
                    }
                }
                Ref::Rect {
                    sheet,
                    r0,
                    r1,
                    c0,
                    c1,
                } => {
                    for &(_, v) in group {
                        self.ranges.remove(sheet, (r0, r1, c0, c1), v);
                    }
                }
            }
        }
    }

    pub(crate) fn unregister(&mut self, vertex: u32, refs: &[Ref]) {
        for &r in refs {
            match r {
                Ref::Cell { sheet, coord } => {
                    let key = cell_key(sheet, coord);
                    if let Some(deps) = self.cell_deps.get_mut(&key) {
                        self.cell_edges -= usize::from(deps.remove(vertex));
                        if deps.is_empty() {
                            self.cell_deps.remove(&key);
                        }
                    }
                }
                Ref::Rect {
                    sheet,
                    r0,
                    r1,
                    c0,
                    c1,
                } => self.ranges.remove(sheet, (r0, r1, c0, c1), vertex),
            }
        }
    }

    /// Calls `f` for every formula that reads the cell, directly or through a range.
    #[inline]
    fn visit_dependents(&self, sheet: u16, coord: PackedCellCoordinate, mut f: impl FnMut(u32)) {
        if let Some(deps) = self.cell_deps.get(&cell_key(sheet, coord)) {
            deps.for_each(&mut f);
        }
        self.ranges
            .visit_covering(sheet, coord.row(), coord.col(), |deps| {
                deps.for_each(&mut f)
            });
    }

    /// Adds to `dirty` the formulas that read a seed cell directly or through a range (no further closure).
    pub(crate) fn mark_dependents(&self, seeds: &[(u16, PackedCellCoordinate)], dirty: &mut IdSet) {
        for &(sheet, coord) in seeds {
            self.visit_dependents(sheet, coord, |v| {
                dirty.insert(v);
            });
        }
    }

    /// Adds to `dirty` every formula that transitively reads a seed cell or a vertex already in
    /// `dirty`. `scratch` is a reusable work queue.
    pub(crate) fn expand_dirty(
        &self,
        seeds: &[(u16, PackedCellCoordinate)],
        dirty: &mut IdSet,
        scratch: &mut Vec<u32>,
    ) {
        scratch.clear();
        dirty.for_each(|v| scratch.push(v));
        for &(sheet, coord) in seeds {
            self.visit_dependents(sheet, coord, |v| {
                if dirty.insert(v) {
                    scratch.push(v);
                }
            });
        }
        while let Some(v) = scratch.pop() {
            let (sheet, coord) = self.site(v);
            self.visit_dependents(sheet, coord, |d| {
                if dirty.insert(d) {
                    scratch.push(d);
                }
            });
        }
    }
}
