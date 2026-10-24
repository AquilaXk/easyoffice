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
