//! Real source files for `convert_document_format` (PD-075).
//!
//! `{"sourceBase64": ..., "fileName": ...}` carries the bytes of a user's file:
//! - `"action":"decode"` returns the structured editor model (`blocks` / `sheets` / `slides`, the
//!   same shape structured saves send) plus the detected format, target workspace module, and
//!   what the model could not carry;
//! - otherwise the bytes are decoded and converted to `targetFormat` (the editor text is not used).
//!
//! Decoding runs through `UniversalFormatConverter::decode_rich`, so ZIP-bomb ratio/size
//! ceilings, path-traversal checks, and XXE/DTD rejection apply to every path.

use easyoffice_formats::{
    base64_codec, evaluate_table_formulas, resolve_code, CellMerge, CellStyle, DecodedContent,
    DecodedDocument, DocumentFormat, EasyOfficeError, OdsUnpacked, PdfWriteOptions, RichAlignment,
    RichBlock, RichField, RichImage, RichListKind, RichParagraph, RichScript, RichToc,
    SheetStyling, StrictJsonValue, UniversalFormatConverter, CUSTOM_BASE,
};
use easyoffice_sheet::PackedCellCoordinate;
use std::collections::HashSet;
use std::fmt::Write;

use crate::convert_hwp::ShellWriters;
use crate::convert_password::{accepts_loss, password_lost, REFUSED_PASSWORD_LOSS};
use crate::convert_structured::{encode_source_target, StructuredContent};
use crate::convert_structured_doc::parse_pdf_options;
use crate::page_setup_json::write_page_setup;
use crate::sheet_look_json::write_look;
use crate::sheet_rules_json::rules_json;
use crate::EasyOfficeIpcRouter;

/// Upper bound on a decoded source file: the number the page checks before it sends
/// (`MAX_FILE_BYTES` in `ui/file-open.js`; `scripts/test-file-size-limits.mjs` keeps them equal).
/// Its base64 form plus the request's other fields stays under `MAX_IPC_HTTP_BODY_BYTES`.
const MAX_SOURCE_BYTES: usize = 10 * 1_048_576;

type JsonEntries = [(String, StrictJsonValue)];

fn limit(entries: &JsonEntries, key: &str) -> Result<Option<u32>, EasyOfficeError> {
    match StrictJsonValue::get_field(entries, key) {
        None | Some(StrictJsonValue::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .filter(|n| *n >= 1 && *n <= u64::from(u32::MAX))
            .map(|n| Some(n as u32))
            .ok_or_else(|| {
                EasyOfficeError::InvalidArgument(format!("'{key}'는 1 이상의 정수여야 합니다."))
            }),
    }
}

/// Appends `s` as a JSON string literal.
fn push_json_str(out: &mut String, s: &str) {
    out.push('"');
    EasyOfficeIpcRouter::escape_json_into(out, s);
    out.push('"');
}

fn write_paragraph(out: &mut String, p: &RichParagraph) {
    out.push_str("{\"type\":\"paragraph\",\"runs\":[");
    for (idx, r) in p.runs.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push_str("{\"text\":");
        push_json_str(out, &r.text);
        for (on, key) in [
            (r.bold, "bold"),
            (r.italic, "italic"),
            (r.underline, "underline"),
            (r.strike, "strike"),
        ] {
            if on {
                let _ = write!(out, ",\"{key}\":true");
            }
        }
        if let Some(size) = r.size_half_pt {
            let _ = write!(out, ",\"sizeHalfPt\":{size}");
        }
        match r.field {
            Some(RichField::PageNumber) => out.push_str(",\"field\":\"page\""),
            Some(RichField::TotalPages) => out.push_str(",\"field\":\"pages\""),
            Some(RichField::FileName) => out.push_str(",\"field\":\"filename\""),
            None => {}
        }
        if let Some(link) = &r.link {
            out.push_str(",\"link\":");
            push_json_str(out, link);
        }
        match r.script {
            RichScript::Normal => {}
            RichScript::Superscript => out.push_str(",\"script\":\"superscript\""),
            RichScript::Subscript => out.push_str(",\"script\":\"subscript\""),
        }
        for (value, key) in [
            (&r.font, "font"),
            (&r.color, "color"),
            (&r.highlight, "highlight"),
        ] {
            if let Some(v) = value {
                let _ = write!(out, ",\"{key}\":");
                push_json_str(out, v);
            }
        }
        out.push('}');
    }
    out.push(']');
    if let Some(a) = p.alignment {
        let token = match a {
            RichAlignment::Left => "left",
            RichAlignment::Center => "center",
            RichAlignment::Right => "right",
            RichAlignment::Justify => "justify",
        };
        let _ = write!(out, ",\"align\":\"{token}\"");
    }
    if p.heading_level > 0 {
        let _ = write!(out, ",\"heading\":{}", p.heading_level);
    }
    if let Some(page) = p.toc_page {
        let _ = write!(out, ",\"tocPage\":{page},\"tocDots\":{}", p.toc_dots);
    }
    if let Some(number) = p.bookmark {
        let _ = write!(out, ",\"bookmark\":{number}");
    }
    if p.page_break_before {
        out.push_str(",\"pageBreakBefore\":true");
    }
    match p.toc {
        Some(RichToc::Title) => out.push_str(",\"toc\":0"),
        Some(RichToc::Entry(level)) => {
            let _ = write!(out, ",\"toc\":{level}");
        }
        None => {}
    }
    if let Some(l) = p.list {
        let token = match l {
            RichListKind::Bullet => "bullet",
            RichListKind::Ordered => "ordered",
        };
        let _ = write!(out, ",\"list\":\"{token}\"");
    }
    if let Some(v) = p.left_indent_twips {
        let _ = write!(out, ",\"leftIndentTwips\":{v}");
    }
    if let Some(v) = p.first_line_indent_twips {
        let _ = write!(out, ",\"firstLineIndentTwips\":{v}");
    }
    if let Some(v) = p.line_spacing_percent {
        let _ = write!(out, ",\"lineSpacingPercent\":{v}");
    }
    out.push('}');
}

