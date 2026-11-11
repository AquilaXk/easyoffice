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
    base64_codec, evaluate_table_formulas, CellMerge, CellStyle, DecodedContent, DecodedDocument,
    DocumentFormat, EasyOfficeError, OdsUnpacked, PdfWriteOptions, RichAlignment, RichBlock,
    RichImage, RichListKind, RichParagraph, RichScript, SheetStyling, StrictJsonValue,
    UniversalFormatConverter,
};
use easyoffice_sheet::PackedCellCoordinate;
use std::collections::HashSet;
use std::fmt::Write;

use crate::convert_structured::{encode_target, RichPdfRenderer, StructuredContent};
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

fn json_str(s: &str) -> String {
    format!("\"{}\"", EasyOfficeIpcRouter::escape_json_str(s))
}

fn paragraph_json(p: &RichParagraph) -> String {
    let mut out = String::from("{\"type\":\"paragraph\",\"runs\":[");
    for (idx, r) in p.runs.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        let _ = write!(out, "{{\"text\":{}", json_str(&r.text));
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
                let _ = write!(out, ",\"{key}\":{}", json_str(v));
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
    out
}

/// Picture block of the editor model: the encoded bytes travel as base64 and come back unchanged.
fn image_json(img: &RichImage) -> String {
    let (width_emu, height_emu) = img.display_emu();
    format!(
        "{{\"type\":\"image\",\"format\":\"{}\",\"base64\":\"{}\",\"widthPx\":{},\"heightPx\":{},\"widthEmu\":{width_emu},\"heightEmu\":{height_emu},\"alt\":{}}}",
        img.format().extension(),
        base64_codec::encode(img.bytes()),
        img.width_px(),
        img.height_px(),
        json_str(img.alt())
    )
}

/// `,"merges":[{"row":0,"col":0,"rowSpan":1,"colSpan":2}]` for a table with merged regions (PD-130),
/// nothing otherwise. Covered cells stay in `rows` as empty text.
fn merges_json(merges: &[CellMerge]) -> String {
    if merges.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(12 + merges.len() * 44);
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
    out
}

fn block_json(b: &RichBlock) -> String {
    match b {
        RichBlock::Paragraph(p) => paragraph_json(p),
        RichBlock::Image(img) => image_json(img),
        RichBlock::Table(table) => {
            let mut out = String::with_capacity(32 + table.rows.len() * 32);
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
                    let _ = write!(out, "{{\"text\":{}}}", json_str(cell));
                }
                out.push(']');
            }
            out.push(']');
            out.push_str(&merges_json(&table.merges));
            out.push('}');
            out
        }
    }
}

/// Keeps cells inside `max_rows` x `max_cols`; returns the number of dropped cells.
fn clip_sheets(sheets: &mut [OdsUnpacked], max_rows: Option<u32>, max_cols: Option<u32>) -> usize {
    let mut dropped = 0usize;
    for (_, cells) in sheets {
        let before = cells.len();
        cells
            .retain(|(r, c, _)| max_rows.is_none_or(|m| *r < m) && max_cols.is_none_or(|m| *c < m));
        dropped += before - cells.len();
    }
    dropped
}

/// `{"bold":true,"sizePt":11.25}`: only the set flags and the explicit size.
fn style_json(style: &CellStyle) -> String {
    let mut out = String::with_capacity(48);
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
        let _ = write!(out, "\"sizePt\":{size}");
    }
    out.push('}');
    out
}

#[inline]
const fn pack_row_col(r: u32, c: u32) -> u64 {
    ((r as u64) << 32) | (c as u64)
}

/// Cells of one sheet as JSON: valued cells first, then styled cells without a value.
fn sheet_cells_json(cells: &[(u32, u32, String)], styling: Option<&SheetStyling>) -> Vec<String> {
    let style_of = |r: u32, c: u32| styling.and_then(|s| s.cells.get(&(r, c)));
    let mut valued: HashSet<u64> = HashSet::with_capacity(cells.len());
    let mut out: Vec<String> = Vec::with_capacity(cells.len());
    for (r, c, raw) in cells {
        valued.insert(pack_row_col(*r, *c));
        if let Ok(coord) = PackedCellCoordinate::new(*r, *c) {
            let a1 = coord.to_a1();
            let mut item = String::with_capacity(32 + raw.len());
            let _ = write!(item, "{{\"ref\":\"{a1}\",\"raw\":{}", json_str(raw));
            if let Some(s) = style_of(*r, *c) {
                let _ = write!(item, ",\"style\":{}", style_json(s));
            }
            item.push('}');
            out.push(item);
        }
    }
    if let Some(st) = styling {
        for (&(r, c), style) in &st.cells {
            if valued.contains(&pack_row_col(r, c)) {
                continue;
            }
            if let Ok(coord) = PackedCellCoordinate::new(r, c) {
                out.push(format!(
                    "{{\"ref\":\"{}\",\"raw\":\"\",\"style\":{}}}",
                    coord.to_a1(),
                    style_json(style)
                ));
            }
        }
    }
    out
}

