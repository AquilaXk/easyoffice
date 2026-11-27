//! 참조 구현: dream-num/univer-office-copilot (Apache-2.0, Copyright DreamNum Co., Ltd.) 및 continuedev/continue (Apache-2.0, Copyright Continue Dev, Inc., see THIRD_PARTY_NOTICES.md)
//!
//! 코파일럿 요청의 문서 문맥(`context`)과 선택(`selection`) 해석. 모든 배열·문자열은 상한을 두고,
//! 넘으면 잘라 쓰지 않고 요청 전체를 거부한다.

use easyoffice_formats::{EasyOfficeError, StrictJsonValue};

use super::types::{
    opt_scalar_string, opt_str, MAX_BASE_FIELDS, MAX_BASE_RECORDS, MAX_BASE_TABLES,
    MAX_BASE_TEXT_BYTES, MAX_BOARD_CONNECTORS, MAX_BOARD_NODES, MAX_BOARD_TEXT_BYTES,
    MAX_DOC_PARAGRAPHS, MAX_SHEET_CELLS, MAX_SLIDES, MAX_SLIDE_BULLETS, MAX_SLIDE_ELEMENTS,
    MAX_TEXT_CONTEXT_BYTES,
};

/// 선택 영역이 가리키는 슬라이드 칸.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlideField {
    Title,
    Body,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectionContext {
    pub sheet: Option<String>,
    pub range: Option<String>,
    pub paragraph_index: Option<usize>,
    /// 선택이 걸친 마지막 문단(포함). 없으면 `paragraph_index` 한 문단이다.
    pub paragraph_end: Option<usize>,
    pub slide_index: Option<usize>,
    pub slide_field: Option<SlideField>,
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
    pub sheet_tabs: Vec<String>,
    pub used_range: Option<String>,
    pub total_cells: Option<usize>,
}

impl Default for SheetContext {
    fn default() -> Self {
        Self {
            name: "Sheet1".to_string(),
            cells: Vec::new(),
            sheet_tabs: Vec::new(),
            used_range: None,
            total_cells: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocContext {
    /// `Some("")`는 내용이 실제로 빈 문단, `None`은 창/절단으로 아직 싣지 않은 희소 슬롯이다.
    pub paragraphs: Vec<Option<String>>,
    pub selected_text: String,
    /// 문서에는 있지만 이 요청에 싣지 못한 문단 수(클라이언트가 알린다).
    pub omitted_paragraphs: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlideElementSummary {
    pub id: String,
    pub kind: String,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlideItem {
    pub title: String,
    pub bullets: Vec<String>,
    pub notes: String,
    pub layout: String,
    pub elements: Vec<SlideElementSummary>,
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
    pub active_page: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseField {
    pub name: String,
    pub kind: String,
}

/// BASE 테이블 하나: 필드 이름·유형과 앞쪽 레코드(필드 순서대로 표시 문자열).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BaseContext {
    pub tables: Vec<String>,
    pub table: String,
    pub fields: Vec<BaseField>,
    pub records: Vec<Vec<String>>,
    pub total_records: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardNode {
    pub id: String,
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BoardContext {
    pub nodes: Vec<BoardNode>,
    /// 연결선의 (시작 요소 ID, 끝 요소 ID).
    pub connectors: Vec<(String, String)>,
    pub total_nodes: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CopilotContext {
    pub sheet: Option<SheetContext>,
    pub doc: Option<DocContext>,
    pub slide: Option<SlideContext>,
    pub pdf: Option<PdfContext>,
    pub base: Option<BaseContext>,
    pub board: Option<BoardContext>,
}

fn bounded_text<'a>(
    obj: &'a [(String, StrictJsonValue)],
    key: &str,
    limit: usize,
    what: &str,
) -> Result<&'a str, EasyOfficeError> {
    let text = opt_str(obj, key)?.unwrap_or("");
    if text.len() > limit {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "{what} 길이가 상한({limit} bytes)을 초과했습니다."
        )));
    }
    Ok(text)
}

fn bounded_array<'a>(
    obj: &'a [(String, StrictJsonValue)],
    key: &str,
    limit: usize,
    what: &str,
) -> Result<&'a [StrictJsonValue], EasyOfficeError> {
    match StrictJsonValue::get_field(obj, key) {
        None | Some(StrictJsonValue::Null) => Ok(&[]),
        Some(v) => {
            let items = v.as_array().ok_or_else(|| {
                EasyOfficeError::InvalidArgument(format!("'{what}'는 배열이어야 합니다."))
            })?;
            if items.len() > limit {
                return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                    "{what} 수({})가 상한({limit}개)을 초과했습니다.",
                    items.len()
                )));
            }
            Ok(items)
        }
    }
}

fn opt_usize_field(obj: &[(String, StrictJsonValue)], key: &str) -> Option<usize> {
    StrictJsonValue::get_field(obj, key)
        .and_then(StrictJsonValue::as_u64)
        .and_then(|v| usize::try_from(v).ok())
}

