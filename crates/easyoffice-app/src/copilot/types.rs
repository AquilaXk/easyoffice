//! 참조 구현: dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd.) 및 continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., see THIRD_PARTY_NOTICES.md)

use easyoffice_formats::{EasyOfficeError, StrictJsonValue};
use easyoffice_sheet::{CellRange, PackedCellCoordinate};
use std::net::SocketAddr;

use super::cancel::CancelHandle;
use super::context::{parse_context, parse_selection, CopilotContext, SelectionContext};
pub use super::op_types::{
    BoardGraphEdge, BoardGraphNode, CopilotLocation, CopilotOperation, CopilotRunResponse,
    CopilotToolCall, DocBlock, DocReplacement, SheetWriteCell, SlideChartSeries, SlideElementSpec,
};

pub const MAX_PROMPT_BYTES: usize = 16_384;
pub const MAX_HISTORY_ITEMS: usize = 32;
pub const MAX_HISTORY_ITEM_BYTES: usize = 16_384;
/// 요청 문맥으로 받는 시트 셀 수 상한.
pub const MAX_SHEET_CELLS: usize = 10_000;
/// 모델이 한 번에 쓸 수 있는 셀 수 상한. 화면 쪽 적용 상한(`copilot-actions.js` `MAX_WRITE_CELLS`)과 같다.
/// 화면이 넘는 작업을 통째로 거부하고 되돌리므로 서버가 먼저 같은 기준으로 거른다.
pub const MAX_SHEET_WRITE_CELLS: usize = 1_024;
/// 쓰기·서식·차트 범위의 셀 수 상한(`copilot-actions.js` `MAX_RANGE_CELLS`와 같다).
pub const MAX_SHEET_RANGE_CELLS: usize = 4_096;
/// 차트 한 개의 계열 수 상한(`sheet-layout.js` `MAX_CHART_SERIES`). 범주 열(행) 하나를 더한다.
pub const MAX_CHART_SERIES: usize = 32;
pub const MAX_CHART_TITLE_CHARS: usize = 160;
pub const MIN_CHART_WIDTH: u64 = 180;
pub const MAX_CHART_WIDTH: u64 = 1_600;
pub const MIN_CHART_HEIGHT: u64 = 140;
pub const MAX_CHART_HEIGHT: u64 = 1_200;
pub const MAX_DOC_PARAGRAPHS: usize = 2_048;
/// 모델 프롬프트에 싣는 문단 수. 클라이언트가 보내는 문단 수(`MAX_CONTEXT_PARAGRAPHS`)와 같다.
pub const MAX_PROMPT_DOC_PARAGRAPHS: usize = 200;
pub const MAX_PROMPT_SHEET_CELLS: usize = 300;
pub const MAX_PROMPT_SLIDES: usize = 64;
pub const MAX_PROMPT_PDF_CHARS: usize = 12_000;
pub const MAX_PROMPT_CONTEXT_BYTES: usize = 24 * 1024;
pub const MAX_CJK_PROMPT_CONTEXT_BYTES: usize = 36 * 1024;
pub const MAX_BASE_TABLES: usize = 64;
pub const MAX_BASE_FIELDS: usize = 64;
pub const MAX_BASE_RECORDS: usize = 100;
pub const MAX_PROMPT_BASE_RECORDS: usize = 50;
pub const MAX_BASE_TEXT_BYTES: usize = 1_024;
pub const MAX_BOARD_NODES: usize = 256;
pub const MAX_BOARD_CONNECTORS: usize = 1_024;
pub const MAX_BOARD_TEXT_BYTES: usize = 1_024;
pub const MAX_OUTPUT_TOKENS: u32 = 8_192;
pub const LIGHTWEIGHT_MAX_OUTPUT_TOKENS: u32 = 96;
pub const MAX_PROMPT_CACHE_KEY_BYTES: usize = 512;
pub const MAX_SLIDES: usize = 256;
pub const MAX_SLIDE_BULLETS: usize = 64;
pub const MAX_SLIDE_ELEMENTS: usize = 64;
pub const MAX_TEXT_CONTEXT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_API_KEY_BYTES: usize = 4_096;
pub const MAX_BASE_URL_BYTES: usize = 2_048;
pub const MAX_MODEL_NAME_BYTES: usize = 256;
pub const MAX_LLM_REQUEST_BYTES: usize = 512 * 1024;
pub const MAX_LLM_RESPONSE_BYTES: usize = 512 * 1024;
pub const MAX_LLM_OPERATIONS: usize = 64;
pub const MAX_LLM_LOCATIONS: usize = 64;
pub const MAX_LLM_TOOL_CALLS: usize = 16;
pub const MAX_DOC_APPEND_BLOCKS: usize = 256;
pub const MAX_DOC_TABLE_ROWS: usize = 128;
pub const MAX_DOC_TABLE_COLS: usize = 32;
pub const MAX_DISCOVERED_MODELS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
}

