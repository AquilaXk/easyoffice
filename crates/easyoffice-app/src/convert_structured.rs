//! Structured save payloads for `convert_document_format` (PD-073).
//!
//! Instead of the legacy line markup in `body` (`[[TABLE_ROW]]`, `[[SLIDE]]`, …), the WebUI sends
//! one of:
//! - `blocks`: ordered `{"type":"paragraph","runs":[{"text","bold","italic","underline","strike",
//!   "sizeHalfPt","font","color","highlight","link"}],
//!   "align","heading","list"}` and `{"type":"table","rows":[[{"text","value"}]],
//!   "merges":[{"row","col","rowSpan","colSpan"}]}` items (`merges` is optional, PD-130);
//! - `sheets`: `[{"name","cells":[{"ref":"A1","raw":"=SUM(A1:A2)","style":{"bold":true,"sizePt":11.25,"nf":1}}],`n//!   "columns":[{"index":1,"widthPx":120}],"numFmts":["0.00"]}]` for every workbook sheet (`style`, `columns`
//!   and `numFmts` are optional, PD-089; `index` is the 1-based column number; `nf` is the 1-based entry
//!   of `numFmts`, PD-193);
//! - `slides`: `[{"title","bullets":[...],"notes"}]`.
//!
//! Every string is literal content; nothing in it is parsed as markup.

use std::collections::BTreeMap;

use easyoffice_formats::{
    base64_codec, code_within_bounds, evaluate_table_formulas, normalize_hex_color,
    safe_link_target, CellMerge, CellStyle, EasyOfficeError, NumFmtTable, OdsUnpacked, OoxmlCodec,
    PdfWriteOptions, RichAlignment, RichBlock, RichDocument, RichImage, RichListKind,
    RichParagraph, RichRun, RichScript, RichTable, RichToc, SheetStyling, StrictJsonValue,
    MAX_COLUMN_PX, MAX_IMAGE_BYTES, MAX_NUM_FMTS, MAX_SIZE_PT, MAX_TOC_LEVEL, MIN_COLUMN_PX,
    MIN_SIZE_PT,
};
use easyoffice_sheet::{PackedCellCoordinate, WorkbookEngine};

use crate::convert_structured_doc::parse_pdf_options;

/// Largest indent (twips, 1/20 pt) a paragraph may carry: 200 inches.
const MAX_INDENT_TWIPS: i64 = 288_000;
/// Largest line spacing, as a percentage of single spacing.
const MAX_LINE_SPACING_PERCENT: i64 = 1_000;

/// Upper bound on runs + table cells + sheet cells + slide items in one payload.
const MAX_STRUCTURED_ITEMS: usize = 500_000;
const MAX_SHEETS: usize = 256;
/// Upper bound on custom column widths of one sheet (the XLSX grid has 16,384 columns).
const MAX_COLUMN_ENTRIES: usize = 16_384;

pub(crate) type SlideTriple = (String, Vec<String>, String);

pub(crate) enum StructuredContent {
    /// A document and the page settings a PDF target writes with it.
    Document(RichDocument, PdfWriteOptions),
    /// Sheets plus their cell formatting and column widths (one SheetStyling per sheet).
    Workbook(Vec<OdsUnpacked>, Vec<SheetStyling>),
    Slides(Vec<SlideTriple>),
}

pub(crate) fn invalid(msg: impl Into<String>) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(msg.into())
}

pub(crate) type JsonObject = [(String, StrictJsonValue)];

pub(crate) fn field<'a>(obj: &'a JsonObject, key: &str) -> Option<&'a StrictJsonValue> {
    match StrictJsonValue::get_field(obj, key) {
        None | Some(StrictJsonValue::Null) => None,
        Some(v) => Some(v),
    }
}

