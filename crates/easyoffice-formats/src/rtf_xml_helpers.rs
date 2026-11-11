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
        let raw_cow = match std::str::from_utf8(bytes) {
            Ok(s) => std::borrow::Cow::Borrowed(s),
            Err(_) => std::borrow::Cow::Owned(crate::cp949::decode_cp949_or_latin1_bytes(bytes)),
        };
        let raw = raw_cow.as_ref();

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
                .strip_prefix("\\pard")
                .and_then(|s| s.strip_suffix("\\par"))
            {
                let text = crate::rich_rtf_writer::strip_rtf_format_words(rest);
                paragraphs.push(Self::unescape_rtf_text(&text));
            } else if line.starts_with("{\\trowd") && line.ends_with("\\row}") {
                let tbl_idx = extract_between(line, "{\\*\\tblidx ", "}")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(0);
                let row_cells = crate::rich_rtf_table::flat_row_cells(line)
                    .iter()
                    .map(|c| Self::unescape_rtf_text(c))
                    .collect();
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
        use std::fmt::Write;
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
                    let _ = write!(out, "\\u{signed}?");
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
        let mut unicode_skip = crate::rtf_escape::DEFAULT_UNICODE_SKIP;

        while i < units.len() {
            if units[i] == crate::rtf_escape::RTF_BACKSLASH {
                if let Some((decoded, next_i)) =
                    crate::rtf_escape::decode_rtf_escape(&units, i, &mut unicode_skip)
                {
                    out_units.extend(decoded);
                    i = next_i;
                    continue;
                }
            }
            out_units.push(units[i]);
            i += 1;
        }

        String::from_utf16_lossy(&out_units)
    }
}

pub use crate::utf16_swar::*;

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
pub(crate) fn write_col_letters(buf: &mut [u8; 4], col_zero_based: usize) -> &str {
    let mut n = col_zero_based.saturating_add(1);
    let mut pos = buf.len();
    while n > 0 && pos > 0 {
        pos -= 1;
        let rem = ((n - 1) % 26) as u8;
        buf[pos] = b'A' + rem;
        n = (n - 1) / 26;
    }
    std::str::from_utf8(&buf[pos..]).unwrap_or("")
}

pub(crate) fn format_col_letters(col: u32) -> String {
    let mut buf = [0u8; 4];
    write_col_letters(&mut buf, col as usize).to_string()
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
fn find_tag_close_gt(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'"' | b'\'' => {
                let q = bytes[i];
                i = crate::XmlSanitizer::find_byte_swar(bytes, i + 1, q)? + 1;
            }
            b'>' => return Some(i),
            _ => i += 1,
        }
    }
    None
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
            let gt_pos = find_tag_close_gt(bytes, after_tag)?;
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
                let close_gt = find_tag_close_gt(bytes, after_close_name)?;
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
            let open_gt = find_tag_close_gt(bytes, after_open_name)?;
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

pub(crate) fn extract_attr<'a>(xml: &'a str, tag_prefix: &str, attr_name: &str) -> Option<&'a str> {
    let tag_start = xml.find(tag_prefix)?;
    let rest = &xml[tag_start + tag_prefix.len()..];
    let tag_end = find_tag_close_gt(rest.as_bytes(), 0)?;
    extract_attr_from_fragment(&rest[..tag_end], attr_name)
}

pub(crate) fn extract_attr_from_fragment<'a>(
    fragment: &'a str,
    attr_name: &str,
) -> Option<&'a str> {
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
        let valid_boundary = idx == 0
            || is_xml_ws(frag_bytes[idx - 1])
            || (!attr_name.contains(':')
                && idx >= 3
                && &frag_bytes[idx - 3..idx] == b"eo:"
                && (idx == 3 || is_xml_ws(frag_bytes[idx - 4])));
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
                    return Some(&fragment[val_start..val_end]);
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

