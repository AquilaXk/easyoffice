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
            Self::parse_bfrange_block(block, out);
            rest_range = &after[end_pos + "endbfrange".len()..];
        }
    }

    fn parse_bfrange_block(block: &str, out: &mut PdfToUnicodeMap) {
        let bytes = block.as_bytes();
        let mut i = 0usize;
        let next_hex = |idx: &mut usize| -> Option<&str> {
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
        };
        while i < bytes.len() {
            let Some(lo_hex) = next_hex(&mut i) else {
                break;
            };
            let Some(hi_hex) = next_hex(&mut i) else {
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
                let mut code = lo;
                while i < bytes.len() {
                    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    if i < bytes.len() && bytes[i] == b']' {
                        i += 1;
                        break;
                    }
                    if bytes[i] == b'<' {
                        if let Some(close) = block[i + 1..].find('>') {
                            let dst_hex = &block[i + 1..i + 1 + close];
                            i += close + 2;
                            if code <= hi {
                                if let Some(dst) = decode_pdf_hex_string(dst_hex, true) {
                                    out.map.insert(code, dst);
                                }
                                code = code.saturating_add(1);
                            }
                            continue;
                        }
                    }
                    i += 1;
                }
            } else if let Some(dst_hex) = next_hex(&mut i) {
                if let Some((dst_base, _)) = Self::parse_code_hex(dst_hex) {
                    if lo == 0x0000 && hi == 0xFFFF && dst_base == 0x0000 {
                        continue;
                    }
                    for offset in 0..=(hi - lo) {
                        if let Some(ch) = char::from_u32(u32::from(dst_base) + u32::from(offset)) {
                            out.map.insert(lo + offset, ch.to_string());
                        }
                    }
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

const POS_EPSILON: f64 = 1e-6;
const SAME_LINE_EPSILON: f64 = 1e-3;
const INVISIBLE_RENDER_MODE: i32 = 3;
/// Text-state slot `(invisible, font size, x, y)` of the canonical hidden watermark.
const CANONICAL_WM_SLOT: (bool, f64, f64, f64) = (true, 1.0, 0.0, 1.0);
/// Slot of the visible watermark.
const VISIBLE_WM_SLOT: (bool, f64, f64, f64) = (false, 10.0, 320.0, 746.0);
/// Slot of the visible stamp.
const STAMP_SLOT: (bool, f64, f64, f64) = (false, 10.0, 386.0, 604.0);
/// Slot of the canonical hidden form marker.
const CANONICAL_FORM_SLOT: (bool, f64, f64, f64) = (true, 1.0, 0.0, 2.0);
/// Slot of the canonical hidden body text.
const CANONICAL_BODY_SLOT: (bool, f64, f64, f64) = (true, 1.0, 0.0, 0.0);

/// Whether the graphics state and text position match a fixed marker slot.
fn matches_slot(gs: &PdfGraphicsState, pos: (f64, f64), slot: (bool, f64, f64, f64)) -> bool {
    let (invisible, size, x, y) = slot;
    (gs.render_mode == INVISIBLE_RENDER_MODE) == invisible
        && (gs.font_size - size).abs() < POS_EPSILON
        && (pos.0 - x).abs() < POS_EPSILON
        && (pos.1 - y).abs() < POS_EPSILON
}

impl PdfStreamCollector {
    fn record(&mut self, text: String, gs: PdfGraphicsState, pos: (f64, f64), bt_id: usize) {
        if self.record_marker(&text, &gs, pos) {
            return;
        }
        if matches_slot(&gs, pos, CANONICAL_BODY_SLOT) {
            self.canonical_chunks.push(text);
        } else {
            self.push_segment(text, pos, bt_id);
        }
    }

    /// Consumes watermark / stamp / form marker text; returns true when handled.
    fn record_marker(&mut self, text: &str, gs: &PdfGraphicsState, pos: (f64, f64)) -> bool {
        if matches_slot(gs, pos, CANONICAL_WM_SLOT) || matches_slot(gs, pos, VISIBLE_WM_SLOT) {
            if let Some(wm) = text.strip_prefix("[WM] ") {
                self.watermark = Some(wm.to_string());
                return true;
            }
        }
        if matches_slot(gs, pos, STAMP_SLOT) {
            if let Some(st) = text.strip_prefix("[STAMP] ") {
                self.stamp = Some(st.to_string());
                return true;
            }
        }
        matches_slot(gs, pos, CANONICAL_FORM_SLOT) && text.starts_with("[FORM] ")
    }

    fn push_segment(&mut self, text: String, pos: (f64, f64), bt_id: usize) {
        let (tlm_x, tlm_y) = pos;
        let same_line_in_bt = self.last_bt_pos.is_some_and(|(prev_bt, prev_x, prev_y)| {
            prev_bt == bt_id
                && (tlm_y - prev_y).abs() < SAME_LINE_EPSILON
                && (tlm_x - prev_x).abs() > SAME_LINE_EPSILON
        });
        match self.stream_segments.last_mut() {
            Some(last_seg) if same_line_in_bt => last_seg.push_str(&text),
            _ => self.stream_segments.push(text),
        }
        self.last_bt_pos = Some((bt_id, tlm_x, tlm_y));
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

const MAX_GRAPHICS_STATE_DEPTH: usize = 64;
const LEADING_EPSILON: f64 = 1e-6;
const KERN_SPACE_THRESHOLD: f64 = -180.0;

/// Mutable interpreter state for one content stream pass.
#[derive(Default)]
struct PdfStreamInterpreter {
    in_text_object: bool,
    bt_counter: usize,
    gs: PdfGraphicsState,
    gs_stack: Vec<PdfGraphicsState>,
    tlm_x: f64,
    tlm_y: f64,
    dict_depth: usize,
    in_array: bool,
    array_buf: String,
    array_has_text: bool,
    operands: Vec<PdfStreamOperand>,
    collector: PdfStreamCollector,
}

/// End index of the regular-character run starting at `from`.
#[inline]
fn pdf_token_end(bytes: &[u8], from: usize) -> usize {
    let mut i = from;
    while i < bytes.len() && !is_pdf_delim_or_ws(bytes[i]) {
        i += 1;
    }
    i
}

/// The last two numeric operands as `(x, y)`, without allocating.
fn last_two_numeric_operands(operands: &[PdfStreamOperand]) -> Option<(f64, f64)> {
    let mut nums = operands.iter().rev().filter_map(|op| match op {
        PdfStreamOperand::Num(n) => Some(*n),
        _ => None,
    });
    let y = nums.next()?;
    let x = nums.next()?;
    Some((x, y))
}

/// Latest font size and font name operands (scanning from the end).
fn font_operands(operands: &[PdfStreamOperand]) -> (Option<f64>, Option<&str>) {
    let mut size_opt = None;
    let mut name_opt = None;
    for op in operands.iter().rev() {
        match op {
            PdfStreamOperand::Num(sz) if size_opt.is_none() => size_opt = Some(*sz),
            PdfStreamOperand::Name(nm) if name_opt.is_none() => name_opt = Some(nm.as_str()),
            _ => {}
        }
        if size_opt.is_some() && name_opt.is_some() {
            break;
        }
    }
    (size_opt, name_opt)
}

impl PdfStreamInterpreter {
    #[inline]
    fn in_active_text(&self) -> bool {
        self.dict_depth == 0 && self.in_text_object
    }

    fn push_text(&mut self, decoded: String) {
        if self.in_array {
            self.array_buf.push_str(&decoded);
            self.array_has_text = true;
        } else {
            self.operands.push(PdfStreamOperand::Str(decoded));
        }
    }

    fn apply_non_code(
        &mut self,
        kind: PdfNonCodeKind,
        stream: &str,
        cmap: Option<&PdfToUnicodeMap>,
    ) {
        match kind {
            PdfNonCodeKind::Comment { .. } => {}
            PdfNonCodeKind::DictOpen => self.dict_depth += 1,
            PdfNonCodeKind::DictClose => self.dict_depth = self.dict_depth.saturating_sub(1),
            PdfNonCodeKind::ParenString {
                inner_start,
                inner_end,
            } => {
                if self.in_active_text() {
                    self.push_text(unescape_pdf_text(&stream[inner_start..inner_end]));
                }
            }
            PdfNonCodeKind::HexString {
                inner_start,
                inner_end,
            } => {
                if !self.in_active_text() {
                    return;
                }
                let hex = &stream[inner_start..inner_end];
                if let Some(decoded) =
                    decode_pdf_hex_string_with_cmap(hex, self.gs.font_utf16be, cmap)
                {
                    self.push_text(decoded);
                }
            }
        }
    }

    fn open_array(&mut self) {
        if self.in_active_text() {
            self.in_array = true;
            self.array_buf.clear();
            self.array_has_text = false;
        }
    }

    fn close_array(&mut self) {
        if !(self.in_active_text() && self.in_array) {
            return;
        }
        self.in_array = false;
        if self.array_has_text {
            let text = std::mem::take(&mut self.array_buf);
            self.operands.push(PdfStreamOperand::ArrayText(text));
        } else {
            self.array_buf.clear();
        }
        self.array_has_text = false;
    }

    /// Inserts a word gap for large negative kerning inside a text array.
    fn note_kerning(&mut self, tok: &str) {
        if !self.array_has_text || self.array_buf.ends_with(' ') {
            return;
        }
        if tok
            .parse::<f64>()
            .is_ok_and(|kern| kern <= KERN_SPACE_THRESHOLD)
        {
            self.array_buf.push(' ');
        }
    }

    /// Handles a regular-character token (number or operator keyword).
    fn apply_token(&mut self, tok: &str) {
        if self.dict_depth > 0 {
            return;
        }
        if self.in_array {
            self.note_kerning(tok);
            return;
        }
        if let Ok(num) = tok.parse::<f64>() {
            self.operands.push(PdfStreamOperand::Num(num));
            return;
        }
        self.run_operator(tok);
        self.operands.clear();
    }

    /// Dispatches one code byte at `i`; returns the next scan index.
    fn apply_code_token(&mut self, stream: &str, i: usize) -> usize {
        let bytes = stream.as_bytes();
        match bytes[i] {
            b'[' => {
                self.open_array();
                i + 1
            }
            b']' => {
                self.close_array();
                i + 1
            }
            b'>' | b'{' | b'}' => i + 1,
            b'/' => {
                let end = pdf_token_end(bytes, i + 1);
                if self.dict_depth == 0 && !self.in_array {
                    self.operands
                        .push(PdfStreamOperand::Name(stream[i..end].to_string()));
                }
                end
            }
            _ => {
                let end = pdf_token_end(bytes, i);
                self.apply_token(&stream[i..end]);
                end
            }
        }
    }

    fn run_operator(&mut self, op: &str) {
        match op {
            "q" => self.save_state(),
            "Q" => self.restore_state(),
            "BT" => self.begin_text(),
            "ET" => self.in_text_object = false,
            "Tf" => self.set_font(),
            "Tr" => {
                if let Some(m) = last_numeric_operand(&self.operands) {
                    self.gs.render_mode = m as i32;
                }
            }
            "TL" => {
                if let Some(tl) = last_numeric_operand(&self.operands) {
                    self.gs.leading = tl;
                }
            }
            "Td" | "TD" | "Tm" => self.move_text(op),
            "T*" => self.next_line(),
            "'" | "\"" if self.in_text_object => {
                self.next_line();
                self.record_string_operand();
            }
            "Tj" if self.in_text_object => self.record_string_operand(),
            "TJ" if self.in_text_object => self.record_array_operand(),
            _ => {}
        }
    }

    fn save_state(&mut self) {
        if self.gs_stack.len() < MAX_GRAPHICS_STATE_DEPTH {
            self.gs_stack.push(self.gs);
        }
    }

    fn restore_state(&mut self) {
        if let Some(prev) = self.gs_stack.pop() {
            self.gs = prev;
        }
    }

    fn begin_text(&mut self) {
        self.in_text_object = true;
        self.bt_counter += 1;
        self.tlm_x = 0.0;
        self.tlm_y = 0.0;
    }

    fn set_font(&mut self) {
        let (size_opt, name_opt) = font_operands(&self.operands);
        if let Some(nm) = name_opt {
            self.gs.font_utf16be = nm == "/F2";
        }
        if let Some(sz) = size_opt {
            self.gs.font_size = sz;
        }
    }

    fn move_text(&mut self, op: &str) {
        let Some((x, y)) = last_two_numeric_operands(&self.operands) else {
            return;
        };
        match op {
            "Tm" => {
                self.tlm_x = x;
                self.tlm_y = y;
            }
            _ => {
                if op == "TD" {
                    self.gs.leading = -y;
                }
                self.tlm_x += x;
                self.tlm_y += y;
            }
        }
    }

    fn next_line(&mut self) {
        self.tlm_y -= if self.gs.leading.abs() > LEADING_EPSILON {
            self.gs.leading
        } else {
            self.gs.font_size
        };
    }

    fn record_current(&mut self, text: String) {
        let pos = (self.tlm_x, self.tlm_y);
        self.collector.record(text, self.gs, pos, self.bt_counter);
    }

    fn record_string_operand(&mut self) {
        if let Some(text) = last_string_operand(&self.operands) {
            self.record_current(text);
        }
    }

    fn record_array_operand(&mut self) {
        let text = self.operands.iter().rev().find_map(|op| match op {
            PdfStreamOperand::ArrayText(s) => Some(s.clone()),
            _ => None,
        });
        if let Some(text) = text {
            self.record_current(text);
        }
    }
}

pub(crate) fn parse_pdf_content_stream_with_cmap(
    stream: &str,
    cmap: Option<&PdfToUnicodeMap>,
) -> ParsedPdfContentStream {
    let bytes = stream.as_bytes();
    let mut i = 0usize;
    let mut interp = PdfStreamInterpreter::default();
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        match step_pdf_non_code_token(bytes, i) {
            Ok(Some(step)) => {
                interp.apply_non_code(step.kind, stream, cmap);
                i = step.next_index;
            }
            Err(()) => break,
            Ok(None) => i = interp.apply_code_token(stream, i),
        }
    }
    interp.collector.finish()
}

fn last_numeric_operand(operands: &[PdfStreamOperand]) -> Option<f64> {
    operands.iter().rev().find_map(|op| match op {
        PdfStreamOperand::Num(n) => Some(*n),
        _ => None,
    })
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
    if !clean.iter().all(u8::is_ascii_hexdigit) {
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
                Some('v') => out.push('\u{000B}'),
                Some('u') => {
                    let mut hex = String::with_capacity(4);
                    let mut cloned = chars.clone();
                    for _ in 0..4 {
                        if let Some(c) = cloned.next() {
                            if c.is_ascii_hexdigit() {
                                hex.push(c);
                            } else {
                                break;
                            }
                        }
                    }
                    if hex.len() == 4 {
                        for _ in 0..4 {
                            let _ = chars.next();
                        }
                        if let Ok(code) = u16::from_str_radix(&hex, 16) {
                            if (0xD800..=0xDBFF).contains(&code) {
                                let mut look = chars.clone();
                                if look.next() == Some('\\') && look.next() == Some('u') {
                                    let mut low_hex = String::with_capacity(4);
                                    for _ in 0..4 {
                                        if let Some(lc) = look.next() {
                                            if lc.is_ascii_hexdigit() {
                                                low_hex.push(lc);
                                            }
                                        }
                                    }
                                    if low_hex.len() == 4 {
                                        if let Ok(low) = u16::from_str_radix(&low_hex, 16) {
                                            if (0xDC00..=0xDFFF).contains(&low) {
                                                for _ in 0..6 {
                                                    let _ = chars.next();
                                                }
                                                let scalar = 0x10000
                                                    + (((u32::from(code) - 0xD800) << 10)
                                                        | (u32::from(low) - 0xDC00));
                                                if let Some(sc) = char::from_u32(scalar) {
                                                    out.push(sc);
                                                    continue;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if let Some(sc) = char::from_u32(u32::from(code)) {
                                out.push(sc);
                            }
                        }
                    } else {
                        out.push('u');
                    }
                }
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
