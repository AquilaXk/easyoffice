use crate::EasyOfficeError;
use std::sync::OnceLock;

mod compress;
mod out_buf;
#[cfg(test)]
mod reference_inflate;
use out_buf::OutBuf;
#[cfg(test)]
use out_buf::OUT_SLACK;

const MAX_BITS: usize = 15;
const FAST_LUT_BITS: u32 = 9;
const FAST_LUT_SIZE: usize = 1 << FAST_LUT_BITS;
const FAST_LUT_MASK: u64 = (FAST_LUT_SIZE as u64) - 1;
const MAX_MATCH_LEN: usize = 258;
const MIN_MATCH_LEN: usize = 3;
const WINDOW_SIZE: usize = 32_768;
const ADLER_MOD: u32 = 65_521;
const ADLER_NMAX: usize = 5_552;

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];

const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];

const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];

const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

const CL_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

const LENGTH_CODE_LUT: [u8; 259] = {
    let mut lut = [0u8; 259];
    let mut len = 3usize;
    while len <= 257 {
        let mut idx = 0usize;
        while idx + 1 < 28 && (LENGTH_BASE[idx + 1] as usize) <= len {
            idx += 1;
        }
        lut[len] = idx as u8;
        len += 1;
    }
    lut[258] = 28;
    lut
};

const DIST_CODE_LUT: [u8; 512] = {
    let mut lut = [0u8; 512];
    let mut d = 0usize;
    while d < 256 {
        let dist = d + 1;
        let mut idx = 0usize;
        while idx + 1 < 30 && (DIST_BASE[idx + 1] as usize) <= dist {
            idx += 1;
        }
        lut[d] = idx as u8;
        d += 1;
    }
    let mut hi = 0usize;
    while hi < 256 {
        let dist = (hi << 7) + 1;
        let mut idx = 0usize;
        while idx + 1 < 30 && (DIST_BASE[idx + 1] as usize) <= dist {
            idx += 1;
        }
        lut[256 + hi] = idx as u8;
        hi += 1;
    }
    lut
};

struct BitReader<'a> {
    input: &'a [u8],
    byte_pos: usize,
    bit_buf: u64,
    bit_count: u32,
}

impl<'a> BitReader<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            byte_pos: 0,
            bit_buf: 0,
            bit_count: 0,
        }
    }

    #[inline]
    fn fill(&mut self) {
        if self.bit_count <= 56 && self.byte_pos + 8 <= self.input.len() {
            let chunk = u64::from_le_bytes(
                self.input[self.byte_pos..self.byte_pos + 8]
                    .try_into()
                    .unwrap(),
            );
            let take = ((64 - self.bit_count) >> 3) as usize;
            let take_bits = (take as u32) << 3;
            let mask = if take_bits == 64 {
                u64::MAX
            } else {
                (1u64 << take_bits) - 1
            };
            self.bit_buf |= (chunk & mask) << self.bit_count;
            self.byte_pos += take;
            self.bit_count += take_bits;
            return;
        }
        while self.bit_count <= 56 && self.byte_pos < self.input.len() {
            self.bit_buf |= u64::from(self.input[self.byte_pos]) << self.bit_count;
            self.byte_pos += 1;
            self.bit_count += 8;
        }
    }

    #[inline]
    fn read_bits(&mut self, n: u32) -> Result<u32, EasyOfficeError> {
        if n == 0 {
            return Ok(0);
        }
        if self.bit_count < n {
            self.fill();
            if self.bit_count < n {
                return Err(EasyOfficeError::CorruptedContainer(
                    "DEFLATE 비트스트림이 예기치 않게 절단되었습니다.".to_string(),
                ));
            }
        }
        let mask = (1u64 << n) - 1;
        let val = (self.bit_buf & mask) as u32;
        self.bit_buf >>= n;
        self.bit_count -= n;
        Ok(val)
    }

    #[inline]
    fn align_to_byte(&mut self) {
        let discard = self.bit_count & 7;
        self.bit_buf >>= discard;
        self.bit_count -= discard;
    }

    #[inline]
    fn byte_pos(&self) -> usize {
        self.byte_pos - (self.bit_count / 8) as usize
    }

    fn read_aligned_u16_le(&mut self) -> Result<u16, EasyOfficeError> {
        let lo = self.read_bits(8)? as u16;
        let hi = self.read_bits(8)? as u16;
        Ok(lo | (hi << 8))
    }

    fn copy_aligned_bytes(
        &mut self,
        len: usize,
        out: &mut OutBuf,
        max_output_bytes: usize,
        input_len: usize,
    ) -> Result<(), EasyOfficeError> {
        check_budget(out.len() + len, max_output_bytes, input_len)?;
        out.reserve_extra(len);
        let mut remaining = len;
        while remaining > 0 && self.bit_count >= 8 {
            out.push_unchecked_space((self.bit_buf & 0xFF) as u8);
            self.bit_buf >>= 8;
            self.bit_count -= 8;
            remaining -= 1;
        }
        if remaining > 0 {
            if self.byte_pos + remaining > self.input.len() {
                return Err(EasyOfficeError::CorruptedContainer(
                    "DEFLATE Stored 블록 바이트가 절단되었습니다.".to_string(),
                ));
            }
            out.extend_from_slice(&self.input[self.byte_pos..self.byte_pos + remaining]);
            self.byte_pos += remaining;
        }
        Ok(())
    }
}

