//! 코파일럿 전 모듈 개편 통합 검증 (PD-360):
//! 제공자 프로토콜 수리, PD-304 슬라이드·문서 범위·표 왕복 검증, 교차 모듈 차단,
//! 네이티브 도구 호출(OpenAI·Anthropic·Google), 확장 연산(SHEET·BASE·BOARD·조회 도구),
//! 클라우드 전송 전 로컬 개인정보 치환·복원(조각 경계 스트리밍 포함).

use easyoffice_app::{
    ChunkSink, CopilotLlmRequest, CopilotStreamEvent, EasyOfficeIpcRouter, ShellWriters,
};
use easyoffice_formats::{EasyOfficeError, StrictJsonValue};
use std::sync::Mutex;

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
                return Err("중단됨".to_string());
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
    let res = EasyOfficeIpcRouter::dispatch_for_owner_with_shell_writers(
        "run_smart_assistant",
        payload,
        "",
        writers,
    );
    (
        res,
        seen_reqs.into_inner().unwrap(),
        seen_pieces.into_inner().unwrap(),
    )
}

fn parse_obj(raw: &str) -> Vec<(String, StrictJsonValue)> {
    StrictJsonValue::parse(raw)
        .expect("valid JSON response")
        .as_object()
        .expect("JSON object")
        .to_vec()
}

fn ops_of(o: &[(String, StrictJsonValue)]) -> Vec<StrictJsonValue> {
    StrictJsonValue::get_field(o, "operations")
        .and_then(StrictJsonValue::as_array)
        .expect("operations array")
        .to_vec()
}

fn tools_of(o: &[(String, StrictJsonValue)]) -> Vec<StrictJsonValue> {
    StrictJsonValue::get_field(o, "toolCalls")
        .and_then(StrictJsonValue::as_array)
        .map(<[StrictJsonValue]>::to_vec)
        .unwrap_or_default()
}