/// Picture block of the editor model: the encoded bytes travel as base64 and come back unchanged.
fn write_image(out: &mut String, img: &RichImage) {
    let (width_emu, height_emu) = img.display_emu();
    let _ = write!(
        out,
        "{{\"type\":\"image\",\"format\":\"{}\",\"base64\":\"{}\",\"widthPx\":{},\"heightPx\":{},\"widthEmu\":{width_emu},\"heightEmu\":{height_emu},\"alt\":",
        img.format().extension(),
        base64_codec::encode(img.bytes()),
        img.width_px(),
        img.height_px(),
    );
    push_json_str(out, img.alt());
    out.push('}');
}

/// `,"merges":[{"row":0,"col":0,"rowSpan":1,"colSpan":2}]` for a table with merged regions (PD-130),
/// nothing otherwise. Covered cells stay in `rows` as empty text.
fn write_merges(out: &mut String, merges: &[CellMerge]) {
    if merges.is_empty() {
        return;
    }
    out.push_str(",\"merges\":[");
    for (idx, m) in merges.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"row\":{},\"col\":{},\"rowSpan\":{},\"colSpan\":{}}}",
            m.row, m.col, m.row_span, m.col_span
        );
    }
    out.push(']');
}

fn write_block(out: &mut String, b: &RichBlock) {
    match b {
        RichBlock::Paragraph(p) => write_paragraph(out, p),
        RichBlock::Image(img) => write_image(out, img),
        RichBlock::Table(table) => {
            out.push_str("{\"type\":\"table\",\"rows\":[");
            for (r_idx, row) in table.rows.iter().enumerate() {
                if r_idx > 0 {
                    out.push(',');
                }
                out.push('[');
                for (c_idx, cell) in row.iter().enumerate() {
                    if c_idx > 0 {
                        out.push(',');
                    }
                    out.push_str("{\"text\":");
                    push_json_str(out, cell);
                    out.push('}');
                }
                out.push(']');
            }
            out.push(']');
            write_merges(out, &table.merges);
            out.push('}');
        }
    }
}

/// Whether a cell at (`row`, `col`) lies inside the editor grid.
fn in_grid(row: u32, col: u32, max_rows: Option<u32>, max_cols: Option<u32>) -> bool {
    max_rows.is_none_or(|m| row < m) && max_cols.is_none_or(|m| col < m)
}

/// Number of cells outside the editor grid.
fn cells_outside_grid(
    sheets: &[OdsUnpacked],
    max_rows: Option<u32>,
    max_cols: Option<u32>,
) -> usize {
    if max_rows.is_none() && max_cols.is_none() {
        return 0;
    }
    sheets
        .iter()
        .flat_map(|(_, cells)| cells)
        .filter(|(r, c, _)| !in_grid(*r, *c, max_rows, max_cols))
        .count()
}

