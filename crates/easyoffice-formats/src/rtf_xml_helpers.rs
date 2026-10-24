use crate::{ContainerFamily, DocumentFormat, EasyOfficeError, FormatDetector};

/// Rich Text Format (`RTF 1.9.1`) payload model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtfPayload {
    pub title: String,
    pub paragraphs: Vec<String>,
    pub tables: Vec<Vec<Vec<String>>>,
}

/// Rich Text Format (`RTF 1.9.1`) encoder and decoder with Unicode `\uN?` escapes and table support.
pub struct RtfCodec;

impl RtfCodec {
    pub fn encode(payload: &RtfPayload) -> Vec<u8> {
        let mut lines: Vec<String> = Vec::with_capacity(payload.paragraphs.len() + 8);
        lines.push("{\\rtf1\\ansi\\deff0".to_string());
        lines.push(format!(
            "{{\\info{{\\title {}}}}}",
            Self::escape_rtf_text(&payload.title)
        ));
        for p in &payload.paragraphs {
            lines.push(format!("\\pard {}\\par", Self::escape_rtf_text(p)));
        }
        for (t_idx, tbl) in payload.tables.iter().enumerate() {
            for row in tbl {
                let cells_str = row
                    .iter()
                    .map(|c| format!("\\cellx {}\\cell", Self::escape_rtf_text(c)))
                    .collect::<Vec<_>>()
                    .join(" ");
                lines.push(format!(
                    "{{\\trowd{{\\*\\tblidx {t_idx}}} {cells_str}\\row}}"
                ));
            }
        }
        lines.push("}".to_string());
        lines.join("\n").into_bytes()
    }

    pub fn decode(bytes: &[u8]) -> Result<RtfPayload, EasyOfficeError> {
        let fmt = FormatDetector::detect(bytes, Some("rtf"))?;
        if fmt != DocumentFormat::Rtf {
            return Err(EasyOfficeError::SignatureMismatch {
                expected: ContainerFamily::PlainTextUtf8,
                detail: "RTF 시그니처 불일치".to_string(),
            });
        }
        let raw = std::str::from_utf8(bytes)
            .map_err(|_| EasyOfficeError::InvalidUtf8("RTF UTF-8 디코딩 실패".to_string()))?;

        let mut title = "RTF 문서".to_string();
        let mut paragraphs: Vec<String> = Vec::new();
        let mut tables: Vec<(usize, Vec<Vec<String>>)> = Vec::new();

        for raw_line in raw.lines() {
            let line = raw_line.trim();
            if let Some(rest) = line
                .strip_prefix("{\\info{\\title ")
                .and_then(|s| s.strip_suffix("}}"))
            {
                title = Self::unescape_rtf_text(rest);
            } else if let Some(rest) = line
                .strip_prefix("\\pard ")
                .and_then(|s| s.strip_suffix("\\par"))
            {
                paragraphs.push(Self::unescape_rtf_text(rest));
            } else if line.starts_with("{\\trowd") && line.ends_with("\\row}") {
                let tbl_idx = extract_between(line, "{\\*\\tblidx ", "}")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(0);
                let mut row_cells = Vec::new();
                for seg in line.split("\\cellx ").skip(1) {
                    if let Some(end_pos) = seg.find("\\cell") {
                        row_cells.push(Self::unescape_rtf_text(&seg[..end_pos]));
                    }
                }
                if let Some(last) = tables.last_mut().filter(|(idx, _)| *idx == tbl_idx) {
                    last.1.push(row_cells);
                } else {
                    tables.push((tbl_idx, vec![row_cells]));
                }
            }
        }

        Ok(RtfPayload {
            title,
            paragraphs,
            tables: tables.into_iter().map(|(_, rows)| rows).collect(),
        })
    }

    pub fn escape_rtf_text(input: &str) -> String {
        let mut out = String::with_capacity(input.len() * 2);
        for unit in input.encode_utf16() {
            match unit {
                0x5C => out.push_str("\\\\"),
                0x7B => out.push_str("\\{"),
                0x7D => out.push_str("\\}"),
                0x0A => out.push_str("\\line "),
                0x20..=0x7E => out.push(unit as u8 as char),
                _ => {
                    let signed = unit as i16;
                    out.push_str(&format!("\\u{signed}?"));
                }
            }
        }
        out
    }

