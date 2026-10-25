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
pub fn encode_utf16le_swar(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 2);
    for u in text.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

#[inline]
const fn is_xml10_valid_char(ch: char) -> bool {
    let cp = ch as u32;
    matches!(
        cp,
        0x09 | 0x0A | 0x0D | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF
    )
}

#[inline]
pub fn unescape_xml_single_pass(text: &str) -> String {
    let bytes = text.as_bytes();
    let Some(mut amp_pos) = crate::XmlSanitizer::find_byte_swar(bytes, 0, b'&') else {
        return text.to_string();
    };
    let mut out = String::with_capacity(bytes.len());
    let mut pos = 0usize;
    loop {
        if amp_pos > pos {
            out.push_str(&text[pos..amp_pos]);
        }
        let after_idx = amp_pos + 1;
        let scan_end = (after_idx + 12).min(bytes.len());
        let mut semi_pos = None;
        let mut i = after_idx;
        while i < scan_end {
            if bytes[i] == b';' {
                semi_pos = Some(i);
                break;
            }
            i += 1;
        }
        if let Some(semi) = semi_pos {
            let entity = &text[after_idx..semi];
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
                        .filter(|&c| is_xml10_valid_char(c))
                }
                _ if entity.starts_with('#') => entity[1..]
                    .parse::<u32>()
                    .ok()
                    .and_then(char::from_u32)
                    .filter(|&c| is_xml10_valid_char(c)),
                _ => None,
            };
            if let Some(ch) = decoded {
                out.push(ch);
                pos = semi + 1;
                if let Some(next_amp) = crate::XmlSanitizer::find_byte_swar(bytes, pos, b'&') {
                    amp_pos = next_amp;
                    continue;
                }
                break;
            }
        }
        out.push('&');
        pos = after_idx;
        let Some(next_amp) = crate::XmlSanitizer::find_byte_swar(bytes, pos, b'&') else {
            break;
        };
        amp_pos = next_amp;
    }
    if pos < bytes.len() {
        out.push_str(&text[pos..]);
    }
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

pub(crate) fn parse_a1_zero_based(a1: &str) -> Option<(u32, u32)> {
    let trimmed = a1.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut col: u32 = 0;
    let mut row: u32 = 0;
    let mut letter_count = 0usize;
    let mut digit_count = 0usize;
    for b in trimmed.bytes() {
        if b == b'$' {
            continue;
        }
        if b.is_ascii_alphabetic() {
            if digit_count > 0 || letter_count >= 3 {
                return None;
            }
            letter_count += 1;
            let digit = (b.to_ascii_uppercase() - b'A') as u32 + 1;
            col = col.checked_mul(26)?.checked_add(digit)?;
        } else if b.is_ascii_digit() {
            if letter_count == 0 {
                return None;
            }
            digit_count += 1;
            row = row.checked_mul(10)?.checked_add((b - b'0') as u32)?;
        } else {
            return None;
        }
    }
    if letter_count == 0 || digit_count == 0 || col == 0 || row == 0 {
        return None;
    }
    let row0 = row - 1;
    let col0 = col - 1;
    if row0 <= 1_048_575 && col0 <= 16_383 {
        Some((row0, col0))
    } else {
        None
    }
}

#[allow(non_snake_case)]
#[inline]
pub(crate) fn parse_a1_ZeroBased(a1: &str) -> Option<(u32, u32)> {
    parse_a1_zero_based(a1)
}

pub(crate) fn extract_between<'a>(src: &'a str, start_pat: &str, end_pat: &str) -> Option<&'a str> {
    let start = src.find(start_pat)? + start_pat.len();
    let end = src[start..].find(end_pat)? + start;
    Some(&src[start..end])
}

#[inline]
fn is_xml_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

#[inline]
fn is_tag_name_boundary(b: u8) -> bool {
    matches!(b, b'>' | b'/' | b' ' | b'\t' | b'\n' | b'\r')
}

#[inline]
fn is_self_closing_open_tag(bytes: &[u8], after_tag: usize, gt_pos: usize) -> bool {
    let mut p = gt_pos;
    while p > after_tag && is_xml_ws(bytes[p - 1]) {
        p -= 1;
    }
    p > after_tag && bytes[p - 1] == b'/'
}

