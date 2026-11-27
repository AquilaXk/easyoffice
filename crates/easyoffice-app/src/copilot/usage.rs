//! 멀티 프로바이더(OpenAI·Anthropic·Google) 응답의 토큰 usage 계측 파서 및 정규화 직렬화.
// 참조 구현: tribgames/mixdog (Apache-2.0, d5ab088efcc2680f0bb4c43448255ca1653fb3c6),
// anomalyco/opencode (MIT) — 스트리밍·비스트리밍 응답의 캐시·추론 토큰 정규화 패턴.

use easyoffice_formats::StrictJsonValue;
use std::fmt::Write as _;

use super::types::ProviderKind;

/// 한 턴의 LLM 호출이 보고한 입력·출력·캐시·추론 토큰 사용량이다.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    pub reasoning_tokens: u64,
}

impl TokenUsage {
    pub const fn is_empty(self) -> bool {
        self.input_tokens == 0
            && self.output_tokens == 0
            && self.cached_tokens == 0
            && self.reasoning_tokens == 0
    }

    /// 스트리밍 청크(`message_start` -> `message_delta`, `usage`, `usageMetadata`)의 누적 최댓값을 합친다.
    pub fn merge_latest(&mut self, next: Self) {
        self.input_tokens = self.input_tokens.max(next.input_tokens);
        self.output_tokens = self.output_tokens.max(next.output_tokens);
        self.cached_tokens = self.cached_tokens.max(next.cached_tokens);
        self.reasoning_tokens = self.reasoning_tokens.max(next.reasoning_tokens);
    }
}

fn u64_field(obj: &[(String, StrictJsonValue)], key: &str) -> Option<u64> {
    StrictJsonValue::get_field(obj, key).and_then(StrictJsonValue::as_u64)
}

fn obj_field<'a>(
    obj: &'a [(String, StrictJsonValue)],
    key: &str,
) -> Option<&'a [(String, StrictJsonValue)]> {
    StrictJsonValue::get_field(obj, key).and_then(StrictJsonValue::as_object)
}

fn parse_openai_usage_obj(u: &[(String, StrictJsonValue)]) -> TokenUsage {
    let input_tokens = u64_field(u, "prompt_tokens")
        .or_else(|| u64_field(u, "input_tokens"))
        .unwrap_or(0);
    let output_tokens = u64_field(u, "completion_tokens")
        .or_else(|| u64_field(u, "output_tokens"))
        .unwrap_or(0);
    let cached_tokens = obj_field(u, "prompt_tokens_details")
        .or_else(|| obj_field(u, "input_tokens_details"))
        .and_then(|d| u64_field(d, "cached_tokens"))
        .or_else(|| u64_field(u, "cached_tokens"))
        .or_else(|| u64_field(u, "prompt_cache_hit_tokens"))
        .unwrap_or(0);
    let reasoning_tokens = obj_field(u, "completion_tokens_details")
        .or_else(|| obj_field(u, "output_tokens_details"))
        .and_then(|d| u64_field(d, "reasoning_tokens"))
        .or_else(|| u64_field(u, "reasoning_tokens"))
        .unwrap_or(0);
    TokenUsage {
        input_tokens,
        output_tokens,
        cached_tokens,
        reasoning_tokens,
    }
}

fn parse_anthropic_usage_obj(u: &[(String, StrictJsonValue)]) -> TokenUsage {
    let base_in = u64_field(u, "input_tokens").unwrap_or(0);
    let cache_create = u64_field(u, "cache_creation_input_tokens").unwrap_or(0);
    let cache_read = u64_field(u, "cache_read_input_tokens").unwrap_or(0);
    let input_tokens = base_in
        .saturating_add(cache_create)
        .saturating_add(cache_read);
    let output_tokens = u64_field(u, "output_tokens").unwrap_or(0);
    let reasoning_tokens = u64_field(u, "reasoning_tokens").unwrap_or(0);
    TokenUsage {
        input_tokens,
        output_tokens,
        cached_tokens: cache_read,
        reasoning_tokens,
    }
}

