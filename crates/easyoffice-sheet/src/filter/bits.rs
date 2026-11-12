//! One bit per data row of a filter range: set means the row is hidden. Combining the conditions
//! of several columns and turning the result into runs works a 64-row word at a time.

/// Hidden flags of `len` rows, row `i` being bit `i % 64` of word `i / 64`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowBits {
    words: Vec<u64>,
    len: usize,
}

impl RowBits {
    pub fn new(len: usize) -> Self {
        Self {
            words: vec![0; len.div_ceil(64)],
            len,
        }
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub fn get(&self, i: usize) -> bool {
        self.words
            .get(i / 64)
            .is_some_and(|w| (w >> (i % 64)) & 1 == 1)
    }

    #[inline]
    pub fn set(&mut self, i: usize) {
        if i < self.len {
            self.words[i / 64] |= 1 << (i % 64);
        }
    }

    pub fn count(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Adds the rows of `other` (the same length).
    pub fn union_with(&mut self, other: &Self) {
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a |= *b;
        }
    }

    /// Sets the rows of the inclusive `runs` (zero-based, relative to the first row of the
    /// bits), ignoring what lies past the end.
    pub fn set_runs(&mut self, runs: impl IntoIterator<Item = (usize, usize)>) {
        for (start, end) in runs {
            let end = end.min(self.len.saturating_sub(1));
            if start > end {
                continue;
            }
            let (first, last) = (start / 64, end / 64);
            let head = !0u64 << (start % 64);
            let tail = !0u64 >> (63 - end % 64);
            if first == last {
                self.words[first] |= head & tail;
            } else {
                self.words[first] |= head;
                self.words[first + 1..last].fill(!0);
                self.words[last] |= tail;
            }
        }
    }

    /// First row at or after `from` whose bit equals `value`, or `len` when there is none.
    fn next(&self, from: usize, value: bool) -> usize {
        let mut w = from / 64;
        let Some(&first) = self.words.get(w) else {
            return self.len;
        };
        let flip = if value { 0 } else { !0u64 };
        let mut masked = (first ^ flip) & (!0u64 << (from % 64));
        loop {
            if masked != 0 {
                return (w * 64 + masked.trailing_zeros() as usize).min(self.len);
            }
            w += 1;
            match self.words.get(w) {
                Some(&word) => masked = word ^ flip,
                None => return self.len,
            }
        }
    }

    /// The set rows as sorted, disjoint, non-adjacent inclusive runs, each shifted by `base`.
    pub fn runs(&self, base: u32) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        let mut at = 0usize;
        while at < self.len {
            let start = self.next(at, true);
            if start >= self.len {
                break;
            }
            let end = self.next(start, false);
            out.push((base + start as u32, base + end as u32 - 1));
            at = end;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naive(bits: &RowBits, base: u32) -> Vec<(u32, u32)> {
        let mut out: Vec<(u32, u32)> = Vec::new();
        for i in 0..bits.len() {
            if bits.get(i) {
                let row = base + i as u32;
                match out.last_mut() {
                    Some(last) if last.1 + 1 == row => last.1 = row,
                    _ => out.push((row, row)),
                }
            }
        }
        out
    }

    #[test]
    fn runs_cross_word_edges_and_match_a_row_by_row_scan() {
        let mut bits = RowBits::new(300);
        bits.set_runs([
            (0, 0),
            (3, 70),
            (72, 72),
            (127, 128),
            (190, 255),
            (299, 299),
        ]);
        assert_eq!(
            bits.runs(10),
            vec![
                (10, 10),
                (13, 80),
                (82, 82),
                (137, 138),
                (200, 265),
                (309, 309)
            ]
        );
        assert_eq!(bits.runs(0), naive(&bits, 0));
        assert_eq!(bits.count(), 1 + 68 + 1 + 2 + 66 + 1);
    }

    #[test]
    fn pseudo_random_patterns_agree_with_the_scan() {
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        for len in [1usize, 63, 64, 65, 200, 1000] {
            for density in [1u64, 3, 8, 60] {
                let mut bits = RowBits::new(len);
                for i in 0..len {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    if seed % 64 < density {
                        bits.set(i);
                    }
                }
                assert_eq!(bits.runs(5), naive(&bits, 5), "len {len} density {density}");
            }
        }
        let mut full = RowBits::new(130);
        full.set_runs([(0, 129)]);
        assert_eq!(full.runs(0), vec![(0, 129)]);
        assert_eq!(RowBits::new(130).runs(0), Vec::new());
    }

    #[test]
    fn union_and_out_of_range_sets() {
        let mut a = RowBits::new(70);
        let mut b = RowBits::new(70);
        a.set(1);
        b.set(69);
        b.set(70);
        a.union_with(&b);
        assert!(a.get(1) && a.get(69) && !a.get(70) && a.count() == 2);
        a.set_runs([(68, 500)]);
        assert_eq!(a.runs(0), vec![(1, 1), (68, 69)]);
    }
}
