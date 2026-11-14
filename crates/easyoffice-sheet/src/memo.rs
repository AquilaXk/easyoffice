//! Evaluation storage for the sheet engine: a seeded multiplicative hasher for packed-coordinate
//! keys, the slot-indexed value memo (dense pages instead of hashing for the primary sheet), the
//! per-evaluation "visiting" bit set used for cycle detection, and the 2-bit DFS state set used by
//! the dependency pre-pass.

use crate::{EvaluatedCellValue, PackedCellCoordinate, PackedWorkbookCoord};
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::OnceLock;

const MIX: u64 = 0x9E37_79B9_7F4A_7C15;

fn process_seed() -> u64 {
    static SEED: OnceLock<u64> = OnceLock::new();
    *SEED.get_or_init(|| {
        use std::hash::{BuildHasher, RandomState};
        RandomState::new().hash_one(0x5EED_u64) | 1
    })
}

/// Multiplicative hasher for already-packed integer keys. It folds a per-process random seed into
/// every hash so crafted coordinates cannot force collisions, while staying much cheaper than
/// SipHash on the evaluation hot path.
#[derive(Debug, Clone, Copy)]
pub struct CoordHasher(u64);

impl Default for CoordHasher {
    #[inline]
    fn default() -> Self {
        Self(process_seed())
    }
}

impl Hasher for CoordHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0 ^ (self.0 >> 32)
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0.rotate_left(5) ^ u64::from(b)).wrapping_mul(MIX);
        }
    }

    #[inline]
    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(MIX);
    }

    #[inline]
    fn write_u32(&mut self, value: u32) {
        self.write_u64(u64::from(value));
    }

    #[inline]
    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }
}

/// Lightweight deterministic single-word mixing hasher (`DetWordHasher`) for purely internal,
/// non-adversarial integer or pointer keys (dense vertex ids, sheet indices, `Arc` pointers).
/// External coordinate or sheet-name maps must retain seeded [`CoordHasher`] or `RandomState`.
#[derive(Debug, Clone, Copy, Default)]
pub struct DetWordHasher(u64);

impl Hasher for DetWordHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0 ^ (self.0 >> 32)
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0.rotate_left(5) ^ u64::from(b)).wrapping_mul(MIX);
        }
    }

    #[inline]
    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(MIX);
    }

    #[inline]
    fn write_u32(&mut self, value: u32) {
        self.write_u64(u64::from(value));
    }

    #[inline]
    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }
}

/// `HashMap` keyed by packed integers using seeded [`CoordHasher`].
pub type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<CoordHasher>>;

/// `HashMap` keyed by trusted internal integer/pointer keys using [`DetWordHasher`].
pub type DetMap<K, V> = HashMap<K, V, BuildHasherDefault<DetWordHasher>>;

/// `HashSet` keyed by trusted internal integer/pointer keys using [`DetWordHasher`].
pub type DetSet<K> = HashSet<K, BuildHasherDefault<DetWordHasher>>;

/// Map from packed cell coordinate to a value (raw text, compiled AST, ...).
pub type CellMap<V> = FastMap<PackedCellCoordinate, V>;

const PAGE_ROW_SHIFT: u32 = 5;
const PAGE_CELLS: usize = 32 * 16;
const DENSE_COLS: u32 = 16;
const LEAF_SHIFT: u32 = 7;
const LEAF_LEN: usize = 1 << LEAF_SHIFT;
const LEAF_MASK: usize = LEAF_LEN - 1;
const MAX_ROOT_GROUPS: usize = 256;

type RadixLeaf<P> = [Option<Box<P>>; LEAF_LEN];

/// Two-level radix page directory: at most 256 root group pointers each holding a 128-page leaf
/// (32,768 pages total covering `MAX_ROWS = 1_048_576`). Sparse high-row cells allocate at most
/// one leaf group instead of a flat 32k-slot directory.
struct RadixDir<P> {
    groups: Vec<Option<Box<RadixLeaf<P>>>>,
}

