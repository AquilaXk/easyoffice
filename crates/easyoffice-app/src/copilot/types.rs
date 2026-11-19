//! 참조 구현: dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd.) 및 continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., see THIRD_PARTY_NOTICES.md)

use easyoffice_formats::{EasyOfficeError, StrictJsonValue};
use easyoffice_sheet::{CellRange, PackedCellCoordinate};
use std::net::SocketAddr;

pub const MAX_PROMPT_BYTES: usize = 16_384;
pub const MAX_HISTORY_ITEMS: usize = 32;
pub const MAX_HISTORY_ITEM_BYTES: usize = 16_384;
pub const MAX_SHEET_CELLS: usize = 10_000;
pub const MAX_DOC_PARAGRAPHS: usize = 2_048;
pub const MAX_SLIDES: usize = 256;
pub const MAX_SLIDE_BULLETS: usize = 64;
pub const MAX_TEXT_CONTEXT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_API_KEY_BYTES: usize = 4_096;
pub const MAX_BASE_URL_BYTES: usize = 2_048;
pub const MAX_MODEL_NAME_BYTES: usize = 256;
pub const MAX_LLM_REQUEST_BYTES: usize = 512 * 1024;
pub const MAX_LLM_RESPONSE_BYTES: usize = 512 * 1024;
pub const MAX_LLM_OPERATIONS: usize = 64;
pub const MAX_LLM_LOCATIONS: usize = 64;
pub const MAX_DOC_APPEND_BLOCKS: usize = 256;
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
}

/// Host-supplied HTTP(S) completion callback (e.g. desktop OS TLS transport).
pub type CopilotLlmClient<'a> = dyn Fn(&CopilotLlmRequest) -> Result<String, String> + 'a;

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
    Slide,
    Pdf,
}

