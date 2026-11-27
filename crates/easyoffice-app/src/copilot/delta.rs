//! 세션 앵커 기반 Tier 4 가변 상태(선택 범위·선택 문단·현재 쪽·현재 슬라이드·변경분 delta) 분리 및 보충기.
//!
//! Tier 2 고정 문서 스냅샷(`anchor`)에서 가변 필드를 분리하고, 창 밖으로 밀린 선택 셀·활성 PDF 쪽·
//! 최신 문서 변경분(`contextDelta`)만 이스케이프·바이트 상한 검증을 거쳐 Tier 4 꼬리에 덧붙인다.

use easyoffice_formats::{EasyOfficeError, StrictJsonValue};
use std::collections::BTreeSet;
use std::fmt::Write as _;

use super::cache::clamp_utf8_boundary;
use super::compact::{
    format_cell_display, parse_cell_coord, parse_pdf_page_prefix, parse_range_bounds, slide_line,
};
use super::context::PdfContext;
use super::prompt::escape_context_text;
use super::types::{opt_str, CopilotModule, CopilotRunRequest, MAX_DOC_PARAGRAPHS, MAX_SLIDES};

pub const MAX_DELTA_TEXT_BYTES: usize = 8 * 1024;
pub const MAX_DELTA_UPDATED_PARAGRAPHS: usize = 64;
pub const MAX_TIER4_VOLATILE_BYTES: usize = 12 * 1024;
pub const MAX_TIER4_SUPPLEMENT_CELLS: usize = 100;

/// 앵커 이후 클라이언트가 보낸 증분 변경 메타와 요약 줄.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextDelta {
    /// `[문서 최신 변경분]` 본문에 실릴 변경 요약 텍스트(이스케이프 전 원문).
    pub summary: String,
    /// 앵커 이후 최신 총 문단 수(문단 추가 뒤 `doc.replace_paragraphs` 인덱스 상한 보정에 사용).
    pub doc_total_paragraphs: Option<usize>,
    /// 앵커 이후 수정·추가된 문단 원문(`(index, raw_text)`), `doc.replace_span` 검증에 사용.
    pub updated_paragraphs: Vec<(usize, String)>,
    /// 앵커 이후 최신 슬라이드 개수.
    pub slide_count: Option<usize>,
}

/// Tier 2 고정 스냅샷에 실제로 실린 항목 집합(Tier 4 중복 회피 및 창 밖 보충 판별용).
#[derive(Debug, Clone, Default)]
pub struct Tier2Included {
    pub doc_paras: BTreeSet<usize>,
    pub doc_selected_matches_loaded: bool,
    pub sheet_coords: BTreeSet<(u32, u32)>,
    pub pdf_pages: BTreeSet<usize>,
    pub slide_expanded: BTreeSet<usize>,
}

fn opt_bounded_usize(
    obj: &[(String, StrictJsonValue)],
    key: &str,
    max: usize,
    what: &str,
) -> Result<Option<usize>, EasyOfficeError> {
    match StrictJsonValue::get_field(obj, key) {
        None | Some(StrictJsonValue::Null) => Ok(None),
        Some(v) => {
            let n = v.as_u64().ok_or_else(|| {
                EasyOfficeError::InvalidArgument(format!(
                    "'contextDelta.{key}'는 음수 없는 정수여야 합니다."
                ))
            })?;
            let val = usize::try_from(n).map_err(|_| {
                EasyOfficeError::ResourceBudgetExceeded(format!(
                    "{what} 값이 허용 범위를 초과했습니다."
                ))
            })?;
            if val > max {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "{what}({val})가 상한({max})을 초과했습니다."
                )));
            }
            Ok(Some(val))
        }
    }
}