#[inline]
fn find_valid_open_tag(bytes: &[u8], start: usize, tag_bytes: &[u8]) -> Option<(usize, usize)> {
    let need = 1 + tag_bytes.len();
    let mut pos = start;
    while pos + need < bytes.len() {
        let lt = crate::XmlSanitizer::find_byte_swar(bytes, pos, b'<')?;
        let after_tag = lt + need;
        if after_tag >= bytes.len() {
            return None;
        }
        if &bytes[lt + 1..after_tag] == tag_bytes && is_tag_name_boundary(bytes[after_tag]) {
            let gt_pos = crate::XmlSanitizer::find_byte_swar(bytes, after_tag, b'>')?;
            return Some((lt, gt_pos));
        }
        pos = lt + 1;
    }
    None
}

fn find_matching_close_tag(
    bytes: &[u8],
    body_start: usize,
    tag_bytes: &[u8],
) -> Option<(usize, usize)> {
    let open_prefix_len = 1 + tag_bytes.len();
    let close_prefix_len = 2 + tag_bytes.len();
    let mut depth = 1usize;
    let mut scan = body_start;

    while scan < bytes.len() {
        let lt = crate::XmlSanitizer::find_byte_swar(bytes, scan, b'<')?;
        if lt + close_prefix_len < bytes.len()
            && bytes[lt + 1] == b'/'
            && &bytes[lt + 2..lt + close_prefix_len] == tag_bytes
        {
            let after_close_name = lt + close_prefix_len;
            if bytes[after_close_name] == b'>' || is_xml_ws(bytes[after_close_name]) {
                let close_gt = crate::XmlSanitizer::find_byte_swar(bytes, after_close_name, b'>')?;
                depth -= 1;
                if depth == 0 {
                    return Some((lt, close_gt + 1));
                }
                scan = close_gt + 1;
                continue;
            }
        }
        if lt + open_prefix_len < bytes.len()
            && &bytes[lt + 1..lt + open_prefix_len] == tag_bytes
            && is_tag_name_boundary(bytes[lt + open_prefix_len])
        {
            let after_open_name = lt + open_prefix_len;
            let open_gt = crate::XmlSanitizer::find_byte_swar(bytes, after_open_name, b'>')?;
            if !is_self_closing_open_tag(bytes, after_open_name, open_gt) {
                depth += 1;
            }
            scan = open_gt + 1;
            continue;
        }
        scan = lt + 1;
    }
    None
}

pub(crate) fn extract_attr(xml: &str, tag_prefix: &str, attr_name: &str) -> Option<String> {
    let tag_start = xml.find(tag_prefix)?;
    let rest = &xml[tag_start + tag_prefix.len()..];
    let tag_end = crate::XmlSanitizer::find_byte_swar(rest.as_bytes(), 0, b'>')?;
    extract_attr_from_fragment(&rest[..tag_end], attr_name)
}

pub(crate) fn extract_attr_from_fragment(fragment: &str, attr_name: &str) -> Option<String> {
    let frag_bytes = fragment.as_bytes();
    let attr_bytes = attr_name.as_bytes();
    let need_len = attr_bytes.len() + 3;
    if frag_bytes.len() < need_len || attr_bytes.is_empty() {
        return None;
    }
    let first_b = attr_bytes[0];
    let mut pos = 0usize;
    while pos + need_len <= frag_bytes.len() {
        let Some(idx) = crate::XmlSanitizer::find_byte_swar(frag_bytes, pos, first_b) else {
            break;
        };
        if idx + need_len > frag_bytes.len() {
            break;
        }
        let valid_boundary = idx == 0 || is_xml_ws(frag_bytes[idx - 1]);
        if valid_boundary && &frag_bytes[idx..idx + attr_bytes.len()] == attr_bytes {
            let mut eq_pos = idx + attr_bytes.len();
            while eq_pos < frag_bytes.len() && is_xml_ws(frag_bytes[eq_pos]) {
                eq_pos += 1;
            }
            if eq_pos < frag_bytes.len() && frag_bytes[eq_pos] == b'=' {
                let mut q_pos = eq_pos + 1;
                while q_pos < frag_bytes.len() && is_xml_ws(frag_bytes[q_pos]) {
                    q_pos += 1;
                }
                if q_pos < frag_bytes.len()
                    && (frag_bytes[q_pos] == b'"' || frag_bytes[q_pos] == b'\'')
                {
                    let quote_b = frag_bytes[q_pos];
                    let val_start = q_pos + 1;
                    let val_end =
                        crate::XmlSanitizer::find_byte_swar(frag_bytes, val_start, quote_b)?;
                    return Some(fragment[val_start..val_end].to_string());
                }
            }
        }
        pos = idx + 1;
    }
    None
}

