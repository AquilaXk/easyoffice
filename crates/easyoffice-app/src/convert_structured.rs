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
    check_document_images, clean_font_name, code_within_bounds, frame_paragraph,
    normalize_hex_color, safe_link_target, CellStyle, DefaultFont, EasyOfficeError, FontTable,
    ImageBudget, NumFmtTable, OdsUnpacked, OoxmlCodec, PdfWriteOptions, RichAlignment, RichBlock,
    RichColumns, RichDocument, RichField, RichListKind, RichListNumber, RichPageFrame,
    RichParagraph, RichRun, RichScript, RichSlideDeck, RichToc, SheetStyling, StrictJsonValue,
    MAX_COLUMN_PX, MAX_FONT_FACES, MAX_NUM_FMTS, MAX_PAGE_FRAME_PARAGRAPHS, MAX_PARA_SPACING_TWIPS,
    MAX_SIZE_PT, MAX_TOC_LEVEL, MAX_TOC_PAGE, MIN_COLUMN_PX, MIN_SIZE_PT,
};
use easyoffice_sheet::{PackedCellCoordinate, WorkbookEngine};

#[path = "convert_structured_notes.rs"]
pub(crate) mod convert_structured_notes;
#[path = "convert_structured_table.rs"]
pub(crate) mod convert_structured_table;

use crate::convert_structured_doc::parse_pdf_options;
use crate::page_setup_json::parse_page_setup;
use crate::sheet_look_json::parse_look;

/// Largest indent (twips, 1/20 pt) a paragraph may carry: 200 inches.
const MAX_INDENT_TWIPS: i64 = 288_000;
/// Largest line spacing, as a percentage of single spacing.
const MAX_LINE_SPACING_PERCENT: i64 = 1_000;
/// Maximum UTF-8 byte length of a `frameCopy` header/footer snapshot string.
const MAX_FRAME_COPY_BYTES: usize = 64 * 1024;

/// Upper bound on runs + table cells + sheet cells + slide items in one payload.
const MAX_STRUCTURED_ITEMS: usize = 500_000;
const MAX_SHEETS: usize = 256;
/// Upper bound on custom column widths of one sheet (the XLSX grid has 16,384 columns).
const MAX_COLUMN_ENTRIES: usize = 16_384;

pub(crate) type SlideTriple = (String, Vec<String>, String);

// One value per save request; boxing the document would only add an indirection for the common variant.
#[allow(clippy::large_enum_variant)]
pub(crate) enum StructuredContent {
    /// A document and the page settings a PDF target writes with it.
    Document(RichDocument, PdfWriteOptions),
    /// Sheets plus their cell formatting and column widths (one SheetStyling per sheet).
    Workbook(Vec<OdsUnpacked>, Vec<SheetStyling>),
    Slides(Vec<SlideTriple>),
    SlideDeck(RichSlideDeck),
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

fn opt_frame_copy(obj: &JsonObject) -> Result<Option<String>, EasyOfficeError> {
    match field(obj, "frameCopy") {
        None => Ok(None),
        Some(v) => {
            if let Some(b) = v.as_bool() {
                return Ok(b.then(String::new));
            }
            if let Some(s) = v.as_str() {
                if s.len() > MAX_FRAME_COPY_BYTES {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "구조화 저장: 'frameCopy' 원문 길이가 한도를 넘었습니다.".to_string(),
                    ));
                }
                return Ok(Some(s.to_string()));
            }
            Err(invalid(
                "구조화 저장: 'frameCopy'는 불리언 또는 문자열이어야 합니다.",
            ))
        }
    }
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

pub(crate) fn as_array<'a>(
    v: &'a StrictJsonValue,
    what: &str,
) -> Result<&'a [StrictJsonValue], EasyOfficeError> {
    v.as_array()
        .ok_or_else(|| invalid(format!("구조화 저장: '{what}'는 배열이어야 합니다.")))
}

pub(crate) fn as_object<'a>(
    v: &'a StrictJsonValue,
    what: &str,
) -> Result<&'a JsonObject, EasyOfficeError> {
    v.as_object()
        .ok_or_else(|| invalid(format!("구조화 저장: '{what}' 항목은 객체여야 합니다.")))
}

