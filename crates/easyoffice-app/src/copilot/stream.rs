//! 참조 구현: continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., 커밋 5522c6f4, see THIRD_PARTY_NOTICES.md)
//! `packages/fetch/src/stream.ts:54-99`(`parseDataLine`·`parseSseLine`: `data:` 한 줄의 JSON, `error` 필드는 스트림 오류, `[DONE]`),
//! `core/llm/llms/OpenAI.ts:686`(`choices[0].delta.content`), `core/llm/llms/Anthropic.ts:346-360`(`content_block_delta`의 `text_delta`·`thinking_delta`),
//! `core/llm/llms/Gemini.ts:369-455`(`processGeminiResponse`: `streamGenerateContent`가 돌려주는 JSON 배열 조각을 `\n,`로 나눠 읽는다)를 옮겼다.
//! SSE 줄 해석은 `sse.rs`가 한다.
//!
//! 스트림은 화면에 보여 줄 글을 먼저 흘려 보내는 용도다. 최종 본문은 전송이 끝난 뒤 한꺼번에 검증하므로(`llm_client`),
//! 스트림이 끊기거나 형식이 틀려도 검증을 건너뛰지 않는다.

use easyoffice_formats::{EasyOfficeError, StrictJsonValue};

use super::providers::extract_provider_completion;
use super::sse::{MessageEvent, SseDecoder};
use super::types::{ProviderKind, MAX_LLM_RESPONSE_BYTES};

/// 이벤트 하나가 더한 것.
#[derive(Debug, Default, PartialEq, Eq)]
struct Delta {
    text: String,
    reasoning: String,
    error: Option<String>,
    done: bool,
}

/// 모델 본문 이벤트(`data:` JSON)를 제공자별 델타로 바꾼다. JSON이 아니거나 모르는 모양이면 `None`이다.
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
    match kind {
        ProviderKind::Anthropic => read_anthropic(root, event, &mut delta),
        ProviderKind::Google => read_google(root, &mut delta),
        _ => read_openai(root, &mut delta),
    }
    Some(delta)
}

// 옮김[continue-delta] 시작
fn read_openai(root: &[(String, StrictJsonValue)], delta: &mut Delta) {
    let Some(choice) = StrictJsonValue::get_field(root, "choices")
        .and_then(StrictJsonValue::as_array)
        .and_then(|c| c.first())
        .and_then(StrictJsonValue::as_object)
    else {
        return;
    };
    if let Some(d) =
        StrictJsonValue::get_field(choice, "delta").and_then(StrictJsonValue::as_object)
    {
        delta
            .text
            .push_str(StrictJsonValue::get_str(d, "content").unwrap_or(""));
        delta.reasoning.push_str(
            StrictJsonValue::get_str(d, "reasoning_content")
                .or_else(|| StrictJsonValue::get_str(d, "reasoning"))
                .unwrap_or(""),
        );
    }
    delta.done = StrictJsonValue::get_str(choice, "finish_reason").is_some();
}

fn read_anthropic(root: &[(String, StrictJsonValue)], event: &MessageEvent, delta: &mut Delta) {
    let kind = StrictJsonValue::get_str(root, "type").unwrap_or(event.event.as_str());
    match kind {
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
    for part in parts
        .into_iter()
        .flatten()
        .filter_map(StrictJsonValue::as_object)
    {
        let Some(text) = StrictJsonValue::get_str(part, "text") else {
            continue;
        };
        let thought = StrictJsonValue::get_field(part, "thought")
            .and_then(StrictJsonValue::as_bool)
            .unwrap_or(false);
        if thought {
            delta.reasoning.push_str(text);
        } else {
            delta.text.push_str(text);
        }
    }
}
// 옮김[continue-delta] 끝

/// SSE 조각에서 읽히는 이벤트마다 델타를 낸다. 모르는 모양의 이벤트는 건너뛴다. 오류 이벤트나 이벤트 크기 상한을 만나면 거기서 멈춘다.
fn read_sse_deltas(
    kind: ProviderKind,
    decoder: &mut SseDecoder,
    mut chunk: &[u8],
) -> Result<Vec<Delta>, String> {
    let mut deltas = Vec::new();
    while let Some(event) = decoder.next(&mut chunk) {
        let event = event.map_err(|err| err.to_string())?;
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

/// `error` 필드의 사유. 객체면 `message`, 아니면 문자열 그대로(원본 `parseDataLine`).
fn error_message(err: &StrictJsonValue) -> String {
    err.as_object()
        .and_then(|o| StrictJsonValue::get_str(o, "message"))
        .or_else(|| err.as_str())
        .unwrap_or("모델 서버가 스트림 오류를 알렸습니다.")
        .to_string()
}

// 옮김[continue-gemini-array] 시작
/// `streamGenerateContent`의 본문은 `[{..}\n,{..}\n,{..}]` 모양의 JSON 배열이 조각으로 온다. 배열 괄호를 걷어 내고 `\n,`로 나눠
/// 읽히는 조각을 델타로 내며, 아직 덜 온 마지막 조각은 다음 조각을 기다린다.
#[derive(Default)]
struct JsonArrayReader {
    buffer: Vec<u8>,
}

impl JsonArrayReader {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<Delta>, &'static str> {
        self.buffer.extend_from_slice(chunk);
        if self.buffer.len() > MAX_LLM_RESPONSE_BYTES {
            return Err("모델 응답 크기가 허용 상한(512 KiB)을 초과했습니다.");
        }
        if self.buffer.first() == Some(&b'[') {
            self.buffer.remove(0);
        }
        if self.buffer.last() == Some(&b']') {
            self.buffer.pop();
        }
        if self.buffer.first() == Some(&b',') {
            self.buffer.remove(0);
        }
        let parts: Vec<&[u8]> = split_on(&self.buffer, b"\n,");
        let mut deltas = Vec::new();
        let mut found_incomplete = false;
        for part in &parts {
            let parsed = std::str::from_utf8(part)
                .ok()
                .and_then(|text| StrictJsonValue::parse(text).ok());
            let Some(parsed) = parsed.as_ref().and_then(StrictJsonValue::as_object) else {
                found_incomplete = true;
                continue;
            };
            let mut delta = Delta::default();
            if let Some(err) = StrictJsonValue::get_field(parsed, "error") {
                delta.error = Some(error_message(err));
            } else {
                read_google(parsed, &mut delta);
            }
            deltas.push(delta);
        }
        self.buffer = match parts.last() {
            Some(last) if found_incomplete => last.to_vec(),
            _ => Vec::new(),
        };
        Ok(deltas)
    }
}

fn split_on<'a>(haystack: &'a [u8], needle: &[u8]) -> Vec<&'a [u8]> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut at = 0;
    while at + needle.len() <= haystack.len() {
        if &haystack[at..at + needle.len()] == needle {
            parts.push(&haystack[start..at]);
            at += needle.len();
            start = at;
        } else {
            at += 1;
        }
    }
    parts.push(&haystack[start..]);
    parts
}
// 옮김[continue-gemini-array] 끝