pub(crate) fn extract_tag_bodies<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    extract_tags_with_attrs(xml, tag)
        .into_iter()
        .map(|(_, body)| body)
        .collect()
}

pub(crate) fn extract_tags_with_attrs<'a>(xml: &'a str, tag: &str) -> Vec<(&'a str, &'a str)> {
    let bytes = xml.as_bytes();
    let tag_bytes = tag.as_bytes();
    let open_prefix_len = 1 + tag_bytes.len();
    let mut out = Vec::new();
    let mut cursor = 0usize;

    while let Some((open_pos, gt_pos)) = find_valid_open_tag(bytes, cursor, tag_bytes) {
        let after_tag = open_pos + open_prefix_len;
        if is_self_closing_open_tag(bytes, after_tag, gt_pos) {
            cursor = gt_pos + 1;
            continue;
        }
        let attrs = &xml[after_tag..gt_pos];
        let body_start = gt_pos + 1;
        let Some((body_end, next_cursor)) = find_matching_close_tag(bytes, body_start, tag_bytes)
        else {
            break;
        };
        out.push((attrs, &xml[body_start..body_end]));
        cursor = next_cursor;
    }
    out
}

pub(crate) fn extract_tags_or_self_closing_with_attrs<'a>(
    xml: &'a str,
    tag: &str,
) -> Vec<(&'a str, &'a str)> {
    let bytes = xml.as_bytes();
    let tag_bytes = tag.as_bytes();
    let open_prefix_len = 1 + tag_bytes.len();
    let mut out = Vec::new();
    let mut cursor = 0usize;

    while let Some((open_pos, gt_pos)) = find_valid_open_tag(bytes, cursor, tag_bytes) {
        let after_tag = open_pos + open_prefix_len;
        if is_self_closing_open_tag(bytes, after_tag, gt_pos) {
            let mut attr_end = gt_pos;
            while attr_end > after_tag && is_xml_ws(bytes[attr_end - 1]) {
                attr_end -= 1;
            }
            if attr_end > after_tag && bytes[attr_end - 1] == b'/' {
                attr_end -= 1;
            }
            out.push((&xml[after_tag..attr_end], ""));
            cursor = gt_pos + 1;
            continue;
        }
        let attrs = &xml[after_tag..gt_pos];
        let body_start = gt_pos + 1;
        let Some((body_end, next_cursor)) = find_matching_close_tag(bytes, body_start, tag_bytes)
        else {
            break;
        };
        out.push((attrs, &xml[body_start..body_end]));
        cursor = next_cursor;
    }
    out
}

pub(crate) fn extract_ods_cells_from_row(row_xml: &str) -> Vec<(bool, &str, &str)> {
    const CELL_TAG: &[u8] = b"table:table-cell";
    const COVERED_TAG: &[u8] = b"table:covered-table-cell";
    let bytes = row_xml.as_bytes();
    let mut out = Vec::new();
    let mut cursor = 0usize;

    while cursor < bytes.len() {
        let next_cell = find_valid_open_tag(bytes, cursor, CELL_TAG);
        let next_covered = find_valid_open_tag(bytes, cursor, COVERED_TAG);
        let (is_covered, open_pos, gt_pos, tag_bytes) = match (next_cell, next_covered) {
            (Some((p1, g1)), Some((p2, g2))) => {
                if p1 <= p2 {
                    (false, p1, g1, CELL_TAG)
                } else {
                    (true, p2, g2, COVERED_TAG)
                }
            }
            (Some((p1, g1)), None) => (false, p1, g1, CELL_TAG),
            (None, Some((p2, g2))) => (true, p2, g2, COVERED_TAG),
            (None, None) => break,
        };
        let after_tag = open_pos + 1 + tag_bytes.len();
        if is_self_closing_open_tag(bytes, after_tag, gt_pos) {
            let mut attr_end = gt_pos;
            while attr_end > after_tag && is_xml_ws(bytes[attr_end - 1]) {
                attr_end -= 1;
            }
            if attr_end > after_tag && bytes[attr_end - 1] == b'/' {
                attr_end -= 1;
            }
            out.push((is_covered, &row_xml[after_tag..attr_end], ""));
            cursor = gt_pos + 1;
            continue;
        }
        let attrs = &row_xml[after_tag..gt_pos];
        let body_start = gt_pos + 1;
        let Some((body_end, next_cursor)) = find_matching_close_tag(bytes, body_start, tag_bytes)
        else {
            break;
        };
        out.push((is_covered, attrs, &row_xml[body_start..body_end]));
        cursor = next_cursor;
    }
    out
}