impl<P> RadixDir<P> {
    #[inline]
    fn new() -> Self {
        Self { groups: Vec::new() }
    }

    #[inline]
    fn get(&self, page: usize) -> Option<&P> {
        let leaf = self.groups.get(page >> LEAF_SHIFT)?.as_deref()?;
        leaf[page & LEAF_MASK].as_deref()
    }

    #[inline]
    fn get_mut(&mut self, page: usize) -> Option<&mut P> {
        let leaf = self.groups.get_mut(page >> LEAF_SHIFT)?.as_deref_mut()?;
        leaf[page & LEAF_MASK].as_deref_mut()
    }

    #[inline]
    fn get_or_insert_with(&mut self, page: usize, init: impl FnOnce() -> Box<P>) -> &mut P {
        let root_idx = page >> LEAF_SHIFT;
        debug_assert!(root_idx < MAX_ROOT_GROUPS);
        if self.groups.len() <= root_idx {
            self.groups.resize_with(root_idx + 1, || None);
        }
        let leaf =
            self.groups[root_idx].get_or_insert_with(|| Box::new(std::array::from_fn(|_| None)));
        leaf[page & LEAF_MASK].get_or_insert_with(init)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.groups.len()
    }

    #[cfg(test)]
    fn allocated_groups(&self) -> usize {
        self.groups.iter().filter(|g| g.is_some()).count()
    }
}

#[inline]
const fn page_slot(row: u32, col: u32) -> (usize, usize) {
    (
        (row >> PAGE_ROW_SHIFT) as usize,
        (((row & 31) << 4) | col) as usize,
    )
}

/// 32x16 block of cells: an arena index per cell (0 = not memoized, otherwise index + 1) and the
/// evaluation "visiting" bits. Values live in a compact arena so untouched cells cost 4 bytes
/// instead of a whole `EvaluatedCellValue`.
struct MemoPage {
    slots: [u32; PAGE_CELLS],
    visiting: [u64; PAGE_CELLS / 64],
}

impl MemoPage {
    fn new() -> Box<Self> {
        Box::new(Self {
            slots: [0; PAGE_CELLS],
            visiting: [0; PAGE_CELLS / 64],
        })
    }
}

/// Memoized cell values plus the evaluation "visiting" set. Cells of the primary sheet in the
/// first 16 columns live in lazily allocated 32x16 pages addressed by two-level radix shifts; all
/// other cells fall back to a seeded fast hash map.
pub struct MemoTable {
    primary_sheet_id: u16,
    pages: RadixDir<MemoPage>,
    arena: Vec<EvaluatedCellValue>,
    /// Arena slots (index + 1) released by `remove_at`, reused before the arena grows.
    free: Vec<u32>,
    overflow: FastMap<u64, EvaluatedCellValue>,
    visiting_overflow: FastMap<u64, ()>,
}

impl MemoTable {
    pub fn new(primary_sheet_id: u16) -> Self {
        Self {
            primary_sheet_id,
            pages: RadixDir::new(),
            arena: Vec::new(),
            free: Vec::new(),
            overflow: FastMap::default(),
            visiting_overflow: FastMap::default(),
        }
    }

    #[inline]
    fn dense(&self, sheet_id: u16, coord: PackedCellCoordinate) -> Option<(usize, usize)> {
        (sheet_id == self.primary_sheet_id
            && coord.col() < DENSE_COLS
            && coord.row() < PackedCellCoordinate::MAX_ROWS)
            .then(|| page_slot(coord.row(), coord.col()))
    }

    #[inline]
    pub fn get_at(
        &self,
        sheet_id: u16,
        coord: PackedCellCoordinate,
    ) -> Option<&EvaluatedCellValue> {
        match self.dense(sheet_id, coord) {
            Some((page, slot)) => {
                let index = self.pages.get(page)?.slots[slot];
                (index != 0).then(|| &self.arena[index as usize - 1])
            }
            None => self
                .overflow
                .get(&PackedWorkbookCoord::new(sheet_id, coord).0),
        }
    }

