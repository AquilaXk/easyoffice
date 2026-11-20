//! 참조 구현: dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd.) 및 continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., see THIRD_PARTY_NOTICES.md)
//!
//! 모델이 돌려준 `operations`·`locations`를 검사한다. 형식·범위·개수가 어긋난 작업은 버리고,
//! 버린 개수를 응답에 실어 사용자에게 알린다.

use easyoffice_formats::StrictJsonValue;

use super::context::SlideField;
use super::types::{
    is_valid_a1_coord, is_valid_hex_color, is_valid_range_ref, parse_bounded_range, ChartLegend,
    ChartType, CopilotLocation, CopilotModule, CopilotOperation, CopilotRunRequest, DocBlock,
    DocReplacement, SheetWriteCell, MAX_CHART_HEIGHT, MAX_CHART_SERIES, MAX_CHART_TITLE_CHARS,
    MAX_CHART_WIDTH, MAX_DOC_APPEND_BLOCKS, MAX_LLM_LOCATIONS, MAX_LLM_OPERATIONS,
    MAX_SHEET_WRITE_CELLS, MIN_CHART_HEIGHT, MIN_CHART_WIDTH,
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
    let accepted: Vec<CopilotOperation> = items
        .iter()
        .take(MAX_LLM_OPERATIONS)
        .filter_map(|item| {
            item.as_object()
                .and_then(|o| validate_single_operation(req, o))
        })
        .collect();
    let operations = if req.command.is_some() {
        restrict_to_command(req, accepted)
    } else {
        accepted
    };
    ValidatedOperations {
        dropped: items.len() - operations.len(),
        operations,
    }
}

/// 선택 영역 명령의 결과는 선택한 문단(칸)을 바꾸는 작업 하나만 받는다.
/// 선택 밖을 건드리는 작업은 일부만 살리지 않고 통째로 버린다.
fn restrict_to_command(
    req: &CopilotRunRequest,
    accepted: Vec<CopilotOperation>,
) -> Vec<CopilotOperation> {
    let sel = &req.selection;
    let kept = match req.module {
        CopilotModule::Doc => {
            let start = sel.paragraph_index.unwrap_or(0);
            let end = sel.paragraph_end.unwrap_or(start);
            accepted.into_iter().find(|op| {
                matches!(op, CopilotOperation::DocReplaceParagraphs { replacements }
                    if replacements.iter().all(|r| (start..=end).contains(&r.index)))
            })
        }
        CopilotModule::Slide => accepted.into_iter().find_map(|op| match op {
            CopilotOperation::SlideUpdate {
                index,
                title,
                bullets,
                ..
            } if Some(index) == sel.slide_index => match sel.slide_field {
                Some(SlideField::Title) => title.map(|title| CopilotOperation::SlideUpdate {
                    index,
                    title: Some(title),
                    bullets: None,
                    notes: None,
                }),
                _ => bullets.map(|bullets| CopilotOperation::SlideUpdate {
                    index,
                    title: None,
                    bullets: Some(bullets),
                    notes: None,
                }),
            },
            _ => None,
        }),
        _ => None,
    };
    kept.into_iter().collect()
}

fn opt_hex(obj: &[(String, StrictJsonValue)], key: &str) -> Option<String> {
    StrictJsonValue::get_str(obj, key)
        .map(str::trim)
        .filter(|s| is_valid_hex_color(s))
        .map(ToString::to_string)
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

fn opt_usize(obj: &[(String, StrictJsonValue)], key: &str) -> Option<usize> {
    StrictJsonValue::get_field(obj, key)
        .and_then(StrictJsonValue::as_u64)
        .and_then(|v| usize::try_from(v).ok())
}

fn validate_single_operation(
    req: &CopilotRunRequest,
    obj: &[(String, StrictJsonValue)],
) -> Option<CopilotOperation> {
    let kind = StrictJsonValue::get_str(obj, "kind")?;
    let default_sheet = req
        .context
        .sheet
        .as_ref()
        .map(|s| s.name.as_str())
        .unwrap_or("Sheet1");
    match kind {
        "sheet.write" => {
            let range = StrictJsonValue::get_str(obj, "range")?.trim();
            if !is_valid_range_ref(range) {
                return None;
            }
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
            Some(CopilotOperation::SheetWrite {
                sheet,
                range: range.to_string(),
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
        "doc.append" => {
            let raw_blocks = StrictJsonValue::get_field(obj, "blocks")?.as_array()?;
            if raw_blocks.is_empty() || raw_blocks.len() > MAX_DOC_APPEND_BLOCKS {
                return None;
            }
            let mut blocks = Vec::with_capacity(raw_blocks.len());
            for b_val in raw_blocks {
                let b_obj = b_val.as_object()?;
                if let Some(list_kind) = StrictJsonValue::get_str(b_obj, "list") {
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
            Some(CopilotOperation::DocAppend { blocks })
        }
        "doc.replace_paragraphs" => {
            let arr = StrictJsonValue::get_field(obj, "replacements")?.as_array()?;
            let max_para = req
                .context
                .doc
                .as_ref()
                .map(|d| d.paragraphs.len())
                .unwrap_or(usize::MAX);
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
        "doc.scrub_pii" => Some(CopilotOperation::DocScrubPii),
        "slide.add" => Some(CopilotOperation::SlideAdd {
            title: StrictJsonValue::get_str(obj, "title")?.to_string(),
            bullets: opt_str_list(obj, "bullets").unwrap_or_default(),
            notes: StrictJsonValue::get_str(obj, "notes")
                .unwrap_or("")
                .to_string(),
        }),
        "slide.update" => {
            let index = opt_usize(obj, "index")?;
            if let Some(slides) = req.context.slide.as_ref().map(|s| s.slides.len()) {
                if index >= slides {
                    return None;
                }
            }
            Some(CopilotOperation::SlideUpdate {
                index,
                title: StrictJsonValue::get_str(obj, "title").map(ToString::to_string),
                bullets: opt_str_list(obj, "bullets"),
                notes: StrictJsonValue::get_str(obj, "notes").map(ToString::to_string),
            })
        }
        _ => None,
    }
}

pub fn validate_locations(items: &[StrictJsonValue]) -> Vec<CopilotLocation> {
    let mut out = Vec::new();
    for item in items.iter().take(MAX_LLM_LOCATIONS) {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let Some(module) = StrictJsonValue::get_str(obj, "module")
            .filter(|m| matches!(*m, "SHEET" | "DOC" | "SLIDE"))
        else {
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
    // 첫 열(행)은 범주 이름이고 나머지가 계열이다.
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