#[cfg(test)]
pub(crate) fn extract_ods_cells_from_row(row_xml: &str) -> Vec<(bool, &str, &str)> {
    const CELL_TAG: &[u8] = b"table:table-cell";
    const COVERED_TAG: &[u8] = b"table:covered-table-cell";
    let bytes = row_xml.as_bytes();
    let mut out = Vec::new();
    let mut cursor = 0usize;
    let mut next_cell = find_valid_open_tag(bytes, 0, CELL_TAG);
    let mut next_covered = find_valid_open_tag(bytes, 0, COVERED_TAG);

    while cursor < bytes.len() {
        if next_cell.is_some_and(|(p, _)| p < cursor) {
            next_cell = find_valid_open_tag(bytes, cursor, CELL_TAG);
        }
        if next_covered.is_some_and(|(p, _)| p < cursor) {
            next_covered = find_valid_open_tag(bytes, cursor, COVERED_TAG);
        }
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

pub(crate) fn extract_balanced_elements<'a>(xml: &'a str, tag: &str) -> Vec<(&'a str, &'a str)> {
    extract_tags_or_self_closing_with_attrs(xml, tag)
}

pub(crate) fn extract_odf_blocks_in_order<'a>(
    xml: &'a str,
    tags: &[&'a str],
) -> Vec<(&'a str, &'a str, &'a str)> {
    let bytes = xml.as_bytes();
    let mut out = Vec::new();
    let mut cursor = 0usize;
    let mut cached_open: Vec<Option<(usize, usize)>> = tags
        .iter()
        .map(|t| find_valid_open_tag(bytes, 0, t.as_bytes()))
        .collect();

    while cursor < bytes.len() {
        let mut best: Option<(&'a str, usize, usize)> = None;
        for (idx, &tag) in tags.iter().enumerate() {
            if cached_open[idx].is_some_and(|(pos, _)| pos < cursor) {
                cached_open[idx] = find_valid_open_tag(bytes, cursor, tag.as_bytes());
            }
            if let Some((open_pos, gt_pos)) = cached_open[idx] {
                if best.is_none_or(|(_, best_pos, _)| open_pos < best_pos) {
                    best = Some((tag, open_pos, gt_pos));
                }
            }
        }
        let Some((tag, open_pos, gt_pos)) = best else {
            break;
        };
        let tag_bytes = tag.as_bytes();
        let after_tag = open_pos + 1 + tag_bytes.len();
        if is_self_closing_open_tag(bytes, after_tag, gt_pos) {
            let mut attr_end = gt_pos;
            while attr_end > after_tag && is_xml_ws(bytes[attr_end - 1]) {
                attr_end -= 1;
            }
            if attr_end > after_tag && bytes[attr_end - 1] == b'/' {
                attr_end -= 1;
            }
            out.push((tag, &xml[after_tag..attr_end], ""));
            cursor = gt_pos + 1;
            continue;
        }
        let attrs = &xml[after_tag..gt_pos];
        let body_start = gt_pos + 1;
        let Some((body_end, next_cursor)) = find_matching_close_tag(bytes, body_start, tag_bytes)
        else {
            break;
        };
        out.push((tag, attrs, &xml[body_start..body_end]));
        cursor = next_cursor;
    }
    out
}

pub(crate) fn extract_odf_inline_text(xml_fragment: &str) -> String {
    let bytes = xml_fragment.as_bytes();
    let mut out = String::with_capacity(xml_fragment.len());
    let mut cursor = 0usize;

    while cursor < bytes.len() {
        let Some(lt) = crate::XmlSanitizer::find_byte_swar(bytes, cursor, b'<') else {
            out.push_str(&crate::XmlSanitizer::unescape(&xml_fragment[cursor..]));
            break;
        };
        if lt > cursor {
            out.push_str(&crate::XmlSanitizer::unescape(&xml_fragment[cursor..lt]));
        }
        let Some(gt) = find_tag_close_gt(bytes, lt + 1) else {
            out.push_str(&crate::XmlSanitizer::unescape(&xml_fragment[lt..]));
            break;
        };
        let tag_inner = &bytes[lt + 1..gt];
        if !tag_inner.starts_with(b"/") {
            if tag_inner.starts_with(b"text:s")
                && (tag_inner.len() == 6 || is_tag_name_boundary(tag_inner[6]))
            {
                let attrs_frag = &xml_fragment[lt + 1 + 6..gt];
                let count = extract_attr_from_fragment(attrs_frag, "text:c")
                    .or_else(|| extract_attr_from_fragment(attrs_frag, "c"))
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .unwrap_or(1)
                    .min(4096);
                for _ in 0..count {
                    out.push(' ');
                }
            } else if tag_inner.starts_with(b"text:tab")
                && (tag_inner.len() == 8 || is_tag_name_boundary(tag_inner[8]))
            {
                out.push('\t');
            } else if tag_inner.starts_with(b"text:line-break")
                && (tag_inner.len() == 15 || is_tag_name_boundary(tag_inner[15]))
            {
                out.push('\n');
            }
        }
        cursor = gt + 1;
    }
    out
}

pub(crate) fn to_open_formula_syntax_impl(formula_without_eq: &str) -> String {
    let bytes = formula_without_eq.as_bytes();
    let mut out = String::with_capacity(formula_without_eq.len() + 16);
    let mut i = 0usize;
    let mut in_quotes = false;

    while i < bytes.len() {
        if bytes[i] == b'"' {
            in_quotes = !in_quotes;
            out.push('"');
            i += 1;
            continue;
        }
        if in_quotes {
            let ch = formula_without_eq[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        if bytes[i] == b'[' {
            let mut j = i + 1;
            let mut in_sq = false;
            while j < bytes.len() {
                if bytes[j] == b'\'' {
                    in_sq = !in_sq;
                } else if bytes[j] == b']' && !in_sq {
                    j += 1;
                    break;
                }
                j += formula_without_eq[j..].chars().next().unwrap().len_utf8();
            }
            out.push_str(&formula_without_eq[i..j]);
            i = j;
            continue;
        }
        if let Some((consumed, formatted)) = try_convert_a1_or_sheet_ref(&formula_without_eq[i..]) {
            let prev_is_ident = i > 0
                && (bytes[i - 1].is_ascii_alphanumeric()
                    || bytes[i - 1] == b'_'
                    || bytes[i - 1] == b'.');
            if !prev_is_ident {
                out.push_str(&formatted);
                i += consumed;
                continue;
            }
        }
        let ch = formula_without_eq[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn scan_optional_sheet_prefix(s: &str) -> (Option<&str>, usize) {
    let bytes = s.as_bytes();
    if bytes.first() == Some(&b'\'') {
        let mut idx = 1usize;
        while idx < bytes.len() {
            if bytes[idx] == b'\'' {
                if idx + 1 < bytes.len() && bytes[idx + 1] == b'\'' {
                    idx += 2;
                    continue;
                }
                if idx + 1 < bytes.len() && bytes[idx + 1] == b'!' {
                    return (Some(&s[..=idx]), idx + 2);
                }
                break;
            }
            idx += s[idx..].chars().next().unwrap().len_utf8();
        }
    } else if let Some(bang_pos) = s.find('!') {
        let candidate = &s[..bang_pos];
        if !candidate.is_empty() && candidate.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return (Some(candidate), bang_pos + 1);
        }
    }
    (None, 0)
}

fn try_convert_a1_or_sheet_ref(s: &str) -> Option<(usize, String)> {
    let bytes = s.as_bytes();
    let (sheet_prefix, pos) = scan_optional_sheet_prefix(s);
    let (c1_len, c1_str) = scan_single_a1_coord(&bytes[pos..])?;
    let mut end_pos = pos + c1_len;
    if end_pos < bytes.len() && bytes[end_pos] == b'(' {
        return None;
    }

    if end_pos < bytes.len() && bytes[end_pos] == b':' {
        let rhs_slice = &s[end_pos + 1..];
        let (rhs_sheet, rhs_pos) = scan_optional_sheet_prefix(rhs_slice);
        if let Some((c2_len, c2_str)) = scan_single_a1_coord(&rhs_slice.as_bytes()[rhs_pos..]) {
            let after_range = end_pos + 1 + rhs_pos + c2_len;
            if after_range >= bytes.len() || !bytes[after_range].is_ascii_alphanumeric() {
                end_pos = after_range;
                let left_part = match sheet_prefix {
                    Some(sh) => format!("{sh}.{c1_str}"),
                    None => format!(".{c1_str}"),
                };
                let right_part = match rhs_sheet {
                    Some(rsh) if Some(rsh) != sheet_prefix => format!("{rsh}.{c2_str}"),
                    _ => format!(".{c2_str}"),
                };
                return Some((end_pos, format!("[{left_part}:{right_part}]")));
            }
        }
    }

    let formatted = if let Some(sh) = sheet_prefix {
        format!("[{sh}.{c1_str}]")
    } else {
        format!("[.{c1_str}]")
    };
    Some((end_pos, formatted))
}

pub(crate) fn scan_single_a1_coord(bytes: &[u8]) -> Option<(usize, String)> {
    let mut idx = 0usize;
    let mut out = String::new();
    if idx < bytes.len() && bytes[idx] == b'$' {
        out.push('$');
        idx += 1;
    }
    let col_start = idx;
    while idx < bytes.len() && bytes[idx].is_ascii_alphabetic() {
        out.push(bytes[idx] as char);
        idx += 1;
    }
    let col_len = idx - col_start;
    if col_len == 0 || col_len > 3 {
        return None;
    }
    if idx < bytes.len() && bytes[idx] == b'$' {
        out.push('$');
        idx += 1;
    }
    let row_start = idx;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        out.push(bytes[idx] as char);
        idx += 1;
    }
    if idx == row_start || idx - row_start > 7 {
        return None;
    }
    if idx < bytes.len() && (bytes[idx].is_ascii_alphanumeric() || bytes[idx] == b'_') {
        return None;
    }
    Some((idx, out))
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
        assert_eq!(extract_attr_from_fragment(tags[0].0, "r"), Some("B3"));
    }

    #[test]
    fn test_f10_parse_a1_zero_based_checked_bounds() {
        assert_eq!(parse_a1_zero_based("A1"), Some((0, 0)));
        assert_eq!(parse_a1_zero_based("XFD1048576"), Some((1_048_575, 16_383)));
        assert_eq!(parse_a1_zero_based("XFE1"), None);
        assert_eq!(parse_a1_zero_based("A1048577"), None);
        assert_eq!(parse_a1_zero_based("AAAA1"), None);
        assert_eq!(parse_a1_zero_based("ZZZZZZZZZZ999999999999"), None);
    }

    #[test]
    fn test_encode_utf16le_swar_morton_and_backpatch() {
        let samples = [
            "",
            "A",
            "ABCD",
            "ABCDEFGH",
            "ABCDEFGHIJKLM",
            "EasyOffice ASCII 0123456789!@#$%^&*()",
            "한글과 ASCII Mixed 12345678 가나다라마바사 🚀🎉",
            "ABCD한글EFGH🎉IJKL",
        ];
        let mut buf = Vec::new();
        for s in samples {
            let swar = encode_utf16le_swar(s);
            let scalar = encode_utf16le_scalar_baseline(s);
            assert_eq!(swar, scalar, "mismatch for sample: {s}");
            append_len_prefixed_utf16le(&mut buf, s);
        }
        let mut cursor = 0usize;
        for s in samples {
            let decoded = read_len_prefixed_utf16le(&buf, &mut cursor, buf.len()).unwrap();
            assert_eq!(decoded, s);
        }
        assert_eq!(cursor, buf.len());
    }
}
