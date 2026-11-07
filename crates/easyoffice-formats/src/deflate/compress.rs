//! DEFLATE compressor: hash-chain LZ77 matcher plus per-block dynamic Huffman coding.
//!
//! The design (single pass over the input, small bounded hash chains, symbol statistics gathered
//! while matching, a Huffman code built per block, and the cheapest of stored / fixed / dynamic
//! block types chosen from the exact bit costs) follows the fast compression levels of
//! miniz_oxide 0.8.9 (MIT OR Zlib OR Apache-2.0, https://github.com/Frommi/miniz_oxide, itself
//! derived from miniz by Rich Geldreich). The code here is an independent safe-Rust
//! implementation of that approach; see THIRD_PARTY_NOTICES.md for the licence text.
//!
//! No `unsafe`. All buffers are reserved before the matching loop; the loop only indexes
//! pre-sized arrays and appends to pre-reserved vectors.

use super::{
    find_dist_code, find_length_code, reverse_bits, CL_ORDER, DIST_BASE, DIST_EXTRA, LENGTH_BASE,
    LENGTH_EXTRA, MAX_MATCH_LEN, MIN_MATCH_LEN, WINDOW_SIZE,
};

/// Compression effort: hash-chain probes per position and whether one-step lazy matching is used.
#[derive(Clone, Copy)]
struct LevelParams {
    max_chain: usize,
    /// Matches at least this long are taken immediately (no lazy lookahead, chain search stops).
    nice_len: usize,
    lazy: bool,
}

/// Level table: index 0 is level 1 (fastest), the last entry is the smallest output.
const LEVELS: [LevelParams; 4] = [
    LevelParams {
        max_chain: 1,
        nice_len: 32,
        lazy: false,
    },
    LevelParams {
        max_chain: 8,
        nice_len: 64,
        lazy: false,
    },
    LevelParams {
        max_chain: 8,
        nice_len: 64,
        lazy: true,
    },
    LevelParams {
        max_chain: 16,
        nice_len: 128,
        lazy: true,
    },
];

/// Level used by `deflate_raw` / `deflate_zlib` (1-based index into `LEVELS`).
pub(super) const DEFAULT_LEVEL: u8 = 2;

const MAX_CODE_LEN: u8 = 15;
const MAX_CL_CODE_LEN: u8 = 7;
const LIT_LEN_SYMBOLS: usize = 288;
const DIST_SYMBOLS: usize = 30;
const CL_SYMBOLS: usize = 19;
const END_OF_BLOCK: usize = 256;
const LENGTH_SYMBOL_BASE: usize = 257;
/// Tokens (literals + matches) gathered before a block is flushed.
const BLOCK_TOKENS: usize = 1 << 15;
const STORED_MAX: usize = 65_535;
const MIN_HASH_BITS: u32 = 8;
const MAX_HASH_BITS: u32 = 15;
const HASH_MULTIPLIER: u32 = 0x9E37_79B1;
/// Positions seeded at each end of a long match.
const INSERT_EDGE: usize = 8;
const NO_POS: u32 = u32::MAX;
const MATCH_FLAG: u32 = 1 << 31;
const MAX_LEVEL_INDEX: u8 = LEVELS.len() as u8;

/// LSB-first bit sink with a 64-bit accumulator (at most 15 bits are written per call).
struct BitSink {
    out: Vec<u8>,
    buf: u64,
    count: u32,
}

impl BitSink {
    fn with_capacity(cap: usize) -> Self {
        Self {
            out: Vec::with_capacity(cap),
            buf: 0,
            count: 0,
        }
    }

    #[inline]
    fn put(&mut self, value: u32, bits: u32) {
        self.buf |= u64::from(value) << self.count;
        self.count += bits;
        if self.count >= 32 {
            self.out.extend_from_slice(&(self.buf as u32).to_le_bytes());
            self.buf >>= 32;
            self.count -= 32;
        }
    }

    fn align_to_byte(&mut self) {
        while self.count > 0 {
            self.out.push(self.buf as u8);
            self.buf >>= 8;
            self.count = self.count.saturating_sub(8);
        }
        self.buf = 0;
    }

    fn put_bytes(&mut self, bytes: &[u8]) {
        self.out.extend_from_slice(bytes);
    }

    fn finish(mut self) -> Vec<u8> {
        self.align_to_byte();
        self.out
    }
}