impl HttpMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
        }
    }
}

/// Validated HTTP(S) model request handed to the desktop host transport or loopback client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopilotLlmRequest {
    pub method: HttpMethod,
    pub url: String,
    pub host: String,
    pub port: u16,
    pub path: String,
    pub is_https: bool,
    pub resolved_addrs: Vec<SocketAddr>,
    pub headers: Vec<(String, String)>,
    pub body_json: String,
    /// 요청 취소 신호. 전송 계층이 소켓을 등록하고 취소 때 끊는다.
    pub cancel: CancelHandle,
    /// 응답을 스트림(`text/event-stream` 또는 Google JSON 배열 스트림)으로 받는다.
    pub stream: bool,
}

impl CopilotLlmRequest {
    /// 스트림 요청이라도 Google `:streamGenerateContent`는 SSE가 아니라 JSON 배열 스트림이므로 `application/json`을 요청한다.
    pub fn accept_header(&self) -> &'static str {
        if self.stream && !self.path.contains(":streamGenerateContent") {
            "text/event-stream"
        } else {
            "application/json"
        }
    }
}

/// Host-supplied HTTP(S) completion callback (e.g. desktop OS TLS transport).
pub type CopilotLlmClient<'a> = dyn Fn(&CopilotLlmRequest) -> Result<String, String> + 'a;

/// 응답 본문 조각을 받는 콜백. `false`를 돌려주면 전송 계층이 읽기를 멈춘다.
pub type ChunkSink<'a> = dyn FnMut(&[u8]) -> bool + 'a;

/// `stream`이 켜진 요청을 실행하는 호스트 전송. 본문이 오는 대로 `sink`에 넘기고, 끝나면 전체 본문을 돌려준다.
pub type CopilotStreamClient<'a> =
    dyn Fn(&CopilotLlmRequest, &mut ChunkSink<'_>) -> Result<String, String> + 'a;

/// 스트림으로 받은 모델 글 한 조각. 호스트가 화면으로 보낸다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopilotStreamEvent<'a> {
    pub request_id: &'a str,
    /// 요청 안에서 0부터 늘어나는 번호. 화면은 빠진 번호가 있으면 스트림 글을 버리고 최종 응답을 쓴다.
    pub seq: u32,
    pub text: &'a str,
}

pub type CopilotEventSink<'a> = dyn Fn(&CopilotStreamEvent<'_>) + 'a;

/// 코파일럿이 호스트(데스크톱 셸)에 기대는 것: 일반 전송, 스트림 전송, 스트림 조각을 화면으로 보내는 통로.
#[derive(Clone, Copy, Default)]
pub struct CopilotHost<'a> {
    pub llm: Option<&'a CopilotLlmClient<'a>>,
    pub llm_stream: Option<&'a CopilotStreamClient<'a>>,
    pub events: Option<&'a CopilotEventSink<'a>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopilotMode {
    Ask,
    Edit,
}

