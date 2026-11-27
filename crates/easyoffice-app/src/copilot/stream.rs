//! 참조 구현: continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., 커밋 5522c6f4, see THIRD_PARTY_NOTICES.md)
//! `packages/fetch/src/stream.ts:54-99`(`parseDataLine`·`parseSseLine`: `data:` 한 줄의 JSON, `error` 필드는 스트림 오류, `[DONE]`),
//! `core/llm/llms/OpenAI.ts:686`(`choices[0].delta.content`), `core/llm/llms/Anthropic.ts:346-360`(`content_block_delta`의 `text_delta`·`thinking_delta`),
//! `core/llm/llms/Gemini.ts:369-455`(`processGeminiResponse`)를 옮기고 균형 중괄호 스캐너와 네이티브 도구 델타 조립을 더했다.

use easyoffice_formats::{EasyOfficeError, StrictJsonValue};
use sse_core::{MessageEvent, SseDecoder, SseEvent};

use super::providers::extract_provider_completion_with_usage;
use super::schema::{synthesize_completion_from_tools, NativeToolInvocation};
use super::types::{ProviderKind, MAX_LLM_RESPONSE_BYTES, MAX_LLM_TOOL_CALLS};
use super::usage::TokenUsage;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ToolCallChunk {
    slot: usize,
    name: Option<String>,
    args_piece: String,
}

/// 이벤트 하나가 더한 것.
#[derive(Debug, Default, PartialEq, Eq)]
struct Delta {
    text: String,
    reasoning: String,
    tool_chunks: Vec<ToolCallChunk>,
    usage: Option<TokenUsage>,
    error: Option<String>,
    done: bool,
}

fn read_delta(kind: ProviderKind, event: &MessageEvent) -> Option<Delta> {
    if event.data.trim() == "[DONE]" {
        return Some(Delta {
            done: true,
            ..Delta::default()
        });
    }
    let parsed = StrictJsonValue::parse(&event.data).ok()?;
    let root = parsed.as_object()?;
    let mut delta = Delta::default();
    if let Some(err) = StrictJsonValue::get_field(root, "error") {
        delta.error = Some(error_message(err));
        return Some(delta);
    }
    delta.usage = super::usage::extract_usage_from_root(kind, root);
    match kind {
        ProviderKind::Anthropic => read_anthropic(root, event.event.as_ref(), &mut delta),
        ProviderKind::Google => read_google(root, &mut delta),
        _ => read_openai(root, &mut delta),
    }
    Some(delta)
}

fn read_openai(root: &[(String, StrictJsonValue)], delta: &mut Delta) {
    let Some(choice) = StrictJsonValue::get_field(root, "choices")
        .and_then(StrictJsonValue::as_array)
        .and_then(|c| c.first())
        .and_then(StrictJsonValue::as_object)
    else {
        return;
    };
    if let Some(d) = StrictJsonValue::get_field(choice, "delta")
        .or_else(|| StrictJsonValue::get_field(choice, "message"))
        .and_then(StrictJsonValue::as_object)
    {
        delta
            .text
            .push_str(StrictJsonValue::get_str(d, "content").unwrap_or(""));
        delta.reasoning.push_str(
            StrictJsonValue::get_str(d, "reasoning_content")
                .or_else(|| StrictJsonValue::get_str(d, "reasoning"))
                .unwrap_or(""),
        );
        if let Some(tcs) =
            StrictJsonValue::get_field(d, "tool_calls").and_then(StrictJsonValue::as_array)
        {
            for tc in tcs {
                let Some(tc_obj) = tc.as_object() else {
                    continue;
                };
                let slot = StrictJsonValue::get_field(tc_obj, "index")
                    .and_then(StrictJsonValue::as_u64)
                    .and_then(|n| usize::try_from(n).ok())
                    .unwrap_or(0);
                if slot >= MAX_LLM_TOOL_CALLS {
                    continue;
                }
                if let Some(fn_obj) = StrictJsonValue::get_field(tc_obj, "function")
                    .and_then(StrictJsonValue::as_object)
                {
                    delta.tool_chunks.push(ToolCallChunk {
                        slot,
                        name: StrictJsonValue::get_str(fn_obj, "name").map(ToString::to_string),
                        args_piece: StrictJsonValue::get_str(fn_obj, "arguments")
                            .unwrap_or("")
                            .to_string(),
                    });
                }
            }
        }
    }
    delta.done = StrictJsonValue::get_str(choice, "finish_reason").is_some();
}

