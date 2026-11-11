//! Structured save payloads for `convert_document_format` (PD-073).
//!
//! Instead of the legacy line markup in `body` (`[[TABLE_ROW]]`, `[[SLIDE]]`, …), the WebUI sends
//! one of:
//! - `blocks`: ordered `{"type":"paragraph","runs":[{"text","bold","italic","underline","strike",
//!   "sizeHalfPt","font","color","highlight"}],
//!   "align","heading","list"}` and `{"type":"table","rows":[[{"text","value"}]],
//!   "merges":[{"row","col","rowSpan","colSpan"}]}` items (`merges` is optional, PD-130);
//! - `sheets`: `[{"name","cells":[{"ref":"A1","raw":"=SUM(A1:A2)","style":{"bold":true,"sizePt":11.25}}],`n//!   "columns":[{"index":1,"widthPx":120}]}]` for every workbook sheet (`style` and `columns` are optional,`n//!   PD-089; `index` is the 1-based column number);
//! - `slides`: `[{"title","bullets":[...],"notes"}]`.
//!
//! Every string is literal content; nothing in it is parsed as markup.

use std::collections::BTreeMap;

use easyoffice_formats::{
    base64_codec, evaluate_table_formulas, normalize_hex_color, pages_write_loss, CellMerge,
    CellStyle, CsvCodec, DocumentFormat, EasyOfficeError, LegacyOle2Codec, LegacyXlsPayload,
    OdfCodec, OdsUnpacked, OoxmlCodec, PdfWriteOptions, RichAlignment, RichBlock, RichDocument,
    RichImage, RichListKind, RichParagraph, RichRun, RichScript, RichTable, SheetStyling,
    StrictJsonValue, UniversalDocumentModel, UniversalFormatConverter, XlsxCachedValue,
    XlsxFormulaCache, MAX_COLUMN_PX, MAX_IMAGE_BYTES, MAX_SIZE_PT, MIN_COLUMN_PX, MIN_SIZE_PT,
};
use easyoffice_sheet::{
    EvaluatedCellValue, PackedCellCoordinate, WorkbookEngine, WorkbookEvalTarget,
};

use crate::convert_ipc::compute_xlsx_formula_cache;
use crate::convert_structured_doc::{is_document_format, parse_pdf_options, slides_to_document};

/// Largest indent (twips, 1/20 pt) a paragraph may carry: 200 inches.
const MAX_INDENT_TWIPS: i64 = 288_000;
/// Largest line spacing, as a percentage of single spacing.
const MAX_LINE_SPACING_PERCENT: i64 = 1_000;

/// Upper bound on runs + table cells + sheet cells + slide items in one payload.
const MAX_STRUCTURED_ITEMS: usize = 500_000;
const MAX_SHEETS: usize = 256;
/// Upper bound on custom column widths of one sheet (the XLSX grid has 16,384 columns).
const MAX_COLUMN_ENTRIES: usize = 16_384;
/// Upper bound on cells of the value grids rendered when a workbook is saved to a document format.
const MAX_GRID_CELLS: usize = CsvCodec::MAX_GRID_CELLS;