#[test]
fn reasoning_models_omit_temperature_and_google_uses_thinking_level_or_budget() {
    for model in ["o1", "o3-mini", "o4-mini", "gpt-5"] {
        let req = format!(
            r#"{{"action":"copilot_run","mode":"ask","module":"DOC","prompt":"요약","provider":{{"kind":"openai","baseUrl":"https://93.184.216.34/v1","model":"{model}","apiKey":"sk-test"}}}}"#
        );
        let (res, reqs) = dispatch_raw_llm(
            &req,
            &openai_text_reply(r#"{"reply":"완료","operations":[]}"#),
        );
        assert!(res.is_ok(), "{model} should succeed");
        assert!(
            !reqs[0].body_json.contains("\"temperature\""),
            "reasoning model {model} must not inject temperature: {}",
            reqs[0].body_json
        );
    }

    let non_reasoning = r#"{"action":"copilot_run","mode":"ask","module":"DOC","prompt":"요약","provider":{"kind":"openai","baseUrl":"https://93.184.216.34/v1","model":"gpt-4o","apiKey":"sk-test"}}"#;
    let (_, gpt4o_reqs) = dispatch_raw_llm(
        non_reasoning,
        &openai_text_reply(r#"{"reply":"완료","operations":[]}"#),
    );
    assert!(gpt4o_reqs[0].body_json.contains("\"temperature\":0.2"));

    // Gemini 3 / 4 uses thinkingLevel and omits forced temperature: 0.2
    let google_resp = r#"{"candidates":[{"content":{"parts":[{"text":"{\"reply\":\"제미나이 답\",\"operations\":[]}"}]}}]}"#;
    let gemini3 = r#"{"action":"copilot_run","mode":"ask","module":"DOC","prompt":"요약","provider":{"kind":"google","baseUrl":"https://93.184.216.34/v1beta","model":"gemini-3-pro","apiKey":"AIza-test","reasoningEffort":"high"}}"#;
    let (res3, reqs3) = dispatch_raw_llm(gemini3, google_resp);
    assert!(res3.is_ok());
    assert!(
        reqs3[0].body_json.contains(r#""thinkingLevel":"high""#)
            && reqs3[0].body_json.contains(r#""thinkingConfig":"#),
        "Gemini 3 must send thinkingLevel: {}",
        reqs3[0].body_json
    );
    assert!(
        !reqs3[0].body_json.contains("\"temperature\""),
        "Gemini 3 must not force low temperature: {}",
        reqs3[0].body_json
    );

    // Gemini 2.5 adds budget to maxOutputTokens (8192 + 4096 = 12288) and uses thinkingBudget
    let gemini25 = r#"{"action":"copilot_run","mode":"ask","module":"DOC","prompt":"요약","provider":{"kind":"google","baseUrl":"https://93.184.216.34/v1beta","model":"gemini-2.5-pro","apiKey":"AIza-test","reasoningEffort":"medium"}}"#;
    let (res25, reqs25) = dispatch_raw_llm(gemini25, google_resp);
    assert!(res25.is_ok());
    assert!(
        reqs25[0].body_json.contains(r#""maxOutputTokens":12288"#),
        "Gemini 2.5 must expand maxOutputTokens by budget: {}",
        reqs25[0].body_json
    );
    assert!(reqs25[0].body_json.contains(r#""thinkingBudget":4096"#));
    assert!(!reqs25[0].body_json.contains("\"temperature\""));
}

#[test]
fn google_compact_json_array_fallback_parses_both_streaming_and_buffered() {
    let compact_array = r#"[{"candidates":[{"content":{"parts":[{"text":"{\"reply\":\"배열 첫 조각, "}]}}]},{"candidates":[{"content":{"parts":[{"text":"둘째 조각\",\"operations\":[{\"op\":\"doc.append\",\"text\":\"새 문단\"}]}"}]}}]}]"#;
    let stream_req = r#"{"action":"copilot_run","mode":"edit","module":"DOC","prompt":"추가","stream":true,"requestId":"g-arr-1","provider":{"kind":"google","baseUrl":"https://93.184.216.34/v1beta","model":"gemini-3-flash","apiKey":"AIza-test"}}"#;
    let (res, reqs, pieces) = dispatch_stream_chunks(stream_req, &[compact_array]);
    let o = parse_obj(&res.expect("google compact array stream ok"));
    assert_eq!(
        StrictJsonValue::get_str(&o, "reply"),
        Some("배열 첫 조각, 둘째 조각")
    );
    assert_eq!(ops_of(&o).len(), 1);
    assert!(
        !pieces.is_empty(),
        "stream pieces emitted from compact JSON array"
    );
    assert!(reqs[0].url.contains(":streamGenerateContent"));

    // Non-streaming fallback when a proxy returns a JSON array on generateContent
    let plain_req = r#"{"action":"copilot_run","mode":"edit","module":"DOC","prompt":"추가","provider":{"kind":"google","baseUrl":"https://93.184.216.34/v1beta","model":"gemini-3-flash","apiKey":"AIza-test"}}"#;
    let (plain_res, _) = dispatch_raw_llm(plain_req, compact_array);
    let plain_obj = parse_obj(&plain_res.expect("google non-SSE array fallback ok"));
    assert_eq!(
        StrictJsonValue::get_str(&plain_obj, "reply"),
        Some("배열 첫 조각, 둘째 조각")
    );
    assert_eq!(ops_of(&plain_obj).len(), 1);
}

#[test]
fn anthropic_official_host_attaches_ephemeral_cache_control_only_for_official_domain() {
    let anthropic_resp =
        r#"{"content":[{"type":"text","text":"{\"reply\":\"캐시 확인\",\"operations\":[]}"}]}"#;
    let official_proxy_req = r#"{"action":"copilot_run","mode":"ask","module":"DOC","prompt":"확인","provider":{"kind":"anthropic","baseUrl":"https://93.184.216.34/v1","model":"claude-sonnet-4-5","apiKey":"sk-ant-test"}}"#;
    let (_, custom_reqs) = dispatch_raw_llm(official_proxy_req, anthropic_resp);
    assert!(
        !custom_reqs[0].body_json.contains("cache_control"),
        "custom IP/proxy should keep string system field"
    );

    // Build body directly via public module helper through official host check
    let parsed_entries = StrictJsonValue::parse(
        r#"{"action":"copilot_run","mode":"ask","module":"DOC","prompt":"확인","provider":{"kind":"anthropic","baseUrl":"https://api.anthropic.com","model":"claude-sonnet-4-5","apiKey":"sk-ant-test"}}"#,
    )
    .unwrap();
    let parsed_req =
        easyoffice_app::copilot::types::parse_run_request(parsed_entries.as_object().unwrap())
            .unwrap();
    let official_body = easyoffice_app::copilot::providers::build_provider_chat_body(
        &parsed_req,
        "시스템 지시문",
        "사용자 질문",
        false,
    )
    .unwrap();
    assert!(
        official_body.contains(r#""cache_control":{"type":"ephemeral"}"#),
        "official api.anthropic.com must attach ephemeral cache_control: {official_body}"
    );
}

#[test]
fn pd304_slide_elements_doc_range_and_table_roundtrip_through_ipc_router() {
    let slide_req = r#"{
        "action":"copilot_run",
        "mode":"edit",
        "module":"SLIDE",
        "prompt":"발표 장표와 개체를 추가해 주세요",
        "provider":{"kind":"remote_https","baseUrl":"https://93.184.216.34/v1","model":"gpt-4o","apiKey":"sk-test"},
        "context":{
            "slide":{
                "activeIndex":0,
                "aspect":"16:9",
                "slides":[{
                    "index":0,
                    "title":"개요",
                    "bullets":["항목 1"],
                    "notes":"노트",
                    "layout":"twoColumn",
                    "elements":[{"id":"el-1","kind":"textbox","x":40,"y":60,"w":200,"h":80,"text":"기존 상자"}]
                }]
            }
        }
    }"#;
    let slide_model_reply = r##"{
        "reply":"슬라이드와 개체를 구성했습니다.",
        "operations":[
            {
                "op":"slide.add",
                "title":"실적 요약",
                "body":"핵심 지표 정리",
                "layout":"titleAndContent",
                "transition":"fade",
                "elements":[{"id":"tb-1","kind":"textbox","x":60,"y":120,"w":400,"h":120,"text":"매출 20% 증가","fontSizePt":20}]
            },
            {
                "op":"slide.add_element",
                "index":0,
                "element":{"id":"ch-1","kind":"chart","x":480,"y":120,"w":360,"h":240,"chartType":"bar","title":"분기별 추이","categories":["1분기","2분기"],"series":[{"name":"매출","values":[120,160]}]}
            },
            {
                "op":"slide.add_element",
                "index":0,
                "element":{"id":"bad-oob","kind":"shape","x":-20,"y":10,"w":100,"h":100}
            }
        ]
    }"##;
    let (res, reqs) = dispatch_raw_llm(slide_req, &openai_text_reply(slide_model_reply));
    assert!(
        reqs[0].body_json.contains("레이아웃=twoColumn") && reqs[0].body_json.contains("기존 상자"),
        "slide layout and elements must reach prompt context: {}",
        reqs[0].body_json
    );
    let o = parse_obj(&res.expect("slide IPC roundtrip ok"));
    let ops = ops_of(&o);
    assert_eq!(
        ops.len(),
        2,
        "valid slide.add and slide.add_element pass while OOB element is dropped"
    );
    assert_eq!(
        StrictJsonValue::get_field(&o, "droppedOperations").and_then(StrictJsonValue::as_u64),
        Some(1)
    );
    let first_op = ops[0].as_object().unwrap();
    assert_eq!(
        StrictJsonValue::get_str(first_op, "kind"),
        Some("slide.add")
    );
    assert_eq!(
        StrictJsonValue::get_str(first_op, "layout"),
        Some("titleAndContent")
    );
    assert_eq!(
        StrictJsonValue::get_str(first_op, "transition"),
        Some("fade")
    );
    let bullets = StrictJsonValue::get_field(first_op, "bullets")
        .and_then(StrictJsonValue::as_array)
        .unwrap();
    assert_eq!(bullets[0].as_str(), Some("핵심 지표 정리"));
    let second_op = ops[1].as_object().unwrap();
    assert_eq!(
        StrictJsonValue::get_str(second_op, "kind"),
        Some("slide.add_element")
    );

    // Document range replacement + table block + visualize_table command
    let doc_req = r#"{
        "action":"copilot_run",
        "mode":"edit",
        "module":"DOC",
        "command":"visualize_table",
        "prompt":"표로 시각화",
        "selection":{"paragraphIndex":1,"paragraphEnd":2},
        "provider":{"kind":"remote_https","baseUrl":"https://93.184.216.34/v1","model":"gpt-4o","apiKey":"sk-test"},
        "context":{"doc":{"paragraphs":["머리말","항목 A: 100","항목 B: 200","맺음말"]}}
    }"#;
    let doc_model_reply = r#"{
        "reply":"선택 문단을 표로 변환했습니다.",
        "operations":[
            {
                "op":"doc.replace_paragraphs",
                "startIndex":1,
                "endIndex":2,
                "blocks":[
                    {"kind":"table","headerRow":true,"rows":[["항목","수치"],["항목 A","100"],["항목 B","200"]]}
                ]
            }
        ]
    }"#;
    let (doc_res, _) = dispatch_raw_llm(doc_req, &openai_text_reply(doc_model_reply));
    let doc_obj = parse_obj(&doc_res.expect("doc visualize_table ok"));
    let doc_ops = ops_of(&doc_obj);
    assert_eq!(doc_ops.len(), 1);
    let op0 = doc_ops[0].as_object().unwrap();
    assert_eq!(
        StrictJsonValue::get_str(op0, "kind"),
        Some("doc.replace_paragraphs")
    );
    assert_eq!(
        StrictJsonValue::get_field(op0, "startIndex").and_then(StrictJsonValue::as_u64),
        Some(1)
    );
    assert_eq!(
        StrictJsonValue::get_field(op0, "endIndex").and_then(StrictJsonValue::as_u64),
        Some(2)
    );
}

#[test]
fn cross_module_operations_are_rejected_even_when_command_is_none() {
    let mixed_reply = r#"{
        "reply":"혼합 응답",
        "operations":[
            {"op":"doc.append","text":"문서 추가"},
            {"op":"sheet.write","sheet":"Sheet1","cells":[{"coord":"A1","raw":"42"}]},
            {"op":"slide.add","title":"새 슬라이드"},
            {"op":"base.add_field","name":"상태","fieldType":"text"},
            {"op":"board.add_graph","nodes":[{"id":"n1","text":"시작"},{"id":"n2","text":"끝"}],"edges":[{"from":"n1","to":"n2"}]}
        ]
    }"#;
    for (module, expected_op) in [
        ("DOC", "doc.append"),
        ("HWP", "doc.append"),
        ("SHEET", "sheet.write"),
        ("SLIDE", "slide.add"),
        ("BASE", "base.add_field"),
        ("BOARD", "board.add_graph"),
    ] {
        let req = format!(
            r#"{{"action":"copilot_run","mode":"edit","module":"{module}","prompt":"편집","provider":{{"kind":"remote_https","baseUrl":"https://93.184.216.34/v1","model":"gpt-4o","apiKey":"sk-test"}}}}"#
        );
        let (res, _) = dispatch_raw_llm(&req, &openai_text_reply(mixed_reply));
        let o = parse_obj(&res.unwrap());
        let ops = ops_of(&o);
        assert_eq!(ops.len(), 1, "{module} must keep only its own operation");
        assert_eq!(
            StrictJsonValue::get_str(ops[0].as_object().unwrap(), "kind"),
            Some(expected_op)
        );
        assert_eq!(
            StrictJsonValue::get_field(&o, "droppedOperations").and_then(StrictJsonValue::as_u64),
            Some(4)
        );
    }
}

#[test]
fn native_tool_calling_works_across_openai_anthropic_and_google() {
    // 1. OpenAI native tool_calls (read tool + edit op)
    let openai_tool_resp = r#"{
        "choices":[{
            "message":{
                "role":"assistant",
                "content":"시트 범위를 먼저 조회합니다.",
                "tool_calls":[
                    {
                        "id":"call_1",
                        "type":"function",
                        "function":{
                            "name":"read_sheet_range",
                            "arguments":"{\"sheet\":\"Sheet1\",\"range\":\"A1:D20\"}"
                        }
                    }
                ]
            }
        }]
    }"#;
    let sheet_req = r#"{"action":"copilot_run","mode":"edit","module":"SHEET","prompt":"A1:D20 확인","provider":{"kind":"openai","baseUrl":"https://93.184.216.34/v1","model":"gpt-4o","apiKey":"sk-test"}}"#;
    let (res_oai, reqs_oai) = dispatch_raw_llm(sheet_req, openai_tool_resp);
    assert!(reqs_oai[0].body_json.contains("\"tools\":["));
    let obj_oai = parse_obj(&res_oai.unwrap());
    let tcs_oai = tools_of(&obj_oai);
    assert_eq!(tcs_oai.len(), 1);
    assert_eq!(
        StrictJsonValue::get_str(tcs_oai[0].as_object().unwrap(), "name"),
        Some("read_sheet_range")
    );

    // 2. Anthropic native tool_use via streaming SSE deltas
    let anthropic_sse = [
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"피벗 테이블을 만듭니다.\"}}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu_1\",\"name\":\"emit_copilot_turn\",\"input\":{}}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"reply\\\":\\\"피벗 생성\\\",\\\"operations\\\":[{\\\"op\\\":\\\"sheet.pivot\\\",\\\"sourceRange\\\":\\\"A1:C10\\\",\\\"rowField\\\":\\\"지역\\\",\\\"valueField\\\":\\\"매출\\\",\\\"agg\\\":\\\"sum\\\"}]}\"}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    ];
    let anth_req = r#"{"action":"copilot_run","mode":"edit","module":"SHEET","prompt":"피벗 추가","stream":true,"requestId":"anth-tc-1","provider":{"kind":"anthropic","baseUrl":"https://93.184.216.34/v1","model":"claude-sonnet-4-5","apiKey":"sk-ant-test"}}"#;
    let (res_anth, _, _) = dispatch_stream_chunks(anth_req, &anthropic_sse);
    let obj_anth = parse_obj(&res_anth.unwrap());
    let ops_anth = ops_of(&obj_anth);
    assert_eq!(ops_anth.len(), 1);
    assert_eq!(
        StrictJsonValue::get_str(ops_anth[0].as_object().unwrap(), "kind"),
        Some("sheet.pivot")
    );

    // 3. Google native functionCall on PDF read tool
    let google_fc = r#"{
        "candidates":[{
            "content":{
                "parts":[
                    {"functionCall":{"name":"search_pdf_pages","args":{"query":"계약 금액","maxHits":5}}}
                ]
            }
        }]
    }"#;
    let pdf_req = r#"{"action":"copilot_run","mode":"ask","module":"PDF","prompt":"계약 금액이 몇 쪽에 있나요?","provider":{"kind":"google","baseUrl":"https://93.184.216.34/v1beta","model":"gemini-3-flash","apiKey":"AIza-test"}}"#;
    let (res_g, reqs_g) = dispatch_raw_llm(pdf_req, google_fc);
    assert!(reqs_g[0].body_json.contains("\"functionDeclarations\""));
    let obj_g = parse_obj(&res_g.unwrap());
    let tcs_g = tools_of(&obj_g);
    assert_eq!(tcs_g.len(), 1);
    assert_eq!(
        StrictJsonValue::get_str(tcs_g[0].as_object().unwrap(), "name"),
        Some("search_pdf_pages")
    );
}

#[test]
fn extended_sheet_base_and_board_operations_enforce_strict_bounds() {
    // Valid sheet extended ops + rejected excessive row insert (> 64)
    let sheet_reply = r#"{
        "reply":"시트 정리 완료",
        "operations":[
            {"op":"sheet.table","range":"A1:D10","name":"매출표","style":"blue"},
            {"op":"sheet.cond_format","range":"D2:D10","preset":"greaterThan","value1":"1000"},
            {"op":"sheet.sort_filter","action":"sort","range":"A1:D10","colOffset":3,"ascending":false},
            {"op":"sheet.structure","action":"insert_rows","at":2,"count":3},
            {"op":"sheet.structure","action":"insert_rows","at":2,"count":999},
            {"op":"sheet.note","coord":"B2","text":"검토 완료"}
        ]
    }"#;
    let sheet_req = r#"{"action":"copilot_run","mode":"edit","module":"SHEET","prompt":"시트 구성","provider":{"kind":"remote_https","baseUrl":"https://93.184.216.34/v1","model":"gpt-4o","apiKey":"sk-test"}}"#;
    let (s_res, _) = dispatch_raw_llm(sheet_req, &openai_text_reply(sheet_reply));
    let s_obj = parse_obj(&s_res.unwrap());
    assert_eq!(ops_of(&s_obj).len(), 5);
    assert_eq!(
        StrictJsonValue::get_field(&s_obj, "droppedOperations").and_then(StrictJsonValue::as_u64),
        Some(1)
    );

    // Valid board graph vs dangling edge rejection
    let board_reply = r#"{
        "reply":"흐름도 생성",
        "operations":[
            {
                "op":"board.add_graph",
                "direction":"LR",
                "nodes":[{"id":"a","kind":"shape","text":"요청 접수"},{"id":"b","kind":"shape","text":"승인 완료"}],
                "edges":[{"from":"a","to":"b","label":"검토 통과"}]
            },
            {
                "op":"board.add_graph",
                "nodes":[{"id":"x","text":"고립"}],
                "edges":[{"from":"x","to":"missing"}]
            }
        ]
    }"#;
    let board_req = r#"{"action":"copilot_run","mode":"edit","module":"BOARD","prompt":"흐름도","provider":{"kind":"remote_https","baseUrl":"https://93.184.216.34/v1","model":"gpt-4o","apiKey":"sk-test"}}"#;
    let (b_res, _) = dispatch_raw_llm(board_req, &openai_text_reply(board_reply));
    let b_obj = parse_obj(&b_res.unwrap());
    assert_eq!(ops_of(&b_obj).len(), 1);
    assert_eq!(
        StrictJsonValue::get_field(&b_obj, "droppedOperations").and_then(StrictJsonValue::as_u64),
        Some(1)
    );

    // BASE set_view normalizes snake_case `not_empty` -> `notEmpty` and rejects unsupported `gallery` view
    let base_reply = r#"{
        "reply":"보기 설정",
        "operations":[
            {"op":"base.set_view","view":"kanban","filterField":"담당자","filterOp":"not_empty"},
            {"op":"base.set_view","view":"gallery"}
        ]
    }"#;
    let base_req = r#"{"action":"copilot_run","mode":"edit","module":"BASE","prompt":"보기 정리","provider":{"kind":"remote_https","baseUrl":"https://93.184.216.34/v1","model":"gpt-4o","apiKey":"sk-test"}}"#;
    let (base_res, _) = dispatch_raw_llm(base_req, &openai_text_reply(base_reply));
    let base_obj = parse_obj(&base_res.unwrap());
    let base_ops = ops_of(&base_obj);
    assert_eq!(base_ops.len(), 1);
    assert_eq!(
        StrictJsonValue::get_str(base_ops[0].as_object().unwrap(), "filterOp"),
        Some("notEmpty")
    );
    assert_eq!(
        StrictJsonValue::get_field(&base_obj, "droppedOperations")
            .and_then(StrictJsonValue::as_u64),
        Some(1)
    );
}