impl CopilotMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Edit => "edit",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, EasyOfficeError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "ask" => Ok(Self::Ask),
            "edit" => Ok(Self::Edit),
            other => Err(EasyOfficeError::InvalidArgument(format!(
                "지원하지 않는 코파일럿 모드입니다: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopilotModule {
    Sheet,
    Doc,
    Hwp,
    Slide,
    Pdf,
    Base,
    Board,
}

impl CopilotModule {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sheet => "SHEET",
            Self::Doc => "DOC",
            Self::Hwp => "HWP",
            Self::Slide => "SLIDE",
            Self::Pdf => "PDF",
            Self::Base => "BASE",
            Self::Board => "BOARD",
        }
    }

    /// 편집 연산이 없는 모듈(`PDF`만 읽기 전용).
    pub fn is_ask_only(self) -> bool {
        matches!(self, Self::Pdf)
    }

    pub fn parse(raw: &str) -> Result<Self, EasyOfficeError> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "SHEET" => Ok(Self::Sheet),
            "DOC" => Ok(Self::Doc),
            "HWP" => Ok(Self::Hwp),
            "SLIDE" => Ok(Self::Slide),
            "PDF" => Ok(Self::Pdf),
            "BASE" => Ok(Self::Base),
            "BOARD" => Ok(Self::Board),
            other => Err(EasyOfficeError::InvalidArgument(format!(
                "지원하지 않는 코파일럿 문서 모듈입니다: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningEffort {
    Low,
    Medium,
    High,
}

impl ReasoningEffort {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    pub fn parse_optional(raw: &str) -> Result<Option<Self>, EasyOfficeError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" => Ok(None),
            "low" => Ok(Some(Self::Low)),
            "medium" => Ok(Some(Self::Medium)),
            "high" => Ok(Some(Self::High)),
            other => Err(EasyOfficeError::InvalidArgument(format!(
                "지원하지 않는 추론 수준입니다: {other}"
            ))),
        }
    }

    pub fn budget_tokens(self) -> u32 {
        match self {
            Self::Low => 1024,
            Self::Medium => 4096,
            Self::High => 8192,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    OpenAi,
    Anthropic,
    Google,
    DeepSeek,
    OpenRouter,
    Mistral,
    Groq,
    Xai,
    Ollama,
    LmStudio,
    RemoteHttps,
    LocalHttp,
}

impl ProviderKind {
    pub fn parse(raw: &str) -> Result<Self, EasyOfficeError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" => Err(EasyOfficeError::InvalidArgument(
                "모델 설정에서 제공자와 모델을 먼저 선택해야 합니다.".to_string(),
            )),
            "openai" => Ok(Self::OpenAi),
            "anthropic" => Ok(Self::Anthropic),
            "google" | "gemini" => Ok(Self::Google),
            "deepseek" => Ok(Self::DeepSeek),
            "openrouter" => Ok(Self::OpenRouter),
            "mistral" => Ok(Self::Mistral),
            "groq" => Ok(Self::Groq),
            "xai" => Ok(Self::Xai),
            "ollama" => Ok(Self::Ollama),
            "lmstudio" | "lm_studio" => Ok(Self::LmStudio),
            "custom" | "remote_https" => Ok(Self::RemoteHttps),
            "local_http" => Ok(Self::LocalHttp),
            other => Err(EasyOfficeError::InvalidArgument(format!(
                "지원하지 않는 모델 공급자 유형입니다: {other}"
            ))),
        }
    }

    pub fn as_id(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
            Self::Google => "google",
            Self::DeepSeek => "deepseek",
            Self::OpenRouter => "openrouter",
            Self::Mistral => "mistral",
            Self::Groq => "groq",
            Self::Xai => "xai",
            Self::Ollama => "ollama",
            Self::LmStudio => "lmstudio",
            Self::RemoteHttps => "custom",
            Self::LocalHttp => "local_http",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::OpenAi => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::Google => "Google",
            Self::DeepSeek => "DeepSeek",
            Self::OpenRouter => "OpenRouter",
            Self::Mistral => "Mistral",
            Self::Groq => "Groq",
            Self::Xai => "xAI",
            Self::Ollama => "Ollama (로컬)",
            Self::LmStudio => "LM Studio (로컬)",
            Self::RemoteHttps => "OpenAI 호환 API",
            Self::LocalHttp => "로컬 서버 (127.0.0.1)",
        }
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            Self::RemoteHttps | Self::LocalHttp => "",
            Self::OpenAi => "https://api.openai.com/v1",
            Self::Anthropic => "https://api.anthropic.com/v1",
            Self::Google => "https://generativelanguage.googleapis.com/v1beta",
            Self::DeepSeek => "https://api.deepseek.com",
            Self::OpenRouter => "https://openrouter.ai/api/v1",
            Self::Mistral => "https://api.mistral.ai/v1",
            Self::Groq => "https://api.groq.com/openai/v1",
            Self::Xai => "https://api.x.ai/v1",
            Self::Ollama => "http://127.0.0.1:11434/v1",
            Self::LmStudio => "http://127.0.0.1:1234/v1",
        }
    }

    pub fn is_local_loopback(self) -> bool {
        matches!(self, Self::Ollama | Self::LmStudio | Self::LocalHttp)
    }

    pub fn requires_api_key(self) -> bool {
        matches!(
            self,
            Self::OpenAi
                | Self::Anthropic
                | Self::Google
                | Self::DeepSeek
                | Self::OpenRouter
                | Self::Mistral
                | Self::Groq
                | Self::Xai
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub reasoning_effort: Option<ReasoningEffort>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryMessage {
    pub role: String,
    pub text: String,
}

/// 선택 영역 명령. 모델에 보내는 지시문은 `prompt.rs`가 명령마다 따로 쓴다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopilotCommand {
    Rewrite,
    Summarize,
    Shorten,
    Lengthen,
    FixSpelling,
    Translate,
    VisualizeTable,
}

impl CopilotCommand {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rewrite => "rewrite",
            Self::Summarize => "summarize",
            Self::Shorten => "shorten",
            Self::Lengthen => "lengthen",
            Self::FixSpelling => "fix_spelling",
            Self::Translate => "translate",
            Self::VisualizeTable => "visualize_table",
        }
    }

    pub fn parse_optional(raw: &str) -> Result<Option<Self>, EasyOfficeError> {
        match raw.trim() {
            "" => Ok(None),
            "rewrite" => Ok(Some(Self::Rewrite)),
            "summarize" => Ok(Some(Self::Summarize)),
            "shorten" => Ok(Some(Self::Shorten)),
            "lengthen" => Ok(Some(Self::Lengthen)),
            "fix_spelling" => Ok(Some(Self::FixSpelling)),
            "translate" => Ok(Some(Self::Translate)),
            "visualize_table" => Ok(Some(Self::VisualizeTable)),
            other => Err(EasyOfficeError::InvalidArgument(format!(
                "지원하지 않는 선택 영역 명령입니다: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopilotRunRequest {
    pub mode: CopilotMode,
    pub module: CopilotModule,
    pub command: Option<CopilotCommand>,
    pub prompt: String,
    pub history: Vec<HistoryMessage>,
    pub loop_steps: Vec<HistoryMessage>,
    pub prompt_cache_key: Option<String>,
    pub lightweight: bool,
    pub selection: SelectionContext,
    pub context: CopilotContext,
    pub context_delta: Option<super::delta::ContextDelta>,
    pub provider: ProviderConfig,
}

impl CopilotRunRequest {
    /// 질문 모드이거나 편집 연산이 없는 모듈(`PDF`)이면 모델이 돌려준 연산은 전부 버린다.
    pub fn is_read_only(&self) -> bool {
        self.mode == CopilotMode::Ask || self.module.is_ask_only()
    }

    /// 이력 없는 첫 턴 단순 질문·단순 서식 변경인지 판별한다.
    pub fn is_simple_first_turn(&self) -> bool {
        if !self.history.is_empty() || !self.loop_steps.is_empty() || self.command.is_some() {
            return false;
        }
        let prompt = self.prompt.trim();
        if prompt.len() > 160 {
            return false;
        }
        const COMPLEX_MARKERS: [&str; 12] = [
            "분석",
            "비교",
            "추론",
            "검증",
            "설계",
            "계산",
            "수식",
            "함수",
            "차트",
            "피벗",
            "다이어그램",
            "슬라이드 추가",
        ];
        if COMPLEX_MARKERS.iter().any(|m| prompt.contains(m)) {
            return false;
        }
        const COMPLEX_EN_MARKERS: [&str; 11] = [
            "analyze",
            "analyse",
            "compare",
            "calculate",
            "formula",
            "chart",
            "pivot",
            "diagram",
            "design",
            "evaluate",
            "explain why",
        ];
        let prompt_lower = prompt.to_ascii_lowercase();
        if COMPLEX_EN_MARKERS.iter().any(|m| prompt_lower.contains(m)) {
            return false;
        }
        if self.is_read_only() {
            return true;
        }
        const SIMPLE_FORMAT_MARKERS: [&str; 11] = [
            "서식",
            "굵게",
            "글꼴",
            "배경색",
            "색상",
            "정렬",
            "밑줄",
            "기울임",
            "너비",
            "높이",
            "맞춤법",
        ];
        SIMPLE_FORMAT_MARKERS.iter().any(|m| prompt.contains(m))
    }

    /// 요청 유형별 기본 출력 토큰 상한을 차등 산정한다(추론 모델 예산은 호출 측에서 가산).
    pub fn effective_output_base_cap(&self) -> u32 {
        if self.lightweight {
            return LIGHTWEIGHT_MAX_OUTPUT_TOKENS;
        }
        if let Some(cmd) = self.command {
            return if cmd == CopilotCommand::VisualizeTable {
                4_096
            } else {
                2_048
            };
        }
        if self.is_read_only() {
            return if self.module != CopilotModule::Pdf
                && self.history.is_empty()
                && self.loop_steps.is_empty()
                && self.prompt.len() <= 160
            {
                2_048
            } else {
                4_096
            };
        }
        MAX_OUTPUT_TOKENS
    }

    /// 단순 변환 선택 명령(`Summarize` 포함) 및 이력 없는 첫 턴 단순 질문/서식은 추론 강도를 `low`로 제한한다.
    pub fn effective_reasoning_effort(&self) -> Option<ReasoningEffort> {
        if self.lightweight {
            return None;
        }
        let effort = self.provider.reasoning_effort?;
        if matches!(
            self.command,
            Some(
                CopilotCommand::FixSpelling
                    | CopilotCommand::Shorten
                    | CopilotCommand::Lengthen
                    | CopilotCommand::Rewrite
                    | CopilotCommand::Translate
                    | CopilotCommand::Summarize
            )
        ) || self.is_simple_first_turn()
        {
            return Some(ReasoningEffort::Low);
        }
        Some(effort)
    }

    #[cfg(test)]
    pub fn default_for_test(module: CopilotModule) -> Self {
        Self {
            mode: CopilotMode::Ask,
            module,
            command: None,
            prompt: "확인".to_string(),
            history: Vec::new(),
            loop_steps: Vec::new(),
            prompt_cache_key: None,
            lightweight: false,
            selection: SelectionContext::default(),
            context: CopilotContext::default(),
            context_delta: None,
            provider: ProviderConfig {
                kind: ProviderKind::OpenAi,
                base_url: "https://api.openai.com/v1".to_string(),
                model: "gpt-4o".to_string(),
                api_key: "sk-test".to_string(),
                reasoning_effort: None,
            },
        }
    }
}

/// 화면 쪽 `CHART_TYPES`(`sheet-layout.js`)와 같은 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartType {
    Column,
    Bar,
    Line,
    Pie,
    Scatter,
}

impl ChartType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Column => "column",
            Self::Bar => "bar",
            Self::Line => "line",
            Self::Pie => "pie",
            Self::Scatter => "scatter",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "column" => Some(Self::Column),
            "bar" => Some(Self::Bar),
            "line" => Some(Self::Line),
            "pie" => Some(Self::Pie),
            "scatter" => Some(Self::Scatter),
            _ => None,
        }
    }
}

