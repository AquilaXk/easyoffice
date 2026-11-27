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

use easyoffice_formats::native_package::PackageKind;
use easyoffice_formats::{
    evaluate_table_formulas, resolve_code, resolve_font_name, CellMerge, CellStyle, DecodedContent,
    DecodedDocument, DocumentFormat, EasyOfficeError, OdsUnpacked, PdfWriteOptions, RichAlignment,
    RichBlock, RichField, RichImage, RichListKind, RichParagraph, RichRun, RichScript,
    RichSlideDeck, RichToc, SheetStyling, StrictJsonValue, TableCellLink, UniversalFormatConverter,
    CUSTOM_BASE,
};
use easyoffice_sheet::PackedCellCoordinate;
use std::collections::HashSet;
use std::fmt::Write;

use crate::convert_hwp::ShellWriters;
use crate::convert_password::{accepts_loss, password_lost, REFUSED_PASSWORD_LOSS};
use crate::convert_structured::convert_structured_table::{write_image_fields, write_table_extras};
use crate::convert_structured::{
    convert_structured_defaults, convert_structured_notes, encode_source_target, StructuredContent,
};
use crate::convert_structured_doc::parse_pdf_options;
use crate::line_spacing_json::write_line_spacing_json;
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

use easyoffice_formats::json_out::push_json_string as push_json_str;

pub(crate) fn write_runs_json(out: &mut String, runs: &[RichRun]) {
    for (idx, r) in runs.iter().enumerate() {
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
        if let Some(spacing) = r.letter_spacing_twips {
            let _ = write!(out, ",\"letterSpacingTwips\":{spacing}");
        }
        r.advanced.write_json(out);
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
            (&r.font_face, "fontFace"),
            (&r.color, "color"),
            (&r.highlight, "highlight"),
        ] {
            if let Some(v) = value {
                let _ = write!(out, ",\"{key}\":");
                push_json_str(out, v);
            }
        }
        convert_structured_notes::write_run_annotations(out, r);
        out.push('}');
    }
}

fn write_paragraph(out: &mut String, p: &RichParagraph) {
    out.push_str("{\"type\":\"paragraph\",\"runs\":[");
    write_runs_json(out, &p.runs);
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
    if let Some(copy) = &p.frame_copy {
        if copy.is_empty() {
            out.push_str(",\"frameCopy\":true");
        } else {
            out.push_str(",\"frameCopy\":");
            push_json_str(out, copy);
        }
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
            RichListKind::Task => "task",
        };
        let _ = write!(out, ",\"list\":\"{token}\"");
        if l == RichListKind::Task {
            let _ = write!(out, ",\"checked\":{}", p.task_checked);
        }
        if p.list_level > 0 {
            let _ = write!(out, ",\"listLevel\":{}", p.list_level.min(8));
        }
        if l == RichListKind::Ordered {
            if let Some(num) = p
                .list_number
                .filter(|&n| n != easyoffice_formats::RichListNumber::default())
            {
                let _ = write!(out, ",\"listNumber\":\"{}\"", num.to_wire_code());
            }
        }
    }
    if let Some(cols) = p.columns.filter(|c| c.count > 1) {
        let _ = write!(
            out,
            ",\"columns\":{{\"count\":{},\"equal\":{},\"gapTwips\":{}}}",
            cols.count, cols.equal, cols.gap_twips
        );
    }
    if let Some(v) = p.left_indent_twips {
        let _ = write!(out, ",\"leftIndentTwips\":{v}");
    }
    if let Some(v) = p.right_indent_twips {
        let _ = write!(out, ",\"rightIndentTwips\":{v}");
    }
    if let Some(v) = p.first_line_indent_twips {
        let _ = write!(out, ",\"firstLineIndentTwips\":{v}");
    }
    if let Some(spacing) = p.line_spacing {
        write_line_spacing_json(out, spacing);
    }
    if let Some(v) = p.space_before_twips {
        let _ = write!(out, ",\"spaceBeforeTwips\":{v}");
    }
    if let Some(v) = p.space_after_twips {
        let _ = write!(out, ",\"spaceAfterTwips\":{v}");
    }
    out.push('}');
}

