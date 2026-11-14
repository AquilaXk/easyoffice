use crate::EasyOfficeError;

const MAX_BITSET_WORDS: usize = 1_048_576;
const MAX_BITSET_SECTORS: usize = MAX_BITSET_WORDS * 64;
const MAX_INITIAL_WORDS: usize = 65_536;

/// Compact `u64` word bitset replacing `HashSet<u32>` for $O(1)$ OLE2 sector cycle and claim tracking.
#[derive(Debug, Clone)]
pub struct SectorBitset {
    words: Vec<u64>,
    limit: usize,
}

impl SectorBitset {
    #[inline]
    pub fn with_capacity(max_sectors: usize) -> Self {
        let initial_words = (max_sectors >> 6).saturating_add(1).min(MAX_INITIAL_WORDS);
        Self {
            words: vec![0u64; initial_words],
            limit: MAX_BITSET_SECTORS,
        }
    }

    #[inline]
    pub fn with_limit(max_sectors: usize) -> Self {
        let limit = max_sectors.min(MAX_BITSET_SECTORS);
        let initial_words = (limit >> 6).saturating_add(1).min(MAX_INITIAL_WORDS);
        Self {
            words: vec![0u64; initial_words],
            limit,
        }
    }

    #[inline]
    pub fn insert(&mut self, sec: u32) -> bool {
        let idx = sec as usize;
        if idx >= self.limit {
            return false;
        }
        let word_idx = idx >> 6;
        if word_idx >= self.words.len() {
            if word_idx >= MAX_BITSET_WORDS {
                return false;
            }
            self.words.resize(word_idx + 1, 0);
        }
        let mask = 1u64 << (idx & 63);
        if (self.words[word_idx] & mask) != 0 {
            false
        } else {
            self.words[word_idx] |= mask;
            true
        }
    }
}

#[inline]
const fn pack_utf16le_ascii4(word: u64) -> [u8; 4] {
    let lo16 = (word | (word >> 8)) & 0x0000_FFFF_0000_FFFF;
    let lo32 = (lo16 | (lo16 >> 16)) as u32;
    lo32.to_le_bytes()
}

/// Single-pass SWAR UTF-16LE decoder scanning 4 code units (`u64`) at a time without allocating `Vec<u16>`.
#[inline]
pub fn decode_utf16le_swar(bytes: &[u8]) -> Result<String, EasyOfficeError> {
    decode_utf16_swar::<false>(bytes)
}

/// Single-pass SWAR UTF-16BE decoder scanning 4 code units (`u64`) at a time without allocating `Vec<u16>`.
#[inline]
pub fn decode_utf16be_swar(bytes: &[u8]) -> Result<String, EasyOfficeError> {
    decode_utf16_swar::<true>(bytes)
}

#[inline]
fn decode_utf16_swar<const BE: bool>(bytes: &[u8]) -> Result<String, EasyOfficeError> {
    if bytes.len() % 2 != 0 {
        let msg = if BE {
            "UTF-16BE 바이트 길이가 짝수가 아닙니다."
        } else {
            "UTF-16LE 바이트 길이가 짝수가 아닙니다."
        };
        return Err(EasyOfficeError::CorruptedContainer(msg.to_string()));
    }
    let ascii_mask: u64 = if BE {
        0x80FF_80FF_80FF_80FF
    } else {
        0xFF80_FF80_FF80_FF80
    };
    let mut out = String::with_capacity(bytes.len() / 2);
    let mut offset = 0usize;
    let mut ascii_buf = [0u8; 64];

    while offset + 8 <= bytes.len() {
        let word = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
        if (word & ascii_mask) == 0 {
            let mut filled = 4usize;
            let norm = if BE { word >> 8 } else { word };
            ascii_buf[0..4].copy_from_slice(&pack_utf16le_ascii4(norm));
            offset += 8;
            while filled < ascii_buf.len() && offset + 8 <= bytes.len() {
                let next = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
                if (next & ascii_mask) != 0 {
                    break;
                }
                let next_norm = if BE { next >> 8 } else { next };
                ascii_buf[filled..filled + 4].copy_from_slice(&pack_utf16le_ascii4(next_norm));
                filled += 4;
                offset += 8;
            }
            if let Ok(s) = std::str::from_utf8(&ascii_buf[..filled]) {
                out.push_str(s);
            }
        } else {
            decode_one_utf16_unit::<BE>(bytes, &mut offset, &mut out)?;
        }
    }

    while offset < bytes.len() {
        decode_one_utf16_unit::<BE>(bytes, &mut offset, &mut out)?;
    }

    Ok(out)
}

