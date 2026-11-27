//! 참조 구현: dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd.) 및 continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., see THIRD_PARTY_NOTICES.md)

use easyoffice_formats::{EasyOfficeError, StrictJsonValue};

use super::cancel::CancelHandle;
use super::operations::{
    validate_locations, validate_operations, validate_tool_calls, ValidatedOperations,
};
use super::pii::{PiiVault, StreamingPiiRestorer};
use super::prompt::build_prompt_bundle_for_llm;
use super::providers::{
    build_provider_chat_body_with_all_parts, build_provider_headers, build_provider_ping_body,
    extract_provider_completion_with_usage, parse_discovered_models, serialize_models_response,
};
use super::ssrf::ValidatedEndpoint;
use super::stream::StreamAssembler;
use super::types::{
    push_json_str, CopilotEventSink, CopilotLlmClient, CopilotLlmRequest, CopilotRunRequest,
    CopilotRunResponse, CopilotStreamClient, CopilotStreamEvent, HttpMethod, ProviderConfig,
};
use super::usage::TokenUsage;
pub use super::wire::{send_local_http, send_local_http_streaming};

pub fn validate_wire_headers(headers: &[(String, String)]) -> Result<(), EasyOfficeError> {
    if headers.len() > 8 {
        return Err(EasyOfficeError::SecurityViolation(
            "모델 요청 헤더 수가 허용 상한을 초과했습니다.".to_string(),
        ));
    }
    for (name, val) in headers {
        let allowed_name = matches!(
            name.as_str(),
            "Authorization" | "x-api-key" | "anthropic-version" | "x-goog-api-key"
        );
        if !allowed_name {
            return Err(EasyOfficeError::SecurityViolation(
                "허용되지 않은 모델 인증 헤더 이름입니다.".to_string(),
            ));
        }
        let check_slice = if name == "Authorization" {
            val.strip_prefix("Bearer ").unwrap_or(val.as_str())
        } else {
            val.as_str()
        };
        if check_slice.is_empty() || check_slice.bytes().any(|b| !(0x21..=0x7E).contains(&b)) {
            return Err(EasyOfficeError::SecurityViolation(
                "인증 헤더에는 공백·제어 문자·비ASCII 문자를 포함할 수 없습니다.".to_string(),
            ));
        }
    }
    Ok(())
}

fn host_error(msg: String) -> EasyOfficeError {
    if msg.contains("리다이렉트") || msg.contains("사설") || msg.contains("보안") {
        EasyOfficeError::SecurityViolation(msg)
    } else {
        EasyOfficeError::InvalidArgument(msg)
    }
}

fn precheck(req: &CopilotLlmRequest) -> Result<(), EasyOfficeError> {
    validate_wire_headers(&req.headers)?;
    req.cancel.check().map_err(EasyOfficeError::InvalidArgument)
}

fn dispatch_transport(
    req: &CopilotLlmRequest,
    host_llm: Option<&CopilotLlmClient<'_>>,
) -> Result<String, EasyOfficeError> {
    precheck(req)?;
    if let Some(client) = host_llm {
        return client(req).map_err(host_error);
    }
    if req.is_https {
        return Err(EasyOfficeError::InvalidArgument(
            "원격 HTTPS 모델 연결은 데스크톱 앱에서 사용할 수 있습니다. 로컬 HTTP(127.0.0.1) 서버를 선택하세요."
                .to_string(),
        ));
    }
    send_local_http(req)
}

fn build_http_req(
    method: HttpMethod,
    endpoint: &ValidatedEndpoint,
    headers: Vec<(String, String)>,
    body_json: String,
    cancel: CancelHandle,
) -> CopilotLlmRequest {
    CopilotLlmRequest {
        method,
        url: endpoint.url.clone(),
        host: endpoint.host.clone(),
        port: endpoint.port,
        path: endpoint.path.clone(),
        is_https: endpoint.is_https,
        resolved_addrs: endpoint.resolved_addrs.clone(),
        headers,
        body_json,
        cancel,
        stream: false,
    }
}