#[test]
fn cloud_pii_redaction_masks_outbound_payload_and_restores_chunk_split_stream_while_loopback_skips()
{
    // Document contains an existing literal `[PII_EMAIL_1]` to verify collision avoidance (`[PII_EMAIL_2]` etc.)
    let remote_req = r#"{
        "action":"copilot_run",
        "mode":"edit",
        "module":"DOC",
        "prompt":"담당자 minsu.kim@example.co.kr 연락처 010-2345-6789 안내 문단을 다듬어 주세요",
        "stream":true,
        "requestId":"pii-stream-1",
        "provider":{"kind":"openai","baseUrl":"https://93.184.216.34/v1","model":"gpt-4o","apiKey":"sk-test"},
        "context":{
            "doc":{
                "paragraphs":[
                    "기존 토큰 예시 [PII_EMAIL_1] 유지",
                    "고객 주민번호 900101-1234567 및 메일 minsu.kim@example.co.kr 확인"
                ]
            }
        }
    }"#;

    // Stream splits `[PII_EMAIL_2]` and `[PII_PHONE_3]` across SSE chunks, plus includes an unissued `[PII_FAKE_99]`.
    let sse_chunks = [
        "data: {\"choices\":[{\"delta\":{\"content\":\"{\\\"reply\\\":\\\"담당자 [PII_EMA\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"IL_2] (연락처 [PII_PHO\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"NE_3], 미발급 [PII_FAKE_99]) 안내 완료\\\",\\\"operations\\\":[{\\\"op\\\":\\\"doc.append\\\",\\\"text\\\":\\\"문의: [PII_EMAIL_2] / [PII_PHONE_3]\\\"}]}\"}}]}\n\n",
        "data: [DONE]\n\n",
    ];
    let (res, reqs, pieces) = dispatch_stream_chunks(remote_req, &sse_chunks);
    let outbound = &reqs[0].body_json;
    assert!(
        !outbound.contains("minsu.kim@example.co.kr"),
        "raw email must not leave to remote provider: {outbound}"
    );
    assert!(
        !outbound.contains("010-2345-6789"),
        "raw phone must not leave to remote provider: {outbound}"
    );
    assert!(
        !outbound.contains("900101-1234567"),
        "raw RRN must not leave to remote provider: {outbound}"
    );
    assert!(
        outbound.contains("[PII_EMAIL_2]"),
        "collision avoidance starts at 2 after literal [PII_EMAIL_1]: {outbound}"
    );
    assert!(
        outbound.contains("[PII_PHONE_3]"),
        "phone token present in outbound body: {outbound}"
    );
    assert!(
        outbound.contains("[PII_RRN_4]"),
        "RRN token present in outbound body: {outbound}"
    );

    let o = parse_obj(&res.expect("PII stream roundtrip ok"));
    let final_reply = StrictJsonValue::get_str(&o, "reply").unwrap();
    assert_eq!(
        final_reply,
        "담당자 minsu.kim@example.co.kr (연락처 010-2345-6789, 미발급 [PII_FAKE_99]) 안내 완료"
    );
    let streamed_joined = pieces.join("");
    assert!(
        streamed_joined.contains("minsu.kim@example.co.kr")
            && streamed_joined.contains("010-2345-6789"),
        "chunk-split PII tokens restored in live stream: {streamed_joined}"
    );
    let ops = ops_of(&o);
    let op_obj = ops[0].as_object().unwrap();
    let blocks = StrictJsonValue::get_field(op_obj, "blocks")
        .and_then(StrictJsonValue::as_array)
        .unwrap();
    assert_eq!(
        StrictJsonValue::get_str(blocks[0].as_object().unwrap(), "text"),
        Some("문의: minsu.kim@example.co.kr / 010-2345-6789")
    );

    // Loopback provider (Ollama on 127.0.0.1) sends raw text without redaction
    let local_req = r#"{
        "action":"copilot_run",
        "mode":"ask",
        "module":"DOC",
        "prompt":"담당자 minsu.kim@example.co.kr 확인",
        "provider":{"kind":"ollama","baseUrl":"http://127.0.0.1:11434/v1","model":"qwen3"}
    }"#;
    let (l_res, l_reqs) = dispatch_raw_llm(
        local_req,
        &openai_text_reply(r#"{"reply":"로컬 응답","operations":[]}"#),
    );
    assert!(l_res.is_ok());
    assert!(
        l_reqs[0].body_json.contains("minsu.kim@example.co.kr"),
        "loopback provider keeps raw local text without unnecessary redaction"
    );
}

