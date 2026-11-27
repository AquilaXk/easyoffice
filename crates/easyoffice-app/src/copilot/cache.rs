//! 4계층 프롬프트 캐시 메시지 조립, OpenAI prompt_cache_key 정규화 및 Anthropic 캐시 브레이크포인트 직렬화.
// 참조 구현: tribgames/mixdog (Apache-2.0, d5ab088efcc2680f0bb4c43448255ca1653fb3c6),
// anomalyco/opencode (MIT) — 정적 도구·시스템 고정, 64자 SHA-256 캐시 키, 계층형 캐시 배치.

use sha2::{Digest, Sha256};
use std::fmt::Write as _;

use super::types::{push_json_str, CopilotModule, CopilotRunRequest, HistoryMessage, ProviderKind};

const MAX_CACHE_SEED_BYTES: usize = 512;

/// 계층형 대화 배치의 한 텍스트 블록(Anthropic 캐시 브레이크포인트 여부 포함).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnBlock {
    pub text: String,
    pub cache_breakpoint: bool,
}

/// 5계층 대화 배치에서 한 턴의 역할과 연속 텍스트 블록 목록이다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnMessage {
    pub role: &'static str,
    pub blocks: Vec<TurnBlock>,
}

pub(super) fn clamp_utf8_boundary(s: &str, mut end: usize) -> usize {
    if end >= s.len() {
        return s.len();
    }
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    end
}

fn extract_host_lower(base_url: &str) -> String {
    let trimmed = base_url.trim();
    let after_scheme = trimmed.split_once("://").map_or(trimmed, |(_, rest)| rest);
    let authority = after_scheme.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, hp)| hp);
    let host = if let Some(inner) = host_port.strip_prefix('[') {
        inner.split(']').next().unwrap_or(inner)
    } else {
        host_port.split(':').next().unwrap_or(host_port)
    };
    host.to_ascii_lowercase()
}

pub fn is_official_anthropic_host(base_url: &str) -> bool {
    extract_host_lower(base_url) == "api.anthropic.com"
}

pub fn is_official_openai_host(base_url: &str) -> bool {
    extract_host_lower(base_url) == "api.openai.com"
}

pub fn is_official_deepseek_host(base_url: &str) -> bool {
    extract_host_lower(base_url) == "api.deepseek.com"
}

pub fn is_official_openrouter_host(base_url: &str) -> bool {
    extract_host_lower(base_url) == "openrouter.ai"
}

pub fn is_official_xai_host(base_url: &str) -> bool {
    extract_host_lower(base_url) == "api.x.ai"
}

/// 네이티브 함수 호출(`tools`)을 안전하게 켤 수 있는 공식 제공자·호스트인지 판별한다.
pub fn uses_native_tools(req: &CopilotRunRequest) -> bool {
    match req.provider.kind {
        ProviderKind::OpenAi | ProviderKind::Anthropic | ProviderKind::Google => true,
        ProviderKind::DeepSeek => is_official_deepseek_host(&req.provider.base_url),
        ProviderKind::OpenRouter => is_official_openrouter_host(&req.provider.base_url),
        ProviderKind::Xai => is_official_xai_host(&req.provider.base_url),
        _ => false,
    }
}

/// 공식 OpenRouter 경유 `anthropic/*` 일반 대화 모델에 정적 블록 `cache_control`을 켤지 판별한다.
pub fn is_openrouter_anthropic_cache(req: &CopilotRunRequest) -> bool {
    !req.lightweight
        && req.command.is_none()
        && req.provider.kind == ProviderKind::OpenRouter
        && is_official_openrouter_host(&req.provider.base_url)
        && req
            .provider
            .model
            .trim()
            .to_ascii_lowercase()
            .starts_with("anthropic/")
}

/// OpenAI 계열 공식 엔드포인트에서 `stream_options.include_usage: true`를 안전하게 켤 수 있는지 판별한다.
pub fn supports_openai_stream_usage(req: &CopilotRunRequest) -> bool {
    match req.provider.kind {
        ProviderKind::OpenAi => is_official_openai_host(&req.provider.base_url),
        ProviderKind::DeepSeek => is_official_deepseek_host(&req.provider.base_url),
        ProviderKind::OpenRouter => is_official_openrouter_host(&req.provider.base_url),
        ProviderKind::Xai => is_official_xai_host(&req.provider.base_url),
        _ => false,
    }
}