    #[inline]
    pub fn get(&self, key: &PackedWorkbookCoord) -> Option<&EvaluatedCellValue> {
        self.get_at(key.sheet_id(), coord_of(*key))
    }

    #[inline]
    pub fn contains_at(&self, sheet_id: u16, coord: PackedCellCoordinate) -> bool {
        self.get_at(sheet_id, coord).is_some()
    }

    #[inline]
    pub fn contains_key(&self, key: &PackedWorkbookCoord) -> bool {
        self.get(key).is_some()
    }

    /// Stores `value` for the cell (replacing any previous one).
    pub fn insert_at(
        &mut self,
        sheet_id: u16,
        coord: PackedCellCoordinate,
        value: EvaluatedCellValue,
    ) {
        match self.dense(sheet_id, coord) {
            Some((page, slot)) => {
                let existing = self.page_mut(page).slots[slot];
                if existing != 0 {
                    self.arena[existing as usize - 1] = value;
                } else if let Some(reused) = self.free.pop() {
                    self.arena[reused as usize - 1] = value;
                    self.page_mut(page).slots[slot] = reused;
                } else {
                    // The step budget keeps memoized cells far below u32::MAX.
                    let next = u32::try_from(self.arena.len() + 1).unwrap_or(u32::MAX);
                    self.arena.push(value);
                    self.page_mut(page).slots[slot] = next;
                }
            }
            None => {
                self.overflow
                    .insert(PackedWorkbookCoord::new(sheet_id, coord).0, value);
            }
        }
    }

    /// Forgets the cell's memoized value and returns it. A long-lived memo (an editing session)
    /// uses this to drop the values of cells that must be recomputed.
    pub fn remove_at(
        &mut self,
        sheet_id: u16,
        coord: PackedCellCoordinate,
    ) -> Option<EvaluatedCellValue> {
        match self.dense(sheet_id, coord) {
            Some((page, slot)) => {
                let page = self.pages.get_mut(page)?;
                let index = std::mem::take(&mut page.slots[slot]);
                if index == 0 {
                    return None;
                }
                self.free.push(index);
                Some(std::mem::replace(
                    &mut self.arena[index as usize - 1],
                    EvaluatedCellValue::Empty,
                ))
            }
            None => self
                .overflow
                .remove(&PackedWorkbookCoord::new(sheet_id, coord).0),
        }
    }

    pub fn insert(&mut self, key: PackedWorkbookCoord, value: EvaluatedCellValue) {
        self.insert_at(key.sheet_id(), coord_of(key), value);
    }

    fn page_mut(&mut self, page: usize) -> &mut MemoPage {
        self.pages.get_or_insert_with(page, MemoPage::new)
    }

    /// Marks the cell as being evaluated; `false` when it already is (a cycle).
    #[inline]
    pub fn visit_insert(&mut self, sheet_id: u16, coord: PackedCellCoordinate) -> bool {
        match self.dense(sheet_id, coord) {
            Some((page, slot)) => {
                let word = &mut self.page_mut(page).visiting[slot >> 6];
                let mask = 1u64 << (slot & 63);
                let fresh = *word & mask == 0;
                *word |= mask;
                fresh
            }
            None => self
                .visiting_overflow
                .insert(PackedWorkbookCoord::new(sheet_id, coord).0, ())
                .is_none(),
        }
    }

    #[inline]
    pub fn visit_remove(&mut self, sheet_id: u16, coord: PackedCellCoordinate) {
        match self.dense(sheet_id, coord) {
            Some((page, slot)) => {
                if let Some(p) = self.pages.get_mut(page) {
                    p.visiting[slot >> 6] &= !(1u64 << (slot & 63));
                }
            }
            None => {
                self.visiting_overflow
                    .remove(&PackedWorkbookCoord::new(sheet_id, coord).0);
            }
        }
    }