#[test]
fn google_sse_streaming_and_parameters_json_schema_work_alongside_array_fallback() {
    let sse_chunks = [
        "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"{\\\"reply\\\":\\\"SSE 첫 조각, \"}]}}]}\n\n",
        "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"SSE 둘째 조각\\\",\\\"operations\\\":[{\\\"op\\\":\\\"doc.append\\\",\\\"text\\\":\\\"추가 문단\\\"}]}\"}]}}]}\n\n",
    ];
    let stream_req = r#"{"action":"copilot_run","mode":"edit","module":"DOC","prompt":"추가","stream":true,"requestId":"g-sse-1","provider":{"kind":"google","baseUrl":"https://93.184.216.34/v1beta","model":"gemini-3-flash","apiKey":"AIza-test"}}"#;
    let (res, reqs, pieces) = dispatch_stream_chunks(stream_req, &sse_chunks);
    let o = parse_obj(&res.expect("google SSE stream ok"));
    assert_eq!(
        StrictJsonValue::get_str(&o, "reply"),
        Some("SSE 첫 조각, SSE 둘째 조각")
    );
    assert_eq!(ops_of(&o).len(), 1);
    assert!(
        !pieces.is_empty(),
        "Google SSE stream must emit live text chunks"
    );
    assert!(
        reqs[0].body_json.contains("\"parametersJsonSchema\"")
            && !reqs[0].body_json.contains("\"parameters\":{"),
        "Google functionDeclarations must use parametersJsonSchema: {}",
        reqs[0].body_json
    );
    assert!(
        !reqs[0].body_json.contains(r#"{"type":"object"}"#),
        "schemas must not contain bare empty object definitions: {}",
        reqs[0].body_json
    );
}

#[test]
fn non_reasoning_anthropic_and_deepseek_models_omit_thinking_fields() {
    let anthropic_resp =
        r#"{"content":[{"type":"text","text":"{\"reply\":\"하단 응답\",\"operations\":[]}"}]}"#;
    let haiku_req = r#"{"action":"copilot_run","mode":"ask","module":"DOC","prompt":"확인","provider":{"kind":"anthropic","baseUrl":"https://93.184.216.34/v1","model":"claude-3-5-haiku-20241022","apiKey":"sk-ant-test","reasoningEffort":"high"}}"#;
    let (res_h, reqs_h) = dispatch_raw_llm(haiku_req, anthropic_resp);
    assert!(res_h.is_ok());
    assert!(
        !reqs_h[0].body_json.contains("\"thinking\""),
        "non-reasoning Anthropic model must not inject thinking budget: {}",
        reqs_h[0].body_json
    );

    let ds_chat_req = r#"{"action":"copilot_run","mode":"ask","module":"DOC","prompt":"확인","provider":{"kind":"remote_https","baseUrl":"https://93.184.216.34/v1","model":"deepseek-chat","apiKey":"sk-ds","reasoningEffort":"high"}}"#;
    let (res_ds, reqs_ds) = dispatch_raw_llm(
        ds_chat_req,
        &openai_text_reply(r#"{"reply":"완료","operations":[]}"#),
    );
    assert!(res_ds.is_ok());
    assert!(
        !reqs_ds[0].body_json.contains("\"thinking\"")
            && !reqs_ds[0].body_json.contains("\"reasoning_effort\""),
        "deepseek-chat must not receive reasoning_effort/thinking: {}",
        reqs_ds[0].body_json
    );
}

#[test]
fn pii_restoration_runs_before_validation_for_cell_coords_sheet_names_and_length_bounds() {
    let long_email = format!("{}@example.com", "a".repeat(170));
    let sheet_req = format!(
        r#"{{
            "action":"copilot_run",
            "mode":"edit",
            "module":"SHEET",
            "prompt":"셀 AB1000000과 긴 제목 {long_email} 반영",
            "selection":{{"sheet":"admin@corp.kr","range":"AB1000000"}},
            "provider":{{"kind":"openai","baseUrl":"https://93.184.216.34/v1","model":"gpt-4o","apiKey":"sk-test"}},
            "context":{{"sheet":{{"name":"admin@corp.kr","cells":[{{"coord":"AB1000000","raw":"10","value":"10"}}],"rowCount":1000005}}}}
        }}"#
    );
    // Outbound scan order: prompt ("AB1000000" -> [PII_ID_1], long_email -> [PII_EMAIL_2]),
    // selection/context ("admin@corp.kr" -> [PII_EMAIL_3]).
    let model_reply = r#"{
        "reply":"시트 [PII_EMAIL_3]의 [PII_ID_1] 갱신, 꼬리 조각 [PII_EMA",
        "operations":[
            {"op":"sheet.write","sheet":"[PII_EMAIL_3]","cells":[{"coord":"[PII_ID_1]","raw":"99"}]},
            {"op":"sheet.chart","sheet":"[PII_EMAIL_3]","chartType":"bar","dataRange":"A1:B3","title":"[PII_EMAIL_2]"}
        ]
    }"#;
    let (res, reqs) = dispatch_raw_llm(&sheet_req, &openai_text_reply(model_reply));
    assert!(
        !reqs[0].body_json.contains("admin@corp.kr") && !reqs[0].body_json.contains("AB1000000"),
        "outbound payload redacted sheet email and passport-shaped coord: {}",
        reqs[0].body_json
    );
    let o = parse_obj(&res.expect("PII pre-validation restore ok"));
    let ops = ops_of(&o);
    assert_eq!(
        ops.len(),
        1,
        "valid AB1000000 sheet.write survives while >160-char restored chart title is dropped"
    );
    assert_eq!(
        StrictJsonValue::get_field(&o, "droppedOperations").and_then(StrictJsonValue::as_u64),
        Some(1)
    );
    let op0 = ops[0].as_object().unwrap();
    assert_eq!(
        StrictJsonValue::get_str(op0, "sheet"),
        Some("admin@corp.kr")
    );
    let cells = StrictJsonValue::get_field(op0, "cells")
        .and_then(StrictJsonValue::as_array)
        .unwrap();
    assert_eq!(
        StrictJsonValue::get_str(cells[0].as_object().unwrap(), "coord"),
        Some("AB1000000")
    );
}