/// The number format codes a sheet's cells use, in first-use order, with a constant-time lookup
/// from a cell's format id to its 1-based entry (PD-193). Built-in ids resolve to their code, so
/// the page never needs the id table.
struct NumFmtIndex<'a> {
    codes: Vec<&'a str>,
    entry_of: Vec<u16>,
}

impl<'a> NumFmtIndex<'a> {
    fn build(styling: Option<&'a SheetStyling>) -> Self {
        let mut index = Self {
            codes: Vec::new(),
            entry_of: Vec::new(),
        };
        let Some(styling) = styling else {
            return index;
        };
        let slots = usize::from(CUSTOM_BASE) + styling.num_fmts.len();
        for style in styling.cells.values() {
            let id = usize::from(style.num_fmt);
            if id == 0 || id >= slots {
                continue;
            }
            if index.entry_of.is_empty() {
                index.entry_of = vec![0; slots];
            }
            if index.entry_of[id] != 0 {
                continue;
            }
            if let Some(code) = resolve_code(style.num_fmt, &styling.num_fmts) {
                index.codes.push(code);
                index.entry_of[id] = index.codes.len() as u16;
            }
        }
        index
    }

    fn entry(&self, num_fmt: u16) -> Option<u16> {
        self.entry_of
            .get(usize::from(num_fmt))
            .copied()
            .filter(|&e| e != 0)
    }
}

/// `{"bold":true,"sizePt":11.25,"nf":2}`: only the set flags, the explicit size and the format
/// entry.
fn write_style(out: &mut String, style: &CellStyle, num_fmts: &NumFmtIndex<'_>) {
    out.push('{');
    let mut first = true;
    for (on, key) in [
        (style.bold, "bold"),
        (style.italic, "italic"),
        (style.underline, "underline"),
        (style.strike, "strike"),
    ] {
        if on {
            if !first {
                out.push(',');
            }
            first = false;
            let _ = write!(out, "\"{key}\":true");
        }
    }
    if let Some(size) = style.size_pt {
        if !first {
            out.push(',');
        }
        first = false;
        let _ = write!(out, "\"sizePt\":{size}");
    }
    if let Some(entry) = num_fmts.entry(style.num_fmt) {
        if !first {
            out.push(',');
        }
        first = false;
        let _ = write!(out, "\"nf\":{entry}");
    }
    let mut wrote = !first;
    write_look(out, style, &mut wrote);
    out.push('}');
}

#[inline]
const fn pack_row_col(r: u32, c: u32) -> u64 {
    ((r as u64) << 32) | (c as u64)
}

/// Cells of one sheet inside the grid as a JSON array body: valued cells first, then styled
/// cells without a value.
fn write_sheet_cells(
    out: &mut String,
    cells: &[(u32, u32, String)],
    styling: Option<&SheetStyling>,
    num_fmts: &NumFmtIndex<'_>,
    max_rows: Option<u32>,
    max_cols: Option<u32>,
) {
    let styles = styling.filter(|s| !s.cells.is_empty());
    // Only a styled cell can reappear as "styled without a value", so only those are remembered.
    let mut valued: HashSet<u64> = HashSet::new();
    let mut first = true;
    for (r, c, raw) in cells {
        if !in_grid(*r, *c, max_rows, max_cols) {
            continue;
        }
        let style = styles.and_then(|s| s.cells.get(&(*r, *c)));
        if style.is_some() {
            valued.insert(pack_row_col(*r, *c));
        }
        let Ok(coord) = PackedCellCoordinate::new(*r, *c) else {
            continue;
        };
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str("{\"ref\":\"");
        coord.write_a1(out);
        out.push_str("\",\"raw\":");
        push_json_str(out, raw);
        if let Some(style) = style {
            out.push_str(",\"style\":");
            write_style(out, style, num_fmts);
        }
        out.push('}');
    }
    let Some(styles) = styles else {
        return;
    };
    for (&(r, c), style) in &styles.cells {
        if !in_grid(r, c, max_rows, max_cols) || valued.contains(&pack_row_col(r, c)) {
            continue;
        }
        let Ok(coord) = PackedCellCoordinate::new(r, c) else {
            continue;
        };
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str("{\"ref\":\"");
        coord.write_a1(out);
        out.push_str("\",\"raw\":\"\",\"style\":");
        write_style(out, style, num_fmts);
        out.push('}');
    }
}

