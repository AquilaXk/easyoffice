use crate::{EasyOfficeError, ZipSecurityGuard};
use std::sync::OnceLock;

const MAX_BITS: usize = 15;
const FAST_LUT_BITS: u32 = 9;
const FAST_LUT_SIZE: usize = 1 << FAST_LUT_BITS;
const FAST_LUT_MASK: u64 = (FAST_LUT_SIZE as u64) - 1;
const MAX_MATCH_LEN: usize = 258;
const MIN_MATCH_LEN: usize = 3;
const WINDOW_SIZE: usize = 32_768;
const HASH_BITS: usize = 12;
const HASH_SIZE: usize = 1 << HASH_BITS;
const HASH_MASK: usize = HASH_SIZE - 1;
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
        out: &mut Vec<u8>,
        max_output_bytes: usize,
        input_len: usize,
    ) -> Result<(), EasyOfficeError> {
        check_budget(out.len() + len, max_output_bytes, input_len)?;
        let mut remaining = len;
        while remaining > 0 && self.bit_count >= 8 {
            out.push((self.bit_buf & 0xFF) as u8);
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

struct HuffmanTable {
    max_bits: u32,
    fast_lut: [u16; FAST_LUT_SIZE],
    entries: Vec<u16>,
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
                max_bits: 1,
                fast_lut: [0u16; FAST_LUT_SIZE],
                entries: Vec::new(),
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

        let mut next_code = [0u16; MAX_BITS + 1];
        let mut code = 0u16;
        for bits in 1..=max_bits {
            code = (code + bl_count[bits - 1]) << 1;
            next_code[bits] = code;
        }

        let mut fast_lut = [0u16; FAST_LUT_SIZE];
        let mut entries = if max_bits > FAST_LUT_BITS as usize {
            vec![0u16; 1usize << max_bits]
        } else {
            Vec::new()
        };

        for (sym, &len_u8) in lengths.iter().enumerate() {
            let len = len_u8 as usize;
            if len == 0 {
                continue;
            }
            let canonical = next_code[len];
            next_code[len] += 1;
            let rev = reverse_bits(canonical, len_u8) as usize;
            let packed = ((sym as u16) << 5) | (len as u16);
            let step = 1usize << len;

            if len <= FAST_LUT_BITS as usize {
                let mut idx = rev;
                while idx < FAST_LUT_SIZE {
                    fast_lut[idx] = packed;
                    idx += step;
                }
            }
            if !entries.is_empty() {
                let table_size = entries.len();
                let mut idx = rev;
                while idx < table_size {
                    entries[idx] = packed;
                    idx += step;
                }
            }
        }

        Ok(Self {
            max_bits: max_bits as u32,
            fast_lut,
            entries,
        })
    }

    #[inline]
    fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16, EasyOfficeError> {
        if reader.bit_count < MAX_BITS as u32 {
            reader.fill();
        }
        let fast_entry = self.fast_lut[(reader.bit_buf & FAST_LUT_MASK) as usize];
        let fast_len = (fast_entry & 0x1F) as u32;
        if fast_len != 0 {
            if reader.bit_count < fast_len {
                return Err(EasyOfficeError::CorruptedContainer(
                    "유효하지 않은 DEFLATE 허프만 코드 또는 절단된 스트림입니다.".to_string(),
                ));
            }
            reader.bit_buf >>= fast_len;
            reader.bit_count -= fast_len;
            return Ok(fast_entry >> 5);
        }
        if self.entries.is_empty() || reader.bit_count == 0 {
            return Err(EasyOfficeError::CorruptedContainer(
                "DEFLATE 허프만 심볼 디코딩 중 스트림이 종료되었거나 유효하지 않은 코드입니다."
                    .to_string(),
            ));
        }
        let mask = (1u64 << self.max_bits) - 1;
        let peek = (reader.bit_buf & mask) as usize;
        let entry = self.entries[peek];
        let bit_len = (entry & 0x1F) as u32;
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
    input_len: usize,
) -> Result<(), EasyOfficeError> {
    if new_len > max_output_bytes {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "DEFLATE 출력 크기({new_len} bytes)가 허용 상한({max_output_bytes} bytes)을 초과했습니다."
        )));
    }
    if input_len > 0
        && new_len > 4096
        && new_len
            > input_len
                .max(1)
                .saturating_mul(ZipSecurityGuard::MAX_COMPRESSION_RATIO as usize)
    {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "DEFLATE 압축 해제 비율({}x)이 보안 상한({}x)을 초과했습니다.",
            new_len / input_len.max(1),
            ZipSecurityGuard::MAX_COMPRESSION_RATIO
        )));
    }
    Ok(())
}