    pub fn unescape_rtf_text(input: &str) -> String {
        if !input.contains('\\') && !input.contains('{') && !input.contains('}') {
            return input.trim().to_string();
        }
        let units: Vec<u16> = input.encode_utf16().collect();
        let mut out_units: Vec<u16> = Vec::with_capacity(units.len());
        let mut i = 0usize;

        while i < units.len() {
            let u = units[i];
            if u == 0x5C && i + 1 < units.len() {
                let next = units[i + 1];
                if next == 0x5C || next == 0x7B || next == 0x7D {
                    out_units.push(next);
                    i += 2;
                    continue;
                }
                if i + 5 <= units.len()
                    && units[i + 1] == b'l' as u16
                    && units[i + 2] == b'i' as u16
                    && units[i + 3] == b'n' as u16
                    && units[i + 4] == b'e' as u16
                {
                    out_units.push(b'\n' as u16);
                    i += 5;
                    if i < units.len() && units[i] == b' ' as u16 {
                        i += 1;
                    }
                    continue;
                }
                if next == b'u' as u16 {
                    let mut num_start = i + 2;
                    let mut neg = false;
                    if num_start < units.len() && units[num_start] == b'-' as u16 {
                        neg = true;
                        num_start += 1;
                    }
                    let mut num_end = num_start;
                    let mut acc: i32 = 0;
                    while num_end < units.len()
                        && units[num_end] >= b'0' as u16
                        && units[num_end] <= b'9' as u16
                    {
                        acc = acc
                            .wrapping_mul(10)
                            .wrapping_add((units[num_end] - b'0' as u16) as i32);
                        num_end += 1;
                    }
                    if num_end > num_start {
                        let signed_val = if neg { -acc } else { acc };
                        out_units.push((signed_val as i16) as u16);
                        i = num_end;
                        if i < units.len() && units[i] == b'?' as u16 {
                            i += 1;
                        }
                        continue;
                    }
                }
            }
            out_units.push(u);
            i += 1;
        }

        String::from_utf16_lossy(&out_units)
    }
}

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
    if !bytes.len().is_multiple_of(2) {
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
    if !bytes.len().is_multiple_of(2) {
        return Err(EasyOfficeError::CorruptedContainer(
            "UTF-16LE 바이트 길이가 짝수가 아닙니다.".to_string(),
        ));
    }
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16(&units).map_err(|_| {
        EasyOfficeError::InvalidUtf8("유효하지 않은 UTF-16LE 시퀀스입니다.".to_string())
    })
}

#[inline]
pub fn encode_utf16le_swar(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2);
    for u in text.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

/// Single-pass XML entity unescaper with `&`-absence fast path and numeric entity decoding.
#[inline]
pub fn unescape_xml_single_pass(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp_pos) = rest.find('&') {
        out.push_str(&rest[..amp_pos]);
        let after_amp = &rest[amp_pos + 1..];
        if let Some(semi_offset) = after_amp
            .as_bytes()
            .iter()
            .take(12)
            .position(|&b| b == b';')
        {
            let entity = &after_amp[..semi_offset];
            let decoded = match entity {
                "quot" => Some('"'),
                "apos" => Some('\''),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "amp" => Some('&'),
                _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                    u32::from_str_radix(&entity[2..], 16)
                        .ok()
                        .and_then(char::from_u32)
                }
                _ if entity.starts_with('#') => {
                    entity[1..].parse::<u32>().ok().and_then(char::from_u32)
                }
                _ => None,
            };
            if let Some(ch) = decoded {
                out.push(ch);
                rest = &after_amp[semi_offset + 1..];
                continue;
            }
        }
        out.push('&');
        rest = after_amp;
    }
    out.push_str(rest);
    out
}

/// Baseline 5-pass `.replace` XML unescaper retained for benchmark verification.
pub fn unescape_xml_five_pass_baseline(text: &str) -> String {
    text.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
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
    let bytes = encode_utf16le_swar(text);
    buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(&bytes);
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
    if !byte_len.is_multiple_of(2) || *cursor + byte_len > limit {
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

pub(crate) fn format_col_letters(col: u32) -> String {
    let mut n = col + 1;
    let mut chars = Vec::new();
    while n > 0 {
        let rem = ((n - 1) % 26) as u8;
        chars.push((b'A' + rem) as char);
        n = (n - 1) / 26;
    }
    chars.into_iter().rev().collect()
}

#[allow(non_snake_case)]
pub(crate) fn parse_a1_ZeroBased(a1: &str) -> Option<(u32, u32)> {
    let mut col: u32 = 0;
    let mut row: u32 = 0;
    for b in a1.bytes() {
        if b.is_ascii_alphabetic() {
            col = col * 26 + ((b.to_ascii_uppercase() - b'A') as u32 + 1);
        } else if b.is_ascii_digit() {
            row = row * 10 + ((b - b'0') as u32);
        }
    }
    if col > 0 && row > 0 {
        Some((row - 1, col - 1))
    } else {
        None
    }
}

pub(crate) fn extract_between<'a>(src: &'a str, start_pat: &str, end_pat: &str) -> Option<&'a str> {
    let start = src.find(start_pat)? + start_pat.len();
    let end = src[start..].find(end_pat)? + start;
    Some(&src[start..end])
}