pub(crate) fn strip_tag_blocks(xml: &str, tag: &str) -> String {
    let bytes = xml.as_bytes();
    let tag_bytes = tag.as_bytes();
    let open_prefix_len = 1 + tag_bytes.len();
    let mut out = String::with_capacity(xml.len());
    let mut cursor = 0usize;

    while let Some((open_pos, gt_pos)) = find_valid_open_tag(bytes, cursor, tag_bytes) {
        out.push_str(&xml[cursor..open_pos]);
        let after_tag = open_pos + open_prefix_len;
        if is_self_closing_open_tag(bytes, after_tag, gt_pos) {
            cursor = gt_pos + 1;
            continue;
        }
        let body_start = gt_pos + 1;
        if let Some((_, next_cursor)) = find_matching_close_tag(bytes, body_start, tag_bytes) {
            cursor = next_cursor;
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

    #[test]
    fn test_f7_xml_tag_boundary_nesting_and_attr_parsing() {
        let odt_xml = concat!(
            "<office:text>",
            "<table:table-column/>",
            "<text:p>문단1</text:p>",
            "<table:table><table:table-row><table:table-cell><text:p>셀</text:p></table:table-cell></table:table-row></table:table>",
            "<text:p>문단2</text:p>",
            "</office:text>"
        );
        let stripped = strip_tag_blocks(odt_xml, "table:table");
        assert!(stripped.contains("문단1"));
        assert!(stripped.contains("문단2"));
        assert!(!stripped.contains("셀"));

        let nested_tbl = concat!(
            "<w:body>",
            "<w:p><w:t>머리글</w:t></w:p>",
            "<w:tbl><w:tr><w:tc><w:tbl><w:tr><w:tc><w:p><w:t>내부표</w:t></w:p></w:tc></w:tr></w:tbl></w:tc></w:tr></w:tbl>",
            "<w:tbl/>",
            "<w:p><w:t>바닥글</w:t></w:p>",
            "</w:body>"
        );
        let stripped_nested = strip_tag_blocks(nested_tbl, "w:tbl");
        assert!(stripped_nested.contains("머리글"));
        assert!(stripped_nested.contains("바닥글"));
        assert!(!stripped_nested.contains("내부표"));

        let extracted_nested = extract_tags_with_attrs(nested_tbl, "w:tbl");
        assert_eq!(extracted_nested.len(), 1);
        assert!(extracted_nested[0].1.contains("내부표"));

        let newline_tag = "<c\n  other_r=\"Z99\"\n  r='B3'\n  t=\"s\"><v>0</v></c>";
        let tags = extract_tags_with_attrs(newline_tag, "c");
        assert_eq!(tags.len(), 1);
        assert_eq!(
            extract_attr_from_fragment(tags[0].0, "r"),
            Some("B3".to_string())
        );
    }

    #[test]
    fn test_f10_parse_a1_zero_based_checked_bounds() {
        assert_eq!(parse_a1_ZeroBased("A1"), Some((0, 0)));
        assert_eq!(parse_a1_ZeroBased("XFD1048576"), Some((1_048_575, 16_383)));
        assert_eq!(parse_a1_ZeroBased("XFE1"), None);
        assert_eq!(parse_a1_ZeroBased("A1048577"), None);
        assert_eq!(parse_a1_ZeroBased("AAAA1"), None);
        assert_eq!(parse_a1_ZeroBased("ZZZZZZZZZZ999999999999"), None);
    }
}
