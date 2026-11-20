//! PD-268: 선택 영역을 먼저 싣고 남은 예산으로 앞뒤 문맥을 채우는 문맥 창.

use easyoffice_app::{EasyOfficeIpcRouter, ShellWriters};
use easyoffice_formats::{EasyOfficeError, StrictJsonValue};
use std::sync::Mutex;

const REMOTE: &str = r#""provider": { "kind": "remote_https", "baseUrl": "https://93.184.216.34/v1", "model": "gpt-5", "apiKey": "sk-test" }"#;

fn completion_json(content: &str) -> String {
    StrictJsonValue::Object(vec![(
        "choices".to_string(),
        StrictJsonValue::Array(vec![StrictJsonValue::Object(vec![(
            "message".to_string(),
            StrictJsonValue::Object(vec![(
                "content".to_string(),
                StrictJsonValue::Str(content.to_string()),
            )]),
        )])]),
    )])
    .to_json_string()
}

fn run(payload: &str) -> (Result<String, EasyOfficeError>, Vec<String>) {
    let bodies = Mutex::new(Vec::new());
    let reply = completion_json(r#"{"reply":"x"}"#);
    let callback = |req: &easyoffice_app::CopilotLlmRequest| -> Result<String, String> {
        bodies.lock().unwrap().push(req.body_json.clone());
        Ok(reply.clone())
    };
    let writers = ShellWriters {
        llm: Some(&callback),
        ..ShellWriters::default()
    };
    let result = EasyOfficeIpcRouter::dispatch_for_owner_with_shell_writers(
        "run_smart_assistant",
        payload,
        "",
        writers,
    );
    (result, bodies.into_inner().unwrap())
}

/// 문단 `count`개(각 `chars`자, 번호 글자로 구분)를 가진 문서에서 `target` 문단을 "다시 쓰기"로 요청한다.
fn doc_command(count: usize, chars: usize, start: usize, end: usize) -> String {
    let paragraphs: Vec<String> = (0..count)
        .map(|i| format!(r#""P{i:03}{}""#, "가".repeat(chars)))
        .collect();
    format!(
        r#"{{"action":"copilot_run","mode":"edit","module":"DOC","command":"rewrite","prompt":"다시 쓰기","selection":{{"paragraphIndex":{start},"paragraphEnd":{end}}},"context":{{"doc":{{"paragraphs":[{}]}}}},{REMOTE}}}"#,
        paragraphs.join(",")
    )
}

fn user_content(body: &str) -> String {
    let parsed = StrictJsonValue::parse(body).unwrap();
    let messages = StrictJsonValue::get_field(parsed.as_object().unwrap(), "messages")
        .and_then(StrictJsonValue::as_array)
        .unwrap()
        .to_vec();
    let last = messages.last().unwrap().as_object().unwrap();
    StrictJsonValue::get_str(last, "content")
        .unwrap()
        .to_string()
}

fn has_paragraph(content: &str, index: usize) -> bool {
    content.contains(&format!("  [{index}] P{index:03}"))
}

#[test]
fn small_selection_after_long_paragraphs_is_sent_with_the_nearest_context() {
    // 앞 문단 20개가 각 3 KiB라 문맥 24 KiB를 앞쪽만으로도 넘는다. 전에는 선택에 닿기 전에 예산이 바닥나 거부됐다.
    let (res, bodies) = run(&doc_command(30, 1_000, 25, 25));
    res.unwrap();
    let content = user_content(&bodies[0]);
    assert!(
        has_paragraph(&content, 25),
        "the selection is in the context"
    );
    assert!(
        has_paragraph(&content, 24),
        "the paragraph before it is kept"
    );
    assert!(
        has_paragraph(&content, 26),
        "the paragraph after it is kept"
    );
    assert!(
        !has_paragraph(&content, 0),
        "the far start is cut from the top"
    );
    assert!(
        content.contains("[앞 "),
        "the cut front is reported: {content}"
    );
    assert!(content.contains("[이하 "), "the cut end is reported");
    assert!(
        content.len() < 24 * 1024 + 2_048,
        "the context stays inside its limit"
    );
}

#[test]
fn the_budget_left_after_the_selection_is_shared_by_size() {
    // 앞쪽이 크고 뒤쪽이 작으면 줄이는 몫도 앞쪽이 더 크다(앞쪽은 위에서부터, 뒤쪽은 아래에서부터 뺀다).
    let mut paragraphs: Vec<String> = (0..40)
        .map(|i| format!(r#""P{i:03}{}""#, "가".repeat(900)))
        .collect();
    for (i, p) in paragraphs.iter_mut().enumerate().skip(21) {
        *p = format!(r#""P{i:03}짧음""#);
    }
    let payload = format!(
        r#"{{"action":"copilot_run","mode":"edit","module":"DOC","command":"rewrite","prompt":"x","selection":{{"paragraphIndex":20}},"context":{{"doc":{{"paragraphs":[{}]}}}},{REMOTE}}}"#,
        paragraphs.join(",")
    );
    let (res, bodies) = run(&payload);
    res.unwrap();
    let content = user_content(&bodies[0]);
    assert!(has_paragraph(&content, 20));
    assert!(
        has_paragraph(&content, 21),
        "the nearest line of the small tail stays"
    );
    assert!(
        has_paragraph(&content, 19),
        "the nearest line of the large head stays"
    );
    assert!(!has_paragraph(&content, 0));
}

#[test]
fn a_selection_that_does_not_fit_alone_is_still_refused() {
    let (res, bodies) = run(&doc_command(4, 3_000, 0, 3));
    let Err(EasyOfficeError::InvalidArgument(msg)) = res else {
        panic!("{res:?}");
    };
    assert_eq!(msg, "선택한 범위가 너무 커서 처리할 수 없습니다.");
    assert!(bodies.is_empty());
}

#[test]
fn a_selection_past_the_first_window_is_found_in_a_long_document() {
    let (res, bodies) = run(&doc_command(240, 40, 230, 231));
    res.unwrap();
    let content = user_content(&bodies[0]);
    assert!(has_paragraph(&content, 230) && has_paragraph(&content, 231));
}