#[inline]
fn decode_one_utf16_unit<const BE: bool>(
    bytes: &[u8],
    offset: &mut usize,
    out: &mut String,
) -> Result<(), EasyOfficeError> {
    let pair = [bytes[*offset], bytes[*offset + 1]];
    let u = if BE {
        u16::from_be_bytes(pair)
    } else {
        u16::from_le_bytes(pair)
    };
    *offset += 2;
    if u < 0x80 {
        out.push(u as u8 as char);
        Ok(())
    } else if !(0xD800..=0xDFFF).contains(&u) {
        out.push(char::from_u32(u as u32).unwrap());
        Ok(())
    } else if (0xD800..=0xDBFF).contains(&u) && *offset + 2 <= bytes.len() {
        let pair2 = [bytes[*offset], bytes[*offset + 1]];
        let u2 = if BE {
            u16::from_be_bytes(pair2)
        } else {
            u16::from_le_bytes(pair2)
        };
        if (0xDC00..=0xDFFF).contains(&u2) {
            *offset += 2;
            let cp = 0x10000 + (((u as u32 - 0xD800) << 10) | (u2 as u32 - 0xDC00));
            out.push(char::from_u32(cp).unwrap());
            return Ok(());
        }
        Err(EasyOfficeError::InvalidUtf8(
            "유효하지 않은 UTF-16 서로게이트 쌍입니다.".to_string(),
        ))
    } else {
        Err(EasyOfficeError::InvalidUtf8(
            "유효하지 않은 UTF-16 시퀀스입니다.".to_string(),
        ))
    }
}

/// Baseline two-pass `Vec<u16>` UTF-16LE decoder retained for benchmark verification.
pub fn decode_utf16le_vec_baseline(bytes: &[u8]) -> Result<String, EasyOfficeError> {
    if bytes.len() % 2 != 0 {
        return Err(EasyOfficeError::CorruptedContainer(
            "UTF-16LE 바이트 길이가 짝수가 아닙니다.".to_string(),
        ));
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16(&units).map_err(|_| {
        EasyOfficeError::InvalidUtf8("유효하지 않은 UTF-16LE 시퀀스입니다.".to_string())
    })
}

#[inline]
pub const fn expand_ascii4_to_utf16le_u64(four_bytes: u32) -> u64 {
    let mut x = four_bytes as u64;
    x = (x | (x << 16)) & 0x0000_FFFF_0000_FFFF;
    x = (x | (x << 8)) & 0x00FF_00FF_00FF_00FF;
    x
}

#[inline]
pub fn encode_utf16le_swar_into(text: &str, out: &mut Vec<u8>) {
    out.reserve(text.len() * 2);
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut pos = 0usize;
    let mut u16_buf = [0u16; 2];

    while pos + 8 <= len {
        let word = u64::from_le_bytes([
            bytes[pos],
            bytes[pos + 1],
            bytes[pos + 2],
            bytes[pos + 3],
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]);
        if (word & 0x8080_8080_8080_8080) == 0 {
            let lo = expand_ascii4_to_utf16le_u64(word as u32);
            let hi = expand_ascii4_to_utf16le_u64((word >> 32) as u32);
            out.extend_from_slice(&lo.to_le_bytes());
            out.extend_from_slice(&hi.to_le_bytes());
            pos += 8;
        } else if ((word as u32) & 0x8080_8080) == 0 {
            let lo = expand_ascii4_to_utf16le_u64(word as u32);
            out.extend_from_slice(&lo.to_le_bytes());
            pos += 4;
        } else if bytes[pos] < 0x80 {
            out.extend_from_slice(&[bytes[pos], 0]);
            pos += 1;
        } else if let Some(ch) = text[pos..].chars().next() {
            for u in ch.encode_utf16(&mut u16_buf) {
                out.extend_from_slice(&u.to_le_bytes());
            }
            pos += ch.len_utf8();
        } else {
            break;
        }
    }

    if pos + 4 <= len {
        let word4 =
            u32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]);
        if (word4 & 0x8080_8080) == 0 {
            let lo = expand_ascii4_to_utf16le_u64(word4);
            out.extend_from_slice(&lo.to_le_bytes());
            pos += 4;
        }
    }

    while pos < len {
        if bytes[pos] < 0x80 {
            out.extend_from_slice(&[bytes[pos], 0]);
            pos += 1;
        } else if let Some(ch) = text[pos..].chars().next() {
            for u in ch.encode_utf16(&mut u16_buf) {
                out.extend_from_slice(&u.to_le_bytes());
            }
            pos += ch.len_utf8();
        } else {
            break;
        }
    }
}

#[inline]
pub fn encode_utf16le_swar(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2);
    encode_utf16le_swar_into(text, &mut out);
    out
}

/// Baseline scalar `encode_utf16()` UTF-16LE encoder retained for benchmark verification.
pub fn encode_utf16le_scalar_baseline(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2);
    for u in text.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

#[inline]
pub(crate) fn encode_utf16le(text: &str) -> Vec<u8> {
    encode_utf16le_swar(text)
}

#[inline]
pub(crate) fn decode_utf16le(bytes: &[u8]) -> Result<String, EasyOfficeError> {
    decode_utf16le_swar(bytes)
}

