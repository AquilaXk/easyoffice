//! Structured readback and data-integrity checks across sheets, documents, slides, Hangul,
//! Pages, and PDF (PD-248).
//!
//! Shared by:
//! - `convert_document_format` with `"action": "inspect"` (or `"readback"`), on either binary
//!   `"sourceBase64"` files or in-memory structured `"blocks"` / `"sheets"` / `"slides"` payloads;
//! - `evaluate_sheet` with `"action": "inspect"` (stateless workbook) or
//!   `"sessionAction": "inspect"` (live [`easyoffice_sheet::RecalcSession`]);
//! - `easyoffice-app --inspect <FILE>` (`--readback <FILE>`) CLI.

mod doc_slide_pdf;
mod sheet;

use std::fmt::Write;
use std::time::Instant;

use easyoffice_formats::{
    CellBox, DecodedContent, DocumentFormat, EasyOfficeError, FormatDetector, PdfSerializer,
    PdfWriteOptions, StrictJsonValue, UniversalFormatConverter,
};
use easyoffice_sheet::{Budget, RecalcError, RecalcSession};

use crate::convert_structured::{parse_structured_content, StructuredContent};
use crate::eval_limit::save_eval_time;
use crate::sheet_rules_json::parse_rules;
use crate::sheet_session::{self, EXPIRED, LIMITS, REFUSED};
use crate::EasyOfficeIpcRouter;

pub const DEFAULT_READBACK_ITEMS: usize = 1_000;
pub const MAX_READBACK_ITEMS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadbackQuery {
    pub sheet: Option<String>,
    pub range: Option<CellBox>,
    pub max_items: usize,
}

impl Default for ReadbackQuery {
    fn default() -> Self {
        Self {
            sheet: None,
            range: None,
            max_items: DEFAULT_READBACK_ITEMS,
        }
    }
}

pub(super) fn push_json_str(out: &mut String, s: &str) {
    out.push('"');
    EasyOfficeIpcRouter::escape_json_into(out, s);
    out.push('"');
}

pub(super) fn push_opt_json_str(out: &mut String, s: Option<&str>) {
    match s {
        Some(v) => push_json_str(out, v),
        None => out.push_str("null"),
    }
}

pub(super) fn same_sheet_name(a: &str, b: &str) -> bool {
    if a.is_ascii() && b.is_ascii() {
        a.eq_ignore_ascii_case(b)
    } else {
        a.chars()
            .flat_map(char::to_lowercase)
            .eq(b.chars().flat_map(char::to_lowercase))
    }
}

fn unquote_sheet_prefix(raw: &str) -> Result<String, EasyOfficeError> {
    let t = raw.trim();
    if t.is_empty() {
        return Err(EasyOfficeError::InvalidArgument(
            "범위의 시트 이름이 비어 있습니다.".to_string(),
        ));
    }
    if let Some(inner) = t.strip_prefix('\'') {
        let Some(body) = inner.strip_suffix('\'') else {
            return Err(EasyOfficeError::InvalidArgument(format!(
                "닫히지 않은 시트 이름 인용입니다: {raw}"
            )));
        };
        let unescaped = body.replace("''", "'");
        if unescaped.trim().is_empty() {
            return Err(EasyOfficeError::InvalidArgument(
                "범위의 시트 이름이 비어 있습니다.".to_string(),
            ));
        }
        return Ok(unescaped);
    }
    Ok(t.to_string())
}