#[inline]
fn push_checked_byte(
    out: &mut Vec<u8>,
    b: u8,
    max_output_bytes: usize,
    input_len: usize,
) -> Result<(), EasyOfficeError> {
    check_budget(out.len() + 1, max_output_bytes, input_len)?;
    out.push(b);
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

fn decode_huffman_block(
    reader: &mut BitReader<'_>,
    lit_table: &HuffmanTable,
    dist_table: &HuffmanTable,
    out: &mut Vec<u8>,
    max_output_bytes: usize,
    input_len: usize,
) -> Result<(), EasyOfficeError> {
    loop {
        let sym = lit_table.decode(reader)?;
        if sym < 256 {
            push_checked_byte(out, sym as u8, max_output_bytes, input_len)?;
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

            if distance == 0 || distance > out.len() {
                return Err(EasyOfficeError::CorruptedContainer(format!(
                    "DEFLATE LZ77 역참조 거리({distance})가 출력 버퍼 길이({})를 벗어났습니다.",
                    out.len()
                )));
            }

            check_budget(out.len() + match_len, max_output_bytes, input_len)?;
            if distance == 1 {
                let b = out[out.len() - 1];
                out.resize(out.len() + match_len, b);
            } else if distance >= match_len {
                let start = out.len() - distance;
                out.extend_from_within(start..start + match_len);
            } else {
                for _ in 0..match_len {
                    let b = out[out.len() - distance];
                    out.push(b);
                }
            }
        } else {
            return Err(EasyOfficeError::CorruptedContainer(format!(
                "유효하지 않은 DEFLATE 리터럴/길이 심볼({sym})입니다."
            )));
        }
    }
    Ok(())
}

fn inflate_raw_with_consumed(
    input: &[u8],
    max_output_bytes: usize,
) -> Result<(Vec<u8>, usize), EasyOfficeError> {
    if input.is_empty() {
        return Err(EasyOfficeError::CorruptedContainer(
            "비어 있는 DEFLATE 스트림입니다.".to_string(),
        ));
    }

    let mut reader = BitReader::new(input);
    let mut out = Vec::with_capacity(input.len().saturating_mul(2).min(65_536));

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

    Ok((out, reader.byte_pos()))
}

/// Decompresses an RFC 1951 raw DEFLATE bitstream with Fail-Closed output budget enforcement.
pub fn inflate_raw(input: &[u8], max_output_bytes: usize) -> Result<Vec<u8>, EasyOfficeError> {
    inflate_raw_with_consumed(input, max_output_bytes).map(|(out, _)| out)
}

/// Computes the RFC 1950 Adler-32 checksum using `NMAX = 5552` deferred modulo reduction.
#[inline]
pub fn adler32(bytes: &[u8]) -> u32 {
    let mut s1: u32 = 1;
    let mut s2: u32 = 0;

    for chunk in bytes.chunks(ADLER_NMAX) {
        let mut i = 0usize;
        while i + 16 <= chunk.len() {
            for k in 0..16 {
                s1 += u32::from(chunk[i + k]);
                s2 += s1;
            }
            i += 16;
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
        && (u16::from(input[0]) * 256 + u16::from(input[1])).is_multiple_of(31)
}

/// Decompresses an RFC 1950 zlib stream and validates the 2-byte header and 4-byte Adler-32 trailer.
pub fn inflate_zlib(input: &[u8], max_output_bytes: usize) -> Result<Vec<u8>, EasyOfficeError> {
    if input.len() < 6 || !is_valid_zlib_header(input) {
        return Err(EasyOfficeError::CorruptedContainer(
            "유효하지 않은 RFC 1950 zlib 스트림(헤더 2 + 데이터 + Adler-32 4)입니다.".to_string(),
        ));
    }
    let decoded = inflate_raw(&input[2..input.len() - 4], max_output_bytes)?;
    let expected_adler = u32::from_be_bytes(input[input.len() - 4..].try_into().unwrap());
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
        match inflate_raw_with_consumed(&input[2..], max_output_bytes) {
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
                }
                return Ok(decoded);
            }
            Err(e @ EasyOfficeError::ResourceBudgetExceeded(_)) => return Err(e),
            Err(_) => {}
        }
    }
    inflate_raw(input, max_output_bytes)
}

