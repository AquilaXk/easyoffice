//! 참조 구현: dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd.) 및 continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., see THIRD_PARTY_NOTICES.md)

use easyoffice_formats::{EasyOfficeError, StrictJsonValue};
use std::fmt::Write as _;

pub mod cancel;
pub mod context;
pub mod llm_client;
pub mod operations;
pub mod prompt;
pub mod providers;
pub mod ssrf;
pub mod types;
pub mod window;
pub mod wire;

pub use types::{CopilotLlmClient, CopilotLlmRequest, HttpMethod};

use providers::{chat_endpoint_target, models_endpoint_target};
use types::{
    parse_provider_config, parse_run_request, push_json_str, CopilotLocation, CopilotOperation,
    CopilotRunResponse, DocBlock,
};

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
    handle_copilot_ipc_entries(action, &entries, llm)
}

pub(crate) fn handle_copilot_ipc_entries(
    action: &str,
    entries: &[(String, StrictJsonValue)],
    llm: Option<&CopilotLlmClient<'_>>,
) -> Result<String, EasyOfficeError> {
    match action {
        "copilot_test_provider" => handle_test_provider(entries, llm),
        "copilot_list_models" => handle_list_models(entries, llm),
        "copilot_run" => handle_run(entries, llm),
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
        return llm_client::test_provider_models_endpoint(&provider, &endpoint, llm);
    }
    let target = chat_endpoint_target(&provider);
    let endpoint = ssrf::validate_provider_target(
        provider.kind.is_local_loopback(),
        &provider.base_url,
        target,
    )?;
    llm_client::test_provider_endpoint(&provider, &endpoint, llm)
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

fn handle_run(
    entries: &[(String, StrictJsonValue)],
    llm: Option<&CopilotLlmClient<'_>>,
) -> Result<String, EasyOfficeError> {
    let req = parse_run_request(entries)?;
    // 취소 신호는 요청이 끝날 때까지 등록해 둔다. requestId가 없는 요청은 취소할 수 없다.
    let (cancel_handle, _registration) = match StrictJsonValue::get_str(entries, "requestId") {
        Some(id) => {
            let (handle, guard) = cancel::register(id)?;
            (handle, Some(guard))
        }
        None => (cancel::CancelHandle::default(), None),
    };
    let target = chat_endpoint_target(&req.provider);
    let endpoint = ssrf::validate_provider_target(
        req.provider.kind.is_local_loopback(),
        &req.provider.base_url,
        target,
    )?;
    let mut response = llm_client::run_remote_or_local(&req, &endpoint, llm, cancel_handle)?;

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

pub fn serialize_run_response(resp: &CopilotRunResponse) -> String {
    let mut out = String::with_capacity(512 + resp.reply.len());
    out.push_str("{\"status\":\"ok\",\"mode\":");
    push_json_str(&mut out, resp.mode.as_str());
    out.push_str(",\"reply\":");
    push_json_str(&mut out, &resp.reply);
    out.push_str(",\"reasoning\":");
    push_json_str(&mut out, &resp.reasoning);
    let _ = write!(
        out,
        ",\"updatedInPlace\":{},\"droppedOperations\":{},\"operations\":[",
        resp.updated_in_place, resp.dropped_operations
    );
    for (idx, op) in resp.operations.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        write_operation_json(&mut out, op);
    }
    out.push_str("],\"locations\":[");
    for (idx, loc) in resp.locations.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        write_location_json(&mut out, loc);
    }
    out.push_str("]}");
    out
}

fn write_str_array(out: &mut String, items: &[String]) {
    out.push('[');
    for (idx, s) in items.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        push_json_str(out, s);
    }
    out.push(']');
}

