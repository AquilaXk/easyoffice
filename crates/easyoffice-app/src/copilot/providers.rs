//! 참조 구현: continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., `packages/llm-info/src/providers/*` 및 `core/llm/llms/{Anthropic,Gemini,OpenAI,Deepseek,OpenRouter,Groq,Mistral,xAI,Ollama}.ts`) 및 dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd., `server/src/modules/providers/{index,models}.ts`, see THIRD_PARTY_NOTICES.md)

use easyoffice_formats::{EasyOfficeError, StrictJsonValue};
use std::fmt::Write as _;

use super::cache::{
    assemble_ordered_turns_with_volatile, build_prompt_cache_key, is_official_anthropic_host,
    is_official_openai_host, is_openrouter_anthropic_cache, supports_openai_stream_usage,
    write_anthropic_system_and_messages, write_google_contents, write_openai_messages_with_cache,
};
use super::schema::{
    has_emitted_tools, synthesize_completion_from_tools, write_provider_tools_field,
    NativeToolInvocation,
};
use super::ssrf::EndpointTarget;
use super::types::{
    push_json_str, CopilotRunRequest, HistoryMessage, ProviderConfig, ProviderKind,
    ReasoningEffort, MAX_DISCOVERED_MODELS, MAX_LLM_REQUEST_BYTES, MAX_OUTPUT_TOKENS,
};
use super::usage::{extract_usage_from_root, TokenUsage};

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

fn openai_output_cap(
    kind: ProviderKind,
    base_cap: u32,
    effort: Option<ReasoningEffort>,
) -> (&'static str, u32) {
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
    (field, base_cap + extra)
}

pub fn build_provider_ping_body(provider: &ProviderConfig) -> String {
    let model = provider.model.trim();
    match provider.kind {
        ProviderKind::Anthropic => {
            let mut out = String::with_capacity(256);
            out.push_str("{\"model\":");
            push_json_str(&mut out, model);
            out.push_str(
                ",\"max_tokens\":1,\"messages\":[{\"role\":\"user\",\"content\":\"ping\"}]}",
            );
            out
        }
        ProviderKind::Google => {
            "{\"contents\":[{\"role\":\"user\",\"parts\":[{\"text\":\"ping\"}]}],\"generationConfig\":{\"maxOutputTokens\":1}}"
                .to_string()
        }
        _ => {
            let mut out = String::with_capacity(256);
            out.push_str("{\"model\":");
            push_json_str(&mut out, model);
            let (field, _) = openai_output_cap(provider.kind, MAX_OUTPUT_TOKENS, None);
            let _ = write!(
                out,
                ",\"messages\":[{{\"role\":\"user\",\"content\":\"ping\"}}],\"{field}\":1}}"
            );
            out
        }
    }
}

fn is_gemini_3_or_4(clean_model_lower: &str) -> bool {
    clean_model_lower.starts_with("gemini-3")
        || clean_model_lower.starts_with("gemini-4")
        || clean_model_lower.contains("/gemini-3")
        || clean_model_lower.contains("/gemini-4")
}

pub fn build_provider_chat_body(
    req: &CopilotRunRequest,
    system_prompt: &str,
    user_content: &str,
    stream: bool,
) -> Result<String, EasyOfficeError> {
    build_provider_chat_body_with_history(req, &req.history, system_prompt, user_content, stream)
}

pub fn build_provider_chat_body_with_history(
    req: &CopilotRunRequest,
    history: &[HistoryMessage],
    system_prompt: &str,
    user_content: &str,
    stream: bool,
) -> Result<String, EasyOfficeError> {
    build_provider_chat_body_with_parts(
        req,
        history,
        "",
        user_content,
        &req.loop_steps,
        stream,
        system_prompt,
    )
}

