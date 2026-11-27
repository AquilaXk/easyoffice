//! 참조 구현: dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd.) 및 continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., see THIRD_PARTY_NOTICES.md)
//!
//! 모델이 돌려준 `operations`·`locations`·`toolCalls`를 검사한다. 요청 모듈(`req.module`)과 일치하지 않는
//! 교차 모듈 작업과 형식·범위·개수 위반 작업은 즉시 버리고 `droppedOperations`에 센다.

use easyoffice_formats::StrictJsonValue;

use super::context::SlideField;
pub use super::operations_ext::validate_tool_calls;
use super::operations_ext::{
    opt_hex, opt_usize, parse_slide_element, parse_slide_elements_opt, parse_slide_layout,
    parse_slide_transition, validate_base_or_board_op, validate_extended_sheet_op,
};
use super::types::{
    is_valid_a1_coord, is_valid_range_ref, parse_bounded_range, ChartLegend, ChartType,
    CopilotCommand, CopilotLocation, CopilotModule, CopilotOperation, CopilotRunRequest, DocBlock,
    DocReplacement, SheetWriteCell, MAX_CHART_HEIGHT, MAX_CHART_SERIES, MAX_CHART_TITLE_CHARS,
    MAX_CHART_WIDTH, MAX_DOC_APPEND_BLOCKS, MAX_DOC_TABLE_COLS, MAX_DOC_TABLE_ROWS,
    MAX_LLM_LOCATIONS, MAX_LLM_OPERATIONS, MAX_PROMPT_CONTEXT_BYTES, MAX_SHEET_WRITE_CELLS,
    MAX_SLIDES, MAX_SLIDE_BULLETS, MIN_CHART_HEIGHT, MIN_CHART_WIDTH,
};

/// 검사를 통과한 작업과 버린 작업 수.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ValidatedOperations {
    pub operations: Vec<CopilotOperation>,
    pub dropped: usize,
}

pub fn validate_operations(
    req: &CopilotRunRequest,
    items: &[StrictJsonValue],
) -> ValidatedOperations {
    let mut known_slides = req.context.slide.as_ref().map(|s| s.slides.len());
    let mut accepted: Vec<CopilotOperation> = Vec::new();
    for item in items.iter().take(MAX_LLM_OPERATIONS) {
        let Some(obj) = item.as_object() else {
            continue;
        };
        if let Some(op) = validate_single_operation(req, obj, known_slides) {
            if matches!(op, CopilotOperation::SlideAdd { .. }) {
                if let Some(n) = &mut known_slides {
                    *n = (*n + 1).min(MAX_SLIDES);
                }
            }
            accepted.push(op);
        }
    }
    let operations = if req.command.is_some() {
        restrict_to_command(req, accepted)
    } else {
        accepted
    };
    ValidatedOperations {
        dropped: items.len().saturating_sub(operations.len()),
        operations,
    }
}

fn op_matches_module(kind: &str, module: CopilotModule) -> bool {
    match module {
        CopilotModule::Sheet => kind.starts_with("sheet."),
        CopilotModule::Doc => kind.starts_with("doc."),
        CopilotModule::Hwp => {
            matches!(
                kind,
                "doc.append" | "doc.replace_paragraphs" | "doc.replace_span"
            )
        }
        CopilotModule::Slide => kind.starts_with("slide."),
        CopilotModule::Base => kind.starts_with("base."),
        CopilotModule::Board => kind.starts_with("board."),
        CopilotModule::Pdf => false,
    }
}

fn strip_simple_inline_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for ch in text.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

