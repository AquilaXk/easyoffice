//! 검증에서 버려진 코파일럿 연산의 구체적 사유·좌표·검색어 요약(`droppedReasons`, 최대 8건) 생성기.

use easyoffice_formats::StrictJsonValue;

use super::cache::clamp_utf8_boundary;
use super::delta::MAX_DELTA_TEXT_BYTES;
use super::types::{
    is_valid_a1_coord, is_valid_range_ref, CopilotModule, CopilotRunRequest, MAX_LLM_OPERATIONS,
    MAX_PROMPT_CONTEXT_BYTES, MAX_SHEET_WRITE_CELLS, MAX_SLIDES,
};

pub const MAX_DROPPED_REASONS: usize = 8;
const MAX_REASON_BYTES: usize = 200;

pub fn push_reason(reasons: &mut Vec<String>, raw: String) {
    if reasons.len() >= MAX_DROPPED_REASONS {
        return;
    }
    let clean = raw.replace(['\r', '\n'], " ");
    let clipped = if clean.len() > MAX_REASON_BYTES {
        let cut = clamp_utf8_boundary(&clean, MAX_REASON_BYTES.saturating_sub(3));
        format!("{}...", &clean[..cut])
    } else {
        clean
    };
    if !clipped.is_empty() && !reasons.iter().any(|existing| existing == &clipped) {
        reasons.push(clipped);
    }
}

fn opt_usize_val(obj: &[(String, StrictJsonValue)], key: &str) -> Option<usize> {
    StrictJsonValue::get_field(obj, key)
        .and_then(StrictJsonValue::as_u64)
        .and_then(|v| usize::try_from(v).ok())
}

pub fn effective_doc_para_count(req: &CopilotRunRequest) -> usize {
    if let Some(delta_total) = req
        .context_delta
        .as_ref()
        .and_then(|d| d.doc_total_paragraphs)
    {
        return delta_total;
    }
    req.context
        .doc
        .as_ref()
        .map(|d| d.paragraphs.len() + d.omitted_paragraphs)
        .unwrap_or(usize::MAX)
}

pub fn effective_doc_para_text(req: &CopilotRunRequest, index: usize) -> Option<&str> {
    if let Some(delta) = &req.context_delta {
        if let Some((_, text)) = delta
            .updated_paragraphs
            .iter()
            .rev()
            .find(|(idx, _)| *idx == index)
        {
            return Some(text.as_str());
        }
    }
    req.context
        .doc
        .as_ref()
        .and_then(|d| d.paragraphs.get(index))
        .and_then(|slot| slot.as_deref())
}

pub fn effective_slide_count(req: &CopilotRunRequest) -> Option<usize> {
    req.context_delta
        .as_ref()
        .and_then(|d| d.slide_count)
        .or_else(|| req.context.slide.as_ref().map(|s| s.slides.len()))
}

fn clip_snippet(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        text
    } else {
        &text[..clamp_utf8_boundary(text, max_bytes)]
    }
}