/// Two-level decoding table: a `FAST_LUT_BITS`-wide primary lookup holding `symbol << 5 | length`
/// for codes up to that length, and small per-prefix secondary tables for longer codes.
struct HuffmanTable {
    fast_lut: [u16; FAST_LUT_SIZE],
    /// Per primary slot: `offset << 3 | sub_bits` of the secondary table, or 0 when none.
    links: [u32; FAST_LUT_SIZE],
    sub_entries: Vec<u16>,
}

impl HuffmanTable {
    fn from_code_lengths(lengths: &[u8]) -> Result<Self, EasyOfficeError> {
        let mut bl_count = [0u16; MAX_BITS + 1];
        let mut max_bits = 0usize;

        for &len in lengths {
            let l = len as usize;
            if l > MAX_BITS {
                return Err(EasyOfficeError::CorruptedContainer(
                    "DEFLATE 허프만 코드 길이가 15비트를 초과했습니다.".to_string(),
                ));
            }
            if l > 0 {
                bl_count[l] += 1;
                if l > max_bits {
                    max_bits = l;
                }
            }
        }

        if max_bits == 0 {
            return Ok(Self {
                fast_lut: [0u16; FAST_LUT_SIZE],
                links: [0u32; FAST_LUT_SIZE],
                sub_entries: Vec::new(),
            });
        }

        let mut left = 1i32;
        for &cnt in &bl_count[1..=max_bits] {
            left = (left << 1) - i32::from(cnt);
            if left < 0 {
                return Err(EasyOfficeError::CorruptedContainer(
                    "DEFLATE 허프만 트리가 과도하게 할당(oversubscribed)되었습니다.".to_string(),
                ));
            }
        }
        let is_single_symbol_1bit = max_bits == 1 && bl_count[1] == 1;
        if left != 0 && !is_single_symbol_1bit {
            return Err(EasyOfficeError::CorruptedContainer(
                "DEFLATE 허프만 트리가 불완전(under-subscribed)합니다.".to_string(),
            ));
        }

        let mut next_code = [0u16; MAX_BITS + 1];
        let mut code = 0u16;
        for bits in 1..=max_bits {
            code = (code + bl_count[bits - 1]) << 1;
            next_code[bits] = code;
        }

        // Reversed (LSB-first) code per symbol, kept for the secondary-table pass.
        let mut table = Self {
            fast_lut: [0u16; FAST_LUT_SIZE],
            links: [0u32; FAST_LUT_SIZE],
            sub_entries: Vec::new(),
        };
        let mut long_codes: Vec<(u16, u16, u8)> = Vec::new();
        let mut group_bits = [0u8; FAST_LUT_SIZE];
        for (sym, &len_u8) in lengths.iter().enumerate() {
            let len = len_u8 as usize;
            if len == 0 {
                continue;
            }
            let canonical = next_code[len];
            next_code[len] += 1;
            let rev = reverse_bits(canonical, len_u8);
            let packed = ((sym as u16) << 5) | (len as u16);
            if len <= FAST_LUT_BITS as usize {
                let step = 1usize << len;
                let mut idx = usize::from(rev);
                while idx < FAST_LUT_SIZE {
                    table.fast_lut[idx] = packed;
                    idx += step;
                }
            } else {
                let prefix = usize::from(rev) & (FAST_LUT_SIZE - 1);
                let extra = len_u8 - FAST_LUT_BITS as u8;
                group_bits[prefix] = group_bits[prefix].max(extra);
                long_codes.push((rev, packed, len_u8));
            }
        }
        table.build_secondary(&group_bits, &long_codes);
        Ok(table)
    }