pub(crate) fn append_len_prefixed_utf16le(buf: &mut Vec<u8>, text: &str) {
    let len_pos = buf.len();
    buf.extend_from_slice(&0u32.to_le_bytes());
    let start_pos = buf.len();
    encode_utf16le_swar_into(text, buf);
    let byte_len = (buf.len() - start_pos) as u32;
    buf[len_pos..len_pos + 4].copy_from_slice(&byte_len.to_le_bytes());
}

pub(crate) fn read_len_prefixed_utf16le(
    bytes: &[u8],
    cursor: &mut usize,
    limit: usize,
) -> Result<String, EasyOfficeError> {
    let bound = limit.min(bytes.len());
    let Some(after_len) = cursor.checked_add(4) else {
        return Err(EasyOfficeError::CorruptedContainer(
            "UTF-16LE 길이 프리픽스 읽기 범위 초과".to_string(),
        ));
    };
    if after_len > bound {
        return Err(EasyOfficeError::CorruptedContainer(
            "UTF-16LE 길이 프리픽스 읽기 범위 초과".to_string(),
        ));
    }
    let byte_len = read_u32_le(bytes, *cursor)? as usize;
    let Some(end) = after_len.checked_add(byte_len) else {
        return Err(EasyOfficeError::CorruptedContainer(
            "UTF-16LE 문자열 페이로드 범위 초과".to_string(),
        ));
    };
    if byte_len % 2 != 0 || end > bound {
        return Err(EasyOfficeError::CorruptedContainer(
            "UTF-16LE 문자열 페이로드 범위 초과".to_string(),
        ));
    }
    let s = decode_utf16le_swar(&bytes[after_len..end])?;
    *cursor = end;
    Ok(s)
}

#[inline]
pub(crate) fn write_u16_le(buf: &mut [u8], offset: usize, val: u16) {
    buf[offset..offset + 2].copy_from_slice(&val.to_le_bytes());
}

#[inline]
pub(crate) fn write_u32_le(buf: &mut [u8], offset: usize, val: u32) {
    buf[offset..offset + 4].copy_from_slice(&val.to_le_bytes());
}

#[inline]
pub(crate) fn write_u64_le(buf: &mut [u8], offset: usize, val: u64) {
    buf[offset..offset + 8].copy_from_slice(&val.to_le_bytes());
}

#[inline]
pub(crate) fn read_u16_le(bytes: &[u8], offset: usize) -> Result<u16, EasyOfficeError> {
    if offset + 2 > bytes.len() {
        return Err(EasyOfficeError::CorruptedContainer(
            "OLE2 u16 읽기 범위 초과".to_string(),
        ));
    }
    Ok(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]))
}

#[inline]
pub(crate) fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32, EasyOfficeError> {
    if offset + 4 > bytes.len() {
        return Err(EasyOfficeError::CorruptedContainer(
            "OLE2 u32 읽기 범위 초과".to_string(),
        ));
    }
    Ok(u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]))
}

#[inline]
pub(crate) fn read_u64_le(bytes: &[u8], offset: usize) -> Result<u64, EasyOfficeError> {
    if offset + 8 > bytes.len() {
        return Err(EasyOfficeError::CorruptedContainer(
            "OLE2 u64 읽기 범위 초과".to_string(),
        ));
    }
    Ok(u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sector_bitset_limit_and_read_len_prefixed_bounds_fail_closed() {
        let mut bounded = SectorBitset::with_limit(200);
        assert!(bounded.insert(0));
        assert!(!bounded.insert(0));
        assert!(bounded.insert(199));
        assert!(!bounded.insert(200));

        // Unbounded hint `with_capacity(usize::MAX)` must not overflow or OOM.
        let mut huge = SectorBitset::with_capacity(usize::MAX);
        assert!(huge.insert(5));

        // `limit` larger than `bytes.len()` or overflowing `cursor + byte_len` must return Err, not panic.
        let payload = [0x04, 0x00, 0x00, 0x00, b'A', 0x00];
        let mut cursor = 0usize;
        assert!(read_len_prefixed_utf16le(&payload, &mut cursor, 1024).is_err());

        let overflow_len = [0xFE, 0xFF, 0xFF, 0xFF, b'A', 0x00];
        let mut cursor2 = 0usize;
        assert!(read_len_prefixed_utf16le(&overflow_len, &mut cursor2, usize::MAX).is_err());

        // SWAR ASCII batch & mixed UTF-16LE roundtrip parity.
        let ascii_long = "A".repeat(145) + "XYZ123!";
        let mixed = format!("{ascii_long} 한글 가나다 🚀✨ {}", "Q".repeat(69));
        let encoded = encode_utf16le_swar(&mixed);
        assert_eq!(decode_utf16le_swar(&encoded).unwrap(), mixed);
        assert_eq!(
            decode_utf16le_swar(&encoded).unwrap(),
            decode_utf16le_vec_baseline(&encoded).unwrap()
        );
    }
}