#[test]
fn read_doc_blocks_tool_is_exposed_only_when_paragraphs_are_omitted_or_windowed() {
    let ok_openai = openai_text_reply(r#"{"reply":"이어지는 예측 구절","operations":[]}"#);
    let ok_anthropic =
        r#"{"content":[{"type":"text","text":"{\"reply\":\"확인\",\"operations\":[]}"}]}"#;
    let ok_google = r#"{"candidates":[{"content":{"parts":[{"text":"{\"reply\":\"확인\",\"operations\":[]}"}]}}]}"#;

    // 1. Single-paragraph text prediction (`텍스트 예측`) must not expose `read_doc_blocks` or `toolCalls`
    //    anywhere in tools OR system prompt, and drops hallucinated `read_doc_blocks` calls.
    let hallucinated_tool_reply = openai_text_reply(
        r#"{"reply":"이어지는 예측 구절","operations":[],"toolCalls":[{"name":"read_doc_blocks","startIndex":0,"endIndex":10}]}"#,
    );
    let ghost_req = r#"{
        "action":"copilot_run",
        "mode":"ask",
        "module":"DOC",
        "prompt":"다음 구절 제안",
        "selection":{"range":"텍스트 예측"},
        "provider":{"kind":"openai","baseUrl":"https://93.184.216.34/v1","model":"gpt-4o","apiKey":"sk-test"},
        "context":{"doc":{"paragraphs":["초안 문장 시작 "],"selectedText":"초안 문장 시작 ","omittedParagraphs":0}}
    }"#;
    let (g_res, g_reqs) = dispatch_raw_llm(ghost_req, &hallucinated_tool_reply);
    let g_obj = parse_obj(&g_res.expect("ghost request ok"));
    assert!(
        !g_reqs[0].body_json.contains("\"emit_copilot_turn\"")
            && !g_reqs[0].body_json.contains("\"tools\""),
        "read-only ask request with 0 read tools omits emit_copilot_turn and tools: {}",
        g_reqs[0].body_json
    );
    assert!(
        !g_reqs[0].body_json.contains("read_doc_blocks")
            && !g_reqs[0].body_json.contains("toolCalls"),
        "text prediction request must not expose read_doc_blocks or toolCalls anywhere: {}",
        g_reqs[0].body_json
    );
    assert!(
        tools_of(&g_obj).is_empty(),
        "hallucinated read_doc_blocks must be rejected when doc has no unread blocks"
    );

    // 2. Short complete DOC and HWP documents across OpenAI, Anthropic, Google, and Ollama omit `read_doc_blocks`
    //    from both native `tools` AND `system` instructions.
    let short_hwp_anth = r#"{
        "action":"copilot_run",
        "mode":"edit",
        "module":"HWP",
        "prompt":"다듬기",
        "provider":{"kind":"anthropic","baseUrl":"https://93.184.216.34/v1","model":"claude-sonnet-4-5","apiKey":"sk-ant-test"},
        "context":{"doc":{"paragraphs":["첫 문단","둘째 문단"],"omittedParagraphs":0}}
    }"#;
    let (_, short_anth_reqs) = dispatch_raw_llm(short_hwp_anth, ok_anthropic);
    assert!(
        !short_anth_reqs[0].body_json.contains("read_doc_blocks"),
        "short HWP must omit read_doc_blocks from system prompt and tools: {}",
        short_anth_reqs[0].body_json
    );

    let short_doc_google = r#"{
        "action":"copilot_run",
        "mode":"edit",
        "module":"DOC",
        "prompt":"다듬기",
        "provider":{"kind":"google","baseUrl":"https://93.184.216.34/v1beta","model":"gemini-3-flash","apiKey":"AIza-test"},
        "context":{"doc":{"paragraphs":["첫 문단","둘째 문단"],"omittedParagraphs":0}}
    }"#;
    let (_, short_google_reqs) = dispatch_raw_llm(short_doc_google, ok_google);
    assert!(
        !short_google_reqs[0].body_json.contains("read_doc_blocks"),
        "short DOC must omit read_doc_blocks from systemInstruction and tools: {}",
        short_google_reqs[0].body_json
    );

    let short_ollama = r#"{
        "action":"copilot_run",
        "mode":"ask",
        "module":"DOC",
        "prompt":"다음 구절 제안",
        "selection":{"range":"텍스트 예측"},
        "provider":{"kind":"ollama","baseUrl":"http://127.0.0.1:11434/v1","model":"qwen3"},
        "context":{"doc":{"paragraphs":["초안 문장 시작 "],"selectedText":"초안 문장 시작 ","omittedParagraphs":0}}
    }"#;
    let (_, short_ollama_reqs) = dispatch_raw_llm(short_ollama, &ok_openai);
    assert!(
        !short_ollama_reqs[0].body_json.contains("read_doc_blocks")
            && !short_ollama_reqs[0].body_json.contains("toolCalls"),
        "local loopback ghost request must not advertise toolCalls: {}",
        short_ollama_reqs[0].body_json
    );

    // 3. Explicit `omittedParagraphs > 0` exposes `read_doc_blocks`
    let omitted_req = r#"{
        "action":"copilot_run",
        "mode":"edit",
        "module":"DOC",
        "prompt":"전체 요약 추가",
        "provider":{"kind":"openai","baseUrl":"https://93.184.216.34/v1","model":"gpt-4o","apiKey":"sk-test"},
        "context":{"doc":{"paragraphs":["문단 0","문단 1"],"omittedParagraphs":5}}
    }"#;
    let (_, omitted_reqs) = dispatch_raw_llm(omitted_req, &ok_openai);
    assert!(omitted_reqs[0].body_json.contains("\"read_doc_blocks\""));

    // 4. `paragraphs.len() > 200` (with `omittedParagraphs == 0`) exposes `read_doc_blocks`
    let over_200_paras: String = (0..205)
        .map(|i| format!("\"문단 {i}\""))
        .collect::<Vec<_>>()
        .join(",");
    let over_200_req = format!(
        r#"{{"action":"copilot_run","mode":"ask","module":"HWP","prompt":"끝 확인","provider":{{"kind":"openai","baseUrl":"https://93.184.216.34/v1","model":"gpt-4o","apiKey":"sk-test"}},"context":{{"doc":{{"paragraphs":[{over_200_paras}],"omittedParagraphs":0}}}}}}"#
    );
    let (_, over_200_reqs) = dispatch_raw_llm(&over_200_req, &ok_openai);
    assert!(over_200_reqs[0].body_json.contains("\"read_doc_blocks\""));

    // 5. Genuine blank paragraph (`""`) does NOT expose `read_doc_blocks`, while sparse `null` slot DOES
    let blank_req = r#"{
        "action":"copilot_run",
        "mode":"edit",
        "module":"HWP",
        "prompt":"빈 문단 확인",
        "provider":{"kind":"anthropic","baseUrl":"https://93.184.216.34/v1","model":"claude-sonnet-4-5","apiKey":"sk-ant-test"},
        "context":{"doc":{"paragraphs":["첫 문단","","셋째 문단"],"omittedParagraphs":0}}
    }"#;
    let (_, blank_reqs) = dispatch_raw_llm(blank_req, ok_anthropic);
    assert!(
        !blank_reqs[0].body_json.contains("read_doc_blocks"),
        "genuine blank paragraph must not trigger read_doc_blocks: {}",
        blank_reqs[0].body_json
    );

    let sparse_req = r#"{
        "action":"copilot_run",
        "mode":"edit",
        "module":"HWP",
        "prompt":"빈 구간 조회",
        "provider":{"kind":"anthropic","baseUrl":"https://93.184.216.34/v1","model":"claude-sonnet-4-5","apiKey":"sk-ant-test"},
        "context":{"doc":{"paragraphs":["첫 문단",null,"셋째 문단"],"omittedParagraphs":0}}
    }"#;
    let (_, sparse_reqs) = dispatch_raw_llm(sparse_req, ok_anthropic);
    assert!(sparse_reqs[0].body_json.contains("\"read_doc_blocks\""));

    // 6. Byte-budget truncation (`<= 200` paragraphs exceeding 24 KiB prompt budget) exposes `read_doc_blocks`
    let big_para = "가".repeat(400); // 1,200 UTF-8 bytes per paragraph; 25 paragraphs = 30,000 bytes > 24 KiB
    let budget_paras: String = (0..25)
        .map(|_| format!("\"{big_para}\""))
        .collect::<Vec<_>>()
        .join(",");
    let budget_req = format!(
        r#"{{"action":"copilot_run","mode":"ask","module":"DOC","prompt":"요약","provider":{{"kind":"google","baseUrl":"https://93.184.216.34/v1beta","model":"gemini-3-flash","apiKey":"AIza-test"}},"context":{{"doc":{{"paragraphs":[{budget_paras}],"omittedParagraphs":0}}}}}}"#
    );
    let (_, budget_reqs) = dispatch_raw_llm(&budget_req, ok_google);
    assert!(budget_reqs[0].body_json.contains("\"read_doc_blocks\""));

    // 7. Selection commands never expose `read_doc_blocks` even when `omittedParagraphs > 0`
    let cmd_req = r#"{
        "action":"copilot_run",
        "mode":"edit",
        "module":"DOC",
        "command":"rewrite",
        "prompt":"다시 쓰기",
        "selection":{"paragraphIndex":0,"paragraphEnd":0},
        "provider":{"kind":"openai","baseUrl":"https://93.184.216.34/v1","model":"gpt-4o","apiKey":"sk-test"},
        "context":{"doc":{"paragraphs":["문단 0","문단 1"],"omittedParagraphs":12}}
    }"#;
    let (_, cmd_reqs) = dispatch_raw_llm(cmd_req, &ok_openai);
    assert!(
        !cmd_reqs[0].body_json.contains("read_doc_blocks"),
        "selection command must never expose read_doc_blocks: {}",
        cmd_reqs[0].body_json
    );
}