/// 선택 영역 명령의 결과는 선택한 문단(칸)을 바꾸는 작업 하나만 받는다.
/// 선택 밖을 건드리는 작업은 일부만 살리지 않고 통째로 버린다.
fn restrict_to_command(
    req: &CopilotRunRequest,
    accepted: Vec<CopilotOperation>,
) -> Vec<CopilotOperation> {
    let sel = &req.selection;
    let kept = match req.module {
        CopilotModule::Doc | CopilotModule::Hwp => {
            let start = sel.paragraph_index.unwrap_or(0);
            let end = sel.paragraph_end.unwrap_or(start);
            let allow_range = req.command == Some(CopilotCommand::VisualizeTable)
                && req.module == CopilotModule::Doc;
            accepted.into_iter().find(|op| match op {
                CopilotOperation::DocReplaceParagraphs { replacements } => replacements
                    .iter()
                    .all(|r| (start..=end).contains(&r.index)),
                CopilotOperation::DocReplaceSpan { index, .. }
                    if req.command != Some(CopilotCommand::VisualizeTable) =>
                {
                    (start..=end).contains(index)
                }
                CopilotOperation::DocReplaceRange {
                    start_index: Some(s),
                    end_index: Some(e),
                    blocks,
                } if allow_range => *s >= start && *e <= end && *s <= *e && !blocks.is_empty(),
                _ => false,
            })
        }
        CopilotModule::Slide => accepted.into_iter().find_map(|op| match op {
            CopilotOperation::SlideUpdate {
                index,
                title,
                bullets,
                ..
            } if index == sel.slide_index => match sel.slide_field {
                Some(SlideField::Title) => title.map(|title| CopilotOperation::SlideUpdate {
                    index,
                    title: Some(title),
                    bullets: None,
                    notes: None,
                    layout: None,
                    transition: None,
                    elements: None,
                }),
                _ => bullets.map(|bullets| CopilotOperation::SlideUpdate {
                    index,
                    title: None,
                    bullets: Some(bullets),
                    notes: None,
                    layout: None,
                    transition: None,
                    elements: None,
                }),
            },
            _ => None,
        }),
        _ => None,
    };
    kept.into_iter().collect()
}

fn opt_num_fmt(obj: &[(String, StrictJsonValue)]) -> Option<String> {
    StrictJsonValue::get_str(obj, "numFmt")
        .filter(|f| !f.is_empty() && f.len() <= 64 && !f.chars().any(char::is_control))
        .map(ToString::to_string)
}

fn opt_str_list(obj: &[(String, StrictJsonValue)], key: &str) -> Option<Vec<String>> {
    StrictJsonValue::get_field(obj, key)?.as_array().map(|arr| {
        arr.iter()
            .filter_map(StrictJsonValue::as_str)
            .map(ToString::to_string)
            .collect()
    })
}

fn parse_slide_bullets_or_body(obj: &[(String, StrictJsonValue)]) -> Option<Option<Vec<String>>> {
    match StrictJsonValue::get_field(obj, "bullets") {
        Some(StrictJsonValue::Null) => None,
        Some(v) => {
            let arr = v.as_array()?;
            if arr.len() > MAX_SLIDE_BULLETS {
                return None;
            }
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                out.push(item.as_str()?.to_string());
            }
            Some(Some(out))
        }
        None => match StrictJsonValue::get_field(obj, "body") {
            None | Some(StrictJsonValue::Null) => Some(None),
            Some(v) => {
                let s = v.as_str()?;
                let lines: Vec<String> = s
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .take(MAX_SLIDE_BULLETS)
                    .map(ToString::to_string)
                    .collect();
                Some(Some(lines))
            }
        },
    }
}