/// A prepared Huffman code: per-symbol bit-reversed code and length.
struct CodeTable {
    codes: Vec<u16>,
    lens: Vec<u8>,
}

impl CodeTable {
    fn from_lengths(lens: &[u8]) -> Self {
        let mut bl_count = [0u16; MAX_CODE_LEN as usize + 1];
        for &l in lens {
            bl_count[usize::from(l)] += 1;
        }
        bl_count[0] = 0;
        let mut next_code = [0u16; MAX_CODE_LEN as usize + 2];
        let mut code = 0u16;
        for bits in 1..=usize::from(MAX_CODE_LEN) {
            code = (code + bl_count[bits - 1]) << 1;
            next_code[bits] = code;
        }
        let codes = lens
            .iter()
            .map(|&l| {
                if l == 0 {
                    return 0;
                }
                let c = next_code[usize::from(l)];
                next_code[usize::from(l)] += 1;
                reverse_bits(c, l)
            })
            .collect();
        Self {
            codes,
            lens: lens.to_vec(),
        }
    }

    #[inline]
    fn put(&self, sink: &mut BitSink, sym: usize) {
        sink.put(u32::from(self.codes[sym]), u32::from(self.lens[sym]));
    }
}

fn fixed_lit_lengths() -> [u8; LIT_LEN_SYMBOLS] {
    let mut lens = [8u8; LIT_LEN_SYMBOLS];
    for l in lens.iter_mut().take(256).skip(144) {
        *l = 9;
    }
    for l in lens.iter_mut().take(280).skip(256) {
        *l = 7;
    }
    lens
}

/// Code lengths (at most `limit`) for `freqs`; unused symbols get length 0.
/// At least two symbols are always coded so the resulting code is complete.
fn build_code_lengths(freqs: &[u32], limit: u8, lens: &mut [u8]) {
    lens.iter_mut().for_each(|l| *l = 0);
    let mut sorted: Vec<(u32, u16)> = freqs
        .iter()
        .enumerate()
        .filter(|(_, &f)| f > 0)
        .map(|(sym, &f)| (f, sym as u16))
        .collect();
    if sorted.len() < 2 {
        let first = sorted.first().map_or(0, |&(_, s)| usize::from(s));
        lens[first] = 1;
        lens[usize::from(first == 0)] = 1;
        return;
    }
    sorted.sort_unstable();
    let n = sorted.len();
    // Two-queue Huffman construction over the ascending leaf weights.
    let mut weight: Vec<u64> = Vec::with_capacity(2 * n);
    weight.extend(sorted.iter().map(|&(f, _)| u64::from(f)));
    let mut parent = vec![0usize; 2 * n];
    let mut leaf = 0usize;
    let mut node = n;
    for _ in 0..n - 1 {
        let mut picked = [0usize; 2];
        for slot in &mut picked {
            let take_leaf = leaf < n && (node >= weight.len() || weight[leaf] <= weight[node]);
            if take_leaf {
                *slot = leaf;
                leaf += 1;
            } else {
                *slot = node;
                node += 1;
            }
        }
        weight.push(weight[picked[0]] + weight[picked[1]]);
        let new_idx = weight.len() - 1;
        parent[picked[0]] = new_idx;
        parent[picked[1]] = new_idx;
    }
    let root = weight.len() - 1;
    let mut depth = vec![0u8; weight.len()];
    for i in (0..root).rev() {
        depth[i] = depth[parent[i]].saturating_add(1);
    }
    // Count codes per length, then squeeze everything into `limit` bits (Kraft repair).
    let mut num_codes = [0u32; 64];
    for &d in &depth[..n] {
        num_codes[usize::from(d).min(63)] += 1;
    }
    enforce_max_code_len(&mut num_codes, usize::from(limit));
    // Longest codes go to the rarest symbols (`sorted` is ascending by frequency).
    let mut idx = 0usize;
    for len in (1..=usize::from(limit)).rev() {
        for _ in 0..num_codes[len] {
            lens[usize::from(sorted[idx].1)] = len as u8;
            idx += 1;
        }
    }
}

/// Folds code lengths above `limit` into `limit` and repairs the Kraft sum to exactly 1.
fn enforce_max_code_len(num_codes: &mut [u32; 64], limit: usize) {
    for i in limit + 1..num_codes.len() {
        num_codes[limit] += num_codes[i];
        num_codes[i] = 0;
    }
    let mut total: u64 = (1..=limit)
        .map(|i| u64::from(num_codes[i]) << (limit - i))
        .sum();
    let target = 1u64 << limit;
    while total > target {
        num_codes[limit] -= 1;
        for i in (1..limit).rev() {
            if num_codes[i] != 0 {
                num_codes[i] -= 1;
                num_codes[i + 1] += 2;
                break;
            }
        }
        total -= 1;
    }
}