fn read_anthropic(root: &[(String, StrictJsonValue)], event_name: &str, delta: &mut Delta) {
    let kind = StrictJsonValue::get_str(root, "type").unwrap_or(event_name);
    let slot = StrictJsonValue::get_field(root, "index")
        .and_then(StrictJsonValue::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(0);
    match kind {
        "content_block_start" => {
            if slot < MAX_LLM_TOOL_CALLS {
                if let Some(cb) = StrictJsonValue::get_field(root, "content_block")
                    .and_then(StrictJsonValue::as_object)
                {
                    if StrictJsonValue::get_str(cb, "type") == Some("tool_use") {
                        if let Some(name) = StrictJsonValue::get_str(cb, "name") {
                            delta.tool_chunks.push(ToolCallChunk {
                                slot,
                                name: Some(name.to_string()),
                                args_piece: String::new(),
                            });
                        }
                    }
                }
            }
        }
        "content_block_delta" => {
            let Some(d) =
                StrictJsonValue::get_field(root, "delta").and_then(StrictJsonValue::as_object)
            else {
                return;
            };
            match StrictJsonValue::get_str(d, "type").unwrap_or("") {
                "text_delta" => delta
                    .text
                    .push_str(StrictJsonValue::get_str(d, "text").unwrap_or("")),
                "thinking_delta" => delta
                    .reasoning
                    .push_str(StrictJsonValue::get_str(d, "thinking").unwrap_or("")),
                "input_json_delta" if slot < MAX_LLM_TOOL_CALLS => {
                    if let Some(partial) = StrictJsonValue::get_str(d, "partial_json") {
                        delta.tool_chunks.push(ToolCallChunk {
                            slot,
                            name: None,
                            args_piece: partial.to_string(),
                        });
                    }
                }
                _ => {}
            }
        }
        "message_stop" => delta.done = true,
        _ => {}
    }
}

fn read_google(root: &[(String, StrictJsonValue)], delta: &mut Delta) {
    let parts = StrictJsonValue::get_field(root, "candidates")
        .and_then(StrictJsonValue::as_array)
        .and_then(|c| c.first())
        .and_then(StrictJsonValue::as_object)
        .and_then(|c| StrictJsonValue::get_field(c, "content"))
        .and_then(StrictJsonValue::as_object)
        .and_then(|c| StrictJsonValue::get_field(c, "parts"))
        .and_then(StrictJsonValue::as_array);
    let mut fc_slot = 0usize;
    for part in parts
        .into_iter()
        .flatten()
        .filter_map(StrictJsonValue::as_object)
    {
        if let Some(text) = StrictJsonValue::get_str(part, "text") {
            let thought = StrictJsonValue::get_field(part, "thought")
                .and_then(StrictJsonValue::as_bool)
                .unwrap_or(false);
            if thought {
                delta.reasoning.push_str(text);
            } else {
                delta.text.push_str(text);
            }
        }
        if let Some(fc) =
            StrictJsonValue::get_field(part, "functionCall").and_then(StrictJsonValue::as_object)
        {
            if fc_slot < MAX_LLM_TOOL_CALLS {
                if let Some(name) = StrictJsonValue::get_str(fc, "name") {
                    let args = StrictJsonValue::get_field(fc, "args")
                        .map_or_else(|| "{}".to_string(), StrictJsonValue::to_json_string);
                    delta.tool_chunks.push(ToolCallChunk {
                        slot: MAX_LLM_TOOL_CALLS + fc_slot,
                        name: Some(name.to_string()),
                        args_piece: args,
                    });
                    fc_slot += 1;
                }
            }
        }
    }
}

fn read_sse_deltas(
    kind: ProviderKind,
    decoder: &mut SseDecoder,
    mut chunk: &[u8],
) -> Result<Vec<Delta>, String> {
    let mut deltas = Vec::new();
    while let Some(event) = decoder.next(&mut chunk) {
        let SseEvent::Message(event) =
            event.map_err(|_| "SSE 이벤트 하나가 허용 크기를 넘었습니다.".to_string())?
        else {
            continue;
        };
        let Some(delta) = read_delta(kind, &event) else {
            continue;
        };
        let failed = delta.error.is_some();
        deltas.push(delta);
        if failed {
            break;
        }
    }
    Ok(deltas)
}

fn error_message(err: &StrictJsonValue) -> String {
    err.as_object()
        .and_then(|o| StrictJsonValue::get_str(o, "message"))
        .or_else(|| err.as_str())
        .unwrap_or("모델 서버가 스트림 오류를 알렸습니다.")
        .to_string()
}

/// JSON 배열 스트림(`[{..},{..}]` 또는 `[{..}\n,{..}\n]`)에서 최상위 객체 경계를 균형 중괄호로 찾아 하나씩 꺼낸다.
#[derive(Default)]
struct JsonArrayReader {
    buffer: Vec<u8>,
    started: bool,
    /// Where the scan of the object at the head of `buffer` stands, so a chunk continues it.
    scan: ObjectScan,
    /// Bytes the scan has looked at in all (a test checks each is looked at once).
    #[cfg(test)]
    examined: usize,
}

/// The state of a scan for the end of one top-level JSON object: nesting depth and whether the
/// next byte is inside a string or after a backslash, at byte `pos` of the buffer.
#[derive(Default)]
struct ObjectScan {
    pos: usize,
    depth: usize,
    in_string: bool,
    escaped: bool,
}

fn parse_object(kind: ProviderKind, bytes: &[u8]) -> Option<Delta> {
    let text = std::str::from_utf8(bytes).ok()?;
    let parsed = StrictJsonValue::parse(text).ok()?;
    let object = parsed.as_object()?;
    let mut delta = Delta::default();
    if let Some(err) = StrictJsonValue::get_field(object, "error") {
        delta.error = Some(error_message(err));
    } else {
        delta.usage = super::usage::extract_usage_from_root(kind, object);
        match kind {
            ProviderKind::Google => read_google(object, &mut delta),
            ProviderKind::Anthropic => read_anthropic(object, "", &mut delta),
            _ => read_openai(object, &mut delta),
        }
    }
    Some(delta)
}

/// `buf[0] == b'{'`일 때 문자열 리터럴·이스케이프를 건너뛰며 닫는 최상위 `}` 다음 바이트 오프셋을 찾는다.
/// `scan`이 이전 호출이 멈춘 자리를 기억하므로 같은 `buf`에 바이트가 덧붙은 호출은 새 바이트만 본다.
fn scan_complete_json_object(buf: &[u8], scan: &mut ObjectScan) -> Option<usize> {
    if buf.first() != Some(&b'{') {
        return None;
    }
    for (idx, &b) in buf.iter().enumerate().skip(scan.pos) {
        if scan.in_string {
            if scan.escaped {
                scan.escaped = false;
            } else if b == b'\\' {
                scan.escaped = true;
            } else if b == b'"' {
                scan.in_string = false;
            }
            continue;
        }
        match b {
            b'"' => scan.in_string = true,
            b'{' => scan.depth += 1,
            b'}' => {
                scan.depth = scan.depth.saturating_sub(1);
                if scan.depth == 0 {
                    *scan = ObjectScan::default();
                    return Some(idx + 1);
                }
            }
            _ => {}
        }
    }
    scan.pos = buf.len();
    None
}

impl JsonArrayReader {
    fn trim_head(&mut self) {
        let mut cut = 0;
        while !self.started {
            match self.buffer.get(cut) {
                Some(b) if b.is_ascii_whitespace() => cut += 1,
                Some(b'[') => {
                    cut += 1;
                    self.started = true;
                }
                Some(_) => self.started = true,
                None => break,
            }
        }
        while self
            .buffer
            .get(cut)
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b',' || *b == b']')
        {
            cut += 1;
        }
        self.buffer.drain(..cut);
    }

    fn push(&mut self, kind: ProviderKind, chunk: &[u8]) -> Result<Vec<Delta>, &'static str> {
        self.buffer.extend_from_slice(chunk);
        if self.buffer.len() > MAX_LLM_RESPONSE_BYTES {
            return Err("모델 응답 크기가 허용 상한(512 KiB)을 초과했습니다.");
        }
        let mut deltas = Vec::new();
        loop {
            self.trim_head();
            if self.buffer.is_empty() {
                break;
            }
            #[cfg(test)]
            {
                self.examined += self.buffer.len().saturating_sub(self.scan.pos);
            }
            let Some(obj_end) = scan_complete_json_object(&self.buffer, &mut self.scan) else {
                break;
            };
            if let Some(delta) = parse_object(kind, &self.buffer[..obj_end]) {
                let is_err = delta.error.is_some();
                deltas.push(delta);
                self.buffer.drain(..obj_end);
                if is_err {
                    break;
                }
            } else {
                self.buffer.drain(..obj_end);
            }
        }
        Ok(deltas)
    }
}