/// Custom column widths inside the grid as a JSON array body (1-based `index`).
fn write_columns(out: &mut String, styling: Option<&SheetStyling>, max_cols: Option<u32>) {
    let Some(s) = styling else {
        return;
    };
    let mut first = true;
    for (c, px) in &s.columns {
        if max_cols.is_some_and(|m| *c >= m) {
            continue;
        }
        if !first {
            out.push(',');
        }
        first = false;
        let _ = write!(out, "{{\"index\":{},\"widthPx\":{px}}}", c + 1);
    }
}

fn write_sheet(
    out: &mut String,
    name: &str,
    cells: &[(u32, u32, String)],
    styling: Option<&SheetStyling>,
    max_rows: Option<u32>,
    max_cols: Option<u32>,
) {
    out.push_str("{\"name\":");
    push_json_str(out, name);
    out.push_str(",\"cells\":[");
    let num_fmts = NumFmtIndex::build(styling);
    write_sheet_cells(out, cells, styling, &num_fmts, max_rows, max_cols);
    out.push_str("],\"columns\":[");
    write_columns(out, styling, max_cols);
    out.push(']');
    if !num_fmts.codes.is_empty() {
        out.push_str(",\"numFmts\":[");
        write_list(out, &num_fmts.codes, |out, code| push_json_str(out, code));
        out.push(']');
    }
    if let Some(st) = styling.filter(|st| !st.rules.is_empty()) {
        out.push_str(",\"rules\":");
        out.push_str(&rules_json(&st.rules));
    }
    if let Some(st) = styling.filter(|st| !st.links.is_empty()) {
        out.push_str(",\"links\":");
        out.push_str(&crate::sheet_links_json::links_json(
            &st.links, max_rows, max_cols,
        ));
    }
    if let Some(st) = styling {
        match crate::sheet_filter_ipc::split_for_open(cells, st) {
            Some((filter, hidden)) => {
                crate::sheet_layout_json::write_layout_split(out, st, &hidden, Some(&filter));
            }
            None => crate::sheet_layout_json::write_layout(out, st),
        }
    }
    out.push('}');
}

/// Appends `items` separated by commas, each written by `write`.
fn write_list<T>(out: &mut String, items: &[T], mut write: impl FnMut(&mut String, &T)) {
    for (idx, item) in items.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        write(out, item);
    }
}