    /// Lays out one secondary table per primary prefix that has codes longer than the primary width.
    fn build_secondary(&mut self, group_bits: &[u8; FAST_LUT_SIZE], long_codes: &[(u16, u16, u8)]) {
        if long_codes.is_empty() {
            return;
        }
        let mut total = 0usize;
        for (prefix, &bits) in group_bits.iter().enumerate() {
            if bits != 0 {
                self.links[prefix] = ((total as u32) << 3) | u32::from(bits);
                total += 1usize << bits;
            }
        }
        self.sub_entries = vec![0u16; total];
        for &(rev, packed, len) in long_codes {
            let prefix = usize::from(rev) & (FAST_LUT_SIZE - 1);
            let link = self.links[prefix];
            let size = 1usize << (link & 7);
            let base = (link >> 3) as usize;
            let step = 1usize << (len - FAST_LUT_BITS as u8);
            let mut idx = usize::from(rev) >> FAST_LUT_BITS;
            while idx < size {
                self.sub_entries[base + idx] = packed;
                idx += step;
            }
        }
    }

    /// Entry for the next code in `bits` (the low bits of the bit buffer); 0 when invalid.
    #[inline]
    fn lookup(&self, bits: u64) -> u16 {
        let fast = self.fast_lut[(bits & FAST_LUT_MASK) as usize];
        if fast != 0 {
            return fast;
        }
        let link = self.links[(bits & FAST_LUT_MASK) as usize];
        if link == 0 {
            return 0;
        }
        let sub_mask = (1u64 << (link & 7)) - 1;
        self.sub_entries[(link >> 3) as usize + ((bits >> FAST_LUT_BITS) & sub_mask) as usize]
    }

    #[inline]
    fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16, EasyOfficeError> {
        if reader.bit_count < MAX_BITS as u32 {
            reader.fill();
        }
        let entry = self.lookup(reader.bit_buf);
        let bit_len = u32::from(entry & 0x1F);
        if bit_len == 0 || reader.bit_count < bit_len {
            return Err(EasyOfficeError::CorruptedContainer(
                "유효하지 않은 DEFLATE 허프만 코드 또는 절단된 스트림입니다.".to_string(),
            ));
        }
        reader.bit_buf >>= bit_len;
        reader.bit_count -= bit_len;
        Ok(entry >> 5)
    }
}

#[inline]
fn reverse_bits(code: u16, bit_len: u8) -> u16 {
    if bit_len == 0 {
        0
    } else {
        code.reverse_bits() >> (16 - bit_len)
    }
}

#[inline]
fn check_budget(
    new_len: usize,
    max_output_bytes: usize,
    _input_len: usize,
) -> Result<(), EasyOfficeError> {
    if new_len > max_output_bytes {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "DEFLATE 출력 크기({new_len} bytes)가 허용 상한({max_output_bytes} bytes)을 초과했습니다."
        )));
    }
    Ok(())
}