pub fn test_provider_endpoint(
    provider: &ProviderConfig,
    endpoint: &ValidatedEndpoint,
    host_llm: Option<&CopilotLlmClient<'_>>,
) -> Result<String, EasyOfficeError> {
    let model = provider.model.trim();
    if model.is_empty() {
        return Err(EasyOfficeError::InvalidArgument(
            "모델 ID를 입력하세요.".to_string(),
        ));
    }
    let headers = build_provider_headers(provider)?;
    let body_json = build_provider_ping_body(provider);
    let req = build_http_req(
        HttpMethod::Post,
        endpoint,
        headers,
        body_json,
        CancelHandle::default(),
    );
    let raw = dispatch_transport(&req, host_llm)?;
    let _parsed = StrictJsonValue::parse(&raw).map_err(|_| {
        EasyOfficeError::InvalidArgument(
            "모델 서버 응답이 올바른 JSON 형식이 아닙니다.".to_string(),
        )
    })?;
    let label = format!("{} 연결 확인 완료 ({model})", provider.kind.display_name());
    let mut out = String::with_capacity(128);
    out.push_str("{\"status\":\"ok\",\"message\":");
    push_json_str(&mut out, &label);
    out.push('}');
    Ok(out)
}

pub fn should_fail_closed_on_test_error(err: &EasyOfficeError) -> bool {
    match err {
        EasyOfficeError::SecurityViolation(_) | EasyOfficeError::ResourceBudgetExceeded(_) => true,
        EasyOfficeError::InvalidArgument(msg) => {
            msg.contains("콘텐츠 인코딩")
                || msg.contains("HTTP 401")
                || msg.contains("상한")
                || msg.contains("데스크톱 앱")
                || msg.contains("API 키")
        }
        _ => false,
    }
}

pub fn test_provider_models_endpoint(
    provider: &ProviderConfig,
    endpoint: &ValidatedEndpoint,
    target_model: Option<&str>,
    host_llm: Option<&CopilotLlmClient<'_>>,
) -> Result<String, EasyOfficeError> {
    let headers = build_provider_headers(provider)?;
    let req = build_http_req(
        HttpMethod::Get,
        endpoint,
        headers,
        String::new(),
        CancelHandle::default(),
    );
    let raw = dispatch_transport(&req, host_llm)?;
    let discovered = parse_discovered_models(provider.kind, &raw)?;
    if let Some(expected) = target_model.map(str::trim).filter(|m| !m.is_empty()) {
        let found = discovered.iter().any(|m| {
            m.id == expected
                || m.id.strip_prefix("models/") == Some(expected)
                || expected.strip_prefix("models/") == Some(m.id.as_str())
        });
        if !found {
            return Err(EasyOfficeError::InvalidArgument(
                "선택한 모델이 제공자 목록에 없어 최소 ping으로 전환합니다.".to_string(),
            ));
        }
        let label = format!(
            "{} 연결 확인 완료 ({expected})",
            provider.kind.display_name()
        );
        let mut out = String::with_capacity(128);
        out.push_str("{\"status\":\"ok\",\"message\":");
        push_json_str(&mut out, &label);
        out.push('}');
        return Ok(out);
    }
    let label = format!(
        "{} 연결 확인 완료 (사용 가능한 모델 {}개)",
        provider.kind.display_name(),
        discovered.len()
    );
    let mut out = String::with_capacity(128);
    out.push_str("{\"status\":\"ok\",\"message\":");
    push_json_str(&mut out, &label);
    out.push('}');
    Ok(out)
}

pub fn discover_provider_models(
    provider: &ProviderConfig,
    endpoint: Option<&ValidatedEndpoint>,
    host_llm: Option<&CopilotLlmClient<'_>>,
) -> Result<String, EasyOfficeError> {
    let Some(ep) = endpoint else {
        return Ok(serialize_models_response(provider.kind, &[], false));
    };
    let headers = build_provider_headers(provider)?;
    let req = build_http_req(
        HttpMethod::Get,
        ep,
        headers,
        String::new(),
        CancelHandle::default(),
    );
    let raw = dispatch_transport(&req, host_llm)?;
    let discovered = parse_discovered_models(provider.kind, &raw)?;
    Ok(serialize_models_response(provider.kind, &discovered, true))
}

