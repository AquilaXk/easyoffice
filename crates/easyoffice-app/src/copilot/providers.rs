//! 참조 구현: continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., `packages/llm-info/src/providers/*` 및 `core/llm/llms/{Anthropic,Gemini,OpenAI,Deepseek,OpenRouter,Groq,Mistral,xAI,Ollama}.ts`) 및 dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd., `server/src/modules/providers/{index,models}.ts`, see THIRD_PARTY_NOTICES.md)

use easyoffice_formats::{EasyOfficeError, StrictJsonValue};
use std::fmt::Write as _;

use super::ssrf::EndpointTarget;
use super::types::{
    push_json_str, CopilotRunRequest, HistoryMessage, ProviderConfig, ProviderKind,
    ReasoningEffort, MAX_DISCOVERED_MODELS, MAX_LLM_REQUEST_BYTES, MAX_OUTPUT_TOKENS,
};

pub const ANTHROPIC_API_VERSION: &str = "2023-06-01";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredModel {
    pub id: String,
    pub name: String,
    pub context_window: Option<u32>,
    pub supports_reasoning: bool,
}

pub fn validate_api_key_safe(api_key: &str) -> Result<&str, EasyOfficeError> {
    let trimmed = api_key.trim();
    if trimmed.bytes().any(|b| !(0x21..=0x7E).contains(&b)) {
        return Err(EasyOfficeError::SecurityViolation(
            "API 키에는 공백·제어 문자·비ASCII 문자를 포함할 수 없습니다.".to_string(),
        ));
    }
    Ok(trimmed)
}

pub fn build_provider_headers(
    provider: &ProviderConfig,
) -> Result<Vec<(String, String)>, EasyOfficeError> {
    let key = validate_api_key_safe(&provider.api_key)?;
    if provider.kind.requires_api_key() && key.is_empty() {
        return Err(EasyOfficeError::InvalidArgument(format!(
            "{} 제공자 API 키를 입력하세요.",
            provider.kind.display_name()
        )));
    }
    match provider.kind {
        ProviderKind::Anthropic => Ok(vec![
            ("x-api-key".to_string(), key.to_string()),
            (
                "anthropic-version".to_string(),
                ANTHROPIC_API_VERSION.to_string(),
            ),
        ]),
        ProviderKind::Google => Ok(vec![("x-goog-api-key".to_string(), key.to_string())]),
        _ => {
            if key.is_empty() {
                Ok(Vec::new())
            } else {
                Ok(vec![("Authorization".to_string(), format!("Bearer {key}"))])
            }
        }
    }
}

pub fn chat_endpoint_target(provider: &ProviderConfig, stream: bool) -> EndpointTarget<'_> {
    match provider.kind {
        ProviderKind::Anthropic => EndpointTarget::AnthropicMessages,
        ProviderKind::Google if stream => EndpointTarget::GoogleStream {
            model: provider.model.trim(),
        },
        ProviderKind::Google => EndpointTarget::GoogleGenerate {
            model: provider.model.trim(),
        },
        ProviderKind::DeepSeek => EndpointTarget::OpenAiChat { default_prefix: "" },
        _ => EndpointTarget::OpenAiChat {
            default_prefix: "/v1",
        },
    }
}

pub fn models_endpoint_target(provider: &ProviderConfig) -> EndpointTarget<'static> {
    match provider.kind {
        ProviderKind::Google => EndpointTarget::ModelsList {
            default_prefix: "/v1beta",
        },
        ProviderKind::DeepSeek => EndpointTarget::ModelsList { default_prefix: "" },
        _ => EndpointTarget::ModelsList {
            default_prefix: "/v1",
        },
    }
}

/// OpenAI 호환 본문의 출력 상한 필드와 값. 공식 OpenAI는 추론 모델이 `max_tokens`를 거부하므로
/// `max_completion_tokens`를 쓰고, 그 밖의 호환 서버는 널리 받는 `max_tokens`를 쓴다.
/// 추론 토큰도 상한에 들어가는 제공자는 추론 예산만큼 더한다(Anthropic 경로와 같은 방식).
/// DeepSeek은 모델별 상한이 8,192라 그대로 둔다.
fn openai_output_cap(kind: ProviderKind, effort: Option<ReasoningEffort>) -> (&'static str, u32) {
    let field = if kind == ProviderKind::OpenAi {
        "max_completion_tokens"
    } else {
        "max_tokens"
    };
    let reasoning_counts = matches!(
        kind,
        ProviderKind::OpenAi | ProviderKind::OpenRouter | ProviderKind::Xai
    );
    let extra = effort
        .filter(|_| reasoning_counts)
        .map_or(0, ReasoningEffort::budget_tokens);
    (field, MAX_OUTPUT_TOKENS + extra)
}