pub fn parse_context_delta(
    entries: &[(String, StrictJsonValue)],
) -> Result<Option<ContextDelta>, EasyOfficeError> {
    let Some(val) = StrictJsonValue::get_field(entries, "contextDelta") else {
        return Ok(None);
    };
    if matches!(val, StrictJsonValue::Null) {
        return Ok(None);
    }
    if let Some(raw) = val.as_str() {
        if raw.len() > MAX_DELTA_TEXT_BYTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "문서 변경분 크기({} bytes)가 상한({MAX_DELTA_TEXT_BYTES} bytes)을 초과했습니다.",
                raw.len()
            )));
        }
        let trimmed = raw.trim();
        return Ok((!trimmed.is_empty()).then(|| ContextDelta {
            summary: trimmed.to_string(),
            ..ContextDelta::default()
        }));
    }
    let obj = val.as_object().ok_or_else(|| {
        EasyOfficeError::InvalidArgument(
            "'contextDelta'는 문자열 또는 객체여야 합니다.".to_string(),
        )
    })?;

    let mut summary = String::new();
    let direct_text = match opt_str(obj, "summary")? {
        Some(s) => Some(s),
        None => opt_str(obj, "text")?,
    };
    if let Some(t) = direct_text {
        if t.len() > MAX_DELTA_TEXT_BYTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "문서 변경분 크기({} bytes)가 상한({MAX_DELTA_TEXT_BYTES} bytes)을 초과했습니다.",
                t.len()
            )));
        }
        summary.push_str(t.trim());
    }
    if let Some(lines_val) = StrictJsonValue::get_field(obj, "lines") {
        if !matches!(lines_val, StrictJsonValue::Null) {
            let arr = lines_val.as_array().ok_or_else(|| {
                EasyOfficeError::InvalidArgument(
                    "'contextDelta.lines'는 문자열 배열이어야 합니다.".to_string(),
                )
            })?;
            for item in arr {
                let line = item.as_str().ok_or_else(|| {
                    EasyOfficeError::InvalidArgument(
                        "'contextDelta.lines' 항목은 문자열이어야 합니다.".to_string(),
                    )
                })?;
                let add = line.len() + usize::from(!summary.is_empty());
                if summary.len().saturating_add(add) > MAX_DELTA_TEXT_BYTES {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "문서 변경분 줄 총 크기가 상한({MAX_DELTA_TEXT_BYTES} bytes)을 초과했습니다."
                    )));
                }
                if !summary.is_empty() {
                    summary.push('\n');
                }
                summary.push_str(line);
            }
        }
    }

    let doc_total_paragraphs = opt_bounded_usize(
        obj,
        "docTotalParagraphs",
        MAX_DOC_PARAGRAPHS,
        "contextDelta.docTotalParagraphs",
    )?;
    let slide_count = opt_bounded_usize(obj, "slideCount", MAX_SLIDES, "contextDelta.slideCount")?;

    let mut updated_paragraphs = Vec::new();
    if let Some(up_val) = StrictJsonValue::get_field(obj, "updatedParagraphs") {
        if !matches!(up_val, StrictJsonValue::Null) {
            let items = up_val.as_array().ok_or_else(|| {
                EasyOfficeError::InvalidArgument(
                    "'contextDelta.updatedParagraphs'는 배열이어야 합니다.".to_string(),
                )
            })?;
            if items.len() > MAX_DELTA_UPDATED_PARAGRAPHS {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "변경 문단 수({})가 상한({MAX_DELTA_UPDATED_PARAGRAPHS}개)을 초과했습니다.",
                    items.len()
                )));
            }
            let mut total_bytes = summary.len();
            for item in items {
                let p_obj = item.as_object().ok_or_else(|| {
                    EasyOfficeError::InvalidArgument(
                        "변경 문단 항목은 객체여야 합니다.".to_string(),
                    )
                })?;
                let idx_u64 = StrictJsonValue::get_field(p_obj, "index")
                    .and_then(StrictJsonValue::as_u64)
                    .ok_or_else(|| {
                        EasyOfficeError::InvalidArgument(
                            "변경 문단 항목에는 음수 없는 정수 'index'가 필요합니다.".to_string(),
                        )
                    })?;
                let index = usize::try_from(idx_u64).unwrap_or(usize::MAX);
                if index >= MAX_DOC_PARAGRAPHS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "변경 문단 인덱스가 상한을 초과했습니다.".to_string(),
                    ));
                }
                let text = opt_str(p_obj, "text")?.unwrap_or("");
                total_bytes = total_bytes.saturating_add(text.len());
                if total_bytes > MAX_DELTA_TEXT_BYTES {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "변경 문단 총 크기가 상한({MAX_DELTA_TEXT_BYTES} bytes)을 초과했습니다."
                    )));
                }
                updated_paragraphs.push((index, text.to_string()));
            }
        }
    }

    if summary.is_empty()
        && doc_total_paragraphs.is_none()
        && updated_paragraphs.is_empty()
        && slide_count.is_none()
    {
        return Ok(None);
    }

    Ok(Some(ContextDelta {
        summary,
        doc_total_paragraphs,
        updated_paragraphs,
        slide_count,
    }))
}