impl CopilotModule {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sheet => "SHEET",
            Self::Doc => "DOC",
            Self::Slide => "SLIDE",
            Self::Pdf => "PDF",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, EasyOfficeError> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "SHEET" => Ok(Self::Sheet),
            "DOC" | "HWP" => Ok(Self::Doc),
            "SLIDE" => Ok(Self::Slide),
            "PDF" => Ok(Self::Pdf),
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectionContext {
    pub sheet: Option<String>,
    pub range: Option<String>,
    pub paragraph_index: Option<usize>,
    pub slide_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetCellInput {
    pub coord: String,
    pub raw: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetContext {
    pub name: String,
    pub cells: Vec<SheetCellInput>,
}

impl Default for SheetContext {
    fn default() -> Self {
        Self {
            name: "Sheet1".to_string(),
            cells: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocContext {
    pub paragraphs: Vec<String>,
    pub selected_text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlideItem {
    pub title: String,
    pub bullets: Vec<String>,
    pub notes: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlideContext {
    pub active_index: usize,
    pub slides: Vec<SlideItem>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PdfContext {
    pub text: String,
    pub page_count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CopilotContext {
    pub sheet: Option<SheetContext>,
    pub doc: Option<DocContext>,
    pub slide: Option<SlideContext>,
    pub pdf: Option<PdfContext>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopilotRunRequest {
    pub mode: CopilotMode,
    pub module: CopilotModule,
    pub prompt: String,
    pub history: Vec<HistoryMessage>,
    pub selection: SelectionContext,
    pub context: CopilotContext,
    pub provider: ProviderConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetWriteCell {
    pub coord: String,
    pub raw: String,
    pub num_fmt: Option<String>,
    pub bold: Option<bool>,
    pub fill: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocBlock {
    Heading { level: u8, text: String },
    Paragraph { text: String },
    List { kind: String, items: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocReplacement {
    pub index: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopilotOperation {
    SheetWrite {
        sheet: String,
        range: String,
        cells: Vec<SheetWriteCell>,
    },
    SheetFormat {
        sheet: String,
        range: String,
        fill: Option<String>,
        color: Option<String>,
        bold: Option<bool>,
        num_fmt: Option<String>,
    },
    DocAppend {
        blocks: Vec<DocBlock>,
    },
    DocReplaceParagraphs {
        replacements: Vec<DocReplacement>,
    },
    DocScrubPii,
    SlideAdd {
        title: String,
        bullets: Vec<String>,
        notes: String,
    },
    SlideUpdate {
        index: usize,
        title: Option<String>,
        bullets: Option<Vec<String>>,
        notes: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopilotLocation {
    pub module: String,
    pub label: String,
    pub sheet: Option<String>,
    pub range: Option<String>,
    pub paragraph_index: Option<usize>,
    pub slide_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopilotRunResponse {
    pub mode: CopilotMode,
    pub reply: String,
    pub reasoning: String,
    pub updated_in_place: bool,
    pub operations: Vec<CopilotOperation>,
    pub locations: Vec<CopilotLocation>,
}

pub fn escape_json_str_into(out: &mut String, s: &str) {
    crate::EasyOfficeIpcRouter::escape_json_into(out, s);
}

pub fn push_json_str(out: &mut String, s: &str) {
    out.push('"');
    escape_json_str_into(out, s);
    out.push('"');
}

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

pub fn is_valid_range_ref(raw: &str) -> bool {
    let t = raw.trim();
    if t.is_empty() || t.len() > 40 || !t.bytes().all(|b| b.is_ascii_alphanumeric() || b == b':') {
        return false;
    }
    CellRange::parse(t).is_ok_and(|r| r.cell_count() <= MAX_SHEET_CELLS)
}

fn opt_str<'a>(
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

fn opt_scalar_string(
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

    let history = parse_history(entries)?;
    let selection = parse_selection(entries)?;
    let context = parse_context(entries)?;
    let provider = parse_provider_config(entries)?;

    Ok(CopilotRunRequest {
        mode,
        module,
        prompt,
        history,
        selection,
        context,
        provider,
    })
}

fn parse_history(
    entries: &[(String, StrictJsonValue)],
) -> Result<Vec<HistoryMessage>, EasyOfficeError> {
    let Some(val) = StrictJsonValue::get_field(entries, "history") else {
        return Ok(Vec::new());
    };
    if matches!(val, StrictJsonValue::Null) {
        return Ok(Vec::new());
    }
    let items = val.as_array().ok_or_else(|| {
        EasyOfficeError::InvalidArgument("'history' 필드는 배열이어야 합니다.".to_string())
    })?;
    if items.len() > MAX_HISTORY_ITEMS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "대화 기록 항목 수({})가 상한({MAX_HISTORY_ITEMS}개)을 초과했습니다.",
            items.len()
        )));
    }
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let obj = item.as_object().ok_or_else(|| {
            EasyOfficeError::InvalidArgument("history 항목은 객체여야 합니다.".to_string())
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
                "history 항목 크기가 상한({MAX_HISTORY_ITEM_BYTES} bytes)을 초과했습니다."
            )));
        }
        if !text.is_empty() {
            out.push(HistoryMessage { role, text });
        }
    }
    Ok(out)
}

fn parse_selection(
    entries: &[(String, StrictJsonValue)],
) -> Result<SelectionContext, EasyOfficeError> {
    let Some(val) = StrictJsonValue::get_field(entries, "selection") else {
        return Ok(SelectionContext::default());
    };
    if matches!(val, StrictJsonValue::Null) {
        return Ok(SelectionContext::default());
    }
    if let Some(raw_str) = val.as_str() {
        let trimmed = raw_str.trim();
        if trimmed.len() > 100 {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "선택 범위 문자열이 너무 깁니다.".to_string(),
            ));
        }
        return Ok(SelectionContext {
            range: (!trimmed.is_empty()).then(|| trimmed.to_string()),
            ..SelectionContext::default()
        });
    }
    let obj = val.as_object().ok_or_else(|| {
        EasyOfficeError::InvalidArgument("'selection' 필드는 객체여야 합니다.".to_string())
    })?;
    let sheet = opt_str(obj, "sheet")?
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);
    let range = opt_str(obj, "range")?
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);
    if range.as_ref().is_some_and(|r| r.len() > 100) {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "선택 범위 문자열이 너무 깁니다.".to_string(),
        ));
    }
    let paragraph_index = StrictJsonValue::get_field(obj, "paragraphIndex")
        .and_then(StrictJsonValue::as_u64)
        .and_then(|v| usize::try_from(v).ok());
    let slide_index = StrictJsonValue::get_field(obj, "slideIndex")
        .and_then(StrictJsonValue::as_u64)
        .and_then(|v| usize::try_from(v).ok());
    Ok(SelectionContext {
        sheet,
        range,
        paragraph_index,
        slide_index,
    })
}

fn parse_context(entries: &[(String, StrictJsonValue)]) -> Result<CopilotContext, EasyOfficeError> {
    let Some(val) = StrictJsonValue::get_field(entries, "context") else {
        return Ok(CopilotContext::default());
    };
    if matches!(val, StrictJsonValue::Null) {
        return Ok(CopilotContext::default());
    }
    let obj = val.as_object().ok_or_else(|| {
        EasyOfficeError::InvalidArgument("'context' 필드는 객체여야 합니다.".to_string())
    })?;

    let sheet = if let Some(s_val) = StrictJsonValue::get_field(obj, "sheet") {
        if matches!(s_val, StrictJsonValue::Null) {
            None
        } else {
            let s_obj = s_val.as_object().ok_or_else(|| {
                EasyOfficeError::InvalidArgument("'context.sheet'는 객체여야 합니다.".to_string())
            })?;
            let name = opt_str(s_obj, "name")?
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("Sheet1")
                .to_string();
            let cells = parse_sheet_cells(s_obj)?;
            Some(SheetContext { name, cells })
        }
    } else {
        None
    };

    let doc = if let Some(d_val) = StrictJsonValue::get_field(obj, "doc") {
        if matches!(d_val, StrictJsonValue::Null) {
            None
        } else {
            let d_obj = d_val.as_object().ok_or_else(|| {
                EasyOfficeError::InvalidArgument("'context.doc'는 객체여야 합니다.".to_string())
            })?;
            Some(parse_doc_context(d_obj)?)
        }
    } else {
        None
    };

    let slide = if let Some(sl_val) = StrictJsonValue::get_field(obj, "slide") {
        if matches!(sl_val, StrictJsonValue::Null) {
            None
        } else {
            let sl_obj = sl_val.as_object().ok_or_else(|| {
                EasyOfficeError::InvalidArgument("'context.slide'는 객체여야 합니다.".to_string())
            })?;
            Some(parse_slide_context(sl_obj)?)
        }
    } else {
        None
    };

    let pdf = if let Some(p_val) = StrictJsonValue::get_field(obj, "pdf") {
        if matches!(p_val, StrictJsonValue::Null) {
            None
        } else {
            let p_obj = p_val.as_object().ok_or_else(|| {
                EasyOfficeError::InvalidArgument("'context.pdf'는 객체여야 합니다.".to_string())
            })?;
            let text = opt_str(p_obj, "text")?.unwrap_or("").to_string();
            if text.len() > MAX_TEXT_CONTEXT_BYTES {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "PDF 추출 텍스트 크기가 상한을 초과했습니다.".to_string(),
                ));
            }
            let page_count = StrictJsonValue::get_field(p_obj, "pageCount")
                .and_then(StrictJsonValue::as_u64)
                .and_then(|v| usize::try_from(v).ok())
                .unwrap_or(0);
            Some(PdfContext { text, page_count })
        }
    } else {
        None
    };

    Ok(CopilotContext {
        sheet,
        doc,
        slide,
        pdf,
    })
}

fn parse_sheet_cells(
    obj: &[(String, StrictJsonValue)],
) -> Result<Vec<SheetCellInput>, EasyOfficeError> {
    let Some(cells_val) = StrictJsonValue::get_field(obj, "cells") else {
        return Ok(Vec::new());
    };
    if matches!(cells_val, StrictJsonValue::Null) {
        return Ok(Vec::new());
    }
    let items = cells_val.as_array().ok_or_else(|| {
        EasyOfficeError::InvalidArgument("'sheet.cells'는 배열이어야 합니다.".to_string())
    })?;
    if items.len() > MAX_SHEET_CELLS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "시트 셀 수({})가 상한({MAX_SHEET_CELLS}개)을 초과했습니다.",
            items.len()
        )));
    }
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let cell_obj = item.as_object().ok_or_else(|| {
            EasyOfficeError::InvalidArgument("시트 셀 항목은 객체여야 합니다.".to_string())
        })?;
        let coord = match opt_str(cell_obj, "coord")? {
            Some(c) => c,
            None => match opt_str(cell_obj, "cell")? {
                Some(c) => c,
                None => opt_str(cell_obj, "ref")?.unwrap_or(""),
            },
        }
        .trim()
        .to_string();
        if coord.is_empty() {
            continue;
        }
        let raw = opt_scalar_string(cell_obj, "raw")?;
        let value = opt_scalar_string(cell_obj, "value")?;
        out.push(SheetCellInput { coord, raw, value });
    }
    Ok(out)
}

