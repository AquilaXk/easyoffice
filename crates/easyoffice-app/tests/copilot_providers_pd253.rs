use easyoffice_app::copilot::HttpMethod;
use easyoffice_app::{EasyOfficeIpcRouter, ShellWriters};
use easyoffice_formats::{EasyOfficeError, StrictJsonValue};
use std::cell::RefCell;

fn parse_obj(raw: &str) -> Vec<(String, StrictJsonValue)> {
    StrictJsonValue::parse(raw)
        .expect("valid JSON")
        .as_object()
        .expect("root object")
        .to_vec()
}

#[test]
fn live_model_discovery_filters_non_chat_models_and_rejects_hardcoded_or_off_effort() {
    for invalid in [
        r#"{"action":"copilot_list_models"}"#,
        r#"{"action":"copilot_list_models","provider":{"kind":""}}"#,
        r#"{"action":"copilot_list_models","provider":{"kind":"builtin"}}"#,
        r#"{"action":"copilot_run","mode":"ask","module":"DOC","prompt":"요약","provider":{"kind":"openai","baseUrl":"https://93.184.216.34/v1","model":"o3","apiKey":"sk-1","reasoningEffort":"off"}}"#,
    ] {
        assert!(
            matches!(
                EasyOfficeIpcRouter::dispatch("run_smart_assistant", invalid),
                Err(EasyOfficeError::InvalidArgument(_))
            ),
            "invalid provider or 'off' reasoning effort must fail closed: {invalid}"
        );
    }

    let openai_models_cb =
        |http_req: &easyoffice_app::CopilotLlmRequest| -> Result<String, String> {
            assert_eq!(http_req.method, HttpMethod::Get);
            assert_eq!(http_req.path, "/v1/models");
            Ok(r#"{
          "object": "list",
          "data": [
            { "id": "o3", "context_window": 200000 },
            { "id": "gpt-4o", "context_window": 128000 },
            { "id": "text-embedding-3-small" },
            { "id": "whisper-1" },
            { "id": "dall-e-3" },
            { "id": "omni-moderation-latest" }
          ]
        }"#
            .to_string())
        };
    let disc_req = r#"{
      "action": "copilot_list_models",
      "discover": true,
      "provider": {
        "kind": "openai",
        "baseUrl": "https://93.184.216.34/v1",
        "apiKey": "sk-live-key"
      }
    }"#;
    let raw = EasyOfficeIpcRouter::dispatch_for_owner_with_shell_writers(
        "run_smart_assistant",
        disc_req,
        "",
        ShellWriters {
            llm: Some(&openai_models_cb),
            ..ShellWriters::default()
        },
    )
    .unwrap();
    let obj = parse_obj(&raw);
    assert_eq!(StrictJsonValue::get_str(&obj, "status"), Some("ok"));
    assert_eq!(
        StrictJsonValue::get_field(&obj, "live").and_then(StrictJsonValue::as_bool),
        Some(true)
    );
    let models = StrictJsonValue::get_field(&obj, "models")
        .and_then(StrictJsonValue::as_array)
        .unwrap();
    assert_eq!(
        models.len(),
        2,
        "embeddings/whisper/dall-e/moderation filtered out: {raw}"
    );
    let m0 = models[0].as_object().unwrap();
    let m1 = models[1].as_object().unwrap();
    assert_eq!(StrictJsonValue::get_str(m0, "id"), Some("o3"));
    assert_eq!(
        StrictJsonValue::get_field(m0, "supportsReasoning").and_then(StrictJsonValue::as_bool),
        Some(true)
    );
    assert_eq!(StrictJsonValue::get_str(m1, "id"), Some("gpt-4o"));
    assert_eq!(
        StrictJsonValue::get_field(m1, "supportsReasoning").and_then(StrictJsonValue::as_bool),
        Some(false)
    );
}