/// 화면 쪽 `CHART_LEGENDS`와 같은 범례 위치.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartLegend {
    Right,
    Bottom,
    Top,
    Left,
    None,
}

impl ChartLegend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Right => "right",
            Self::Bottom => "bottom",
            Self::Top => "top",
            Self::Left => "left",
            Self::None => "none",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "right" => Some(Self::Right),
            "bottom" => Some(Self::Bottom),
            "top" => Some(Self::Top),
            "left" => Some(Self::Left),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

pub use easyoffice_formats::json_out::push_json_string as push_json_str;

pub fn is_valid_hex_color(raw: &str) -> bool {
    let b = raw.as_bytes();
    b.len() == 7 && b[0] == b'#' && b[1..].iter().all(u8::is_ascii_hexdigit)
}

pub fn is_valid_a1_coord(raw: &str) -> bool {
    let t = raw.trim();
    !t.is_empty()
        && t.len() <= 16
        && t.bytes().all(|b| b.is_ascii_alphanumeric())
        && PackedCellCoordinate::parse_a1(t).is_ok()
}

/// 셀 범위 문자열을 해석한다. 문자 집합·길이·셀 수(`MAX_SHEET_RANGE_CELLS`)를 넘으면 `None`.
pub fn parse_bounded_range(raw: &str) -> Option<CellRange> {
    let t = raw.trim();
    if t.is_empty() || t.len() > 40 || !t.bytes().all(|b| b.is_ascii_alphanumeric() || b == b':') {
        return None;
    }
    CellRange::parse(t)
        .ok()
        .filter(|r| r.cell_count() <= MAX_SHEET_RANGE_CELLS)
}