pub(crate) fn opt_str<'a>(
    obj: &'a JsonObject,
    key: &str,
) -> Result<Option<&'a str>, EasyOfficeError> {
    field(obj, key)
        .map(|v| {
            v.as_str()
                .ok_or_else(|| invalid(format!("구조화 저장: '{key}'는 문자열이어야 합니다.")))
        })
        .transpose()
}

fn opt_bool(obj: &JsonObject, key: &str) -> Result<bool, EasyOfficeError> {
    field(obj, key).map_or(Ok(false), |v| {
        v.as_bool()
            .ok_or_else(|| invalid(format!("구조화 저장: '{key}'는 true/false여야 합니다.")))
    })
}

/// Run size in half-points (`w:sz` unit): an integer from 2 (1 pt) to 1638 (819 pt).
fn opt_size_half_pt(obj: &JsonObject) -> Result<Option<u16>, EasyOfficeError> {
    field(obj, "sizeHalfPt")
        .map(|v| {
            v.as_u64()
                .filter(|n| (2..=1638).contains(n))
                .map(|n| n as u16)
                .ok_or_else(|| invalid("구조화 저장: 'sizeHalfPt'는 2~1638 정수여야 합니다."))
        })
        .transpose()
}

/// Hyperlink address of a run: an `http`, `https` or `mailto` target or a `#_Toc<number>` bookmark (`safe_link_target`); anything else
/// rejects the save instead of being written.
fn opt_link(obj: &JsonObject) -> Result<Option<String>, EasyOfficeError> {
    match opt_str(obj, "link")?.filter(|l| !l.is_empty()) {
        None => Ok(None),
        Some(raw) => safe_link_target(raw)
            .map(|url| Some(url.to_string()))
            .ok_or_else(|| invalid("구조화 저장: 허용되지 않는 링크 주소입니다.")),
    }
}

/// Font family name: 1-64 characters without control characters.
fn opt_font(obj: &JsonObject) -> Result<Option<String>, EasyOfficeError> {
    match opt_str(obj, "font")?
        .map(str::trim)
        .filter(|f| !f.is_empty())
    {
        None => Ok(None),
        Some(f) if f.chars().count() <= 64 && !f.chars().any(char::is_control) => {
            Ok(Some(f.to_string()))
        }
        Some(_) => Err(invalid(
            "구조화 저장: 'font'는 제어 문자 없는 64자 이하 이름이어야 합니다.",
        )),
    }
}

/// `#RRGGBB` or `RRGGBB` color, stored as upper-case `RRGGBB`.
fn opt_color(obj: &JsonObject, key: &str) -> Result<Option<String>, EasyOfficeError> {
    match opt_str(obj, key)?.map(str::trim).filter(|c| !c.is_empty()) {
        None => Ok(None),
        Some(c) => normalize_hex_color(c)
            .map(Some)
            .ok_or_else(|| invalid(format!("구조화 저장: '{key}'는 #RRGGBB 색이어야 합니다."))),
    }
}

fn as_array<'a>(
    v: &'a StrictJsonValue,
    what: &str,
) -> Result<&'a [StrictJsonValue], EasyOfficeError> {
    v.as_array()
        .ok_or_else(|| invalid(format!("구조화 저장: '{what}'는 배열이어야 합니다.")))
}

fn as_object<'a>(v: &'a StrictJsonValue, what: &str) -> Result<&'a JsonObject, EasyOfficeError> {
    v.as_object()
        .ok_or_else(|| invalid(format!("구조화 저장: '{what}' 항목은 객체여야 합니다.")))
}

struct Budget(usize);

impl Budget {
    fn take(&mut self, n: usize) -> Result<(), EasyOfficeError> {
        self.0 = self.0.saturating_add(n);
        if self.0 > MAX_STRUCTURED_ITEMS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "구조화 저장 항목 수가 한도({MAX_STRUCTURED_ITEMS})를 초과했습니다."
            )));
        }
        Ok(())
    }
}

/// Structured content of a save payload plus what the parser had to drop from it.
pub(crate) type ParsedStructured = (StructuredContent, Vec<String>);