fn write_operation_json(out: &mut String, op: &CopilotOperation) {
    match op {
        CopilotOperation::SheetWrite {
            sheet,
            range,
            cells,
        } => {
            out.push_str("{\"kind\":\"sheet.write\",\"sheet\":");
            push_json_str(out, sheet);
            out.push_str(",\"range\":");
            push_json_str(out, range);
            out.push_str(",\"cells\":[");
            for (idx, c) in cells.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                out.push_str("{\"coord\":");
                push_json_str(out, &c.coord);
                out.push_str(",\"raw\":");
                push_json_str(out, &c.raw);
                if let Some(fmt) = &c.num_fmt {
                    out.push_str(",\"numFmt\":");
                    push_json_str(out, fmt);
                }
                if let Some(bold) = c.bold {
                    let _ = write!(out, ",\"bold\":{bold}");
                }
                if let Some(fill) = &c.fill {
                    out.push_str(",\"fill\":");
                    push_json_str(out, fill);
                }
                if let Some(color) = &c.color {
                    out.push_str(",\"color\":");
                    push_json_str(out, color);
                }
                out.push('}');
            }
            out.push_str("]}");
        }
        CopilotOperation::SheetFormat {
            sheet,
            range,
            fill,
            color,
            bold,
            num_fmt,
        } => {
            out.push_str("{\"kind\":\"sheet.format\",\"sheet\":");
            push_json_str(out, sheet);
            out.push_str(",\"range\":");
            push_json_str(out, range);
            if let Some(f) = fill {
                out.push_str(",\"fill\":");
                push_json_str(out, f);
            }
            if let Some(c) = color {
                out.push_str(",\"color\":");
                push_json_str(out, c);
            }
            if let Some(b) = bold {
                let _ = write!(out, ",\"bold\":{b}");
            }
            if let Some(fmt) = num_fmt {
                out.push_str(",\"numFmt\":");
                push_json_str(out, fmt);
            }
            out.push('}');
        }
        CopilotOperation::SheetChart {
            sheet,
            chart_type,
            data_range,
            title,
            legend,
            switch_row_col,
            width,
            height,
        } => {
            out.push_str("{\"kind\":\"sheet.chart\",\"sheet\":");
            push_json_str(out, sheet);
            let _ = write!(
                out,
                ",\"chartType\":\"{}\",\"dataRange\":",
                chart_type.as_str()
            );
            push_json_str(out, data_range);
            out.push_str(",\"title\":");
            push_json_str(out, title);
            let _ = write!(
                out,
                ",\"legend\":\"{}\",\"switchRowCol\":{switch_row_col}",
                legend.as_str()
            );
            if let Some(w) = width {
                let _ = write!(out, ",\"width\":{w}");
            }
            if let Some(h) = height {
                let _ = write!(out, ",\"height\":{h}");
            }
            out.push('}');
        }
        CopilotOperation::DocAppend { blocks } => {
            out.push_str("{\"kind\":\"doc.append\",\"blocks\":[");
            for (idx, b) in blocks.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                match b {
                    DocBlock::Heading { level, text } => {
                        let _ = write!(out, "{{\"heading\":{level},\"text\":");
                        push_json_str(out, text);
                        out.push('}');
                    }
                    DocBlock::Paragraph { text } => {
                        out.push_str("{\"text\":");
                        push_json_str(out, text);
                        out.push('}');
                    }
                    DocBlock::List { kind, items } => {
                        out.push_str("{\"list\":");
                        push_json_str(out, kind);
                        out.push_str(",\"items\":");
                        write_str_array(out, items);
                        out.push('}');
                    }
                }
            }
            out.push_str("]}");
        }
        CopilotOperation::DocReplaceParagraphs { replacements } => {
            out.push_str("{\"kind\":\"doc.replace_paragraphs\",\"replacements\":[");
            for (idx, r) in replacements.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                let _ = write!(out, "{{\"index\":{},\"text\":", r.index);
                push_json_str(out, &r.text);
                out.push('}');
            }
            out.push_str("]}");
        }
        CopilotOperation::DocScrubPii => {
            out.push_str("{\"kind\":\"doc.scrub_pii\"}");
        }
        CopilotOperation::SlideAdd {
            title,
            bullets,
            notes,
        } => {
            out.push_str("{\"kind\":\"slide.add\",\"title\":");
            push_json_str(out, title);
            out.push_str(",\"bullets\":");
            write_str_array(out, bullets);
            out.push_str(",\"notes\":");
            push_json_str(out, notes);
            out.push('}');
        }
        CopilotOperation::SlideUpdate {
            index,
            title,
            bullets,
            notes,
        } => {
            let _ = write!(out, "{{\"kind\":\"slide.update\",\"index\":{index}");
            if let Some(t) = title {
                out.push_str(",\"title\":");
                push_json_str(out, t);
            }
            if let Some(bs) = bullets {
                out.push_str(",\"bullets\":");
                write_str_array(out, bs);
            }
            if let Some(n) = notes {
                out.push_str(",\"notes\":");
                push_json_str(out, n);
            }
            out.push('}');
        }
    }
}

fn write_location_json(out: &mut String, loc: &CopilotLocation) {
    out.push_str("{\"module\":");
    push_json_str(out, &loc.module);
    out.push_str(",\"label\":");
    push_json_str(out, &loc.label);
    if let Some(sheet) = &loc.sheet {
        out.push_str(",\"sheet\":");
        push_json_str(out, sheet);
    }
    if let Some(range) = &loc.range {
        out.push_str(",\"range\":");
        push_json_str(out, range);
    }
    if let Some(p_idx) = loc.paragraph_index {
        let _ = write!(out, ",\"paragraphIndex\":{p_idx}");
    }
    if let Some(s_idx) = loc.slide_index {
        let _ = write!(out, ",\"slideIndex\":{s_idx}");
    }
    out.push('}');
}
