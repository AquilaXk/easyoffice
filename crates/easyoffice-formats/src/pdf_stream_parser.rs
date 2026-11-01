#[path = "pdf_boundary_lexer.rs"]
pub(crate) mod pdf_boundary_lexer;

pub(crate) use pdf_boundary_lexer::*;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub(crate) struct PdfToUnicodeMap {
    pub(crate) map: HashMap<u16, String>,
    pub(crate) is_two_byte: bool,
}

impl PdfToUnicodeMap {
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
                    out.map.insert(code, dst);
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
            let tokens = Self::extract_hex_tokens(block);
            for triple in tokens.chunks_exact(3) {
                let (Some((lo, len_lo)), Some((hi, _)), Some((dst_base, _))) = (
                    Self::parse_code_hex(triple[0]),
                    Self::parse_code_hex(triple[1]),
                    Self::parse_code_hex(triple[2]),
                ) else {
                    continue;
                };
                if lo == 0x0000 && hi == 0xFFFF && dst_base == 0x0000 {
                    continue;
                }
                if hi >= lo && (hi - lo) <= 4096 {
                    if len_lo == 2 {
                        out.is_two_byte = true;
                    }
                    for offset in 0..=(hi - lo) {
                        if let Some(ch) = char::from_u32(u32::from(dst_base) + u32::from(offset)) {
                            out.map.insert(lo + offset, ch.to_string());
                        }
                    }
                }
            }
            rest_range = &after[end_pos + "endbfrange".len()..];
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

#[derive(Debug, Clone, Copy)]
struct PdfGraphicsState {
    font_utf16be: bool,
    font_size: f64,
    render_mode: i32,
    leading: f64,
}

impl Default for PdfGraphicsState {
    fn default() -> Self {
        Self {
            font_utf16be: false,
            font_size: 12.0,
            render_mode: 0,
            leading: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
enum PdfStreamOperand {
    Num(f64),
    Name(String),
    Str(String),
    ArrayText(String),
}

pub(crate) struct ParsedPdfContentStream {
    pub(crate) body_text: String,
    pub(crate) watermark: Option<String>,
    pub(crate) stamp: Option<String>,
}

#[derive(Default)]
struct PdfStreamCollector {
    canonical_chunks: Vec<String>,
    stream_segments: Vec<String>,
    last_bt_pos: Option<(usize, f64, f64)>,
    watermark: Option<String>,
    stamp: Option<String>,
}

impl PdfStreamCollector {
    fn record(&mut self, text: String, gs: PdfGraphicsState, pos: (f64, f64), bt_id: usize) {
        let (tlm_x, tlm_y) = pos;
        let is_canonical_wm = gs.render_mode == 3
            && (gs.font_size - 1.0).abs() < 1e-6
            && tlm_x.abs() < 1e-6
            && (tlm_y - 1.0).abs() < 1e-6;
        let is_visible_wm = gs.render_mode != 3
            && (gs.font_size - 10.0).abs() < 1e-6
            && (tlm_x - 320.0).abs() < 1e-6
            && (tlm_y - 746.0).abs() < 1e-6;
        if is_canonical_wm || is_visible_wm {
            if let Some(wm) = text.strip_prefix("[WM] ") {
                self.watermark = Some(wm.to_string());
                return;
            }
        }
        let is_stamp_slot = gs.render_mode != 3
            && (gs.font_size - 10.0).abs() < 1e-6
            && (tlm_x - 386.0).abs() < 1e-6
            && (tlm_y - 604.0).abs() < 1e-6;
        if is_stamp_slot {
            if let Some(st) = text.strip_prefix("[STAMP] ") {
                self.stamp = Some(st.to_string());
                return;
            }
        }
        let is_canonical_form = gs.render_mode == 3
            && (gs.font_size - 1.0).abs() < 1e-6
            && tlm_x.abs() < 1e-6
            && (tlm_y - 2.0).abs() < 1e-6
            && text.starts_with("[FORM] ");
        if is_canonical_form {
            return;
        }
        let is_canonical_body = gs.render_mode == 3
            && (gs.font_size - 1.0).abs() < 1e-6
            && tlm_x.abs() < 1e-6
            && tlm_y.abs() < 1e-6;
        if is_canonical_body {
            self.canonical_chunks.push(text);
        } else {
            let same_line_in_bt = self.last_bt_pos.is_some_and(|(prev_bt, prev_x, prev_y)| {
                prev_bt == bt_id && (tlm_y - prev_y).abs() < 1e-3 && (tlm_x - prev_x).abs() > 1e-3
            });
            if same_line_in_bt {
                if let Some(last_seg) = self.stream_segments.last_mut() {
                    last_seg.push_str(&text);
                } else {
                    self.stream_segments.push(text);
                }
            } else {
                self.stream_segments.push(text);
            }
            self.last_bt_pos = Some((bt_id, tlm_x, tlm_y));
        }
    }

    fn finish(self) -> ParsedPdfContentStream {
        let body_text = if !self.canonical_chunks.is_empty() {
            self.canonical_chunks.join("\n")
        } else {
            self.stream_segments.join("\n")
        };
        ParsedPdfContentStream {
            body_text,
            watermark: self.watermark,
            stamp: self.stamp,
        }
    }
}

#[allow(dead_code)]
pub(crate) fn parse_pdf_content_stream(stream: &str) -> ParsedPdfContentStream {
    parse_pdf_content_stream_with_cmap(stream, None)
}

pub(crate) fn parse_pdf_content_stream_with_cmap(
    stream: &str,
    cmap: Option<&PdfToUnicodeMap>,
) -> ParsedPdfContentStream {
    let bytes = stream.as_bytes();
    let mut i = 0usize;
    let mut in_text_object = false;
    let mut bt_counter = 0usize;
    let mut gs = PdfGraphicsState::default();
    let mut gs_stack: Vec<PdfGraphicsState> = Vec::new();
    let mut tlm_x = 0.0f64;
    let mut tlm_y = 0.0f64;
    let mut dict_depth = 0usize;
    let mut in_array = false;
    let mut array_buf = String::new();
    let mut array_has_text = false;
    let mut operands: Vec<PdfStreamOperand> = Vec::new();
    let mut collector = PdfStreamCollector::default();

    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        match step_pdf_non_code_token(bytes, i) {
            Ok(Some(step)) => {
                match step.kind {
                    PdfNonCodeKind::Comment { .. } => {}
                    PdfNonCodeKind::DictOpen => {
                        dict_depth += 1;
                    }
                    PdfNonCodeKind::DictClose => {
                        dict_depth = dict_depth.saturating_sub(1);
                    }
                    PdfNonCodeKind::ParenString {
                        inner_start,
                        inner_end,
                    } => {
                        if dict_depth == 0 && in_text_object {
                            let decoded = unescape_pdf_text(&stream[inner_start..inner_end]);
                            if in_array {
                                array_buf.push_str(&decoded);
                                array_has_text = true;
                            } else {
                                operands.push(PdfStreamOperand::Str(decoded));
                            }
                        }
                    }
                    PdfNonCodeKind::HexString {
                        inner_start,
                        inner_end,
                    } => {
                        if dict_depth == 0 && in_text_object {
                            if let Some(decoded) = decode_pdf_hex_string_with_cmap(
                                &stream[inner_start..inner_end],
                                gs.font_utf16be,
                                cmap,
                            ) {
                                if in_array {
                                    array_buf.push_str(&decoded);
                                    array_has_text = true;
                                } else {
                                    operands.push(PdfStreamOperand::Str(decoded));
                                }
                            }
                        }
                    }
                }
                i = step.next_index;
                continue;
            }
            Err(()) => break,
            Ok(None) => {}
        }
        if b == b'[' {
            if dict_depth == 0 && in_text_object {
                in_array = true;
                array_buf.clear();
                array_has_text = false;
            }
            i += 1;
            continue;
        }
        if b == b']' {
            if dict_depth == 0 && in_text_object && in_array {
                in_array = false;
                if array_has_text {
                    operands.push(PdfStreamOperand::ArrayText(std::mem::take(&mut array_buf)));
                } else {
                    array_buf.clear();
                }
                array_has_text = false;
            }
            i += 1;
            continue;
        }
        if b == b'>' || b == b'{' || b == b'}' {
            i += 1;
            continue;
        }
        if b == b'/' {
            let start = i;
            i += 1;
            while i < bytes.len() && !is_pdf_delim_or_ws(bytes[i]) {
                i += 1;
            }
            if dict_depth == 0 && !in_array {
                operands.push(PdfStreamOperand::Name(stream[start..i].to_string()));
            }
            continue;
        }
        let start = i;
        while i < bytes.len() && !is_pdf_delim_or_ws(bytes[i]) {
            i += 1;
        }
        let tok = &stream[start..i];
        if dict_depth > 0 {
            continue;
        }
        if in_array {
            if array_has_text && !array_buf.ends_with(' ') {
                if let Ok(kern) = tok.parse::<f64>() {
                    if kern <= -180.0 {
                        array_buf.push(' ');
                    }
                }
            }
            continue;
        }
        if let Ok(num) = tok.parse::<f64>() {
            operands.push(PdfStreamOperand::Num(num));
            continue;
        }
        match tok {
            "q" => {
                if gs_stack.len() < 64 {
                    gs_stack.push(gs);
                }
            }
            "Q" => {
                if let Some(prev) = gs_stack.pop() {
                    gs = prev;
                }
            }
            "BT" => {
                in_text_object = true;
                bt_counter += 1;
                tlm_x = 0.0;
                tlm_y = 0.0;
            }
            "ET" => {
                in_text_object = false;
            }
            "Tf" => {
                let mut size_opt = None;
                let mut name_opt = None;
                for op in operands.iter().rev() {
                    match op {
                        PdfStreamOperand::Num(sz) if size_opt.is_none() => size_opt = Some(*sz),
                        PdfStreamOperand::Name(nm) if name_opt.is_none() => {
                            name_opt = Some(nm.as_str())
                        }
                        _ => {}
                    }
                    if size_opt.is_some() && name_opt.is_some() {
                        break;
                    }
                }
                if let Some(nm) = name_opt {
                    gs.font_utf16be = nm == "/F2";
                }
                if let Some(sz) = size_opt {
                    gs.font_size = sz;
                }
            }
            "Tr" => {
                if let Some(m) = last_numeric_operand(&operands) {
                    gs.render_mode = m as i32;
                }
            }
            "TL" => {
                if let Some(tl) = last_numeric_operand(&operands) {
                    gs.leading = tl;
                }
            }
            "Td" => {
                let nums = extract_numeric_operands(&operands);
                if nums.len() >= 2 {
                    tlm_x += nums[nums.len() - 2];
                    tlm_y += nums[nums.len() - 1];
                }
            }
            "TD" => {
                let nums = extract_numeric_operands(&operands);
                if nums.len() >= 2 {
                    let tx = nums[nums.len() - 2];
                    let ty = nums[nums.len() - 1];
                    gs.leading = -ty;
                    tlm_x += tx;
                    tlm_y += ty;
                }
            }
            "T*" => {
                tlm_y -= if gs.leading.abs() > 1e-6 {
                    gs.leading
                } else {
                    gs.font_size
                };
            }
            "Tm" => {
                let nums = extract_numeric_operands(&operands);
                if nums.len() >= 2 {
                    tlm_x = nums[nums.len() - 2];
                    tlm_y = nums[nums.len() - 1];
                }
            }
            "'" | "\"" if in_text_object => {
                tlm_y -= if gs.leading.abs() > 1e-6 {
                    gs.leading
                } else {
                    gs.font_size
                };
                if let Some(text) = last_string_operand(&operands) {
                    collector.record(text, gs, (tlm_x, tlm_y), bt_counter);
                }
            }
            "Tj" if in_text_object => {
                if let Some(text) = last_string_operand(&operands) {
                    collector.record(text, gs, (tlm_x, tlm_y), bt_counter);
                }
            }
            "TJ" if in_text_object => {
                if let Some(text) = operands.iter().rev().find_map(|op| match op {
                    PdfStreamOperand::ArrayText(s) => Some(s.clone()),
                    _ => None,
                }) {
                    collector.record(text, gs, (tlm_x, tlm_y), bt_counter);
                }
            }
            _ => {}
        }
        operands.clear();
    }
    collector.finish()
}

fn last_numeric_operand(operands: &[PdfStreamOperand]) -> Option<f64> {
    operands.iter().rev().find_map(|op| match op {
        PdfStreamOperand::Num(n) => Some(*n),
        _ => None,
    })
}

fn extract_numeric_operands(operands: &[PdfStreamOperand]) -> Vec<f64> {
    operands
        .iter()
        .filter_map(|op| match op {
            PdfStreamOperand::Num(n) => Some(*n),
            _ => None,
        })
        .collect()
}

fn last_string_operand(operands: &[PdfStreamOperand]) -> Option<String> {
    operands.iter().rev().find_map(|op| match op {
        PdfStreamOperand::Str(s) => Some(s.clone()),
        _ => None,
    })
}

pub(crate) fn decode_pdf_hex_string(hex: &str, prefer_utf16be: bool) -> Option<String> {
    decode_pdf_hex_string_with_cmap(hex, prefer_utf16be, None)
}

pub(crate) fn decode_pdf_hex_string_with_cmap(
    hex: &str,
    prefer_utf16be: bool,
    cmap: Option<&PdfToUnicodeMap>,
) -> Option<String> {
    let mut clean: Vec<u8> = hex.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if clean.is_empty() {
        return Some(String::new());
    }
    if !clean.iter().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    if clean.len() % 2 != 0 {
        clean.push(b'0');
    }
    let mut bytes = Vec::with_capacity(clean.len() / 2);
    let mut idx = 0usize;
    while idx + 2 <= clean.len() {
        let chunk = std::str::from_utf8(&clean[idx..idx + 2]).ok()?;
        bytes.push(u8::from_str_radix(chunk, 16).ok()?);
        idx += 2;
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        if rest.len() % 2 != 0 {
            return None;
        }
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        return Some(String::from_utf16_lossy(&units));
    }
    if let Some(cm) = cmap {
        if !cm.map.is_empty() {
            if cm.is_two_byte && bytes.len() % 2 == 0 {
                let mut mapped = String::new();
                let mut any_hit = false;
                for pair in bytes.chunks_exact(2) {
                    let code = u16::from_be_bytes([pair[0], pair[1]]);
                    if let Some(s) = cm.map.get(&code) {
                        mapped.push_str(s);
                        any_hit = true;
                    } else if let Some(ch) = char::from_u32(u32::from(code)) {
                        mapped.push(ch);
                    }
                }
                if any_hit {
                    return Some(mapped);
                }
            } else if !cm.is_two_byte {
                let mut mapped = String::new();
                let mut any_hit = false;
                for &b in &bytes {
                    if let Some(s) = cm.map.get(&u16::from(b)) {
                        mapped.push_str(s);
                        any_hit = true;
                    } else {
                        mapped.push(char::from(b));
                    }
                }
                if any_hit {
                    return Some(mapped);
                }
            }
        }
    }
    if prefer_utf16be && bytes.len() % 2 == 0 {
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        return Some(String::from_utf16_lossy(&units));
    }
    match String::from_utf8(bytes) {
        Ok(s) => Some(s),
        Err(e) => Some(e.into_bytes().into_iter().map(char::from).collect()),
    }
}

pub(crate) fn escape_pdf_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(ch),
        }
    }
    out
}