/// Returns the structured content (and warnings) if the payload carries `blocks`, `sheets`, or
/// `slides`.
pub(crate) fn parse_structured_content(
    entries: &JsonObject,
) -> Result<Option<ParsedStructured>, EasyOfficeError> {
    let mut budget = Budget(0);
    if let Some(v) = field(entries, "blocks") {
        let mut warnings = Vec::new();
        let blocks = parse_blocks(as_array(v, "blocks")?, &mut budget, &mut warnings)?;
        let doc = RichDocument {
            title: String::new(),
            blocks,
            official_agency: None,
        };
        let content = StructuredContent::Document(doc, parse_pdf_options(entries)?);
        return Ok(Some((content, warnings)));
    }
    if let Some(v) = field(entries, "sheets") {
        return parse_sheets(as_array(v, "sheets")?, &mut budget)
            .map(|(sheets, styling)| Some((StructuredContent::Workbook(sheets, styling), vec![])));
    }
    if let Some(v) = field(entries, "slides") {
        return parse_slides(as_array(v, "slides")?, &mut budget)
            .map(|s| Some((StructuredContent::Slides(s), vec![])));
    }
    Ok(None)
}

fn parse_paragraph(
    obj: &JsonObject,
    budget: &mut Budget,
) -> Result<RichParagraph, EasyOfficeError> {
    let mut runs = Vec::new();
    if let Some(v) = field(obj, "runs") {
        for item in as_array(v, "runs")? {
            budget.take(1)?;
            let run = as_object(item, "runs")?;
            let text = field(run, "text")
                .ok_or_else(|| invalid("구조화 저장: run에는 'text'가 필요합니다."))?
                .as_str()
                .ok_or_else(|| invalid("구조화 저장: run 'text'는 문자열이어야 합니다."))?;
            runs.push(RichRun {
                link: opt_link(run)?,
                text: text.to_string(),
                bold: opt_bool(run, "bold")?,
                italic: opt_bool(run, "italic")?,
                underline: opt_bool(run, "underline")?,
                strike: opt_bool(run, "strike")?,
                size_half_pt: opt_size_half_pt(run)?,
                font: opt_font(run)?,
                color: opt_color(run, "color")?,
                highlight: opt_color(run, "highlight")?,
                script: opt_script(run)?,
            });
        }
    }
    let alignment = opt_str(obj, "align")?
        .filter(|a| !a.trim().is_empty())
        .map(|a| {
            RichAlignment::parse(a)
                .ok_or_else(|| invalid(format!("구조화 저장: 알 수 없는 정렬 '{a}'")))
        })
        .transpose()?;
    let heading_level = match field(obj, "heading") {
        None => 0,
        Some(v) => v
            .as_u64()
            .filter(|n| *n <= 6)
            .ok_or_else(|| invalid("구조화 저장: 'heading'은 0~6 정수여야 합니다."))?
            as u8,
    };
    let toc = match field(obj, "toc") {
        None => None,
        Some(v) => Some(
            v.as_u64()
                .filter(|n| *n <= u64::from(MAX_TOC_LEVEL))
                .map(|n| {
                    if n == 0 {
                        RichToc::Title
                    } else {
                        RichToc::Entry(n as u8)
                    }
                })
                .ok_or_else(|| invalid("구조화 저장: 'toc'는 0~9 정수여야 합니다."))?,
        ),
    };
    let bookmark = match field(obj, "bookmark") {
        None => None,
        Some(v) => Some(
            v.as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| *n > 0)
                .ok_or_else(|| invalid("구조화 저장: 'bookmark'는 1 이상의 정수여야 합니다."))?,
        ),
    };
    let list = match opt_str(obj, "list")? {
        None | Some("") => None,
        Some("bullet") => Some(RichListKind::Bullet),
        Some("ordered") => Some(RichListKind::Ordered),
        Some(other) => return Err(invalid(format!("구조화 저장: 알 수 없는 목록 '{other}'"))),
    };
    Ok(RichParagraph {
        list_level: 0,
        list_number: None,
        runs,
        alignment,
        heading_level,
        list,
        left_indent_twips: opt_bounded(obj, "leftIndentTwips", 0, MAX_INDENT_TWIPS)?
            .map(|v| v as u32),
        first_line_indent_twips: opt_bounded(
            obj,
            "firstLineIndentTwips",
            -MAX_INDENT_TWIPS,
            MAX_INDENT_TWIPS,
        )?
        .map(|v| v as i32),
        line_spacing_percent: opt_bounded(obj, "lineSpacingPercent", 1, MAX_LINE_SPACING_PERCENT)?
            .map(|v| v as u16),
        toc,
        bookmark,
        page_break_before: opt_bool(obj, "pageBreakBefore")?,
    })
}