/// Run-length encoded code-length stream: `(symbol, extra value)` pairs.
fn rle_code_lengths(lens: &[u8], out: &mut Vec<(u8, u8)>) {
    out.clear();
    let mut i = 0usize;
    while i < lens.len() {
        let value = lens[i];
        let mut run = 1usize;
        while i + run < lens.len() && lens[i + run] == value {
            run += 1;
        }
        i += run;
        if value == 0 {
            while run >= 11 {
                let n = run.min(138);
                out.push((18, (n - 11) as u8));
                run -= n;
            }
            if run >= 3 {
                out.push((17, (run - 3) as u8));
                run = 0;
            }
            out.extend(std::iter::repeat_n((0u8, 0u8), run));
        } else {
            out.push((value, 0));
            run -= 1;
            while run >= 3 {
                let n = run.min(6);
                out.push((16, (n - 3) as u8));
                run -= n;
            }
            out.extend(std::iter::repeat_n((value, 0u8), run));
        }
    }
}

const CL_EXTRA_BITS: [u32; CL_SYMBOLS] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 3, 7];

/// Everything needed to emit (and cost) a dynamic block header.
struct DynamicHeader {
    lit_lens: [u8; LIT_LEN_SYMBOLS],
    dist_lens: [u8; DIST_SYMBOLS],
    cl_lens: [u8; CL_SYMBOLS],
    rle: Vec<(u8, u8)>,
    hlit: usize,
    hdist: usize,
    hclen: usize,
}

impl DynamicHeader {
    fn build(lit_freq: &[u32; LIT_LEN_SYMBOLS], dist_freq: &[u32; DIST_SYMBOLS]) -> Self {
        let mut lit_lens = [0u8; LIT_LEN_SYMBOLS];
        let mut dist_lens = [0u8; DIST_SYMBOLS];
        build_code_lengths(lit_freq, MAX_CODE_LEN, &mut lit_lens);
        build_code_lengths(dist_freq, MAX_CODE_LEN, &mut dist_lens);
        let hlit = lit_lens
            .iter()
            .rposition(|&l| l != 0)
            .map_or(257, |p| (p + 1).max(257));
        let hdist = dist_lens.iter().rposition(|&l| l != 0).map_or(1, |p| p + 1);
        let mut combined = [0u8; LIT_LEN_SYMBOLS + DIST_SYMBOLS];
        combined[..hlit].copy_from_slice(&lit_lens[..hlit]);
        combined[hlit..hlit + hdist].copy_from_slice(&dist_lens[..hdist]);
        let mut rle = Vec::with_capacity(hlit + hdist);
        rle_code_lengths(&combined[..hlit + hdist], &mut rle);
        let mut cl_freq = [0u32; CL_SYMBOLS];
        for &(sym, _) in &rle {
            cl_freq[usize::from(sym)] += 1;
        }
        let mut cl_lens = [0u8; CL_SYMBOLS];
        build_code_lengths(&cl_freq, MAX_CL_CODE_LEN, &mut cl_lens);
        let hclen = CL_ORDER
            .iter()
            .rposition(|&o| cl_lens[o] != 0)
            .map_or(4, |p| (p + 1).max(4));
        Self {
            lit_lens,
            dist_lens,
            cl_lens,
            rle,
            hlit,
            hdist,
            hclen,
        }
    }

    /// Header size in bits (excluding the 3 block-type bits).
    fn bit_cost(&self) -> u64 {
        let mut bits = 5 + 5 + 4 + 3 * self.hclen as u64;
        for &(sym, _) in &self.rle {
            bits += u64::from(self.cl_lens[usize::from(sym)])
                + u64::from(CL_EXTRA_BITS[usize::from(sym)]);
        }
        bits
    }

    fn write(&self, sink: &mut BitSink) {
        sink.put((self.hlit - 257) as u32, 5);
        sink.put((self.hdist - 1) as u32, 5);
        sink.put((self.hclen - 4) as u32, 4);
        for &o in CL_ORDER.iter().take(self.hclen) {
            sink.put(u32::from(self.cl_lens[o]), 3);
        }
        let cl = CodeTable::from_lengths(&self.cl_lens);
        for &(sym, extra) in &self.rle {
            cl.put(sink, usize::from(sym));
            let bits = CL_EXTRA_BITS[usize::from(sym)];
            if bits > 0 {
                sink.put(u32::from(extra), bits);
            }
        }
    }
}