pub fn build_provider_ping_body(provider: &ProviderConfig) -> String {
    let model = provider.model.trim();
    match provider.kind {
        ProviderKind::Anthropic => {
            let mut out = String::with_capacity(256);
            out.push_str("{\"model\":");
            push_json_str(&mut out, model);
            out.push_str(",\"max_tokens\":8,\"messages\":[{\"role\":\"user\",\"content\":\"ping\"}]}");
            out
        }
        ProviderKind::Google => {
            "{\"contents\":[{\"role\":\"user\",\"parts\":[{\"text\":\"ping\"}]}],\"generationConfig\":{\"maxOutputTokens\":8}}"
                .to_string()
        }
        _ => {
            let mut out = String::with_capacity(256);
            out.push_str("{\"model\":");
            push_json_str(&mut out, model);
            let (field, _) = openai_output_cap(provider.kind, None);
            let _ = write!(
                out,
                ",\"messages\":[{{\"role\":\"user\",\"content\":\"ping\"}}],\"{field}\":8}}"
            );
            out
        }
    }
}

fn coalesce_turns<'a>(
    history: &'a [HistoryMessage],
    user_content: &'a str,
) -> Vec<(&'static str, String)> {
    let mut turns: Vec<(&'static str, String)> = Vec::with_capacity(history.len() + 1);
    for h in history {
        let role = if h.role == "assistant" {
            "assistant"
        } else {
            "user"
        };
        if turns.is_empty() && role != "user" {
            continue;
        }
        if let Some(last) = turns.last_mut() {
            if last.0 == role {
                last.1.push_str("\n\n");
                last.1.push_str(&h.text);
                continue;
            }
        }
        turns.push((role, h.text.clone()));
    }
    if let Some(last) = turns.last_mut() {
        if last.0 == "user" {
            last.1.push_str("\n\n");
            last.1.push_str(user_content);
            return turns;
        }
    }
    turns.push(("user", user_content.to_string()));
    turns
}

/// `stream`이면 Anthropic·OpenAI 호환 본문에 `"stream":true`를 넣는다(Gemini는 주소가 스트림 메서드로 달라지고 본문은 같다).
pub fn build_provider_chat_body(
    req: &CopilotRunRequest,
    system_prompt: &str,
    user_content: &str,
    stream: bool,
) -> Result<String, EasyOfficeError> {
    let model = req.provider.model.trim();
    if model.is_empty() {
        return Err(EasyOfficeError::InvalidArgument(
            "모델 설정에서 사용할 모델을 먼저 선택하세요.".to_string(),
        ));
    }
    let effort = req.provider.reasoning_effort;
    let body = match req.provider.kind {
        ProviderKind::Anthropic => {
            let turns = coalesce_turns(&req.history, user_content);
            let budget = effort.map_or(0, |e| e.budget_tokens());
            let max_tokens = MAX_OUTPUT_TOKENS + budget;
            let mut out = String::with_capacity(system_prompt.len() + user_content.len() + 512);
            out.push_str("{\"model\":");
            push_json_str(&mut out, model);
            let _ = write!(out, ",\"max_tokens\":{max_tokens},\"system\":");
            push_json_str(&mut out, system_prompt);
            out.push_str(",\"messages\":[");
            for (idx, (role, text)) in turns.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                out.push_str("{\"role\":");
                push_json_str(&mut out, role);
                out.push_str(",\"content\":");
                push_json_str(&mut out, text);
                out.push('}');
            }
            out.push(']');
            if budget > 0 {
                let _ = write!(
                    out,
                    ",\"thinking\":{{\"type\":\"enabled\",\"budget_tokens\":{budget}}}"
                );
            } else {
                out.push_str(",\"temperature\":0.2");
            }
            if stream {
                out.push_str(",\"stream\":true");
            }
            out.push('}');
            out
        }
        ProviderKind::Google => {
            let turns = coalesce_turns(&req.history, user_content);
            let budget = effort.map_or(0, |e| e.budget_tokens());
            let mut out = String::with_capacity(system_prompt.len() + user_content.len() + 512);
            out.push_str("{\"systemInstruction\":{\"parts\":[{\"text\":");
            push_json_str(&mut out, system_prompt);
            out.push_str("}]},\"contents\":[");
            for (idx, (role, text)) in turns.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                let g_role = if *role == "assistant" {
                    "model"
                } else {
                    "user"
                };
                out.push_str("{\"role\":");
                push_json_str(&mut out, g_role);
                out.push_str(",\"parts\":[{\"text\":");
                push_json_str(&mut out, text);
                out.push_str("}]}");
            }
            let _ = write!(
                out,
                "],\"generationConfig\":{{\"temperature\":0.2,\"maxOutputTokens\":{MAX_OUTPUT_TOKENS}"
            );
            if budget > 0 {
                let _ = write!(
                    out,
                    ",\"thinkingConfig\":{{\"includeThoughts\":true,\"thinkingBudget\":{budget}}}"
                );
            }
            out.push_str("}}");
            out
        }
        _ => {
            let mut out = String::with_capacity(system_prompt.len() + user_content.len() + 512);
            out.push_str("{\"model\":");
            push_json_str(&mut out, model);
            out.push_str(",\"messages\":[{\"role\":\"system\",\"content\":");
            push_json_str(&mut out, system_prompt);
            out.push('}');
            for h in &req.history {
                out.push_str(",{\"role\":");
                push_json_str(&mut out, &h.role);
                out.push_str(",\"content\":");
                push_json_str(&mut out, &h.text);
                out.push('}');
            }
            out.push_str(",{\"role\":\"user\",\"content\":");
            push_json_str(&mut out, user_content);
            out.push_str("}]");
            let (cap_field, cap) = openai_output_cap(req.provider.kind, effort);
            let _ = write!(out, ",\"{cap_field}\":{cap}");
            if let Some(level) = effort {
                if req.provider.kind == ProviderKind::DeepSeek {
                    out.push_str(",\"thinking\":{\"type\":\"enabled\"},\"reasoning_effort\":");
                    push_json_str(&mut out, level.as_str());
                } else if matches!(
                    req.provider.kind,
                    ProviderKind::OpenAi | ProviderKind::OpenRouter | ProviderKind::Xai
                ) {
                    out.push_str(",\"reasoning_effort\":");
                    push_json_str(&mut out, level.as_str());
                } else if !model.contains("reasoner") {
                    out.push_str(",\"temperature\":0.2");
                }
            } else if !model.contains("reasoner") {
                out.push_str(",\"temperature\":0.2");
            }
            if stream {
                out.push_str(",\"stream\":true");
            }
            out.push('}');
            out
        }
    };

    if body.len() > MAX_LLM_REQUEST_BYTES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "모델 요청 본문 크기({} bytes)가 상한({MAX_LLM_REQUEST_BYTES} bytes)을 초과했습니다.",
            body.len()
        )));
    }
    Ok(body)
}

