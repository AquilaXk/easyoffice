use crate::EasyOfficeError;

/// Compact `u64` word bitset replacing `HashSet<u32>` for $O(1)$ OLE2 sector cycle and claim tracking.
#[derive(Debug, Clone)]
pub struct SectorBitset {
    words: Vec<u64>,
}

impl SectorBitset {
    #[inline]
    pub fn with_capacity(max_sectors: usize) -> Self {
        Self {
            words: vec![0u64; (max_sectors + 64) / 64],
        }
    }

    #[inline]
    pub fn insert(&mut self, sec: u32) -> bool {
        let idx = sec as usize;
        let word_idx = idx >> 6;
        if word_idx >= self.words.len() {
            if word_idx > 1_048_576 {
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

/// Single-pass SWAR UTF-16LE decoder scanning 4 code units (`u64`) at a time without allocating `Vec<u16>`.
#[inline]
pub fn decode_utf16le_swar(bytes: &[u8]) -> Result<String, EasyOfficeError> {
    if bytes.len() % 2 != 0 {
        return Err(EasyOfficeError::CorruptedContainer(
            "UTF-16LE 바이트 길이가 짝수가 아닙니다.".to_string(),
        ));
    }
    let mut out = String::with_capacity(bytes.len() / 2);
    let mut offset = 0usize;

    while offset + 8 <= bytes.len() {
        let word = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
        if (word & 0xFF80_FF80_FF80_FF80) == 0 {
            out.push((word & 0x7F) as u8 as char);
            out.push(((word >> 16) & 0x7F) as u8 as char);
            out.push(((word >> 32) & 0x7F) as u8 as char);
            out.push(((word >> 48) & 0x7F) as u8 as char);
            offset += 8;
        } else {
            decode_one_utf16le_unit(bytes, &mut offset, &mut out)?;
        }
    }

    while offset < bytes.len() {
        decode_one_utf16le_unit(bytes, &mut offset, &mut out)?;
    }

    Ok(out)
}

#[inline]
fn decode_one_utf16le_unit(
    bytes: &[u8],
    offset: &mut usize,
    out: &mut String,
) -> Result<(), EasyOfficeError> {
    let u = u16::from_le_bytes([bytes[*offset], bytes[*offset + 1]]);
    *offset += 2;
    if u < 0x80 {
        out.push(u as u8 as char);
        Ok(())
    } else if !(0xD800..=0xDFFF).contains(&u) {
        out.push(char::from_u32(u as u32).unwrap());
        Ok(())
    } else if (0xD800..=0xDBFF).contains(&u) && *offset + 2 <= bytes.len() {
        let u2 = u16::from_le_bytes([bytes[*offset], bytes[*offset + 1]]);
        if (0xDC00..=0xDFFF).contains(&u2) {
            *offset += 2;
            let cp = 0x10000 + (((u as u32 - 0xD800) << 10) | (u2 as u32 - 0xDC00));
            out.push(char::from_u32(cp).unwrap());
            return Ok(());
        }
        Err(EasyOfficeError::InvalidUtf8(
            "유효하지 않은 UTF-16LE 서로게이트 쌍입니다.".to_string(),
        ))
    } else {
        Err(EasyOfficeError::InvalidUtf8(
            "유효하지 않은 UTF-16LE 시퀀스입니다.".to_string(),
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
    if *cursor + 4 > limit {
        return Err(EasyOfficeError::CorruptedContainer(
            "UTF-16LE 길이 프리픽스 읽기 범위 초과".to_string(),
        ));
    }
    let byte_len = read_u32_le(bytes, *cursor)? as usize;
    *cursor += 4;
    if byte_len % 2 != 0 || *cursor + byte_len > limit {
        return Err(EasyOfficeError::CorruptedContainer(
            "UTF-16LE 문자열 페이로드 범위 초과".to_string(),
        ));
    }
    let s = decode_utf16le_swar(&bytes[*cursor..*cursor + byte_len])?;
    *cursor += byte_len;
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