/// Picture block of the editor model: the encoded bytes travel as base64 and come back unchanged.
fn write_image(out: &mut String, img: &RichImage) {
    out.push_str("{\"type\":\"image\",");
    write_image_fields(out, img);
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

fn write_table_links(out: &mut String, links: &[TableCellLink]) {
    if links.is_empty() {
        return;
    }
    out.push_str(",\"links\":[");
    for (idx, l) in links.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"row\":{},\"col\":{},\"start\":{},\"end\":{},\"url\":",
            l.row, l.col, l.start, l.end
        );
        push_json_str(out, &l.url);
        out.push('}');
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
            write_table_links(out, &table.links);
            write_table_extras(out, table);
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

/// The non-default font family names a sheet's cells use, in first-use order, with an `O(1)` lookup
/// from `CellStyle::font` to its 1-based `"fn"` entry (PD-244).
struct FontIndex<'a> {
    names: Vec<&'a str>,
    entry_of: Vec<u16>,
}

impl<'a> FontIndex<'a> {
    fn build(styling: Option<&'a SheetStyling>) -> Self {
        let mut index = Self {
            names: Vec::new(),
            entry_of: Vec::new(),
        };
        let Some(styling) = styling.filter(|s| !s.fonts.is_empty()) else {
            return index;
        };
        let slots = styling.fonts.len() + 1;
        for style in styling.cells.values() {
            let id = usize::from(style.font);
            if id == 0 || id >= slots {
                continue;
            }
            if index.entry_of.is_empty() {
                index.entry_of = vec![0; slots];
            }
            if index.entry_of[id] != 0 {
                continue;
            }
            if let Some(name) = resolve_font_name(style.font, &styling.fonts) {
                index.names.push(name);
                index.entry_of[id] = index.names.len() as u16;
            }
        }
        index
    }

    fn entry(&self, font: u16) -> Option<u16> {
        self.entry_of
            .get(usize::from(font))
            .copied()
            .filter(|&e| e != 0)
    }
}