pub fn is_valid_range_ref(raw: &str) -> bool {
    parse_bounded_range(raw).is_some()
}

pub(super) fn opt_str<'a>(
    entries: &'a [(String, StrictJsonValue)],
    key: &str,
) -> Result<Option<&'a str>, EasyOfficeError> {
    match StrictJsonValue::get_field(entries, key) {
        None | Some(StrictJsonValue::Null) => Ok(None),
        Some(StrictJsonValue::Str(s)) => Ok(Some(s.as_str())),
        Some(_) => Err(EasyOfficeError::InvalidArgument(format!(
            "JSON 필드 '{key}'는 문자열이어야 합니다."
        ))),
    }
}

pub(super) fn opt_scalar_string(
    entries: &[(String, StrictJsonValue)],
    key: &str,
) -> Result<String, EasyOfficeError> {
    match StrictJsonValue::get_field(entries, key) {
        None | Some(StrictJsonValue::Null) => Ok(String::new()),
        Some(v) => v.to_scalar_string().ok_or_else(|| {
            EasyOfficeError::InvalidArgument(format!("JSON 필드 '{key}'는 스칼라 값이어야 합니다."))
        }),
    }
}

pub fn parse_provider_config(
    entries: &[(String, StrictJsonValue)],
) -> Result<ProviderConfig, EasyOfficeError> {
    let Some(val) = StrictJsonValue::get_field(entries, "provider") else {
        return Err(EasyOfficeError::InvalidArgument(
            "모델 설정에서 제공자와 모델을 먼저 선택해야 합니다.".to_string(),
        ));
    };
    if matches!(val, StrictJsonValue::Null) {
        return Err(EasyOfficeError::InvalidArgument(
            "모델 설정에서 제공자와 모델을 먼저 선택해야 합니다.".to_string(),
        ));
    }
    let obj = val.as_object().ok_or_else(|| {
        EasyOfficeError::InvalidArgument("'provider' 필드는 객체여야 합니다.".to_string())
    })?;
    let raw_api_key = opt_str(obj, "apiKey")?.unwrap_or("");
    if raw_api_key.len() > MAX_API_KEY_BYTES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "apiKey 길이가 상한({MAX_API_KEY_BYTES} bytes)을 초과했습니다."
        )));
    }
    let kind = ProviderKind::parse(opt_str(obj, "kind")?.unwrap_or(""))?;
    let raw_base = opt_str(obj, "baseUrl")?.unwrap_or("").trim();
    if raw_base.len() > MAX_BASE_URL_BYTES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "baseUrl 길이가 상한({MAX_BASE_URL_BYTES} bytes)을 초과했습니다."
        )));
    }
    let base_url = if raw_base.is_empty() {
        kind.default_base_url().to_string()
    } else {
        raw_base.to_string()
    };
    let raw_model = opt_str(obj, "model")?.unwrap_or("").trim();
    if raw_model.len() > MAX_MODEL_NAME_BYTES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "model 길이가 상한({MAX_MODEL_NAME_BYTES} bytes)을 초과했습니다."
        )));
    }
    let model = raw_model.to_string();
    let api_key = raw_api_key.to_string();
    let reasoning_effort =
        ReasoningEffort::parse_optional(opt_str(obj, "reasoningEffort")?.unwrap_or(""))?;
    Ok(ProviderConfig {
        kind,
        base_url,
        model,
        api_key,
        reasoning_effort,
    })
}