/// Custom column widths as JSON (1-based `index`).
fn columns_json(styling: Option<&SheetStyling>) -> String {
    let Some(s) = styling else {
        return String::new();
    };
    if s.columns.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(s.columns.len() * 28);
    for (idx, (c, px)) in s.columns.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        let _ = write!(out, "{{\"index\":{},\"widthPx\":{px}}}", c + 1);
    }
    out
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
    let (kind, payload, count) = match &decoded.content {
        DecodedContent::Document(doc) => {
            let blocks: Vec<String> = doc.blocks.iter().map(block_json).collect();
            (
                "document",
                format!("\"blocks\":[{}]", blocks.join(",")),
                doc.blocks.len(),
            )
        }
        DecodedContent::Workbook(sheets) => {
            let mut sheets = sheets.clone();
            let dropped = clip_sheets(&mut sheets, max_rows, max_cols);
            if dropped > 0 {
                warnings.push(format!(
                    "편집기 격자(최대 {}행 × {}열) 밖의 셀 {dropped}개는 가져오지 않았습니다.",
                    max_rows.map_or("무제한".to_string(), |m| m.to_string()),
                    max_cols.map_or("무제한".to_string(), |m| m.to_string()),
                ));
            }
            let count = sheets.len();
            let items: Vec<String> = sheets
                .iter()
                .map(|(name, cells)| {
                    let mut sheet_styling = styling_of(styling, name).cloned();
                    if let Some(st) = sheet_styling.as_mut() {
                        st.cells.retain(|(r, c), _| {
                            max_rows.is_none_or(|m| *r < m) && max_cols.is_none_or(|m| *c < m)
                        });
                        st.columns.retain(|c, _| max_cols.is_none_or(|m| *c < m));
                    }
                    let cells = sheet_cells_json(cells, sheet_styling.as_ref());
                    format!(
                        "{{\"name\":{},\"cells\":[{}],\"columns\":[{}]}}",
                        json_str(name),
                        cells.join(","),
                        columns_json(sheet_styling.as_ref())
                    )
                })
                .collect();
            (
                "workbook",
                format!("\"sheets\":[{}]", items.join(",")),
                count,
            )
        }
        DecodedContent::Slides(slides) => {
            let items: Vec<String> = slides
                .iter()
                .map(|(title, bullets, notes)| {
                    let bullets: Vec<String> = bullets.iter().map(|b| json_str(b)).collect();
                    format!(
                        "{{\"title\":{},\"bullets\":[{}],\"notes\":{}}}",
                        json_str(title),
                        bullets.join(","),
                        json_str(notes)
                    )
                })
                .collect();
            (
                "slides",
                format!("\"slides\":[{}]", items.join(",")),
                slides.len(),
            )
        }
    };
    let warnings_json: Vec<String> = warnings.iter().map(|w| json_str(w)).collect();
    Ok(format!(
        concat!(
            "{{\"status\":\"OK\",\"action\":\"decode\",\"fileName\":{},\"format\":\"{}\",",
            "\"module\":\"{}\",\"kind\":\"{}\",\"title\":{},\"sourceBytes\":{},",
            "\"specification\":{},\"itemCount\":{},\"warnings\":[{}],{}}}"
        ),
        json_str(file_name),
        decoded.format.extension(),
        decoded.format.editor_module(),
        kind,
        json_str(&decoded.title),
        source_bytes,
        json_str(decoded.format.standard_specification()),
        count,
        warnings_json.join(","),
        payload
    ))
}

fn styling_of<'a>(styling: &'a [(String, SheetStyling)], name: &str) -> Option<&'a SheetStyling> {
    styling.iter().find(|(n, _)| n == name).map(|(_, s)| s)
}

fn into_structured(
    content: DecodedContent,
    styling: &[(String, SheetStyling)],
) -> StructuredContent {
    match content {
        DecodedContent::Document(mut doc) => {
            for block in &mut doc.blocks {
                if let RichBlock::Table(table) = block {
                    evaluate_table_formulas(&mut table.rows);
                }
            }
            StructuredContent::Document(doc, PdfWriteOptions::default())
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
        renderer: Option<&RichPdfRenderer<'_>>,
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
        let content = into_structured(decoded.content, &styling);
        let dst_bytes = encode_target(&content, dst_fmt, src_fmt, &title, renderer, &mut warnings)?;
        Self::conversion_response(src_fmt, dst_fmt, &bytes, &dst_bytes, &warnings)
    }
}

#[cfg(test)]
mod tests;
