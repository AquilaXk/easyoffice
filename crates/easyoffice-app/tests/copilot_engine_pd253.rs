use easyoffice_app::{EasyOfficeIpcRouter, ShellWriters};
use easyoffice_formats::{EasyOfficeError, StrictJsonValue};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

fn parse_obj(raw: &str) -> Vec<(String, StrictJsonValue)> {
    StrictJsonValue::parse(raw)
        .expect("valid JSON response")
        .as_object()
        .expect("root JSON object")
        .to_vec()
}

fn spawn_mock_llm_server(
    raw_http_response: String,
) -> (u16, Arc<AtomicBool>, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop);
    let handle = thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let mut captured_req = String::new();
        for _ in 0..400 {
            if stop_flag.load(Ordering::Acquire) {
                break;
            }
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 4096];
                    loop {
                        match stream.read(&mut chunk) {
                            Ok(0) => break,
                            Ok(n) => {
                                buf.extend_from_slice(&chunk[..n]);
                                if let Some(hdr_end) = buf.windows(4).position(|w| w == b"\r\n\r\n")
                                {
                                    let hdr_str = String::from_utf8_lossy(&buf[..hdr_end]);
                                    let content_len = hdr_str
                                        .lines()
                                        .find_map(|l| {
                                            let (k, v) = l.split_once(':')?;
                                            if k.trim().eq_ignore_ascii_case("content-length") {
                                                v.trim().parse::<usize>().ok()
                                            } else {
                                                None
                                            }
                                        })
                                        .unwrap_or(0);
                                    if buf.len() >= hdr_end + 4 + content_len {
                                        break;
                                    }
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    captured_req = String::from_utf8_lossy(&buf).into_owned();
                    let _ = stream.write_all(raw_http_response.as_bytes());
                    let _ = stream.flush();
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }
        captured_req
    });
    (port, stop, handle)
}

