//! JSON encoding and strict parsing for rich table widths, cell styles, and inline cell images.

use std::fmt::Write;

use easyoffice_formats::{
    base64_codec, evaluate_table_formulas, safe_link_target, CellFill, CellMerge, EasyOfficeError,
    ImageBudget, RichImage, RichTable, StrictJsonValue, TableBorderEdge, TableBorderKind,
    TableCellBorders, TableCellImage, TableCellLink, TableCellStyleEntry, MAX_COL_WIDTH_TWIPS,
    MAX_IMAGES_PER_DOCUMENT, MAX_IMAGE_BYTES, MAX_STYLED_TABLE_CELLS, MAX_TABLE_LINKS,
    MIN_COL_WIDTH_TWIPS,
};

#[path = "convert_structured_table_cells.rs"]
mod convert_structured_table_cells;

use super::{as_array, as_object, field, invalid, opt_bounded, opt_str, Budget, JsonObject};
use crate::EasyOfficeIpcRouter;

const MAX_COLUMN_WIDTH_ENTRIES: usize = 16_384;

/// Appends common image JSON fields (`"format":...,"alt":"..."`) without outer braces.
pub(crate) fn write_image_fields(out: &mut String, img: &RichImage) {
    let (width_emu, height_emu) = img.display_emu();
    let _ = write!(
        out,
        "\"format\":\"{}\",\"base64\":\"{}\",\"widthPx\":{},\"heightPx\":{},\"widthEmu\":{width_emu},\"heightEmu\":{height_emu},\"alt\":\"",
        img.format().extension(),
        base64_codec::encode(img.bytes()),
        img.width_px(),
        img.height_px(),
    );
    EasyOfficeIpcRouter::escape_json_into(out, img.alt());
    out.push('"');
}

/// Appends optional `colWidthsTwips`, `cellStyles`, and `cellImages` JSON properties to an open
/// table object in `decode` responses.
pub(crate) fn write_table_extras(out: &mut String, table: &RichTable) {
    if let Some(widths) = table.validated_col_widths() {
        out.push_str(",\"colWidthsTwips\":[");
        for (idx, w) in widths.iter().enumerate() {
            if idx > 0 {
                out.push(',');
            }
            let _ = write!(out, "{w}");
        }
        out.push(']');
    }
    let styles = table.validated_cell_styles();
    if !styles.is_empty() {
        out.push_str(",\"cellStyles\":[");
        for (idx, entry) in styles.iter().enumerate() {
            if idx > 0 {
                out.push(',');
            }
            let _ = write!(out, "{{\"row\":{},\"col\":{}", entry.row, entry.col);
            if !entry.fill.is_none() {
                out.push_str(",\"fill\":\"");
                entry.fill.write_hex(out);
                out.push('"');
            }
            if !entry.borders.is_unspecified() {
                write_borders_json(out, entry.borders);
            }
            out.push('}');
        }
        out.push(']');
    }
    let images = table.validated_cell_images();
    if !images.is_empty() {
        out.push_str(",\"cellImages\":[");
        for (idx, ci) in images.iter().enumerate() {
            if idx > 0 {
                out.push(',');
            }
            let _ = write!(out, "{{\"row\":{},\"col\":{},", ci.row, ci.col);
            write_image_fields(out, &ci.image);
            out.push('}');
        }
        out.push(']');
    }
    convert_structured_table_cells::write_cell_paragraphs_json(out, table);
}

fn write_borders_json(out: &mut String, borders: TableCellBorders) {
    out.push_str(",\"borders\":{");
    let mut first = true;
    for (side, edge) in [
        ("top", borders.top),
        ("right", borders.right),
        ("bottom", borders.bottom),
        ("left", borders.left),
    ] {
        let Some(kind) = edge.kind().wire_name() else {
            continue;
        };
        if !first {
            out.push(',');
        }
        first = false;
        let _ = write!(out, "\"{side}\":{{\"kind\":\"{kind}\"");
        if edge.kind() != TableBorderKind::None {
            out.push_str(",\"color\":\"");
            edge.write_hex(out);
            out.push('"');
        }
        out.push('}');
    }
    out.push('}');
}

