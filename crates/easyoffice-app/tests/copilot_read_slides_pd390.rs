//! SLIDE 읽기 도구 `read_slides(start, count)`(PD-390): 상시 노출(캐시 접두 유지), 응답의 도구 호출 변환, 악의적 입력 Fail-Closed.

use easyoffice_app::{CopilotLlmRequest, EasyOfficeIpcRouter, ShellWriters};
use easyoffice_formats::StrictJsonValue;
use std::sync::Mutex;

const OPENAI: &str =
    r#"{"kind":"openai","baseUrl":"https://api.openai.com/v1","model":"gpt-4o","apiKey":"sk-1"}"#;
const PLAIN_REPLY: &str =
    r#"{"choices":[{"message":{"content":"{\"reply\":\"ok\",\"operations\":[]}"}}]}"#;

fn dispatch(payload: &str, llm_response: &str) -> (String, Vec<CopilotLlmRequest>) {
    let seen = Mutex::new(Vec::new());
    let response = llm_response.to_string();
    let callback = |req: &CopilotLlmRequest| -> Result<String, String> {
        seen.lock().unwrap().push(req.clone());
        Ok(response.clone())
    };
    let writers = ShellWriters {
        llm: Some(&callback),
        ..ShellWriters::default()
    };
    let out = EasyOfficeIpcRouter::dispatch_for_owner_with_shell_writers(
        "run_smart_assistant",
        payload,
        "",
        writers,
    )
    .expect("the run succeeds");
    (out, seen.into_inner().unwrap())
}

fn slide_request(mode: &str, slides: usize, extra: &str) -> String {
    let items: Vec<String> = (0..slides)
        .map(|i| format!(r#"{{"title":"슬라이드 {i}","bullets":["항목 {i}"]}}"#))
        .collect();
    format!(
        r#"{{"action":"copilot_run","mode":"{mode}","module":"SLIDE","prompt":"다듬어 주세요",{extra}"context":{{"slide":{{"activeIndex":0,"slides":[{}]}}}},"provider":{OPENAI}}}"#,
        items.join(",")
    )
}

fn tool_reply(name: &str, arguments: &str) -> String {
    let arguments = StrictJsonValue::Str(arguments.to_string()).to_json_string();
    format!(
        r#"{{"choices":[{{"message":{{"tool_calls":[{{"id":"c1","type":"function","function":{{"name":"{name}","arguments":{arguments}}}}}]}}}}]}}"#
    )
}

/// The `toolCalls` of a run as `(name, start, count)`.
fn read_calls(out: &str) -> Vec<(String, u64, u64)> {
    let StrictJsonValue::Object(obj) = StrictJsonValue::parse(out).unwrap() else {
        panic!("an object");
    };
    StrictJsonValue::get_field(&obj, "toolCalls")
        .and_then(StrictJsonValue::as_array)
        .unwrap_or_default()
        .iter()
        .map(|call| {
            let call = call.as_object().unwrap();
            let number =
                |key| StrictJsonValue::get_field(call, key).and_then(StrictJsonValue::as_u64);
            (
                StrictJsonValue::get_str(call, "name").unwrap().to_string(),
                number("start").unwrap(),
                number("count").unwrap(),
            )
        })
        .collect()
}

fn ask_read(arguments: &str) -> Vec<(String, u64, u64)> {
    let (out, _) = dispatch(
        &slide_request("edit", 8, ""),
        &tool_reply("read_slides", arguments),
    );
    read_calls(&out)
}

#[test]
fn a_slide_chat_always_offers_read_slides_whatever_the_deck_size() {
    for slides in [1, 3, 64] {
        for mode in ["edit", "ask"] {
            let (_, requests) = dispatch(&slide_request(mode, slides, ""), PLAIN_REPLY);
            let body = &requests[0].body_json;
            assert!(
                body.contains(r#""name":"read_slides""#),
                "{mode} with {slides} slides: {body}"
            );
            assert!(
                body.contains(r#""maximum":20"#),
                "the count limit is in the schema"
            );
        }
    }
}

#[test]
fn a_selection_command_offers_no_slide_reading_and_drops_a_hallucinated_call() {
    let command = slide_request(
        "edit",
        2,
        r#""command":"summarize","selection":{"slideIndex":0,"slideField":"title"},"#,
    );
    let (_, requests) = dispatch(&command, PLAIN_REPLY);
    assert!(!requests[0].body_json.contains("read_slides"));
    let (out, _) = dispatch(
        &command,
        &tool_reply("read_slides", r#"{"start":0,"count":2}"#),
    );
    assert!(read_calls(&out).is_empty());
}

#[test]
fn the_model_reads_slides_from_a_start_for_a_count() {
    assert_eq!(
        ask_read(r#"{"start":2,"count":3}"#),
        [("read_slides".into(), 2, 3)]
    );
    assert_eq!(
        ask_read("{}"),
        [("read_slides".into(), 0, 5)],
        "no arguments read the first five"
    );
    assert_eq!(
        ask_read(r#"{"start":255,"count":1}"#),
        [("read_slides".into(), 255, 1)]
    );
}

#[test]
fn a_huge_count_is_cut_to_the_limit_and_a_start_past_the_deck_limit_is_dropped() {
    assert_eq!(
        ask_read(r#"{"start":0,"count":1000000}"#),
        [("read_slides".into(), 0, 20)]
    );
    assert_eq!(
        ask_read(r#"{"start":4,"count":18446744073709551615}"#),
        [("read_slides".into(), 4, 20)]
    );
    assert!(
        ask_read(r#"{"start":256,"count":1}"#).is_empty(),
        "start must be inside the 256 slides a deck may have"
    );
    assert!(ask_read(r#"{"start":9999999999,"count":1}"#).is_empty());
}

#[test]
fn a_negative_fractional_or_text_argument_drops_the_call_instead_of_reading_from_the_first_slide() {
    for arguments in [
        r#"{"start":-1,"count":3}"#,
        r#"{"start":0,"count":-3}"#,
        r#"{"start":1.5,"count":3}"#,
        r#"{"start":"2","count":3}"#,
        r#"{"start":0,"count":0}"#,
        r#"{"start":0,"count":"many"}"#,
    ] {
        assert!(ask_read(arguments).is_empty(), "{arguments}");
    }
}

#[test]
fn only_a_slide_chat_takes_read_slides() {
    let doc = format!(
        r#"{{"action":"copilot_run","mode":"edit","module":"DOC","prompt":"다듬어 주세요","context":{{"doc":{{"paragraphs":["가"]}}}},"provider":{OPENAI}}}"#
    );
    let (out, requests) = dispatch(&doc, &tool_reply("read_slides", r#"{"start":0,"count":2}"#));
    assert!(!requests[0].body_json.contains("read_slides"));
    assert!(read_calls(&out).is_empty());
}