pub fn extract_provider_completion(
    kind: ProviderKind,
    raw_json: &str,
) -> Result<(String, String), EasyOfficeError> {
    let parsed = StrictJsonValue::parse(raw_json).map_err(|_| {
        EasyOfficeError::InvalidArgument("모델 응답이 유효한 JSON이 아닙니다.".to_string())
    })?;
    let root = parsed.as_object().ok_or_else(|| {
        EasyOfficeError::InvalidArgument("모델 응답 루트는 JSON 객체여야 합니다.".to_string())
    })?;

    if kind == ProviderKind::Anthropic {
        if let Some(blocks) =
            StrictJsonValue::get_field(root, "content").and_then(StrictJsonValue::as_array)
        {
            let mut text_out = String::new();
            let mut thinking_out = String::new();
            for block in blocks {
                let Some(b_obj) = block.as_object() else {
                    continue;
                };
                match StrictJsonValue::get_str(b_obj, "type").unwrap_or("") {
                    "thinking" => {
                        if let Some(t) = StrictJsonValue::get_str(b_obj, "thinking") {
                            if !thinking_out.is_empty() {
                                thinking_out.push('\n');
                            }
                            thinking_out.push_str(t);
                        }
                    }
                    "text" => {
                        if let Some(t) = StrictJsonValue::get_str(b_obj, "text") {
                            text_out.push_str(t);
                        }
                    }
                    _ => {}
                }
            }
            if !text_out.is_empty() {
                return Ok((text_out, thinking_out));
            }
        }
    }

    if kind == ProviderKind::Google {
        if let Some(cands) =
            StrictJsonValue::get_field(root, "candidates").and_then(StrictJsonValue::as_array)
        {
            if let Some(first) = cands.first().and_then(StrictJsonValue::as_object) {
                if let Some(content_obj) = StrictJsonValue::get_field(first, "content")
                    .and_then(StrictJsonValue::as_object)
                {
                    if let Some(parts) = StrictJsonValue::get_field(content_obj, "parts")
                        .and_then(StrictJsonValue::as_array)
                    {
                        let mut text_out = String::new();
                        let mut thinking_out = String::new();
                        for part in parts {
                            let Some(p_obj) = part.as_object() else {
                                continue;
                            };
                            let is_thought = StrictJsonValue::get_field(p_obj, "thought")
                                .and_then(StrictJsonValue::as_bool)
                                .unwrap_or(false);
                            if let Some(t) = StrictJsonValue::get_str(p_obj, "text") {
                                if is_thought {
                                    if !thinking_out.is_empty() {
                                        thinking_out.push('\n');
                                    }
                                    thinking_out.push_str(t);
                                } else {
                                    text_out.push_str(t);
                                }
                            }
                        }
                        if !text_out.is_empty() {
                            return Ok((text_out, thinking_out));
                        }
                    }
                }
            }
        }
    }

    if let Some(choices) =
        StrictJsonValue::get_field(root, "choices").and_then(StrictJsonValue::as_array)
    {
        if let Some(first) = choices.first().and_then(StrictJsonValue::as_object) {
            if let Some(msg) =
                StrictJsonValue::get_field(first, "message").and_then(StrictJsonValue::as_object)
            {
                let content = StrictJsonValue::get_str(msg, "content")
                    .unwrap_or("")
                    .to_string();
                let reasoning = StrictJsonValue::get_str(msg, "reasoning")
                    .or_else(|| StrictJsonValue::get_str(msg, "reasoning_content"))
                    .unwrap_or("")
                    .to_string();
                if !content.is_empty() {
                    return Ok((content, reasoning));
                }
            }
            if let Some(text) = StrictJsonValue::get_str(first, "text") {
                return Ok((text.to_string(), String::new()));
            }
        }
    }
    if StrictJsonValue::get_str(root, "reply").is_some() {
        return Ok((raw_json.to_string(), String::new()));
    }
    Err(EasyOfficeError::InvalidArgument(
        "모델 응답에서 메시지 본문을 찾을 수 없습니다.".to_string(),
    ))
}