pub(crate) fn extract_attr(xml: &str, tag_prefix: &str, attr_name: &str) -> Option<String> {
    let tag_start = xml.find(tag_prefix)?;
    let rest = &xml[tag_start..];
    let tag_end = rest.find('>')?;
    extract_attr_from_fragment(&rest[..tag_end], attr_name)
}

pub(crate) fn extract_attr_from_fragment(fragment: &str, attr_name: &str) -> Option<String> {
    let pattern = format!("{attr_name}=\"");
    let start = fragment.find(&pattern)? + pattern.len();
    let end = fragment[start..].find('"')? + start;
    Some(fragment[start..end].to_string())
}

pub(crate) fn extract_tag_bodies<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    extract_tags_with_attrs(xml, tag)
        .into_iter()
        .map(|(_, body)| body)
        .collect()
}

pub(crate) fn extract_tags_with_attrs<'a>(xml: &'a str, tag: &str) -> Vec<(&'a str, &'a str)> {
    let open_prefix = format!("<{tag}");
    let close_tag = format!("</{tag}>");
    let mut out = Vec::new();
    let mut cursor = 0usize;

    while let Some(rel_open) = xml[cursor..].find(&open_prefix) {
        let open_pos = cursor + rel_open;
        let after_tag = open_pos + open_prefix.len();
        if after_tag < xml.len() {
            let next_ch = xml.as_bytes()[after_tag];
            if next_ch != b'>' && next_ch != b' ' && next_ch != b'/' {
                cursor = after_tag;
                continue;
            }
        }
        let Some(rel_gt) = xml[after_tag..].find('>') else {
            break;
        };
        let gt_pos = after_tag + rel_gt;
        if xml.as_bytes()[gt_pos - 1] == b'/' {
            cursor = gt_pos + 1;
            continue;
        }
        let attrs = &xml[after_tag..gt_pos];
        let body_start = gt_pos + 1;
        let Some(rel_close) = xml[body_start..].find(&close_tag) else {
            break;
        };
        let body_end = body_start + rel_close;
        out.push((attrs, &xml[body_start..body_end]));
        cursor = body_end + close_tag.len();
    }
    out
}

pub(crate) fn strip_tag_blocks(xml: &str, tag: &str) -> String {
    let open_prefix = format!("<{tag}");
    let close_tag = format!("</{tag}>");
    let mut out = String::with_capacity(xml.len());
    let mut cursor = 0usize;

    while let Some(rel_open) = xml[cursor..].find(&open_prefix) {
        let open_pos = cursor + rel_open;
        out.push_str(&xml[cursor..open_pos]);
        if let Some(rel_close) = xml[open_pos..].find(&close_tag) {
            cursor = open_pos + rel_close + close_tag.len();
        } else {
            cursor = xml.len();
            break;
        }
    }
    out.push_str(&xml[cursor..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sector_bitset_and_swar_utf16_and_xml_unescape_parity() {
        let mut bitset = SectorBitset::with_capacity(200);
        assert!(bitset.insert(0));
        assert!(!bitset.insert(0));
        assert!(bitset.insert(63));
        assert!(bitset.insert(64));
        assert!(!bitset.insert(64));
        assert!(bitset.insert(512));
        assert!(!bitset.insert(512));

        for sample in [
            "Pure ASCII 64-bit SWAR fast path test with > 32 chars!",
            "행정안전부 표준 전자문서 한글·영문 혼합 UTF-16LE 라운드트립",
            "Emoji surrogate pair 🚀✨ and Hangul 가나다라",
        ] {
            let encoded = encode_utf16le_swar(sample);
            let swar = decode_utf16le_swar(&encoded).unwrap();
            let base = decode_utf16le_vec_baseline(&encoded).unwrap();
            assert_eq!(swar, sample);
            assert_eq!(swar, base);
        }

        let xml_sample = "Plain text without entities &amp; with &lt;tag attr=&quot;val&quot;&gt; and &#xAC00; &#44032; &apos;ok&apos;";
        assert_eq!(
            unescape_xml_single_pass("No entities at all"),
            "No entities at all"
        );
        assert_eq!(
            unescape_xml_single_pass("&quot;&apos;&lt;&gt;&amp;"),
            unescape_xml_five_pass_baseline("&quot;&apos;&lt;&gt;&amp;")
        );
        assert!(unescape_xml_single_pass(xml_sample).contains("가 가 'ok'"));
    }
}