fn fixed_huffman_tables() -> &'static (HuffmanTable, HuffmanTable) {
    static FIXED_TABLES: OnceLock<(HuffmanTable, HuffmanTable)> = OnceLock::new();
    FIXED_TABLES.get_or_init(|| {
        let mut lit_lengths = [0u8; 288];
        lit_lengths[..=143].fill(8);
        lit_lengths[144..=255].fill(9);
        lit_lengths[256..=279].fill(7);
        lit_lengths[280..=287].fill(8);
        let dist_lengths = [5u8; 32];
        (
            HuffmanTable::from_code_lengths(&lit_lengths).expect("valid RFC 1951 fixed lit"),
            HuffmanTable::from_code_lengths(&dist_lengths).expect("valid RFC 1951 fixed dist"),
        )
    })
}

fn read_dynamic_huffman_tables(
    reader: &mut BitReader<'_>,
) -> Result<(HuffmanTable, HuffmanTable), EasyOfficeError> {
    let hlit = (reader.read_bits(5)? as usize) + 257;
    let hdist = (reader.read_bits(5)? as usize) + 1;
    let hclen = (reader.read_bits(4)? as usize) + 4;
    if hlit > 286 || hdist > 30 {
        return Err(EasyOfficeError::CorruptedContainer(
            "DEFLATE 동적 허프만 HLIT/HDIST 헤더가 RFC 1951 범위를 벗어났습니다.".to_string(),
        ));
    }

    let mut cl_lengths = [0u8; 19];
    for i in 0..hclen {
        cl_lengths[CL_ORDER[i]] = reader.read_bits(3)? as u8;
    }
    let cl_table = HuffmanTable::from_code_lengths(&cl_lengths)?;

    let total_codes = hlit + hdist;
    let mut combined = Vec::with_capacity(total_codes);
    while combined.len() < total_codes {
        let sym = cl_table.decode(reader)?;
        match sym {
            0..=15 => combined.push(sym as u8),
            16 => {
                let prev = *combined.last().ok_or_else(|| {
                    EasyOfficeError::CorruptedContainer(
                        "DEFLATE 동적 허프만 RLE(16)가 선행 코드 없이 나타났습니다.".to_string(),
                    )
                })?;
                let repeat = (reader.read_bits(2)? as usize) + 3;
                if combined.len() + repeat > total_codes {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "DEFLATE 동적 허프만 RLE(16) 반복 횟수가 테이블 크기를 초과했습니다."
                            .to_string(),
                    ));
                }
                combined.resize(combined.len() + repeat, prev);
            }
            17 => {
                let repeat = (reader.read_bits(3)? as usize) + 3;
                if combined.len() + repeat > total_codes {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "DEFLATE 동적 허프만 RLE(17) 반복 횟수가 테이블 크기를 초과했습니다."
                            .to_string(),
                    ));
                }
                combined.resize(combined.len() + repeat, 0);
            }
            18 => {
                let repeat = (reader.read_bits(7)? as usize) + 11;
                if combined.len() + repeat > total_codes {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "DEFLATE 동적 허프만 RLE(18) 반복 횟수가 테이블 크기를 초과했습니다."
                            .to_string(),
                    ));
                }
                combined.resize(combined.len() + repeat, 0);
            }
            _ => {
                return Err(EasyOfficeError::CorruptedContainer(
                    "DEFLATE 동적 허프만 코드 길이 심볼 오류".to_string(),
                ));
            }
        }
    }

    let lit_slice = &combined[..hlit];
    if lit_slice.get(256).copied().unwrap_or(0) == 0 {
        return Err(EasyOfficeError::CorruptedContainer(
            "DEFLATE 동적 허프만 테이블에 EOB(256) 심볼이 누락되었습니다.".to_string(),
        ));
    }
    let dist_slice = &combined[hlit..];
    Ok((
        HuffmanTable::from_code_lengths(lit_slice)?,
        HuffmanTable::from_code_lengths(dist_slice)?,
    ))
}