pub fn build_provider_chat_body_with_parts(
    req: &CopilotRunRequest,
    prior_history: &[HistoryMessage],
    context_block: &str,
    user_prompt: &str,
    loop_steps: &[HistoryMessage],
    stream: bool,
    system_prompt: &str,
) -> Result<String, EasyOfficeError> {
    build_provider_chat_body_with_all_parts(
        req,
        prior_history,
        context_block,
        "",
        user_prompt,
        loop_steps,
        stream,
        system_prompt,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_provider_chat_body_with_all_parts(
    req: &CopilotRunRequest,
    prior_history: &[HistoryMessage],
    context_block: &str,
    volatile_state: &str,
    user_prompt: &str,
    loop_steps: &[HistoryMessage],
    stream: bool,
    system_prompt: &str,
) -> Result<String, EasyOfficeError> {
    let model = req.provider.model.trim();
    if model.is_empty() {
        return Err(EasyOfficeError::InvalidArgument(
            "모델 설정에서 사용할 모델을 먼저 선택하세요.".to_string(),
        ));
    }
    let effort = req.effective_reasoning_effort();
    let base_cap = req.effective_output_base_cap();
    let is_reasoning = infer_reasoning_support(model);
    let body = match req.provider.kind {
        ProviderKind::Anthropic => {
            let turns = assemble_ordered_turns_with_volatile(
                req,
                prior_history,
                context_block,
                volatile_state,
                user_prompt,
                loop_steps,
                true,
            );
            let budget = if req.lightweight {
                0
            } else {
                effort
                    .filter(|_| is_reasoning)
                    .map_or(0, ReasoningEffort::budget_tokens)
            };
            let max_tokens = base_cap + budget;
            let official_host = is_official_anthropic_host(&req.provider.base_url);
            let allow_cache_breakpoints =
                !req.lightweight && req.command.is_none() && official_host;
            let cache_system_breakpoint = !has_emitted_tools(req);
            let mut out = String::with_capacity(
                system_prompt.len()
                    + context_block.len()
                    + volatile_state.len()
                    + user_prompt.len()
                    + 1024,
            );
            out.push_str("{\"model\":");
            push_json_str(&mut out, model);
            let _ = write!(out, ",\"max_tokens\":{max_tokens}");
            write_anthropic_system_and_messages(
                &mut out,
                system_prompt,
                &turns,
                official_host,
                allow_cache_breakpoints,
                cache_system_breakpoint,
            );
            write_provider_tools_field(&mut out, req);
            if !req.lightweight {
                if budget > 0 {
                    let _ = write!(
                        out,
                        ",\"thinking\":{{\"type\":\"enabled\",\"budget_tokens\":{budget}}}"
                    );
                } else if !is_reasoning {
                    out.push_str(",\"temperature\":0.2");
                }
            }
            if stream {
                out.push_str(",\"stream\":true");
            }
            out.push('}');
            out
        }
        ProviderKind::Google => {
            let turns = assemble_ordered_turns_with_volatile(
                req,
                prior_history,
                context_block,
                volatile_state,
                user_prompt,
                loop_steps,
                true,
            );
            let clean_lower = model
                .strip_prefix("models/")
                .unwrap_or(model)
                .to_ascii_lowercase();
            let gemini_34 = is_gemini_3_or_4(&clean_lower);
            let budget = if req.lightweight {
                0
            } else {
                effort
                    .filter(|_| is_reasoning || gemini_34)
                    .map_or(0, ReasoningEffort::budget_tokens)
            };
            let max_out = base_cap + budget;
            let mut out = String::with_capacity(
                system_prompt.len()
                    + context_block.len()
                    + volatile_state.len()
                    + user_prompt.len()
                    + 1024,
            );
            out.push_str("{\"systemInstruction\":{\"parts\":[{\"text\":");
            push_json_str(&mut out, system_prompt);
            out.push_str("}]}");
            write_google_contents(&mut out, &turns);
            write_provider_tools_field(&mut out, req);
            out.push_str(",\"generationConfig\":{");
            if !gemini_34 && !is_reasoning && budget == 0 {
                out.push_str("\"temperature\":0.2,");
            }
            let _ = write!(out, "\"maxOutputTokens\":{max_out}");
            if !req.lightweight {
                if gemini_34 {
                    if let Some(level) = effort {
                        out.push_str(
                            ",\"thinkingConfig\":{\"includeThoughts\":true,\"thinkingLevel\":",
                        );
                        push_json_str(&mut out, level.as_str());
                        out.push('}');
                    }
                } else if budget > 0 {
                    let _ = write!(
                        out,
                        ",\"thinkingConfig\":{{\"includeThoughts\":true,\"thinkingBudget\":{budget}}}"
                    );
                }
            }
            out.push_str("}}");
            out
        }
        _ => {
            let or_anth_cache = is_openrouter_anthropic_cache(req);
            let turns = assemble_ordered_turns_with_volatile(
                req,
                prior_history,
                context_block,
                volatile_state,
                user_prompt,
                loop_steps,
                false,
            );
            let mut out = String::with_capacity(
                system_prompt.len()
                    + context_block.len()
                    + volatile_state.len()
                    + user_prompt.len()
                    + 1024,
            );
            out.push_str("{\"model\":");
            push_json_str(&mut out, model);
            write_openai_messages_with_cache(
                &mut out,
                system_prompt,
                &turns,
                or_anth_cache,
                !has_emitted_tools(req),
            );
            let (cap_field, cap) = openai_output_cap(req.provider.kind, base_cap, effort);
            let _ = write!(out, ",\"{cap_field}\":{cap}");
            write_provider_tools_field(&mut out, req);
            if req.provider.kind == ProviderKind::OpenAi
                && is_official_openai_host(&req.provider.base_url)
            {
                let cache_key = build_prompt_cache_key(req);
                out.push_str(",\"prompt_cache_key\":");
                push_json_str(&mut out, &cache_key);
            }
            if !req.lightweight {
                if let Some(level) = effort {
                    if req.provider.kind == ProviderKind::DeepSeek && is_reasoning {
                        out.push_str(",\"thinking\":{\"type\":\"enabled\"},\"reasoning_effort\":");
                        push_json_str(&mut out, level.as_str());
                    } else if matches!(
                        req.provider.kind,
                        ProviderKind::OpenAi | ProviderKind::OpenRouter | ProviderKind::Xai
                    ) && is_reasoning
                    {
                        out.push_str(",\"reasoning_effort\":");
                        push_json_str(&mut out, level.as_str());
                    } else if !is_reasoning {
                        out.push_str(",\"temperature\":0.2");
                    }
                } else if !is_reasoning {
                    out.push_str(",\"temperature\":0.2");
                }
            } else if !is_reasoning {
                out.push_str(",\"temperature\":0.2");
            }
            if stream {
                out.push_str(",\"stream\":true");
                if supports_openai_stream_usage(req) {
                    out.push_str(",\"stream_options\":{\"include_usage\":true}");
                }
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

fn extract_from_root_object(
    kind: ProviderKind,
    root: &[(String, StrictJsonValue)],
    raw_fallback: Option<&str>,
    text_out: &mut String,
    thinking_out: &mut String,
    tools_out: &mut Vec<NativeToolInvocation>,
) -> bool {
    let mut matched = false;
    if kind == ProviderKind::Anthropic {
        if let Some(blocks) =
            StrictJsonValue::get_field(root, "content").and_then(StrictJsonValue::as_array)
        {
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
                            matched = true;
                        }
                    }
                    "tool_use" => {
                        if let Some(name) = StrictJsonValue::get_str(b_obj, "name") {
                            let input = StrictJsonValue::get_field(b_obj, "input")
                                .map_or_else(|| "{}".to_string(), StrictJsonValue::to_json_string);
                            tools_out.push(NativeToolInvocation {
                                name: name.to_string(),
                                arguments_json: input,
                            });
                            matched = true;
                        }
                    }
                    _ => {}
                }
            }
            if matched {
                return true;
            }
        }
    }

    if let Some(cands) =
        StrictJsonValue::get_field(root, "candidates").and_then(StrictJsonValue::as_array)
    {
        if let Some(first) = cands.first().and_then(StrictJsonValue::as_object) {
            if let Some(content_obj) =
                StrictJsonValue::get_field(first, "content").and_then(StrictJsonValue::as_object)
            {
                if let Some(parts) = StrictJsonValue::get_field(content_obj, "parts")
                    .and_then(StrictJsonValue::as_array)
                {
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
                                matched = true;
                            }
                        }
                        if let Some(fc) = StrictJsonValue::get_field(p_obj, "functionCall")
                            .and_then(StrictJsonValue::as_object)
                        {
                            if let Some(name) = StrictJsonValue::get_str(fc, "name") {
                                let args = StrictJsonValue::get_field(fc, "args").map_or_else(
                                    || "{}".to_string(),
                                    StrictJsonValue::to_json_string,
                                );
                                tools_out.push(NativeToolInvocation {
                                    name: name.to_string(),
                                    arguments_json: args,
                                });
                                matched = true;
                            }
                        }
                    }
                    if matched {
                        return true;
                    }
                }
            }
        }
    }

    if let Some(choices) =
        StrictJsonValue::get_field(root, "choices").and_then(StrictJsonValue::as_array)
    {
        if let Some(first) = choices.first().and_then(StrictJsonValue::as_object) {
            let msg_or_delta = StrictJsonValue::get_field(first, "message")
                .or_else(|| StrictJsonValue::get_field(first, "delta"))
                .and_then(StrictJsonValue::as_object);
            if let Some(msg) = msg_or_delta {
                if let Some(c) = StrictJsonValue::get_str(msg, "content") {
                    if !c.is_empty() {
                        text_out.push_str(c);
                        matched = true;
                    }
                }
                if let Some(r) = StrictJsonValue::get_str(msg, "reasoning")
                    .or_else(|| StrictJsonValue::get_str(msg, "reasoning_content"))
                {
                    if !r.is_empty() {
                        thinking_out.push_str(r);
                    }
                }
                if let Some(tcs) = StrictJsonValue::get_field(msg, "tool_calls")
                    .and_then(StrictJsonValue::as_array)
                {
                    for tc in tcs {
                        if let Some(fn_obj) = tc
                            .as_object()
                            .and_then(|o| StrictJsonValue::get_field(o, "function"))
                            .and_then(StrictJsonValue::as_object)
                        {
                            if let Some(name) = StrictJsonValue::get_str(fn_obj, "name") {
                                let args = StrictJsonValue::get_str(fn_obj, "arguments")
                                    .unwrap_or("{}")
                                    .to_string();
                                tools_out.push(NativeToolInvocation {
                                    name: name.to_string(),
                                    arguments_json: args,
                                });
                                matched = true;
                            }
                        }
                    }
                }
                if matched {
                    return true;
                }
            }
            if let Some(text) = StrictJsonValue::get_str(first, "text") {
                text_out.push_str(text);
                return true;
            }
        }
    }

    if let Some(raw) = raw_fallback {
        if StrictJsonValue::get_str(root, "reply").is_some() {
            text_out.push_str(raw);
            return true;
        }
    }
    false
}