pub fn parse_run_request(
    entries: &[(String, StrictJsonValue)],
) -> Result<CopilotRunRequest, EasyOfficeError> {
    let mode = CopilotMode::parse(opt_str(entries, "mode")?.unwrap_or("ask"))?;
    let module = CopilotModule::parse(opt_str(entries, "module")?.unwrap_or("DOC"))?;
    let command = CopilotCommand::parse_optional(opt_str(entries, "command")?.unwrap_or(""))?;
    let prompt_raw = opt_str(entries, "prompt")?.unwrap_or("");
    if prompt_raw.len() > MAX_PROMPT_BYTES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "프롬프트 크기({} bytes)가 상한({MAX_PROMPT_BYTES} bytes)을 초과했습니다.",
            prompt_raw.len()
        )));
    }
    let prompt = prompt_raw.trim().to_string();
    if prompt.is_empty() {
        return Err(EasyOfficeError::InvalidArgument(
            "프롬프트를 입력하세요.".to_string(),
        ));
    }

    let history = parse_history_field(entries, "history")?;
    let loop_steps = parse_history_field(entries, "loopSteps")?;
    if history.len().saturating_add(loop_steps.len()) > MAX_HISTORY_ITEMS * 2 {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "대화 및 루프 단계 총 개수가 상한을 초과했습니다.".to_string(),
        ));
    }
    let prompt_cache_key = match opt_str(entries, "promptCacheKey")? {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.len() > MAX_PROMPT_CACHE_KEY_BYTES {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "promptCacheKey 크기가 상한({MAX_PROMPT_CACHE_KEY_BYTES} bytes)을 초과했습니다."
                )));
            }
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
        None => None,
    };
    let lightweight = StrictJsonValue::get_field(entries, "lightweight")
        .and_then(StrictJsonValue::as_bool)
        .unwrap_or(false);
    let selection = parse_selection(entries)?;
    let context = parse_context(entries)?;
    let context_delta = super::delta::parse_context_delta(entries)?;
    let provider = parse_provider_config(entries)?;

    if let Some(cmd) = command {
        validate_command_target(mode, module, cmd, &selection, &context)?;
    }

    Ok(CopilotRunRequest {
        mode,
        module,
        command,
        prompt,
        history,
        loop_steps,
        prompt_cache_key,
        lightweight,
        selection,
        context,
        context_delta,
        provider,
    })
}