/// Bits a single symbol can consume in the fast loop: literal/length code (15) + length extra (5)
/// + distance code (15) + distance extra (13).
const FAST_SYMBOL_BITS: u32 = 48;

fn invalid_huffman_code() -> EasyOfficeError {
    EasyOfficeError::CorruptedContainer(
        "유효하지 않은 DEFLATE 허프만 코드 또는 절단된 스트림입니다.".to_string(),
    )
}

impl HuffmanTable {
    /// Decodes one symbol assuming the reader already holds at least `MAX_BITS` valid bits.
    #[inline]
    fn decode_prefilled(&self, reader: &mut BitReader<'_>) -> Result<u16, EasyOfficeError> {
        let entry = self.lookup(reader.bit_buf);
        let bit_len = u32::from(entry & 0x1F);
        if bit_len == 0 {
            return Err(invalid_huffman_code());
        }
        reader.bit_buf >>= bit_len;
        reader.bit_count -= bit_len;
        Ok(entry >> 5)
    }
}

/// Validates a decoded `(match_len, distance)` pair and appends the match.
#[inline]
fn apply_match(
    out: &mut OutBuf,
    match_len: usize,
    distance: usize,
    max_output_bytes: usize,
    input_len: usize,
) -> Result<(), EasyOfficeError> {
    if distance == 0 || distance > out.len() {
        return Err(EasyOfficeError::CorruptedContainer(format!(
            "DEFLATE LZ77 역참조 거리({distance})가 출력 버퍼 길이({})를 벗어났습니다.",
            out.len()
        )));
    }
    check_budget(out.len() + match_len, max_output_bytes, input_len)?;
    out.ensure_slack();
    out.copy_match(distance, match_len);
    Ok(())
}

/// Length symbol remainder (extra bits, distance symbol, distance extra bits) with the reader
/// holding at least `FAST_SYMBOL_BITS` bits from the start of the symbol.
#[inline]
fn decode_match_prefilled(
    reader: &mut BitReader<'_>,
    sym: u16,
    dist_table: &HuffmanTable,
    out: &mut OutBuf,
    max_output_bytes: usize,
    input_len: usize,
) -> Result<(), EasyOfficeError> {
    let l_idx = usize::from(sym - 257);
    let extra_bits = u32::from(LENGTH_EXTRA[l_idx]);
    let extra_val = (reader.bit_buf & ((1u64 << extra_bits) - 1)) as usize;
    reader.bit_buf >>= extra_bits;
    reader.bit_count -= extra_bits;
    let match_len = usize::from(LENGTH_BASE[l_idx]) + extra_val;

    let dist_sym = usize::from(dist_table.decode_prefilled(reader)?);
    if dist_sym >= 30 {
        return Err(EasyOfficeError::CorruptedContainer(format!(
            "유효하지 않은 DEFLATE 거리 코드({dist_sym})입니다."
        )));
    }
    let d_extra_bits = u32::from(DIST_EXTRA[dist_sym]);
    let d_extra_val = (reader.bit_buf & ((1u64 << d_extra_bits) - 1)) as usize;
    reader.bit_buf >>= d_extra_bits;
    reader.bit_count -= d_extra_bits;
    let distance = usize::from(DIST_BASE[dist_sym]) + d_extra_val;
    apply_match(out, match_len, distance, max_output_bytes, input_len)
}

