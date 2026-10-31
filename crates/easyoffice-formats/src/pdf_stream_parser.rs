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

#[derive(Debug, Clone, Copy)]
pub(crate) enum PdfNonCodeKind {
    Comment {
        start: usize,
        end: usize,
    },
    ParenString {
        inner_start: usize,
        inner_end: usize,
    },
    DictOpen,
    DictClose,
    HexString {
        inner_start: usize,
        inner_end: usize,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PdfNonCodeStep {
    pub(crate) next_index: usize,
    pub(crate) kind: PdfNonCodeKind,
}

pub(crate) fn is_pdf_ws(b: u8) -> bool {
    matches!(b, 0x00 | 0x09 | 0x0A | 0x0C | 0x0D | 0x20)
}

pub(crate) fn is_pdf_delim_or_ws(b: u8) -> bool {
    is_pdf_ws(b) || b"()<>[]{}/%".contains(&b)
}

pub(crate) fn decode_pdf_name(raw_after_slash: &[u8]) -> String {
    let mut out = Vec::with_capacity(raw_after_slash.len() + 1);
    out.push(b'/');
    let mut i = 0usize;
    while i < raw_after_slash.len() {
        if raw_after_slash[i] == b'#'
            && i + 2 < raw_after_slash.len()
            && raw_after_slash[i + 1].is_ascii_hexdigit()
            && raw_after_slash[i + 2].is_ascii_hexdigit()
        {
            if let Ok(hex_str) = std::str::from_utf8(&raw_after_slash[i + 1..i + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex_str, 16) {
                    out.push(byte);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(raw_after_slash[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(crate) fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack.len() >= needle.len()
        && haystack.windows(needle.len()).any(|w| w == needle)
}

pub(crate) fn is_standalone_pdf_kw(bytes: &[u8], pos: usize, kw: &[u8]) -> bool {
    pos + kw.len() <= bytes.len()
        && &bytes[pos..pos + kw.len()] == kw
        && (pos == 0 || is_pdf_delim_or_ws(bytes[pos - 1]))
        && (pos + kw.len() == bytes.len() || is_pdf_delim_or_ws(bytes[pos + kw.len()]))
}

pub(crate) fn skip_pdf_ws_only(bytes: &[u8], mut p: usize, limit: usize) -> usize {
    while p < limit && is_pdf_ws(bytes[p]) {
        p += 1;
    }
    p
}

pub(crate) fn skip_pdf_ws_and_comments(bytes: &[u8], mut p: usize, limit: usize) -> usize {
    while p < limit {
        if is_pdf_ws(bytes[p]) {
            p += 1;
        } else if bytes[p] == b'%' {
            p += 1;
            while p < limit && bytes[p] != b'\n' && bytes[p] != b'\r' {
                p += 1;
            }
        } else {
            break;
        }
    }
    p
}

pub(crate) fn parse_indirect_obj_header(
    bytes: &[u8],
    pos: usize,
    limit: usize,
) -> Option<(u32, u16, usize)> {
    if pos >= limit
        || !bytes[pos].is_ascii_digit()
        || (pos > 0 && !is_pdf_delim_or_ws(bytes[pos - 1]))
    {
        return None;
    }
    let mut q = pos;
    while q < limit && bytes[q].is_ascii_digit() {
        q += 1;
    }
    let obj_num = std::str::from_utf8(&bytes[pos..q])
        .ok()?
        .parse::<u32>()
        .ok()?;
    let q_gen = skip_pdf_ws_only(bytes, q, limit);
    if q_gen == q || q_gen >= limit || !bytes[q_gen].is_ascii_digit() {
        return None;
    }
    let mut r = q_gen;
    while r < limit && bytes[r].is_ascii_digit() {
        r += 1;
    }
    let gen_num = std::str::from_utf8(&bytes[q_gen..r])
        .ok()?
        .parse::<u16>()
        .ok()?;
    let r_obj = skip_pdf_ws_only(bytes, r, limit);
    if r_obj == r || r_obj + 3 > limit || !is_standalone_pdf_kw(&bytes[..limit], r_obj, b"obj") {
        return None;
    }
    Some((obj_num, gen_num, r_obj + 3))
}

pub(crate) fn match_indirect_obj_header(bytes: &[u8], pos: usize, limit: usize) -> Option<usize> {
    parse_indirect_obj_header(bytes, pos, limit).map(|(_, _, end)| end)
}

pub(crate) fn skip_pdf_ws_stopping_at_eof(bytes: &[u8], mut pos: usize, limit: usize) -> usize {
    let end = limit.min(bytes.len());
    while pos < end {
        if is_pdf_ws(bytes[pos]) {
            pos += 1;
        } else if bytes[pos] == b'%' {
            if pos + 5 <= bytes.len() && &bytes[pos..pos + 5] == b"%%EOF" {
                break;
            }
            while pos < end && bytes[pos] != b'\n' && bytes[pos] != b'\r' {
                pos += 1;
            }
        } else {
            break;
        }
    }
    pos
}

pub(crate) fn has_structural_after_endobj_cached(
    slice: &[u8],
    after_endobj: usize,
    cache: &mut (usize, usize),
) -> bool {
    let len = slice.len();
    if after_endobj >= len || (!is_pdf_ws(slice[after_endobj]) && slice[after_endobj] != b'%') {
        return false;
    }
    if cache.0 <= after_endobj && after_endobj < cache.1 {
        return false;
    }
    let q = skip_pdf_ws_and_comments(slice, after_endobj, len);
    if match_indirect_obj_header(slice, q, len).is_some() {
        return true;
    }
    if q + 4 <= len
        && is_standalone_pdf_kw(slice, q, b"xref")
        && (q + 4 == len || is_pdf_ws(slice[q + 4]) || slice[q + 4] == b'%')
    {
        return true;
    }
    if q + 7 < len && is_standalone_pdf_kw(slice, q, b"trailer") {
        let qt = skip_pdf_ws_and_comments(slice, q + 7, len);
        if qt + 1 < len && slice[qt] == b'<' && slice[qt + 1] == b'<' {
            return true;
        }
    }
    if q + 9 < len && is_standalone_pdf_kw(slice, q, b"startxref") {
        let qs = skip_pdf_ws_and_comments(slice, q + 9, len);
        if qs > q + 9 && qs < len && slice[qs].is_ascii_digit() {
            return true;
        }
    }
    *cache = (after_endobj, q.max(after_endobj + 1));
    false
}

pub(crate) fn span_crosses_object_boundary(bytes: &[u8], start: usize, end: usize) -> bool {
    if start >= end || end > bytes.len() {
        return false;
    }
    let slice = &bytes[start..end];
    let len = slice.len();
    let paren_table = ParenJumpTable::build(slice);
    let mut endobj_cache = (0usize, 0usize);
    let mut p = 0usize;
    while p < len {
        match step_indexed_non_code_token(slice, &paren_table, p, len) {
            Ok(Some(step)) => {
                p = step.next_index;
                continue;
            }
            Err(()) => {
                p += 1;
                continue;
            }
            Ok(None) => {}
        }
        if p + 9 <= len && is_standalone_pdf_kw(slice, p, b"endstream") {
            let q = skip_pdf_ws_and_comments(slice, p + 9, len);
            if (q + 6 <= len
                && is_standalone_pdf_kw(slice, q, b"endobj")
                && has_structural_after_endobj_cached(slice, q + 6, &mut endobj_cache))
                || match_indirect_obj_header(slice, q, len).is_some()
            {
                return true;
            }
        } else if p + 6 <= len
            && is_standalone_pdf_kw(slice, p, b"endobj")
            && has_structural_after_endobj_cached(slice, p + 6, &mut endobj_cache)
        {
            return true;
        }
        p += 1;
    }
    false
}

pub(crate) fn validate_balanced_pdf_literals(stream_bytes: &[u8]) -> bool {
    let mut i = 0usize;
    while i < stream_bytes.len() {
        match step_pdf_non_code_token(stream_bytes, i) {
            Ok(Some(step)) => {
                i = step.next_index;
            }
            Ok(None) => i += 1,
            Err(()) => return false,
        }
    }
    true
}

pub(crate) fn advance_past_stream_eol(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'\r' {
        i += 1;
        if i < bytes.len() && bytes[i] == b'\n' {
            i += 1;
        }
    } else if i < bytes.len() && bytes[i] == b'\n' {
        i += 1;
    }
    i
}

pub(crate) fn step_pdf_non_code_token(
    bytes: &[u8],
    i: usize,
) -> Result<Option<PdfNonCodeStep>, ()> {
    match bytes[i] {
        b'%' => {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b'\n' && bytes[j] != b'\r' {
                j += 1;
            }
            Ok(Some(PdfNonCodeStep {
                next_index: j,
                kind: PdfNonCodeKind::Comment { start: i, end: j },
            }))
        }
        b'(' => {
            let mut depth = 1usize;
            let mut j = i + 1;
            while j < bytes.len() && depth > 0 {
                if bytes[j] == b'\\' {
                    j = (j + 2).min(bytes.len());
                    continue;
                }
                if bytes[j] == b'(' {
                    depth += 1;
                } else if bytes[j] == b')' {
                    depth -= 1;
                }
                j += 1;
            }
            if depth != 0 {
                Err(())
            } else {
                Ok(Some(PdfNonCodeStep {
                    next_index: j,
                    kind: PdfNonCodeKind::ParenString {
                        inner_start: i + 1,
                        inner_end: j - 1,
                    },
                }))
            }
        }
        b'<' if i + 1 < bytes.len() && bytes[i + 1] == b'<' => Ok(Some(PdfNonCodeStep {
            next_index: i + 2,
            kind: PdfNonCodeKind::DictOpen,
        })),
        b'>' if i + 1 < bytes.len() && bytes[i + 1] == b'>' => Ok(Some(PdfNonCodeStep {
            next_index: i + 2,
            kind: PdfNonCodeKind::DictClose,
        })),
        b'<' => {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b'>' {
                if !bytes[j].is_ascii_hexdigit() && !is_pdf_ws(bytes[j]) {
                    return Err(());
                }
                j += 1;
            }
            if j >= bytes.len() {
                Err(())
            } else {
                Ok(Some(PdfNonCodeStep {
                    next_index: j + 1,
                    kind: PdfNonCodeKind::HexString {
                        inner_start: i + 1,
                        inner_end: j,
                    },
                }))
            }
        }
        _ => Ok(None),
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

const MAX_PAREN_JUMP_DEPTH: usize = 4096;

pub(crate) fn is_paren_reset_boundary(
    bytes: &[u8],
    i: usize,
    endobj_cache: &mut (usize, usize),
) -> Option<usize> {
    let len = bytes.len();
    if i + 5 <= len && &bytes[i..i + 5] == b"%%EOF" {
        return Some(i + 5);
    }
    if is_standalone_pdf_kw(bytes, i, b"endobj") {
        let mut b = i;
        while b > 0 && matches!(bytes[b - 1], b' ' | b'\t') {
            b -= 1;
        }
        let preceded_structurally = b > 0
            && (matches!(bytes[b - 1], b'\n' | b'\r' | b']')
                || (b >= 2 && bytes[b - 2] == b'>' && bytes[b - 1] == b'>'));
        let mut a = i + 6;
        while a < len && matches!(bytes[a], b' ' | b'\t') {
            a += 1;
        }
        let followed_by_eol = a >= len || matches!(bytes[a], b'\n' | b'\r' | b'%');
        if (preceded_structurally || followed_by_eol)
            && has_structural_after_endobj_cached(bytes, i + 6, endobj_cache)
        {
            return Some(i + 6);
        }
    }
    if i > 0 && matches!(bytes[i - 1], b'\n' | b'\r') {
        let line_tok = {
            let mut t = i;
            while t < len && matches!(bytes[t], b' ' | b'\t') {
                t += 1;
            }
            t
        };
        if is_standalone_pdf_kw(bytes, line_tok, b"trailer") {
            let qt = skip_pdf_ws_stopping_at_eof(bytes, line_tok + 7, len);
            if qt + 1 < len && bytes[qt] == b'<' && bytes[qt + 1] == b'<' {
                return Some(line_tok);
            }
        }
        if is_standalone_pdf_kw(bytes, line_tok, b"xref")
            && line_tok + 4 < len
            && (is_pdf_ws(bytes[line_tok + 4]) || bytes[line_tok + 4] == b'%')
        {
            let qx = skip_pdf_ws_stopping_at_eof(bytes, line_tok + 4, len);
            let mut qd = qx;
            while qd < len && bytes[qd].is_ascii_digit() {
                qd += 1;
            }
            let qd_ws = skip_pdf_ws_only(bytes, qd, len);
            let valid_xref_follow = qx >= len
                || (qx + 5 <= len && &bytes[qx..qx + 5] == b"%%EOF")
                || is_standalone_pdf_kw(bytes, qx, b"trailer")
                || (qd > qx && qd_ws > qd && qd_ws < len && bytes[qd_ws].is_ascii_digit());
            if valid_xref_follow {
                return Some(line_tok);
            }
        }
        if is_standalone_pdf_kw(bytes, line_tok, b"startxref") {
            let qs = skip_pdf_ws_stopping_at_eof(bytes, line_tok + 9, len);
            if qs > line_tok + 9 && qs < len && bytes[qs].is_ascii_digit() {
                return Some(line_tok);
            }
        }
        if let Some((_, _, after_hdr)) = parse_indirect_obj_header(bytes, line_tok, len) {
            let qo = skip_pdf_ws_stopping_at_eof(bytes, after_hdr, len);
            if qo + 1 < len && bytes[qo] == b'<' && bytes[qo + 1] == b'<' {
                return Some(line_tok);
            }
        }
    }
    None
}

pub(crate) struct ParenJumpTable {
    pairs: Vec<(u32, u32)>,
}

impl ParenJumpTable {
    pub(crate) fn build(bytes: &[u8]) -> Self {
        Self::build_inner(bytes, false)
    }

    pub(crate) fn build_damaged(bytes: &[u8]) -> Self {
        Self::build_inner(bytes, true)
    }

    fn build_inner(bytes: &[u8], reset_on_boundaries: bool) -> Self {
        let len = bytes.len();
        let mut pairs: Vec<(u32, u32)> = Vec::new();
        let mut stack: std::collections::VecDeque<u32> = std::collections::VecDeque::new();
        let mut endobj_cache = (0usize, 0usize);
        let mut i = 0usize;
        while i < len {
            if reset_on_boundaries && !stack.is_empty() {
                if let Some(next_i) = is_paren_reset_boundary(bytes, i, &mut endobj_cache) {
                    stack.clear();
                    if next_i > i {
                        i = next_i;
                        continue;
                    }
                }
            }
            let in_paren = !stack.is_empty();
            if !in_paren && bytes[i] == b'%' {
                if i + 5 <= len && &bytes[i..i + 5] == b"%%EOF" {
                    i += 5;
                    continue;
                }
                while i < len && bytes[i] != b'\n' && bytes[i] != b'\r' {
                    i += 1;
                }
                continue;
            }
            if in_paren && bytes[i] == b'\\' {
                i = (i + 2).min(len);
                continue;
            }
            if bytes[i] == b'(' {
                if let Ok(pos32) = u32::try_from(i) {
                    if stack.len() == MAX_PAREN_JUMP_DEPTH {
                        stack.pop_front();
                    }
                    stack.push_back(pos32);
                }
            } else if bytes[i] == b')' {
                if let Some(open_pos) = stack.pop_back() {
                    if let Ok(end32) = u32::try_from(i + 1) {
                        pairs.push((open_pos, end32));
                    }
                }
            }
            i += 1;
        }
        pairs.sort_unstable_by_key(|&(open_pos, _)| open_pos);
        Self { pairs }
    }

    pub(crate) fn close_end(&self, open_pos: usize, limit: usize) -> Option<usize> {
        let open32 = u32::try_from(open_pos).ok()?;
        let idx = self.pairs.binary_search_by_key(&open32, |&(o, _)| o).ok()?;
        let end_pos = self.pairs[idx].1 as usize;
        (end_pos <= limit).then_some(end_pos)
    }
}

pub(crate) fn step_indexed_non_code_token(
    bytes: &[u8],
    table: &ParenJumpTable,
    i: usize,
    limit: usize,
) -> Result<Option<PdfNonCodeStep>, ()> {
    if i >= limit {
        return Ok(None);
    }
    if bytes[i] == b'(' {
        let next_index = table.close_end(i, limit).ok_or(())?;
        return Ok(Some(PdfNonCodeStep {
            next_index,
            kind: PdfNonCodeKind::ParenString {
                inner_start: i + 1,
                inner_end: next_index - 1,
            },
        }));
    }
    step_pdf_non_code_token(&bytes[..limit], i)
}

pub(crate) fn skip_damaged_truncated_stream(bytes: &[u8], stream_start: usize) -> usize {
    let mut endobj_cache = (0usize, 0usize);
    let mut q = stream_start;
    while q < bytes.len() {
        if is_standalone_pdf_kw(bytes, q, b"endobj")
            && (q + 6 >= bytes.len()
                || has_structural_after_endobj_cached(bytes, q + 6, &mut endobj_cache))
        {
            return q + 6;
        }
        q += 1;
    }
    bytes.len()
}