fn parse_doc_blocks(raw_blocks: &[StrictJsonValue], allow_tables: bool) -> Option<Vec<DocBlock>> {
    if raw_blocks.is_empty() || raw_blocks.len() > MAX_DOC_APPEND_BLOCKS {
        return None;
    }
    let mut blocks = Vec::with_capacity(raw_blocks.len());
    for b_val in raw_blocks {
        if let Some(s) = b_val.as_str() {
            blocks.push(DocBlock::Paragraph {
                text: s.to_string(),
            });
            continue;
        }
        let b_obj = b_val.as_object()?;
        let table_spec = StrictJsonValue::get_field(b_obj, "table")
            .and_then(StrictJsonValue::as_object)
            .or_else(|| {
                (StrictJsonValue::get_str(b_obj, "kind") == Some("table")
                    || StrictJsonValue::get_field(b_obj, "rows").is_some())
                .then_some(b_obj)
            });
        if let Some(t_obj) = table_spec {
            if !allow_tables {
                return None;
            }
            let raw_rows = StrictJsonValue::get_field(t_obj, "rows")?.as_array()?;
            if raw_rows.is_empty() || raw_rows.len() > MAX_DOC_TABLE_ROWS {
                return None;
            }
            let mut rows = Vec::with_capacity(raw_rows.len());
            for r_val in raw_rows {
                let cells = r_val.as_array()?;
                if cells.is_empty() || cells.len() > MAX_DOC_TABLE_COLS {
                    return None;
                }
                let mut row = Vec::with_capacity(cells.len());
                for cell in cells {
                    let s = cell.to_scalar_string()?;
                    if s.len() > 8_192 {
                        return None;
                    }
                    row.push(s);
                }
                rows.push(row);
            }
            let header_row = StrictJsonValue::get_field(t_obj, "headerRow")
                .and_then(StrictJsonValue::as_bool)
                .unwrap_or(true);
            blocks.push(DocBlock::Table { rows, header_row });
        } else if let Some(list_kind) = StrictJsonValue::get_str(b_obj, "list") {
            if !matches!(list_kind, "bullet" | "ordered" | "task") {
                return None;
            }
            blocks.push(DocBlock::List {
                kind: list_kind.to_string(),
                items: opt_str_list(b_obj, "items")?,
            });
        } else if let Some(lvl) =
            StrictJsonValue::get_field(b_obj, "heading").and_then(StrictJsonValue::as_u64)
        {
            let level = u8::try_from(lvl.clamp(1, 6)).ok()?;
            blocks.push(DocBlock::Heading {
                level,
                text: StrictJsonValue::get_str(b_obj, "text")?.to_string(),
            });
        } else {
            blocks.push(DocBlock::Paragraph {
                text: StrictJsonValue::get_str(b_obj, "text")?.to_string(),
            });
        }
    }
    Some(blocks)
}