/// Fast loop: runs while eight or more input bytes remain, so one refill covers a whole symbol
/// and no per-field bit-count checks are needed. Returns `true` once end-of-block was decoded
/// and `false` when the input tail needs the checked path.
fn decode_huffman_fast(
    reader: &mut BitReader<'_>,
    lit_table: &HuffmanTable,
    dist_table: &HuffmanTable,
    out: &mut OutBuf,
    max_output_bytes: usize,
    input_len: usize,
) -> Result<bool, EasyOfficeError> {
    loop {
        if reader.bit_count < FAST_SYMBOL_BITS {
            if reader.byte_pos + 8 > reader.input.len() {
                return Ok(false);
            }
            reader.fill();
        }
        let sym = lit_table.decode_prefilled(reader)?;
        if sym < 256 {
            if out.len() >= max_output_bytes {
                check_budget(out.len() + 1, max_output_bytes, input_len)?;
            }
            out.push_literal(sym as u8);
        } else if sym == 256 {
            return Ok(true);
        } else if sym <= 285 {
            decode_match_prefilled(reader, sym, dist_table, out, max_output_bytes, input_len)?;
        } else {
            return Err(EasyOfficeError::CorruptedContainer(format!(
                "유효하지 않은 DEFLATE 리터럴/길이 심볼({sym})입니다."
            )));
        }
    }
}

fn decode_huffman_block(
    reader: &mut BitReader<'_>,
    lit_table: &HuffmanTable,
    dist_table: &HuffmanTable,
    out: &mut OutBuf,
    max_output_bytes: usize,
    input_len: usize,
) -> Result<(), EasyOfficeError> {
    if decode_huffman_fast(
        reader,
        lit_table,
        dist_table,
        out,
        max_output_bytes,
        input_len,
    )? {
        return Ok(());
    }
    // Input tail: fully bounds-checked decoding.
    loop {
        let sym = lit_table.decode(reader)?;
        if sym < 256 {
            check_budget(out.len() + 1, max_output_bytes, input_len)?;
            out.push_literal(sym as u8);
        } else if sym == 256 {
            break;
        } else if sym <= 285 {
            let l_idx = (sym - 257) as usize;
            let extra_bits = u32::from(LENGTH_EXTRA[l_idx]);
            let extra_val = reader.read_bits(extra_bits)? as usize;
            let match_len = (LENGTH_BASE[l_idx] as usize) + extra_val;

            let dist_sym = dist_table.decode(reader)? as usize;
            if dist_sym >= 30 {
                return Err(EasyOfficeError::CorruptedContainer(format!(
                    "유효하지 않은 DEFLATE 거리 코드({dist_sym})입니다."
                )));
            }
            let d_extra_bits = u32::from(DIST_EXTRA[dist_sym]);
            let d_extra_val = reader.read_bits(d_extra_bits)? as usize;
            let distance = (DIST_BASE[dist_sym] as usize) + d_extra_val;
            apply_match(out, match_len, distance, max_output_bytes, input_len)?;
        } else {
            return Err(EasyOfficeError::CorruptedContainer(format!(
                "유효하지 않은 DEFLATE 리터럴/길이 심볼({sym})입니다."
            )));
        }
    }
    Ok(())
}

pub(crate) fn inflate_raw_with_consumed(
    input: &[u8],
    max_output_bytes: usize,
    expected_len: usize,
) -> Result<(Vec<u8>, usize), EasyOfficeError> {
    if input.is_empty() {
        return Err(EasyOfficeError::CorruptedContainer(
            "비어 있는 DEFLATE 스트림입니다.".to_string(),
        ));
    }

    let mut reader = BitReader::new(input);
    let mut out = OutBuf::new(input.len(), max_output_bytes, expected_len);

    loop {
        let bfinal = reader.read_bits(1)?;
        let btype = reader.read_bits(2)?;

        match btype {
            0 => {
                reader.align_to_byte();
                let len = reader.read_aligned_u16_le()?;
                let nlen = reader.read_aligned_u16_le()?;
                if nlen != !len {
                    return Err(EasyOfficeError::CorruptedContainer(
                        "DEFLATE Stored 블록 LEN/NLEN 1의 보수 검증에 실패했습니다.".to_string(),
                    ));
                }
                reader.copy_aligned_bytes(len as usize, &mut out, max_output_bytes, input.len())?;
            }
            1 => {
                let (lit_table, dist_table) = fixed_huffman_tables();
                decode_huffman_block(
                    &mut reader,
                    lit_table,
                    dist_table,
                    &mut out,
                    max_output_bytes,
                    input.len(),
                )?;
            }
            2 => {
                let (lit_table, dist_table) = read_dynamic_huffman_tables(&mut reader)?;
                decode_huffman_block(
                    &mut reader,
                    &lit_table,
                    &dist_table,
                    &mut out,
                    max_output_bytes,
                    input.len(),
                )?;
            }
            _ => {
                return Err(EasyOfficeError::CorruptedContainer(
                    "DEFLATE 예약된 블록 타입(BTYPE=11)이 감지되었습니다.".to_string(),
                ));
            }
        }

        if bfinal != 0 {
            reader.align_to_byte();
            break;
        }
    }

    let consumed = reader.byte_pos();
    Ok((out.finish(), consumed))
}