/// Splits an optional `Sheet1!A1:D10` or `'분기 요약'!B2:E20` notation into `(sheet, CellBox)`.
fn parse_sheet_and_range(
    explicit_sheet: Option<&str>,
    raw_range: Option<&str>,
) -> Result<(Option<String>, Option<CellBox>), EasyOfficeError> {
    let mut sheet = explicit_sheet
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);
    let Some(range_str) = raw_range.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok((sheet, None));
    };
    let a1_part = if let Some((prefix, rest)) = split_sheet_bang(range_str) {
        let from_range = unquote_sheet_prefix(prefix)?;
        if let Some(existing) = &sheet {
            if !same_sheet_name(existing, &from_range) {
                return Err(EasyOfficeError::InvalidArgument(format!(
                    "지정한 시트('{existing}')와 범위 시트('{from_range}')가 서로 다릅니다."
                )));
            }
        } else {
            sheet = Some(from_range);
        }
        rest.trim()
    } else {
        range_str
    };
    let parsed = CellBox::parse(a1_part).ok_or_else(|| {
        EasyOfficeError::InvalidArgument(format!("유효하지 않은 A1 범위입니다: {range_str}"))
    })?;
    Ok((sheet, Some(parsed)))
}

fn split_sheet_bang(s: &str) -> Option<(&str, &str)> {
    let bytes = s.as_bytes();
    if bytes.first() == Some(&b'\'') {
        let mut i = 1usize;
        while i < bytes.len() {
            if bytes[i] == b'\'' {
                if bytes.get(i + 1) == Some(&b'\'') {
                    i += 2;
                    continue;
                }
                if bytes.get(i + 1) == Some(&b'!') {
                    return Some((&s[..=i], &s[i + 2..]));
                }
                return None;
            }
            i += 1;
        }
        None
    } else {
        s.split_once('!')
    }
}

pub fn validate_max_items(value: u64) -> Result<usize, EasyOfficeError> {
    if value == 0 || value > MAX_READBACK_ITEMS as u64 {
        return Err(EasyOfficeError::InvalidArgument(format!(
            "'maxItems'는 1 이상 {MAX_READBACK_ITEMS} 이하의 정수여야 합니다."
        )));
    }
    Ok(value as usize)
}

impl ReadbackQuery {
    pub fn from_cli(
        sheet: Option<&str>,
        range: Option<&str>,
        max_items: Option<usize>,
    ) -> Result<Self, EasyOfficeError> {
        let (sheet, range) = parse_sheet_and_range(sheet, range)?;
        let max_items = match max_items {
            Some(n) => validate_max_items(n as u64)?,
            None => DEFAULT_READBACK_ITEMS,
        };
        Ok(Self {
            sheet,
            range,
            max_items,
        })
    }

    pub fn from_entries(entries: &[(String, StrictJsonValue)]) -> Result<Self, EasyOfficeError> {
        let sheet_raw = EasyOfficeIpcRouter::optional_json_str(entries, "sheet")?;
        let range_raw = EasyOfficeIpcRouter::optional_json_str(entries, "range")?;
        let max_items = match StrictJsonValue::get_field(entries, "maxItems") {
            None | Some(StrictJsonValue::Null) => DEFAULT_READBACK_ITEMS,
            Some(v) => v
                .as_u64()
                .ok_or_else(|| {
                    EasyOfficeError::InvalidArgument(format!(
                        "'maxItems'는 1 이상 {MAX_READBACK_ITEMS} 이하의 정수여야 합니다."
                    ))
                })
                .and_then(validate_max_items)?,
        };
        let (sheet, range) = parse_sheet_and_range(sheet_raw, range_raw)?;
        Ok(Self {
            sheet,
            range,
            max_items,
        })
    }
}

pub(crate) fn is_inspect_action(action: Option<&str>) -> bool {
    matches!(action, Some("inspect" | "readback"))
}

fn write_envelope_prefix(
    out: &mut String,
    file_name: &str,
    fmt: DocumentFormat,
    kind: &str,
    title: &str,
    source_bytes: usize,
    warnings: &[String],
) {
    out.push_str("{\"status\":\"OK\",\"action\":\"inspect\",\"fileName\":");
    push_json_str(out, file_name);
    let _ = write!(
        out,
        ",\"format\":\"{}\",\"module\":\"{}\",\"kind\":\"{kind}\",\"title\":",
        fmt.extension(),
        fmt.editor_module(),
    );
    push_json_str(out, title);
    let _ = write!(out, ",\"sourceBytes\":{source_bytes},\"specification\":");
    push_json_str(out, fmt.standard_specification());
    out.push_str(",\"warnings\":[");
    for (idx, w) in warnings.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        push_json_str(out, w);
    }
    out.push_str("],");
}

