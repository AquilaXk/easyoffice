//! Interval index for range dependencies: which registered row intervals contain a point.
//!
//! 참조 구현: formualizer (MIT OR Apache-2.0), Copyright (c) 2025-2026 Frank Colson and
//! Formualizer contributors, `crates/formualizer-eval/src/engine/interval_tree.rs` (`IntervalTree`:
//! `insert`, `query`, point-interval lookups). The reference keeps a `BTreeMap` keyed by the low end
//! and answers `query` by scanning every entry whose low end is at or below the query, which is
//! linear in the number of registered intervals. Changes: entries are keyed `(low, id)` with the
//! high end as the value, split into power-of-two length classes so a point query scans, per class,
//! only the window of low ends that can still reach the point (`O(classes * log n + candidates)`),
//! a bit mask skips empty classes, and values are `u32` ids instead of hash sets. Source and
//! license text: `THIRD_PARTY_NOTICES.md`.

use std::collections::BTreeMap;

/// Longest interval is the whole row axis (2^20 rows), so classes `0..=20` cover every length.
const MAX_CLASSES: usize = 21;

#[derive(Debug, Default, Clone)]
pub(crate) struct IntervalIndex {
    /// `classes[k]` holds the intervals whose length is in `[2^k, 2^(k+1))`, keyed `(low, id)`.
    classes: Vec<BTreeMap<(u32, u32), u32>>,
    /// Bit `k` is set while `classes[k]` is non-empty.
    mask: u32,
    len: usize,
}

#[inline]
fn class_of(low: u32, high: u32) -> usize {
    let len = high - low + 1;
    (31 - len.leading_zeros()) as usize
}

impl IntervalIndex {
    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Registers the inclusive interval `[low, high]` under `id` (`low <= high`).
    pub(crate) fn insert(&mut self, low: u32, high: u32, id: u32) {
        debug_assert!(low <= high);
        let class = class_of(low, high);
        debug_assert!(class < MAX_CLASSES);
        if self.classes.len() <= class {
            self.classes.resize_with(class + 1, BTreeMap::new);
        }
        if self.classes[class].insert((low, id), high).is_none() {
            self.len += 1;
        }
        self.mask |= 1 << class;
    }

    pub(crate) fn remove(&mut self, low: u32, high: u32, id: u32) {
        let class = class_of(low, high);
        let Some(map) = self.classes.get_mut(class) else {
            return;
        };
        if map.remove(&(low, id)).is_some() {
            self.len -= 1;
        }
        if map.is_empty() {
            self.mask &= !(1 << class);
        }
    }

    /// Calls `f(id)` for every interval that contains `point`.
    #[inline]
    pub(crate) fn stab(&self, point: u32, mut f: impl FnMut(u32)) {
        let mut mask = self.mask;
        while mask != 0 {
            let class = mask.trailing_zeros() as usize;
            mask &= mask - 1;
            // Length < 2^(class+1), so an interval reaching `point` starts after point - 2^(class+1).
            let first_low = point.saturating_sub((2u32 << class) - 2);
            for (&(_, id), &high) in self.classes[class].range((first_low, 0)..=(point, u32::MAX)) {
                if high >= point {
                    f(id);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xorshift(x: &mut u64) -> u64 {
        *x ^= *x << 13;
        *x ^= *x >> 7;
        *x ^= *x << 17;
        *x
    }

    #[test]
    fn stab_matches_a_linear_scan_under_random_inserts_and_removals() {
        let mut index = IntervalIndex::default();
        let mut oracle: Vec<(u32, u32, u32)> = Vec::new();
        let mut x = 0x2545_F491_4F6C_DD1Du64;
        let mut next_id = 0u32;
        for step in 0..6_000 {
            let r = xorshift(&mut x);
            if r % 4 != 0 || oracle.is_empty() {
                let low = (xorshift(&mut x) % 2_000) as u32;
                let span = match xorshift(&mut x) % 4 {
                    0 => 0,
                    1 => xorshift(&mut x) % 8,
                    2 => xorshift(&mut x) % 300,
                    _ => xorshift(&mut x) % 1_048_000,
                } as u32;
                let high = (low + span).min(1_048_575);
                index.insert(low, high, next_id);
                oracle.push((low, high, next_id));
                next_id += 1;
            } else {
                let pos = (xorshift(&mut x) % oracle.len() as u64) as usize;
                let (low, high, id) = oracle.swap_remove(pos);
                index.remove(low, high, id);
            }
            if step % 50 == 0 {
                for point in [0, 1, 7, 255, 256, 999, 2_000, 2_300, 600_000, 1_048_575] {
                    let mut got = Vec::new();
                    index.stab(point, |id| got.push(id));
                    got.sort_unstable();
                    let mut want: Vec<u32> = oracle
                        .iter()
                        .filter(|&&(l, h, _)| l <= point && point <= h)
                        .map(|&(_, _, id)| id)
                        .collect();
                    want.sort_unstable();
                    assert_eq!(got, want, "point {point} at step {step}");
                }
            }
        }
        assert_eq!(index.len, oracle.len());
        for &(l, h, id) in &oracle.clone() {
            index.remove(l, h, id);
        }
        assert!(index.is_empty() && index.mask == 0);
    }
}