fn parse_doc_context(obj: &[(String, StrictJsonValue)]) -> Result<DocContext, EasyOfficeError> {
    let mut total_bytes = 0usize;
    let paragraphs = match StrictJsonValue::get_field(obj, "paragraphs") {
        None | Some(StrictJsonValue::Null) => Vec::new(),
        Some(v) => {
            let items = v.as_array().ok_or_else(|| {
                EasyOfficeError::InvalidArgument(
                    "'doc.paragraphs'는 배열이어야 합니다.".to_string(),
                )
            })?;
            if items.len() > MAX_DOC_PARAGRAPHS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "문서 문단 수({})가 상한({MAX_DOC_PARAGRAPHS}개)을 초과했습니다.",
                    items.len()
                )));
            }
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                let s = item.as_str().ok_or_else(|| {
                    EasyOfficeError::InvalidArgument(
                        "'doc.paragraphs' 항목은 문자열이어야 합니다.".to_string(),
                    )
                })?;
                total_bytes = total_bytes.saturating_add(s.len());
                if total_bytes > MAX_TEXT_CONTEXT_BYTES {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "문서 문단 총 크기가 상한을 초과했습니다.".to_string(),
                    ));
                }
                out.push(s.to_string());
            }
            out
        }
    };
    let selected_text = opt_str(obj, "selectedText")?.unwrap_or("").to_string();
    if total_bytes.saturating_add(selected_text.len()) > MAX_TEXT_CONTEXT_BYTES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "문서 선택 텍스트 크기가 상한을 초과했습니다.".to_string(),
        ));
    }
    Ok(DocContext {
        paragraphs,
        selected_text,
    })
}