fn validate_single_operation(
    req: &CopilotRunRequest,
    obj: &[(String, StrictJsonValue)],
    known_slides: Option<usize>,
) -> Option<CopilotOperation> {
    let kind = StrictJsonValue::get_str(obj, "kind")
        .or_else(|| StrictJsonValue::get_str(obj, "op"))?
        .trim();
    if !op_matches_module(kind, req.module) {
        return None;
    }
    let default_sheet = req
        .context
        .sheet
        .as_ref()
        .map(|s| s.name.as_str())
        .unwrap_or("Sheet1");
    match kind {
        "sheet.write" => {
            let sheet = StrictJsonValue::get_str(obj, "sheet")
                .unwrap_or(default_sheet)
                .to_string();
            let raw_cells = StrictJsonValue::get_field(obj, "cells")?.as_array()?;
            if raw_cells.is_empty() || raw_cells.len() > MAX_SHEET_WRITE_CELLS {
                return None;
            }
            let mut cells = Vec::with_capacity(raw_cells.len());
            for c_val in raw_cells {
                let c_obj = c_val.as_object()?;
                let coord = StrictJsonValue::get_str(c_obj, "coord")?.trim();
                if !is_valid_a1_coord(coord) {
                    return None;
                }
                let raw = StrictJsonValue::get_field(c_obj, "raw")
                    .and_then(StrictJsonValue::to_scalar_string)
                    .unwrap_or_default();
                cells.push(SheetWriteCell {
                    coord: coord.to_string(),
                    raw,
                    num_fmt: opt_num_fmt(c_obj),
                    bold: StrictJsonValue::get_field(c_obj, "bold")
                        .and_then(StrictJsonValue::as_bool),
                    fill: opt_hex(c_obj, "fill"),
                    color: opt_hex(c_obj, "color"),
                });
            }
            let range = match StrictJsonValue::get_str(obj, "range") {
                Some(r) => {
                    let t = r.trim();
                    if !is_valid_range_ref(t) {
                        return None;
                    }
                    t.to_string()
                }
                None => cells[0].coord.clone(),
            };
            Some(CopilotOperation::SheetWrite {
                sheet,
                range,
                cells,
            })
        }
        "sheet.format" => {
            let range = StrictJsonValue::get_str(obj, "range")?.trim();
            if !is_valid_range_ref(range) {
                return None;
            }
            let sheet = StrictJsonValue::get_str(obj, "sheet")
                .unwrap_or(default_sheet)
                .to_string();
            Some(CopilotOperation::SheetFormat {
                sheet,
                range: range.to_string(),
                fill: opt_hex(obj, "fill"),
                color: opt_hex(obj, "color"),
                bold: StrictJsonValue::get_field(obj, "bold").and_then(StrictJsonValue::as_bool),
                num_fmt: opt_num_fmt(obj),
            })
        }
        "sheet.chart" => validate_sheet_chart(obj, default_sheet),
        "sheet.table" | "sheet.pivot" | "sheet.cond_format" | "sheet.sort_filter"
        | "sheet.structure" | "sheet.note" => validate_extended_sheet_op(kind, obj, default_sheet),
        "doc.append" => {
            let allow_tables = req.module == CopilotModule::Doc;
            if let Some(raw_blocks) =
                StrictJsonValue::get_field(obj, "blocks").and_then(StrictJsonValue::as_array)
            {
                let blocks = parse_doc_blocks(raw_blocks, allow_tables)?;
                return Some(CopilotOperation::DocAppend { blocks });
            }
            let text = StrictJsonValue::get_str(obj, "text")?;
            Some(CopilotOperation::DocAppend {
                blocks: vec![DocBlock::Paragraph {
                    text: text.to_string(),
                }],
            })
        }
        "doc.replace_paragraphs" => {
            let max_para = req
                .context
                .doc
                .as_ref()
                .map(|d| d.paragraphs.len() + d.omitted_paragraphs)
                .unwrap_or(usize::MAX);
            if let Some(raw_blocks) =
                StrictJsonValue::get_field(obj, "blocks").and_then(StrictJsonValue::as_array)
            {
                let blocks = parse_doc_blocks(raw_blocks, req.module == CopilotModule::Doc)?;
                let start_index = opt_usize(obj, "startIndex");
                let end_index = opt_usize(obj, "endIndex");
                match (start_index, end_index) {
                    (Some(s), Some(e)) => {
                        if s > e || e >= max_para {
                            return None;
                        }
                        return Some(CopilotOperation::DocReplaceRange {
                            start_index: Some(s),
                            end_index: Some(e),
                            blocks,
                        });
                    }
                    (Some(s), None) => {
                        if s >= max_para {
                            return None;
                        }
                        return Some(CopilotOperation::DocReplaceRange {
                            start_index: Some(s),
                            end_index: Some(s),
                            blocks,
                        });
                    }
                    (None, None) => {
                        return Some(CopilotOperation::DocReplaceRange {
                            start_index: None,
                            end_index: None,
                            blocks,
                        });
                    }
                    (None, Some(_)) => return None,
                }
            }
            let arr = StrictJsonValue::get_field(obj, "replacements")?.as_array()?;
            let mut replacements = Vec::new();
            for item in arr {
                let r_obj = item.as_object()?;
                let index = opt_usize(r_obj, "index")?;
                if index >= max_para {
                    return None;
                }
                replacements.push(DocReplacement {
                    index,
                    text: StrictJsonValue::get_str(r_obj, "text")?.to_string(),
                });
            }
            (!replacements.is_empty())
                .then_some(CopilotOperation::DocReplaceParagraphs { replacements })
        }
        "doc.replace_span" => {
            let max_para = req
                .context
                .doc
                .as_ref()
                .map(|d| d.paragraphs.len() + d.omitted_paragraphs)
                .unwrap_or(usize::MAX);
            let index = opt_usize(obj, "index")?;
            if index >= max_para {
                return None;
            }
            let search = StrictJsonValue::get_str(obj, "search")?;
            let replace = StrictJsonValue::get_str(obj, "replace")?;
            if search.trim().is_empty()
                || search.len() > 8_192
                || replace.len() > MAX_PROMPT_CONTEXT_BYTES
            {
                return None;
            }
            if let Some(Some(para_text)) = req
                .context
                .doc
                .as_ref()
                .and_then(|d| d.paragraphs.get(index))
            {
                let raw_hits = para_text.match_indices(search).count();
                if raw_hits > 1 {
                    return None;
                }
                if raw_hits == 0 {
                    let plain = strip_simple_inline_tags(para_text);
                    if plain.match_indices(search).count() != 1 {
                        return None;
                    }
                }
            }
            Some(CopilotOperation::DocReplaceSpan {
                index,
                search: search.to_string(),
                replace: replace.to_string(),
            })
        }
        "doc.scrub_pii" => Some(CopilotOperation::DocScrubPii),
        "slide.add" => {
            if known_slides.is_some_and(|n| n >= MAX_SLIDES) {
                return None;
            }
            let layout = parse_slide_layout(obj)?;
            let transition = parse_slide_transition(obj)?;
            let elements = parse_slide_elements_opt(obj)?;
            let bullets = parse_slide_bullets_or_body(obj)?.unwrap_or_default();
            let title = match StrictJsonValue::get_str(obj, "title") {
                Some(t) => t.to_string(),
                None if layout.is_some() || elements.is_some() || !bullets.is_empty() => {
                    String::new()
                }
                None => return None,
            };
            let notes = StrictJsonValue::get_str(obj, "notes")
                .unwrap_or("")
                .to_string();
            Some(CopilotOperation::SlideAdd {
                title,
                bullets,
                notes,
                layout,
                transition,
                elements,
            })
        }
        "slide.update" => {
            let index = match StrictJsonValue::get_field(obj, "index") {
                None | Some(StrictJsonValue::Null) => None,
                Some(v) => {
                    let idx = v.as_u64().and_then(|n| usize::try_from(n).ok())?;
                    if known_slides.is_some_and(|slides| idx >= slides) {
                        return None;
                    }
                    Some(idx)
                }
            };
            let title = StrictJsonValue::get_str(obj, "title").map(ToString::to_string);
            let bullets = parse_slide_bullets_or_body(obj)?;
            let notes = StrictJsonValue::get_str(obj, "notes").map(ToString::to_string);
            let layout = parse_slide_layout(obj)?;
            let transition = parse_slide_transition(obj)?;
            let elements = parse_slide_elements_opt(obj)?;
            if title.is_none()
                && bullets.is_none()
                && notes.is_none()
                && layout.is_none()
                && transition.is_none()
                && elements.is_none()
            {
                return None;
            }
            Some(CopilotOperation::SlideUpdate {
                index,
                title,
                bullets,
                notes,
                layout,
                transition,
                elements,
            })
        }
        "slide.add_element" => {
            let slide_index = match StrictJsonValue::get_field(obj, "slideIndex")
                .or_else(|| StrictJsonValue::get_field(obj, "index"))
            {
                None | Some(StrictJsonValue::Null) => None,
                Some(v) => {
                    let idx = v.as_u64().and_then(|n| usize::try_from(n).ok())?;
                    if known_slides.is_some_and(|slides| idx >= slides) {
                        return None;
                    }
                    Some(idx)
                }
            };
            let el_obj = StrictJsonValue::get_field(obj, "element")?.as_object()?;
            let element = Box::new(parse_slide_element(el_obj)?);
            Some(CopilotOperation::SlideAddElement {
                slide_index,
                element,
            })
        }
        "base.add_field"
        | "base.add_records"
        | "base.update_records"
        | "base.set_view"
        | "board.add_graph" => validate_base_or_board_op(kind, obj),
        _ => None,
    }
}

