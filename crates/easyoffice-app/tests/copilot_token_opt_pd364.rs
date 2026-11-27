//! 코파일럿 토큰 사용 최적화 통합 검증(PD-364):
//! P0: 4계층 캐시·불변 PDF 문맥 배치·정적 시스템 프롬프트·OpenAI SHA-256 prompt_cache_key·include_usage·
//!     Anthropic 블록 배열/최상위 cache_control·usage 계측·무료 GET /models 우선 연결 시험·모듈별 고밀도 압축·
//!     DOC/HWP 실제 빈 문단("")과 희소 슬롯(null) 분리·selected_text 중복 제거.
//! P1: lightweight 경량 예측 경로·읽기 전용 평문 실시간 스트리밍·edit 스키마 경량화·emit_copilot_turn reply 스트리밍.
//! P2: 단독 선택 명령 reasoning_effort 상한·doc.replace_span 검증/PII 복원·악의적 상한 Fail-Closed 거부.

use easyoffice_app::copilot::HttpMethod;
use easyoffice_app::{
    ChunkSink, CopilotLlmRequest, CopilotStreamEvent, EasyOfficeIpcRouter, ShellWriters,
};
use easyoffice_formats::{EasyOfficeError, StrictJsonValue};
use std::sync::Mutex;

fn parse_obj(raw: &str) -> Vec<(String, StrictJsonValue)> {
    match StrictJsonValue::parse(raw).expect("valid JSON") {
        StrictJsonValue::Object(obj) => obj,
        other => panic!("expected object, got {other:?}"),
    }
}

fn openai_text_reply(content: &str) -> String {
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

fn dispatch_raw_llm(
    payload: &str,
    raw_llm_response: &str,
) -> (Result<String, EasyOfficeError>, Vec<CopilotLlmRequest>) {
    let seen_reqs = Mutex::new(Vec::new());
    let resp = raw_llm_response.to_string();
    let callback = |req: &CopilotLlmRequest| -> Result<String, String> {
        seen_reqs.lock().unwrap().push(req.clone());
        Ok(resp.clone())
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
    (result, seen_reqs.into_inner().unwrap())
}

fn dispatch_stream_chunks(
    payload: &str,
    chunks: &[&str],
) -> (
    Result<String, EasyOfficeError>,
    Vec<CopilotLlmRequest>,
    Vec<String>,
) {
    let seen_reqs = Mutex::new(Vec::new());
    let seen_pieces = Mutex::new(Vec::new());
    let owned_chunks: Vec<String> = chunks.iter().map(|s| (*s).to_string()).collect();

    let stream_cb = |req: &CopilotLlmRequest, sink: &mut ChunkSink<'_>| -> Result<String, String> {
        seen_reqs.lock().unwrap().push(req.clone());
        let mut full = String::new();
        for c in &owned_chunks {
            full.push_str(c);
            if !sink(c.as_bytes()) {
                break;
            }
        }
        Ok(full)
    };
    let event_cb = |ev: &CopilotStreamEvent<'_>| {
        seen_pieces.lock().unwrap().push(ev.text.to_string());
    };
    let writers = ShellWriters {
        llm_stream: Some(&stream_cb),
        copilot_events: Some(&event_cb),
        ..ShellWriters::default()
    };
    let result = EasyOfficeIpcRouter::dispatch_for_owner_with_shell_writers(
        "run_smart_assistant",
        payload,
        "",
        writers,
    );
    (
        result,
        seen_reqs.into_inner().unwrap(),
        seen_pieces.into_inner().unwrap(),
    )
}

