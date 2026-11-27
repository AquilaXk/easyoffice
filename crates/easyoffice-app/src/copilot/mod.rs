//! 참조 구현: dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd.) 및 continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., see THIRD_PARTY_NOTICES.md)

use easyoffice_formats::{EasyOfficeError, StrictJsonValue};

pub mod cache;
pub mod cancel;
pub mod compact;
pub mod context;
pub mod delta;
pub mod llm_client;
pub mod op_reasons;
pub mod op_serialize;
pub mod op_types;
pub mod operations;
pub mod operations_ext;
pub mod pii;
pub mod prompt;
pub mod providers;
pub mod schema;
pub mod ssrf;
pub mod stream;
pub mod stream_reply;
pub mod types;
pub mod usage;
pub mod window;
pub mod wire;

pub use op_serialize::serialize_run_response;
pub use types::{
    ChunkSink, CopilotEventSink, CopilotHost, CopilotLlmClient, CopilotLlmRequest,
    CopilotStreamClient, CopilotStreamEvent, HttpMethod,
};

use providers::{chat_endpoint_target, models_endpoint_target};
use types::{parse_provider_config, parse_run_request};

pub fn handle_copilot_ipc(
    payload: &str,
    llm: Option<&CopilotLlmClient<'_>>,
) -> Result<String, EasyOfficeError> {
    let entries = crate::EasyOfficeIpcRouter::parse_rpc_json_object(payload)?;
    let action = StrictJsonValue::get_str(&entries, "action").ok_or_else(|| {
        EasyOfficeError::InvalidArgument(
            "코파일럿 IPC 요청에 'action' 필드가 필요합니다.".to_string(),
        )
    })?;
    handle_copilot_ipc_entries(
        action,
        &entries,
        CopilotHost {
            llm,
            ..CopilotHost::default()
        },
    )
}

pub(crate) fn handle_copilot_ipc_entries(
    action: &str,
    entries: &[(String, StrictJsonValue)],
    host: CopilotHost<'_>,
) -> Result<String, EasyOfficeError> {
    let llm = host.llm;
    match action {
        "copilot_test_provider" => handle_test_provider(entries, llm),
        "copilot_list_models" => handle_list_models(entries, llm),
        "copilot_run" => handle_run(entries, host),
        "copilot_cancel" => handle_cancel(entries),
        other => Err(EasyOfficeError::InvalidArgument(format!(
            "지원하지 않는 코파일럿 액션입니다: {other}"
        ))),
    }
}

fn handle_test_provider(
    entries: &[(String, StrictJsonValue)],
    llm: Option<&CopilotLlmClient<'_>>,
) -> Result<String, EasyOfficeError> {
    let provider = parse_provider_config(entries)?;
    if provider.model.trim().is_empty() {
        let target = models_endpoint_target(&provider);
        let endpoint = ssrf::validate_provider_target(
            provider.kind.is_local_loopback(),
            &provider.base_url,
            target,
        )?;
        return llm_client::test_provider_models_endpoint(&provider, &endpoint, None, llm);
    }
    // 모델이 지정된 경우에도 엔드포인트 보안 검증 후 무료 GET /models를 우선 확인한다.
    let chat_target = chat_endpoint_target(&provider, false);
    let chat_endpoint = ssrf::validate_provider_target(
        provider.kind.is_local_loopback(),
        &provider.base_url,
        chat_target,
    )?;
    let models_target = models_endpoint_target(&provider);
    if let Ok(models_endpoint) = ssrf::validate_provider_target(
        provider.kind.is_local_loopback(),
        &provider.base_url,
        models_target,
    ) {
        match llm_client::test_provider_models_endpoint(
            &provider,
            &models_endpoint,
            Some(&provider.model),
            llm,
        ) {
            Ok(json) => return Ok(json),
            Err(
                err @ (EasyOfficeError::SecurityViolation(_)
                | EasyOfficeError::ResourceBudgetExceeded(_)),
            ) => return Err(err),
            Err(err) if llm_client::should_fail_closed_on_test_error(&err) => return Err(err),
            Err(_) => {}
        }
    }
    llm_client::test_provider_endpoint(&provider, &chat_endpoint, llm)
}

fn handle_list_models(
    entries: &[(String, StrictJsonValue)],
    llm: Option<&CopilotLlmClient<'_>>,
) -> Result<String, EasyOfficeError> {
    let provider = parse_provider_config(entries)?;
    let optional = StrictJsonValue::get_field(entries, "optional")
        .and_then(StrictJsonValue::as_bool)
        .unwrap_or(false);
    if optional && llm.is_none() && !provider.kind.is_local_loopback() {
        return llm_client::discover_provider_models(&provider, None, llm);
    }
    let target = models_endpoint_target(&provider);
    let endpoint = match ssrf::validate_provider_target(
        provider.kind.is_local_loopback(),
        &provider.base_url,
        target,
    ) {
        Ok(ep) => ep,
        Err(_) if optional => return llm_client::discover_provider_models(&provider, None, llm),
        Err(e) => return Err(e),
    };
    match llm_client::discover_provider_models(&provider, Some(&endpoint), llm) {
        Ok(res) => Ok(res),
        Err(_) if optional => llm_client::discover_provider_models(&provider, None, llm),
        Err(e) => Err(e),
    }
}

fn stream_plan<'a>(
    entries: &'a [(String, StrictJsonValue)],
    host: CopilotHost<'a>,
) -> Option<llm_client::StreamPlan<'a>> {
    let wanted = StrictJsonValue::get_field(entries, "stream").and_then(StrictJsonValue::as_bool);
    if wanted != Some(true) {
        return None;
    }
    let request_id = StrictJsonValue::get_str(entries, "requestId")?;
    Some(llm_client::StreamPlan {
        request_id,
        client: host.llm_stream?,
        events: host.events?,
    })
}

fn handle_run(
    entries: &[(String, StrictJsonValue)],
    host: CopilotHost<'_>,
) -> Result<String, EasyOfficeError> {
    let req = parse_run_request(entries)?;
    let (cancel_handle, _registration) = match StrictJsonValue::get_str(entries, "requestId") {
        Some(id) => {
            let (handle, guard) = cancel::register(id)?;
            (handle, Some(guard))
        }
        None => (cancel::CancelHandle::default(), None),
    };
    let stream = stream_plan(entries, host);
    let target = chat_endpoint_target(&req.provider, stream.is_some());
    let endpoint = ssrf::validate_provider_target(
        req.provider.kind.is_local_loopback(),
        &req.provider.base_url,
        target,
    )?;
    let mut response =
        llm_client::run_remote_or_local(&req, &endpoint, host.llm, stream, cancel_handle)?;

    // Fail-closed read-only invariant across all providers.
    if req.is_read_only() {
        response.operations.clear();
        response.dropped_operations = 0;
        response.updated_in_place = false;
    } else {
        response.updated_in_place = !response.operations.is_empty();
    }

    Ok(serialize_run_response(&response))
}

fn handle_cancel(entries: &[(String, StrictJsonValue)]) -> Result<String, EasyOfficeError> {
    let id = StrictJsonValue::get_str(entries, "requestId").ok_or_else(|| {
        EasyOfficeError::InvalidArgument("copilot_cancel에는 'requestId'가 필요합니다.".to_string())
    })?;
    let found = cancel::cancel_request(id)?;
    Ok(format!("{{\"status\":\"ok\",\"cancelled\":{found}}}"))
}
