#[path = "pdf_boundary_lexer.rs"]
pub(crate) mod pdf_boundary_lexer;

pub(crate) use pdf_boundary_lexer::*;

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
    watermark: Option<String>,
    stamp: Option<String>,
}

impl PdfStreamCollector {
    fn record(&mut self, text: String, gs: PdfGraphicsState, pos: (f64, f64)) {
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
            self.stream_segments.push(text);
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

pub(crate) fn parse_pdf_content_stream(stream: &str) -> ParsedPdfContentStream {
    let bytes = stream.as_bytes();
    let mut i = 0usize;
    let mut in_text_object = false;
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
                            if let Some(decoded) = decode_pdf_hex_string(
                                &stream[inner_start..inner_end],
                                gs.font_utf16be,
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
        if dict_depth > 0 || in_array {
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
                tlm_y -= gs.leading;
            }
            "Tm" => {
                let nums = extract_numeric_operands(&operands);
                if nums.len() >= 2 {
                    tlm_x = nums[nums.len() - 2];
                    tlm_y = nums[nums.len() - 1];
                }
            }
            "'" | "\"" if in_text_object => {
                tlm_y -= gs.leading;
                if let Some(text) = last_string_operand(&operands) {
                    collector.record(text, gs, (tlm_x, tlm_y));
                }
            }
            "Tj" if in_text_object => {
                if let Some(text) = last_string_operand(&operands) {
                    collector.record(text, gs, (tlm_x, tlm_y));
                }
            }
            "TJ" if in_text_object => {
                if let Some(text) = operands.iter().rev().find_map(|op| match op {
                    PdfStreamOperand::ArrayText(s) => Some(s.clone()),
                    _ => None,
                }) {
                    collector.record(text, gs, (tlm_x, tlm_y));
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
    out
}