/// Bit cost of coding the symbol statistics with the given code lengths.
fn symbol_cost(freq: &[u32], lens: &[u8]) -> u64 {
    freq.iter()
        .zip(lens)
        .map(|(&f, &l)| u64::from(f) * u64::from(l))
        .sum()
}

#[inline]
fn token_literal(byte: u8) -> u32 {
    u32::from(byte)
}

#[inline]
fn token_match(len: usize, dist: usize) -> u32 {
    MATCH_FLAG | (((len - MIN_MATCH_LEN) as u32) << 16) | ((dist - 1) as u32)
}

/// Length of the common prefix of `input[a..]` and `input[b..]`, capped at `max_len`
/// (eight bytes per step).
#[inline]
fn common_prefix_len(input: &[u8], a: usize, b: usize, max_len: usize) -> usize {
    let mut n = 0usize;
    while n + 8 <= max_len {
        let x = u64::from_le_bytes(input[a + n..a + n + 8].try_into().unwrap());
        let y = u64::from_le_bytes(input[b + n..b + n + 8].try_into().unwrap());
        let diff = x ^ y;
        if diff == 0 {
            n += 8;
        } else {
            return n + (diff.trailing_zeros() >> 3) as usize;
        }
    }
    while n < max_len && input[a + n] == input[b + n] {
        n += 1;
    }
    n
}

/// Per-thread working buffers reused across calls so small inputs do not pay for fresh
/// multi-hundred-KiB allocations (and the page faults that come with them) every time.
#[derive(Default)]
struct Scratch {
    head: Vec<u32>,
    prev: Vec<u32>,
    tokens: Vec<u32>,
}

thread_local! {
    static SCRATCH: std::cell::RefCell<Scratch> = std::cell::RefCell::new(Scratch::default());
}

struct Encoder<'a> {
    input: &'a [u8],
    params: LevelParams,
    head: Vec<u32>,
    prev: Vec<u32>,
    hash_shift: u32,
    window_mask: usize,
    tokens: Vec<u32>,
    lit_freq: [u32; LIT_LEN_SYMBOLS],
    dist_freq: [u32; DIST_SYMBOLS],
    extra_bits: u64,
    block_start: usize,
    sink: BitSink,
}

impl<'a> Encoder<'a> {
    fn new(input: &'a [u8], params: LevelParams, scratch: Scratch) -> Self {
        let len = input.len();
        let hash_bits =
            (usize::BITS - len.max(1).leading_zeros()).clamp(MIN_HASH_BITS, MAX_HASH_BITS);
        let ring = len.next_power_of_two().clamp(256, WINDOW_SIZE);
        let Scratch {
            mut head,
            mut prev,
            mut tokens,
        } = scratch;
        head.clear();
        head.resize(1usize << hash_bits, NO_POS);
        prev.clear();
        prev.resize(ring, NO_POS);
        tokens.clear();
        tokens.reserve(BLOCK_TOKENS.min(len + 1));
        Self {
            input,
            params,
            head,
            prev,
            hash_shift: 32 - hash_bits,
            window_mask: ring - 1,
            tokens,
            lit_freq: [0; LIT_LEN_SYMBOLS],
            dist_freq: [0; DIST_SYMBOLS],
            extra_bits: 0,
            block_start: 0,
            sink: BitSink::with_capacity(len / 2 + 64),
        }
    }

    /// Hash of the three bytes at `pos` (requires `pos + 3 <= len`).
    #[inline]
    fn hash_at(&self, pos: usize) -> usize {
        let v = match self.input.get(pos..pos + 4) {
            Some(w) => u32::from_le_bytes([w[0], w[1], w[2], w[3]]) & 0x00FF_FFFF,
            None => {
                u32::from(self.input[pos])
                    | (u32::from(self.input[pos + 1]) << 8)
                    | (u32::from(self.input[pos + 2]) << 16)
            }
        };
        (v.wrapping_mul(HASH_MULTIPLIER) >> self.hash_shift) as usize
    }