enum Wire {
    Sse(SseDecoder),
    JsonArray(JsonArrayReader),
}

/// 응답 본문 조각을 먹여 모델 글을 조립한다.
pub struct StreamAssembler {
    kind: ProviderKind,
    wire: Wire,
    text: String,
    reasoning: String,
    events: u32,
    failure: Option<String>,
}

impl StreamAssembler {
    pub fn new(kind: ProviderKind) -> Self {
        Self {
            kind,
            wire: if kind == ProviderKind::Google {
                Wire::JsonArray(JsonArrayReader::default())
            } else {
                Wire::Sse(SseDecoder::new())
            },
            text: String::new(),
            reasoning: String::new(),
            events: 0,
            failure: None,
        }
    }

    /// 조각을 먹이고 이번 조각이 더한 모델 글을 돌려준다. 실패하면 [`failure`](Self::failure)에 사유가 남고 `None`이다.
    pub fn feed(&mut self, chunk: &[u8]) -> Option<String> {
        let deltas = match &mut self.wire {
            Wire::Sse(decoder) => read_sse_deltas(self.kind, decoder, chunk),
            Wire::JsonArray(reader) => reader.push(chunk).map_err(str::to_string),
        };
        let mut added = String::new();
        match deltas {
            Ok(deltas) => {
                for delta in deltas {
                    self.events += 1;
                    if let Some(err) = delta.error {
                        self.failure = Some(err);
                        break;
                    }
                    self.reasoning.push_str(&delta.reasoning);
                    self.text.push_str(&delta.text);
                    added.push_str(&delta.text);
                }
            }
            Err(message) => self.failure = Some(message),
        }
        if self.text.len() + self.reasoning.len() > MAX_LLM_RESPONSE_BYTES {
            self.failure = Some("모델 응답 크기가 허용 상한(512 KiB)을 초과했습니다.".to_string());
        }
        (self.failure.is_none() && !added.is_empty()).then_some(added)
    }

    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    /// 전송이 끝난 뒤의 본문과 생각. 서버가 스트림 대신 일반 JSON을 돌려줬으면(스트림을 무시하는 서버) 그 본문에서 읽는다.
    pub fn finish(self, raw_body: &str) -> Result<(String, String), EasyOfficeError> {
        if let Some(message) = self.failure {
            return Err(EasyOfficeError::InvalidArgument(message));
        }
        if self.events == 0 {
            return extract_provider_completion(self.kind, raw_body);
        }
        if self.text.is_empty() {
            return Err(EasyOfficeError::InvalidArgument(
                "모델 응답에서 메시지 본문을 찾을 수 없습니다.".to_string(),
            ));
        }
        Ok((self.text, self.reasoning))
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
    fn a_google_object_split_across_reads_waits_for_its_end() {
        let whole = "[{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"완성\"}]}}]}\n]";
        let bytes = whole.as_bytes();
        // 멀티바이트 글자 한가운데서도 자른다.
        let cut = whole.find("완").unwrap() + 1;
        let mut asm = StreamAssembler::new(ProviderKind::Google);
        let mut shown = Vec::new();
        for part in [&bytes[..cut], &bytes[cut..]] {
            shown.extend(asm.feed(part));
        }
        assert_eq!(shown, ["완성"]);
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
}