/// Optional integer field within `min..=max`.
pub(crate) fn opt_bounded(
    obj: &JsonObject,
    key: &str,
    min: i64,
    max: i64,
) -> Result<Option<i64>, EasyOfficeError> {
    field(obj, key)
        .map(|v| match v {
            StrictJsonValue::Number(text) => text
                .parse::<i64>()
                .ok()
                .filter(|n| (min..=max).contains(n))
                .ok_or_else(|| {
                    invalid(format!(
                        "구조화 저장: '{key}'는 {min}~{max} 정수여야 합니다."
                    ))
                }),
            _ => Err(invalid(format!("구조화 저장: '{key}'는 숫자여야 합니다."))),
        })
        .transpose()
}

/// Run position: `"superscript"`, `"subscript"` or absent.
fn opt_script(run: &JsonObject) -> Result<RichScript, EasyOfficeError> {
    match opt_str(run, "script")? {
        None | Some("") | Some("normal") => Ok(RichScript::Normal),
        Some("superscript") => Ok(RichScript::Superscript),
        Some("subscript") => Ok(RichScript::Subscript),
        Some(other) => Err(invalid(format!("구조화 저장: 알 수 없는 첨자 '{other}'"))),
    }
}

/// Picture block: base64 bytes (validated by [`RichImage::new`], so the PD-096 limits apply
/// again on every save), alternative text and an optional display size in EMU.
fn parse_image(obj: &JsonObject) -> Result<RichImage, EasyOfficeError> {
    let encoded = opt_str(obj, "base64")?
        .ok_or_else(|| invalid("구조화 저장: 그림에는 'base64'가 필요합니다."))?;
    if encoded.len() / 4 * 3 > MAX_IMAGE_BYTES + 3 {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "그림 크기가 한도({MAX_IMAGE_BYTES}바이트)를 넘었습니다."
        )));
    }
    let bytes = base64_codec::decode(encoded)
        .ok_or_else(|| invalid("구조화 저장: 그림 'base64'가 올바르지 않습니다."))?;
    let mut image = RichImage::new(bytes, opt_str(obj, "alt")?.unwrap_or(""))?;
    let width = opt_bounded(obj, "widthEmu", 1, i64::MAX)?;
    let height = opt_bounded(obj, "heightEmu", 1, i64::MAX)?;
    if let (Some(w), Some(h)) = (width, height) {
        image.set_display_emu(w as u64, h as u64)?;
    }
    Ok(image)
}

/// `{"row","col","rowSpan","colSpan"}`: non-negative integers. Values past `u32` saturate, so the
/// table's own validation drops them like any other region outside the grid.
fn parse_merge(item: &StrictJsonValue) -> Result<CellMerge, EasyOfficeError> {
    let obj = as_object(item, "merges")?;
    let number = |key: &str| {
        field(obj, key)
            .and_then(StrictJsonValue::as_u64)
            .map(|n| u32::try_from(n).unwrap_or(u32::MAX))
            .ok_or_else(|| {
                invalid(format!(
                    "구조화 저장: 병합의 '{key}'는 0 이상의 정수여야 합니다."
                ))
            })
    };
    Ok(CellMerge {
        row: number("row")?,
        col: number("col")?,
        row_span: number("rowSpan")?,
        col_span: number("colSpan")?,
    })
}

