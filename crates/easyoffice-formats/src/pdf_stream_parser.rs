#[path = "pdf_boundary_lexer.rs"]
pub(crate) mod pdf_boundary_lexer;

pub(crate) use pdf_boundary_lexer::*;
#[path = "pdf_tounicode_map.rs"]
mod pdf_tounicode_map;
pub(crate) use pdf_tounicode_map::*;

#[derive(Debug, Clone, Copy)]
struct PdfGraphicsState<'a> {
    font: PdfFontDecoder<'a>,
    font_size: f64,
    render_mode: i32,
    leading: f64,
}

impl Default for PdfGraphicsState<'_> {
    fn default() -> Self {
        Self {
            font: PdfFontDecoder::default(),
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
    /// The stream decoded to more text than [`MAX_PAGE_TEXT_BYTES`]; the rest was dropped.
    pub(crate) exceeded: bool,
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
fn matches_slot(gs: &PdfGraphicsState<'_>, pos: (f64, f64), slot: (bool, f64, f64, f64)) -> bool {
    let (invisible, size, x, y) = slot;
    (gs.render_mode == INVISIBLE_RENDER_MODE) == invisible
        && (gs.font_size - size).abs() < POS_EPSILON
        && (pos.0 - x).abs() < POS_EPSILON
        && (pos.1 - y).abs() < POS_EPSILON
}

impl PdfStreamCollector {
    fn record(&mut self, text: String, gs: PdfGraphicsState<'_>, pos: (f64, f64), bt_id: usize) {
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
    fn record_marker(&mut self, text: &str, gs: &PdfGraphicsState<'_>, pos: (f64, f64)) -> bool {
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
            exceeded: false,
            body_text,
            watermark: self.watermark,
            stamp: self.stamp,
        }
    }
}

/// Decoded text one content stream may produce. A font map turns two bytes into up to
/// [`MAX_DST_UNITS`](pdf_tounicode_map) characters, so a stream is capped by what it decodes to, not
/// by its own size.
pub(crate) const MAX_PAGE_TEXT_BYTES: usize = 32 * 1024 * 1024;

const MAX_GRAPHICS_STATE_DEPTH: usize = 64;
const LEADING_EPSILON: f64 = 1e-6;
const KERN_SPACE_THRESHOLD: f64 = -180.0;

/// Mutable interpreter state for one content stream pass.
struct PdfStreamInterpreter<'a> {
    fonts: &'a PdfPageFonts<'a>,
    in_text_object: bool,
    bt_counter: usize,
    gs: PdfGraphicsState<'a>,
    gs_stack: Vec<PdfGraphicsState<'a>>,
    tlm_x: f64,
    tlm_y: f64,
    dict_depth: usize,
    in_array: bool,
    array_buf: String,
    array_has_text: bool,
    operands: Vec<PdfStreamOperand>,
    collector: PdfStreamCollector,
    text_bytes: usize,
    text_limit: usize,
    exceeded: bool,
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

impl<'a> PdfStreamInterpreter<'a> {
    fn new(fonts: &'a PdfPageFonts<'a>, text_limit: usize) -> Self {
        Self {
            fonts,
            in_text_object: false,
            bt_counter: 0,
            gs: PdfGraphicsState::default(),
            gs_stack: Vec::new(),
            tlm_x: 0.0,
            tlm_y: 0.0,
            dict_depth: 0,
            in_array: false,
            array_buf: String::new(),
            array_has_text: false,
            operands: Vec::new(),
            collector: PdfStreamCollector::default(),
            text_bytes: 0,
            text_limit,
            exceeded: false,
        }
    }

    #[inline]
    fn in_active_text(&self) -> bool {
        self.dict_depth == 0 && self.in_text_object
    }

    fn push_text(&mut self, decoded: String) {
        self.text_bytes = self.text_bytes.saturating_add(decoded.len());
        if self.text_bytes > self.text_limit {
            self.exceeded = true;
            return;
        }
        if self.in_array {
            self.array_buf.push_str(&decoded);
            self.array_has_text = true;
        } else {
            self.operands.push(PdfStreamOperand::Str(decoded));
        }
    }

    fn apply_non_code(&mut self, kind: PdfNonCodeKind, stream: &str) {
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
                let font = self.gs.font;
                if let Some(decoded) = decode_pdf_hex_string_with_cmap(hex, font.utf16be, font.cmap)
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
            self.gs.font = self.fonts.get(nm).copied().unwrap_or_default();
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

pub(crate) fn parse_pdf_content_stream_with_fonts(
    stream: &str,
    fonts: &PdfPageFonts<'_>,
) -> ParsedPdfContentStream {
    parse_pdf_content_stream_within(stream, fonts, MAX_PAGE_TEXT_BYTES)
}

/// [`parse_pdf_content_stream_with_fonts`] that decodes at most `text_limit` bytes of text.
fn parse_pdf_content_stream_within(
    stream: &str,
    fonts: &PdfPageFonts<'_>,
    text_limit: usize,
) -> ParsedPdfContentStream {
    let bytes = stream.as_bytes();
    let mut i = 0usize;
    let mut interp = PdfStreamInterpreter::new(fonts, text_limit);
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        match step_pdf_non_code_token(bytes, i) {
            Ok(Some(step)) => {
                interp.apply_non_code(step.kind, stream);
                i = step.next_index;
            }
            Err(()) => break,
            Ok(None) => i = interp.apply_code_token(stream, i),
        }
    }
    let exceeded = interp.exceeded;
    ParsedPdfContentStream {
        exceeded,
        ..interp.collector.finish()
    }
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

/// Decodes string bytes through a ToUnicode map; `None` when the map has no hit (or no usable width).
/// Most text one string may decode to through a font map; past it the string is read without the map.
const MAX_MAPPED_BYTES: usize = 1 << 20;

fn map_bytes_with_cmap(cm: &PdfToUnicodeMap, bytes: &[u8]) -> Option<String> {
    if cm.is_empty() {
        return None;
    }
    let mut mapped = String::with_capacity(bytes.len());
    let mut any_hit = false;
    if cm.is_two_byte && bytes.len() % 2 == 0 {
        for pair in bytes.chunks_exact(2) {
            let code = u16::from_be_bytes([pair[0], pair[1]]);
            if code <= 0xFF
                && (cm.single_byte_mask[usize::from(code >> 6)] & (1u64 << (code & 63))) != 0
            {
                mapped.push(cm.single_byte_chars[usize::from(code)]);
                any_hit = true;
            } else if let Some(&ch) = cm.bmp_single.get(&code) {
                mapped.push(ch);
                any_hit = true;
            } else if let Some(s) = cm.map.get(&code) {
                mapped.push_str(s);
                any_hit = true;
            } else if let Some(ch) = char::from_u32(u32::from(code)) {
                mapped.push(ch);
            }
            if mapped.len() > MAX_MAPPED_BYTES {
                return None;
            }
        }
    } else if !cm.is_two_byte {
        for &b in bytes {
            if (cm.single_byte_mask[usize::from(b >> 6)] & (1u64 << (b & 63))) != 0 {
                mapped.push(cm.single_byte_chars[usize::from(b)]);
                any_hit = true;
            } else if let Some(s) = cm.map.get(&u16::from(b)) {
                mapped.push_str(s);
                any_hit = true;
            } else {
                mapped.push(char::from(b));
            }
            if mapped.len() > MAX_MAPPED_BYTES {
                return None;
            }
        }
    }
    any_hit.then_some(mapped)
}

const HEX_NIBBLE: [u8; 256] = {
    let mut table = [0xFFu8; 256];
    let mut i = 0usize;
    while i < 10 {
        table[b'0' as usize + i] = i as u8;
        i += 1;
    }
    let mut i = 0usize;
    while i < 6 {
        table[b'A' as usize + i] = (10 + i) as u8;
        table[b'a' as usize + i] = (10 + i) as u8;
        i += 1;
    }
    table
};

fn decode_hex_bytes_single_pass(hex: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::with_capacity(hex.len() / 2 + 1);
    let mut hi_nibble: Option<u8> = None;
    for b in hex.bytes() {
        if b.is_ascii_whitespace() {
            continue;
        }
        let n = HEX_NIBBLE[usize::from(b)];
        if n == 0xFF {
            return None;
        }
        if let Some(hi) = hi_nibble.take() {
            bytes.push((hi << 4) | n);
        } else {
            hi_nibble = Some(n);
        }
    }
    if let Some(hi) = hi_nibble {
        bytes.push(hi << 4);
    }
    Some(bytes)
}

pub(crate) fn decode_pdf_hex_string_with_cmap(
    hex: &str,
    prefer_utf16be: bool,
    cmap: Option<&PdfToUnicodeMap>,
) -> Option<String> {
    let bytes = decode_hex_bytes_single_pass(hex)?;
    if bytes.is_empty() {
        return Some(String::new());
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
    if let Some(mapped) = cmap.and_then(|cm| map_bytes_with_cmap(cm, &bytes)) {
        return Some(mapped);
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

type PdfTextChars<'a> = std::iter::Peekable<std::str::Chars<'a>>;

/// Reads exactly four leading hex digits from a cloned iterator; `None` if fewer are available.
#[inline]
fn peek_hex4(chars: &PdfTextChars<'_>) -> Option<u16> {
    let mut cloned = chars.clone();
    let mut value = 0u16;
    for _ in 0..4 {
        let digit = cloned.next().and_then(|c| c.to_digit(16))?;
        value = (value << 4) | digit as u16;
    }
    Some(value)
}

/// Low surrogate of a following `\uDC00`-style escape (non-hex characters inside it are skipped).
fn peek_low_surrogate(chars: &PdfTextChars<'_>) -> Option<u16> {
    let mut look = chars.clone();
    if look.next() != Some('\\') || look.next() != Some('u') {
        return None;
    }
    let mut low_hex = String::with_capacity(4);
    for _ in 0..4 {
        if let Some(lc) = look.next() {
            if lc.is_ascii_hexdigit() {
                low_hex.push(lc);
            }
        }
    }
    if low_hex.len() != 4 {
        return None;
    }
    u16::from_str_radix(&low_hex, 16)
        .ok()
        .filter(|low| (0xDC00..=0xDFFF).contains(low))
}

/// Handles the text after `\u`: four hex digits (with surrogate pairs) or a literal `u`.
fn decode_unicode_escape(chars: &mut PdfTextChars<'_>, out: &mut String) {
    let Some(code) = peek_hex4(chars) else {
        out.push('u');
        return;
    };
    for _ in 0..4 {
        let _ = chars.next();
    }
    if (0xD800..=0xDBFF).contains(&code) {
        if let Some(low) = peek_low_surrogate(chars) {
            for _ in 0..6 {
                let _ = chars.next();
            }
            let scalar = 0x10000 + (((u32::from(code) - 0xD800) << 10) | (u32::from(low) - 0xDC00));
            if let Some(sc) = char::from_u32(scalar) {
                out.push(sc);
                return;
            }
        }
    }
    if let Some(sc) = char::from_u32(u32::from(code)) {
        out.push(sc);
    }
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
                Some('u') => decode_unicode_escape(&mut chars, &mut out),
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
    crate::base64_codec::decode(input)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode_budget::DecodeBudget;
    use crate::EasyOfficeError;

    #[test]
    fn single_pass_hex_decoder_and_bfrange_multi_unit_utf16() {
        assert_eq!(
            decode_pdf_hex_string("48 65 6C 6C 6F", false).as_deref(),
            Some("Hello")
        );
        assert_eq!(decode_pdf_hex_string("4", false).as_deref(), Some("@"));
        assert!(decode_pdf_hex_string("4G", false).is_none());

        let cmap_stream = concat!(
            "2 beginbfrange\n",
            "<0001> <0002> <00660069>\n",
            "<0010> <0011> <D83DDE00>\n",
            "endbfrange\n"
        );
        let mut cmap = PdfToUnicodeMap::default();
        PdfToUnicodeMap::parse_cmap_stream(cmap_stream, &mut cmap, &mut DecodeBudget::new())
            .unwrap();
        assert_eq!(cmap.map.get(&0x0001).map(String::as_str), Some("fi"));
        assert_eq!(cmap.map.get(&0x0002).map(String::as_str), Some("fj"));
        assert_eq!(cmap.map.get(&0x0010).map(String::as_str), Some("😀"));
        assert_eq!(cmap.map.get(&0x0011).map(String::as_str), Some("😁"));
    }

    fn parsed_cmap(
        text: &str,
        budget: &mut DecodeBudget,
    ) -> Result<PdfToUnicodeMap, EasyOfficeError> {
        let mut cmap = PdfToUnicodeMap::default();
        PdfToUnicodeMap::parse_cmap_stream(text, &mut cmap, budget)?;
        Ok(cmap)
    }

    #[test]
    fn a_long_range_destination_does_not_multiply_into_the_table() {
        // 10 KB of destination under one range of 65535 codes: 650 MB of strings before the limit.
        let dst = "0041".repeat(2500);
        let text = format!("1 beginbfrange\n<0001> <FFFF> <{dst}>\nendbfrange\n");
        let cmap = parsed_cmap(&text, &mut DecodeBudget::new()).unwrap();
        assert!(cmap.map.is_empty());
        let long_char = format!("1 beginbfchar\n<0001> <{dst}>\nendbfchar\n");
        assert!(parsed_cmap(&long_char, &mut DecodeBudget::new())
            .unwrap()
            .map
            .is_empty());
    }

    #[test]
    fn font_maps_are_charged_to_the_document_budget() {
        let text = "1 beginbfrange\n<0001> <FFFF> <00660069>\nendbfrange\n";
        assert!(parsed_cmap(text, &mut DecodeBudget::new()).is_ok());
        let mut small = DecodeBudget::with_limit(1024 * 1024);
        assert!(matches!(
            parsed_cmap(text, &mut small),
            Err(EasyOfficeError::ResourceBudgetExceeded(_))
        ));
        let singles = "1 beginbfrange\n<0100> <FFFF> <0100>\nendbfrange\n";
        let mut small = DecodeBudget::with_limit(1024 * 1024);
        assert!(parsed_cmap(singles, &mut small).is_err());
    }

    #[test]
    fn a_string_that_would_decode_past_the_output_limit_is_read_without_the_map() {
        let cmap = parsed_cmap(
            "1 beginbfchar\n<0001> <00410042004300440045004600470048>\nendbfchar\n",
            &mut DecodeBudget::new(),
        )
        .unwrap();
        let hex = "0001".repeat(200_000);
        let decoded = decode_pdf_hex_string_with_cmap(&hex, true, Some(&cmap)).unwrap();
        assert!(decoded.len() <= 700_000, "{}", decoded.len());
        let small = decode_pdf_hex_string_with_cmap("00010001", true, Some(&cmap)).unwrap();
        assert_eq!(small, "ABCDEFGHABCDEFGH");
    }

    #[test]
    fn a_stream_that_decodes_to_too_much_text_says_so() {
        let fonts = PdfPageFonts::new();
        let stream = "BT (aaaaaaaaaa) Tj (bbbbbbbbbb) Tj ET";
        assert!(!parse_pdf_content_stream_within(stream, &fonts, 100).exceeded);
        assert!(parse_pdf_content_stream_within(stream, &fonts, 15).exceeded);
    }

    #[test]
    fn stray_angle_brackets_without_a_close_are_read_in_one_pass() {
        let text = format!("1 beginbfrange\n{}\nendbfrange\n", "< ".repeat(200_000));
        let started = std::time::Instant::now();
        assert!(parsed_cmap(&text, &mut DecodeBudget::new())
            .unwrap()
            .is_empty());
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }
}