pub(crate) fn unescape_pdf_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('b') => out.push('\u{0008}'),
                Some('f') => out.push('\u{000C}'),
                Some('\r') => {
                    if chars.peek() == Some(&'\n') {
                        let _ = chars.next();
                    }
                }
                Some('\n') => {}
                Some(oct @ '0'..='7') => {
                    let mut val = (oct as u32) - ('0' as u32);
                    for _ in 0..2 {
                        if let Some(&next_oct @ '0'..='7') = chars.peek() {
                            let _ = chars.next();
                            val = (val * 8) + ((next_oct as u32) - ('0' as u32));
                        } else {
                            break;
                        }
                    }
                    out.push(char::from((val & 0xFF) as u8));
                }
                Some(escaped) => out.push(escaped),
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    if out.starts_with("\u{00FE}\u{00FF}") && out.chars().all(|c| (c as u32) <= 0xFF) {
        let raw: Vec<u8> = out.chars().skip(2).map(|c| c as u8).collect();
        if raw.len() % 2 == 0 {
            let units: Vec<u16> = raw
                .chunks_exact(2)
                .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                .collect();
            return String::from_utf16_lossy(&units);
        }
    }
    out
}

pub(crate) fn decode_base64_bytes_impl(input: &str) -> Option<Vec<u8>> {
    let clean: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if clean.is_empty() || (clean.len() & 3) != 0 {
        return None;
    }
    let val = |b: u8| -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let mut out = Vec::with_capacity((clean.len() / 4) * 3);
    let mut idx = 0usize;
    while idx + 4 <= clean.len() {
        if (clean[idx + 2] == b'=' || clean[idx + 3] == b'=') && idx + 4 != clean.len() {
            return None;
        }
        let a = val(clean[idx])?;
        let b = val(clean[idx + 1])?;
        out.push((a << 2) | (b >> 4));
        if clean[idx + 2] != b'=' {
            let c = val(clean[idx + 2])?;
            out.push(((b & 0x0F) << 4) | (c >> 2));
            if clean[idx + 3] != b'=' {
                let d = val(clean[idx + 3])?;
                out.push(((c & 0x03) << 6) | d);
            }
        } else if clean[idx + 3] != b'=' {
            return None;
        }
        idx += 4;
    }
    Some(out)
}

pub(crate) fn encode_utf16be_hex_impl(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(text.len() * 4);
    for unit in text.encode_utf16() {
        out.push(HEX[((unit >> 12) & 0x0F) as usize] as char);
        out.push(HEX[((unit >> 8) & 0x0F) as usize] as char);
        out.push(HEX[((unit >> 4) & 0x0F) as usize] as char);
        out.push(HEX[(unit & 0x0F) as usize] as char);
    }
    out
}