/// `{"bold":true,"sizePt":11.25,"nf":2,"fn":1}`: only the set flags, explicit size, format and font.
fn write_style(
    out: &mut String,
    style: &CellStyle,
    num_fmts: &NumFmtIndex<'_>,
    fonts: &FontIndex<'_>,
) {
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
    if let Some(entry) = fonts.entry(style.font) {
        if !first {
            out.push(',');
        }
        first = false;
        let _ = write!(out, "\"fn\":{entry}");
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
    fonts: &FontIndex<'_>,
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
        if let Some(kind) = styling.and_then(|s| s.explicit_cell_kind(*r, *c, raw)) {
            let _ = write!(out, ",\"t\":\"{}\"", kind.tag());
        }
        if let Some(ph) = styling.and_then(|s| s.cell_phonetic(*r, *c, raw)) {
            out.push_str(",\"ph\":");
            crate::sheet_phonetic_json::write_cell_phonetic_json(out, ph);
        }
        if let Some(style) = style {
            out.push_str(",\"style\":");
            write_style(out, style, num_fmts, fonts);
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
        write_style(out, style, num_fmts, fonts);
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
    let fonts = FontIndex::build(styling);
    write_sheet_cells(out, cells, styling, &num_fmts, &fonts, max_rows, max_cols);
    out.push_str("],\"columns\":[");
    write_columns(out, styling, max_cols);
    out.push(']');
    if !num_fmts.codes.is_empty() {
        out.push_str(",\"numFmts\":[");
        write_list(out, &num_fmts.codes, |out, code| push_json_str(out, code));
        out.push(']');
    }
    if !fonts.names.is_empty() {
        out.push_str(",\"fonts\":[");
        write_list(out, &fonts.names, |out, face| push_json_str(out, face));
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
            Some(split) => crate::sheet_layout_json::write_layout_split(
                out,
                st,
                &split.hidden,
                split.filter.as_ref(),
                &split.tables,
            ),
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

#[cfg(test)]
fn decode_response(
    decoded: &DecodedDocument,
    styling: &[(String, SheetStyling)],
    file_name: &str,
    source_bytes: usize,
    max_rows: Option<u32>,
    max_cols: Option<u32>,
) -> Result<String, EasyOfficeError> {
    decode_response_with_deck(
        decoded,
        styling,
        None,
        file_name,
        source_bytes,
        max_rows,
        max_cols,
    )
}

fn decode_response_with_deck(
    decoded: &DecodedDocument,
    styling: &[(String, SheetStyling)],
    slide_deck: Option<&RichSlideDeck>,
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
                if st.fonts_rejected > 0 {
                    warnings.push(format!(
                        "글꼴 이름 {}개는 한도를 넘거나 올바르지 않아 기본 글꼴로 가져왔습니다.",
                        st.fonts_rejected
                    ));
                }
                push_rejected_object_warnings(name, st, &mut warnings);
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
            if doc.page_frame.first_page_different {
                out.push_str("\"firstPageDifferent\":true,");
            }
            if doc.page_frame.even_odd_different {
                out.push_str("\"evenOddDifferent\":true,");
            }
            out.push_str("\"blocks\":[");
            write_list(&mut out, &doc.blocks, write_block);
            for (key, paragraphs) in [
                ("header", &doc.page_frame.header),
                ("footer", &doc.page_frame.footer),
                ("firstHeader", &doc.page_frame.first_header),
                ("firstFooter", &doc.page_frame.first_footer),
                ("evenHeader", &doc.page_frame.even_header),
                ("evenFooter", &doc.page_frame.even_footer),
            ] {
                if !paragraphs.is_empty() {
                    let _ = write!(out, "],\"{key}\":[");
                    write_list(&mut out, paragraphs, write_paragraph);
                }
            }
            out.push(']');
            convert_structured_notes::write_doc_root_notes(&mut out, doc);
            convert_structured_defaults::write_doc_defaults(&mut out, doc);
            out.push('}');
            return Ok(out);
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
            if let Some(theme) = styling
                .first()
                .and_then(|(_, st)| st.workbook_theme.as_ref())
            {
                out.push_str(",\"workbookTheme\":");
                crate::sheet_table_json::write_workbook_theme(&mut out, theme);
            }
            if styling.first().is_some_and(|(_, st)| st.date1904) {
                out.push_str(",\"date1904\":true");
            }
            if let Some((_, first)) = styling.first() {
                crate::sheet_default_font_json::write_default_font(&mut out, first);
                crate::sheet_calc_options_json::write_calc_options(&mut out, first);
            }
            out.push('}');
            return Ok(out);
        }
        DecodedContent::Slides(slides) => {
            if let Some(deck) = slide_deck {
                crate::slide_deck_json::write_slide_deck_decode_fields(
                    &mut out,
                    deck,
                    push_json_str,
                );
            } else {
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

/// Adds the lines for the charts, pivot tables, pictures and notes a reader turned away from `st`, the styling of sheet `name`.
pub(crate) fn push_rejected_object_warnings(
    name: &str,
    st: &SheetStyling,
    warnings: &mut Vec<String>,
) {
    for (what, n, why) in [
        ("차트", st.charts_rejected, "한도를 넘거나 올바르지 않아"),
        (
            "피벗 테이블",
            st.pivots_rejected,
            "한도를 넘거나 올바르지 않아",
        ),
        (
            "그림",
            st.images_rejected,
            "지원하지 않는 형식이거나 손상되어",
        ),
        ("메모", st.notes_rejected, "한도를 넘거나 올바르지 않아"),
    ] {
        if n > 0 {
            warnings.push(format!(
                "{name} 시트의 {what} {n}개는 {why} 가져오지 않았습니다."
            ));
        }
    }
}

fn styling_of<'a>(styling: &'a [(String, SheetStyling)], name: &str) -> Option<&'a SheetStyling> {
    styling.iter().find(|(n, _)| n == name).map(|(_, s)| s)
}

fn into_structured(
    content: DecodedContent,
    styling: &[(String, SheetStyling)],
    pdf: PdfWriteOptions,
    slide_deck: Option<RichSlideDeck>,
) -> StructuredContent {
    match (content, slide_deck) {
        (DecodedContent::Document(mut doc), _) => {
            for block in &mut doc.blocks {
                if let RichBlock::Table(table) = block {
                    evaluate_table_formulas(&mut table.rows);
                }
            }
            StructuredContent::Document(doc, pdf)
        }
        (DecodedContent::Workbook(sheets), _) => {
            let stylings = sheets
                .iter()
                .map(|(name, _)| styling_of(styling, name).cloned().unwrap_or_default())
                .collect();
            StructuredContent::Workbook(sheets, stylings)
        }
        (DecodedContent::Slides(_), Some(deck)) => StructuredContent::SlideDeck(deck),
        (DecodedContent::Slides(slides), None) => StructuredContent::Slides(slides),
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
        let action = Self::optional_json_str(entries, "action")?.unwrap_or("convert");
        if crate::readback::is_inspect_action(Some(action)) {
            let query = crate::readback::ReadbackQuery::from_entries(entries)?;
            return crate::readback::inspect_source_bytes(&bytes, file_name, source_hint, &query);
        }
        if action == "decode" {
            if let Some(kind) = file_name.and_then(PackageKind::from_file_name) {
                return Self::decode_native_package(kind, &bytes);
            }
            let max_rows = limit(entries, "maxRows")?;
            let max_cols = limit(entries, "maxCols")?;
            let (decoded, styling, slide_deck) =
                UniversalFormatConverter::decode_rich_full(&bytes, source_hint)?;
            let mut out = decode_response_with_deck(
                &decoded,
                &styling,
                slide_deck.as_ref(),
                file_name.unwrap_or(""),
                bytes.len(),
                max_rows,
                max_cols,
            )?;
            if decoded.format == DocumentFormat::Pdf {
                crate::readback::doc_slide_pdf::append_pdf_outlines_json(&mut out, &bytes);
            }
            return Ok(out);
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
        let pdf_options = parse_pdf_options(entries)?;
        let allow_loss = accepts_loss(entries)?;
        let fallback_title = Self::optional_json_str(entries, "title")?
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("변환_문서");
        let (decoded, styling, slide_deck) =
            UniversalFormatConverter::decode_rich_full(&bytes, source_hint)?;
        let decoded_title = decoded.title.trim();
        let title = if !decoded_title.is_empty()
            && !matches!(
                decoded_title,
                "Untitled" | "EasyOffice Document" | " EasyOffice Document"
            ) {
            decoded_title.to_string()
        } else {
            slide_deck
                .as_ref()
                .map(|d| d.title.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| fallback_title.to_string())
        };
        let src_fmt = decoded.format;
        let mut warnings = decoded.warnings.clone();
        let content = into_structured(decoded.content, &styling, pdf_options, slide_deck);
        if !allow_loss && password_lost(&content, dst_fmt) {
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
        Self::conversion_response(
            src_fmt,
            dst_fmt,
            bytes.len(),
            &dst_bytes,
            &warnings,
            writers.hwp.is_some(),
        )
    }
}

#[cfg(test)]
mod char_advanced_tests;
#[cfg(test)]
mod char_open_type_tests;
#[cfg(test)]
mod font_face_tests;
#[cfg(test)]
mod page_break_tests;
#[cfg(test)]
mod page_frame_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod toc_tests;
