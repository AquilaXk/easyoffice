//! Test-only oracle: the inflate implementation from before the fast-path port, preserved
//! verbatim (renamed) so the fuzz tests can compare the new decoder against it.

#![allow(clippy::all)]

use super::*;

impl<'a> BitReader<'a> {
    fn ref_copy_aligned_bytes(
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

struct RefHuffmanTable {
    max_bits: u32,
    fast_lut: [u16; FAST_LUT_SIZE],
    entries: Vec<u16>,
}

impl RefHuffmanTable {
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
fn ref_push_checked_byte(
    out: &mut Vec<u8>,
    b: u8,
    max_output_bytes: usize,
    input_len: usize,
) -> Result<(), EasyOfficeError> {
    check_budget(out.len() + 1, max_output_bytes, input_len)?;
    out.push(b);
    Ok(())
}

fn ref_fixed_huffman_tables() -> &'static (RefHuffmanTable, RefHuffmanTable) {
    static REF_FIXED_TABLES: OnceLock<(RefHuffmanTable, RefHuffmanTable)> = OnceLock::new();
    REF_FIXED_TABLES.get_or_init(|| {
        let mut lit_lengths = [0u8; 288];
        lit_lengths[..=143].fill(8);
        lit_lengths[144..=255].fill(9);
        lit_lengths[256..=279].fill(7);
        lit_lengths[280..=287].fill(8);
        let dist_lengths = [5u8; 32];
        (
            RefHuffmanTable::from_code_lengths(&lit_lengths).expect("valid RFC 1951 fixed lit"),
            RefHuffmanTable::from_code_lengths(&dist_lengths).expect("valid RFC 1951 fixed dist"),
        )
    })
}

fn ref_read_dynamic_huffman_tables(
    reader: &mut BitReader<'_>,
) -> Result<(RefHuffmanTable, RefHuffmanTable), EasyOfficeError> {
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
    let cl_table = RefHuffmanTable::from_code_lengths(&cl_lengths)?;

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
        RefHuffmanTable::from_code_lengths(lit_slice)?,
        RefHuffmanTable::from_code_lengths(dist_slice)?,
    ))
}

fn ref_decode_huffman_block(
    reader: &mut BitReader<'_>,
    lit_table: &RefHuffmanTable,
    dist_table: &RefHuffmanTable,
    out: &mut Vec<u8>,
    max_output_bytes: usize,
    input_len: usize,
) -> Result<(), EasyOfficeError> {
    loop {
        let sym = lit_table.decode(reader)?;
        if sym < 256 {
            ref_push_checked_byte(out, sym as u8, max_output_bytes, input_len)?;
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
                let base_start = out.len() - distance;
                let mut remaining = match_len;
                while remaining > 0 {
                    let avail = out.len() - base_start;
                    let step = remaining.min(avail);
                    out.extend_from_within(base_start..base_start + step);
                    remaining -= step;
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

/// The pre-port inflate (single-level tables, `Vec` output), kept as the oracle for the fuzz tests.
pub(super) fn reference_inflate_raw(
    input: &[u8],
    max_output_bytes: usize,
) -> Result<Vec<u8>, EasyOfficeError> {
    if input.is_empty() {
        return Err(EasyOfficeError::CorruptedContainer("empty".to_string()));
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
                    return Err(EasyOfficeError::CorruptedContainer("nlen".to_string()));
                }
                reader.ref_copy_aligned_bytes(
                    len as usize,
                    &mut out,
                    max_output_bytes,
                    input.len(),
                )?;
            }
            1 => {
                let (lit, dist) = ref_fixed_huffman_tables();
                ref_decode_huffman_block(
                    &mut reader,
                    lit,
                    dist,
                    &mut out,
                    max_output_bytes,
                    input.len(),
                )?;
            }
            2 => {
                let (lit, dist) = ref_read_dynamic_huffman_tables(&mut reader)?;
                ref_decode_huffman_block(
                    &mut reader,
                    &lit,
                    &dist,
                    &mut out,
                    max_output_bytes,
                    input.len(),
                )?;
            }
            _ => return Err(EasyOfficeError::CorruptedContainer("btype".to_string())),
        }
        if bfinal != 0 {
            break;
        }
    }
    Ok(out)
}