    /// Records `pos` in the hash chains and returns the previous head for its hash.
    #[inline]
    fn insert(&mut self, pos: usize, h: usize) -> u32 {
        let first = self.head[h];
        self.prev[pos & self.window_mask] = first;
        self.head[h] = pos as u32;
        first
    }

    /// Best `(length, distance)` for `pos` along the chain starting at `first`
    /// (length 0 when nothing reaches `MIN_MATCH_LEN`).
    #[inline]
    fn find_match(&self, pos: usize, first: u32, floor_len: usize) -> (usize, usize) {
        let input = self.input;
        let max_len = (input.len() - pos).min(MAX_MATCH_LEN);
        let mut best_len = floor_len.max(MIN_MATCH_LEN - 1);
        let mut best_dist = 0usize;
        let mut cand = first;
        let mut steps = 0usize;
        while cand != NO_POS && steps < self.params.max_chain {
            let c_pos = cand as usize;
            let dist = pos - c_pos;
            if dist == 0 || dist > WINDOW_SIZE {
                break;
            }
            if best_len < max_len && input[c_pos + best_len] == input[pos + best_len] {
                let m = common_prefix_len(input, c_pos, pos, max_len);
                if m > best_len {
                    best_len = m;
                    best_dist = dist;
                    if m >= self.params.nice_len || m == max_len {
                        break;
                    }
                }
            }
            let next = self.prev[c_pos & self.window_mask];
            if next >= cand {
                break;
            }
            cand = next;
            steps += 1;
        }
        if best_dist == 0 {
            (0, 0)
        } else {
            (best_len, best_dist)
        }
    }

    #[inline]
    fn push_literal(&mut self, byte: u8) {
        self.tokens.push(token_literal(byte));
        self.lit_freq[usize::from(byte)] += 1;
    }

    #[inline]
    fn push_match(&mut self, len: usize, dist: usize) {
        self.tokens.push(token_match(len, dist));
        let l = find_length_code(len);
        let d = find_dist_code(dist);
        self.lit_freq[LENGTH_SYMBOL_BASE + l] += 1;
        self.dist_freq[d] += 1;
        self.extra_bits += u64::from(LENGTH_EXTRA[l]) + u64::from(DIST_EXTRA[d]);
    }

    /// Inserts the positions covered by a match (positions with fewer than three bytes left are
    /// skipped). Long matches only seed their head and tail: the interior of a long repeat adds
    /// little matching power but costs one hash insertion per byte.
    #[inline]
    fn insert_range(&mut self, start: usize, end: usize) {
        let last = self.input.len().saturating_sub(MIN_MATCH_LEN - 1);
        let end = end.min(last);
        if end <= start {
            return;
        }
        if end - start > 2 * INSERT_EDGE {
            self.insert_span(start, start + INSERT_EDGE);
            self.insert_span(end - INSERT_EDGE, end);
        } else {
            self.insert_span(start, end);
        }
    }

    #[inline]
    fn insert_span(&mut self, start: usize, end: usize) {
        for p in start..end {
            let h = self.hash_at(p);
            self.insert(p, h);
        }
    }

    fn run(mut self) -> (Vec<u8>, Scratch) {
        let len = self.input.len();
        let hashable_end = len.saturating_sub(MIN_MATCH_LEN - 1);
        let mut pos = 0usize;
        let mut pending: (usize, usize) = (0, 0);
        while pos < len {
            let (mut m_len, mut m_dist) = pending;
            pending = (0, 0);
            if pos < hashable_end {
                let h = self.hash_at(pos);
                let first = self.insert(pos, h);
                if m_len == 0 {
                    (m_len, m_dist) = self.find_match(pos, first, 0);
                }
                if self.params.lazy
                    && m_len >= MIN_MATCH_LEN
                    && m_len < self.params.nice_len
                    && pos + 1 < hashable_end
                {
                    let h2 = self.hash_at(pos + 1);
                    let first2 = self.head[h2];
                    let next = self.find_match(pos + 1, first2, m_len);
                    if next.0 > m_len {
                        self.push_literal(self.input[pos]);
                        pending = next;
                        pos += 1;
                        self.flush_if_full(pos);
                        continue;
                    }
                }
            }
            if m_len >= MIN_MATCH_LEN {
                self.push_match(m_len, m_dist);
                self.insert_range(pos + 1, pos + m_len);
                pos += m_len;
            } else {
                self.push_literal(self.input[pos]);
                pos += 1;
            }
            self.flush_if_full(pos);
        }
        self.flush_block(len, true);
        let scratch = Scratch {
            head: self.head,
            prev: self.prev,
            tokens: self.tokens,
        };
        (self.sink.finish(), scratch)
    }