struct BitWriter {
    out: Vec<u8>,
    bit_buf: u64,
    bit_count: u32,
}

impl BitWriter {
    fn with_capacity(cap: usize) -> Self {
        Self {
            out: Vec::with_capacity(cap),
            bit_buf: 0,
            bit_count: 0,
        }
    }

    #[inline]
    fn write_bits_lsb(&mut self, val: u32, n: u32) {
        if n == 0 {
            return;
        }
        let mask = (1u64 << n) - 1;
        self.bit_buf |= (u64::from(val) & mask) << self.bit_count;
        self.bit_count += n;
        if self.bit_count >= 32 {
            self.out
                .extend_from_slice(&(self.bit_buf as u32).to_le_bytes());
            self.bit_buf >>= 32;
            self.bit_count -= 32;
        }
        while self.bit_count >= 8 {
            self.out.push((self.bit_buf & 0xFF) as u8);
            self.bit_buf >>= 8;
            self.bit_count -= 8;
        }
    }

    #[inline]
    fn write_huffman_msb(&mut self, code: u16, len: u8) {
        let rev = reverse_bits(code, len);
        self.write_bits_lsb(u32::from(rev), u32::from(len));
    }

    #[inline]
    fn write_fixed_sym(&mut self, sym: u16) {
        match sym {
            0..=143 => self.write_huffman_msb(0x30 + sym, 8),
            144..=255 => self.write_huffman_msb(0x190 + (sym - 144), 9),
            256..=279 => self.write_huffman_msb(sym - 256, 7),
            280..=287 => self.write_huffman_msb(0xC0 + (sym - 280), 8),
            _ => unreachable!(),
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.bit_count > 0 {
            self.out.push((self.bit_buf & 0xFF) as u8);
        }
        self.out
    }
}

#[inline]
fn hash3(bytes: &[u8], pos: usize) -> usize {
    let b0 = bytes[pos] as usize;
    let b1 = bytes[pos + 1] as usize;
    let b2 = bytes[pos + 2] as usize;
    ((b0 << 8) ^ (b1 << 4) ^ b2) & HASH_MASK
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

/// Compresses `input` into a deterministic RFC 1951 raw DEFLATE bitstream using Fixed Huffman + SWAR `u64` LZ77.
pub fn deflate_raw(input: &[u8]) -> Vec<u8> {
    let mut writer = BitWriter::with_capacity(input.len() / 2 + 16);
    writer.write_bits_lsb(1, 1);
    writer.write_bits_lsb(1, 2);

    if input.len() < MIN_MATCH_LEN {
        for &b in input {
            writer.write_fixed_sym(u16::from(b));
        }
        writer.write_fixed_sym(256);
        return writer.finish();
    }

    let mut head = [-1i32; HASH_SIZE];
    let mut prev = vec![-1i32; input.len()];
    let mut pos = 0usize;

    while pos < input.len() {
        let mut best_len = 0usize;
        let mut best_dist = 0usize;

        if pos + MIN_MATCH_LEN <= input.len() {
            let h = hash3(input, pos);
            let mut candidate = head[h];
            prev[pos] = candidate;
            head[h] = pos as i32;

            let max_len = (input.len() - pos).min(MAX_MATCH_LEN);
            let min_pos = pos.saturating_sub(WINDOW_SIZE);
            let mut chain_steps = 0usize;

            while candidate >= (min_pos as i32) && chain_steps < 64 {
                let c_pos = candidate as usize;
                let dist = pos - c_pos;
                if dist > 0
                    && dist <= WINDOW_SIZE
                    && input[c_pos + best_len] == input[pos + best_len]
                {
                    let mut m_len = 0usize;
                    while m_len + 8 <= max_len {
                        let a = u64::from_le_bytes(
                            input[c_pos + m_len..c_pos + m_len + 8].try_into().unwrap(),
                        );
                        let b = u64::from_le_bytes(
                            input[pos + m_len..pos + m_len + 8].try_into().unwrap(),
                        );
                        let diff = a ^ b;
                        if diff == 0 {
                            m_len += 8;
                        } else {
                            m_len += (diff.trailing_zeros() >> 3) as usize;
                            break;
                        }
                    }
                    while m_len < max_len && input[c_pos + m_len] == input[pos + m_len] {
                        m_len += 1;
                    }
                    if m_len >= MIN_MATCH_LEN && m_len > best_len {
                        best_len = m_len;
                        best_dist = dist;
                        if best_len == max_len {
                            break;
                        }
                    }
                }
                candidate = prev[c_pos];
                chain_steps += 1;
            }
        }

        if best_len >= MIN_MATCH_LEN {
            let l_idx = find_length_code(best_len);
            writer.write_fixed_sym(257 + (l_idx as u16));
            let l_extra = u32::from(LENGTH_EXTRA[l_idx]);
            if l_extra > 0 {
                writer.write_bits_lsb((best_len - (LENGTH_BASE[l_idx] as usize)) as u32, l_extra);
            }

            let d_idx = find_dist_code(best_dist);
            writer.write_huffman_msb(d_idx as u16, 5);
            let d_extra = u32::from(DIST_EXTRA[d_idx]);
            if d_extra > 0 {
                writer.write_bits_lsb((best_dist - (DIST_BASE[d_idx] as usize)) as u32, d_extra);
            }

            for step in 1..best_len {
                let adv = pos + step;
                if adv + MIN_MATCH_LEN <= input.len() {
                    let h = hash3(input, adv);
                    prev[adv] = head[h];
                    head[h] = adv as i32;
                }
            }
            pos += best_len;
        } else {
            writer.write_fixed_sym(u16::from(input[pos]));
            pos += 1;
        }
    }

    writer.write_fixed_sym(256);
    writer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deflate_and_inflate_fixed_stored_and_zlib_auto() {
        // RFC 1950 standard Adler-32 test vector: "Wikipedia" -> 0x11E60398
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);

        let xml = b"<w:document><w:body><w:p><w:r><w:t>EasyOffice RFC 1951 DEFLATE</w:t></w:p></w:body></w:document>"
            .repeat(8);
        let compressed = deflate_raw(&xml);
        assert!(compressed.len() < xml.len() / 3);
        let decompressed = inflate_raw(&compressed, 1024 * 1024).unwrap();
        assert_eq!(decompressed, xml);

        let zlib_full = deflate_zlib(&xml);
        assert_eq!(inflate_zlib(&zlib_full, 1024 * 1024).unwrap(), xml);
        assert_eq!(inflate_auto(&zlib_full, 1024 * 1024).unwrap(), xml);

        let mut bad_zlib = zlib_full.clone();
        let last_idx = bad_zlib.len() - 1;
        bad_zlib[last_idx] ^= 0xFF;
        assert!(matches!(
            inflate_zlib(&bad_zlib, 1024 * 1024),
            Err(EasyOfficeError::CorruptedContainer(_))
        ));
        assert!(matches!(
            inflate_auto(&bad_zlib, 1024 * 1024),
            Err(EasyOfficeError::CorruptedContainer(_))
        ));

        let stored_stream = [0x01u8, 0x05, 0x00, 0xFA, 0xFF, b'H', b'e', b'l', b'l', b'o'];
        assert_eq!(inflate_raw(&stored_stream, 1024).unwrap(), b"Hello");

        let mut zlib_wrapped = vec![0x78, 0x9C];
        zlib_wrapped.extend_from_slice(&compressed);
        assert_eq!(inflate_auto(&zlib_wrapped, 1024 * 1024).unwrap(), xml);

        assert!(matches!(
            inflate_raw(&[0x07], 1024),
            Err(EasyOfficeError::CorruptedContainer(_))
        ));

        assert!(matches!(
            inflate_raw(&compressed, 16),
            Err(EasyOfficeError::ResourceBudgetExceeded(_))
        ));
    }
}