pub fn diagnose_dropped_op(
    req: &CopilotRunRequest,
    obj: &[(String, StrictJsonValue)],
    known_slides: Option<usize>,
    op_matches: bool,
    plain_hits_fn: impl Fn(&str, &str) -> (usize, usize),
) -> String {
    let kind = StrictJsonValue::get_str(obj, "kind")
        .or_else(|| StrictJsonValue::get_str(obj, "op"))
        .map(str::trim)
        .unwrap_or("");
    if kind.is_empty() {
        return "연산 객체에 'kind' 또는 'op' 필드가 없습니다.".to_string();
    }
    if !op_matches {
        return format!(
            "연산 '{kind}'은(는) {} 모듈에서 허용되지 않습니다.",
            req.module.as_str()
        );
    }
    match kind {
        "doc.replace_span" => {
            let max_para = effective_doc_para_count(req);
            let Some(index) = opt_usize_val(obj, "index") else {
                return "doc.replace_span: 'index'(문단 번호)가 빠졌거나 음수입니다.".to_string();
            };
            if index >= max_para {
                return format!(
                    "doc.replace_span: 문단 인덱스 {index}가 문서 범위(0..{max_para})를 벗어났습니다."
                );
            }
            let search = StrictJsonValue::get_str(obj, "search").unwrap_or("");
            let replace = StrictJsonValue::get_str(obj, "replace").unwrap_or("");
            if search.trim().is_empty() || search.len() > MAX_DELTA_TEXT_BYTES {
                return format!(
                    "doc.replace_span(index={index}): 'search'가 비어 있거나 너무 깁니다."
                );
            }
            if replace.len() > MAX_PROMPT_CONTEXT_BYTES {
                return format!(
                    "doc.replace_span(index={index}): 'replace' 길이가 상한을 초과했습니다."
                );
            }
            if let Some(para_text) = effective_doc_para_text(req, index) {
                let (plain_hits, raw_hits) = plain_hits_fn(para_text, search);
                let snippet = clip_snippet(search, 48);
                if plain_hits > 1 || (plain_hits == 0 && raw_hits > 1) {
                    return format!(
                        "doc.replace_span(index={index}): search='{snippet}'가 문단에서 여러 번 일치해 모호합니다."
                    );
                }
                return format!(
                    "doc.replace_span(index={index}): search='{snippet}'가 대상 문단에서 일치하지 않습니다."
                );
            }
            format!("doc.replace_span(index={index}): 형식이 유효하지 않습니다.")
        }
        "doc.replace_paragraphs" => {
            let max_para = effective_doc_para_count(req);
            if let Some(s) = opt_usize_val(obj, "startIndex") {
                let e = opt_usize_val(obj, "endIndex").unwrap_or(s);
                if s > e || e >= max_para {
                    return format!(
                        "doc.replace_paragraphs: 범위 [{s}..{e}]가 문서 범위(0..{max_para}) 밖입니다."
                    );
                }
            }
            if let Some(arr) =
                StrictJsonValue::get_field(obj, "replacements").and_then(StrictJsonValue::as_array)
            {
                for item in arr {
                    if let Some(r_obj) = item.as_object() {
                        if let Some(idx) = opt_usize_val(r_obj, "index") {
                            if idx >= max_para {
                                return format!(
                                    "doc.replace_paragraphs: 문단 인덱스 {idx}가 범위(0..{max_para})를 벗어났습니다."
                                );
                            }
                        }
                    }
                }
            }
            if req.module == CopilotModule::Hwp {
                return "doc.replace_paragraphs(HWP): 표 블록·잘못된 인덱스·HTML 포함 여부를 확인하세요."
                    .to_string();
            }
            "doc.replace_paragraphs: replacements 또는 blocks/startIndex/endIndex 형식이 잘못되었습니다."
                .to_string()
        }
        "sheet.write" => {
            if let Some(range) = StrictJsonValue::get_str(obj, "range") {
                if !is_valid_range_ref(range) {
                    return format!(
                        "sheet.write: 잘못된 범위 '{}'",
                        clip_snippet(range.trim(), 40)
                    );
                }
            }
            if let Some(cells) =
                StrictJsonValue::get_field(obj, "cells").and_then(StrictJsonValue::as_array)
            {
                if cells.is_empty() || cells.len() > MAX_SHEET_WRITE_CELLS {
                    return format!(
                        "sheet.write: cells 개수({})가 허용 범위(1..={MAX_SHEET_WRITE_CELLS}) 밖입니다.",
                        cells.len()
                    );
                }
                for c in cells {
                    if let Some(coord) = c
                        .as_object()
                        .and_then(|co| StrictJsonValue::get_str(co, "coord"))
                    {
                        if !is_valid_a1_coord(coord) {
                            return format!(
                                "sheet.write: 잘못된 셀 좌표 '{}'",
                                clip_snippet(coord.trim(), 32)
                            );
                        }
                    }
                }
            }
            "sheet.write: cells 배열 또는 좌표·서식 필드가 올바르지 않습니다.".to_string()
        }
        "sheet.format" | "sheet.chart" | "sheet.table" | "sheet.pivot" | "sheet.cond_format"
        | "sheet.sort_filter" => {
            let range = StrictJsonValue::get_str(obj, "range")
                .or_else(|| StrictJsonValue::get_str(obj, "dataRange"))
                .or_else(|| StrictJsonValue::get_str(obj, "sourceRange"))
                .unwrap_or("");
            if !range.is_empty() && !is_valid_range_ref(range) {
                return format!("{kind}: 잘못된 범위 '{}'", clip_snippet(range.trim(), 40));
            }
            format!("{kind}: 필수 필드·범위·열거형 값이 올바르지 않습니다.")
        }
        "sheet.note" => {
            let coord = StrictJsonValue::get_str(obj, "coord").unwrap_or("");
            if !coord.is_empty() && !is_valid_a1_coord(coord) {
                return format!(
                    "sheet.note: 잘못된 좌표 '{}'",
                    clip_snippet(coord.trim(), 32)
                );
            }
            "sheet.note: coord 또는 text 필드가 유효하지 않습니다.".to_string()
        }
        "slide.add" if known_slides.is_some_and(|n| n >= MAX_SLIDES) => {
            format!("slide.add: 최대 슬라이드 수({MAX_SLIDES})에 도달했습니다.")
        }
        "slide.update" | "slide.add_element" => {
            let idx = opt_usize_val(obj, "slideIndex").or_else(|| opt_usize_val(obj, "index"));
            if let (Some(i), Some(total)) = (idx, known_slides) {
                if i >= total {
                    return format!("{kind}: 슬라이드 인덱스 {i}가 범위(0..{total}) 밖입니다.");
                }
            }
            format!("{kind}: 대상 필드 또는 개체 사양(960x540 경계)이 올바르지 않습니다.")
        }
        _ => format!("{kind}: 필수 필드, 좌표, 범위 또는 값 상한을 위반했습니다."),
    }
}

pub fn too_many_ops_reason(total: usize) -> String {
    format!("요청된 연산 수({total})가 한 번에 처리할 수 있는 상한({MAX_LLM_OPERATIONS}개)을 초과했습니다.")
}