    /// `true` while `coord` is actively on the evaluation stack.
    #[inline]
    pub fn is_visiting(&self, sheet_id: u16, coord: PackedCellCoordinate) -> bool {
        match self.dense(sheet_id, coord) {
            Some((page, slot)) => self
                .pages
                .get(page)
                .is_some_and(|p| (p.visiting[slot >> 6] & (1u64 << (slot & 63))) != 0),
            None => self
                .visiting_overflow
                .contains_key(&PackedWorkbookCoord::new(sheet_id, coord).0),
        }
    }
}

#[inline]
fn coord_of(key: PackedWorkbookCoord) -> PackedCellCoordinate {
    PackedCellCoordinate((u64::from(key.row()) << 32) | u64::from(key.col()))
}

/// DFS colour of a cell during the dependency pre-pass: 0 unseen, 1 on the stack, 2 finished.
/// Stored as 2 bits per cell (32 cells per `u64`) for the primary sheet's first 16 columns.
pub(crate) struct DfsState {
    primary_sheet_id: u16,
    pages: RadixDir<[u64; PAGE_CELLS / 32]>,
    overflow: FastMap<u64, u8>,
}

impl DfsState {
    pub fn new(primary_sheet_id: u16) -> Self {
        Self {
            primary_sheet_id,
            pages: RadixDir::new(),
            overflow: FastMap::default(),
        }
    }

    #[inline]
    fn is_dense(&self, sheet_id: u16, coord: PackedCellCoordinate) -> bool {
        sheet_id == self.primary_sheet_id
            && coord.col() < DENSE_COLS
            && coord.row() < PackedCellCoordinate::MAX_ROWS
    }

    #[inline]
    pub fn get(&self, sheet_id: u16, coord: PackedCellCoordinate) -> u8 {
        if self.is_dense(sheet_id, coord) {
            let (page, slot) = page_slot(coord.row(), coord.col());
            match self.pages.get(page) {
                Some(words) => ((words[slot >> 5] >> ((slot & 31) * 2)) & 3) as u8,
                None => 0,
            }
        } else {
            self.overflow
                .get(&PackedWorkbookCoord::new(sheet_id, coord).0)
                .copied()
                .unwrap_or(0)
        }
    }