#[test]
fn cache_tiers_pdf_immutable_prefix_anthropic_blocks_and_openai_official_only() {
    // 1. Mutable DOC places [system] -> [priorHistory] -> [<document_context> + prompt] -> [loopSteps]
    let doc_req = r#"{
        "action":"copilot_run","requestId":"r-doc-1","stream":true,"mode":"edit","module":"DOC","prompt":"3번 문단 다듬기",
        "promptCacheKey":"session-doc-42",
        "history":[{"role":"user","text":"이전 질문"},{"role":"assistant","text":"이전 답변"}],
        "loopSteps":[{"role":"assistant","text":"도구 호출"},{"role":"user","text":"[도구 실행 결과] OK"}],
        "context":{"doc":{"paragraphs":["첫 문단","둘째 문단","셋째 문단"],"omittedParagraphs":0}},
        "provider":{"kind":"openai","baseUrl":"https://api.openai.com/v1","model":"gpt-4o","apiKey":"sk-test"}
    }"#;
    let (res, reqs, _) = dispatch_stream_chunks(
        doc_req,
        &[
            "data: {\"choices\":[{\"delta\":{\"content\":\"{\\\"reply\\\":\\\"완료\\\",\\\"operations\\\":[]}\"}}]}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":1400,\"completion_tokens\":30,\"prompt_tokens_details\":{\"cached_tokens\":1024},\"completion_tokens_details\":{\"reasoning_tokens\":12}}}\n\n",
            "data: [DONE]\n\n",
        ],
    );
    let res_json = res.expect("doc stream run ok");
    let body = &reqs[0].body_json;
    let body_obj = parse_obj(body);
    let pck =
        StrictJsonValue::get_str(&body_obj, "prompt_cache_key").expect("openai prompt_cache_key");
    assert_eq!(pck.len(), 64, "SHA-256 hex key must be 64 chars");
    assert!(pck.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
    assert!(body.contains(r#""stream_options":{"include_usage":true}"#));

    let msgs = StrictJsonValue::get_field(&body_obj, "messages")
        .and_then(StrictJsonValue::as_array)
        .unwrap();
    let msg_texts: Vec<&str> = msgs
        .iter()
        .map(|m| StrictJsonValue::get_str(m.as_object().unwrap(), "content").unwrap())
        .collect();
    assert_eq!(msg_texts[1], "이전 질문");
    assert_eq!(msg_texts[2], "이전 답변");
    assert!(msg_texts[3].starts_with("<document_context>"));
    assert!(msg_texts[3].contains("3번 문단 다듬기"));
    assert_eq!(msg_texts[4], "도구 호출");
    assert_eq!(msg_texts[5], "[도구 실행 결과] OK");

    let out = parse_obj(&res_json);
    let usage = StrictJsonValue::get_field(&out, "usage")
        .and_then(StrictJsonValue::as_object)
        .expect("usage serialized");
    assert_eq!(
        StrictJsonValue::get_field(usage, "inputTokens").and_then(StrictJsonValue::as_u64),
        Some(1400)
    );
    assert_eq!(
        StrictJsonValue::get_field(usage, "outputTokens").and_then(StrictJsonValue::as_u64),
        Some(30)
    );
    assert_eq!(
        StrictJsonValue::get_field(usage, "cachedTokens").and_then(StrictJsonValue::as_u64),
        Some(1024)
    );
    assert_eq!(
        StrictJsonValue::get_field(usage, "reasoningTokens").and_then(StrictJsonValue::as_u64),
        Some(12)
    );

    // 2. Compatible providers (Groq/Mistral/Ollama/LocalHttp/RemoteHttps) NEVER get prompt_cache_key or stream_options
    for (kind, url) in [
        ("groq", "https://93.184.216.34/v1"),
        ("mistral", "https://93.184.216.34/v1"),
        ("ollama", "http://127.0.0.1:11434/v1"),
        ("lm_studio", "http://127.0.0.1:1234/v1"),
        ("remote_https", "https://93.184.216.34/v1"),
    ] {
        let compat_req = format!(
            r#"{{"action":"copilot_run","requestId":"r-compat-{kind}","stream":true,"mode":"ask","module":"DOC","prompt":"안녕","context":{{"doc":{{"paragraphs":["가"]}}}},"provider":{{"kind":"{kind}","baseUrl":"{url}","model":"m1","apiKey":"sk-test"}}}}"#
        );
        let (compat_res, compat_reqs, _) = dispatch_stream_chunks(
            &compat_req,
            &["data: {\"choices\":[{\"delta\":{\"content\":\"응답\"}}]}\n\ndata: [DONE]\n\n"],
        );
        compat_res.expect("compat stream ok");
        assert!(
            !compat_reqs[0].body_json.contains("prompt_cache_key")
                && !compat_reqs[0].body_json.contains("stream_options"),
            "{kind} must not receive OpenAI-only cache/stream_options: {}",
            compat_reqs[0].body_json
        );
    }

    // 3. Immutable read-only PDF puts <document_context> BEFORE priorHistory on official Anthropic with block arrays & root cache_control
    let pdf_req = r#"{
        "action":"copilot_run","mode":"ask","module":"PDF","prompt":"후속 질문 2",
        "history":[{"role":"user","text":"첫 질문 1"},{"role":"assistant","text":"첫 답변 1"}],
        "context":{"pdf":{"text":"[1쪽] 첫 페이지\n[2쪽] 둘째 페이지","pageCount":2,"activePage":2}},
        "provider":{"kind":"anthropic","baseUrl":"https://api.anthropic.com","model":"claude-sonnet-4-5","apiKey":"sk-ant-test"}
    }"#;
    let (_, pdf_reqs) = dispatch_raw_llm(
        pdf_req,
        r#"{"content":[{"type":"text","text":"[2쪽] 답입니다."}],"usage":{"input_tokens":20,"cache_creation_input_tokens":50,"cache_read_input_tokens":800,"output_tokens":15}}"#,
    );
    let p_obj = parse_obj(&pdf_reqs[0].body_json);
    assert!(
        StrictJsonValue::get_field(&p_obj, "cache_control").is_some(),
        "top-level cache_control present"
    );
    let sys_arr = StrictJsonValue::get_field(&p_obj, "system")
        .and_then(StrictJsonValue::as_array)
        .unwrap();
    assert!(StrictJsonValue::get_field(sys_arr[0].as_object().unwrap(), "cache_control").is_some());
    let anth_msgs = StrictJsonValue::get_field(&p_obj, "messages")
        .and_then(StrictJsonValue::as_array)
        .unwrap();
    let first_blocks = StrictJsonValue::get_field(anth_msgs[0].as_object().unwrap(), "content")
        .and_then(StrictJsonValue::as_array)
        .expect("Anthropic message content is always block array");
    let b0 = StrictJsonValue::get_str(first_blocks[0].as_object().unwrap(), "text").unwrap();
    let b1 = StrictJsonValue::get_str(first_blocks[1].as_object().unwrap(), "text").unwrap();
    assert!(
        b0.contains("<document_context>"),
        "PDF document context precedes priorHistory: {b0}"
    );
    assert_eq!(b1, "첫 질문 1");

    // 4. Static system_prompt stays byte-identical whether PII exists in doc or not
    let clean_req = r#"{"action":"copilot_run","mode":"edit","module":"DOC","prompt":"다듬기","context":{"doc":{"paragraphs":["일반 문장"]}},"provider":{"kind":"openai","baseUrl":"https://api.openai.com/v1","model":"gpt-4o","apiKey":"sk-test"}}"#;
    let pii_req = r#"{"action":"copilot_run","mode":"edit","module":"DOC","prompt":"다듬기","context":{"doc":{"paragraphs":["이메일 a@b.com"]}},"provider":{"kind":"openai","baseUrl":"https://api.openai.com/v1","model":"gpt-4o","apiKey":"sk-test"}}"#;
    let (_, clean_r) = dispatch_raw_llm(
        clean_req,
        &openai_text_reply(r#"{"reply":"ok","operations":[]}"#),
    );
    let (_, pii_r) = dispatch_raw_llm(
        pii_req,
        &openai_text_reply(r#"{"reply":"ok","operations":[]}"#),
    );
    let clean_obj = parse_obj(&clean_r[0].body_json);
    let pii_obj = parse_obj(&pii_r[0].body_json);
    let sys_clean = StrictJsonValue::get_field(&clean_obj, "messages")
        .unwrap()
        .as_array()
        .unwrap()[0]
        .as_object()
        .unwrap();
    let sys_pii = StrictJsonValue::get_field(&pii_obj, "messages")
        .unwrap()
        .as_array()
        .unwrap()[0]
        .as_object()
        .unwrap();
    assert_eq!(
        StrictJsonValue::get_str(sys_clean, "content"),
        StrictJsonValue::get_str(sys_pii, "content"),
        "system_prompt must remain static across PII and non-PII turns"
    );
}

#[test]
fn free_models_endpoint_checked_before_chat_ping_and_fallback_uses_max_tokens_1() {
    // 1. Model present in GET /models -> returns ok without touching POST chat
    let seen_paths = Mutex::new(Vec::<(HttpMethod, String, String)>::new());
    let models_only_cb = |req: &CopilotLlmRequest| -> Result<String, String> {
        seen_paths
            .lock()
            .unwrap()
            .push((req.method, req.path.clone(), req.body_json.clone()));
        if req.method == HttpMethod::Get {
            Ok(r#"{"data":[{"id":"gpt-4o"}]}"#.to_string())
        } else {
            Err("chat endpoint must not be called when /models already verifies model".to_string())
        }
    };
    let res_ok = EasyOfficeIpcRouter::dispatch_for_owner_with_shell_writers(
        "run_smart_assistant",
        r#"{"action":"copilot_test_provider","provider":{"kind":"openai","baseUrl":"https://api.openai.com/v1","model":"gpt-4o","apiKey":"sk-test"}}"#,
        "",
        ShellWriters {
            llm: Some(&models_only_cb),
            ..ShellWriters::default()
        },
    );
    assert!(res_ok.is_ok(), "models-first probe succeeded: {res_ok:?}");
    let calls = seen_paths.into_inner().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, HttpMethod::Get);

    // 2. Unlisted custom model falls back to minimal chat ping with max_tokens = 1
    let fb_calls = Mutex::new(Vec::<(HttpMethod, String)>::new());
    let fallback_cb = |req: &CopilotLlmRequest| -> Result<String, String> {
        fb_calls
            .lock()
            .unwrap()
            .push((req.method, req.body_json.clone()));
        if req.method == HttpMethod::Get {
            Ok(r#"{"data":[{"id":"other-model"}]}"#.to_string())
        } else {
            Ok(openai_text_reply("ok"))
        }
    };
    let res_fb = EasyOfficeIpcRouter::dispatch_for_owner_with_shell_writers(
        "run_smart_assistant",
        r#"{"action":"copilot_test_provider","provider":{"kind":"remote_https","baseUrl":"https://93.184.216.34/v1","model":"custom-model","apiKey":"sk-test"}}"#,
        "",
        ShellWriters {
            llm: Some(&fallback_cb),
            ..ShellWriters::default()
        },
    );
    assert!(res_fb.is_ok(), "fallback ping succeeded: {res_fb:?}");
    let fb = fb_calls.into_inner().unwrap();
    assert_eq!(fb.len(), 2);
    assert!(
        fb[1].1.contains(r#""max_tokens":1"#),
        "fallback ping must use max_tokens:1: {}",
        fb[1].1
    );
}

#[test]
fn module_high_density_compression_dedup_tsv_isomorphic_and_pdf_window() {
    // SHEET isomorphic formulas + selection prioritization + raw==value dedup
    let mut cells_json = vec![
        r#"{"coord":"A1","raw":"항목","value":"항목"}"#.to_string(),
        r#"{"coord":"B1","raw":"기준","value":"기준"}"#.to_string(),
        r#"{"coord":"C1","raw":"실적","value":"실적"}"#.to_string(),
        r#"{"coord":"D1","raw":"비율","value":"비율"}"#.to_string(),
    ];
    for r in 2..=15 {
        cells_json.push(format!(r#"{{"coord":"A{r}","raw":"P{r}","value":"P{r}"}}"#));
        cells_json.push(format!(r#"{{"coord":"B{r}","raw":"100","value":"100"}}"#));
        cells_json.push(format!(
            r#"{{"coord":"C{r}","raw":"{}","value":"{}"}}"#,
            r * 10,
            r * 10
        ));
        cells_json.push(format!(
            r#"{{"coord":"D{r}","raw":"=C{r}/B{r}","value":"0.{r}"}}"#
        ));
    }
    let sheet_req = format!(
        r#"{{"action":"copilot_run","mode":"ask","module":"SHEET","prompt":"분석","selection":{{"sheet":"S1","range":"A1:D15"}},"context":{{"sheet":{{"name":"S1","tabs":["S1","요약"],"usedRange":"A1:D15","cells":[{}]}}}},"provider":{{"kind":"ollama","model":"qwen3"}}}}"#,
        cells_json.join(",")
    );
    let (sheet_res, sheet_reqs) = dispatch_raw_llm(&sheet_req, &openai_text_reply("확인"));
    sheet_res.expect("sheet run ok");
    let s_body = &sheet_reqs[0].body_json;
    assert!(s_body.contains("시트: S1 | 탭: S1, 요약 | 사용범위: A1:D15"));
    assert!(
        s_body.contains("동형수식: D2:D15 => =C{r}/B{r}"),
        "isomorphic run compressed: {s_body}"
    );
    assert!(
        !s_body.contains("raw="),
        "raw==value duplicate field removed: {s_body}"
    );

    // DOC selectedText deduplication: matching loaded paragraph omits `선택 텍스트:`, mismatching keeps it
    let dup_sel = r#"{"action":"copilot_run","mode":"edit","module":"DOC","prompt":"요약","command":"summarize","selection":{"paragraphIndex":0,"paragraphEnd":0},"context":{"doc":{"paragraphs":["정확히 같은 문단입니다."],"selectedText":"정확히 같은 문단입니다."}},"provider":{"kind":"ollama","model":"qwen3"}}"#;
    let (dup_res, dup_r) = dispatch_raw_llm(
        dup_sel,
        &openai_text_reply(r#"{"reply":"ok","operations":[]}"#),
    );
    dup_res.expect("dup sel ok");
    assert!(
        !dup_r[0].body_json.contains("선택 텍스트:"),
        "duplicate selectedText suppressed: {}",
        dup_r[0].body_json
    );

    let sub_sel = r#"{"action":"copilot_run","mode":"edit","module":"DOC","prompt":"번역","command":"translate","selection":{"paragraphIndex":0,"paragraphEnd":0},"context":{"doc":{"paragraphs":["긴 문단 전체 내용 중에서 일부"],"selectedText":"일부"}},"provider":{"kind":"ollama","model":"qwen3"}}"#;
    let (sub_res, sub_r) = dispatch_raw_llm(
        sub_sel,
        &openai_text_reply(r#"{"reply":"ok","operations":[]}"#),
    );
    sub_res.expect("sub sel ok");
    assert!(sub_r[0].body_json.contains("선택 텍스트: 일부"));

    // PDF activePage centered windowing
    let pdf_pages: Vec<String> = (1..=60)
        .map(|p| format!("[{p}쪽] {}", "본문 내용 ".repeat(80)))
        .collect();
    let pdf_req = format!(
        r#"{{"action":"copilot_run","mode":"ask","module":"PDF","prompt":"현재 쪽 확인","context":{{"pdf":{{"text":"{}","pageCount":60,"activePage":45}}}},"provider":{{"kind":"ollama","model":"qwen3"}}}}"#,
        pdf_pages.join("\\n")
    );
    let (pdf_res, pdf_r) = dispatch_raw_llm(&pdf_req, &openai_text_reply("[45쪽] 요약"));
    pdf_res.expect("pdf window ok");
    let p_body = &pdf_r[0].body_json;
    assert!(
        p_body.contains("[45쪽]"),
        "activePage kept inside PDF window"
    );
    assert!(p_body.contains("[앞 "), "far earlier pages omitted cleanly");

    // Pre-windowed JS payload with [1..35쪽 생략] front notice keeps activePage=45 intact in Rust
    let js_window_pages: Vec<String> = std::iter::once("[1..35쪽 생략]".to_string())
        .chain((36..=50).map(|p| format!("[{p}쪽] 페이지 {p} 본문")))
        .collect();
    let pdf_js_win_req = format!(
        r#"{{"action":"copilot_run","mode":"ask","module":"PDF","prompt":"45쪽 확인","context":{{"pdf":{{"text":"{}","pageCount":50,"activePage":45}}}},"provider":{{"kind":"ollama","model":"qwen3"}}}}"#,
        js_window_pages.join("\\n\\n")
    );
    let (pdf_jw_res, pdf_jw_r) =
        dispatch_raw_llm(&pdf_js_win_req, &openai_text_reply("[45쪽] 요약"));
    pdf_jw_res.expect("pdf pre-windowed ok");
    assert!(pdf_jw_r[0].body_json.contains("[1..35쪽 생략]"));
    assert!(pdf_jw_r[0].body_json.contains("[45쪽] 페이지 45 본문"));

    // Oversized single-line Korean activePage=45 (>24 KiB UTF-8) preserves [1..44쪽 생략] and [45쪽] prefix instead of dropping page 45
    let huge_pdf_req = format!(
        r#"{{"action":"copilot_run","mode":"ask","module":"PDF","prompt":"45쪽 확인","context":{{"pdf":{{"text":"[1..44쪽 생략]\\n\\n[45쪽] HUGE45_{}","pageCount":50,"activePage":45}}}},"provider":{{"kind":"ollama","model":"qwen3"}}}}"#,
        "한".repeat(10_000)
    );
    let (huge_pdf_res, huge_pdf_r) =
        dispatch_raw_llm(&huge_pdf_req, &openai_text_reply("[45쪽] 요약"));
    huge_pdf_res.expect("oversized activePage pdf ok");
    let huge_body = &huge_pdf_r[0].body_json;
    assert!(
        huge_body.contains("[1..44쪽 생략]"),
        "front omit header preserved on oversized activePage: {huge_body}"
    );
    assert!(
        huge_body.contains("[45쪽] HUGE45_"),
        "oversized activePage=45 content preserved instead of dropped: {huge_body}"
    );
    assert!(huge_body.contains("[이하 1줄 생략]"));

    // Multi-line activePage=45 (>24 KiB) preceded by large page 44 skips page 44 and emits page 45 lines
    let p44_lines: Vec<String> = (0..10)
        .map(|i| format!("{}44_{i}", "가".repeat(800)))
        .collect();
    let p45_lines: Vec<String> = (0..15)
        .map(|i| format!("{}45_{i}", "나".repeat(800)))
        .collect();
    let multiline_pdf = format!(
        "[44쪽] {}\\n\\n[45쪽] {}",
        p44_lines.join("\\n"),
        p45_lines.join("\\n")
    );
    let ml_req = format!(
        r#"{{"action":"copilot_run","mode":"ask","module":"PDF","prompt":"45쪽","context":{{"pdf":{{"text":"{multiline_pdf}","pageCount":50,"activePage":45}}}},"provider":{{"kind":"ollama","model":"qwen3"}}}}"#
    );
    let (ml_res, ml_r) = dispatch_raw_llm(&ml_req, &openai_text_reply("[45쪽] 확인"));
    ml_res.expect("multiline oversized activePage pdf ok");
    let ml_body = &ml_r[0].body_json;
    assert!(ml_body.contains("[앞 "), "preceding page skipped");
    assert!(ml_body.contains("[45쪽] "), "activePage=45 kept");
    assert!(!ml_body.contains("[44쪽]"), "earlier huge page 44 skipped");

    // SLIDE omits empty notes and default titleAndContent layout
    let slide_req = r#"{"action":"copilot_run","mode":"ask","module":"SLIDE","prompt":"점검","context":{"slide":{"activeIndex":0,"slides":[{"title":"표지","bullets":["핵심"],"notes":"","layout":"titleAndContent","elements":[]}]}},"provider":{"kind":"ollama","model":"qwen3"}}"#;
    let (slide_res, slide_r) = dispatch_raw_llm(slide_req, &openai_text_reply("좋습니다"));
    slide_res.expect("slide compact ok");
    assert!(!slide_r[0].body_json.contains("노트="));
    assert!(!slide_r[0].body_json.contains("레이아웃=titleAndContent"));
}

#[test]
fn lightweight_ask_streaming_schema_slimming_and_emit_reply_streaming() {
    // 1. lightweight request omits tools, caps at 96 tokens, disables thinking/reasoning
    let lw_req = r#"{
        "action":"copilot_run","mode":"ask","module":"DOC","lightweight":true,"prompt":"다음 한 줄 이어쓰기",
        "context":{"doc":{"paragraphs":["오늘 보고서 요약은 "]}},
        "provider":{"kind":"google","baseUrl":"https://93.184.216.34/v1beta","model":"gemini-3-flash","apiKey":"AIza-test","reasoningEffort":"high"}
    }"#;
    let (lw_res, lw_reqs) = dispatch_raw_llm(
        lw_req,
        r#"{"candidates":[{"content":{"parts":[{"text":"매출 성장입니다."}]}}]}"#,
    );
    lw_res.expect("lw google ok");
    assert!(!lw_reqs[0].body_json.contains("\"tools\""));
    assert!(lw_reqs[0].body_json.contains(r#""maxOutputTokens":96"#));
    assert!(!lw_reqs[0].body_json.contains("thinkingConfig"));

    // 2. Read-only PDF with official OpenAI exposes search_pdf_pages ONLY (no emit_copilot_turn) & accepts plain-text stream
    let pdf_ask = r#"{"action":"copilot_run","requestId":"r-pdf-stream","stream":true,"mode":"ask","module":"PDF","prompt":"매출?","context":{"pdf":{"text":"[1쪽] 매출 100","pageCount":1}},"provider":{"kind":"openai","baseUrl":"https://api.openai.com/v1","model":"gpt-4o","apiKey":"sk-test"}}"#;
    let (pdf_res, pdf_reqs, pdf_pieces) = dispatch_stream_chunks(
        pdf_ask,
        &[
            "data: {\"choices\":[{\"delta\":{\"content\":\"매출은 \"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"100입니다.\"}}]}\n\n",
            "data: [DONE]\n\n",
        ],
    );
    assert!(pdf_reqs[0].body_json.contains("search_pdf_pages"));
    assert!(!pdf_reqs[0].body_json.contains("emit_copilot_turn"));
    assert_eq!(pdf_pieces.join(""), "매출은 100입니다.");
    assert_eq!(
        StrictJsonValue::get_str(&parse_obj(&pdf_res.unwrap()), "reply"),
        Some("매출은 100입니다.")
    );

    // 3. Edit schema removes reasoning & locations; SLIDE single $defs; command specializes schema
    let slide_edit = r#"{"action":"copilot_run","mode":"edit","module":"SLIDE","prompt":"개체 추가","context":{"slide":{"activeIndex":0,"slides":[{"title":"t","bullets":[]}]}},"provider":{"kind":"openai","baseUrl":"https://api.openai.com/v1","model":"gpt-4o","apiKey":"sk-test"}}"#;
    let (se_res, se_reqs) = dispatch_raw_llm(
        slide_edit,
        &openai_text_reply(r#"{"reply":"ok","operations":[]}"#),
    );
    se_res.expect("slide edit ok");
    assert!(!se_reqs[0].body_json.contains(r#""reasoning":"#));
    assert!(!se_reqs[0].body_json.contains(r#""locations":"#));
    assert!(se_reqs[0].body_json.contains(r#""$defs":{"SlideEl""#));
    assert_eq!(
        se_reqs[0].body_json.matches("shapeType").count(),
        1,
        "SLIDE_ELEMENT_SCHEMA not duplicated"
    );

    // 4. Streaming emit_copilot_turn tool_calls streams `"reply"` unescaped & PII-restored in real time
    let edit_pii = r#"{
        "action":"copilot_run","requestId":"r-edit-stream","stream":true,"mode":"edit","module":"DOC","prompt":"이름 정리","command":"fix_spelling",
        "selection":{"paragraphIndex":0,"paragraphEnd":0},
        "context":{"doc":{"paragraphs":["연락처 alice@corp.kr 확인 😀"]}},
        "provider":{"kind":"openai","baseUrl":"https://api.openai.com/v1","model":"gpt-4o","apiKey":"sk-test","reasoningEffort":"high"}
    }"#;
    let sse_tool_chunks = [
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"name\":\"emit_copilot_turn\",\"arguments\":\"{\\\"reply\\\":\\\"고친 \\\\uD83D\"}}]}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\\uDE00 [PII_EMA\"}}]}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"IL_1]\\\",\\\"operations\\\":[{\\\"kind\\\":\\\"doc.replace_paragraphs\\\",\\\"replacements\\\":[{\\\"index\\\":0,\\\"text\\\":\\\"연락처 [PII_EMAIL_1] 확인\\\"}]}]}\"}}]}}]}\n\n",
        "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":300,\"completion_tokens\":40,\"prompt_tokens_details\":{\"cached_tokens\":200}}}\n\n",
        "data: [DONE]\n\n",
    ];
    let (e_res, e_reqs, e_pieces) = dispatch_stream_chunks(edit_pii, &sse_tool_chunks);
    // Standalone FixSpelling clamps reasoningEffort from high -> low and specializes schema
    assert!(e_reqs[0]
        .body_json
        .contains(r#""enum":["doc.replace_paragraphs"]"#));
    assert!(!e_reqs[0].body_json.contains("doc.append"));
    let joined_stream = e_pieces.join("");
    assert_eq!(
        joined_stream, "고친 😀 alice@corp.kr",
        "emit_copilot_turn reply streamed live with surrogate + PII restore"
    );
    let e_obj = parse_obj(&e_res.unwrap());
    assert_eq!(
        StrictJsonValue::get_str(&e_obj, "reply"),
        Some("고친 😀 alice@corp.kr")
    );

    // Standalone selection command clamps OpenAI gpt-5 reasoning_effort high -> low when no history, preserves high with history
    let cmd_alone = r#"{"action":"copilot_run","mode":"edit","module":"DOC","prompt":"맞춤법","command":"fix_spelling","selection":{"paragraphIndex":0,"paragraphEnd":0},"context":{"doc":{"paragraphs":["문장"]}},"provider":{"kind":"openai","baseUrl":"https://api.openai.com/v1","model":"gpt-5","apiKey":"sk-test","reasoningEffort":"high"}}"#;
    let (_, ca_reqs) = dispatch_raw_llm(
        cmd_alone,
        &openai_text_reply(r#"{"reply":"ok","operations":[]}"#),
    );
    assert!(ca_reqs[0].body_json.contains(r#""reasoning_effort":"low""#));

    let cmd_with_hist = r#"{"action":"copilot_run","mode":"edit","module":"DOC","prompt":"맞춤법","command":"fix_spelling","history":[{"role":"user","text":"이전"}],"selection":{"paragraphIndex":0,"paragraphEnd":0},"context":{"doc":{"paragraphs":["문장"]}},"provider":{"kind":"openai","baseUrl":"https://api.openai.com/v1","model":"gpt-5","apiKey":"sk-test","reasoningEffort":"high"}}"#;
    let (_, ch_reqs) = dispatch_raw_llm(
        cmd_with_hist,
        &openai_text_reply(r#"{"reply":"ok","operations":[]}"#),
    );
    assert!(ch_reqs[0]
        .body_json
        .contains(r#""reasoning_effort":"high""#));
}

#[test]
fn doc_replace_span_validation_pii_and_malicious_inputs_fail_closed() {
    // 1. Valid unique span replacement restores PII before exact check; ambiguous / empty / OOB dropped
    let req = r#"{"action":"copilot_run","mode":"edit","module":"DOC","prompt":"부분 교체","context":{"doc":{"paragraphs":["대표 메일 bob@corp.kr 로 연락 주세요.","중복 단어와 중복 단어"]}},"provider":{"kind":"openai","baseUrl":"https://93.184.216.34/v1","model":"gpt-4o","apiKey":"sk-test"}}"#;
    let reply = openai_text_reply(
        r#"{"reply":"수정","operations":[
            {"kind":"doc.replace_span","index":0,"search":"[PII_EMAIL_1] 로","replace":"[PII_EMAIL_1]로"},
            {"kind":"doc.replace_span","index":1,"search":"중복 단어","replace":"단일"},
            {"kind":"doc.replace_span","index":0,"search":"   ","replace":"x"},
            {"kind":"doc.replace_span","index":99,"search":"없음","replace":"x"}
        ]}"#,
    );
    let (res, _) = dispatch_raw_llm(req, &reply);
    let obj = parse_obj(&res.unwrap());
    assert_eq!(
        StrictJsonValue::get_field(&obj, "droppedOperations").and_then(StrictJsonValue::as_u64),
        Some(3)
    );
    let ops = StrictJsonValue::get_field(&obj, "operations")
        .and_then(StrictJsonValue::as_array)
        .unwrap();
    assert_eq!(ops.len(), 1);
    let op0 = ops[0].as_object().unwrap();
    assert_eq!(
        StrictJsonValue::get_str(op0, "kind"),
        Some("doc.replace_span")
    );
    assert_eq!(
        StrictJsonValue::get_str(op0, "search"),
        Some("bob@corp.kr 로")
    );
    assert_eq!(
        StrictJsonValue::get_str(op0, "replace"),
        Some("bob@corp.kr로")
    );

    // 2. Hostile / oversized inputs fail closed
    let bad_para = r#"{"action":"copilot_run","mode":"ask","module":"DOC","prompt":"x","context":{"doc":{"paragraphs":[123]}},"provider":{"kind":"ollama"}}"#;
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("run_smart_assistant", bad_para),
        Err(EasyOfficeError::InvalidArgument(_))
    ));
    let big_key = format!(
        r#"{{"action":"copilot_run","mode":"ask","module":"DOC","prompt":"x","promptCacheKey":"{}","context":{{"doc":{{"paragraphs":["a"]}}}},"provider":{{"kind":"ollama"}}}}"#,
        "k".repeat(600)
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("run_smart_assistant", &big_key),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
    let lw_cmd = r#"{"action":"copilot_run","mode":"ask","module":"DOC","lightweight":true,"prompt":"x","context":{"doc":{"paragraphs":["a"]}},"provider":{"kind":"openai","baseUrl":"https://api.openai.com/v1","model":"gpt-5","apiKey":"sk-test","reasoningEffort":"high"}}"#;
    let (_, lw_reqs) = dispatch_raw_llm(lw_cmd, &openai_text_reply("b"));
    assert!(!lw_reqs[0].body_json.contains("reasoning_effort"));
    assert!(lw_reqs[0]
        .body_json
        .contains(r#""max_completion_tokens":96"#));

    // Real byte measurements across key request profiles
    let edit_cmd = r#"{"action":"copilot_run","mode":"edit","module":"DOC","prompt":"맞춤법","command":"fix_spelling","selection":{"paragraphIndex":0,"paragraphEnd":0},"context":{"doc":{"paragraphs":["테스트 문장"]}},"provider":{"kind":"openai","baseUrl":"https://api.openai.com/v1","model":"gpt-4o","apiKey":"sk-test"}}"#;
    let (_, edit_cmd_reqs) = dispatch_raw_llm(
        edit_cmd,
        &openai_text_reply(r#"{"reply":"ok","operations":[]}"#),
    );
    let ask_short = r#"{"action":"copilot_run","mode":"ask","module":"DOC","prompt":"요약","context":{"doc":{"paragraphs":["테스트 문장"]}},"provider":{"kind":"openai","baseUrl":"https://api.openai.com/v1","model":"gpt-4o","apiKey":"sk-test"}}"#;
    let (_, ask_short_reqs) = dispatch_raw_llm(ask_short, &openai_text_reply("요약"));
    let lw_short = r#"{"action":"copilot_run","mode":"ask","module":"DOC","lightweight":true,"prompt":"다음 한 줄","context":{"doc":{"paragraphs":["테스트 문장"]}},"provider":{"kind":"openai","baseUrl":"https://api.openai.com/v1","model":"gpt-4o","apiKey":"sk-test"}}"#;
    let (_, lw_short_reqs) = dispatch_raw_llm(lw_short, &openai_text_reply("이어짐"));
    eprintln!(
        "BENCHMARK_BYTES edit_cmd={} ask_short={} lightweight_ghost={}",
        edit_cmd_reqs[0].body_json.len(),
        ask_short_reqs[0].body_json.len(),
        lw_short_reqs[0].body_json.len()
    );
    assert!(lw_short_reqs[0].body_json.len() < ask_short_reqs[0].body_json.len());
    assert!(ask_short_reqs[0].body_json.len() < edit_cmd_reqs[0].body_json.len());
}

#[test]
fn regression_fixes_entities_formulas_surrogate_compatible_tools_and_usage() {
    // 1. doc.replace_span unescapes HTML entities, keeps math '<', and ignores href attributes in unique check
    let ent_req = r#"{"action":"copilot_run","mode":"edit","module":"DOC","prompt":"치환","context":{"doc":{"paragraphs":[
        "<strong>A &amp; B</strong> 문서",
        "조건 2 < 3 이고 target_val 확인",
        "<a href=\"https://example.com\">visit example.com</a>",
        "<strong>dup</strong> and dup"
    ]}},"provider":{"kind":"openai","baseUrl":"https://api.openai.com/v1","model":"gpt-4o","apiKey":"sk-test"}}"#;
    let ent_reply = openai_text_reply(
        r#"{"reply":"완료","operations":[
            {"kind":"doc.replace_span","index":0,"search":"A & B","replace":"A와 B"},
            {"kind":"doc.replace_span","index":1,"search":"target_val","replace":"next_val"},
            {"kind":"doc.replace_span","index":2,"search":"example.com","replace":"site.kr"},
            {"kind":"doc.replace_span","index":3,"search":"dup","replace":"one"}
        ]}"#,
    );
    let (ent_res, _) = dispatch_raw_llm(ent_req, &ent_reply);
    let ent_obj = parse_obj(&ent_res.unwrap());
    assert_eq!(
        StrictJsonValue::get_field(&ent_obj, "droppedOperations").and_then(StrictJsonValue::as_u64),
        Some(1),
        "duplicate visible text is dropped while entity/math/href spans succeed"
    );
    assert_eq!(
        StrictJsonValue::get_field(&ent_obj, "operations")
            .and_then(StrictJsonValue::as_array)
            .map(<[StrictJsonValue]>::len),
        Some(3)
    );

    // 2. SHEET $A{r} column-absolute, sheet-qualified =S1!A{r} / ='Sheet1'!B{r}, TSV & BASE newline sanitization
    let mut sheet_cells = vec![
        r#"{"coord":"A1","raw":"기초","value":"기초"}"#.to_string(),
        r#"{"coord":"B1","raw":"단가","value":"단가"}"#.to_string(),
        r#"{"coord":"C1","raw":"합","value":"합"}"#.to_string(),
        r#"{"coord":"D1","raw":"참조","value":"참조"}"#.to_string(),
        r#"{"coord":"E1","raw":"시트","value":"시트"}"#.to_string(),
    ];
    for r in 2..=4 {
        sheet_cells.push(format!(r#"{{"coord":"A{r}","raw":"10","value":"10"}}"#));
        sheet_cells.push(format!(
            r#"{{"coord":"B{r}","raw":"메모{r}\n둘째줄\t끝","value":"메모{r}\n둘째줄\t끝"}}"#
        ));
        sheet_cells.push(format!(
            r#"{{"coord":"C{r}","raw":"=$A{r}*10","value":"100"}}"#
        ));
        sheet_cells.push(format!(
            r#"{{"coord":"D{r}","raw":"=S1!A{r}","value":"10"}}"#
        ));
        sheet_cells.push(format!(
            r#"{{"coord":"E{r}","raw":"='Sheet1'!A{r}","value":"10"}}"#
        ));
    }
    let s_req = format!(
        r#"{{"action":"copilot_run","mode":"ask","module":"SHEET","prompt":"검토","context":{{"sheet":{{"name":"S1","cells":[{}]}}}},"provider":{{"kind":"openai","baseUrl":"https://api.openai.com/v1","model":"gpt-4o","apiKey":"sk-test"}}}}"#,
        sheet_cells.join(",")
    );
    let (_, s_reqs) = dispatch_raw_llm(&s_req, &openai_text_reply("ok"));
    let s_body = &s_reqs[0].body_json;
    assert!(
        s_body.contains("동형수식: C2:C4 => =$A{r}*10"),
        "column-abs ref normalized: {s_body}"
    );
    assert!(
        s_body.contains("동형수식: D2:D4 => =S1!A{r}"),
        "short sheet-qualified ref preserved: {s_body}"
    );
    assert!(
        s_body.contains("동형수식: E2:E4 => ='Sheet1'!A{r}"),
        "quoted sheet ref preserved: {s_body}"
    );
    assert!(
        !s_body.contains("메모2\\n둘째줄"),
        "TSV cell newlines sanitized: {s_body}"
    );
    assert!(
        s_body.contains("메모2 둘째줄 끝"),
        "TSV cell stays single line: {s_body}"
    );

    let base_req = r#"{"action":"copilot_run","mode":"ask","module":"BASE","prompt":"확인","context":{"base":{"tables":["T"],"table":"T","totalRecords":1,"fields":[{"name":"메모","kind":"text"}],"records":[["첫줄\n둘째|줄"]]}},"provider":{"kind":"openai","baseUrl":"https://api.openai.com/v1","model":"gpt-4o","apiKey":"sk-test"}}"#;
    let (_, base_reqs) = dispatch_raw_llm(base_req, &openai_text_reply("ok"));
    assert!(
        base_reqs[0].body_json.contains("  [0] 첫줄 둘째/줄"),
        "BASE pipe & newline sanitized: {}",
        base_reqs[0].body_json
    );

    // 3. Compatible (non-official) provider in read-only mode gets JSON toolCalls instructions
    let ollama_pdf = r#"{"action":"copilot_run","mode":"ask","module":"PDF","prompt":"찾기","context":{"pdf":{"text":"[1쪽] 요약","pageCount":3}},"provider":{"kind":"ollama","model":"qwen3"}}"#;
    let (_, ol_reqs) = dispatch_raw_llm(ollama_pdf, &openai_text_reply("확인"));
    assert!(ol_reqs[0].body_json.contains("search_pdf_pages"));
    assert!(ol_reqs[0].body_json.contains("toolCalls"));
    let oai_pdf = r#"{"action":"copilot_run","mode":"ask","module":"PDF","prompt":"찾기","context":{"pdf":{"text":"[1쪽] 요약","pageCount":3}},"provider":{"kind":"openai","baseUrl":"https://api.openai.com/v1","model":"gpt-4o","apiKey":"sk-test"}}"#;
    let (_, oa_reqs) = dispatch_raw_llm(oai_pdf, &openai_text_reply("확인"));
    assert!(oa_reqs[0].body_json.contains("search_pdf_pages"));
    assert!(!oa_reqs[0].body_json.contains("toolCalls"));

    // 4. Unpaired high surrogate inside streamed emit_copilot_turn reply does not stall following characters
    let edit_req = r#"{"action":"copilot_run","requestId":"r-surr","stream":true,"mode":"edit","module":"DOC","prompt":"x","context":{"doc":{"paragraphs":["본문"]}},"provider":{"kind":"openai","baseUrl":"https://api.openai.com/v1","model":"gpt-4o","apiKey":"sk-test"}}"#;
    let surr_chunks = [
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"name\":\"emit_copilot_turn\",\"arguments\":\"{\\\"reply\\\":\\\"앞\\\\uD83D\"}}]}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"!뒤\\\",\\\"operations\\\":[]}\"}}]}}]}\n\n",
        "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":50,\"completion_tokens\":10,\"prompt_cache_hit_tokens\":40}}\n\n",
        "data: [DONE]\n\n",
    ];
    let (surr_res, _, surr_pieces) = dispatch_stream_chunks(edit_req, &surr_chunks);
    assert_eq!(surr_pieces.join(""), "앞\u{FFFD}!뒤");
    let surr_obj = parse_obj(&surr_res.unwrap());
    let u = StrictJsonValue::get_field(&surr_obj, "usage")
        .and_then(StrictJsonValue::as_object)
        .unwrap();
    assert_eq!(
        StrictJsonValue::get_field(u, "cachedTokens").and_then(StrictJsonValue::as_u64),
        Some(40)
    );
}
