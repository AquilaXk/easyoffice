//! PD-268: 모델 응답 스트림. 본문 조각이 오는 대로 화면 통로로 흘리고, 연산은 전송이 끝난 뒤 한꺼번에 검증한다.

use easyoffice_app::copilot::llm_client::send_local_http_streaming;
use easyoffice_app::{
    ChunkSink, CopilotLlmRequest, CopilotStreamEvent, EasyOfficeIpcRouter, ShellWriters,
};
use easyoffice_formats::{EasyOfficeError, StrictJsonValue};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::Duration;

const REMOTE: &str = r#""provider": { "kind": "remote_https", "baseUrl": "https://93.184.216.34/v1", "model": "gpt-5", "apiKey": "sk-test" }"#;
const MODEL_JSON: &str = r##"{"reply":"문단을 추가했습니다.","reasoning":"","operations":[{"kind":"doc.append","blocks":[{"text":"추가 문단"}]},{"kind":"bogus.op"}],"locations":[]}"##;

/// 요청 번호는 시험마다 다르게 준다(진행 중인 요청 번호는 한 번에 하나만 등록된다).
fn payload(id: &str, extra: &str) -> String {
    format!(
        r#"{{"action":"copilot_run","mode":"edit","module":"DOC","prompt":"문단을 추가해 주세요","requestId":"{id}",{REMOTE}{extra}}}"#
    )
}

/// OpenAI 호환 SSE 본문: `content`를 `step`자씩 델타로 나눈다.
fn openai_sse(content: &str, step: usize) -> Vec<String> {
    let chars: Vec<char> = content.chars().collect();
    let mut events: Vec<String> = chars
        .chunks(step)
        .map(|piece| {
            let text: String = piece.iter().collect();
            let json = StrictJsonValue::Str(text).to_json_string();
            format!("data: {{\"choices\":[{{\"delta\":{{\"content\":{json}}}}}]}}\n\n")
        })
        .collect();
    events.push("data: [DONE]\n\n".to_string());
    events
}

type Seen = Mutex<Vec<(String, u32, String)>>;

fn run_streaming(
    payload: &str,
    pieces: Vec<String>,
    seen: &Seen,
    bodies: &Mutex<Vec<String>>,
    with_events: bool,
) -> Result<String, EasyOfficeError> {
    let stream = |req: &CopilotLlmRequest, sink: &mut ChunkSink<'_>| -> Result<String, String> {
        bodies.lock().unwrap().push(req.body_json.clone());
        assert!(req.stream, "the request is marked as a stream");
        let mut whole = String::new();
        for piece in &pieces {
            whole.push_str(piece);
            if !sink(piece.as_bytes()) {
                return Err("멈춤".to_string());
            }
        }
        Ok(whole)
    };
    let plain = |req: &CopilotLlmRequest| -> Result<String, String> {
        bodies.lock().unwrap().push(req.body_json.clone());
        assert!(!req.stream);
        Ok(r#"{"choices":[{"message":{"content":"{\"reply\":\"한꺼번에\"}"}}]}"#.to_string())
    };
    let events = |event: &CopilotStreamEvent<'_>| {
        seen.lock().unwrap().push((
            event.request_id.to_string(),
            event.seq,
            event.text.to_string(),
        ));
    };
    let writers = ShellWriters {
        llm: Some(&plain),
        llm_stream: Some(&stream),
        copilot_events: with_events.then_some(&events as _),
        ..ShellWriters::default()
    };
    EasyOfficeIpcRouter::dispatch_for_owner_with_shell_writers(
        "run_smart_assistant",
        payload,
        "",
        writers,
    )
}

fn obj(raw: &str) -> Vec<(String, StrictJsonValue)> {
    StrictJsonValue::parse(raw)
        .unwrap()
        .as_object()
        .unwrap()
        .to_vec()
}