fn nested_object<'a>(
    obj: &'a [(String, StrictJsonValue)],
    key: &str,
) -> Result<Option<&'a [(String, StrictJsonValue)]>, EasyOfficeError> {
    match StrictJsonValue::get_field(obj, key) {
        None | Some(StrictJsonValue::Null) => Ok(None),
        Some(v) => v.as_object().map(Some).ok_or_else(|| {
            EasyOfficeError::InvalidArgument(format!("'context.{key}'는 객체여야 합니다."))
        }),
    }
}

fn parse_base_context(obj: &[(String, StrictJsonValue)]) -> Result<BaseContext, EasyOfficeError> {
    let mut tables = Vec::new();
    for item in bounded_array(obj, "tables", MAX_BASE_TABLES, "context.base.tables")? {
        let name = item.as_str().ok_or_else(|| {
            EasyOfficeError::InvalidArgument("테이블 이름은 문자열이어야 합니다.".to_string())
        })?;
        if name.len() > MAX_BASE_TEXT_BYTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "테이블 이름이 상한을 초과했습니다.".to_string(),
            ));
        }
        tables.push(name.to_string());
    }
    let table = bounded_text(obj, "table", MAX_BASE_TEXT_BYTES, "테이블 이름")?.to_string();
    let mut fields = Vec::new();
    for item in bounded_array(obj, "fields", MAX_BASE_FIELDS, "context.base.fields")? {
        let f_obj = item.as_object().ok_or_else(|| {
            EasyOfficeError::InvalidArgument("BASE 필드 항목은 객체여야 합니다.".to_string())
        })?;
        fields.push(BaseField {
            name: bounded_text(f_obj, "name", MAX_BASE_TEXT_BYTES, "필드 이름")?.to_string(),
            kind: bounded_text(f_obj, "type", MAX_BASE_TEXT_BYTES, "필드 유형")?.to_string(),
        });
    }
    let mut records = Vec::new();
    for item in bounded_array(obj, "records", MAX_BASE_RECORDS, "context.base.records")? {
        let cells = item.as_array().ok_or_else(|| {
            EasyOfficeError::InvalidArgument("BASE 레코드는 배열이어야 합니다.".to_string())
        })?;
        if cells.len() > MAX_BASE_FIELDS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "BASE 레코드의 칸 수가 상한을 초과했습니다.".to_string(),
            ));
        }
        let mut row = Vec::with_capacity(cells.len());
        for cell in cells {
            let text = cell.to_scalar_string().ok_or_else(|| {
                EasyOfficeError::InvalidArgument("BASE 칸은 스칼라 값이어야 합니다.".to_string())
            })?;
            if text.len() > MAX_BASE_TEXT_BYTES {
                return Err(EasyOfficeError::ResourceBudgetExceeded(
                    "BASE 칸 문자열이 상한을 초과했습니다.".to_string(),
                ));
            }
            row.push(text);
        }
        records.push(row);
    }
    let total_records = opt_usize_field(obj, "totalRecords").unwrap_or(records.len());
    Ok(BaseContext {
        tables,
        table,
        fields,
        records,
        total_records,
    })
}

fn parse_board_context(obj: &[(String, StrictJsonValue)]) -> Result<BoardContext, EasyOfficeError> {
    let mut nodes = Vec::new();
    for item in bounded_array(obj, "nodes", MAX_BOARD_NODES, "context.board.nodes")? {
        let n_obj = item.as_object().ok_or_else(|| {
            EasyOfficeError::InvalidArgument("BOARD 요소는 객체여야 합니다.".to_string())
        })?;
        nodes.push(BoardNode {
            id: bounded_text(n_obj, "id", MAX_BOARD_TEXT_BYTES, "요소 ID")?.to_string(),
            kind: bounded_text(n_obj, "kind", MAX_BOARD_TEXT_BYTES, "요소 종류")?.to_string(),
            text: bounded_text(n_obj, "text", MAX_BOARD_TEXT_BYTES, "요소 글")?.to_string(),
        });
    }
    let mut connectors = Vec::new();
    for item in bounded_array(
        obj,
        "connectors",
        MAX_BOARD_CONNECTORS,
        "context.board.connectors",
    )? {
        let c_obj = item.as_object().ok_or_else(|| {
            EasyOfficeError::InvalidArgument("BOARD 연결선은 객체여야 합니다.".to_string())
        })?;
        connectors.push((
            bounded_text(c_obj, "from", MAX_BOARD_TEXT_BYTES, "연결선 시작")?.to_string(),
            bounded_text(c_obj, "to", MAX_BOARD_TEXT_BYTES, "연결선 끝")?.to_string(),
        ));
    }
    let total_nodes = opt_usize_field(obj, "totalNodes").unwrap_or(nodes.len());
    Ok(BoardContext {
        nodes,
        connectors,
        total_nodes,
    })
}