    #[inline]
    fn flush_if_full(&mut self, pos: usize) {
        if self.tokens.len() >= BLOCK_TOKENS {
            self.flush_block(pos, false);
        }
    }

    /// Emits the pending tokens as the cheapest of stored / fixed / dynamic blocks.
    fn flush_block(&mut self, end: usize, last: bool) {
        self.lit_freq[END_OF_BLOCK] = 1;
        let fixed_lit = fixed_lit_lengths();
        let fixed_dist = [5u8; DIST_SYMBOLS];
        let header = DynamicHeader::build(&self.lit_freq, &self.dist_freq);

        let body_extra = self.extra_bits;
        let fixed_bits = 3
            + symbol_cost(&self.lit_freq, &fixed_lit)
            + symbol_cost(&self.dist_freq, &fixed_dist)
            + body_extra;
        let dynamic_bits = 3
            + header.bit_cost()
            + symbol_cost(&self.lit_freq, &header.lit_lens)
            + symbol_cost(&self.dist_freq, &header.dist_lens)
            + body_extra;
        let span = end - self.block_start;
        let stored_bits = if span <= STORED_MAX {
            3 + 7 + 32 + 8 * span as u64
        } else {
            u64::MAX
        };

        if stored_bits <= fixed_bits && stored_bits <= dynamic_bits {
            self.write_stored(self.block_start, end, last);
        } else if fixed_bits <= dynamic_bits {
            self.sink.put(u32::from(last) | (1 << 1), 3);
            let lit = CodeTable::from_lengths(&fixed_lit);
            let dist = CodeTable::from_lengths(&fixed_dist);
            self.write_tokens(&lit, &dist);
        } else {
            self.sink.put(u32::from(last) | (2 << 1), 3);
            header.write(&mut self.sink);
            let lit = CodeTable::from_lengths(&header.lit_lens);
            let dist = CodeTable::from_lengths(&header.dist_lens);
            self.write_tokens(&lit, &dist);
        }
        self.tokens.clear();
        self.lit_freq = [0; LIT_LEN_SYMBOLS];
        self.dist_freq = [0; DIST_SYMBOLS];
        self.extra_bits = 0;
        self.block_start = end;
    }

    fn write_stored(&mut self, start: usize, end: usize, last: bool) {
        self.sink.put(u32::from(last), 3);
        self.sink.align_to_byte();
        let n = (end - start) as u16;
        self.sink.put_bytes(&n.to_le_bytes());
        self.sink.put_bytes(&(!n).to_le_bytes());
        self.sink.put_bytes(&self.input[start..end]);
    }

    fn write_tokens(&mut self, lit: &CodeTable, dist: &CodeTable) {
        for &t in &self.tokens {
            if t & MATCH_FLAG == 0 {
                lit.put(&mut self.sink, t as usize);
                continue;
            }
            let len = ((t >> 16) & 0xFF) as usize + MIN_MATCH_LEN;
            let d = (t & 0xFFFF) as usize + 1;
            let l_idx = find_length_code(len);
            lit.put(&mut self.sink, LENGTH_SYMBOL_BASE + l_idx);
            let l_extra = u32::from(LENGTH_EXTRA[l_idx]);
            if l_extra > 0 {
                self.sink
                    .put((len - usize::from(LENGTH_BASE[l_idx])) as u32, l_extra);
            }
            let d_idx = find_dist_code(d);
            dist.put(&mut self.sink, d_idx);
            let d_extra = u32::from(DIST_EXTRA[d_idx]);
            if d_extra > 0 {
                self.sink
                    .put((d - usize::from(DIST_BASE[d_idx])) as u32, d_extra);
            }
        }
        lit.put(&mut self.sink, END_OF_BLOCK);
    }
}

/// Compresses `input` into a raw DEFLATE stream at `level` (1 = fastest .. 4 = smallest).
pub(super) fn compress(input: &[u8], level: u8) -> Vec<u8> {
    let idx = usize::from(level.clamp(1, MAX_LEVEL_INDEX)) - 1;
    let scratch = SCRATCH.with(|cell| std::mem::take(&mut *cell.borrow_mut()));
    let (out, scratch) = Encoder::new(input, LEVELS[idx], scratch).run();
    SCRATCH.with(|cell| *cell.borrow_mut() = scratch);
    out
}