fn sniff_first_payload_byte(buf: &[u8]) -> Option<u8> {
    const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";
    let rest = if buf.starts_with(UTF8_BOM) {
        &buf[UTF8_BOM.len()..]
    } else if UTF8_BOM.starts_with(buf) {
        return None;
    } else {
        buf
    };
    rest.iter().copied().find(|b| !b.is_ascii_whitespace())
}

enum Wire {
    Auto(Vec<u8>),
    Sse(SseDecoder),
    JsonArray(JsonArrayReader),
    PlainJson(usize),
}

fn parse_hex4(bytes: &[u8]) -> Option<u16> {
    if bytes.len() < 4 {
        return None;
    }
    let s = std::str::from_utf8(&bytes[..4]).ok()?;
    u16::from_str_radix(s, 16).ok()
}

fn scan_json_str_literal(bytes: &[u8], mut i: usize) -> Option<(String, usize, bool)> {
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

/// `emit_copilot_turn` 도구 인자 JSON 버퍼에서 최상위 `"reply":"..."` 완성 접두사를 추출한다.
fn extract_emit_reply_prefix(json_buf: &str) -> String {
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

pub struct StreamAssembler {
    kind: ProviderKind,
    wire: Wire,
    text: String,
    reasoning: String,
    tool_slots: Vec<(usize, String, String)>,
    emit_reply_sent: usize,
    usage: TokenUsage,
    has_usage: bool,
    tool_bytes: usize,
    events: u32,
    failure: Option<String>,
}

impl StreamAssembler {
    pub fn new(kind: ProviderKind) -> Self {
        Self {
            kind,
            wire: Wire::Auto(Vec::new()),
            text: String::new(),
            reasoning: String::new(),
            tool_slots: Vec::new(),
            emit_reply_sent: 0,
            usage: TokenUsage::default(),
            has_usage: false,
            tool_bytes: 0,
            events: 0,
            failure: None,
        }
    }

    pub fn feed(&mut self, chunk: &[u8]) -> Option<String> {
        let deltas = match &mut self.wire {
            Wire::Auto(pre) => {
                pre.extend_from_slice(chunk);
                if pre.len() > MAX_LLM_RESPONSE_BYTES {
                    Err("모델 응답 크기가 허용 상한(512 KiB)을 초과했습니다.".to_string())
                } else if let Some(first) = sniff_first_payload_byte(pre) {
                    let initial = std::mem::take(pre);
                    match first {
                        b'[' => {
                            let mut reader = JsonArrayReader::default();
                            let res = reader.push(self.kind, &initial).map_err(str::to_string);
                            self.wire = Wire::JsonArray(reader);
                            res
                        }
                        b'{' => {
                            self.wire = Wire::PlainJson(initial.len());
                            Ok(Vec::new())
                        }
                        _ => {
                            let mut decoder = SseDecoder::new();
                            let res = read_sse_deltas(self.kind, &mut decoder, &initial);
                            self.wire = Wire::Sse(decoder);
                            res
                        }
                    }
                } else {
                    Ok(Vec::new())
                }
            }
            Wire::Sse(decoder) => read_sse_deltas(self.kind, decoder, chunk),
            Wire::JsonArray(reader) => reader.push(self.kind, chunk).map_err(str::to_string),
            Wire::PlainJson(seen) => {
                *seen = seen.saturating_add(chunk.len());
                if *seen > MAX_LLM_RESPONSE_BYTES {
                    Err("모델 응답 크기가 허용 상한(512 KiB)을 초과했습니다.".to_string())
                } else {
                    Ok(Vec::new())
                }
            }
        };
        let mut added = String::new();
        let mut touched_emit = false;
        match deltas {
            Ok(deltas) => {
                for delta in deltas {
                    self.events += 1;
                    if let Some(u) = delta.usage {
                        self.usage.merge_latest(u);
                        self.has_usage = true;
                    }
                    if let Some(err) = delta.error {
                        self.failure = Some(err);
                        break;
                    }
                    self.reasoning.push_str(&delta.reasoning);
                    self.text.push_str(&delta.text);
                    added.push_str(&delta.text);
                    for tc in delta.tool_chunks {
                        self.tool_bytes = self.tool_bytes.saturating_add(tc.args_piece.len());
                        if tc.slot >= MAX_LLM_TOOL_CALLS {
                            if self.tool_slots.len() < MAX_LLM_TOOL_CALLS * 2 {
                                let is_emit = tc.name.as_deref() == Some("emit_copilot_turn");
                                self.tool_slots.push((
                                    usize::MAX,
                                    tc.name.unwrap_or_default(),
                                    tc.args_piece,
                                ));
                                touched_emit |= is_emit;
                            }
                        } else if let Some(existing) =
                            self.tool_slots.iter_mut().find(|(s, _, _)| *s == tc.slot)
                        {
                            if let Some(name) = tc.name {
                                if existing.1.is_empty() {
                                    existing.1 = name;
                                }
                            }
                            existing.2.push_str(&tc.args_piece);
                            if existing.1 == "emit_copilot_turn" {
                                touched_emit = true;
                            }
                        } else if self.tool_slots.len() < MAX_LLM_TOOL_CALLS * 2 {
                            let name = tc.name.unwrap_or_default();
                            if name == "emit_copilot_turn" {
                                touched_emit = true;
                            }
                            self.tool_slots.push((tc.slot, name, tc.args_piece));
                        }
                    }
                }
            }
            Err(message) => self.failure = Some(message),
        }
        if touched_emit && self.text.is_empty() {
            if let Some((_, _, args)) = self
                .tool_slots
                .iter()
                .find(|(_, name, _)| name == "emit_copilot_turn")
            {
                let prefix = extract_emit_reply_prefix(args);
                if let Some(new_slice) = prefix.get(self.emit_reply_sent..) {
                    if !new_slice.is_empty() {
                        added.push_str(new_slice);
                        self.emit_reply_sent = prefix.len();
                    }
                }
            }
        }
        if self
            .text
            .len()
            .saturating_add(self.reasoning.len())
            .saturating_add(self.tool_bytes)
            > MAX_LLM_RESPONSE_BYTES
        {
            self.failure = Some("모델 응답 크기가 허용 상한(512 KiB)을 초과했습니다.".to_string());
        }
        (self.failure.is_none() && !added.is_empty()).then_some(added)
    }

    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    pub fn finish(self, raw_body: &str) -> Result<(String, String), EasyOfficeError> {
        let (content, reasoning, _) = self.finish_with_usage(raw_body)?;
        Ok((content, reasoning))
    }

    pub fn finish_with_usage(
        self,
        raw_body: &str,
    ) -> Result<(String, String, Option<TokenUsage>), EasyOfficeError> {
        if let Some(message) = self.failure {
            return Err(EasyOfficeError::InvalidArgument(message));
        }
        if self.events == 0 {
            return extract_provider_completion_with_usage(self.kind, raw_body);
        }
        let invocations: Vec<NativeToolInvocation> = self
            .tool_slots
            .into_iter()
            .filter(|(_, name, _)| !name.is_empty())
            .map(|(_, name, args)| NativeToolInvocation {
                name,
                arguments_json: if args.trim().is_empty() {
                    "{}".to_string()
                } else {
                    args
                },
            })
            .collect();
        if self.text.is_empty() && invocations.is_empty() {
            return Err(EasyOfficeError::InvalidArgument(
                "모델 응답에서 메시지 본문을 찾을 수 없습니다.".to_string(),
            ));
        }
        let content = synthesize_completion_from_tools(self.text, &invocations);
        Ok((
            content,
            self.reasoning,
            self.has_usage.then_some(self.usage),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_all(kind: ProviderKind, parts: &[&str]) -> (StreamAssembler, Vec<String>) {
        let mut asm = StreamAssembler::new(kind);
        let shown = parts
            .iter()
            .filter_map(|p| asm.feed(p.as_bytes()))
            .collect();
        (asm, shown)
    }

    #[test]
    fn openai_deltas_arrive_in_order_and_the_reasoning_stays_apart() {
        let (asm, shown) = feed_all(
            ProviderKind::OpenAi,
            &[
                "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"생각\"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{\"content\":\"안\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"녕\"}}]}\n\n",
                "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
            ],
        );
        assert_eq!(shown, ["안녕"]);
        assert_eq!(
            asm.finish("raw").unwrap(),
            ("안녕".to_string(), "생각".to_string())
        );
    }

    #[test]
    fn an_event_split_across_reads_is_read_once_it_is_whole() {
        let (asm, shown) = feed_all(
            ProviderKind::OpenAi,
            &[
                "data: {\"choices\":[{\"delta\":{\"con",
                "tent\":\"반쪽\"}}]}\n",
                "\n",
            ],
        );
        assert_eq!(shown, ["반쪽"]);
        assert_eq!(asm.finish("").unwrap().0, "반쪽");
    }

    #[test]
    fn anthropic_text_and_thinking_deltas() {
        let (asm, shown) = feed_all(
            ProviderKind::Anthropic,
            &[
                "event: message_start\ndata: {\"type\":\"message_start\"}\n\n",
                "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"음\"}}\n\n",
                "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"답\"}}\n\n",
                "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
            ],
        );
        assert_eq!(shown, ["답"]);
        assert_eq!(
            asm.finish("").unwrap(),
            ("답".to_string(), "음".to_string())
        );
    }

    #[test]
    fn google_parts_split_thoughts_from_text() {
        let (asm, shown) = feed_all(
            ProviderKind::Google,
            &[
                "[{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"고민\",\"thought\":true},{\"text\":\"답\"}]}}]}\n",
                ",{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"변\"}]}}]}\n]",
            ],
        );
        assert_eq!(shown, ["답", "변"]);
        assert_eq!(
            asm.finish("").unwrap(),
            ("답변".to_string(), "고민".to_string())
        );
    }

    #[test]
    fn google_compact_comma_array_without_newlines_is_parsed() {
        let (asm, shown) = feed_all(
            ProviderKind::Google,
            &[
                "[{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"첫째\"}]}}]},{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"둘째\"}]}}]}]",
            ],
        );
        assert_eq!(shown, ["첫째둘째"]);
        assert_eq!(asm.finish("").unwrap().0, "첫째둘째");
    }

    #[test]
    fn a_google_object_split_across_reads_waits_for_its_end() {
        let whole = "[{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"완성\"}]}}]}\n]";
        let bytes = whole.as_bytes();
        let cut = whole.find("완").unwrap() + 1;
        let mut asm = StreamAssembler::new(ProviderKind::Google);
        let mut shown = Vec::new();
        for part in [&bytes[..cut], &bytes[cut..]] {
            shown.extend(asm.feed(part));
        }
        assert_eq!(shown, ["완성"]);
    }

    fn google_part(text: &str) -> String {
        format!("{{\"candidates\":[{{\"content\":{{\"parts\":[{{\"text\":\"{text}\"}}]}}}}]}}")
    }

    #[test]
    fn a_google_array_split_at_a_separator_emits_each_object_once() {
        let (a, b, c) = (google_part("가"), google_part("나"), google_part("다"));
        let chunks = [format!("[{a}"), format!("\n,{b}"), format!("\n,{c}\n]")];
        let mut asm = StreamAssembler::new(ProviderKind::Google);
        let shown: Vec<String> = chunks
            .iter()
            .filter_map(|c| asm.feed(c.as_bytes()))
            .collect();
        assert_eq!(shown.concat(), "가나다", "{shown:?}");
        assert!(asm.failure().is_none());
        assert_eq!(asm.finish("").unwrap().0, "가나다");
    }

    #[test]
    fn a_google_array_cut_at_every_byte_gives_the_same_text_once() {
        let whole = format!(
            "[{}\n,{}\n,{}\n]",
            google_part("하나"),
            google_part("둘"),
            google_part("셋")
        );
        let mut asm = StreamAssembler::new(ProviderKind::Google);
        let mut shown = String::new();
        for byte in whole.as_bytes() {
            if let Some(text) = asm.feed(std::slice::from_ref(byte)) {
                shown.push_str(&text);
            }
        }
        assert_eq!(shown, "하나둘셋");
    }

    #[test]
    fn a_google_chunk_that_happens_to_end_in_a_bracket_is_not_cut() {
        let whole = format!("[{}\n]", google_part("끝"));
        let cut = whole.find("}]").unwrap() + 2;
        let (head, tail) = whole.as_bytes().split_at(cut);
        assert_eq!(head.last(), Some(&b']'));
        let mut asm = StreamAssembler::new(ProviderKind::Google);
        let mut shown = String::new();
        for part in [head, tail] {
            shown.extend(asm.feed(part));
        }
        assert_eq!(shown, "끝");
    }

    #[test]
    fn a_google_error_object_fails_the_stream() {
        let mut asm = StreamAssembler::new(ProviderKind::Google);
        assert!(asm
            .feed(b"[{\"error\":{\"message\":\"blocked\"}}\n]")
            .is_none());
        assert_eq!(asm.failure(), Some("blocked"));
    }

    #[test]
    fn a_server_that_ignores_stream_is_read_as_plain_json() {
        let (asm, shown) = feed_all(
            ProviderKind::OpenAi,
            &["{\"choices\":[{\"message\":{\"content\":\"한꺼번에\"}}]}"],
        );
        assert!(shown.is_empty());
        let raw = "{\"choices\":[{\"message\":{\"content\":\"한꺼번에\"}}]}";
        assert_eq!(asm.finish(raw).unwrap().0, "한꺼번에");
    }

    #[test]
    fn an_error_event_fails_the_stream_and_stops_reading() {
        let mut asm = StreamAssembler::new(ProviderKind::OpenAi);
        assert!(asm.feed(b"data: {\"error\":{\"message\":\"quota\"}}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"x\"}}]}\n\n").is_none());
        assert_eq!(asm.failure(), Some("quota"));
        assert!(asm.finish("").is_err());
    }

    #[test]
    fn a_stream_with_no_text_is_an_error_not_an_empty_answer() {
        let (asm, _) = feed_all(
            ProviderKind::OpenAi,
            &["data: {\"choices\":[{\"delta\":{}}]}\n\ndata: [DONE]\n\n"],
        );
        assert!(asm.finish("").is_err());
    }

    #[test]
    fn a_stream_over_the_response_limit_is_refused() {
        let mut asm = StreamAssembler::new(ProviderKind::OpenAi);
        let line = format!(
            "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{}\"}}}}]}}\n\n",
            "가".repeat(60_000)
        );
        let mut refused = false;
        for _ in 0..8 {
            asm.feed(line.as_bytes());
            refused |= asm.failure().is_some();
        }
        assert!(refused, "540 KiB of text is over the 512 KiB limit");
    }

    #[test]
    fn one_event_over_the_event_limit_fails_the_stream() {
        let mut asm = StreamAssembler::new(ProviderKind::OpenAi);
        asm.feed(format!("data: {}\n\n", "a".repeat(600 * 1024)).as_bytes());
        assert!(asm.failure().is_some());
    }

    #[test]
    fn blank_data_lines_alone_cannot_grow_an_event_without_bound() {
        let mut asm = StreamAssembler::new(ProviderKind::OpenAi);
        asm.feed("data\n".repeat(600 * 1024).as_bytes());
        assert!(asm.failure().is_some());
    }

    #[test]
    fn feeding_one_byte_at_a_time_reads_the_same_text_as_feeding_it_whole() {
        let body = "\u{FEFF}: comment\nid: 7\nretry: 42\ndata: {\"choices\":[{\"delta\":{\"content\":\"안녕\"}}]}\r\n\r\ndata: [DONE]\n\n";
        let mut whole = StreamAssembler::new(ProviderKind::OpenAi);
        whole.feed(body.as_bytes());
        let mut bytewise = StreamAssembler::new(ProviderKind::OpenAi);
        for byte in body.as_bytes() {
            bytewise.feed(std::slice::from_ref(byte));
        }
        assert_eq!(whole.finish("").unwrap().0, "안녕");
        assert_eq!(bytewise.finish("").unwrap().0, "안녕");
    }
}

#[cfg(test)]
#[path = "stream_array_tests.rs"]
mod array_tests;
