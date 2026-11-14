/// Extra bytes kept writable past the logical end so a match can be copied in 8-byte chunks
/// (longest match 258 + 7 bytes of chunk overshoot).
pub(super) const OUT_SLACK: usize = super::MAX_MATCH_LEN + 8;
const MIN_OUT_CAPACITY: usize = 4_096;
/// Most output allocated before any byte is produced. The declared size is untrusted (a ZIP
/// header can claim 512 MiB for a 100-byte stream), so a bigger buffer is only reached by growing
/// with the output that really appears (PD-175).
const MAX_INITIAL_OUT_CAPACITY: usize = 4 * 1024 * 1024;
/// Growth factor while the declared size is still ahead of the buffer.
const HINTED_GROWTH: usize = 4;
/// Theoretical upper bound of RFC 1951 DEFLATE expansion (~1032x per compressed input byte).
const MAX_DEFLATE_EXPANSION: usize = 1_032;
/// Non-overlapping matches longer than this use `memmove` instead of the 8-byte chunk loop.
const LONG_COPY_MIN: usize = 32;

/// Flat output buffer: `buf[..pos]` is the produced data, the rest is writable scratch so the
/// hot loop never goes through `Vec::push` capacity logic.
pub(super) struct OutBuf {
    pub(super) buf: Vec<u8>,
    pos: usize,
    /// Largest `buf.len()` worth allocating (the output budget plus slack).
    hard_cap: usize,
    /// Output size the container declares (0 when unknown); a growth target, never trusted.
    expected_len: usize,
}

impl OutBuf {
    pub(super) fn new(input_len: usize, max_output_bytes: usize, expected_len: usize) -> Self {
        let hard_cap = max_output_bytes.saturating_add(OUT_SLACK);
        let max_possible = input_len
            .saturating_mul(MAX_DEFLATE_EXPANSION)
            .max(MIN_OUT_CAPACITY);
        let guess = if expected_len > 0 {
            expected_len.min(max_possible)
        } else {
            input_len.saturating_mul(8).max(MIN_OUT_CAPACITY)
        };
        let initial = guess
            .min(MAX_INITIAL_OUT_CAPACITY)
            .saturating_add(OUT_SLACK)
            .min(hard_cap);
        Self {
            buf: vec![0u8; initial],
            pos: 0,
            hard_cap,
            expected_len: expected_len.min(max_possible),
        }
    }

    #[inline]
    pub(super) fn len(&self) -> usize {
        self.pos
    }

    /// Guarantees `OUT_SLACK` writable bytes beyond `pos`.
    #[inline]
    pub(super) fn ensure_slack(&mut self) {
        if self.pos + OUT_SLACK > self.buf.len() {
            self.grow(OUT_SLACK);
        }
    }

    /// Guarantees `extra` writable bytes (plus slack) beyond `pos`.
    pub(super) fn reserve_extra(&mut self, extra: usize) {
        if self.pos.saturating_add(extra).saturating_add(OUT_SLACK) > self.buf.len() {
            self.grow(extra.saturating_add(OUT_SLACK));
        }
    }

    #[cold]
    fn grow(&mut self, need: usize) {
        let len = self.buf.len();
        let hint_end = self.expected_len.saturating_add(OUT_SLACK);
        // Output that is still short of the declared size grows quickly toward it and stops there;
        // otherwise (no claim, or a claim already passed) the buffer doubles.
        let step = if hint_end > len {
            len.saturating_mul(HINTED_GROWTH).min(hint_end)
        } else {
            len.saturating_mul(2)
        };
        let wanted = step.max(self.pos.saturating_add(need));
        // The budget is checked before every write, so more than `hard_cap` is never required
        // for output that passes the check (a single stored block may exceed it only by `need`).
        let target = wanted.min(self.hard_cap.max(self.pos.saturating_add(need)));
        self.buf.resize(target, 0);
    }

    /// Appends one byte, growing the buffer when it is full.
    #[inline]
    pub(super) fn push_literal(&mut self, b: u8) {
        if self.pos >= self.buf.len() {
            self.grow(OUT_SLACK);
        }
        self.buf[self.pos] = b;
        self.pos += 1;
    }

    /// Appends one byte; the caller has ensured slack.
    #[inline]
    pub(super) fn push_unchecked_space(&mut self, b: u8) {
        self.buf[self.pos] = b;
        self.pos += 1;
    }

    pub(super) fn extend_from_slice(&mut self, bytes: &[u8]) {
        self.reserve_extra(bytes.len());
        self.buf[self.pos..self.pos + bytes.len()].copy_from_slice(bytes);
        self.pos += bytes.len();
    }

    /// Appends an LZ77 match (`distance` already validated against `pos`, slack ensured).
    #[inline]
    pub(super) fn copy_match(&mut self, distance: usize, match_len: usize) {
        let dst = self.pos;
        let src = dst - distance;
        if distance >= match_len && match_len > LONG_COPY_MIN {
            self.buf.copy_within(src..src + match_len, dst);
        } else if distance >= 8 {
            // Forward 8-byte chunks are safe: each chunk's source lies entirely before its
            // destination. Overshoot into the slack is overwritten by later output.
            let mut copied = 0usize;
            while copied < match_len {
                let chunk: [u8; 8] = self.buf[src + copied..src + copied + 8].try_into().unwrap();
                self.buf[dst + copied..dst + copied + 8].copy_from_slice(&chunk);
                copied += 8;
            }
        } else if distance == 1 {
            let b = self.buf[src];
            self.buf[dst..dst + match_len].fill(b);
        } else {
            // Double the pattern period until >= 16 bytes so subsequent 8-byte chunks have
            // neither overlap nor store-to-load forwarding stalls.
            let mut written = 0usize;
            let mut period = distance;
            while written < match_len && period < 16 {
                let n = period;
                for i in 0..n {
                    self.buf[dst + written + i] = self.buf[src + i];
                }
                written += n;
                period <<= 1;
            }
            let mut copied = 0usize;
            let rem = match_len.saturating_sub(written);
            while copied < rem {
                let chunk: [u8; 8] = self.buf[src + copied..src + copied + 8].try_into().unwrap();
                self.buf[dst + written + copied..dst + written + copied + 8]
                    .copy_from_slice(&chunk);
                copied += 8;
            }
        }
        self.pos += match_len;
    }

    pub(super) fn finish(mut self) -> Vec<u8> {
        self.buf.truncate(self.pos);
        self.buf
    }
}