/// Picture block or cell picture: enforces decoded/document image budgets Fail-Closed before use.
pub(super) fn parse_image_budgeted(
    obj: &JsonObject,
    img_budget: &mut ImageBudget,
) -> Result<RichImage, EasyOfficeError> {
    let encoded = opt_str(obj, "base64")?
        .ok_or_else(|| invalid("구조화 저장: 그림에는 'base64'가 필요합니다."))?;
    if encoded.len() / 4 * 3 > MAX_IMAGE_BYTES + 3 {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "그림 크기가 한도({MAX_IMAGE_BYTES}바이트)를 넘었습니다."
        )));
    }
    let bytes = base64_codec::decode(encoded)
        .ok_or_else(|| invalid("구조화 저장: 그림 'base64'가 올바르지 않습니다."))?;
    img_budget.charge(bytes.len())?;
    let mut image = RichImage::new(bytes, opt_str(obj, "alt")?.unwrap_or(""))?;
    let width = opt_bounded(obj, "widthEmu", 1, i64::MAX)?;
    let height = opt_bounded(obj, "heightEmu", 1, i64::MAX)?;
    if let (Some(w), Some(h)) = (width, height) {
        image.set_display_emu(w as u64, h as u64)?;
    }
    Ok(image)
}

pub(super) fn req_u32(obj: &JsonObject, key: &str, what: &str) -> Result<u32, EasyOfficeError> {
    field(obj, key)
        .and_then(StrictJsonValue::as_u64)
        .map(|n| u32::try_from(n).unwrap_or(u32::MAX))
        .ok_or_else(|| {
            invalid(format!(
                "구조화 저장: {what} '{key}'는 0 이상의 정수여야 합니다."
            ))
        })
}

/// `{"row","col","rowSpan","colSpan"}`: non-negative integers. Values past `u32` saturate, so the
/// table's own validation drops them like any other region outside the grid.
fn parse_merge(item: &StrictJsonValue) -> Result<CellMerge, EasyOfficeError> {
    let obj = as_object(item, "merges")?;
    Ok(CellMerge {
        row: req_u32(obj, "row", "병합의")?,
        col: req_u32(obj, "col", "병합의")?,
        row_span: req_u32(obj, "rowSpan", "병합의")?,
        col_span: req_u32(obj, "colSpan", "병합의")?,
    })
}

fn parse_table_link(item: &StrictJsonValue) -> Result<TableCellLink, EasyOfficeError> {
    let obj = as_object(item, "links")?;
    let raw_url = opt_str(obj, "url")?
        .filter(|u| !u.is_empty())
        .ok_or_else(|| invalid("구조화 저장: 표 링크에는 'url'이 필요합니다."))?;
    let url = safe_link_target(raw_url)
        .ok_or_else(|| invalid("구조화 저장: 허용되지 않는 표 링크 주소입니다."))?
        .to_string();
    Ok(TableCellLink {
        row: req_u32(obj, "row", "표 링크의")?,
        col: req_u32(obj, "col", "표 링크의")?,
        start: req_u32(obj, "start", "표 링크의")?,
        end: req_u32(obj, "end", "표 링크의")?,
        url,
    })
}

fn parse_col_widths(
    obj: &JsonObject,
    max_cols: usize,
    warnings: &mut Vec<String>,
) -> Result<Vec<u32>, EasyOfficeError> {
    let Some(v) = field(obj, "colWidthsTwips") else {
        return Ok(Vec::new());
    };
    let arr = as_array(v, "colWidthsTwips")?;
    if arr.len() > MAX_COLUMN_WIDTH_ENTRIES {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "표 열 너비 항목 수가 한도({MAX_COLUMN_WIDTH_ENTRIES})를 초과했습니다."
        )));
    }
    let mut widths = Vec::with_capacity(arr.len());
    for item in arr {
        let w = item.as_u64().ok_or_else(|| {
            invalid("구조화 저장: 'colWidthsTwips' 항목은 0 이상의 정수여야 합니다.")
        })?;
        let clamped = w.clamp(
            u64::from(MIN_COL_WIDTH_TWIPS),
            u64::from(MAX_COL_WIDTH_TWIPS),
        ) as u32;
        widths.push(clamped);
    }
    if max_cols == 0 || widths.len() != max_cols {
        if !widths.is_empty() {
            warnings.push("표의 열 너비 수가 열 수와 달라 저장하지 않았습니다.".to_string());
        }
        return Ok(Vec::new());
    }
    Ok(widths)
}