fn decode_response(
    decoded: &DecodedDocument,
    styling: &[(String, SheetStyling)],
    file_name: &str,
    source_bytes: usize,
    max_rows: Option<u32>,
    max_cols: Option<u32>,
) -> Result<String, EasyOfficeError> {
    let mut warnings = decoded.warnings.clone();
    let (kind, count) = match &decoded.content {
        DecodedContent::Document(doc) => ("document", doc.blocks.len()),
        DecodedContent::Workbook(sheets) => {
            let dropped = cells_outside_grid(sheets, max_rows, max_cols);
            if dropped > 0 {
                warnings.push(format!(
                    "편집기 격자(최대 {}행 × {}열) 밖의 셀 {dropped}개는 가져오지 않았습니다.",
                    max_rows.map_or("무제한".to_string(), |m| m.to_string()),
                    max_cols.map_or("무제한".to_string(), |m| m.to_string()),
                ));
            }
            for (name, st) in styling {
                warnings.extend(st.rules.unsupported.iter().map(|rule| {
                    format!("{name} 시트의 {rule} 규칙은 지원하지 않아 가져오지 않았습니다.")
                }));
                if st.names_rejected > 0 {
                    warnings.push(format!(
                        "이름 {}개는 한도를 넘거나 올바르지 않아 가져오지 않았습니다.",
                        st.names_rejected
                    ));
                }
                if st.links_rejected > 0 {
                    warnings.push(format!(
                        "{name} 시트의 하이퍼링크 {}개는 허용하지 않는 주소이거나 한도를 넘어 가져오지 않았습니다.",
                        st.links_rejected
                    ));
                }
                if st.tables_rejected > 0 {
                    warnings.push(format!(
                        "{name} 시트의 표 {}개는 한도를 넘거나 올바르지 않아 가져오지 않았습니다.",
                        st.tables_rejected
                    ));
                }
                if st.merges_dropped > 0 {
                    warnings.push(format!(
                        "{name} 시트의 병합 {}개는 표와 겹쳐 풀었습니다. 표 안의 칸은 병합할 수 없습니다.",
                        st.merges_dropped
                    ));
                }
                if st.num_fmts_rejected > 0 {
                    warnings.push(format!(
                        "서식 코드 {}개는 한도를 넘거나 알 수 없어 일반 서식으로 가져왔습니다.",
                        st.num_fmts_rejected
                    ));
                }
            }
            ("workbook", sheets.len())
        }
        DecodedContent::Slides(slides) => ("slides", slides.len()),
    };
    let mut out = String::with_capacity(estimated_len(&decoded.content));
    out.push_str("{\"status\":\"OK\",\"action\":\"decode\",\"fileName\":");
    push_json_str(&mut out, file_name);
    let _ = write!(
        out,
        ",\"format\":\"{}\",\"module\":\"{}\",\"kind\":\"{kind}\",\"title\":",
        decoded.format.extension(),
        decoded.format.editor_module(),
    );
    push_json_str(&mut out, &decoded.title);
    let _ = write!(out, ",\"sourceBytes\":{source_bytes},\"specification\":");
    push_json_str(&mut out, decoded.format.standard_specification());
    let _ = write!(out, ",\"itemCount\":{count},\"warnings\":[");
    write_list(&mut out, &warnings, |out, w| push_json_str(out, w));
    out.push_str("],");
    match &decoded.content {
        DecodedContent::Document(doc) => {
            write_page_setup(&mut out, &doc.page_frame);
            out.push_str("\"blocks\":[");
            write_list(&mut out, &doc.blocks, write_block);
            for (key, paragraphs) in [
                ("header", &doc.page_frame.header),
                ("footer", &doc.page_frame.footer),
            ] {
                if !paragraphs.is_empty() {
                    let _ = write!(out, "],\"{key}\":[");
                    write_list(&mut out, paragraphs, write_paragraph);
                }
            }
        }
        DecodedContent::Workbook(sheets) => {
            out.push_str("\"sheets\":[");
            write_list(&mut out, sheets, |out, (name, cells)| {
                let sheet_styling = styling_of(styling, name);
                write_sheet(out, name, cells, sheet_styling, max_rows, max_cols);
            });
            out.push(']');
            let names = styling
                .first()
                .map_or(&[][..], |(_, st)| &st.defined_names[..]);
            if !names.is_empty() {
                out.push_str(",\"names\":");
                out.push_str(&crate::sheet_names_json::names_json(names));
            }
            if let Some(book) = styling
                .first()
                .and_then(|(_, st)| st.workbook_protect.as_ref())
            {
                out.push_str(",\"workbookProtect\":");
                out.push_str(&crate::sheet_protect_json::workbook_protect_json(book));
            }
            out.push('}');
            return Ok(out);
        }
        DecodedContent::Slides(slides) => {
            out.push_str("\"slides\":[");
            write_list(&mut out, slides, |out, (title, bullets, notes)| {
                out.push_str("{\"title\":");
                push_json_str(out, title);
                out.push_str(",\"bullets\":[");
                write_list(out, bullets, |out, b| push_json_str(out, b));
                out.push_str("],\"notes\":");
                push_json_str(out, notes);
                out.push('}');
            });
        }
    }
    out.push_str("]}");
    Ok(out)
}

/// A starting capacity for the answer: sheets average about 35 bytes of JSON per cell, documents
/// about 600 bytes per block, so the buffer rarely grows more than once.
fn estimated_len(content: &DecodedContent) -> usize {
    const BYTES_PER_CELL: usize = 35;
    const BYTES_PER_BLOCK: usize = 600;
    const FIXED: usize = 1024;
    FIXED
        + match content {
            DecodedContent::Document(doc) => doc.blocks.len() * BYTES_PER_BLOCK,
            DecodedContent::Workbook(sheets) => sheets
                .iter()
                .map(|(_, cells)| cells.len() * BYTES_PER_CELL)
                .sum(),
            DecodedContent::Slides(slides) => slides.len() * BYTES_PER_BLOCK,
        }
}

fn styling_of<'a>(styling: &'a [(String, SheetStyling)], name: &str) -> Option<&'a SheetStyling> {
    styling.iter().find(|(n, _)| n == name).map(|(_, s)| s)
}