/// The page's `merges` are untrusted: the table re-validates them (one-cell, outside the grid,
/// overlapping) and the dropped ones are reported.
fn parse_table(
    obj: &JsonObject,
    budget: &mut Budget,
    warnings: &mut Vec<String>,
) -> Result<RichTable, EasyOfficeError> {
    let rows_v =
        field(obj, "rows").ok_or_else(|| invalid("구조화 저장: 표에는 'rows'가 필요합니다."))?;
    let mut rows = Vec::new();
    for row_v in as_array(rows_v, "rows")? {
        let mut row = Vec::new();
        for cell_v in as_array(row_v, "rows[]")? {
            budget.take(1)?;
            let text = match cell_v {
                StrictJsonValue::Str(s) => s.clone(),
                StrictJsonValue::Object(cell) => {
                    let text = opt_str(cell, "text")?.unwrap_or("");
                    // A computed value sent by the editor wins over the formula text.
                    match opt_str(cell, "value")?.filter(|v| !v.trim().is_empty()) {
                        Some(value) => value.to_string(),
                        None => text.to_string(),
                    }
                }
                _ => return Err(invalid("구조화 저장: 표 셀은 문자열 또는 객체여야 합니다.")),
            };
            row.push(text);
        }
        rows.push(row);
    }
    evaluate_table_formulas(&mut rows);
    let mut merges = Vec::new();
    if let Some(v) = field(obj, "merges") {
        for item in as_array(v, "merges")? {
            budget.take(1)?;
            merges.push(parse_merge(item)?);
        }
    }
    let (table, dropped) = RichTable::with_merges(rows, merges);
    if dropped > 0 {
        warnings.push(format!(
            "표의 병합 {dropped}곳은 범위를 벗어나거나 겹쳐 저장하지 않았습니다."
        ));
    }
    Ok(table)
}

fn parse_blocks(
    items: &[StrictJsonValue],
    budget: &mut Budget,
    warnings: &mut Vec<String>,
) -> Result<Vec<RichBlock>, EasyOfficeError> {
    let mut blocks = Vec::with_capacity(items.len());
    for item in items {
        budget.take(1)?;
        let obj = as_object(item, "blocks")?;
        match opt_str(obj, "type")? {
            Some("paragraph") => blocks.push(RichBlock::Paragraph(parse_paragraph(obj, budget)?)),
            Some("table") => blocks.push(RichBlock::Table(parse_table(obj, budget, warnings)?)),
            Some("image") => blocks.push(RichBlock::Image(parse_image(obj)?)),
            other => {
                return Err(invalid(format!(
                    "구조화 저장: 알 수 없는 블록 유형 {:?}",
                    other.unwrap_or("")
                )))
            }
        }
    }
    Ok(blocks)
}

/// Optional finite number field.
fn opt_number(obj: &JsonObject, key: &str) -> Result<Option<f64>, EasyOfficeError> {
    field(obj, key)
        .map(|v| match v {
            StrictJsonValue::Number(text) => text
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .ok_or_else(|| invalid(format!("구조화 저장: '{key}'는 유한한 수여야 합니다."))),
            _ => Err(invalid(format!("구조화 저장: '{key}'는 숫자여야 합니다."))),
        })
        .transpose()
}