pub fn validate_locations(items: &[StrictJsonValue]) -> Vec<CopilotLocation> {
    let mut out = Vec::new();
    for item in items.iter().take(MAX_LLM_LOCATIONS) {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let Some(module) = StrictJsonValue::get_str(obj, "module").filter(|m| {
            matches!(
                *m,
                "SHEET" | "DOC" | "HWP" | "SLIDE" | "PDF" | "BASE" | "BOARD"
            )
        }) else {
            continue;
        };
        let Some(label) = StrictJsonValue::get_str(obj, "label").filter(|l| !l.trim().is_empty())
        else {
            continue;
        };
        out.push(CopilotLocation {
            module: module.to_string(),
            label: label.to_string(),
            sheet: StrictJsonValue::get_str(obj, "sheet").map(ToString::to_string),
            range: StrictJsonValue::get_str(obj, "range")
                .filter(|r| is_valid_range_ref(r))
                .map(ToString::to_string),
            paragraph_index: opt_usize(obj, "paragraphIndex"),
            slide_index: opt_usize(obj, "slideIndex"),
            page: opt_usize(obj, "page").filter(|p| *p >= 1 && *p <= 10_000),
        });
    }
    out
}

/// `Some(None)`: 없음, `Some(Some(n))`: 범위 안의 값, `None`: 있지만 범위 밖이거나 정수가 아님.
fn opt_dimension(
    obj: &[(String, StrictJsonValue)],
    key: &str,
    min: u64,
    max: u64,
) -> Option<Option<u32>> {
    match StrictJsonValue::get_field(obj, key) {
        None | Some(StrictJsonValue::Null) => Some(None),
        Some(v) => v
            .as_u64()
            .filter(|n| (min..=max).contains(n))
            .and_then(|n| u32::try_from(n).ok())
            .map(Some),
    }
}