pub(super) fn parse_selection(
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
    let slide_field = match opt_str(obj, "slideField")?.map(str::trim) {
        None | Some("") => None,
        Some("title") => Some(SlideField::Title),
        Some("body") => Some(SlideField::Body),
        Some(_) => {
            return Err(EasyOfficeError::InvalidArgument(
                "'selection.slideField'는 title 또는 body여야 합니다.".to_string(),
            ))
        }
    };
    Ok(SelectionContext {
        sheet,
        range,
        paragraph_index: opt_usize_field(obj, "paragraphIndex"),
        paragraph_end: opt_usize_field(obj, "paragraphEnd"),
        slide_index: opt_usize_field(obj, "slideIndex"),
        slide_field,
    })
}

pub(super) fn parse_context(
    entries: &[(String, StrictJsonValue)],
) -> Result<CopilotContext, EasyOfficeError> {
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
            let mut sheet_tabs = Vec::new();
            for tab_val in bounded_array(
                s_obj,
                if StrictJsonValue::get_field(s_obj, "tabs").is_some() {
                    "tabs"
                } else {
                    "sheets"
                },
                MAX_BASE_TABLES,
                "context.sheet.tabs",
            )? {
                let s = tab_val.as_str().ok_or_else(|| {
                    EasyOfficeError::InvalidArgument(
                        "'context.sheet.tabs' 항목은 문자열이어야 합니다.".to_string(),
                    )
                })?;
                if s.len() > 128 {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "시트 탭 이름 길이가 상한(128 bytes)을 초과했습니다.".to_string(),
                    ));
                }
                if !s.trim().is_empty() {
                    sheet_tabs.push(s.trim().to_string());
                }
            }
            let used_range = opt_str(s_obj, "usedRange")?
                .map(str::trim)
                .filter(|s| !s.is_empty() && s.len() <= 64)
                .map(ToString::to_string);
            let total_cells = opt_usize_field(s_obj, "totalCells");
            Some(SheetContext {
                name,
                cells,
                sheet_tabs,
                used_range,
                total_cells,
            })
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
            let active_page = opt_usize_field(p_obj, "activePage").filter(|&p| p >= 1);
            Some(PdfContext {
                text,
                page_count,
                active_page,
            })
        }
    } else {
        None
    };

    let base = nested_object(obj, "base")?
        .map(parse_base_context)
        .transpose()?;
    let board = nested_object(obj, "board")?
        .map(parse_board_context)
        .transpose()?;

    Ok(CopilotContext {
        sheet,
        doc,
        slide,
        pdf,
        base,
        board,
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
                match item {
                    StrictJsonValue::Null => out.push(None),
                    StrictJsonValue::Str(s) => {
                        total_bytes = total_bytes.saturating_add(s.len());
                        if total_bytes > MAX_TEXT_CONTEXT_BYTES {
                            return Err(EasyOfficeError::ResourceBudgetExceeded(
                                "문서 문단 총 크기가 상한을 초과했습니다.".to_string(),
                            ));
                        }
                        out.push(Some(s.clone()));
                    }
                    _ => {
                        return Err(EasyOfficeError::InvalidArgument(
                            "'doc.paragraphs' 항목은 문자열 또는 null이어야 합니다.".to_string(),
                        ));
                    }
                }
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
        omitted_paragraphs: opt_usize_field(obj, "omittedParagraphs").unwrap_or(0),
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
                let layout = bounded_text(s_obj, "layout", 64, "슬라이드 레이아웃")?.to_string();
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
                let mut elements = Vec::new();
                for el_val in bounded_array(
                    s_obj,
                    "elements",
                    MAX_SLIDE_ELEMENTS,
                    "context.slide.elements",
                )? {
                    let el_obj = el_val.as_object().ok_or_else(|| {
                        EasyOfficeError::InvalidArgument(
                            "슬라이드 개체 항목은 객체여야 합니다.".to_string(),
                        )
                    })?;
                    let num_f64 = |key: &str| match StrictJsonValue::get_field(el_obj, key) {
                        Some(StrictJsonValue::Number(s)) => {
                            s.parse::<f64>().ok().filter(|v| v.is_finite())
                        }
                        _ => None,
                    };
                    let x = num_f64("x").map_or(0, |v| v.round().clamp(-10_000.0, 10_000.0) as i32);
                    let y = num_f64("y").map_or(0, |v| v.round().clamp(-10_000.0, 10_000.0) as i32);
                    let w = num_f64("w")
                        .filter(|v| *v >= 0.0)
                        .map_or(0, |v| v.round().clamp(0.0, 10_000.0) as u32);
                    let h = num_f64("h")
                        .filter(|v| *v >= 0.0)
                        .map_or(0, |v| v.round().clamp(0.0, 10_000.0) as u32);
                    elements.push(SlideElementSummary {
                        id: bounded_text(el_obj, "id", 128, "개체 ID")?.to_string(),
                        kind: bounded_text(el_obj, "kind", 64, "개체 종류")?.to_string(),
                        x,
                        y,
                        w,
                        h,
                        text: bounded_text(el_obj, "text", 2_048, "개체 텍스트")?.to_string(),
                    });
                }
                out.push(SlideItem {
                    title,
                    bullets,
                    notes,
                    layout,
                    elements,
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