/// Cell format of the sheet editor (PD-084/PD-089): `{"bold","italic","underline","strike",
/// "sizePt","nf"}`; `sizePt` is 1 to 409 points and is stored with two decimals; `nf` is the
/// 1-based entry of the sheet's `numFmts` (`num_fmt_ids` holds the format id of each entry).
fn parse_cell_style(obj: &JsonObject, num_fmt_ids: &[u16]) -> Result<CellStyle, EasyOfficeError> {
    let size_pt = opt_number(obj, "sizePt")?
        .map(|pt| {
            if (MIN_SIZE_PT..=MAX_SIZE_PT).contains(&pt) {
                Ok((pt * 100.0).round() / 100.0)
            } else {
                Err(invalid(format!(
                    "구조화 저장: 'sizePt'는 {MIN_SIZE_PT}~{MAX_SIZE_PT} 사이여야 합니다."
                )))
            }
        })
        .transpose()?;
    let num_fmt = match field(obj, "nf") {
        None => 0,
        Some(v) => v
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .filter(|&n| n <= num_fmt_ids.len())
            .map_or_else(
                || {
                    Err(invalid(
                        "구조화 저장: 'nf'가 'numFmts' 범위를 벗어났습니다.",
                    ))
                },
                |n| Ok(n.checked_sub(1).map_or(0, |at| num_fmt_ids[at])),
            )?,
    };
    Ok(CellStyle {
        bold: opt_bool(obj, "bold")?,
        italic: opt_bool(obj, "italic")?,
        underline: opt_bool(obj, "underline")?,
        strike: opt_bool(obj, "strike")?,
        size_pt,
        num_fmt,
    })
}

/// `numFmts`: the sheet's format codes (at most `MAX_NUM_FMTS`, each within the code bounds).
/// Returns the table and the format id of each entry.
fn parse_num_fmts(items: &[StrictJsonValue]) -> Result<(NumFmtTable, Vec<u16>), EasyOfficeError> {
    if items.len() > MAX_NUM_FMTS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "서식 코드 수가 한도({MAX_NUM_FMTS})를 초과했습니다."
        )));
    }
    let mut table = NumFmtTable::default();
    let mut ids = Vec::with_capacity(items.len());
    for item in items {
        let code = item
            .as_str()
            .filter(|code| code_within_bounds(code))
            .ok_or_else(|| invalid("구조화 저장: 서식 코드는 한도 안의 문자열이어야 합니다."))?;
        ids.push(table.intern(code));
    }
    Ok((table, ids))
}

/// `columns`: `[{"index": 1-based column, "widthPx": 8..=2000}]`, at most one entry per column
/// of the sheet grid.
fn parse_columns(
    items: &[StrictJsonValue],
    budget: &mut Budget,
) -> Result<BTreeMap<u32, u32>, EasyOfficeError> {
    if items.len() > MAX_COLUMN_ENTRIES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "열 너비 항목 수가 한도({MAX_COLUMN_ENTRIES})를 초과했습니다."
        )));
    }
    let mut columns = BTreeMap::new();
    for item in items {
        budget.take(1)?;
        let obj = as_object(item, "columns")?;
        let index = field(obj, "index")
            .and_then(StrictJsonValue::as_u64)
            .filter(|n| (1..=u64::from(OoxmlCodec::MAX_XLSX_COLS)).contains(n))
            .ok_or_else(|| invalid("구조화 저장: 열 'index'는 1~16384 정수여야 합니다."))?;
        let width = field(obj, "widthPx")
            .and_then(StrictJsonValue::as_u64)
            .filter(|n| (u64::from(MIN_COLUMN_PX)..=u64::from(MAX_COLUMN_PX)).contains(n))
            .ok_or_else(|| {
                invalid(format!(
                    "구조화 저장: 'widthPx'는 {MIN_COLUMN_PX}~{MAX_COLUMN_PX} 정수여야 합니다."
                ))
            })?;
        columns.insert(index as u32 - 1, width as u32);
    }
    Ok(columns)
}

type ParsedSheets = (Vec<OdsUnpacked>, Vec<SheetStyling>);