/// Decompresses an RFC 1951 raw DEFLATE bitstream with Fail-Closed output budget enforcement.
pub fn inflate_raw(input: &[u8], max_output_bytes: usize) -> Result<Vec<u8>, EasyOfficeError> {
    inflate_raw_with_consumed(input, max_output_bytes, 0).map(|(out, _)| out)
}

/// [`inflate_raw`] for a stream whose output size the container declares (`expected_len`): the
/// output is allocated once at that size. The declaration is only a hint, so a stream that
/// produces more is still cut off at `max_output_bytes`.
pub fn inflate_raw_sized(
    input: &[u8],
    max_output_bytes: usize,
    expected_len: usize,
) -> Result<Vec<u8>, EasyOfficeError> {
    inflate_raw_with_consumed(input, max_output_bytes, expected_len).map(|(out, _)| out)
}

/// Computes the RFC 1950 Adler-32 checksum using `NMAX = 5552` deferred modulo reduction.
#[inline]
pub fn adler32(bytes: &[u8]) -> u32 {
    let mut s1: u32 = 1;
    let mut s2: u32 = 0;

    for chunk in bytes.chunks(ADLER_NMAX) {
        let mut i = 0usize;
        while i + 8 <= chunk.len() {
            let b0 = u32::from(chunk[i]);
            let b1 = u32::from(chunk[i + 1]);
            let b2 = u32::from(chunk[i + 2]);
            let b3 = u32::from(chunk[i + 3]);
            let b4 = u32::from(chunk[i + 4]);
            let b5 = u32::from(chunk[i + 5]);
            let b6 = u32::from(chunk[i + 6]);
            let b7 = u32::from(chunk[i + 7]);
            s2 += 8 * s1 + 8 * b0 + 7 * b1 + 6 * b2 + 5 * b3 + 4 * b4 + 3 * b5 + 2 * b6 + b7;
            s1 += b0 + b1 + b2 + b3 + b4 + b5 + b6 + b7;
            i += 8;
        }
        while i < chunk.len() {
            s1 += u32::from(chunk[i]);
            s2 += s1;
            i += 1;
        }
        s1 %= ADLER_MOD;
        s2 %= ADLER_MOD;
    }

    (s2 << 16) | s1
}

/// Compresses `input` into an RFC 1950 zlib stream (`[0x78, 0x9C] + deflate_raw + 4-byte BE Adler-32`).
pub fn deflate_zlib(input: &[u8]) -> Vec<u8> {
    let raw = deflate_raw(input);
    let checksum = adler32(input);
    let mut out = Vec::with_capacity(2 + raw.len() + 4);
    out.extend_from_slice(&[0x78, 0x9C]);
    out.extend_from_slice(&raw);
    out.extend_from_slice(&checksum.to_be_bytes());
    out
}

#[inline]
fn is_valid_zlib_header(input: &[u8]) -> bool {
    input.len() >= 2
        && (input[0] & 0x0F) == 8
        && (input[0] & 0x80) == 0
        && (input[1] & 0x20) == 0
        && (u16::from(input[0]) * 256 + u16::from(input[1])) % 31 == 0
}