fn parse_slide_context(obj: &[(String, StrictJsonValue)]) -> Result<SlideContext, EasyOfficeError> {
    let active_index = StrictJsonValue::get_field(obj, "activeIndex")
        .and_then(StrictJsonValue::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .unwrap_or(0);
    let slides = match StrictJsonValue::get_field(obj, "slides") {
        None | Some(StrictJsonValue::Null) => Vec::new(),
        Some(v) => {
            let items = v.as_array().ok_or_else(|| {
                EasyOfficeError::InvalidArgument("'slide.slides'는 배열이어야 합니다.".to_string())
            })?;
            if items.len() > MAX_SLIDES {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "슬라이드 수({})가 상한({MAX_SLIDES}개)을 초과했습니다.",
                    items.len()
                )));
            }
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                let s_obj = item.as_object().ok_or_else(|| {
                    EasyOfficeError::InvalidArgument("슬라이드 항목은 객체여야 합니다.".to_string())
                })?;
                let title = opt_str(s_obj, "title")?.unwrap_or("").to_string();
                let notes = opt_str(s_obj, "notes")?.unwrap_or("").to_string();
                let bullets = match StrictJsonValue::get_field(s_obj, "bullets") {
                    None | Some(StrictJsonValue::Null) => opt_str(s_obj, "body")?
                        .unwrap_or("")
                        .lines()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .take(MAX_SLIDE_BULLETS)
                        .map(ToString::to_string)
                        .collect(),
                    Some(bv) => {
                        let arr = bv.as_array().ok_or_else(|| {
                            EasyOfficeError::InvalidArgument(
                                "'slide.bullets'는 문자열 배열이어야 합니다.".to_string(),
                            )
                        })?;
                        if arr.len() > MAX_SLIDE_BULLETS {
                            return Err(EasyOfficeError::ResourceBudgetExceeded(
                                "슬라이드 글머리 기호 수가 상한을 초과했습니다.".to_string(),
                            ));
                        }
                        let mut bs = Vec::with_capacity(arr.len());
                        for b in arr {
                            let s = b.as_str().ok_or_else(|| {
                                EasyOfficeError::InvalidArgument(
                                    "'slide.bullets' 항목은 문자열이어야 합니다.".to_string(),
                                )
                            })?;
                            bs.push(s.to_string());
                        }
                        bs
                    }
                };
                out.push(SlideItem {
                    title,
                    bullets,
                    notes,
                });
            }
            out
        }
    };
    Ok(SlideContext {
        active_index,
        slides,
    })
}
