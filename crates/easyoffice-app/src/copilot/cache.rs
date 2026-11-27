//! 4계층 프롬프트 캐시 메시지 조립, OpenAI prompt_cache_key 정규화 및 Anthropic 캐시 브레이크포인트 직렬화.
// 참조 구현: tribgames/mixdog (Apache-2.0, d5ab088efcc2680f0bb4c43448255ca1653fb3c6),
// anomalyco/opencode (MIT) — 정적 도구·시스템 고정, 64자 SHA-256 캐시 키, 계층형 캐시 배치.

use sha2::{Digest, Sha256};
use std::fmt::Write as _;

use super::types::{push_json_str, CopilotModule, CopilotRunRequest, HistoryMessage};

const MAX_CACHE_SEED_BYTES: usize = 512;

/// 4계층 대화 배치에서 한 턴의 역할과 연속 텍스트 블록 목록이다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnMessage {
    pub role: &'static str,
    pub blocks: Vec<String>,
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
        "eo-v1|{}|{}|{}",
        req.module.as_str(),
        req.mode.as_str(),
        clipped
    );
    let digest = Sha256::digest(canonical.as_bytes());
    let mut hex = String::with_capacity(64);
    for b in digest {
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

fn push_turn(out: &mut Vec<TurnMessage>, role: &'static str, block: String, merge_same_role: bool) {
    if block.trim().is_empty() {
        return;
    }
    if merge_same_role && out.is_empty() && role != "user" {
        return;
    }
    if merge_same_role {
        if let Some(last) = out.last_mut().filter(|m| m.role == role) {
            last.blocks.push(block);
            return;
        }
    }
    out.push(TurnMessage {
        role,
        blocks: vec![block],
    });
}

/// 4계층 캐시 규칙(`[1. tools+system] -> [2. priorHistory] -> [3. <document_context> + prompt] -> [4. loopSteps]`)에 맞춰
/// 메시지 목록을 조립한다. 불변 읽기 전용 `PDF`는 후속 질문에서도 `<document_context>`를 `priorHistory` 앞에 고정한다.
pub fn assemble_ordered_turns(
    req: &CopilotRunRequest,
    prior_history: &[HistoryMessage],
    context_block: &str,
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

    if req.lightweight {
        let body = if context_block.is_empty() {
            prompt_text
        } else {
            format!("{context_block}\n\n{prompt_text}")
        };
        push_turn(&mut out, "user", body, merge_same_role);
        return out;
    }

    let pdf_immutable_prefix =
        req.is_read_only() && req.module == CopilotModule::Pdf && !context_block.is_empty();

    if pdf_immutable_prefix {
        // 불변 PDF는 턴 1과 후속 질문 모두 [system, user(<document_context>), priorHistory..., user(prompt)] 접두를 공유한다.
        push_turn(&mut out, "user", context_block.to_string(), merge_same_role);
        for h in prior_history {
            let role = if h.role == "assistant" {
                "assistant"
            } else {
                "user"
            };
            push_turn(&mut out, role, h.text.clone(), merge_same_role);
        }
        push_turn(&mut out, "user", prompt_text, merge_same_role);
    } else {
        for h in prior_history {
            let role = if h.role == "assistant" {
                "assistant"
            } else {
                "user"
            };
            push_turn(&mut out, role, h.text.clone(), merge_same_role);
        }
        let tier3 = if context_block.is_empty() {
            prompt_text
        } else {
            format!("{context_block}\n\n{prompt_text}")
        };
        push_turn(&mut out, "user", tier3, merge_same_role);
    }

    for step in loop_steps {
        let role = if step.role == "assistant" {
            "assistant"
        } else {
            "user"
        };
        push_turn(&mut out, role, step.text.clone(), merge_same_role);
    }

    out
}

/// Anthropic 요청 본문의 `system` 및 `messages` 필드를 직렬화한다.
/// 공식 호스트(`api.anthropic.com`)에는 루트 `cache_control`, 시스템 블록 브레이크포인트, 일관된 텍스트 블록 배열을 적용한다.
pub fn write_anthropic_system_and_messages(
    out: &mut String,
    system_prompt: &str,
    turns: &[TurnMessage],
    official_host: bool,
) {
    if official_host {
        out.push_str(
            ",\"cache_control\":{\"type\":\"ephemeral\"},\"system\":[{\"type\":\"text\",\"text\":",
        );
        push_json_str(out, system_prompt);
        out.push_str(",\"cache_control\":{\"type\":\"ephemeral\"}}]");
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
        if official_host {
            out.push('[');
            for (b_idx, block) in turn.blocks.iter().enumerate() {
                if b_idx > 0 {
                    out.push(',');
                }
                out.push_str("{\"type\":\"text\",\"text\":");
                push_json_str(out, block);
                out.push('}');
            }
            out.push(']');
        } else {
            let joined = turn.blocks.join("\n\n");
            push_json_str(out, &joined);
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
            push_json_str(out, block);
            out.push('}');
        }
        out.push_str("]}");
    }
    out.push(']');
}

/// OpenAI 및 호환 엔드포인트의 `messages` 배열을 직렬화한다.
pub fn write_openai_messages(out: &mut String, system_prompt: &str, turns: &[TurnMessage]) {
    out.push_str(",\"messages\":[{\"role\":\"system\",\"content\":");
    push_json_str(out, system_prompt);
    out.push('}');
    for turn in turns {
        let joined = turn.blocks.join("\n\n");
        out.push_str(",{\"role\":");
        push_json_str(out, turn.role);
        out.push_str(",\"content\":");
        push_json_str(out, &joined);
        out.push('}');
    }
    out.push(']');
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