fn parse_history_field(
    entries: &[(String, StrictJsonValue)],
    field_name: &str,
) -> Result<Vec<HistoryMessage>, EasyOfficeError> {
    let Some(val) = StrictJsonValue::get_field(entries, field_name) else {
        return Ok(Vec::new());
    };
    if matches!(val, StrictJsonValue::Null) {
        return Ok(Vec::new());
    }
    let items = val.as_array().ok_or_else(|| {
        EasyOfficeError::InvalidArgument(format!("'{field_name}' 필드는 배열이어야 합니다."))
    })?;
    if items.len() > MAX_HISTORY_ITEMS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "{field_name} 항목 수({})가 상한({MAX_HISTORY_ITEMS}개)을 초과했습니다.",
            items.len()
        )));
    }
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let obj = item.as_object().ok_or_else(|| {
            EasyOfficeError::InvalidArgument(format!("{field_name} 항목은 객체여야 합니다."))
        })?;
        let role = match opt_str(obj, "role")?.unwrap_or("user") {
            "assistant" => "assistant".to_string(),
            _ => "user".to_string(),
        };
        let text_raw = match opt_str(obj, "text")? {
            Some(s) => s,
            None => opt_str(obj, "content")?.unwrap_or(""),
        };
        let text = text_raw.trim().to_string();
        if text.len() > MAX_HISTORY_ITEM_BYTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "{field_name} 항목 크기가 상한({MAX_HISTORY_ITEM_BYTES} bytes)을 초과했습니다."
            )));
        }
        if !text.is_empty() {
            out.push(HistoryMessage { role, text });
        }
    }
    Ok(out)
}