#[derive(Clone, Copy)]
pub struct StreamPlan<'a> {
    pub request_id: &'a str,
    pub client: &'a CopilotStreamClient<'a>,
    pub events: &'a CopilotEventSink<'a>,
}

pub fn run_remote_or_local(
    req: &CopilotRunRequest,
    endpoint: &ValidatedEndpoint,
    host_llm: Option<&CopilotLlmClient<'_>>,
    stream: Option<StreamPlan<'_>>,
    cancel: CancelHandle,
) -> Result<CopilotRunResponse, EasyOfficeError> {
    if req.provider.model.trim().is_empty() {
        return Err(EasyOfficeError::InvalidArgument(
            "모델 ID를 입력하세요.".to_string(),
        ));
    }
    let headers = build_provider_headers(&req.provider)?;
    let (system_prompt, raw_context_block, raw_volatile_state, raw_user_prompt) =
        build_prompt_bundle_for_llm(req)?;
    let one_shot = req.lightweight || req.command.is_some();

    let mut vault = PiiVault::new();
    let (history_to_send, ctx_to_send, volatile_to_send, prompt_to_send, steps_to_send) =
        if req.provider.kind.is_local_loopback() {
            (
                if one_shot {
                    Vec::new()
                } else {
                    req.history.clone()
                },
                raw_context_block,
                raw_volatile_state,
                raw_user_prompt,
                req.loop_steps.clone(),
            )
        } else {
            // 실제 배치 순서(Tier 2 문서 -> Tier 3 priorHistory -> Tier 4 가변/질문 -> Tier 5 loopSteps)대로 순차 예약·치환한다.
            let redacted_ctx = vault.redact_text_sequential(&raw_context_block);
            let redacted_history = if one_shot {
                Vec::new()
            } else {
                vault.redact_history_sequential(&req.history)
            };
            let redacted_volatile = vault.redact_text_sequential(&raw_volatile_state);
            let redacted_prompt = vault.redact_text_sequential(&raw_user_prompt);
            let redacted_steps = vault.redact_history_sequential(&req.loop_steps);
            (
                redacted_history,
                redacted_ctx,
                redacted_volatile,
                redacted_prompt,
                redacted_steps,
            )
        };

    let body_json = build_provider_chat_body_with_all_parts(
        req,
        &history_to_send,
        &ctx_to_send,
        &volatile_to_send,
        &prompt_to_send,
        &steps_to_send,
        stream.is_some(),
        &system_prompt,
    )?;
    let mut http_req = build_http_req(HttpMethod::Post, endpoint, headers, body_json, cancel);
    let (content, reasoning, usage) = match stream {
        Some(plan) => {
            http_req.stream = true;
            read_streamed(req, &http_req, plan, &vault)?
        }
        None => {
            let raw_response = dispatch_transport(&http_req, host_llm)?;
            extract_provider_completion_with_usage(req.provider.kind, &raw_response)?
        }
    };
    let mut resp = parse_and_validate_assistant_output(req, &content, reasoning, usage, &vault);
    vault.restore_response(&mut resp);
    Ok(resp)
}

fn read_streamed(
    req: &CopilotRunRequest,
    http_req: &CopilotLlmRequest,
    plan: StreamPlan<'_>,
    vault: &PiiVault,
) -> Result<(String, String, Option<TokenUsage>), EasyOfficeError> {
    precheck(http_req)?;
    let mut assembler = StreamAssembler::new(req.provider.kind);
    let mut restorer = StreamingPiiRestorer::default();
    let mut seq = 0u32;
    let sent = {
        let mut sink = |chunk: &[u8]| {
            if let Some(text) = assembler.feed(chunk) {
                let restored = restorer.push(vault, &text);
                if !restored.is_empty() {
                    (plan.events)(&CopilotStreamEvent {
                        request_id: plan.request_id,
                        seq,
                        text: &restored,
                    });
                    seq += 1;
                }
            }
            assembler.failure().is_none()
        };
        (plan.client)(http_req, &mut sink)
    };
    let raw = match sent {
        Ok(raw) => raw,
        Err(msg) => {
            return Err(match assembler.failure() {
                Some(failure) => EasyOfficeError::InvalidArgument(failure.to_string()),
                None => host_error(msg),
            })
        }
    };
    let flushed = restorer.flush(vault);
    if !flushed.is_empty() {
        (plan.events)(&CopilotStreamEvent {
            request_id: plan.request_id,
            seq,
            text: &flushed,
        });
    }
    assembler.finish_with_usage(&raw)
}