pub fn extract_provider_completion(
    kind: ProviderKind,
    raw_json: &str,
) -> Result<(String, String), EasyOfficeError> {
    let (content, thinking, _) = extract_provider_completion_with_usage(kind, raw_json)?;
    Ok((content, thinking))
}

pub fn extract_provider_completion_with_usage(
    kind: ProviderKind,
    raw_json: &str,
) -> Result<(String, String, Option<TokenUsage>), EasyOfficeError> {
    let parsed = StrictJsonValue::parse(raw_json).map_err(|_| {
        EasyOfficeError::InvalidArgument("모델 응답이 유효한 JSON이 아닙니다.".to_string())
    })?;
    let mut text_out = String::new();
    let mut thinking_out = String::new();
    let mut tools_out = Vec::new();
    let mut usage = TokenUsage::default();
    let mut has_usage = false;

    if let Some(arr) = parsed.as_array() {
        let mut any_matched = false;
        for item in arr {
            let Some(obj) = item.as_object() else {
                continue;
            };
            if let Some(err) = StrictJsonValue::get_field(obj, "error") {
                let msg = err
                    .as_object()
                    .and_then(|o| StrictJsonValue::get_str(o, "message"))
                    .or_else(|| err.as_str())
                    .unwrap_or("모델 서버가 오류를 반환했습니다.");
                return Err(EasyOfficeError::InvalidArgument(msg.to_string()));
            }
            if let Some(u) = extract_usage_from_root(kind, obj) {
                usage.merge_latest(u);
                has_usage = true;
            }
            any_matched |= extract_from_root_object(
                kind,
                obj,
                None,
                &mut text_out,
                &mut thinking_out,
                &mut tools_out,
            );
        }
        if any_matched && (!text_out.is_empty() || !tools_out.is_empty()) {
            let content = synthesize_completion_from_tools(text_out, &tools_out);
            return Ok((content, thinking_out, has_usage.then_some(usage)));
        }
        return Err(EasyOfficeError::InvalidArgument(
            "모델 응답 배열에서 메시지 본문을 찾을 수 없습니다.".to_string(),
        ));
    }

    let root = parsed.as_object().ok_or_else(|| {
        EasyOfficeError::InvalidArgument(
            "모델 응답 루트는 JSON 객체 또는 배열이어야 합니다.".to_string(),
        )
    })?;
    if let Some(u) = extract_usage_from_root(kind, root) {
        usage.merge_latest(u);
        has_usage = true;
    }
    if extract_from_root_object(
        kind,
        root,
        Some(raw_json),
        &mut text_out,
        &mut thinking_out,
        &mut tools_out,
    ) && (!text_out.is_empty() || !tools_out.is_empty())
    {
        let content = synthesize_completion_from_tools(text_out, &tools_out);
        return Ok((content, thinking_out, has_usage.then_some(usage)));
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

pub fn infer_reasoning_support(id: &str) -> bool {
    let lower = id
        .strip_prefix("models/")
        .unwrap_or(id)
        .to_ascii_lowercase();
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