fn http_200_json(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

#[test]
fn unconfigured_or_builtin_provider_fails_closed_and_legacy_diagnostics_remain_intact() {
    for unconfigured in [
        r#"{"action":"copilot_run","mode":"edit","module":"SHEET","prompt":"합계 추가"}"#,
        r#"{"action":"copilot_run","mode":"edit","module":"SHEET","prompt":"합계 추가","provider":null}"#,
        r#"{"action":"copilot_run","mode":"edit","module":"SHEET","prompt":"합계 추가","provider":{"kind":""}}"#,
        r#"{"action":"copilot_run","mode":"edit","module":"SHEET","prompt":"합계 추가","provider":{"kind":"builtin"}}"#,
        r#"{"action":"copilot_test_provider","provider":{"kind":"builtin"}}"#,
    ] {
        assert!(
            matches!(
                EasyOfficeIpcRouter::dispatch("run_smart_assistant", unconfigured),
                Err(EasyOfficeError::InvalidArgument(_))
            ),
            "unconfigured or fake builtin provider must be rejected: {unconfigured}"
        );
    }

    let legacy_plain = EasyOfficeIpcRouter::dispatch(
        "run_smart_assistant",
        "첫 번째 문장입니다. TODO: 보안 결재 완료",
    )
    .unwrap();
    assert!(legacy_plain.contains("\"summaryCount\""));
    assert!(legacy_plain.contains("\"actionItemCount\":1"));

    let legacy_json = EasyOfficeIpcRouter::dispatch(
        "run_smart_assistant",
        r#"{"text":"두 번째 문장입니다. TODO: 검토 완료"}"#,
    )
    .unwrap();
    assert!(legacy_json.contains("\"summaryCount\""));
    assert!(legacy_json.contains("\"actionItemCount\":1"));
}

#[test]
fn ask_mode_and_pdf_module_strip_all_write_operations_across_providers() {
    let llm_callback = |_http_req: &easyoffice_app::CopilotLlmRequest| -> Result<String, String> {
        let mutating_payload = r##"{"reply":"분석 응답","operations":[{"kind":"sheet.write","sheet":"Sheet1","range":"D1:D2","cells":[{"coord":"D1","raw":"비율"},{"coord":"D2","raw":"=C2/B2"}]},{"kind":"doc.append","blocks":[{"tag":"p","text":"추가"}]}],"locations":[{"module":"SHEET","label":"D1:D2","sheet":"Sheet1","range":"D1:D2"}]}"##;
        Ok(StrictJsonValue::Object(vec![(
            "choices".to_string(),
            StrictJsonValue::Array(vec![StrictJsonValue::Object(vec![(
                "message".to_string(),
                StrictJsonValue::Object(vec![(
                    "content".to_string(),
                    StrictJsonValue::Str(mutating_payload.to_string()),
                )]),
            )])]),
        )])
        .to_json_string())
    };
    let writers = ShellWriters {
        llm: Some(&llm_callback),
        ..ShellWriters::default()
    };

    let sheet_ask = r#"{
      "action": "copilot_run",
      "mode": "ask",
      "module": "SHEET",
      "prompt": "목표 대비 %를 설명하세요.",
      "context": { "sheet": { "name": "Sheet1", "cells": [] } },
      "provider": { "kind": "remote_https", "baseUrl": "https://93.184.216.34/v1", "model": "gpt-5", "apiKey": "sk-test" }
    }"#;
    let ask_raw = EasyOfficeIpcRouter::dispatch_for_owner_with_shell_writers(
        "run_smart_assistant",
        sheet_ask,
        "",
        writers,
    )
    .unwrap();
    let ask_obj = parse_obj(&ask_raw);
    assert_eq!(StrictJsonValue::get_str(&ask_obj, "mode"), Some("ask"));
    assert_eq!(
        StrictJsonValue::get_field(&ask_obj, "updatedInPlace").and_then(StrictJsonValue::as_bool),
        Some(false)
    );
    assert!(StrictJsonValue::get_field(&ask_obj, "operations")
        .and_then(StrictJsonValue::as_array)
        .unwrap()
        .is_empty());

    let pdf_edit = r#"{
      "action": "copilot_run",
      "mode": "edit",
      "module": "PDF",
      "prompt": "핵심을 요약하고 문단을 추가하세요.",
      "context": { "pdf": { "text": "제1조 목적.", "pageCount": 3 } },
      "provider": { "kind": "remote_https", "baseUrl": "https://93.184.216.34/v1", "model": "gpt-5", "apiKey": "sk-test" }
    }"#;
    let pdf_raw = EasyOfficeIpcRouter::dispatch_for_owner_with_shell_writers(
        "run_smart_assistant",
        pdf_edit,
        "",
        writers,
    )
    .unwrap();
    let pdf_obj = parse_obj(&pdf_raw);
    assert_eq!(
        StrictJsonValue::get_field(&pdf_obj, "updatedInPlace").and_then(StrictJsonValue::as_bool),
        Some(false)
    );
    assert!(StrictJsonValue::get_field(&pdf_obj, "operations")
        .and_then(StrictJsonValue::as_array)
        .unwrap()
        .is_empty());
}

