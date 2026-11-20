//! 참조 구현: dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd.) 및 continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., see THIRD_PARTY_NOTICES.md)

use easyoffice_formats::{EasyOfficeError, StrictJsonValue};

use super::cancel::CancelHandle;
use super::operations::{validate_locations, validate_operations, ValidatedOperations};
use super::prompt::build_prompts_for_llm;
use super::providers::{
    build_provider_chat_body, build_provider_headers, build_provider_ping_body,
    extract_provider_completion, parse_discovered_models, serialize_models_response,
};
use super::ssrf::ValidatedEndpoint;
use super::stream::StreamAssembler;
use super::types::{
    push_json_str, CopilotEventSink, CopilotLlmClient, CopilotLlmRequest, CopilotRunRequest,
    CopilotRunResponse, CopilotStreamClient, CopilotStreamEvent, HttpMethod, ProviderConfig,
};
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

/// 호스트 전송이 돌려준 오류 문구를 오류 종류로 가른다.
fn host_error(msg: String) -> EasyOfficeError {
    if msg.contains("리다이렉트") || msg.contains("사설") || msg.contains("보안") {
        EasyOfficeError::SecurityViolation(msg)
    } else {
        EasyOfficeError::InvalidArgument(msg)
    }
}

/// 전송 전에 모든 요청이 거치는 검사: 인증 헤더 모양과 이미 온 취소.
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

pub fn test_provider_models_endpoint(
    provider: &ProviderConfig,
    endpoint: &ValidatedEndpoint,
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

/// 스트림으로 받을 때 호스트가 대는 것: 전송, 모델 글 조각을 화면으로 보내는 통로, 조각을 묶는 요청 번호.
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
    let (system_prompt, user_content) = build_prompts_for_llm(req)?;
    let body_json = build_provider_chat_body(req, &system_prompt, &user_content, stream.is_some())?;
    let mut http_req = build_http_req(HttpMethod::Post, endpoint, headers, body_json, cancel);
    let (content, reasoning) = match stream {
        Some(plan) => {
            http_req.stream = true;
            read_streamed(req, &http_req, plan)?
        }
        None => {
            let raw_response = dispatch_transport(&http_req, host_llm)?;
            extract_provider_completion(req.provider.kind, &raw_response)?
        }
    };
    Ok(parse_and_validate_assistant_output(
        req, &content, reasoning,
    ))
}

/// 스트림 전송으로 본문을 받으며 모델 글 조각을 화면으로 흘리고, 끝난 뒤 조립한 본문을 돌려준다.
/// 흘려 보낸 글은 보여 주기만 하고 검증은 돌려주는 본문 전체로 한다.
fn read_streamed(
    req: &CopilotRunRequest,
    http_req: &CopilotLlmRequest,
    plan: StreamPlan<'_>,
) -> Result<(String, String), EasyOfficeError> {
    precheck(http_req)?;
    let mut assembler = StreamAssembler::new(req.provider.kind);
    let mut seq = 0u32;
    let sent = {
        let mut sink = |chunk: &[u8]| {
            if let Some(text) = assembler.feed(chunk) {
                (plan.events)(&CopilotStreamEvent {
                    request_id: plan.request_id,
                    seq,
                    text: &text,
                });
                seq += 1;
            }
            assembler.failure().is_none()
        };
        (plan.client)(http_req, &mut sink)
    };
    let raw = match sent {
        Ok(raw) => raw,
        // 조립기가 스트림 오류로 읽기를 멈추게 했으면 그 사유가 원인이다.
        Err(msg) => {
            return Err(match assembler.failure() {
                Some(failure) => EasyOfficeError::InvalidArgument(failure.to_string()),
                None => host_error(msg),
            })
        }
    };
    assembler.finish(&raw)
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
) -> CopilotRunResponse {
    let candidate = strip_code_fence(content);
    let parsed_obj = StrictJsonValue::parse(candidate)
        .ok()
        .and_then(|v| match v {
            StrictJsonValue::Object(o) => Some(o),
            _ => None,
        });
    let Some(obj) = parsed_obj.filter(|o| StrictJsonValue::get_str(o, "reply").is_some()) else {
        return CopilotRunResponse {
            mode: req.mode,
            reply: content.trim().to_string(),
            reasoning: default_reasoning,
            updated_in_place: false,
            operations: Vec::new(),
            dropped_operations: 0,
            locations: Vec::new(),
        };
    };
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
            // 배열이 아닌 operations는 통째로 버린 하나의 작업으로 센다.
            Some(StrictJsonValue::Null) | None => ValidatedOperations::default(),
            Some(_) => ValidatedOperations {
                operations: Vec::new(),
                dropped: 1,
            },
        }
    };
    let ValidatedOperations {
        operations,
        dropped,
    } = validated;
    let locations = StrictJsonValue::get_field(&obj, "locations")
        .and_then(StrictJsonValue::as_array)
        .map(validate_locations)
        .unwrap_or_default();

    CopilotRunResponse {
        mode: req.mode,
        reply: reply.to_string(),
        reasoning,
        updated_in_place: !operations.is_empty(),
        operations,
        dropped_operations: dropped,
        locations,
    }
}