#[test]
fn text_pieces_reach_the_host_in_order_and_the_answer_is_validated_at_the_end() {
    let (seen, bodies) = (Mutex::new(Vec::new()), Mutex::new(Vec::new()));
    let pieces = openai_sse(MODEL_JSON, 9);
    let res = run_streaming(
        &payload("cp-s-order", r#","stream":true"#),
        pieces,
        &seen,
        &bodies,
        true,
    )
    .unwrap();
    let o = obj(&res);
    assert_eq!(
        StrictJsonValue::get_str(&o, "reply"),
        Some("문단을 추가했습니다.")
    );
    let ops = StrictJsonValue::get_field(&o, "operations")
        .and_then(StrictJsonValue::as_array)
        .unwrap();
    assert_eq!(
        ops.len(),
        1,
        "the operation with an unknown kind is dropped after the stream ended"
    );
    assert_eq!(
        StrictJsonValue::get_field(&o, "droppedOperations").and_then(StrictJsonValue::as_u64),
        Some(1)
    );

    let seen = seen.into_inner().unwrap();
    assert!(
        seen.len() > 3,
        "the text came in several pieces: {}",
        seen.len()
    );
    for (i, (id, seq, _)) in seen.iter().enumerate() {
        assert_eq!(id, "cp-s-order");
        assert_eq!(*seq as usize, i, "numbers count up from 0 without a gap");
    }
    let joined: String = seen.iter().map(|(_, _, t)| t.as_str()).collect();
    assert_eq!(
        joined, MODEL_JSON,
        "the pieces add up to what the model wrote"
    );
    let body = &bodies.lock().unwrap()[0];
    assert!(body.contains(r#""stream":true"#), "{body}");
}

#[test]
fn without_a_stream_request_an_event_path_or_a_request_id_the_answer_comes_whole() {
    for (index, (extra, with_events, request_id)) in [
        ("", true, true),
        (r#","stream":false"#, true, true),
        (r#","stream":true"#, false, true),
        (r#","stream":true"#, true, false),
    ]
    .into_iter()
    .enumerate()
    {
        let (seen, bodies) = (Mutex::new(Vec::new()), Mutex::new(Vec::new()));
        let mut pl = payload(&format!("cp-s-whole-{index}"), extra);
        if !request_id {
            pl = pl.replace(&format!(r#""requestId":"cp-s-whole-{index}","#), "");
        }
        let res =
            run_streaming(&pl, openai_sse(MODEL_JSON, 9), &seen, &bodies, with_events).unwrap();
        assert_eq!(
            StrictJsonValue::get_str(&obj(&res), "reply"),
            Some("한꺼번에"),
            "{extra} events={with_events} id={request_id}"
        );
        assert!(seen.lock().unwrap().is_empty());
        assert!(!bodies.lock().unwrap()[0].contains(r#""stream":true"#));
    }
}

#[test]
fn a_stream_error_event_fails_the_request_and_nothing_is_applied() {
    let (seen, bodies) = (Mutex::new(Vec::new()), Mutex::new(Vec::new()));
    let pieces = vec![
        "data: {\"choices\":[{\"delta\":{\"content\":\"{\\\"reply\\\":\\\"앞\"}}]}\n\n".to_string(),
        "data: {\"error\":{\"message\":\"한도 초과\"}}\n\n".to_string(),
        "data: {\"choices\":[{\"delta\":{\"content\":\"뒤\"}}]}\n\n".to_string(),
    ];
    let res = run_streaming(
        &payload("cp-s-error", r#","stream":true"#),
        pieces,
        &seen,
        &bodies,
        true,
    );
    let Err(EasyOfficeError::InvalidArgument(msg)) = res else {
        panic!("{res:?}");
    };
    assert_eq!(msg, "한도 초과");
    assert_eq!(
        seen.lock().unwrap().len(),
        1,
        "only the text before the error was shown"
    );
}

#[test]
fn a_server_that_ignores_the_stream_flag_is_read_as_one_json_body() {
    let (seen, bodies) = (Mutex::new(Vec::new()), Mutex::new(Vec::new()));
    let whole = StrictJsonValue::Object(vec![(
        "choices".to_string(),
        StrictJsonValue::Array(vec![StrictJsonValue::Object(vec![(
            "message".to_string(),
            StrictJsonValue::Object(vec![(
                "content".to_string(),
                StrictJsonValue::Str(MODEL_JSON.to_string()),
            )]),
        )])]),
    )])
    .to_json_string();
    let res = run_streaming(
        &payload("cp-s-multi", r#","stream":true"#),
        vec![whole],
        &seen,
        &bodies,
        true,
    )
    .unwrap();
    assert_eq!(
        StrictJsonValue::get_str(&obj(&res), "reply"),
        Some("문단을 추가했습니다.")
    );
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn ask_mode_keeps_streaming_text_but_drops_every_operation() {
    let (seen, bodies) = (Mutex::new(Vec::new()), Mutex::new(Vec::new()));
    let pl =
        payload("cp-s-ask", r#","stream":true"#).replace(r#""mode":"edit""#, r#""mode":"ask""#);
    let res = run_streaming(&pl, openai_sse(MODEL_JSON, 20), &seen, &bodies, true).unwrap();
    let o = obj(&res);
    assert!(StrictJsonValue::get_field(&o, "operations")
        .and_then(StrictJsonValue::as_array)
        .unwrap()
        .is_empty());
    assert!(!seen.lock().unwrap().is_empty());
}

#[test]
fn anthropic_and_google_streams_use_their_own_body_and_address() {
    let anthropic = payload("cp-s-claude", r#","stream":true"#).replace(
        REMOTE,
        r#""provider": { "kind": "anthropic", "baseUrl": "https://93.184.216.34", "model": "claude-x", "apiKey": "k" }"#,
    );
    let (seen, bodies) = (Mutex::new(Vec::new()), Mutex::new(Vec::new()));
    let text_event = |t: &str| {
        let json = StrictJsonValue::Str(t.to_string()).to_json_string();
        format!("event: content_block_delta\ndata: {{\"type\":\"content_block_delta\",\"delta\":{{\"type\":\"text_delta\",\"text\":{json}}}}}\n\n")
    };
    let pieces = vec![
        text_event(r#"{"reply":"안"#),
        text_event(r#"녕"}"#),
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".to_string(),
    ];
    let res = run_streaming(&anthropic, pieces, &seen, &bodies, true).unwrap();
    assert_eq!(StrictJsonValue::get_str(&obj(&res), "reply"), Some("안녕"));
    assert!(bodies.lock().unwrap()[0].contains(r#""stream":true"#));

    let google = payload("cp-s-gemini", r#","stream":true"#).replace(
        REMOTE,
        r#""provider": { "kind": "google", "baseUrl": "https://93.184.216.34", "model": "gemini-x", "apiKey": "k" }"#,
    );
    let urls = Mutex::new(Vec::new());
    let stream = |req: &CopilotLlmRequest, sink: &mut ChunkSink<'_>| -> Result<String, String> {
        urls.lock().unwrap().push(req.url.clone());
        let part = "[{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"{\\\"reply\\\":\\\"제미니\\\"}\"}]}}]}\n]";
        sink(part.as_bytes());
        Ok(part.to_string())
    };
    let events = |_: &CopilotStreamEvent<'_>| {};
    let writers = ShellWriters {
        llm_stream: Some(&stream),
        copilot_events: Some(&events),
        ..ShellWriters::default()
    };
    let res = EasyOfficeIpcRouter::dispatch_for_owner_with_shell_writers(
        "run_smart_assistant",
        &google,
        "",
        writers,
    )
    .unwrap();
    assert_eq!(
        StrictJsonValue::get_str(&obj(&res), "reply"),
        Some("제미니")
    );
    let url = urls.lock().unwrap()[0].clone();
    assert!(
        url.ends_with("/v1beta/models/gemini-x:streamGenerateContent"),
        "{url}"
    );
}

#[test]
fn the_loopback_client_hands_over_chunked_pieces_while_the_server_is_still_writing() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (to_server, from_test) = mpsc::channel::<()>();
    let server = thread::spawn(move || {
        let (mut conn, _) = listener.accept().unwrap();
        let mut buf = [0u8; 8192];
        let mut request = Vec::new();
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = conn.read(&mut buf).unwrap();
            request.extend_from_slice(&buf[..n]);
        }
        let request = String::from_utf8_lossy(&request).to_ascii_lowercase();
        assert!(request.contains("accept: text/event-stream"), "{request}");
        let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
        conn.write_all(head.as_bytes()).unwrap();
        let events = openai_sse(r#"{"reply":"첫째 둘째 셋째"}"#, 8);
        let (first, rest) = events.split_at(2);
        let chunk = |s: &str| format!("{:x}\r\n{s}\r\n", s.len());
        for piece in first {
            conn.write_all(chunk(piece).as_bytes()).unwrap();
        }
        conn.flush().unwrap();
        // 화면 통로가 첫 조각을 받았다고 알릴 때까지 나머지를 보내지 않는다.
        from_test
            .recv_timeout(Duration::from_secs(10))
            .expect("the first pieces arrived while the server waited");
        for piece in rest {
            conn.write_all(chunk(piece).as_bytes()).unwrap();
        }
        conn.write_all(b"0\r\n\r\n").unwrap();
    });
    let notified = Mutex::new(false);
    let seen: Seen = Mutex::new(Vec::new());
    let events = |event: &CopilotStreamEvent<'_>| {
        seen.lock().unwrap().push((
            event.request_id.to_string(),
            event.seq,
            event.text.to_string(),
        ));
        let mut once = notified.lock().unwrap();
        if !*once {
            *once = true;
            to_server.send(()).unwrap();
        }
    };
    let stream = |req: &CopilotLlmRequest, sink: &mut ChunkSink<'_>| -> Result<String, String> {
        send_local_http_streaming(req, Some(sink)).map_err(|e| e.to_string())
    };
    let writers = ShellWriters {
        llm_stream: Some(&stream),
        copilot_events: Some(&events),
        ..ShellWriters::default()
    };
    let local = payload("cp-s-local", r#","stream":true"#).replace(
        REMOTE,
        &format!(r#""provider": {{ "kind": "local_http", "baseUrl": "http://127.0.0.1:{port}", "model": "m" }}"#),
    );
    let res = EasyOfficeIpcRouter::dispatch_for_owner_with_shell_writers(
        "run_smart_assistant",
        &local,
        "",
        writers,
    )
    .unwrap();
    server.join().unwrap();
    assert_eq!(
        StrictJsonValue::get_str(&obj(&res), "reply"),
        Some("첫째 둘째 셋째")
    );
    let joined: String = seen
        .lock()
        .unwrap()
        .iter()
        .map(|(_, _, t)| t.clone())
        .collect();
    assert_eq!(joined, r#"{"reply":"첫째 둘째 셋째"}"#);
}

#[test]
fn a_hostile_stream_over_the_response_limit_is_refused() {
    let (seen, bodies) = (Mutex::new(Vec::new()), Mutex::new(Vec::new()));
    let big = "가".repeat(60_000);
    let pieces: Vec<String> = (0..8)
        .map(|_| format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"{big}\"}}}}]}}\n\n"))
        .collect();
    let res = run_streaming(
        &payload("cp-s-hostile", r#","stream":true"#),
        pieces,
        &seen,
        &bodies,
        true,
    );
    assert!(
        matches!(res, Err(EasyOfficeError::InvalidArgument(_))),
        "{res:?}"
    );
}

/// 청크 크기 줄이 `usize` 끝까지 가는 응답을 주는 서버 하나로 스트림 요청을 돌린다.
fn run_against_chunk_size(size_line: &'static str, id: &str) -> Result<String, EasyOfficeError> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (mut conn, _) = listener.accept().unwrap();
        let mut buf = [0u8; 8192];
        let mut request = Vec::new();
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = conn.read(&mut buf).unwrap();
            request.extend_from_slice(&buf[..n]);
        }
        let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
        let _ = conn.write_all(format!("{head}{size_line}\r\nxx").as_bytes());
        thread::sleep(Duration::from_millis(100));
    });
    let events = |_: &CopilotStreamEvent<'_>| {};
    let stream = |req: &CopilotLlmRequest, sink: &mut ChunkSink<'_>| -> Result<String, String> {
        send_local_http_streaming(req, Some(sink)).map_err(|e| e.to_string())
    };
    let writers = ShellWriters {
        llm_stream: Some(&stream),
        copilot_events: Some(&events),
        ..ShellWriters::default()
    };
    let local = payload(id, r#","stream":true"#).replace(
        REMOTE,
        &format!(r#""provider": {{ "kind": "local_http", "baseUrl": "http://127.0.0.1:{port}", "model": "m" }}"#),
    );
    let res = EasyOfficeIpcRouter::dispatch_for_owner_with_shell_writers(
        "run_smart_assistant",
        &local,
        "",
        writers,
    );
    server.join().unwrap();
    res
}

#[test]
fn a_chunk_size_at_the_end_of_usize_is_refused_without_overflowing() {
    for (i, line) in [
        "ffffffffffffffff",
        "fffffffffffffffe",
        "7fffffffffffffff",
        "80000",
    ]
    .into_iter()
    .enumerate()
    {
        let res = run_against_chunk_size(line, &format!("cp-s-chunk-{i}"));
        let Err(EasyOfficeError::ResourceBudgetExceeded(_) | EasyOfficeError::InvalidArgument(_)) =
            res
        else {
            panic!("{line}: {res:?}");
        };
    }
    // 상한(512 KiB)을 넘는 크기는 예산 초과로 거부한다.
    let res = run_against_chunk_size("100000", "cp-s-chunk-big");
    let Err(EasyOfficeError::InvalidArgument(message)) = res else {
        panic!("{res:?}");
    };
    assert!(message.contains("512 KiB"), "{message}");
}