/// Decompresses an RFC 1950 zlib stream and validates the 2-byte header and 4-byte Adler-32 trailer.
pub fn inflate_zlib(input: &[u8], max_output_bytes: usize) -> Result<Vec<u8>, EasyOfficeError> {
    if input.len() < 6 || !is_valid_zlib_header(input) {
        return Err(EasyOfficeError::CorruptedContainer(
            "유효하지 않은 RFC 1950 zlib 스트림(헤더 2 + 데이터 + Adler-32 4)입니다.".to_string(),
        ));
    }
    let (decoded, consumed) = inflate_raw_with_consumed(&input[2..], max_output_bytes, 0)?;
    let rem = &input[2 + consumed..];
    if rem.len() < 4 {
        return Err(EasyOfficeError::CorruptedContainer(
            "RFC 1950 zlib 스트림에 4바이트 Adler-32 트레일러가 누락되었습니다.".to_string(),
        ));
    }
    let expected_adler = u32::from_be_bytes([rem[0], rem[1], rem[2], rem[3]]);
    let actual_adler = adler32(&decoded);
    if actual_adler != expected_adler {
        return Err(EasyOfficeError::CorruptedContainer(format!(
            "RFC 1950 Adler-32 체크섬 불일치 (expected: {expected_adler:#010x}, actual: {actual_adler:#010x})"
        )));
    }
    Ok(decoded)
}

/// Decompresses either a raw RFC 1951 DEFLATE stream or an RFC 1950 zlib-wrapped DEFLATE stream.
pub fn inflate_auto(input: &[u8], max_output_bytes: usize) -> Result<Vec<u8>, EasyOfficeError> {
    if is_valid_zlib_header(input) {
        match inflate_raw_with_consumed(&input[2..], max_output_bytes, 0) {
            Ok((decoded, consumed)) => {
                let rem = &input[2 + consumed..];
                if rem.len() >= 4 {
                    let expected = u32::from_be_bytes([rem[0], rem[1], rem[2], rem[3]]);
                    let actual = adler32(&decoded);
                    if expected != actual {
                        return Err(EasyOfficeError::CorruptedContainer(format!(
                            "RFC 1950 Adler-32 체크섬 불일치 (expected: {expected:#010x}, actual: {actual:#010x})"
                        )));
                    }
                    return Ok(decoded);
                }
                if rem.is_empty() {
                    return Ok(decoded);
                }
                return Err(EasyOfficeError::CorruptedContainer(
                    "RFC 1950 zlib 스트림의 4바이트 Adler-32 트레일러가 잘렸습니다.".to_string(),
                ));
            }
            Err(e @ EasyOfficeError::ResourceBudgetExceeded(_)) => return Err(e),
            Err(_) => {}
        }
    }
    inflate_raw(input, max_output_bytes)
}

#[inline]
fn find_length_code(len: usize) -> usize {
    LENGTH_CODE_LUT[len] as usize
}

#[inline]
fn find_dist_code(dist: usize) -> usize {
    let d = dist - 1;
    if d < 256 {
        DIST_CODE_LUT[d] as usize
    } else {
        DIST_CODE_LUT[256 + (d >> 7)] as usize
    }
}

/// Compresses `input` into an RFC 1951 raw DEFLATE bitstream (hash-chain LZ77 with per-block
/// dynamic Huffman coding; see `compress.rs`). The output is deterministic for a given input.
pub fn deflate_raw(input: &[u8]) -> Vec<u8> {
    compress::compress(input, compress::DEFAULT_LEVEL)
}

/// Like [`deflate_raw`] with an explicit effort `level` (1 = fastest, 4 = smallest; clamped).
pub fn deflate_raw_level(input: &[u8], level: u8) -> Vec<u8> {
    compress::compress(input, level)
}

#[cfg(test)]
mod tests;