#[derive(Default)]
pub(crate) struct Budget {
    items: usize,
    pub(super) revisions: usize,
}

impl Budget {
    pub(crate) fn take(&mut self, n: usize) -> Result<(), EasyOfficeError> {
        self.items = self.items.saturating_add(n);
        if self.items > MAX_STRUCTURED_ITEMS {
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
    let mut budget = Budget::default();
    if let Some(v) = field(entries, "blocks") {
        let mut warnings = Vec::new();
        let mut blocks = parse_blocks(as_array(v, "blocks")?, &mut budget, &mut warnings)?;
        if let Some(doc_cols) = opt_bounded(entries, "docColumns", 0, 16)?
            .filter(|&c| c >= 2)
            .map(|c| RichColumns {
                count: c as u8,
                equal: true,
                gap_twips: 720,
            })
        {
            for block in &mut blocks {
                if let RichBlock::Paragraph(p) = block {
                    if p.columns.is_none() {
                        p.columns = Some(doc_cols);
                    }
                }
            }
        }
        let notes = convert_structured_notes::parse_doc_notes(entries, &mut budget)?;
        let comments = convert_structured_notes::parse_doc_comments(entries, &mut budget)?;
        let track_changes = opt_bool(entries, "trackChanges")?;
        let doc = RichDocument {
            page_frame: parse_page_frame(entries, &mut budget)?,
            title: String::new(),
            blocks,
            official_agency: None,
            notes,
            comments,
            track_changes,
        };
        check_document_images(&doc)?;
        let content = StructuredContent::Document(doc, parse_pdf_options(entries)?);
        return Ok(Some((content, warnings)));
    }
    if let Some(v) = field(entries, "sheets") {
        // The workbook's default font (PD-306) decides the pixels of the anchors and the characters of the widths of every
        // sheet, so each sheet takes it before its layout is read.
        let default_font = match field(entries, "defaultFont") {
            Some(v) => crate::sheet_default_font_json::parse_default_font(v)?,
            None => None,
        };
        let (sheets, mut stylings) =
            parse_sheets(as_array(v, "sheets")?, &mut budget, default_font.as_ref())?;
        if let (Some(names), Some(first)) = (field(entries, "names"), stylings.first_mut()) {
            first.defined_names = crate::sheet_names_json::parse_names_for_save(names)?;
        }
        if let (Some(book), Some(first)) = (field(entries, "workbookProtect"), stylings.first_mut())
        {
            first.workbook_protect = crate::sheet_protect_json::parse_workbook_protect(book)?;
        }
        if let (Some(theme), Some(first)) = (field(entries, "workbookTheme"), stylings.first_mut())
        {
            first.workbook_theme = crate::sheet_table_json::parse_workbook_theme(theme)?;
        }
        let calc_options = field(entries, "calcOptions")
            .map(crate::sheet_calc_options_json::parse_calc_options)
            .transpose()?;
        if let Some(first) = stylings.first_mut() {
            first.date1904 = opt_bool(entries, "date1904")?;
            if let Some(co) = calc_options {
                first.calc_options = co;
            }
        }
        return Ok(Some((
            StructuredContent::Workbook(sheets, stylings),
            vec![],
        )));
    }
    if let Some(v) = field(entries, "slides") {
        return crate::slide_deck_json::parse_slide_deck(
            entries,
            as_array(v, "slides")?,
            &mut budget,
        )
        .map(|d| Some((StructuredContent::SlideDeck(d), vec![])));
    }
    Ok(None)
}

/// `"header"` and `"footer"` of a document payload: arrays of paragraph blocks (at most
/// [`MAX_PAGE_FRAME_PARAGRAPHS`] each); a table or picture there rejects the save.
fn parse_page_frame(
    entries: &JsonObject,
    budget: &mut Budget,
) -> Result<RichPageFrame, EasyOfficeError> {
    let mut parse = |key: &str| -> Result<Vec<RichParagraph>, EasyOfficeError> {
        let Some(value) = field(entries, key) else {
            return Ok(Vec::new());
        };
        let items = as_array(value, key)?;
        if items.len() > MAX_PAGE_FRAME_PARAGRAPHS {
            return Err(invalid(format!(
                "구조화 저장: '{key}'는 문단 {MAX_PAGE_FRAME_PARAGRAPHS}개 이하여야 합니다."
            )));
        }
        items
            .iter()
            .map(|item| {
                budget.take(1)?;
                let obj = as_object(item, key)?;
                if opt_str(obj, "type")? != Some("paragraph") {
                    return Err(invalid(format!(
                        "구조화 저장: '{key}'에는 문단만 올 수 있습니다."
                    )));
                }
                parse_paragraph(obj, budget).map(|p| frame_paragraph(&p))
            })
            .collect()
    };
    let (margins, paper, binding) = parse_page_setup(entries)?;
    let first_header = parse("firstHeader")?;
    let first_footer = parse("firstFooter")?;
    let even_header = parse("evenHeader")?;
    let even_footer = parse("evenFooter")?;
    let first_page_different = opt_bool(entries, "firstPageDifferent")?
        || !first_header.is_empty()
        || !first_footer.is_empty();
    let even_odd_different = opt_bool(entries, "evenOddDifferent")?
        || !even_header.is_empty()
        || !even_footer.is_empty();
    Ok(RichPageFrame {
        header: parse("header")?,
        footer: parse("footer")?,
        first_header,
        first_footer,
        even_header,
        even_footer,
        first_page_different,
        even_odd_different,
        margins,
        paper,
        binding,
    })
}

/// Run field: `"page"` (the page number), `"pages"` (the page count), `"filename"` or absent.
fn opt_run_field(run: &JsonObject) -> Result<Option<RichField>, EasyOfficeError> {
    match opt_str(run, "field")? {
        None | Some("") => Ok(None),
        Some("page") => Ok(Some(RichField::PageNumber)),
        Some("pages") => Ok(Some(RichField::TotalPages)),
        Some("filename") => Ok(Some(RichField::FileName)),
        Some(other) => Err(invalid(format!("구조화 저장: 알 수 없는 필드 '{other}'"))),
    }
}

pub(crate) fn parse_runs(
    obj: &JsonObject,
    budget: &mut Budget,
) -> Result<Vec<RichRun>, EasyOfficeError> {
    let mut runs = Vec::new();
    if let Some(v) = field(obj, "runs") {
        for item in as_array(v, "runs")? {
            budget.take(1)?;
            let run = as_object(item, "runs")?;
            let text = field(run, "text")
                .ok_or_else(|| invalid("구조화 저장: run에는 'text'가 필요합니다."))?
                .as_str()
                .ok_or_else(|| invalid("구조화 저장: run 'text'는 문자열이어야 합니다."))?;
            let (note_ref, comment_id, revision) =
                convert_structured_notes::parse_run_annotations(run, budget)?;
            runs.push(RichRun {
                field: opt_run_field(run)?,
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
                note_ref,
                comment_id,
                revision,
            });
        }
    }
    Ok(runs)
}

pub(crate) fn parse_alignment(obj: &JsonObject) -> Result<Option<RichAlignment>, EasyOfficeError> {
    opt_str(obj, "align")?
        .filter(|a| !a.trim().is_empty())
        .map(|a| {
            RichAlignment::parse(a)
                .ok_or_else(|| invalid(format!("구조화 저장: 알 수 없는 정렬 '{a}'")))
        })
        .transpose()
}

fn parse_paragraph(
    obj: &JsonObject,
    budget: &mut Budget,
) -> Result<RichParagraph, EasyOfficeError> {
    let runs = parse_runs(obj, budget)?;
    let alignment = parse_alignment(obj)?;
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
    let toc_page = match field(obj, "tocPage") {
        None => None,
        Some(v) => Some(
            v.as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| (1..=MAX_TOC_PAGE).contains(n))
                .filter(|_| matches!(toc, Some(RichToc::Entry(_))))
                .ok_or_else(|| {
                    invalid("구조화 저장: 'tocPage'는 목차 항목의 1~9999999 정수여야 합니다.")
                })?,
        ),
    };
    let toc_dots = opt_bool(obj, "tocDots")?;
    let list = match opt_str(obj, "list")? {
        None | Some("") => None,
        Some("bullet") => Some(RichListKind::Bullet),
        Some("ordered") => Some(RichListKind::Ordered),
        Some("task") => Some(RichListKind::Task),
        Some(other) => return Err(invalid(format!("구조화 저장: 알 수 없는 목록 '{other}'"))),
    };
    let task_checked = opt_bool(obj, "checked")? && list == Some(RichListKind::Task);
    let raw_list_level = opt_bounded(obj, "listLevel", 0, 8)?.unwrap_or(0) as u8;
    let list_level = if list.is_some() { raw_list_level } else { 0 };
    let list_number = match opt_str(obj, "listNumber")?.filter(|s| !s.is_empty()) {
        None => None,
        Some(code) => {
            let parsed = RichListNumber::from_wire_code(code)
                .ok_or_else(|| invalid(format!("구조화 저장: 알 수 없는 번호 서식 '{code}'")))?;
            (list == Some(RichListKind::Ordered)).then_some(parsed)
        }
    };
    let columns = match field(obj, "columns") {
        None => None,
        Some(v) => parse_doc_columns(v)?,
    };
    Ok(RichParagraph {
        list_level,
        list_number,
        task_checked,
        runs,
        alignment,
        heading_level,
        list,
        left_indent_twips: opt_bounded(obj, "leftIndentTwips", 0, MAX_INDENT_TWIPS)?
            .map(|v| v as u32),
        right_indent_twips: opt_bounded(obj, "rightIndentTwips", 0, MAX_INDENT_TWIPS)?
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
        space_before_twips: opt_bounded(
            obj,
            "spaceBeforeTwips",
            0,
            i64::from(MAX_PARA_SPACING_TWIPS),
        )?
        .map(|v| v as u32),
        space_after_twips: opt_bounded(
            obj,
            "spaceAfterTwips",
            0,
            i64::from(MAX_PARA_SPACING_TWIPS),
        )?
        .map(|v| v as u32),
        toc,
        bookmark,
        toc_page,
        toc_dots,
        page_break_before: opt_bool(obj, "pageBreakBefore")?,
        columns,
        frame_copy: opt_frame_copy(obj)?,
    })
}

fn parse_doc_columns(v: &StrictJsonValue) -> Result<Option<RichColumns>, EasyOfficeError> {
    let obj = as_object(v, "columns")?;
    let count = opt_bounded(obj, "count", 1, 16)?
        .ok_or_else(|| invalid("구조화 저장: 'columns.count'는 1~16 정수여야 합니다."))?
        as u8;
    let equal = match field(obj, "equal") {
        None => true,
        Some(_) => opt_bool(obj, "equal")?,
    };
    let gap_twips = opt_bounded(obj, "gapTwips", 0, i64::from(u16::MAX))?.unwrap_or(720) as u16;
    Ok((count >= 2).then_some(RichColumns {
        count,
        equal,
        gap_twips,
    }))
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

fn parse_blocks(
    items: &[StrictJsonValue],
    budget: &mut Budget,
    warnings: &mut Vec<String>,
) -> Result<Vec<RichBlock>, EasyOfficeError> {
    let mut blocks = Vec::with_capacity(items.len());
    let mut img_budget = ImageBudget::default();
    for item in items {
        budget.take(1)?;
        let obj = as_object(item, "blocks")?;
        match opt_str(obj, "type")? {
            Some("paragraph") => blocks.push(RichBlock::Paragraph(parse_paragraph(obj, budget)?)),
            Some("table") => blocks.push(RichBlock::Table(convert_structured_table::parse_table(
                obj,
                budget,
                &mut img_budget,
                warnings,
            )?)),
            Some("image") => blocks.push(RichBlock::Image(
                convert_structured_table::parse_image_budgeted(obj, &mut img_budget)?,
            )),
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
/// "sizePt","nf"}` and the look keys of `sheet_look_json` (`ha`, `va`, `wrap`, `bd`, PD-207); `sizePt` is 1 to 409 points and is stored with two decimals; `nf` is the
/// 1-based entry of the sheet's `numFmts` (`num_fmt_ids` holds the format id of each entry).
fn parse_cell_style(
    obj: &JsonObject,
    num_fmt_ids: &[u16],
    font_ids: &[u16],
    font_table: &mut FontTable,
) -> Result<CellStyle, EasyOfficeError> {
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
    let font = match field(obj, "fn") {
        Some(v) => v
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .filter(|&n| n <= font_ids.len())
            .map_or_else(
                || Err(invalid("구조화 저장: 'fn'이 'fonts' 범위를 벗어났습니다.")),
                |n| Ok(n.checked_sub(1).map_or(0, |at| font_ids[at])),
            )?,
        None => match opt_str(obj, "font")? {
            None | Some("") => 0,
            Some(raw) => {
                let Some(clean) = clean_font_name(raw) else {
                    return Err(invalid("구조화 저장: 올바르지 않은 셀 글꼴 이름입니다."));
                };
                let id = font_table.intern(clean);
                if id == 0 {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                        "글꼴 이름 수가 한도({MAX_FONT_FACES})를 초과했습니다."
                    )));
                }
                id
            }
        },
    };
    let mut style = CellStyle {
        bold: opt_bool(obj, "bold")?,
        italic: opt_bool(obj, "italic")?,
        underline: opt_bool(obj, "underline")?,
        strike: opt_bool(obj, "strike")?,
        size_pt,
        num_fmt,
        font,
        ..CellStyle::default()
    };
    parse_look(obj, &mut style)?;
    Ok(style)
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

/// `fonts`: the sheet's non-default font family names (at most `MAX_FONT_FACES`, each validated).
fn parse_sheet_fonts(items: &[StrictJsonValue]) -> Result<(FontTable, Vec<u16>), EasyOfficeError> {
    if items.len() > MAX_FONT_FACES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "글꼴 이름 수가 한도({MAX_FONT_FACES})를 초과했습니다."
        )));
    }
    let mut table = FontTable::default();
    let mut ids = Vec::with_capacity(items.len());
    for item in items {
        let face = item
            .as_str()
            .and_then(clean_font_name)
            .ok_or_else(|| invalid("구조화 저장: 글꼴 이름은 한도 안의 문자열이어야 합니다."))?;
        ids.push(table.intern(face));
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
    default_font: Option<&DefaultFont>,
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
        let mut styling = SheetStyling {
            default_font: default_font.cloned(),
            ..SheetStyling::default()
        };
        let (num_fmts, num_fmt_ids) = match field(obj, "numFmts") {
            Some(v) => parse_num_fmts(as_array(v, "numFmts")?)?,
            None => (NumFmtTable::default(), Vec::new()),
        };
        let (mut font_table, font_ids) = match field(obj, "fonts") {
            Some(v) => parse_sheet_fonts(as_array(v, "fonts")?)?,
            None => (FontTable::default(), Vec::new()),
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
                    let style = parse_cell_style(
                        as_object(style_v, "style")?,
                        &num_fmt_ids,
                        &font_ids,
                        &mut font_table,
                    )?;
                    if !style.is_plain() {
                        styling.cells.insert(key, style);
                    }
                }
                let raw = opt_str(cell, "raw")?.unwrap_or("");
                let explicit_kind = match opt_str(cell, "t")? {
                    None => None,
                    Some(tag) => Some(
                        easyoffice_formats::CellValueKind::from_tag(tag).ok_or_else(|| {
                            invalid(format!("구조화 저장: 알 수 없는 셀 타입 '{tag}'"))
                        })?,
                    ),
                };
                if !raw.is_empty() {
                    if let Some(kind) = explicit_kind {
                        styling.set_cell_kind(key.0, key.1, raw, kind);
                    }
                    if let Some(ph_val) = field(cell, "ph") {
                        if let Some(ph) =
                            crate::sheet_phonetic_json::parse_cell_phonetic_json(ph_val, raw)?
                        {
                            styling.set_cell_phonetic(key.0, key.1, raw, ph);
                        }
                    }
                    cells.insert(key, raw.to_string());
                }
            }
        }
        styling.fonts = font_table.finish().0;
        if let Some(v) = field(obj, "columns") {
            styling.columns = parse_columns(as_array(v, "columns")?, budget)?;
        }
        if let Some(v) = field(obj, "rules") {
            styling.rules = crate::sheet_rules_json::parse_rules(v)?;
        }
        if let Some(v) = field(obj, "links") {
            styling.links = crate::sheet_links_json::parse_links(v)?;
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

pub use crate::convert_structured_encode::RichPdfRenderer;
pub(crate) use crate::convert_structured_encode::{
    encode_source_target, encode_structured, encode_target,
};