fn is_non_chat_model_id(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    lower.contains("embedding")
        || lower.starts_with("tts-")
        || lower.contains("-tts")
        || lower.starts_with("whisper")
        || lower.starts_with("dall-e")
        || lower.contains("moderation")
        || lower.starts_with("babbage")
        || lower.starts_with("davinci-002")
        || lower.contains("rerank")
        || lower.starts_with("imagen")
        || lower.starts_with("veo-")
        || lower.contains("-transcribe")
        || lower.contains("-realtime-")
        || lower == "aqa"
}

fn infer_reasoning_support(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    if lower.contains("flash-lite") && lower.contains("gemini-2.5") {
        return false;
    }
    lower.contains("reason")
        || lower.contains("thinking")
        || lower.contains("r1")
        || lower.contains("qwq")
        || lower.starts_with("o1")
        || lower.starts_with("o3")
        || lower.starts_with("o4")
        || lower.contains("/o1")
        || lower.contains("/o3")
        || lower.contains("/o4")
        || lower.contains("gpt-5")
        || lower.contains("gpt-6")
        || lower.contains("claude-3-7")
        || lower.contains("claude-sonnet-4")
        || lower.contains("claude-opus-4")
        || lower.contains("claude-haiku-4")
        || lower.contains("claude-4")
        || lower.contains("claude-5")
        || lower.contains("gemini-2.5")
        || lower.contains("gemini-3")
        || lower.contains("gemini-4")
        || lower.contains("magistral")
        || lower.contains("grok-3-mini")
        || lower.contains("grok-4")
        || lower.contains("qwen3")
}

fn has_explicit_reasoning_meta(obj: &[(String, StrictJsonValue)]) -> Option<bool> {
    if StrictJsonValue::get_field(obj, "effort").is_some() {
        return Some(true);
    }
    if let Some(thinking) = StrictJsonValue::get_field(obj, "thinking") {
        if let Some(flag) = thinking.as_bool() {
            return Some(flag);
        }
        if thinking.as_object().is_some() {
            return Some(true);
        }
    }
    if let Some(params) =
        StrictJsonValue::get_field(obj, "supported_parameters").and_then(StrictJsonValue::as_array)
    {
        if params.iter().any(|p| {
            matches!(
                p.as_str(),
                Some("reasoning") | Some("reasoning_effort") | Some("include_reasoning")
            )
        }) {
            return Some(true);
        }
    }
    None
}

fn is_safe_discovered_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 150
        && !id.contains("..")
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':' | b'/'))
}