fn parse_border_edge(v: &StrictJsonValue, side: &str) -> Result<TableBorderEdge, EasyOfficeError> {
    match v {
        StrictJsonValue::Null => Ok(TableBorderEdge::UNSPECIFIED),
        _ if v.as_str().is_some() => {
            let s = v.as_str().unwrap_or("").trim();
            if s.is_empty() || s == "-" || s == "unspecified" {
                return Ok(TableBorderEdge::UNSPECIFIED);
            }
            if s == "none" {
                return Ok(TableBorderEdge::NONE);
            }
            let (raw_kind, raw_color) = s.split_once(':').unwrap_or((s, ""));
            let kind = TableBorderKind::from_wire_name(raw_kind).ok_or_else(|| {
                invalid(format!(
                    "구조화 저장: 알 수 없는 셀 테두리 종류 '{raw_kind}'"
                ))
            })?;
            if kind == TableBorderKind::None {
                return Ok(TableBorderEdge::NONE);
            }
            let rgb = if raw_color.trim().is_empty() {
                0
            } else {
                CellFill::from_hex(raw_color)
                    .and_then(CellFill::rgb)
                    .ok_or_else(|| {
                        invalid(format!("구조화 저장: 잘못된 테두리 색 '{raw_color}'"))
                    })?
            };
            Ok(TableBorderEdge::new(kind, rgb))
        }
        _ => {
            let obj = as_object(v, side)?;
            let kind_str = match opt_str(obj, "kind")? {
                Some(k) => Some(k),
                None => opt_str(obj, "line")?,
            };
            match kind_str.map(str::trim) {
                None | Some("") | Some("unspecified") => Ok(TableBorderEdge::UNSPECIFIED),
                Some(raw_kind) => {
                    let kind = TableBorderKind::from_wire_name(raw_kind).ok_or_else(|| {
                        invalid(format!(
                            "구조화 저장: 알 수 없는 셀 테두리 종류 '{raw_kind}'"
                        ))
                    })?;
                    if kind == TableBorderKind::None {
                        return Ok(TableBorderEdge::NONE);
                    }
                    let rgb =
                        match opt_str(obj, "color")?
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                        {
                            None => 0,
                            Some(hex) => {
                                CellFill::from_hex(hex).and_then(CellFill::rgb).ok_or_else(
                                    || invalid(format!("구조화 저장: 잘못된 테두리 색 '{hex}'")),
                                )?
                            }
                        };
                    Ok(TableBorderEdge::new(kind, rgb))
                }
            }
        }
    }
}

fn parse_cell_styles(
    obj: &JsonObject,
    rows: &[Vec<String>],
    budget: &mut Budget,
    warnings: &mut Vec<String>,
) -> Result<Vec<TableCellStyleEntry>, EasyOfficeError> {
    let Some(v) = field(obj, "cellStyles") else {
        return Ok(Vec::new());
    };
    let arr = as_array(v, "cellStyles")?;
    if arr.len() > MAX_STYLED_TABLE_CELLS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "표 셀 서식 수가 한도({MAX_STYLED_TABLE_CELLS})를 초과했습니다."
        )));
    }
    let mut out = Vec::with_capacity(arr.len());
    let mut dropped = 0usize;
    for item in arr {
        budget.take(1)?;
        let cell_obj = as_object(item, "cellStyles[]")?;
        let row = req_u32(cell_obj, "row", "셀 서식의")?;
        let col = req_u32(cell_obj, "col", "셀 서식의")?;
        let fill = match opt_str(cell_obj, "fill")?
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            None => CellFill::NONE,
            Some(hex) => CellFill::from_hex(hex)
                .ok_or_else(|| invalid(format!("구조화 저장: 잘못된 셀 배경색 '{hex}'")))?,
        };
        let mut borders = TableCellBorders::default();
        if let Some(bv) = field(cell_obj, "borders") {
            let b_obj = as_object(bv, "cellStyles[].borders")?;
            if let Some(e) = field(b_obj, "top") {
                borders.top = parse_border_edge(e, "borders.top")?;
            }
            if let Some(e) = field(b_obj, "right") {
                borders.right = parse_border_edge(e, "borders.right")?;
            }
            if let Some(e) = field(b_obj, "bottom") {
                borders.bottom = parse_border_edge(e, "borders.bottom")?;
            }
            if let Some(e) = field(b_obj, "left") {
                borders.left = parse_border_edge(e, "borders.left")?;
            }
        }
        let in_bounds = rows
            .get(row as usize)
            .is_some_and(|r| (col as usize) < r.len());
        if !in_bounds {
            dropped += 1;
            continue;
        }
        if !fill.is_none() || !borders.is_unspecified() {
            out.push(TableCellStyleEntry {
                row,
                col,
                fill,
                borders,
            });
        }
    }
    if dropped > 0 {
        warnings.push(format!(
            "표의 셀 서식 {dropped}곳은 범위를 벗어나 저장하지 않았습니다."
        ));
    }
    Ok(out)
}