/// 선택 영역 명령은 편집 모드의 문서·한글·슬라이드 텍스트에서만 받고, 대상 위치가 있어야 한다.
fn validate_command_target(
    mode: CopilotMode,
    module: CopilotModule,
    command: CopilotCommand,
    selection: &SelectionContext,
    context: &CopilotContext,
) -> Result<(), EasyOfficeError> {
    let invalid = |msg: &str| Err(EasyOfficeError::InvalidArgument(msg.to_string()));
    if mode != CopilotMode::Edit {
        return invalid("선택 영역 명령은 편집 모드에서만 쓸 수 있습니다.");
    }
    match module {
        CopilotModule::Doc | CopilotModule::Hwp => {
            if module == CopilotModule::Hwp && command == CopilotCommand::VisualizeTable {
                return invalid("표로 시각화 명령은 문서 편집기에서만 쓸 수 있습니다.");
            }
            let (Some(start), end) = (selection.paragraph_index, selection.paragraph_end) else {
                return invalid("선택 영역 명령에는 선택한 문단 위치가 필요합니다.");
            };
            let last = end.unwrap_or(start);
            let valid = context.doc.as_ref().is_some_and(|doc| {
                last >= start
                    && last < doc.paragraphs.len()
                    && (start..=last).all(|i| doc.paragraphs[i].is_some())
            });
            if !valid {
                return invalid("선택한 문단 범위가 올바르지 않습니다.");
            }
            Ok(())
        }
        CopilotModule::Slide => {
            if command == CopilotCommand::VisualizeTable {
                return invalid("표로 시각화 명령은 문서 편집기에서만 쓸 수 있습니다.");
            }
            let known = context.slide.as_ref().map_or(0, |slide| slide.slides.len());
            match (selection.slide_index, selection.slide_field) {
                (Some(index), Some(_)) if index < known => Ok(()),
                _ => invalid("선택 영역 명령에는 선택한 슬라이드 칸이 필요합니다."),
            }
        }
        _ => invalid("선택 영역 명령은 문서와 슬라이드 텍스트에서만 쓸 수 있습니다."),
    }
}