pub fn inspect_source_bytes(
    bytes: &[u8],
    file_name: Option<&str>,
    source_format_hint: Option<&str>,
    query: &ReadbackQuery,
) -> Result<String, EasyOfficeError> {
    let clean_name = file_name.map(str::trim).filter(|s| !s.is_empty());
    let hint = clean_name.or_else(|| source_format_hint.map(str::trim).filter(|s| !s.is_empty()));
    let detected = FormatDetector::detect(bytes, hint)?;
    if detected == DocumentFormat::Pdf {
        let report = PdfSerializer::inspect_readback(bytes)?;
        let mut out = String::with_capacity(2048 + report.pages.len() * 256);
        write_envelope_prefix(
            &mut out,
            clean_name.unwrap_or(""),
            DocumentFormat::Pdf,
            "pdf",
            &report.title,
            bytes.len(),
            &[],
        );
        doc_slide_pdf::write_pdf_readback(&mut out, &report, query.max_items);
        out.push('}');
        return Ok(out);
    }

    let (decoded, styling, slide_deck) = UniversalFormatConverter::decode_rich_full(bytes, hint)?;
    let mut warnings = decoded.warnings.clone();
    if matches!(decoded.content, DecodedContent::Workbook(_)) {
        sheet::append_styling_warnings(&styling, &mut warnings);
    }
    let kind = match &decoded.content {
        DecodedContent::Workbook(_) => "workbook",
        DecodedContent::Document(_) => "document",
        DecodedContent::Slides(_) => "slides",
    };
    let mut out = String::with_capacity(4096);
    write_envelope_prefix(
        &mut out,
        clean_name.unwrap_or(""),
        decoded.format,
        kind,
        &decoded.title,
        bytes.len(),
        &warnings,
    );
    match &decoded.content {
        DecodedContent::Workbook(sheets) => {
            sheet::write_workbook_readback(&mut out, sheets, &styling, query)?;
        }
        DecodedContent::Document(doc) => {
            doc_slide_pdf::write_document_readback(&mut out, doc, query.max_items);
        }
        DecodedContent::Slides(slides) => {
            if let Some(deck) = slide_deck.as_ref() {
                doc_slide_pdf::write_slide_deck_readback(&mut out, deck, query.max_items);
            } else {
                doc_slide_pdf::write_slides_readback(&mut out, slides, query.max_items);
            }
        }
    }
    out.push('}');
    Ok(out)
}

pub(crate) fn inspect_structured_entries(
    entries: &[(String, StrictJsonValue)],
) -> Result<String, EasyOfficeError> {
    let query = ReadbackQuery::from_entries(entries)?;
    let src_ext = match EasyOfficeIpcRouter::optional_json_str(entries, "sourceFormat")? {
        Some(s) => s,
        None => EasyOfficeIpcRouter::optional_json_str(entries, "source")?.unwrap_or("docx"),
    }
    .trim();
    let fmt = DocumentFormat::from_extension(src_ext).ok_or_else(|| {
        EasyOfficeError::InvalidArgument(format!("지원하지 않는 원본 포맷: {src_ext}"))
    })?;
    let file_name = EasyOfficeIpcRouter::optional_json_str(entries, "fileName")?
        .map(str::trim)
        .unwrap_or("");
    let title = EasyOfficeIpcRouter::optional_json_str(entries, "title")?
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("문서");
    let Some((structured, mut warnings)) = parse_structured_content(entries)? else {
        return Err(EasyOfficeError::InvalidArgument(
            "inspect 요청에는 sourceBase64 또는 구조화 본문(blocks/sheets/slides)이 필요합니다."
                .to_string(),
        ));
    };
    let mut out = String::with_capacity(4096);
    match structured {
        StructuredContent::Workbook(sheets, stylings) => {
            let named: Vec<(String, easyoffice_formats::SheetStyling)> = sheets
                .iter()
                .enumerate()
                .map(|(idx, (name, _))| {
                    (name.clone(), stylings.get(idx).cloned().unwrap_or_default())
                })
                .collect();
            sheet::append_styling_warnings(&named, &mut warnings);
            write_envelope_prefix(&mut out, file_name, fmt, "workbook", title, 0, &warnings);
            sheet::write_workbook_readback(&mut out, &sheets, &named, &query)?;
        }
        StructuredContent::Document(doc, PdfWriteOptions { .. }) => {
            write_envelope_prefix(&mut out, file_name, fmt, "document", title, 0, &warnings);
            doc_slide_pdf::write_document_readback(&mut out, &doc, query.max_items);
        }
        StructuredContent::SlideDeck(deck) => {
            write_envelope_prefix(&mut out, file_name, fmt, "slides", title, 0, &warnings);
            doc_slide_pdf::write_slide_deck_readback(&mut out, &deck, query.max_items);
        }
        StructuredContent::Slides(slides) => {
            write_envelope_prefix(&mut out, file_name, fmt, "slides", title, 0, &warnings);
            doc_slide_pdf::write_slides_readback(&mut out, &slides, query.max_items);
        }
    }
    out.push('}');
    Ok(out)
}

