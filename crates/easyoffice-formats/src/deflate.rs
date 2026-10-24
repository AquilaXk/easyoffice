use crate::{EasyOfficeError, ZipSecurityGuard};

const MAX_BITS: usize = 15;
const MAX_MATCH_LEN: usize = 258;
const MIN_MATCH_LEN: usize = 3;
const WINDOW_SIZE: usize = 32_768;
const HASH_BITS: usize = 12;
const HASH_SIZE: usize = 1 << HASH_BITS;
const HASH_MASK: usize = HASH_SIZE - 1;

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
        self.fill();
        if self.bit_count < n {
            return Err(EasyOfficeError::CorruptedContainer(
                "DEFLATE 비트스트림이 예기치 않게 절단되었습니다.".to_string(),
            ));
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
        for _ in 0..len {
            let b = self.read_bits(8)? as u8;
            push_checked_byte(out, b, max_output_bytes, input_len)?;
        }
        Ok(())
    }
}

struct HuffmanTable {
    max_bits: u32,
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
                entries: vec![0; 2],
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

        let table_size = 1usize << max_bits;
        let mut entries = vec![0u16; table_size];

        for (sym, &len_u8) in lengths.iter().enumerate() {
            let len = len_u8 as usize;
            if len == 0 {
                continue;
            }
            let canonical = next_code[len];
            next_code[len] += 1;
            let rev = reverse_bits(canonical, len) as usize;
            let packed = ((sym as u16) << 5) | (len as u16);
            let step = 1usize << len;
            let mut idx = rev;
            while idx < table_size {
                entries[idx] = packed;
                idx += step;
            }
        }

        Ok(Self {
            max_bits: max_bits as u32,
            entries,
        })
    }

    #[inline]
    fn decode(&self, reader: &mut BitReader<'_>) -> Result<u16, EasyOfficeError> {
        reader.fill();
        if reader.bit_count == 0 {
            return Err(EasyOfficeError::CorruptedContainer(
                "DEFLATE 허프만 심볼 디코딩 중 스트림이 종료되었습니다.".to_string(),
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
fn reverse_bits(mut val: u16, bit_len: usize) -> u16 {
    let mut out = 0u16;
    for _ in 0..bit_len {
        out = (out << 1) | (val & 1);
        val >>= 1;
    }
    out
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
        && (new_len / input_len.max(1)) > (ZipSecurityGuard::MAX_COMPRESSION_RATIO as usize)
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

fn fixed_huffman_tables() -> Result<(HuffmanTable, HuffmanTable), EasyOfficeError> {
    let mut lit_lengths = [0u8; 288];
    lit_lengths[..=143].fill(8);
    lit_lengths[144..=255].fill(9);
    lit_lengths[256..=279].fill(7);
    lit_lengths[280..=287].fill(8);
    let dist_lengths = [5u8; 32];
    Ok((
        HuffmanTable::from_code_lengths(&lit_lengths)?,
        HuffmanTable::from_code_lengths(&dist_lengths)?,
    ))
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
            for _ in 0..match_len {
                let b = out[out.len() - distance];
                out.push(b);
            }
        } else {
            return Err(EasyOfficeError::CorruptedContainer(format!(
                "유효하지 않은 DEFLATE 리터럴/길이 심볼({sym})입니다."
            )));
        }
    }
    Ok(())
}

/// Decompresses an RFC 1951 raw DEFLATE bitstream with Fail-Closed output budget enforcement.
pub fn inflate_raw(input: &[u8], max_output_bytes: usize) -> Result<Vec<u8>, EasyOfficeError> {
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
                let (lit_table, dist_table) = fixed_huffman_tables()?;
                decode_huffman_block(
                    &mut reader,
                    &lit_table,
                    &dist_table,
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
            break;
        }
    }

    Ok(out)
}

/// Decompresses either a raw RFC 1951 DEFLATE stream or an RFC 1950 zlib-wrapped DEFLATE stream.
pub fn inflate_auto(input: &[u8], max_output_bytes: usize) -> Result<Vec<u8>, EasyOfficeError> {
    if input.len() >= 2 {
        let b0 = input[0];
        let b1 = input[1];
        let is_zlib_header = (b0 & 0x0F) == 8
            && (b0 & 0x80) == 0
            && (b1 & 0x20) == 0
            && ((u16::from(b0) * 256 + u16::from(b1)).is_multiple_of(31));
        if is_zlib_header {
            if let Ok(decoded) = inflate_raw(&input[2..], max_output_bytes) {
                return Ok(decoded);
            }
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
        while self.bit_count >= 8 {
            self.out.push((self.bit_buf & 0xFF) as u8);
            self.bit_buf >>= 8;
            self.bit_count -= 8;
        }
    }

    #[inline]
    fn write_huffman_msb(&mut self, code: u16, len: usize) {
        let rev = reverse_bits(code, len);
        self.write_bits_lsb(u32::from(rev), len as u32);
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
    if len == 258 {
        return 28;
    }
    let mut idx = 0usize;
    while idx + 1 < 28 && (LENGTH_BASE[idx + 1] as usize) <= len {
        idx += 1;
    }
    idx
}

#[inline]
fn find_dist_code(dist: usize) -> usize {
    let mut idx = 0usize;
    while idx + 1 < 30 && (DIST_BASE[idx + 1] as usize) <= dist {
        idx += 1;
    }
    idx
}

/// Compresses `input` into a deterministic RFC 1951 raw DEFLATE bitstream using Fixed Huffman + LZ77 sliding window.
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
        let xml = b"<w:document><w:body><w:p><w:r><w:t>EasyOffice RFC 1951 DEFLATE</w:t></w:p></w:body></w:document>"
            .repeat(8);
        let compressed = deflate_raw(&xml);
        assert!(compressed.len() < xml.len() / 3);
        let decompressed = inflate_raw(&compressed, 1024 * 1024).unwrap();
        assert_eq!(decompressed, xml);

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