#[test]
fn anthropic_native_messages_protocol_headers_thinking_and_response() {
    let seen = RefCell::new(None);
    let llm_cb = |http_req: &easyoffice_app::CopilotLlmRequest| -> Result<String, String> {
        *seen.borrow_mut() = Some(http_req.clone());
        Ok(r#"{
          "id": "msg_01",
          "type": "message",
          "role": "assistant",
          "content": [
            { "type": "thinking", "thinking": "먼저 표 범위를 분석합니다." },
            { "type": "text", "text": "{\"reply\":\"Anthropic 응답 완료\",\"operations\":[{\"kind\":\"doc.append\",\"blocks\":[{\"text\":\"추가 문단\"}]}],\"locations\":[{\"module\":\"DOC\",\"label\":\"문단 2\",\"paragraphIndex\":1}]}" }
          ]
        }"#
        .to_string())
    };
    let writers = ShellWriters {
        llm: Some(&llm_cb),
        ..ShellWriters::default()
    };
    let payload = r#"{
      "action": "copilot_run",
      "mode": "edit",
      "module": "DOC",
      "prompt": "요약 문단을 추가하세요.",
      "history": [
        { "role": "user", "text": "이전 질문 1" },
        { "role": "user", "text": "이전 질문 2" },
        { "role": "assistant", "text": "이전 답변" }
      ],
      "context": { "doc": { "paragraphs": ["첫 문단"] } },
      "provider": {
        "kind": "anthropic",
        "baseUrl": "https://93.184.216.34/v1",
        "model": "claude-sonnet-4-6",
        "apiKey": "sk-ant-test-999",
        "reasoningEffort": "high"
      }
    }"#;

    let raw = EasyOfficeIpcRouter::dispatch_for_owner_with_shell_writers(
        "run_smart_assistant",
        payload,
        "",
        writers,
    )
    .unwrap();

    let cap = seen.into_inner().expect("captured anthropic request");
    assert_eq!(cap.method, HttpMethod::Post);
    assert_eq!(cap.path, "/v1/messages");
    assert!(cap
        .headers
        .contains(&("x-api-key".to_string(), "sk-ant-test-999".to_string())));
    assert!(cap
        .headers
        .contains(&("anthropic-version".to_string(), "2023-06-01".to_string())));
    assert!(!cap.headers.iter().any(|(k, _)| k == "Authorization"));

    let req_body = parse_obj(&cap.body_json);
    assert_eq!(
        StrictJsonValue::get_str(&req_body, "model"),
        Some("claude-sonnet-4-6")
    );
    assert!(StrictJsonValue::get_str(&req_body, "system").is_some());
    assert!(
        StrictJsonValue::get_field(&req_body, "temperature").is_none(),
        "temperature omitted when Anthropic thinking is enabled"
    );
    let thinking = StrictJsonValue::get_field(&req_body, "thinking")
        .and_then(StrictJsonValue::as_object)
        .expect("thinking block");
    assert_eq!(StrictJsonValue::get_str(thinking, "type"), Some("enabled"));
    assert_eq!(
        StrictJsonValue::get_field(thinking, "budget_tokens").and_then(StrictJsonValue::as_u64),
        Some(8192)
    );
    let msgs = StrictJsonValue::get_field(&req_body, "messages")
        .and_then(StrictJsonValue::as_array)
        .unwrap();
    assert_eq!(
        msgs.len(),
        3,
        "consecutive user history turns coalesced into alternating user/assistant/user"
    );

    let resp = parse_obj(&raw);
    assert_eq!(
        StrictJsonValue::get_str(&resp, "reply"),
        Some("Anthropic 응답 완료")
    );
    assert_eq!(
        StrictJsonValue::get_str(&resp, "reasoning"),
        Some("먼저 표 범위를 분석합니다.")
    );
    assert_eq!(
        StrictJsonValue::get_field(&resp, "updatedInPlace").and_then(StrictJsonValue::as_bool),
        Some(true)
    );
}

#[test]
fn google_native_generate_content_protocol_header_only_and_traversal_guard() {
    let seen = RefCell::new(None);
    let llm_cb = |http_req: &easyoffice_app::CopilotLlmRequest| -> Result<String, String> {
        *seen.borrow_mut() = Some(http_req.clone());
        Ok(r#"{
          "candidates": [{
            "content": {
              "role": "model",
              "parts": [
                { "thought": true, "text": "데이터 구조를 확인합니다." },
                { "text": "{\"reply\":\"Google 응답 완료\",\"operations\":[],\"locations\":[]}" }
              ]
            }
          }]
        }"#
        .to_string())
    };
    let writers = ShellWriters {
        llm: Some(&llm_cb),
        ..ShellWriters::default()
    };
    let payload = r#"{
      "action": "copilot_run",
      "mode": "ask",
      "module": "SHEET",
      "prompt": "요약해 주세요.",
      "history": [{ "role": "assistant", "text": "무엇을 도와드릴까요?" }, { "role": "user", "text": "인사" }],
      "context": { "sheet": { "name": "Sheet1", "cells": [] } },
      "provider": {
        "kind": "google",
        "baseUrl": "https://93.184.216.34/v1beta",
        "model": "models/gemini-2.5-pro",
        "apiKey": "AIzaSyTestKey123",
        "reasoningEffort": "low"
      }
    }"#;

    let raw = EasyOfficeIpcRouter::dispatch_for_owner_with_shell_writers(
        "run_smart_assistant",
        payload,
        "",
        writers,
    )
    .unwrap();

    let cap = seen.into_inner().expect("captured google request");
    assert_eq!(cap.method, HttpMethod::Post);
    assert_eq!(cap.path, "/v1beta/models/gemini-2.5-pro:generateContent");
    assert!(!cap.url.contains('?'), "API key must never be in URL query");
    assert!(!cap.url.contains("AIzaSyTestKey123"));
    assert!(cap
        .headers
        .contains(&("x-goog-api-key".to_string(), "AIzaSyTestKey123".to_string())));

    let req_body = parse_obj(&cap.body_json);
    assert!(StrictJsonValue::get_field(&req_body, "systemInstruction").is_some());
    let contents = StrictJsonValue::get_field(&req_body, "contents")
        .and_then(StrictJsonValue::as_array)
        .unwrap();
    assert_eq!(
        contents.len(),
        1,
        "leading assistant history skipped and consecutive user turns merged"
    );

    let resp = parse_obj(&raw);
    assert_eq!(
        StrictJsonValue::get_str(&resp, "reply"),
        Some("Google 응답 완료")
    );
    assert_eq!(
        StrictJsonValue::get_str(&resp, "reasoning"),
        Some("데이터 구조를 확인합니다.")
    );

    // Path traversal inside Google model ID must fail closed with SecurityViolation
    let bad_model = r#"{
      "action": "copilot_test_provider",
      "provider": {
        "kind": "google",
        "baseUrl": "https://93.184.216.34/v1beta",
        "model": "gemini-2.5-pro/../../admin",
        "apiKey": "AIzaSyTestKey123"
      }
    }"#;
    let bad_res = EasyOfficeIpcRouter::dispatch("run_smart_assistant", bad_model);
    assert!(matches!(
        bad_res,
        Err(EasyOfficeError::SecurityViolation(_))
    ));
}

