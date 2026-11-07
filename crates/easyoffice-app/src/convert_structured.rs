//! Structured save payloads for `convert_document_format` (PD-073).
//!
//! Instead of the legacy line markup in `body` (`[[TABLE_ROW]]`, `[[SLIDE]]`, …), the WebUI sends
//! one of:
//! - `blocks`: ordered `{"type":"paragraph","runs":[{"text","bold","italic","underline","strike",
//!   "sizeHalfPt","font","color","highlight"}],
//!   "align","heading","list"}` and `{"type":"table","rows":[[{"text","value"}]]}` items;
//! - `sheets`: `[{"name","cells":[{"ref":"A1","raw":"=SUM(A1:A2)","style":{"bold":true,"sizePt":11.25}}],`n//!   "columns":[{"index":1,"widthPx":120}]}]` for every workbook sheet (`style` and `columns` are optional,`n//!   PD-089; `index` is the 1-based column number);
//! - `slides`: `[{"title","bullets":[...],"notes"}]`.
//!
//! Every string is literal content; nothing in it is parsed as markup.

use std::collections::BTreeMap;

use easyoffice_formats::{
    evaluate_table_formulas, normalize_hex_color, CellStyle, DocumentFormat, EasyOfficeError,
    OdfCodec, OdsUnpacked, OoxmlCodec, RichAlignment, RichBlock, RichDocument, RichListKind,
    RichParagraph, RichRun, SheetStyling, StrictJsonValue, UniversalDocumentModel,
    UniversalFormatConverter, XlsxCachedValue, XlsxFormulaCache, MAX_COLUMN_PX, MAX_SIZE_PT,
    MIN_COLUMN_PX, MIN_SIZE_PT,
};
use easyoffice_sheet::{EvaluatedCellValue, PackedCellCoordinate, WorkbookEngine};

use crate::convert_ipc::compute_xlsx_formula_cache;

/// Upper bound on runs + table cells + sheet cells + slide items in one payload.
const MAX_STRUCTURED_ITEMS: usize = 500_000;
const MAX_SHEETS: usize = 256;
/// Upper bound on custom column widths of one sheet (the XLSX grid has 16,384 columns).
const MAX_COLUMN_ENTRIES: usize = 16_384;
/// Upper bound on cells of the value grids rendered when a workbook is saved to a document format.
const MAX_GRID_CELLS: usize = 2_000_000;

pub(crate) type SlideTriple = (String, Vec<String>, String);
/// Displayed values of one sheet, row by row (rows without cells are omitted).
type ValueGrid = Vec<Vec<String>>;

pub(crate) enum StructuredContent {
    Document(RichDocument),
    /// Sheets plus their cell formatting and column widths (one SheetStyling per sheet).
    Workbook(Vec<OdsUnpacked>, Vec<SheetStyling>),
    Slides(Vec<SlideTriple>),
}

fn invalid(msg: impl Into<String>) -> EasyOfficeError {
    EasyOfficeError::InvalidArgument(msg.into())
}

type JsonObject = [(String, StrictJsonValue)];

fn field<'a>(obj: &'a JsonObject, key: &str) -> Option<&'a StrictJsonValue> {
    match StrictJsonValue::get_field(obj, key) {
        None | Some(StrictJsonValue::Null) => None,
        Some(v) => Some(v),
    }
}

fn opt_str<'a>(obj: &'a JsonObject, key: &str) -> Result<Option<&'a str>, EasyOfficeError> {
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

/// Returns the structured content if the payload carries `blocks`, `sheets`, or `slides`.
pub(crate) fn parse_structured_content(
    entries: &JsonObject,
) -> Result<Option<StructuredContent>, EasyOfficeError> {
    let mut budget = Budget(0);
    if let Some(v) = field(entries, "blocks") {
        let blocks = parse_blocks(as_array(v, "blocks")?, &mut budget)?;
        return Ok(Some(StructuredContent::Document(RichDocument {
            title: String::new(),
            blocks,
            official_agency: None,
        })));
    }
    if let Some(v) = field(entries, "sheets") {
        return parse_sheets(as_array(v, "sheets")?, &mut budget)
            .map(|(sheets, styling)| Some(StructuredContent::Workbook(sheets, styling)));
    }
    if let Some(v) = field(entries, "slides") {
        return parse_slides(as_array(v, "slides")?, &mut budget)
            .map(|s| Some(StructuredContent::Slides(s)));
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
                text: text.to_string(),
                bold: opt_bool(run, "bold")?,
                italic: opt_bool(run, "italic")?,
                underline: opt_bool(run, "underline")?,
                strike: opt_bool(run, "strike")?,
                size_half_pt: opt_size_half_pt(run)?,
                font: opt_font(run)?,
                color: opt_color(run, "color")?,
                highlight: opt_color(run, "highlight")?,
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
        runs,
        alignment,
        heading_level,
        list,
    })
}