fn validate_sheet_chart(
    obj: &[(String, StrictJsonValue)],
    default_sheet: &str,
) -> Option<CopilotOperation> {
    let data_range = StrictJsonValue::get_str(obj, "dataRange")
        .or_else(|| StrictJsonValue::get_str(obj, "range"))?
        .trim();
    let range = parse_bounded_range(data_range)?;
    if range.cell_count() < 2 {
        return None;
    }
    let chart_type =
        ChartType::parse(StrictJsonValue::get_str(obj, "chartType").unwrap_or("column"))?;
    let legend = match StrictJsonValue::get_str(obj, "legend") {
        None => ChartLegend::Right,
        Some(raw) => ChartLegend::parse(raw)?,
    };
    let switch_row_col = StrictJsonValue::get_field(obj, "switchRowCol")
        .and_then(StrictJsonValue::as_bool)
        == Some(true);
    let (rows, cols) = range.dimensions();
    if (if switch_row_col { rows } else { cols }) > MAX_CHART_SERIES + 1 {
        return None;
    }
    let title = StrictJsonValue::get_str(obj, "title").unwrap_or("").trim();
    if title.chars().count() > MAX_CHART_TITLE_CHARS || title.chars().any(char::is_control) {
        return None;
    }
    Some(CopilotOperation::SheetChart {
        sheet: StrictJsonValue::get_str(obj, "sheet")
            .unwrap_or(default_sheet)
            .to_string(),
        chart_type,
        data_range: data_range.to_string(),
        title: title.to_string(),
        legend,
        switch_row_col,
        width: opt_dimension(obj, "width", MIN_CHART_WIDTH, MAX_CHART_WIDTH)?,
        height: opt_dimension(obj, "height", MIN_CHART_HEIGHT, MAX_CHART_HEIGHT)?,
    })
}