/// 요청 문맥에서 정규화된 64자 SHA-256 `prompt_cache_key`를 생성한다.
pub fn build_prompt_cache_key(req: &CopilotRunRequest) -> String {
    let fallback = match req.module {
        CopilotModule::Sheet => req
            .context
            .sheet
            .as_ref()
            .map(|s| s.name.as_str())
            .unwrap_or("sheet"),
        CopilotModule::Base => req
            .context
            .base
            .as_ref()
            .map(|b| b.table.as_str())
            .unwrap_or("base"),
        CopilotModule::Pdf => "pdf",
        _ => "doc",
    };
    let raw_seed = req
        .prompt_cache_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback);
    let clipped = &raw_seed[..clamp_utf8_boundary(raw_seed, MAX_CACHE_SEED_BYTES)];
    let canonical = format!(
        "eo-v1|{}|{}|{}|{}|{}",
        req.module.as_str(),
        req.mode.as_str(),
        req.command
            .map_or("none", super::types::CopilotCommand::as_str),
        req.provider.model.trim(),
        clipped
    );
    let digest = Sha256::digest(canonical.as_bytes());
    let mut hex = String::with_capacity(64);
    for b in digest {
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

fn push_turn(
    out: &mut Vec<TurnMessage>,
    role: &'static str,
    block: String,
    merge_same_role: bool,
    cache_breakpoint: bool,
) -> Option<(usize, usize)> {
    if block.trim().is_empty() {
        return None;
    }
    if merge_same_role && out.is_empty() && role != "user" {
        return None;
    }
    let item = TurnBlock {
        text: block,
        cache_breakpoint,
    };
    if merge_same_role {
        if let Some(last) = out.last_mut().filter(|m| m.role == role) {
            last.blocks.push(item);
            let b_idx = last.blocks.len() - 1;
            return Some((out.len() - 1, b_idx));
        }
    }
    out.push(TurnMessage {
        role,
        blocks: vec![item],
    });
    Some((out.len() - 1, 0))
}

pub fn assemble_ordered_turns(
    req: &CopilotRunRequest,
    prior_history: &[HistoryMessage],
    context_block: &str,
    user_prompt: &str,
    loop_steps: &[HistoryMessage],
    merge_same_role: bool,
) -> Vec<TurnMessage> {
    assemble_ordered_turns_with_volatile(
        req,
        prior_history,
        context_block,
        "",
        user_prompt,
        loop_steps,
        merge_same_role,
    )
}

/// 5계층 캐시 순서대로 메시지 목록을 조립한다.
/// 일반 대화: `[Tier 2: 고정 <document_context>] -> [Tier 3: priorHistory] -> [Tier 4: 가변 상태+prompt] -> [Tier 5: loopSteps]`.
/// 단발 요청(`lightweight` 또는 `command`): `priorHistory` 없이 `[<document_context> + 가변 상태 + prompt]`.
pub fn assemble_ordered_turns_with_volatile(
    req: &CopilotRunRequest,
    prior_history: &[HistoryMessage],
    context_block: &str,
    volatile_state: &str,
    user_prompt: &str,
    loop_steps: &[HistoryMessage],
    merge_same_role: bool,
) -> Vec<TurnMessage> {
    let mut out = Vec::with_capacity(prior_history.len() + loop_steps.len() + 3);
    let prompt_text = if user_prompt.is_empty() {
        req.prompt.clone()
    } else {
        user_prompt.to_string()
    };

    if req.lightweight || req.command.is_some() {
        let mut combined = String::new();
        for part in [context_block, volatile_state, prompt_text.as_str()] {
            if !part.is_empty() {
                if !combined.is_empty() {
                    combined.push_str("\n\n");
                }
                combined.push_str(part);
            }
        }
        push_turn(&mut out, "user", combined, merge_same_role, false);
        if req.command.is_some() {
            for step in loop_steps {
                let role = if step.role == "assistant" {
                    "assistant"
                } else {
                    "user"
                };
                push_turn(&mut out, role, step.text.clone(), merge_same_role, false);
            }
        }
        return out;
    }

    if !context_block.is_empty() {
        push_turn(
            &mut out,
            "user",
            context_block.to_string(),
            merge_same_role,
            true,
        );
    }
    let mut last_history_pos: Option<(usize, usize)> = None;
    let mut seen_history_user = false;
    for h in prior_history {
        let role = if h.role == "assistant" {
            "assistant"
        } else {
            "user"
        };
        if !seen_history_user && role != "user" {
            continue;
        }
        seen_history_user = true;
        if let Some(pos) = push_turn(&mut out, role, h.text.clone(), merge_same_role, false) {
            last_history_pos = Some(pos);
        }
    }
    if let Some((t_idx, b_idx)) = last_history_pos {
        out[t_idx].blocks[b_idx].cache_breakpoint = true;
    }

    let tier4 = if volatile_state.is_empty() {
        prompt_text
    } else if prompt_text.is_empty() {
        volatile_state.to_string()
    } else {
        format!("{volatile_state}\n\n{prompt_text}")
    };
    push_turn(&mut out, "user", tier4, merge_same_role, false);

    for step in loop_steps {
        let role = if step.role == "assistant" {
            "assistant"
        } else {
            "user"
        };
        push_turn(&mut out, role, step.text.clone(), merge_same_role, false);
    }

    out
}

/// Anthropic 요청 본문의 `system` 및 `messages` 필드를 직렬화한다.
/// 루트 `cache_control`을 제거하고 재사용 경계(도구 또는 시스템, Tier 2 문서, priorHistory 끝)에만 브레이크포인트를 둔다.
pub fn write_anthropic_system_and_messages(
    out: &mut String,
    system_prompt: &str,
    turns: &[TurnMessage],
    use_block_array: bool,
    allow_cache_breakpoints: bool,
    cache_system_breakpoint: bool,
) {
    if use_block_array {
        out.push_str(",\"system\":[{\"type\":\"text\",\"text\":");
        push_json_str(out, system_prompt);
        if allow_cache_breakpoints && cache_system_breakpoint {
            out.push_str(",\"cache_control\":{\"type\":\"ephemeral\"}");
        }
        out.push_str("}]");
    } else {
        out.push_str(",\"system\":");
        push_json_str(out, system_prompt);
    }

    out.push_str(",\"messages\":[");
    for (idx, turn) in turns.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push_str("{\"role\":");
        push_json_str(out, turn.role);
        out.push_str(",\"content\":");
        if use_block_array {
            out.push('[');
            for (b_idx, block) in turn.blocks.iter().enumerate() {
                if b_idx > 0 {
                    out.push(',');
                }
                out.push_str("{\"type\":\"text\",\"text\":");
                push_json_str(out, &block.text);
                if allow_cache_breakpoints && block.cache_breakpoint {
                    out.push_str(",\"cache_control\":{\"type\":\"ephemeral\"}");
                }
                out.push('}');
            }
            out.push(']');
        } else {
            let joined: Vec<&str> = turn.blocks.iter().map(|b| b.text.as_str()).collect();
            let joined_str = joined.join("\n\n");
            push_json_str(out, &joined_str);
        }
        out.push('}');
    }
    out.push(']');
}