pub(crate) type SlideTriple = (String, Vec<String>, String);
/// Displayed values of one sheet as a rectangle over its used range.
type ValueGrid = Vec<Vec<String>>;
/// Displayed cells of one sheet as `(row, col, text)`.
type DisplayCells = Vec<(u32, u32, String)>;

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
                link: None,
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
/// "sizePt"}`; `sizePt` is 1 to 409 points and is stored with two decimals.
fn parse_cell_style(obj: &JsonObject) -> Result<CellStyle, EasyOfficeError> {
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
    Ok(CellStyle {
        bold: opt_bool(obj, "bold")?,
        italic: opt_bool(obj, "italic")?,
        underline: opt_bool(obj, "underline")?,
        strike: opt_bool(obj, "strike")?,
        size_pt,
    })
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
        if let Some(v) = field(obj, "cells") {
            for cell_v in as_array(v, "cells")? {
                budget.take(1)?;
                let cell = as_object(cell_v, "cells")?;
                let a1 = opt_str(cell, "ref")?.unwrap_or("");
                let coord = PackedCellCoordinate::parse_a1(a1.trim())
                    .map_err(|_| invalid(format!("구조화 저장: 잘못된 셀 주소 '{a1}'")))?;
                let key = (coord.row(), coord.col());
                if let Some(style_v) = field(cell, "style") {
                    let style = parse_cell_style(as_object(style_v, "style")?)?;
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

fn cached_value(v: &EvaluatedCellValue) -> Option<XlsxCachedValue> {
    match v {
        EvaluatedCellValue::Number(n) if n.is_finite() => Some(XlsxCachedValue::Number(*n)),
        EvaluatedCellValue::Text(s) => Some(XlsxCachedValue::Text(s.clone())),
        EvaluatedCellValue::Boolean(b) => Some(XlsxCachedValue::Boolean(*b)),
        _ => None,
    }
}

/// A formula engine holding every sheet, so cross-sheet references resolve.
fn workbook_engine(sheets: &[OdsUnpacked]) -> WorkbookEngine {
    let mut wb = WorkbookEngine::new();
    for (name, cells) in sheets {
        let sheet = wb.add_sheet(name.clone());
        for (row, col, raw) in cells {
            if let Ok(coord) = PackedCellCoordinate::new(*row, *col) {
                sheet.set_cell_coord(coord, raw.clone());
            }
        }
    }
    wb
}

/// Every sheet with each formula replaced by the value it displays, for the formats that hold
/// values only (a BIFF8 formula needs a compiled token stream, and CSV has no formulas). Also
/// returns how many formulas were replaced.
fn computed_sheets(sheets: &[OdsUnpacked]) -> (Vec<OdsUnpacked>, usize) {
    let wb = workbook_engine(sheets);
    let mut a1_pairs: Vec<(&str, String)> = Vec::new();
    for (name, cells) in sheets {
        for (row, col, raw) in cells {
            if raw.starts_with('=') {
                if let Ok(coord) = PackedCellCoordinate::new(*row, *col) {
                    a1_pairs.push((name.as_str(), coord.to_a1()));
                }
            }
        }
    }
    let formulas = a1_pairs.len();
    let targets: Vec<WorkbookEvalTarget<'_>> = a1_pairs
        .iter()
        .map(|(sheet, a1)| WorkbookEvalTarget {
            sheet,
            expr: a1.as_str(),
            anchor: None,
        })
        .collect();
    let mut eval_iter = wb.evaluate_targets(&targets).values.into_iter();

    let mut computed = Vec::with_capacity(sheets.len());
    for (name, cells) in sheets {
        let mut values = Vec::with_capacity(cells.len());
        for (row, col, raw) in cells {
            if !raw.starts_with('=') {
                values.push((*row, *col, raw.clone()));
                continue;
            }
            if PackedCellCoordinate::new(*row, *col).is_err() {
                continue;
            }
            let value = eval_iter
                .next()
                .unwrap_or(EvaluatedCellValue::Empty)
                .to_display_string();
            values.push((*row, *col, value));
        }
        computed.push((name.clone(), values));
    }
    (computed, formulas)
}

/// Longest list of sheet names a warning spells out.
const WARNED_SHEET_NAMES: usize = 5;

/// XLS: every sheet, formulas as values.
fn encode_xls(
    sheets: &[OdsUnpacked],
    title: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<u8>, EasyOfficeError> {
    let (computed, formulas) = computed_sheets(sheets);
    if formulas > 0 {
        warnings.push(format!(
            "수식 {formulas}개는 계산된 값으로 저장했습니다. XLS 파일에는 수식이 아니라 값이 들어갑니다."
        ));
    }
    let names: Vec<&str> = computed.iter().map(|(name, _)| name.as_str()).collect();
    let clean = LegacyOle2Codec::sanitize_sheet_names(&names);
    let renamed: Vec<String> = names
        .iter()
        .zip(&clean)
        .filter(|(old, new)| old != new)
        .map(|(old, new)| format!("'{old}' → '{new}'"))
        .collect();
    if !renamed.is_empty() {
        let shown = renamed.len().min(WARNED_SHEET_NAMES);
        let rest = match renamed.len() - shown {
            0 => String::new(),
            more => format!(" 외 {more}개"),
        };
        warnings.push(format!(
            "XLS 시트 이름 규칙(31자 이하, : \\ / ? * [ ] 금지, 중복 불가)에 맞춰 시트 {}개의 이름을 바꿨습니다: {}{rest}.",
            renamed.len(),
            renamed[..shown].join(", ")
        ));
    }
    let mut payloads: Vec<LegacyXlsPayload> = computed
        .into_iter()
        .map(|(sheet_name, cells)| LegacyXlsPayload { sheet_name, cells })
        .collect();
    if payloads.is_empty() {
        payloads.push(LegacyXlsPayload {
            sheet_name: title.to_string(),
            cells: Vec::new(),
        });
    }
    LegacyOle2Codec::pack_xls_multi_values(&payloads)
}

/// CSV/TSV hold one sheet: the first, as values. Sheets left out and cells outside what the CSV
/// reader accepts are reported, never dropped silently.
fn encode_delimited(
    sheets: &[OdsUnpacked],
    format: DocumentFormat,
    title: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<u8>, EasyOfficeError> {
    let label = format.extension().to_uppercase();
    let (computed, _) = computed_sheets(sheets);
    let mut computed = computed.into_iter();
    let (first_name, mut cells) = computed.next().unwrap_or_default();
    let left_out: Vec<String> = computed.map(|(name, _)| name).collect();
    if !left_out.is_empty() {
        let shown = left_out.len().min(WARNED_SHEET_NAMES);
        let more = left_out.len() - shown;
        let names = left_out[..shown].join(", ");
        let rest = if more > 0 {
            format!(" 외 {more}개")
        } else {
            String::new()
        };
        warnings.push(format!(
            "{label}는 시트 하나만 담을 수 있어 첫 시트 '{first_name}'만 저장했습니다. 나머지 시트 {}개({names}{rest})는 포함되지 않았습니다.",
            left_out.len()
        ));
    }
    let total = cells.len();
    cells.retain(|(row, col, _)| {
        (*row as usize) < CsvCodec::MAX_ROWS && (*col as usize) < CsvCodec::MAX_COLS
    });
    if cells.len() < total {
        warnings.push(format!(
            "{label}가 읽을 수 있는 범위({}행 x {}열)를 벗어난 셀 {}개는 제외했습니다.",
            CsvCodec::MAX_ROWS,
            CsvCodec::MAX_COLS,
            total - cells.len()
        ));
    }
    let model = UniversalDocumentModel {
        title: title.to_string(),
        sheet_cells: cells,
        ..UniversalDocumentModel::default()
    };
    UniversalFormatConverter::encode(&model, format)
}

/// Evaluates every formula cell across all sheets (cross-sheet references resolve) and returns
/// one formula cache per sheet plus, when `with_grids`, the displayed-value grid of each sheet.
fn evaluate_workbook(
    sheets: &[OdsUnpacked],
    with_grids: bool,
) -> Result<(Vec<XlsxFormulaCache>, Vec<ValueGrid>), EasyOfficeError> {
    let wb = workbook_engine(sheets);
    let mut caches = Vec::with_capacity(sheets.len());
    let mut grids = Vec::new();
    let mut grid_cells = 0usize;
    for (name, cells) in sheets {
        let (cache, displays) = evaluate_sheet(&wb, name, cells, with_grids)?;
        caches.push(cache);
        if with_grids {
            grids.push(rectangular_grid(&displays, &mut grid_cells)?);
        }
    }
    Ok((caches, grids))
}

/// Evaluates one sheet: the formula cache and (when `with_grids`) the displayed cells.
fn evaluate_sheet(
    wb: &WorkbookEngine,
    name: &str,
    cells: &[(u32, u32, String)],
    with_grids: bool,
) -> Result<(XlsxFormulaCache, DisplayCells), EasyOfficeError> {
    let a1_addrs: Vec<String> = cells
        .iter()
        .filter(|(_, _, raw)| raw.starts_with('='))
        .filter_map(|(r, c, _)| {
            PackedCellCoordinate::new(*r, *c)
                .ok()
                .map(|coord| coord.to_a1())
        })
        .collect();
    let targets: Vec<WorkbookEvalTarget<'_>> = a1_addrs
        .iter()
        .map(|a1| WorkbookEvalTarget {
            sheet: name,
            expr: a1.as_str(),
            anchor: None,
        })
        .collect();
    let mut eval_iter = wb.evaluate_targets(&targets).values.into_iter();

    let mut cache = XlsxFormulaCache::new();
    let mut displays = Vec::new();
    for (row, col, raw) in cells {
        let display = if raw.starts_with('=') {
            if PackedCellCoordinate::new(*row, *col).is_err() {
                continue;
            };
            let value = eval_iter.next().unwrap_or(EvaluatedCellValue::Empty);
            if let Some(c) = cached_value(&value) {
                cache.insert((*row, *col), c);
            }
            value.to_display_string()
        } else {
            raw.clone()
        };
        if with_grids {
            displays.push((*row, *col, display));
        }
    }
    Ok((cache, displays))
}

/// The displayed cells as a rectangle over their used range, so every row has the same width and
/// empty rows inside the range keep their place. The grid counts against the total budget before
/// it is allocated, so a sheet that is sparse over a huge range is refused, never cut.
fn rectangular_grid(
    displays: &[(u32, u32, String)],
    grid_cells: &mut usize,
) -> Result<ValueGrid, EasyOfficeError> {
    let Some(first) = displays.first() else {
        return Ok(Vec::new());
    };
    let (mut r0, mut r1, mut c0, mut c1) = (first.0, first.0, first.1, first.1);
    for &(row, col, _) in displays {
        r0 = r0.min(row);
        r1 = r1.max(row);
        c0 = c0.min(col);
        c1 = c1.max(col);
    }
    let (rows, cols) = ((r1 - r0) as usize + 1, (c1 - c0) as usize + 1);
    *grid_cells = grid_cells.saturating_add(rows.saturating_mul(cols));
    if *grid_cells > MAX_GRID_CELLS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(
            "시트 값 표가 너무 커서 문서 형식으로 저장할 수 없습니다.".to_string(),
        ));
    }
    let mut grid = vec![vec![String::new(); cols]; rows];
    for (row, col, display) in displays {
        grid[(row - r0) as usize][(col - c0) as usize].clone_from(display);
    }
    Ok(grid)
}

/// Draws a document's pages for a host that can: the document as HWPX and the page settings in,
/// the PDF out (PD-164).
pub type RichPdfRenderer<'a> = dyn Fn(&[u8], &PdfWriteOptions) -> Result<Vec<u8>, String> + 'a;

/// [`encode_structured`], except that a document converted to PDF is drawn by `renderer` when
/// there is one. A PDF studio save (`source` is PDF) keeps the engine's PDF, which writes the
/// page markers the studio reads back. When the renderer fails the engine PDF is written and
/// `warnings` says why.
pub(crate) fn encode_target(
    content: &StructuredContent,
    format: DocumentFormat,
    source: DocumentFormat,
    title: &str,
    renderer: Option<&RichPdfRenderer<'_>>,
    warnings: &mut Vec<String>,
) -> Result<Vec<u8>, EasyOfficeError> {
    if let (Some(render), StructuredContent::Document(doc, options), DocumentFormat::Pdf) =
        (renderer, content, format)
    {
        if source != DocumentFormat::Pdf {
            let doc = RichDocument {
                title: title.to_string(),
                ..doc.clone()
            };
            let rendered = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Hwpx, None)
                .map_err(|err| err.to_string())
                .and_then(|hwpx| render(&hwpx, options));
            match rendered {
                Ok(pdf) => return Ok(pdf),
                Err(reason) => warnings.push(format!(
                    "서식을 살린 PDF를 만들지 못해 기본 PDF(글자 중심)로 저장했습니다: {reason}"
                )),
            }
        }
    }
    encode_structured(content, format, title, warnings)
}

/// Encodes structured content to `format`, titling the output `title`. What the format cannot
/// hold of the content is added to `warnings`.
pub(crate) fn encode_structured(
    content: &StructuredContent,
    format: DocumentFormat,
    title: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<u8>, EasyOfficeError> {
    if format == DocumentFormat::Pages && !matches!(content, StructuredContent::Document(..)) {
        return Err(EasyOfficeError::InvalidArgument(
            "Pages(.pages)로는 문서 내용만 저장할 수 있습니다. 시트·슬라이드는 다른 형식으로 저장하세요."
                .to_string(),
        ));
    }
    match content {
        StructuredContent::Document(doc, pdf) => {
            let doc = RichDocument {
                title: title.to_string(),
                ..doc.clone()
            };
            if format == DocumentFormat::Pdf {
                let (bytes, notes) = UniversalFormatConverter::encode_rich_pdf(&doc, pdf);
                warnings.extend(notes);
                return Ok(bytes);
            }
            if format == DocumentFormat::Pages {
                warnings.extend(pages_write_loss(&doc).warnings());
            }
            let cache = matches!(format, DocumentFormat::Xlsx | DocumentFormat::Ods)
                .then(|| compute_xlsx_formula_cache(&doc.to_universal_model()));
            UniversalFormatConverter::encode_rich(&doc, format, cache.as_ref())
        }
        StructuredContent::Workbook(sheets, stylings) => match format {
            DocumentFormat::Xlsx => {
                let (caches, _) = evaluate_workbook(sheets, false)?;
                OoxmlCodec::pack_xlsx_multi_styled(sheets, &caches, stylings)
            }
            DocumentFormat::Ods => {
                let (caches, _) = evaluate_workbook(sheets, false)?;
                OdfCodec::pack_ods_multi_styled_with_cache(sheets, &caches, stylings)
            }
            DocumentFormat::Xls => encode_xls(sheets, title, warnings),
            DocumentFormat::Csv | DocumentFormat::Tsv => {
                encode_delimited(sheets, format, title, warnings)
            }
            _ => {
                let (_, grids) = evaluate_workbook(sheets, true)?;
                let mut blocks = Vec::with_capacity(sheets.len() * 2);
                for ((name, _), grid) in sheets.iter().zip(grids) {
                    blocks.push(RichBlock::Paragraph(RichParagraph {
                        runs: vec![RichRun::plain(name.clone())],
                        heading_level: 2,
                        ..RichParagraph::default()
                    }));
                    if !grid.is_empty() {
                        blocks.push(RichBlock::Table(grid.into()));
                    }
                }
                let doc = RichDocument {
                    title: title.to_string(),
                    blocks,
                    official_agency: None,
                };
                UniversalFormatConverter::encode_rich(&doc, format, None)
            }
        },
        StructuredContent::Slides(slides) if is_document_format(format) => {
            let doc = slides_to_document(slides, title);
            UniversalFormatConverter::encode_rich(&doc, format, None)
        }
        StructuredContent::Slides(slides) => {
            let model = UniversalDocumentModel {
                title: title.to_string(),
                slides: slides.clone(),
                ..UniversalDocumentModel::default()
            };
            UniversalFormatConverter::encode(&model, format)
        }
    }
}
