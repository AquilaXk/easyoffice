//! `/ToUnicode` CMap parsing and the per-font decoding rules the content stream interpreter reads.

use super::{decode_hex_bytes_single_pass, decode_pdf_hex_string};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub(crate) struct PdfToUnicodeMap {
    pub(crate) map: HashMap<u16, String>,
    pub(crate) bmp_single: HashMap<u16, char>,
    pub(crate) single_byte_mask: [u64; 4],
    pub(crate) single_byte_chars: [char; 256],
    pub(crate) is_two_byte: bool,
}

impl Default for PdfToUnicodeMap {
    fn default() -> Self {
        Self {
            map: HashMap::new(),
            bmp_single: HashMap::new(),
            single_byte_mask: [0u64; 4],
            single_byte_chars: ['\0'; 256],
            is_two_byte: false,
        }
    }
}

/// Next `<...>` token at or after `*idx` (advancing past it); stray `<` without `>` are skipped.
fn next_hex_token<'a>(block: &'a str, idx: &mut usize) -> Option<&'a str> {
    let bytes = block.as_bytes();
    while *idx < bytes.len() {
        if bytes[*idx] == b'<' {
            if let Some(close) = block[*idx + 1..].find('>') {
                let s = &block[*idx + 1..*idx + 1 + close];
                *idx += close + 2;
                return Some(s);
            }
        }
        *idx += 1;
    }
    None
}