    #[inline]
    pub fn set(&mut self, sheet_id: u16, coord: PackedCellCoordinate, state: u8) {
        if self.is_dense(sheet_id, coord) {
            let (page, slot) = page_slot(coord.row(), coord.col());
            let words = self
                .pages
                .get_or_insert_with(page, || Box::new([0; PAGE_CELLS / 32]));
            let shift = (slot & 31) * 2;
            let word = &mut words[slot >> 5];
            *word = (*word & !(3u64 << shift)) | (u64::from(state & 3) << shift);
        } else {
            self.overflow
                .insert(PackedWorkbookCoord::new(sheet_id, coord).0, state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coord(row: u32, col: u32) -> PackedCellCoordinate {
        PackedCellCoordinate::new(row, col).unwrap()
    }

    #[test]
    fn memo_dense_and_overflow_cells_round_trip_independently() {
        let mut memo = MemoTable::new(7);
        let cases = [
            (7u16, 0u32, 0u32),
            (7, 70_000, 15),
            (7, 3, 16),
            (7, 9, 300),
            (9, 0, 0),
        ];
        for (i, &(sid, r, c)) in cases.iter().enumerate() {
            assert!(!memo.contains_at(sid, coord(r, c)));
            memo.insert_at(sid, coord(r, c), EvaluatedCellValue::Number(i as f64));
        }
        for (i, &(sid, r, c)) in cases.iter().enumerate() {
            assert_eq!(
                memo.get_at(sid, coord(r, c)),
                Some(&EvaluatedCellValue::Number(i as f64))
            );
            let key = PackedWorkbookCoord::new(sid, coord(r, c));
            assert_eq!(memo.get(&key), memo.get_at(sid, coord(r, c)));
        }
        assert!(memo.get_at(7, coord(1, 0)).is_none());
        assert!(memo.get_at(9, coord(1, 0)).is_none());
    }

    #[test]
    fn visiting_set_detects_reentry_and_clears_on_remove() {
        let mut memo = MemoTable::new(1);
        for (sid, r, c) in [(1u16, 5u32, 2u32), (1, 99_999, 15), (1, 4, 40), (2, 5, 2)] {
            assert!(memo.visit_insert(sid, coord(r, c)));
            assert!(!memo.visit_insert(sid, coord(r, c)));
            memo.visit_remove(sid, coord(r, c));
            assert!(memo.visit_insert(sid, coord(r, c)));
        }
    }

    #[test]
    fn dfs_state_keeps_two_bits_per_cell_without_bleeding() {
        let mut state = DfsState::new(3);
        for (sid, r, c) in [
            (3u16, 0u32, 0u32),
            (3, 0, 1),
            (3, 31, 15),
            (3, 32, 0),
            (3, 8, 20),
            (4, 0, 0),
        ] {
            assert_eq!(state.get(sid, coord(r, c)), 0);
        }
        state.set(3, coord(0, 1), 1);
        state.set(3, coord(31, 15), 2);
        state.set(3, coord(8, 20), 2);
        state.set(4, coord(0, 0), 1);
        assert_eq!(state.get(3, coord(0, 0)), 0);
        assert_eq!(state.get(3, coord(0, 1)), 1);
        assert_eq!(state.get(3, coord(31, 15)), 2);
        assert_eq!(state.get(3, coord(32, 0)), 0);
        assert_eq!(state.get(3, coord(8, 20)), 2);
        assert_eq!(state.get(4, coord(0, 0)), 1);
        state.set(3, coord(0, 1), 2);
        assert_eq!(state.get(3, coord(0, 1)), 2);
    }

    #[test]
    fn high_row_memo_and_dfs_bound_outer_directory() {
        let mut memo = MemoTable::new(1);
        let mut state = DfsState::new(1);
        let high = coord(PackedCellCoordinate::MAX_ROWS - 1, 0);
        memo.insert_at(1, high, EvaluatedCellValue::Number(99.0));
        state.set(1, high, 2);
        assert!(
            memo.pages.len() <= 256,
            "memo directory len {}",
            memo.pages.len()
        );
        assert!(
            state.pages.len() <= 256,
            "dfs directory len {}",
            state.pages.len()
        );
        assert_eq!(memo.pages.allocated_groups(), 1);
        assert_eq!(state.pages.allocated_groups(), 1);
        assert_eq!(
            memo.get_at(1, high),
            Some(&EvaluatedCellValue::Number(99.0))
        );
        assert_eq!(state.get(1, high), 2);
        assert!(memo.visit_insert(1, high));
        assert!(memo.is_visiting(1, high));
        memo.visit_remove(1, high);
        assert!(!memo.is_visiting(1, high));
        assert_eq!(
            memo.remove_at(1, high),
            Some(EvaluatedCellValue::Number(99.0))
        );
        assert!(memo.get_at(1, high).is_none());

        // Crafted row >= MAX_ROWS must fail safely to overflow without panicking or growing groups.
        let oob = PackedCellCoordinate(u64::from(PackedCellCoordinate::MAX_ROWS + 500) << 32);
        memo.insert_at(1, oob, EvaluatedCellValue::Number(123.0));
        state.set(1, oob, 1);
        assert_eq!(memo.pages.len(), 256);
        assert_eq!(
            memo.get_at(1, oob),
            Some(&EvaluatedCellValue::Number(123.0))
        );
        assert_eq!(state.get(1, oob), 1);
    }
}