/// Google Gemini `contents` 배열을 직렬화한다. 같은 역할 블록은 `parts` 배열 요소로 보존한다.
pub fn write_google_contents(out: &mut String, turns: &[TurnMessage]) {
    out.push_str(",\"contents\":[");
    for (idx, turn) in turns.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        let g_role = if turn.role == "assistant" {
            "model"
        } else {
            "user"
        };
        out.push_str("{\"role\":");
        push_json_str(out, g_role);
        out.push_str(",\"parts\":[");
        for (b_idx, block) in turn.blocks.iter().enumerate() {
            if b_idx > 0 {
                out.push(',');
            }
            out.push_str("{\"text\":");
            push_json_str(out, &block.text);
            out.push('}');
        }
        out.push_str("]}");
    }
    out.push(']');
}

/// OpenAI 및 호환 엔드포인트의 `messages` 배열을 직렬화한다.
/// OpenRouter `anthropic/` 모델(`openrouter_anth_cache == true`)은 블록 배열과 정적 경계 `cache_control`을 쓴다.
pub fn write_openai_messages_with_cache(
    out: &mut String,
    system_prompt: &str,
    turns: &[TurnMessage],
    openrouter_anth_cache: bool,
    cache_system_breakpoint: bool,
) {
    out.push_str(",\"messages\":[{\"role\":\"system\",\"content\":");
    if openrouter_anth_cache {
        out.push_str("[{\"type\":\"text\",\"text\":");
        push_json_str(out, system_prompt);
        if cache_system_breakpoint {
            out.push_str(",\"cache_control\":{\"type\":\"ephemeral\"}");
        }
        out.push_str("}]");
    } else {
        push_json_str(out, system_prompt);
    }
    out.push('}');
    for turn in turns {
        out.push_str(",{\"role\":");
        push_json_str(out, turn.role);
        out.push_str(",\"content\":");
        if openrouter_anth_cache {
            out.push('[');
            for (b_idx, block) in turn.blocks.iter().enumerate() {
                if b_idx > 0 {
                    out.push(',');
                }
                out.push_str("{\"type\":\"text\",\"text\":");
                push_json_str(out, &block.text);
                if block.cache_breakpoint {
                    out.push_str(",\"cache_control\":{\"type\":\"ephemeral\"}");
                }
                out.push('}');
            }
            out.push(']');
        } else {
            let joined: Vec<&str> = turn.blocks.iter().map(|b| b.text.as_str()).collect();
            let joined_str = joined.join("\n\n");
            push_json_str(out, &joined_str);
        }
        out.push('}');
    }
    out.push(']');
}

pub fn write_openai_messages(out: &mut String, system_prompt: &str, turns: &[TurnMessage]) {
    write_openai_messages_with_cache(out, system_prompt, turns, false, false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_hosts_match_case_insensitively_with_ports() {
        assert!(is_official_openai_host("https://api.openai.com/v1"));
        assert!(is_official_openai_host("https://API.OPENAI.COM:443/v1"));
        assert!(!is_official_openai_host("https://api.groq.com/openai/v1"));
        assert!(is_official_anthropic_host("https://api.anthropic.com"));
        assert!(!is_official_anthropic_host("https://93.184.216.34/v1"));
    }

    #[test]
    fn sha256_cache_key_is_always_64_lowercase_hex() {
        let mut req = CopilotRunRequest::default_for_test(CopilotModule::Doc);
        req.prompt_cache_key = Some("한글 문서 세션 아주 긴 제목 ".repeat(25));
        let key = build_prompt_cache_key(&req);
        assert_eq!(key.len(), 64);
        assert!(key.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
    }
}