fn parse_data_model_items(items: &[StrictJsonValue]) -> Vec<DiscoveredModel> {
    let mut out = Vec::new();
    for item in items.iter().take(MAX_DISCOVERED_MODELS) {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let Some(id) = StrictJsonValue::get_str(obj, "id").map(str::trim) else {
            continue;
        };
        if !is_safe_discovered_id(id) || is_non_chat_model_id(id) {
            continue;
        }
        let name = StrictJsonValue::get_str(obj, "display_name")
            .or_else(|| StrictJsonValue::get_str(obj, "name"))
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(id)
            .trim()
            .to_string();
        let context_window = StrictJsonValue::get_field(obj, "context_window")
            .or_else(|| StrictJsonValue::get_field(obj, "context_length"))
            .and_then(StrictJsonValue::as_u64)
            .and_then(|v| u32::try_from(v).ok());
        let supports_reasoning =
            has_explicit_reasoning_meta(obj).unwrap_or_else(|| infer_reasoning_support(id));
        out.push(DiscoveredModel {
            id: id.to_string(),
            name,
            context_window,
            supports_reasoning,
        });
    }
    out
}

pub fn parse_discovered_models(
    kind: ProviderKind,
    raw_json: &str,
) -> Result<Vec<DiscoveredModel>, EasyOfficeError> {
    let parsed = StrictJsonValue::parse(raw_json).map_err(|_| {
        EasyOfficeError::InvalidArgument("모델 목록 응답이 유효한 JSON이 아닙니다.".to_string())
    })?;
    if let Some(arr) = parsed.as_array() {
        return Ok(parse_data_model_items(arr));
    }
    let root = parsed.as_object().ok_or_else(|| {
        EasyOfficeError::InvalidArgument(
            "모델 목록 루트는 JSON 객체 또는 배열이어야 합니다.".to_string(),
        )
    })?;

    if let Some(data_arr) =
        StrictJsonValue::get_field(root, "data").and_then(StrictJsonValue::as_array)
    {
        return Ok(parse_data_model_items(data_arr));
    }

    let mut out = Vec::new();
    if let Some(models_arr) =
        StrictJsonValue::get_field(root, "models").and_then(StrictJsonValue::as_array)
    {
        for item in models_arr.iter().take(MAX_DISCOVERED_MODELS) {
            let Some(obj) = item.as_object() else {
                continue;
            };
            if kind == ProviderKind::Google {
                if let Some(methods) = StrictJsonValue::get_field(obj, "supportedGenerationMethods")
                    .and_then(StrictJsonValue::as_array)
                {
                    let can_gen = methods
                        .iter()
                        .any(|m| m.as_str() == Some("generateContent"));
                    if !can_gen {
                        continue;
                    }
                }
            }
            let Some(raw_name) = StrictJsonValue::get_str(obj, "name")
                .or_else(|| StrictJsonValue::get_str(obj, "model"))
                .map(str::trim)
            else {
                continue;
            };
            let clean_id = if kind == ProviderKind::Google {
                raw_name.strip_prefix("models/").unwrap_or(raw_name)
            } else {
                raw_name
            };
            if !is_safe_discovered_id(clean_id) || is_non_chat_model_id(clean_id) {
                continue;
            }
            let display_name = StrictJsonValue::get_str(obj, "displayName")
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(clean_id)
                .trim()
                .to_string();
            let context_window = StrictJsonValue::get_field(obj, "inputTokenLimit")
                .and_then(StrictJsonValue::as_u64)
                .and_then(|v| u32::try_from(v).ok());
            let supports_reasoning = has_explicit_reasoning_meta(obj)
                .unwrap_or_else(|| infer_reasoning_support(clean_id));
            out.push(DiscoveredModel {
                id: clean_id.to_string(),
                name: display_name,
                context_window,
                supports_reasoning,
            });
        }
    }

    Ok(out)
}

pub fn serialize_models_response(
    kind: ProviderKind,
    models: &[DiscoveredModel],
    live: bool,
) -> String {
    let mut out = String::with_capacity(256 + models.len() * 96);
    out.push_str("{\"status\":\"ok\",\"provider\":");
    push_json_str(&mut out, kind.as_id());
    let _ = write!(out, ",\"live\":{live},\"models\":[");
    for (idx, m) in models.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push_str("{\"id\":");
        push_json_str(&mut out, &m.id);
        out.push_str(",\"name\":");
        push_json_str(&mut out, &m.name);
        if let Some(ctx) = m.context_window {
            let _ = write!(out, ",\"contextWindow\":{ctx}");
        }
        let _ = write!(out, ",\"supportsReasoning\":{}}}", m.supports_reasoning);
    }
    out.push_str("]}");
    out
}