fn strip_code_fence(raw: &str) -> &str {
    let trimmed = raw.trim();
    if let Some(after_open) = trimmed.strip_prefix("```") {
        let after_lang = match after_open.find('\n') {
            Some(idx) => &after_open[idx + 1..],
            None => after_open,
        };
        if let Some(inner) = after_lang.strip_suffix("```") {
            return inner.trim();
        }
    }
    trimmed
}

fn parse_and_validate_assistant_output(
    req: &CopilotRunRequest,
    content: &str,
    default_reasoning: String,
    usage: Option<TokenUsage>,
    vault: &PiiVault,
) -> CopilotRunResponse {
    let default_reasoning = vault.restore_text(&default_reasoning);
    let candidate = strip_code_fence(content);
    let parsed_obj = StrictJsonValue::parse(candidate)
        .ok()
        .and_then(|v| match v {
            StrictJsonValue::Object(o) => Some(o),
            _ => None,
        });
    let Some(mut obj) = parsed_obj.filter(|o| {
        StrictJsonValue::get_str(o, "reply").is_some()
            || StrictJsonValue::get_field(o, "toolCalls").is_some()
            || StrictJsonValue::get_field(o, "operations").is_some()
    }) else {
        return CopilotRunResponse {
            mode: req.mode,
            reply: vault.restore_text(content.trim()),
            reasoning: default_reasoning,
            updated_in_place: false,
            operations: Vec::new(),
            dropped_operations: 0,
            dropped_reasons: Vec::new(),
            locations: Vec::new(),
            tool_calls: Vec::new(),
            usage,
        };
    };
    // 검증기 실행 전에 자리표시자를 원문으로 복원해 셀 좌표·시트 이름·길이 상한 검사를 실제 값 기준으로 수행한다.
    vault.restore_json_entries(&mut obj);
    let reply = StrictJsonValue::get_str(&obj, "reply").unwrap_or("");
    let reasoning = StrictJsonValue::get_str(&obj, "reasoning")
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .unwrap_or(default_reasoning);

    let validated = if req.is_read_only() {
        ValidatedOperations::default()
    } else {
        match StrictJsonValue::get_field(&obj, "operations") {
            Some(StrictJsonValue::Array(items)) => validate_operations(req, items),
            Some(StrictJsonValue::Null) | None => ValidatedOperations::default(),
            Some(_) => ValidatedOperations {
                operations: Vec::new(),
                dropped: 1,
                dropped_reasons: vec!["operations 필드는 배열이어야 합니다.".to_string()],
            },
        }
    };
    let ValidatedOperations {
        operations,
        dropped,
        dropped_reasons,
    } = validated;
    let locations = StrictJsonValue::get_field(&obj, "locations")
        .and_then(StrictJsonValue::as_array)
        .map(validate_locations)
        .unwrap_or_default();
    let tool_calls = StrictJsonValue::get_field(&obj, "toolCalls")
        .and_then(StrictJsonValue::as_array)
        .map(|items| validate_tool_calls(req, items))
        .unwrap_or_default();

    CopilotRunResponse {
        mode: req.mode,
        reply: reply.to_string(),
        reasoning,
        updated_in_place: !operations.is_empty(),
        operations,
        dropped_operations: dropped,
        dropped_reasons,
        locations,
        tool_calls,
        usage,
    }
}