#[test]
fn ssrf_blocks_private_loopback_credentials_queries_and_bad_schemes() {
    let bad_remote_urls = [
        "http://api.example.com/v1",
        "https://user:pass@api.example.com/v1",
        "https://api.example.com/v1?secret=1",
        "https://api.example.com/v1#frag",
        "https://api.example.com/v1/../admin",
        "https://api.example.com/v1/%2e%2e",
        "https://127.0.0.1/v1",
        "https://10.0.0.1/v1",
        "https://100.64.0.1/v1",
        "https://169.254.169.254/latest/meta-data",
        "https://172.16.0.1/v1",
        "https://192.168.1.10/v1",
        "https://[::1]/v1",
        "https://[fc00::1]/v1",
        "https://[fe80::1]/v1",
        "https://[2001:db8::1]/v1",
        "https://[2002::1]/v1",
        "https://localhost/v1",
    ];
    for url in bad_remote_urls {
        let payload = format!(
            r#"{{"action":"copilot_test_provider","provider":{{"kind":"remote_https","baseUrl":"{url}","model":"m1","apiKey":"k1"}}}}"#
        );
        let res = EasyOfficeIpcRouter::dispatch("run_smart_assistant", &payload);
        assert!(res.is_err(), "remote_https should reject {url}");
    }

    let bad_local_urls = [
        "https://127.0.0.1:8080/v1",
        "http://10.0.0.1:8080/v1",
        "http://192.168.0.10:8080/v1",
        "http://example.com:8080/v1",
        "http://localhost:8080/v1",
        "http://127.0.0.1/v1",
        "http://user@127.0.0.1:8080/v1",
        "http://127.0.0.1:8080/v1?x=1",
    ];
    for url in bad_local_urls {
        let payload = format!(
            r#"{{"action":"copilot_test_provider","provider":{{"kind":"local_http","baseUrl":"{url}","model":"local-m1","apiKey":""}}}}"#
        );
        let res = EasyOfficeIpcRouter::dispatch("run_smart_assistant", &payload);
        assert!(res.is_err(), "local_http should reject {url}");
    }
}