fn into_structured(
    content: DecodedContent,
    styling: &[(String, SheetStyling)],
    pdf: PdfWriteOptions,
) -> StructuredContent {
    match content {
        DecodedContent::Document(mut doc) => {
            for block in &mut doc.blocks {
                if let RichBlock::Table(table) = block {
                    evaluate_table_formulas(&mut table.rows);
                }
            }
            StructuredContent::Document(doc, pdf)
        }
        DecodedContent::Workbook(sheets) => {
            let stylings = sheets
                .iter()
                .map(|(name, _)| styling_of(styling, name).cloned().unwrap_or_default())
                .collect();
            StructuredContent::Workbook(sheets, stylings)
        }
        DecodedContent::Slides(slides) => StructuredContent::Slides(slides),
    }
}

impl EasyOfficeIpcRouter {
    /// Handles a `convert_document_format` request that carries `sourceBase64`.
    pub(crate) fn handle_source_file_request(
        entries: &JsonEntries,
        source_base64: &str,
        writers: ShellWriters<'_>,
    ) -> Result<String, EasyOfficeError> {
        let b64 = source_base64.trim();
        if b64.len() > MAX_SOURCE_BYTES / 3 * 4 + 8 {
            return Err(EasyOfficeError::PayloadTooLarge {
                actual: b64.len() / 4 * 3,
                max: MAX_SOURCE_BYTES,
            });
        }
        let bytes = Self::decode_base64(b64).ok_or_else(|| {
            EasyOfficeError::InvalidArgument("유효하지 않은 Base64 원본 파일입니다.".to_string())
        })?;
        if bytes.len() > MAX_SOURCE_BYTES {
            return Err(EasyOfficeError::PayloadTooLarge {
                actual: bytes.len(),
                max: MAX_SOURCE_BYTES,
            });
        }
        let file_name = Self::optional_json_str(entries, "fileName")?
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let source_hint = match file_name {
            Some(n) => Some(n),
            None => Self::optional_json_str(entries, "sourceFormat")?
                .map(str::trim)
                .filter(|s| !s.is_empty()),
        };
        let (decoded, styling) =
            UniversalFormatConverter::decode_rich_with_styling(&bytes, source_hint)?;
        let action = Self::optional_json_str(entries, "action")?.unwrap_or("convert");
        if action == "decode" {
            let max_rows = limit(entries, "maxRows")?;
            let max_cols = limit(entries, "maxCols")?;
            return decode_response(
                &decoded,
                &styling,
                file_name.unwrap_or(""),
                bytes.len(),
                max_rows,
                max_cols,
            );
        }
        if action != "convert" {
            return Err(EasyOfficeError::InvalidArgument(format!(
                "알 수 없는 action: {action}"
            )));
        }
        let dst_ext = match Self::optional_json_str(entries, "targetFormat")? {
            Some(d) => d,
            None => Self::optional_json_str(entries, "target")?.unwrap_or("hwpx"),
        }
        .trim();
        let dst_fmt = DocumentFormat::from_extension(dst_ext).ok_or_else(|| {
            EasyOfficeError::InvalidArgument(format!("지원하지 않는 대상 포맷: {dst_ext}"))
        })?;
        let fallback_title = Self::optional_json_str(entries, "title")?
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("변환_문서");
        let decoded_title = decoded.title.trim();
        let title = if !decoded_title.is_empty()
            && !matches!(
                decoded_title,
                "Untitled" | "EasyOffice Document" | " EasyOffice Document"
            ) {
            decoded_title.to_string()
        } else {
            fallback_title.to_string()
        };
        let src_fmt = decoded.format;
        let mut warnings = decoded.warnings.clone();
        let content = into_structured(decoded.content, &styling, parse_pdf_options(entries)?);
        if !accepts_loss(entries)? && password_lost(&content, dst_fmt) {
            return Ok(REFUSED_PASSWORD_LOSS.to_string());
        }
        let dst_bytes = encode_source_target(
            &content,
            &bytes,
            dst_fmt,
            src_fmt,
            &title,
            writers,
            &mut warnings,
        )?;
        Self::conversion_response(src_fmt, dst_fmt, bytes.len(), &dst_bytes, &warnings)
    }
}

#[cfg(test)]
mod page_break_tests;
#[cfg(test)]
mod page_frame_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod toc_tests;