pub(crate) fn inspect_stateless_sheet(
    entries: &[(String, StrictJsonValue)],
) -> Result<String, EasyOfficeError> {
    let query = ReadbackQuery::from_entries(entries)?;
    let default_sh = EasyOfficeIpcRouter::resolve_default_sheet_from_entries(entries)?;
    let wb = EasyOfficeIpcRouter::build_workbook_from_json_entries(entries, &default_sh)?;
    let rules_sheet = query.sheet.as_deref().unwrap_or(&default_sh);
    let mut stylings = Vec::new();
    if let Some(rules_val) = StrictJsonValue::get_field(entries, "rules") {
        if !matches!(rules_val, StrictJsonValue::Null) {
            let parsed = parse_rules(rules_val)?;
            let st = easyoffice_formats::SheetStyling {
                rules: parsed,
                ..Default::default()
            };
            stylings.push((rules_sheet.to_string(), st));
        }
    }
    let budget = Budget {
        max_edges: LIMITS.max_session_edges,
        max_graph_bytes: LIMITS.max_session_bytes,
        max_bytes: LIMITS.max_session_bytes,
        max_time: save_eval_time(),
    };
    let mut session = RecalcSession::with_budget(wb, budget).map_err(|err| match err {
        RecalcError::Invalid(e) => e,
        RecalcError::OverBudget => EasyOfficeError::ResourceBudgetExceeded(
            "시트 의존 그래프가 허용 크기를 넘었습니다.".to_string(),
        ),
    })?;
    let outcome = session.recalculate();
    let mut out = String::from("{\"status\":\"OK\",\"action\":\"inspect\",");
    sheet::write_session_readback(&mut out, &mut session, &outcome, &stylings, &query)?;
    out.push('}');
    Ok(out)
}

pub(crate) fn inspect_sheet_session(
    entries: &[(String, StrictJsonValue)],
    owner: &str,
) -> Result<String, EasyOfficeError> {
    let id = sheet_session::session_id(entries)?;
    let query = ReadbackQuery::from_entries(entries)?;
    let Some(handle) = sheet_session::registry().checkout(id, owner, Instant::now()) else {
        return Ok(EXPIRED.to_string());
    };
    let Ok(mut session) = handle.lock() else {
        return Ok(EXPIRED.to_string());
    };
    let outcome = session.recalculate();
    let mut out = format!("{{\"status\":\"OK\",\"session\":\"{id}\",\"action\":\"inspect\",");
    if let Err(err) = sheet::write_session_readback(&mut out, &mut session, &outcome, &[], &query) {
        return match err {
            EasyOfficeError::ResourceBudgetExceeded(_) => Ok(REFUSED.to_string()),
            other => Err(other),
        };
    }
    out.push('}');
    Ok(out)
}