#[test]
fn local_http_mock_llm_roundtrip_ask_block_redirect_and_size_guards() {
    let assistant_json = r#"```json
{"reply":"시트 D열에 수식을 반영했습니다.","operations":[{"kind":"sheet.write","sheet":"Sheet1","range":"D1:D2","cells":[{"coord":"D1","raw":"비율","bold":true},{"coord":"D2","raw":"=C2/B2","numFmt":"0.0%"}]}],"locations":[{"module":"SHEET","label":"D1:D2","sheet":"Sheet1","range":"D1:D2"}]}
```"#;
    let completion_body = StrictJsonValue::Object(vec![(
        "choices".to_string(),
        StrictJsonValue::Array(vec![StrictJsonValue::Object(vec![(
            "message".to_string(),
            StrictJsonValue::Object(vec![(
                "content".to_string(),
                StrictJsonValue::Str(assistant_json.to_string()),
            )]),
        )])]),
    )])
    .to_json_string();
    let (port, stop, handle) = spawn_mock_llm_server(http_200_json(&completion_body));

    let req = format!(
        r#"{{
          "action": "copilot_run",
          "mode": "edit",
          "module": "SHEET",
          "prompt": "비율 열을 추가하세요.",
          "context": {{ "sheet": {{ "name": "Sheet1", "cells": [{{"coord":"B2","raw":"10","value":"10"}},{{"coord":"C2","raw":"8","value":"8"}}] }} }},
          "provider": {{ "kind": "local_http", "baseUrl": "http://127.0.0.1:{port}/v1", "model": "local-7b", "apiKey": "local-secret" }}
        }}"#
    );
    let raw = EasyOfficeIpcRouter::dispatch("run_smart_assistant", &req).unwrap();
    stop.store(true, Ordering::Release);
    let captured_http = handle.join().unwrap();
    assert!(captured_http.contains("POST /v1/chat/completions HTTP/1.1"));
    assert!(captured_http
        .to_ascii_lowercase()
        .contains("accept-encoding: identity"));
    assert!(captured_http.contains("Authorization: Bearer local-secret"));

    let obj = parse_obj(&raw);
    assert_eq!(StrictJsonValue::get_str(&obj, "status"), Some("ok"));
    assert_eq!(
        StrictJsonValue::get_field(&obj, "updatedInPlace").and_then(StrictJsonValue::as_bool),
        Some(true)
    );
    assert_eq!(
        StrictJsonValue::get_field(&obj, "operations")
            .and_then(StrictJsonValue::as_array)
            .unwrap()
            .len(),
        1
    );

    let redirect_resp = "HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/latest\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string();
    let (port_redir, stop_redir, handle_redir) = spawn_mock_llm_server(redirect_resp);
    let req_redir = format!(
        r#"{{"action":"copilot_test_provider","provider":{{"kind":"local_http","baseUrl":"http://127.0.0.1:{port_redir}/v1","model":"local-7b","apiKey":""}}}}"#
    );
    let redir_res = EasyOfficeIpcRouter::dispatch("run_smart_assistant", &req_redir);
    stop_redir.store(true, Ordering::Release);
    let _ = handle_redir.join().unwrap();
    assert!(matches!(
        redir_res,
        Err(EasyOfficeError::SecurityViolation(_))
    ));

    let gzip_resp = "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_string();
    let (port_gz, stop_gz, handle_gz) = spawn_mock_llm_server(gzip_resp);
    let req_gz = format!(
        r#"{{"action":"copilot_test_provider","provider":{{"kind":"local_http","baseUrl":"http://127.0.0.1:{port_gz}/v1","model":"local-7b","apiKey":""}}}}"#
    );
    let gz_res = EasyOfficeIpcRouter::dispatch("run_smart_assistant", &req_gz);
    stop_gz.store(true, Ordering::Release);
    let _ = handle_gz.join().unwrap();
    assert!(gz_res.is_err());

    let huge_body = format!("{{\"ok\":\"{}\"}}", "x".repeat(512 * 1024 + 64));
    let (port_big, stop_big, handle_big) = spawn_mock_llm_server(http_200_json(&huge_body));
    let req_big = format!(
        r#"{{"action":"copilot_test_provider","provider":{{"kind":"local_http","baseUrl":"http://127.0.0.1:{port_big}/v1","model":"local-7b","apiKey":""}}}}"#
    );
    let big_res = EasyOfficeIpcRouter::dispatch("run_smart_assistant", &req_big);
    stop_big.store(true, Ordering::Release);
    let _ = handle_big.join().unwrap();
    assert!(big_res.is_err());
}

#[test]
fn resource_budgets_fail_closed_with_resource_budget_exceeded() {
    let over_prompt = format!(
        r#"{{"action":"copilot_run","mode":"ask","module":"DOC","prompt":"{}","provider":{{"kind":"ollama"}}}}"#,
        "a".repeat(16_385)
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("run_smart_assistant", &over_prompt),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    let hist_items = vec![r#"{"role":"user","text":"hi"}"#; 33].join(",");
    let over_history = format!(
        r#"{{"action":"copilot_run","mode":"ask","module":"DOC","prompt":"요약","history":[{hist_items}],"provider":{{"kind":"ollama"}}}}"#
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("run_smart_assistant", &over_history),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    let cells = vec![r#"{"coord":"A1","raw":"1","value":"1"}"#; 10_001].join(",");
    let over_cells = format!(
        r#"{{"action":"copilot_run","mode":"ask","module":"SHEET","prompt":"요약","context":{{"sheet":{{"name":"S1","cells":[{cells}]}}}},"provider":{{"kind":"ollama"}}}}"#
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("run_smart_assistant", &over_cells),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    let paras = vec![r#""문단""#; 2_049].join(",");
    let over_paras = format!(
        r#"{{"action":"copilot_run","mode":"ask","module":"DOC","prompt":"요약","context":{{"doc":{{"paragraphs":[{paras}]}}}},"provider":{{"kind":"ollama"}}}}"#
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("run_smart_assistant", &over_paras),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    let slides = vec![r#"{"title":"T","bullets":[],"notes":""}"#; 257].join(",");
    let over_slides = format!(
        r#"{{"action":"copilot_run","mode":"ask","module":"SLIDE","prompt":"요약","context":{{"slide":{{"activeIndex":0,"slides":[{slides}]}}}},"provider":{{"kind":"ollama"}}}}"#
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("run_smart_assistant", &over_slides),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    let over_key = format!(
        r#"{{"action":"copilot_run","mode":"ask","module":"DOC","prompt":"요약","provider":{{"kind":"openai","apiKey":"{}"}}}}"#,
        "k".repeat(4_097)
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("run_smart_assistant", &over_key),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn llm_chunked_plaintext_fallback_invalid_op_filter_and_shell_writer_hook() {
    let plain_completion = StrictJsonValue::Object(vec![(
        "choices".to_string(),
        StrictJsonValue::Array(vec![StrictJsonValue::Object(vec![(
            "message".to_string(),
            StrictJsonValue::Object(vec![(
                "content".to_string(),
                StrictJsonValue::Str("일반 텍스트 응답입니다.".to_string()),
            )]),
        )])]),
    )])
    .to_json_string();
    let chunked_resp = format!(
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
        plain_completion.len(),
        plain_completion
    );
    let (port, stop, handle) = spawn_mock_llm_server(chunked_resp);
    let req = format!(
        r#"{{
          "action": "copilot_run",
          "mode": "edit",
          "module": "DOC",
          "prompt": "요약해 주세요.",
          "context": {{ "doc": {{ "paragraphs": ["본문"] }} }},
          "provider": {{ "kind": "local_http", "baseUrl": "http://127.0.0.1:{port}/v1/chat/completions", "model": "local-m", "apiKey": "" }}
        }}"#
    );
    let raw = EasyOfficeIpcRouter::dispatch("run_smart_assistant", &req).unwrap();
    stop.store(true, Ordering::Release);
    let _ = handle.join().unwrap();
    let obj = parse_obj(&raw);
    assert_eq!(
        StrictJsonValue::get_str(&obj, "reply"),
        Some("일반 텍스트 응답입니다.")
    );
    assert_eq!(
        StrictJsonValue::get_field(&obj, "updatedInPlace").and_then(StrictJsonValue::as_bool),
        Some(false)
    );

    let llm_callback = |http_req: &easyoffice_app::CopilotLlmRequest| -> Result<String, String> {
        assert!(http_req.is_https);
        assert_eq!(http_req.host, "93.184.216.34");
        let assistant_payload = r##"{"reply":"검증 테스트","operations":[{"kind":"sheet.write","sheet":"Sheet1","range":"../../bad","cells":[{"coord":"A1","raw":"x"}]},{"kind":"sheet.format","sheet":"Sheet1","range":"A1:B2","fill":"red","color":"#112233","bold":true}],"locations":[]}"##;
        Ok(StrictJsonValue::Object(vec![(
            "choices".to_string(),
            StrictJsonValue::Array(vec![StrictJsonValue::Object(vec![(
                "message".to_string(),
                StrictJsonValue::Object(vec![(
                    "content".to_string(),
                    StrictJsonValue::Str(assistant_payload.to_string()),
                )]),
            )])]),
        )])
        .to_json_string())
    };
    let writers = ShellWriters {
        llm: Some(&llm_callback),
        ..ShellWriters::default()
    };
    let remote_req = r#"{
      "action": "copilot_run",
      "mode": "edit",
      "module": "SHEET",
      "prompt": "강조하세요",
      "context": { "sheet": { "name": "Sheet1", "cells": [] } },
      "provider": { "kind": "remote_https", "baseUrl": "https://93.184.216.34/v1", "model": "m1", "apiKey": "k1" }
    }"#;
    let res = EasyOfficeIpcRouter::dispatch_for_owner_with_shell_writers(
        "run_smart_assistant",
        remote_req,
        "",
        writers,
    )
    .unwrap();
    let res_obj = parse_obj(&res);
    let ops = StrictJsonValue::get_field(&res_obj, "operations")
        .and_then(StrictJsonValue::as_array)
        .unwrap();
    assert_eq!(ops.len(), 1, "bad range op must be dropped: {res}");
    let kept = ops[0].as_object().unwrap();
    assert_eq!(StrictJsonValue::get_str(kept, "kind"), Some("sheet.format"));
    assert!(
        StrictJsonValue::get_field(kept, "fill").is_none(),
        "invalid non-hex fill color must be dropped: {res}"
    );
    assert_eq!(StrictJsonValue::get_str(kept, "color"), Some("#112233"));
}