fn parse_google_usage_obj(u: &[(String, StrictJsonValue)]) -> TokenUsage {
    TokenUsage {
        input_tokens: u64_field(u, "promptTokenCount").unwrap_or(0),
        output_tokens: u64_field(u, "candidatesTokenCount").unwrap_or(0),
        cached_tokens: u64_field(u, "cachedContentTokenCount").unwrap_or(0),
        reasoning_tokens: u64_field(u, "thoughtsTokenCount").unwrap_or(0),
    }
}

/// JSON 루트 객체에서 제공자 형식별 토큰 사용량을 파싱한다.
pub fn extract_usage_from_root(
    kind: ProviderKind,
    root: &[(String, StrictJsonValue)],
) -> Option<TokenUsage> {
    match kind {
        ProviderKind::Anthropic => obj_field(root, "usage")
            .or_else(|| obj_field(root, "message").and_then(|m| obj_field(m, "usage")))
            .map(parse_anthropic_usage_obj),
        ProviderKind::Google => obj_field(root, "usageMetadata").map(parse_google_usage_obj),
        _ => obj_field(root, "usage").map(parse_openai_usage_obj),
    }
}

/// `CopilotRunResponse` JSON 객체 말미에 `"usage":{...}` 필드를 덧붙인다.
pub fn write_usage_json_field(out: &mut String, usage: TokenUsage) {
    let _ = write!(
        out,
        ",\"usage\":{{\"inputTokens\":{},\"outputTokens\":{},\"cachedTokens\":{},\"reasoningTokens\":{}}}",
        usage.input_tokens, usage.output_tokens, usage.cached_tokens, usage.reasoning_tokens
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_root(json: &str) -> Vec<(String, StrictJsonValue)> {
        match StrictJsonValue::parse(json).expect("valid json") {
            StrictJsonValue::Object(obj) => obj,
            _ => panic!("expected object"),
        }
    }

    #[test]
    fn extracts_openai_and_google_and_anthropic_usage() {
        let oa = parse_root(
            r#"{"usage":{"prompt_tokens":1200,"completion_tokens":85,"prompt_tokens_details":{"cached_tokens":960},"completion_tokens_details":{"reasoning_tokens":32}}}"#,
        );
        assert_eq!(
            extract_usage_from_root(ProviderKind::OpenAi, &oa),
            Some(TokenUsage {
                input_tokens: 1200,
                output_tokens: 85,
                cached_tokens: 960,
                reasoning_tokens: 32,
            })
        );

        let null_oa = parse_root(r#"{"choices":[],"usage":null}"#);
        assert_eq!(
            extract_usage_from_root(ProviderKind::OpenAi, &null_oa),
            None
        );

        let ant_start = parse_root(
            r#"{"type":"message_start","message":{"usage":{"input_tokens":12,"cache_creation_input_tokens":200,"cache_read_input_tokens":1500,"output_tokens":1}}}"#,
        );
        let ant_delta = parse_root(r#"{"type":"message_delta","usage":{"output_tokens":64}}"#);
        let mut ant = TokenUsage::default();
        if let Some(u) = extract_usage_from_root(ProviderKind::Anthropic, &ant_start) {
            ant.merge_latest(u);
        }
        if let Some(u) = extract_usage_from_root(ProviderKind::Anthropic, &ant_delta) {
            ant.merge_latest(u);
        }
        assert_eq!(
            ant,
            TokenUsage {
                input_tokens: 1712,
                output_tokens: 64,
                cached_tokens: 1500,
                reasoning_tokens: 0,
            }
        );

        let gg = parse_root(
            r#"{"usageMetadata":{"promptTokenCount":900,"candidatesTokenCount":45,"cachedContentTokenCount":600,"thoughtsTokenCount":120}}"#,
        );
        assert_eq!(
            extract_usage_from_root(ProviderKind::Google, &gg),
            Some(TokenUsage {
                input_tokens: 900,
                output_tokens: 45,
                cached_tokens: 600,
                reasoning_tokens: 120,
            })
        );
    }
}