fn parse_cell_images(
    obj: &JsonObject,
    rows: &[Vec<String>],
    budget: &mut Budget,
    img_budget: &mut ImageBudget,
    warnings: &mut Vec<String>,
) -> Result<Vec<TableCellImage>, EasyOfficeError> {
    let Some(v) = field(obj, "cellImages") else {
        return Ok(Vec::new());
    };
    let arr = as_array(v, "cellImages")?;
    if arr.len() > MAX_IMAGES_PER_DOCUMENT {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "표 셀 그림 수가 한도({MAX_IMAGES_PER_DOCUMENT})를 초과했습니다."
        )));
    }
    let mut out = Vec::with_capacity(arr.len());
    let mut dropped = 0usize;
    for item in arr {
        budget.take(1)?;
        let item_obj = as_object(item, "cellImages[]")?;
        let row = req_u32(item_obj, "row", "셀 그림의")?;
        let col = req_u32(item_obj, "col", "셀 그림의")?;
        let image = parse_image_budgeted(item_obj, img_budget)?;
        let in_bounds = rows
            .get(row as usize)
            .is_some_and(|r| (col as usize) < r.len());
        if in_bounds {
            out.push(TableCellImage { row, col, image });
        } else {
            dropped += 1;
        }
    }
    if dropped > 0 {
        warnings.push(format!(
            "표의 셀 그림 {dropped}개는 범위를 벗어나 저장하지 않았습니다."
        ));
    }
    Ok(out)
}

/// Parses a structured table block including widths, styles, and cell images.
pub(super) fn parse_table(
    obj: &JsonObject,
    budget: &mut Budget,
    img_budget: &mut ImageBudget,
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
    let raw_eq_cells: Vec<(usize, usize, String)> = if field(obj, "cellParagraphs").is_some() {
        rows.iter()
            .enumerate()
            .flat_map(|(r, row)| {
                row.iter()
                    .enumerate()
                    .filter(|(_, cell)| cell.trim_start().starts_with('='))
                    .map(move |(c, cell)| (r, c, cell.clone()))
            })
            .collect()
    } else {
        Vec::new()
    };
    evaluate_table_formulas(&mut rows);
    let formula_cells: Vec<(u32, u32)> = raw_eq_cells
        .into_iter()
        .filter(|(r, c, raw)| rows.get(*r).and_then(|row| row.get(*c)) != Some(raw))
        .map(|(r, c, _)| (r as u32, c as u32))
        .collect();
    let mut merges = Vec::new();
    if let Some(v) = field(obj, "merges") {
        for item in as_array(v, "merges")? {
            budget.take(1)?;
            merges.push(parse_merge(item)?);
        }
    }
    let mut links = Vec::new();
    if let Some(v) = field(obj, "links") {
        let arr = as_array(v, "links")?;
        if arr.len() > MAX_TABLE_LINKS {
            return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
                "표 셀 링크 수가 한도({MAX_TABLE_LINKS})를 초과했습니다."
            )));
        }
        for item in arr {
            budget.take(1)?;
            links.push(parse_table_link(item)?);
        }
    }
    let (mut table, dropped, dropped_links) = RichTable::with_merges_and_links(rows, merges, links);
    if dropped > 0 {
        warnings.push(format!(
            "표의 병합 {dropped}곳은 범위를 벗어나거나 겹쳐 저장하지 않았습니다."
        ));
    }
    if dropped_links > 0 {
        warnings.push(format!(
            "표의 셀 링크 {dropped_links}곳은 범위를 벗어나거나 겹쳐 저장하지 않았습니다."
        ));
    }
    let max_cols = table.column_count();
    table.col_widths_twips = parse_col_widths(obj, max_cols, warnings)?;
    table.cell_styles = parse_cell_styles(obj, &table.rows, budget, warnings)?;
    table.cell_images = parse_cell_images(obj, &table.rows, budget, img_budget, warnings)?;
    table.cell_paragraphs = convert_structured_table_cells::parse_cell_paragraphs(
        obj,
        &table.rows,
        &formula_cells,
        budget,
        warnings,
    )?;
    Ok(table)
}