fn parse_sheets(
    items: &[StrictJsonValue],
    budget: &mut Budget,
) -> Result<ParsedSheets, EasyOfficeError> {
    if items.len() > MAX_SHEETS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "시트 수가 한도({MAX_SHEETS})를 초과했습니다."
        )));
    }
    let mut sheets: Vec<OdsUnpacked> = Vec::with_capacity(items.len());
    let mut stylings: Vec<SheetStyling> = Vec::with_capacity(items.len());
    for item in items {
        let obj = as_object(item, "sheets")?;
        let name = opt_str(obj, "name")?.unwrap_or("").trim().to_string();
        if name.is_empty() {
            return Err(invalid("구조화 저장: 시트 이름이 비어 있습니다."));
        }
        if sheets
            .iter()
            .any(|(n, _)| WorkbookEngine::is_same_sheet_identity(n, &name))
        {
            return Err(invalid(format!(
                "구조화 저장: 시트 이름이 중복됩니다: {name}"
            )));
        }
        let mut cells: BTreeMap<(u32, u32), String> = BTreeMap::new();
        let mut styling = SheetStyling::default();
        let (num_fmts, num_fmt_ids) = match field(obj, "numFmts") {
            Some(v) => parse_num_fmts(as_array(v, "numFmts")?)?,
            None => (NumFmtTable::default(), Vec::new()),
        };
        styling.num_fmts = num_fmts.finish().0;
        if let Some(v) = field(obj, "cells") {
            for cell_v in as_array(v, "cells")? {
                budget.take(1)?;
                let cell = as_object(cell_v, "cells")?;
                let a1 = opt_str(cell, "ref")?.unwrap_or("");
                let coord = PackedCellCoordinate::parse_a1(a1.trim())
                    .map_err(|_| invalid(format!("구조화 저장: 잘못된 셀 주소 '{a1}'")))?;
                let key = (coord.row(), coord.col());
                if let Some(style_v) = field(cell, "style") {
                    let style = parse_cell_style(as_object(style_v, "style")?, &num_fmt_ids)?;
                    if !style.is_plain() {
                        styling.cells.insert(key, style);
                    }
                }
                let raw = opt_str(cell, "raw")?.unwrap_or("");
                if !raw.is_empty() {
                    cells.insert(key, raw.to_string());
                }
            }
        }
        if let Some(v) = field(obj, "columns") {
            styling.columns = parse_columns(as_array(v, "columns")?, budget)?;
        }
        if let Some(v) = field(obj, "rules") {
            styling.rules = crate::sheet_rules_json::parse_rules(v)?;
        }
        crate::sheet_layout_json::parse_layout(obj, &mut styling)?;
        sheets.push((
            name,
            cells.into_iter().map(|((r, c), raw)| (r, c, raw)).collect(),
        ));
        stylings.push(styling);
    }
    Ok((sheets, stylings))
}
fn parse_slides(
    items: &[StrictJsonValue],
    budget: &mut Budget,
) -> Result<Vec<SlideTriple>, EasyOfficeError> {
    let mut slides = Vec::with_capacity(items.len());
    for item in items {
        budget.take(1)?;
        let obj = as_object(item, "slides")?;
        let mut bullets = Vec::new();
        if let Some(v) = field(obj, "bullets") {
            for b in as_array(v, "bullets")? {
                budget.take(1)?;
                let text = b
                    .as_str()
                    .ok_or_else(|| invalid("구조화 저장: 'bullets' 항목은 문자열이어야 합니다."))?;
                if !text.trim().is_empty() {
                    bullets.push(text.to_string());
                }
            }
        }
        slides.push((
            opt_str(obj, "title")?.unwrap_or("").to_string(),
            bullets,
            opt_str(obj, "notes")?.unwrap_or("").to_string(),
        ));
    }
    Ok(slides)
}

pub use crate::convert_structured_encode::RichPdfRenderer;
pub(crate) use crate::convert_structured_encode::{
    encode_source_target, encode_structured, encode_target,
};