fn parse_table(obj: &JsonObject, budget: &mut Budget) -> Result<Vec<Vec<String>>, EasyOfficeError> {
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
    Ok(rows)
}

fn parse_blocks(
    items: &[StrictJsonValue],
    budget: &mut Budget,
) -> Result<Vec<RichBlock>, EasyOfficeError> {
    let mut blocks = Vec::with_capacity(items.len());
    for item in items {
        budget.take(1)?;
        let obj = as_object(item, "blocks")?;
        match opt_str(obj, "type")? {
            Some("paragraph") => blocks.push(RichBlock::Paragraph(parse_paragraph(obj, budget)?)),
            Some("table") => blocks.push(RichBlock::Table(parse_table(obj, budget)?)),
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

/// Evaluates every formula cell across all sheets (cross-sheet references resolve) and returns
/// one formula cache per sheet plus, when `with_grids`, the displayed-value grid of each sheet.
fn evaluate_workbook(
    sheets: &[OdsUnpacked],
    with_grids: bool,
) -> Result<(Vec<XlsxFormulaCache>, Vec<ValueGrid>), EasyOfficeError> {
    let mut wb = WorkbookEngine::new();
    for (name, cells) in sheets {
        wb.add_sheet(name.clone());
        for (row, col, raw) in cells {
            if let Ok(coord) = PackedCellCoordinate::new(*row, *col) {
                let _ = wb.set_cell(name, &coord.to_a1(), raw.clone());
            }
        }
    }
    let mut caches = Vec::with_capacity(sheets.len());
    let mut grids = Vec::new();
    let mut grid_cells = 0usize;
    for (name, cells) in sheets {
        let (cache, rows) = evaluate_sheet(&wb, name, cells, with_grids, &mut grid_cells)?;
        caches.push(cache);
        if with_grids {
            grids.push(rows.into_values().collect());
        }
    }
    Ok((caches, grids))
}

/// Evaluates one sheet: the formula cache and (when `with_grids`) the displayed rows.
fn evaluate_sheet(
    wb: &WorkbookEngine,
    name: &str,
    cells: &[(u32, u32, String)],
    with_grids: bool,
    grid_cells: &mut usize,
) -> Result<(XlsxFormulaCache, BTreeMap<u32, Vec<String>>), EasyOfficeError> {
    let mut cache = XlsxFormulaCache::new();
    let mut rows: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for (row, col, raw) in cells {
        let display = if raw.starts_with('=') {
            let Ok(coord) = PackedCellCoordinate::new(*row, *col) else {
                continue;
            };
            let value = wb.evaluate_cell(name, &coord.to_a1());
            if let Some(c) = cached_value(&value) {
                cache.insert((*row, *col), c);
            }
            value.to_display_string()
        } else {
            raw.clone()
        };
        if with_grids {
            set_grid_cell(
                rows.entry(*row).or_default(),
                *col as usize,
                display,
                grid_cells,
            )?;
        }
    }
    Ok((cache, rows))
}

/// Writes `display` at column `col`, growing the row within the total grid budget.
fn set_grid_cell(
    line: &mut Vec<String>,
    col: usize,
    display: String,
    grid_cells: &mut usize,
) -> Result<(), EasyOfficeError> {
    let needed = col + 1;
    if line.len() < needed {
        *grid_cells = grid_cells.saturating_add(needed - line.len());
        if *grid_cells > MAX_GRID_CELLS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "시트 값 표가 너무 커서 문서 형식으로 저장할 수 없습니다.".to_string(),
            ));
        }
        line.resize(needed, String::new());
    }
    line[col] = display;
    Ok(())
}

/// Encodes structured content to `format`, titling the output `title`.
pub(crate) fn encode_structured(
    content: &StructuredContent,
    format: DocumentFormat,
    title: &str,
) -> Result<Vec<u8>, EasyOfficeError> {
    match content {
        StructuredContent::Document(doc) => {
            let doc = RichDocument {
                title: title.to_string(),
                ..doc.clone()
            };
            let cache = (format == DocumentFormat::Xlsx)
                .then(|| compute_xlsx_formula_cache(&doc.to_universal_model()));
            UniversalFormatConverter::encode_rich(&doc, format, cache.as_ref())
        }
        StructuredContent::Workbook(sheets, stylings) => match format {
            DocumentFormat::Xlsx => {
                let (caches, _) = evaluate_workbook(sheets, false)?;
                OoxmlCodec::pack_xlsx_multi_styled(sheets, &caches, stylings)
            }
            DocumentFormat::Ods => OdfCodec::pack_ods_multi_styled(sheets, stylings),
            DocumentFormat::Xls | DocumentFormat::Csv | DocumentFormat::Tsv => {
                let model = UniversalDocumentModel {
                    title: title.to_string(),
                    sheet_cells: sheets.first().map(|(_, c)| c.clone()).unwrap_or_default(),
                    ..UniversalDocumentModel::default()
                };
                UniversalFormatConverter::encode(&model, format)
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
                        blocks.push(RichBlock::Table(grid));
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