impl PdfToUnicodeMap {
    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.single_byte_mask == [0u64; 4] && self.bmp_single.is_empty() && self.map.is_empty()
    }

    #[inline]
    pub(crate) fn insert_char(&mut self, code: u16, ch: char) {
        if code <= 0xFF {
            self.single_byte_mask[usize::from(code >> 6)] |= 1u64 << (code & 63);
            self.single_byte_chars[usize::from(code)] = ch;
        } else {
            self.bmp_single.insert(code, ch);
        }
    }

    #[inline]
    pub(crate) fn insert_mapping(&mut self, code: u16, dst: String) {
        let mut chars = dst.chars();
        if let (Some(ch), None) = (chars.next(), chars.next()) {
            self.insert_char(code, ch);
        } else {
            self.map.insert(code, dst);
        }
    }

    pub(crate) fn parse_cmap_stream(cmap_text: &str, out: &mut Self) {
        let mut rest = cmap_text;
        while let Some(pos) = rest.find("beginbfchar") {
            let after = &rest[pos + "beginbfchar".len()..];
            let Some(end_pos) = after.find("endbfchar") else {
                break;
            };
            let block = &after[..end_pos];
            let tokens = Self::extract_hex_tokens(block);
            for pair in tokens.chunks_exact(2) {
                if let (Some((code, byte_len)), Some(dst)) = (
                    Self::parse_code_hex(pair[0]),
                    decode_pdf_hex_string(pair[1], true),
                ) {
                    if byte_len == 2 {
                        out.is_two_byte = true;
                    }
                    out.insert_mapping(code, dst);
                }
            }
            rest = &after[end_pos + "endbfchar".len()..];
        }

        let mut rest_range = cmap_text;
        while let Some(pos) = rest_range.find("beginbfrange") {
            let after = &rest_range[pos + "beginbfrange".len()..];
            let Some(end_pos) = after.find("endbfrange") else {
                break;
            };
            let block = &after[..end_pos];
            Self::parse_bfrange_block(block, out);
            rest_range = &after[end_pos + "endbfrange".len()..];
        }
    }

    fn parse_bfrange_block(block: &str, out: &mut PdfToUnicodeMap) {
        let bytes = block.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            let Some(lo_hex) = next_hex_token(block, &mut i) else {
                break;
            };
            let Some(hi_hex) = next_hex_token(block, &mut i) else {
                break;
            };
            let (Some((lo, len_lo)), Some((hi, _))) =
                (Self::parse_code_hex(lo_hex), Self::parse_code_hex(hi_hex))
            else {
                continue;
            };
            if hi < lo {
                continue;
            }
            if len_lo == 2 {
                out.is_two_byte = true;
            }
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'[' {
                i += 1;
                Self::parse_bfrange_array(block, &mut i, (lo, hi), out);
            } else if let Some(dst_hex) = next_hex_token(block, &mut i) {
                Self::apply_bfrange_base(dst_hex, (lo, hi), out);
            }
        }
    }

    /// Reads `<dst> <dst> ... ]` after the opening `[`, mapping consecutive codes from `lo`.
    fn parse_bfrange_array(
        block: &str,
        i: &mut usize,
        (lo, hi): (u16, u16),
        out: &mut PdfToUnicodeMap,
    ) {
        let bytes = block.as_bytes();
        let mut code = lo;
        while *i < bytes.len() {
            while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
                *i += 1;
            }
            if *i < bytes.len() && bytes[*i] == b']' {
                *i += 1;
                break;
            }
            if bytes[*i] == b'<' {
                if let Some(close) = block[*i + 1..].find('>') {
                    let dst_hex = &block[*i + 1..*i + 1 + close];
                    *i += close + 2;
                    if code <= hi {
                        if let Some(dst) = decode_pdf_hex_string(dst_hex, true) {
                            out.insert_mapping(code, dst);
                        }
                        code = code.saturating_add(1);
                    }
                    continue;
                }
            }
            *i += 1;
        }
    }

    /// Maps `lo..=hi` onto consecutive code points or multi-code-unit UTF-16 sequences starting at `<dst>`.
    fn apply_bfrange_base(dst_hex: &str, (lo, hi): (u16, u16), out: &mut PdfToUnicodeMap) {
        if let Some((dst_base, _)) = Self::parse_code_hex(dst_hex) {
            if lo == 0x0000 && hi == 0xFFFF && dst_base == 0x0000 {
                return;
            }
            for offset in 0..=(hi - lo) {
                if let Some(ch) = char::from_u32(u32::from(dst_base) + u32::from(offset)) {
                    out.insert_char(lo + offset, ch);
                }
            }
            return;
        }
        let Some(bytes) = decode_hex_bytes_single_pass(dst_hex) else {
            return;
        };
        if bytes.len() < 4 || bytes.len() % 2 != 0 {
            return;
        }
        let base_units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|p| u16::from_be_bytes([p[0], p[1]]))
            .collect();
        let base_last = *base_units.last().unwrap_or(&0);
        let mut cur_units = base_units;
        for offset in 0..=(hi - lo) {
            let Some(inc) = base_last.checked_add(offset) else {
                break;
            };
            if let Some(last) = cur_units.last_mut() {
                *last = inc;
            }
            if let Ok(s) =
                char::decode_utf16(cur_units.iter().copied()).collect::<Result<String, _>>()
            {
                if !s.is_empty() {
                    out.map.insert(lo + offset, s);
                }
            }
        }
    }

    fn extract_hex_tokens(block: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let bytes = block.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] == b'<' {
                if let Some(close) = block[i + 1..].find('>') {
                    out.push(&block[i + 1..i + 1 + close]);
                    i += close + 2;
                    continue;
                }
            }
            i += 1;
        }
        out
    }

    fn parse_code_hex(hex: &str) -> Option<(u16, usize)> {
        let trimmed = hex.trim();
        if trimmed.is_empty() || trimmed.len() > 4 {
            return None;
        }
        let val = u16::from_str_radix(trimmed, 16).ok()?;
        Some((val, trimmed.len().div_ceil(2)))
    }
}

/// Text decoding rules of one font resource of a page.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PdfFontDecoder<'a> {
    pub(crate) cmap: Option<&'a PdfToUnicodeMap>,
    pub(crate) utf16be: bool,
}

/// Font resources of a page, keyed by resource name including the slash (`/F1`).
pub(crate) type PdfPageFonts<'a> = HashMap<String, PdfFontDecoder<'a>>;