fn push_volatile_line(out: &mut String, line: &str) -> bool {
    if out.len().saturating_add(line.len()).saturating_add(1) > MAX_TIER4_VOLATILE_BYTES {
        return false;
    }
    out.push_str(line);
    out.push('\n');
    true
}

fn extract_pdf_page_lines(pdf: &PdfContext, target_page: usize) -> Vec<String> {
    let mut current_page = 1usize;
    let mut lines = Vec::new();
    for raw in pdf.text.lines() {
        if let Some(p) = parse_pdf_page_prefix(raw) {
            current_page = p;
        }
        if current_page == target_page {
            lines.push(escape_context_text(raw).into_owned());
        }
    }
    lines
}

/// 일반 대화(`!req.lightweight && req.command.is_none()`)용 Tier 4 가변 상태·변경분 문자열을 조립한다.
pub fn build_tier4_volatile_state(req: &CopilotRunRequest, tier2: &Tier2Included) -> String {
    if req.lightweight || req.command.is_some() {
        return String::new();
    }
    let mut out = String::new();
    if let Some(range) = &req.selection.range {
        let _ = push_volatile_line(
            &mut out,
            &format!("선택 범위: {}", escape_context_text(range)),
        );
    }
    match req.module {
        CopilotModule::Doc | CopilotModule::Hwp => {
            if let Some(start) = req.selection.paragraph_index {
                let end = req.selection.paragraph_end.unwrap_or(start);
                let _ = push_volatile_line(&mut out, &format!("선택 문단: [{start}]~[{end}]"));
            }
            if let Some(doc) = &req.context.doc {
                if !doc.selected_text.trim().is_empty() && !tier2.doc_selected_matches_loaded {
                    let esc = escape_context_text(&doc.selected_text);
                    let line = format!("선택 텍스트: {esc}");
                    if !push_volatile_line(&mut out, &line) {
                        let room = MAX_TIER4_VOLATILE_BYTES
                            .saturating_sub(out.len() + "선택 텍스트: \n".len());
                        let cut = clamp_utf8_boundary(&esc, room);
                        if cut > 0 {
                            let _ = writeln!(out, "선택 텍스트: {}", &esc[..cut]);
                        }
                    }
                }
                if let Some(start) = req.selection.paragraph_index {
                    let end = req
                        .selection
                        .paragraph_end
                        .unwrap_or(start)
                        .min(doc.paragraphs.len().saturating_sub(1));
                    let mut wrote_header = false;
                    for idx in start..=end {
                        if tier2.doc_paras.contains(&idx) {
                            continue;
                        }
                        if let Some(Some(p)) = doc.paragraphs.get(idx) {
                            if !wrote_header {
                                if !push_volatile_line(&mut out, "[선택 문단 보충]") {
                                    break;
                                }
                                wrote_header = true;
                            }
                            if !push_volatile_line(
                                &mut out,
                                &format!("  [{idx}] {}", escape_context_text(p)),
                            ) {
                                break;
                            }
                        }
                    }
                }
            }
        }
        CopilotModule::Sheet => {
            if let Some(sel_sheet) = &req.selection.sheet {
                if req
                    .context
                    .sheet
                    .as_ref()
                    .is_some_and(|s| s.name != *sel_sheet)
                {
                    let _ = push_volatile_line(
                        &mut out,
                        &format!("선택 시트: {}", escape_context_text(sel_sheet)),
                    );
                }
            }
            if let (Some(sheet), Some(bounds)) = (
                req.context.sheet.as_ref(),
                req.selection.range.as_deref().and_then(parse_range_bounds),
            ) {
                let ((r1, c1), (r2, c2)) = bounds;
                let mut missing: Vec<(u32, u32, usize)> = sheet
                    .cells
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, cell)| {
                        let (r, c) = parse_cell_coord(&cell.coord)?;
                        ((r1..=r2).contains(&r)
                            && (c1..=c2).contains(&c)
                            && !tier2.sheet_coords.contains(&(r, c)))
                        .then_some((r, c, idx))
                    })
                    .collect();
                missing.sort_by_key(|&(r, c, idx)| (r, c, idx));
                if !missing.is_empty() && push_volatile_line(&mut out, "[선택 범위 보충]") {
                    for (_, _, idx) in missing.into_iter().take(MAX_TIER4_SUPPLEMENT_CELLS) {
                        let cell = &sheet.cells[idx];
                        let display = format_cell_display(&cell.raw, &cell.value);
                        if !push_volatile_line(
                            &mut out,
                            &format!("  {}: {display}", escape_context_text(&cell.coord)),
                        ) {
                            break;
                        }
                    }
                }
            }
        }
        CopilotModule::Slide => {
            if let Some(slide) = &req.context.slide {
                let _ = push_volatile_line(
                    &mut out,
                    &format!("현재 슬라이드 인덱스: {}", slide.active_index),
                );
                if slide.active_index < slide.slides.len()
                    && !tier2.slide_expanded.is_empty()
                    && !tier2.slide_expanded.contains(&slide.active_index)
                    && push_volatile_line(&mut out, "[현재 슬라이드 상세]")
                {
                    let detail = slide_line(slide.active_index, &slide.slides[slide.active_index]);
                    let _ = push_volatile_line(&mut out, &detail);
                }
            }
            if let Some(sel_idx) = req.selection.slide_index {
                let _ = push_volatile_line(&mut out, &format!("선택 슬라이드: {sel_idx}"));
            }
        }
        CopilotModule::Pdf => {
            if let Some(pdf) = &req.context.pdf {
                let active = pdf.active_page.unwrap_or(1).max(1);
                let _ = push_volatile_line(&mut out, &format!("현재 쪽: {active}"));
                if !tier2.pdf_pages.is_empty() && !tier2.pdf_pages.contains(&active) {
                    let page_lines = extract_pdf_page_lines(pdf, active);
                    if !page_lines.is_empty() && push_volatile_line(&mut out, "[현재 쪽 발췌]")
                    {
                        for line in page_lines {
                            if !push_volatile_line(&mut out, &line) {
                                break;
                            }
                        }
                    }
                }
            }
        }
        CopilotModule::Base | CopilotModule::Board => {}
    }

    if let Some(delta) = &req.context_delta {
        let trimmed = delta.summary.trim();
        if !trimmed.is_empty() && push_volatile_line(&mut out, "[문서 최신 변경분]") {
            let esc = escape_context_text(trimmed);
            for line in esc.lines() {
                if !push_volatile_line(&mut out, line) {
                    let room = MAX_TIER4_VOLATILE_BYTES.saturating_sub(out.len() + 1);
                    let cut = clamp_utf8_boundary(line, room);
                    if cut > 0 {
                        out.push_str(&line[..cut]);
                        out.push('\n');
                    }
                    break;
                }
            }
        }
    }

    while out.ends_with('\n') {
        out.pop();
    }
    out
}
