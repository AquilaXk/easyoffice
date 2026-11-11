//! Vertex-id bit set: the dirty set and the cyclic set hold up to every formula of a workbook, and
//! their per-id insert/remove/contains sit on the hot path of every recalculation.
//!
//! 참조 구현: formualizer (MIT OR Apache-2.0), Copyright (c) 2025-2026 Frank Colson and
//! Formualizer contributors, `crates/formualizer-eval/src/engine/idset.rs` (`DenseIdSet`, whole
//! file). Changes: ids are plain `u32` instead of the reference's vertex newtype, and ascending
//! iteration is a `for_each` word scan with `trailing_zeros` instead of a boxed iterator. Source
//! and license text: `THIRD_PARTY_NOTICES.md`.

#[derive(Debug, Default, Clone)]
pub(crate) struct IdSet {
    words: Vec<u64>,
    len: usize,
    /// Word range that may hold set bits (`lo > hi` is never read while `len == 0`).
    lo: usize,
    hi: usize,
}

impl IdSet {
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, id: u32) -> bool {
        let (w, b) = ((id >> 6) as usize, id & 63);
        self.words.get(w).is_some_and(|word| word >> b & 1 == 1)
    }

    /// Inserts `id`; whether it was new.
    #[inline]
    pub(crate) fn insert(&mut self, id: u32) -> bool {
        let (w, b) = ((id >> 6) as usize, id & 63);
        if w >= self.words.len() {
            self.words.resize((w + 1).next_power_of_two().max(16), 0);
        }
        let word = &mut self.words[w];
        if *word >> b & 1 == 1 {
            return false;
        }
        *word |= 1 << b;
        if self.len == 0 {
            self.lo = w;
            self.hi = w;
        } else {
            self.lo = self.lo.min(w);
            self.hi = self.hi.max(w);
        }
        self.len += 1;
        true
    }

    /// Removes `id`; whether it was present.
    #[inline]
    pub(crate) fn remove(&mut self, id: u32) -> bool {
        let (w, b) = ((id >> 6) as usize, id & 63);
        let Some(word) = self.words.get_mut(w) else {
            return false;
        };
        if *word >> b & 1 == 0 {
            return false;
        }
        *word &= !(1 << b);
        self.len -= 1;
        true
    }

    /// Removes every id (the words stay allocated); only the touched word range is cleared.
    pub(crate) fn clear(&mut self) {
        if self.len != 0 {
            self.words[self.lo..=self.hi].fill(0);
        }
        self.len = 0;
    }

    /// Visits ids in ascending order.
    pub(crate) fn for_each(&self, mut f: impl FnMut(u32)) {
        if self.len == 0 {
            return;
        }
        for (i, &word) in self.words[self.lo..=self.hi].iter().enumerate() {
            let base = ((self.lo + i) as u32) << 6;
            let mut w = word;
            while w != 0 {
                f(base | w.trailing_zeros());
                w &= w - 1;
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn to_vec(&self) -> Vec<u32> {
        let mut out = Vec::with_capacity(self.len);
        self.for_each(|id| out.push(id));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn id_set_matches_a_btree_set() {
        let mut set = IdSet::default();
        let mut oracle = BTreeSet::new();
        let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
        for step in 0..20_000u32 {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let id = (x % 5_000) as u32 + if step % 3 == 0 { 70_000 } else { 0 };
            match x % 5 {
                0..=2 => assert_eq!(set.insert(id), oracle.insert(id)),
                3 => assert_eq!(set.remove(id), oracle.remove(&id)),
                _ => assert_eq!(set.contains(id), oracle.contains(&id)),
            }
            if step % 4_999 == 0 {
                set.clear();
                oracle.clear();
            }
            assert_eq!(set.len(), oracle.len());
        }
        assert!(set.to_vec().into_iter().eq(oracle.iter().copied()));
        set.clear();
        assert!(set.is_empty() && set.to_vec().is_empty());
    }
}
