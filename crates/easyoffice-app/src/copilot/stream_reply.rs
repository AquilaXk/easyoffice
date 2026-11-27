//! 스트림으로 들어오는 JSON(`emit_copilot_turn` 인자 및 비네이티브 평문 `{"reply":...}`)에서
//! `operations`·`toolCalls` JSON 원문을 숨기고 `"reply"` 문자열 값만 서로게이트 안전하게 증분 추출한다.

fn parse_hex4(bytes: &[u8]) -> Option<u16> {
    if bytes.len() < 4 {
        return None;
    }
    let s = std::str::from_utf8(&bytes[..4]).ok()?;
    u16::from_str_radix(s, 16).ok()
}

pub(super) fn scan_json_str_literal(bytes: &[u8], mut i: usize) -> Option<(String, usize, bool)> {
    let mut out = String::new();
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            return Some((out, i + 1, true));
        }
        if b == b'\\' {
            if i + 1 >= bytes.len() {
                return Some((out, i, false));
            }
            match bytes[i + 1] {
                b'"' => {
                    out.push('"');
                    i += 2;
                }
                b'\\' => {
                    out.push('\\');
                    i += 2;
                }
                b'/' => {
                    out.push('/');
                    i += 2;
                }
                b'n' => {
                    out.push('\n');
                    i += 2;
                }
                b'r' => {
                    out.push('\r');
                    i += 2;
                }
                b't' => {
                    out.push('\t');
                    i += 2;
                }
                b'b' => {
                    out.push('\x08');
                    i += 2;
                }
                b'f' => {
                    out.push('\x0c');
                    i += 2;
                }
                b'u' => {
                    let Some(hi) = parse_hex4(&bytes[i + 2..]) else {
                        return Some((out, i, false));
                    };
                    if (0xD800..=0xDBFF).contains(&hi) {
                        if i + 6 == bytes.len()
                            || (i + 7 == bytes.len() && bytes[i + 6] == b'\\')
                            || (i + 12 > bytes.len()
                                && i + 8 <= bytes.len()
                                && bytes[i + 6] == b'\\'
                                && bytes[i + 7] == b'u')
                        {
                            return Some((out, i, false));
                        }
                        if i + 12 <= bytes.len() && bytes[i + 6] == b'\\' && bytes[i + 7] == b'u' {
                            if let Some(lo) = parse_hex4(&bytes[i + 8..])
                                .filter(|lo| (0xDC00..=0xDFFF).contains(lo))
                            {
                                let cp = 0x10000
                                    + ((u32::from(hi) - 0xD800) << 10)
                                    + (u32::from(lo) - 0xDC00);
                                out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
                                i += 12;
                            } else {
                                out.push('\u{FFFD}');
                                i += 6;
                            }
                        } else {
                            out.push('\u{FFFD}');
                            i += 6;
                        }
                    } else {
                        out.push(char::from_u32(u32::from(hi)).unwrap_or('\u{FFFD}'));
                        i += 6;
                    }
                }
                other => {
                    out.push(char::from(other));
                    i += 2;
                }
            }
            continue;
        }
        let s = std::str::from_utf8(&bytes[i..]).ok()?;
        let ch = s.chars().next()?;
        out.push(ch);
        i += ch.len_utf8();
    }
    Some((out, i, false))
}

/// JSON 객체 버퍼에서 최상위 `"reply":"..."` 완성 접두사를 추출한다.
pub(super) fn extract_emit_reply_prefix(json_buf: &str) -> String {
    let bytes = json_buf.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if bytes.get(i) != Some(&b'{') {
        return String::new();
    }
    i += 1;
    let mut depth = 1usize;
    let mut expecting_key = true;
    let mut saw_reply_key = false;

    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if depth == 1 && b == b'"' {
            let Some((lit, next_i, complete)) = scan_json_str_literal(bytes, i + 1) else {
                return String::new();
            };
            if expecting_key {
                saw_reply_key = complete && lit == "reply";
                expecting_key = false;
            } else if saw_reply_key {
                return lit;
            }
            if !complete {
                return String::new();
            }
            i = next_i;
            continue;
        }
        if b == b'"' {
            let Some((_, next_i, complete)) = scan_json_str_literal(bytes, i + 1) else {
                return String::new();
            };
            if !complete {
                return String::new();
            }
            i = next_i;
            continue;
        }
        match b {
            b'{' | b'[' => {
                depth += 1;
                saw_reply_key = false;
            }
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    break;
                }
            }
            b',' if depth == 1 => {
                expecting_key = true;
                saw_reply_key = false;
            }
            _ => {}
        }
        i += 1;
    }
    String::new()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum PlainJsonGuard {
    #[default]
    Undecided,
    JsonReply {
        sent: usize,
    },
    Prose,
}

fn slice_json_start(full_text: &str) -> Option<&str> {
    let trimmed = full_text.trim_start();
    if trimmed.starts_with('{') {
        return Some(trimmed);
    }
    if let Some(after_open) = trimmed.strip_prefix("```") {
        if let Some(nl) = after_open.find('\n') {
            let inner = after_open[nl + 1..].trim_start();
            if inner.starts_with('{') {
                return Some(inner);
            }
        }
    }
    None
}

impl PlainJsonGuard {
    pub fn advance(&mut self, full_text: &str, new_delta: &str, added: &mut String) {
        if new_delta.is_empty() && !matches!(self, Self::Undecided) {
            return;
        }
        match *self {
            Self::Prose => added.push_str(new_delta),
            Self::JsonReply { sent } => {
                if let Some(json_slice) = slice_json_start(full_text) {
                    let prefix = extract_emit_reply_prefix(json_slice);
                    if let Some(tail) = prefix.get(sent..) {
                        if !tail.is_empty() {
                            added.push_str(tail);
                            *self = Self::JsonReply { sent: prefix.len() };
                        }
                    }
                }
            }
            Self::Undecided => {
                let trimmed = full_text.trim_start();
                if trimmed.is_empty() {
                    return;
                }
                if trimmed.starts_with('{') {
                    let prefix = extract_emit_reply_prefix(trimmed);
                    if !prefix.is_empty() {
                        added.push_str(&prefix);
                    }
                    *self = Self::JsonReply { sent: prefix.len() };
                    return;
                }
                if trimmed.starts_with('`') {
                    if trimmed.len() < 3 && "```".starts_with(trimmed) {
                        return;
                    }
                    if let Some(after_open) = trimmed.strip_prefix("```") {
                        let Some(nl) = after_open.find('\n') else {
                            if after_open.len() <= 16 {
                                return;
                            }
                            added.push_str(full_text);
                            *self = Self::Prose;
                            return;
                        };
                        let inner = after_open[nl + 1..].trim_start();
                        if inner.is_empty() {
                            return;
                        }
                        if inner.starts_with('{') {
                            let prefix = extract_emit_reply_prefix(inner);
                            if !prefix.is_empty() {
                                added.push_str(&prefix);
                            }
                            *self = Self::JsonReply { sent: prefix.len() };
                            return;
                        }
                    }
                }
                added.push_str(full_text);
                *self = Self::Prose;
            }
        }
    }
}