#[test]
fn deepseek_reasoning_and_live_model_discovery_across_formats() {
    // 1. DeepSeek reasoning_effort + reasoning_content extraction
    let seen_ds = RefCell::new(None);
    let ds_cb = |http_req: &easyoffice_app::CopilotLlmRequest| -> Result<String, String> {
        *seen_ds.borrow_mut() = Some(http_req.clone());
        Ok(r#"{
          "choices": [{
            "message": {
              "role": "assistant",
              "reasoning_content": "수식을 계산했습니다.",
              "content": "{\"reply\":\"DeepSeek 결과\",\"operations\":[],\"locations\":[]}"
            }
          }]
        }"#
        .to_string())
    };
    let ds_req = r#"{
      "action": "copilot_run",
      "mode": "ask",
      "module": "DOC",
      "prompt": "검토",
      "provider": {
        "kind": "deepseek",
        "baseUrl": "https://93.184.216.34",
        "model": "deepseek-reasoner",
        "apiKey": "ds-key-1",
        "reasoningEffort": "medium"
      }
    }"#;
    let ds_raw = EasyOfficeIpcRouter::dispatch_for_owner_with_shell_writers(
        "run_smart_assistant",
        ds_req,
        "",
        ShellWriters {
            llm: Some(&ds_cb),
            ..ShellWriters::default()
        },
    )
    .unwrap();
    let ds_cap = seen_ds.into_inner().unwrap();
    assert_eq!(ds_cap.path, "/chat/completions");
    assert!(ds_cap.body_json.contains("\"reasoning_effort\":\"medium\""));
    assert!(ds_cap
        .body_json
        .contains("\"thinking\":{\"type\":\"enabled\"}"));
    assert!(ds_raw.contains("수식을 계산했습니다."));

    // 2. Live Google /v1beta/models discovery (filters out embedContent-only models & strips models/)
    let g_models_cb = |http_req: &easyoffice_app::CopilotLlmRequest| -> Result<String, String> {
        assert_eq!(http_req.method, HttpMethod::Get);
        assert_eq!(http_req.path, "/v1beta/models");
        Ok(r#"{
          "models": [
            {
              "name": "models/gemini-2.5-pro",
              "displayName": "Gemini 2.5 Pro",
              "inputTokenLimit": 1048576,
              "supportedGenerationMethods": ["generateContent", "countTokens"]
            },
            {
              "name": "models/text-embedding-004",
              "displayName": "Text Embedding",
              "supportedGenerationMethods": ["embedContent"]
            }
          ]
        }"#
        .to_string())
    };
    let g_disc_req = r#"{
      "action": "copilot_list_models",
      "discover": true,
      "provider": {
        "kind": "google",
        "baseUrl": "https://93.184.216.34/v1beta",
        "apiKey": "AIzaKey"
      }
    }"#;
    let g_disc_raw = EasyOfficeIpcRouter::dispatch_for_owner_with_shell_writers(
        "run_smart_assistant",
        g_disc_req,
        "",
        ShellWriters {
            llm: Some(&g_models_cb),
            ..ShellWriters::default()
        },
    )
    .unwrap();
    let g_disc_obj = parse_obj(&g_disc_raw);
    assert_eq!(
        StrictJsonValue::get_field(&g_disc_obj, "live").and_then(StrictJsonValue::as_bool),
        Some(true)
    );
    let g_list = StrictJsonValue::get_field(&g_disc_obj, "models")
        .and_then(StrictJsonValue::as_array)
        .unwrap();
    assert_eq!(g_list.len(), 1);
    assert_eq!(
        StrictJsonValue::get_str(g_list[0].as_object().unwrap(), "id"),
        Some("gemini-2.5-pro")
    );

    // 3. Missing API key on cloud company provider must fail closed before network call
    let missing_key = r#"{
      "action": "copilot_test_provider",
      "provider": { "kind": "openai", "baseUrl": "https://93.184.216.34/v1", "model": "gpt-5", "apiKey": "" }
    }"#;
    assert!(EasyOfficeIpcRouter::dispatch("run_smart_assistant", missing_key).is_err());
}
