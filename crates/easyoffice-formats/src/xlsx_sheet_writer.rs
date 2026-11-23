//! Serialization of XLSX worksheet XML parts (OOXML ECMA-376 Part 1 §18.3).
//!
//! Cells, rows, styles, outlines, formula results and filters are serialized.
//! Empty tail rows up to 1,048,575 caused by full-column filters or sheet-wide unlocking
//! are clamped to the effective sheet bounds to prevent unbounded empty row allocations.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write;

use crate::rtf_xml_helpers::write_col_letters;
use crate::sheet_styling::sheet_rules::xlsx_rules::rules_xml;
use crate::sheet_styling::sheet_table::xlsx_tables::HeaderCells;
use crate::sheet_styling::sheet_view::{level_at, Outline};
use crate::sheet_styling::xlsx_filter::autofilter_xml;
use crate::sheet_styling::xlsx_layout::{layout_cols_xml, merge_cells_xml};
use crate::sheet_styling::xlsx_phonetic::push_inline_is_xml;
use crate::sheet_styling::xlsx_styles::XlsxStyleTable;
use crate::sheet_styling::xlsx_view::{sheet_format_pr_xml, sheet_pr_xml, sheet_views_xml};
use crate::sheet_styling::{merges_clear_of_tables, IdMemo, SheetStyling};
use crate::{CellPhonetic, OoxmlCodec, SpillRef, XlsxCachedValue, XmlSanitizer};

pub(crate) type SharedStringMap<'a> = HashMap<(&'a str, Option<&'a CellPhonetic>), usize>;

pub(crate) fn push_cell_xml(
    sheet_xml: &mut String,
    col_ref: &str,
    row_num: u32,
    xf: usize,
    crate::sheet_styling::xlsx_parts::CellContent {
        raw,
        kind,
        cached,
        spill,
        filled,
        text_only,
        phonetic,
    }: crate::sheet_styling::xlsx_parts::CellContent<'_>,
    sst_map: &SharedStringMap<'_>,
) {
    let _ = write!(sheet_xml, "<c r=\"{col_ref}{row_num}\"");
    if xf > 0 {
        let _ = write!(sheet_xml, " s=\"{xf}\"");
    }
    if raw.is_empty() {
        sheet_xml.push_str("/>");
    } else if text_only || filled {
        push_inline_is_xml(sheet_xml, raw, if filled { None } else { phonetic });
    } else {
        match kind {
            crate::CellValueKind::Formula => {
                let Some(formula) = raw.strip_prefix('=') else {
                    push_shared_or_inline_str(sheet_xml, raw, None, sst_map);
                    return;
                };
                let body = XmlSanitizer::escape(&crate::to_file_formula(formula));
                let (cm, f) = match spill {
                    Some(area) => (
                        " cm=\"1\"",
                        format!("<f t=\"array\" ref=\"{}\">{body}</f>", area.a1()),
                    ),
                    None => ("", format!("<f>{body}</f>")),
                };
                match cached {
                    Some(XlsxCachedValue::Number(n)) if n.is_finite() => {
                        let _ = write!(sheet_xml, "{cm}>{f}<v>{n}</v></c>");
                    }
                    Some(XlsxCachedValue::Text(text)) => {
                        let _ = write!(
                            sheet_xml,
                            " t=\"str\"{cm}>{f}<v>{}</v></c>",
                            XmlSanitizer::escape(text)
                        );
                    }
                    Some(XlsxCachedValue::Boolean(b)) => {
                        let _ = write!(sheet_xml, " t=\"b\"{cm}>{f}<v>{}</v></c>", u8::from(*b));
                    }
                    _ => {
                        let _ = write!(sheet_xml, "{cm}>{f}</c>");
                    }
                }
            }
            crate::CellValueKind::Number
                if crate::cell_numbers::numeric_cell_value(raw).is_some()
                    || raw.trim().parse::<f64>().is_ok_and(f64::is_finite) =>
            {
                let _ = write!(
                    sheet_xml,
                    " t=\"n\"><v>{}</v></c>",
                    XmlSanitizer::escape(raw)
                );
            }
            crate::CellValueKind::Boolean => {
                let v = u8::from(!raw.trim().eq_ignore_ascii_case("FALSE") && raw.trim() != "0");
                let _ = write!(sheet_xml, " t=\"b\"><v>{v}</v></c>");
            }
            crate::CellValueKind::Error => {
                let _ = write!(
                    sheet_xml,
                    " t=\"e\"><v>{}</v></c>",
                    XmlSanitizer::escape(raw)
                );
            }
            crate::CellValueKind::Text | crate::CellValueKind::Number => {
                push_shared_or_inline_str(sheet_xml, raw, phonetic, sst_map);
            }
        }
    }
}

fn push_shared_or_inline_str(
    sheet_xml: &mut String,
    raw: &str,
    phonetic: Option<&CellPhonetic>,
    sst_map: &SharedStringMap<'_>,
) {
    if let Some(&idx) = sst_map.get(&(raw, phonetic)) {
        let _ = write!(sheet_xml, " t=\"s\"><v>{idx}</v></c>");
    } else {
        push_inline_is_xml(sheet_xml, raw, phonetic);
    }
}

/// Cells grouped by row; styled blank cells are included so their style is preserved.
/// Clamps runs ending at the bottom of the sheet (1_048_575) to avoid generating over
/// a million empty <row> elements when data only spans a few rows.
pub(crate) fn group_cells_by_row<'a>(
    cells: &'a [(u32, u32, String)],
    styling: Option<&SheetStyling>,
    blank: &'a String,
) -> BTreeMap<u32, Vec<(u32, &'a String)>> {
    let mut rows_map: BTreeMap<u32, Vec<(u32, &String)>> = BTreeMap::new();
    for (row, col, raw) in cells {
        rows_map.entry(*row).or_default().push((*col, raw));
    }
    if let Some(st) = styling {
        if !st.cells.is_empty() || !st.links.is_empty() {
            let valued: HashSet<(u32, u32)> = cells.iter().map(|(r, c, _)| (*r, *c)).collect();
            let blank_cells: BTreeSet<(u32, u32)> = st
                .cells
                .iter()
                .filter(|(_, style)| !style.is_plain())
                .map(|(&at, _)| at)
                .chain(st.links.keys().copied())
                .filter(|at| !valued.contains(at))
                .collect();
            for (row, col) in blank_cells {
                rows_map.entry(row).or_default().push((col, blank));
            }
        }

        // Determine effective populated row boundary for unvalidated full-column runs
        let bottom = OoxmlCodec::MAX_XLSX_ROWS.saturating_sub(1);
        let max_data_row = cells.iter().map(|(r, _, _)| *r).max().unwrap_or(0);
        let max_style_row = st.cells.keys().map(|(r, _)| *r).max().unwrap_or(0);
        let max_link_row = st.links.keys().map(|(r, _)| *r).max().unwrap_or(0);
        let filter_max = st
            .filter
            .as_ref()
            .map(|f| f.range.row1)
            .filter(|&r| r < bottom)
            .unwrap_or(0);
        let table_max = st
            .tables
            .iter()
            .map(|t| t.range.row1)
            .filter(|&r| r < bottom)
            .max()
            .unwrap_or(0);
        let outline_max = st
            .outline
            .rows
            .iter()
            .map(|&(_, e, _)| e)
            .filter(|&r| r < bottom)
            .max()
            .unwrap_or(0);
        let effective_max = max_data_row
            .max(max_style_row)
            .max(max_link_row)
            .max(filter_max)
            .max(table_max)
            .max(outline_max);

        let clamp_run = |start: u32, end: u32| -> (u32, u32) {
            if end >= bottom && u64::from(end.saturating_sub(start)) >= crate::MAX_HIDDEN_ROWS {
                (start, end.min(effective_max.max(start)))
            } else {
                (start, end)
            }
        };

        let filtered = st.filter_hidden_runs();
        let hidden = st.hidden_rows.iter().chain(&st.outline.collapsed_rows);
        for &(start, end) in hidden.chain(filtered) {
            let (s, e) = clamp_run(start, end);
            for row in s..=e {
                rows_map.entry(row).or_default();
            }
        }
        for &(start, end, _) in &st.outline.rows {
            let (s, e) = clamp_run(start, end);
            for row in s..=e {
                rows_map.entry(row).or_default();
            }
        }
        for row in summary_rows(st) {
            rows_map.entry(row).or_default();
        }
        for &row in st.row_heights.keys() {
            rows_map.entry(row).or_default();
        }
        for &(start, end) in &st.unlocked_rows {
            let (s, e) = clamp_run(start, end);
            for row in s..=e {
                rows_map.entry(row).or_default();
            }
        }
    }
    rows_map
}

/// The rows that carry `collapsed`: the summary of every group folded as a whole.
pub(crate) fn summary_rows(styling: &SheetStyling) -> Vec<u32> {
    let outline = &styling.outline;
    Outline::summary_marks(
        &outline.rows,
        &outline.collapsed_rows,
        &outline.open_rows,
        outline.summary_above,
    )
}

pub(crate) fn build_sheet_xml(
    cells: &[(u32, u32, String)],
    cache: Option<&crate::XlsxFormulaCache>,
    styling: Option<&SheetStyling>,
    style_table: &XlsxStyleTable,
    sst_map: &SharedStringMap<'_>,
    (links_xml, drawing_xml, legacy_drawing_xml, table_parts): (&str, &str, &str, &str),
    headers: &HeaderCells,
) -> String {
    let blank = String::new();
    let rows_map = group_cells_by_row(cells, styling, &blank);
    let mut sheet_xml = String::with_capacity(
        cells
            .len()
            .saturating_mul(64)
            .saturating_add(rows_map.len().saturating_mul(32))
            .saturating_add(256),
    );
    sheet_xml.push_str(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">",
    );
    if let Some(st) = styling {
        sheet_xml.push_str(&sheet_pr_xml(st));
        sheet_xml.push_str(&sheet_views_xml(st.freeze, &st.view));
        sheet_xml.push_str(&sheet_format_pr_xml(st));
        sheet_xml.push_str(&layout_cols_xml(st, style_table.unlocked_xf()));
    } else {
        sheet_xml.push_str(&sheet_format_pr_xml(&SheetStyling::default()));
    }
    let marks = styling.map(summary_rows).unwrap_or_default();
    sheet_xml.push_str("<sheetData>");
    let mut col_buf = [0u8; 4];
    let mut ids = IdMemo::new();
    let spill_areas: &[SpillRef] = styling.map_or(&[], |st| &st.spills);
    let spill_anchors: HashMap<(u32, u32), SpillRef> = styling
        .map(|st| st.spills.iter().map(|r| ((r.row, r.col), *r)).collect())
        .unwrap_or_default();
    for (row, mut row_cells) in rows_map {
        row_cells.sort_by_key(|(col, _)| *col);
        let row_num = row.saturating_add(1);
        let hidden = styling.is_some_and(|st| st.row_hidden(row));
        let _ = write!(
            sheet_xml,
            "<row r=\"{row_num}\"{}",
            if hidden { " hidden=\"1\"" } else { "" }
        );
        if let Some(st) = styling {
            crate::sheet_styling::sheet_row_height::xlsx_row_attrs(
                &st.row_heights,
                row,
                &mut sheet_xml,
            );
            let level = level_at(&st.outline.rows, row);
            if level > 0 {
                let _ = write!(sheet_xml, " outlineLevel=\"{level}\"");
            }
            if marks.binary_search(&row).is_ok() {
                sheet_xml.push_str(" collapsed=\"1\"");
            }
            if crate::sheet_styling::xlsx_layout::run_contains(&st.unlocked_rows, row) {
                let _ = write!(
                    sheet_xml,
                    " s=\"{}\" customFormat=\"1\"",
                    style_table.unlocked_xf()
                );
            }
        }
        sheet_xml.push('>');
        for (col, raw) in row_cells {
            let col_ref = write_col_letters(&mut col_buf, col as usize);
            let xf = styling.map_or(0, |st| style_table.cell_xf(st, row, col, &mut ids));
            let cached = cache.and_then(|c| c.get(&(row, col)));
            let kind = styling.map_or_else(
                || crate::CellValueKind::infer(raw),
                |st| st.cell_kind(row, col, raw),
            );
            push_cell_xml(
                &mut sheet_xml,
                col_ref,
                row_num,
                xf,
                crate::sheet_styling::xlsx_parts::CellContent {
                    raw,
                    kind,
                    cached,
                    spill: spill_anchors.get(&(row, col)),
                    filled: kind == crate::CellValueKind::Formula
                        && spill_areas
                            .iter()
                            .any(|area| area.covers_after_anchor(row, col)),
                    text_only: headers.contains(row, col),
                    phonetic: styling.and_then(|st| st.cell_phonetic(row, col, raw)),
                },
                sst_map,
            );
        }
        sheet_xml.push_str("</row>");
    }
    sheet_xml.push_str("</sheetData>");
    if let Some(st) = styling {
        if let Some(protect) = &st.protect {
            sheet_xml.push_str(&crate::sheet_styling::xlsx_protect::sheet_protection_xml(
                protect,
            ));
        }
        if let Some(filter) = &st.filter {
            sheet_xml.push_str(&autofilter_xml(filter, &style_table.dxf));
        }
        sheet_xml.push_str(&merge_cells_xml(&merges_clear_of_tables(
            &st.merges, &st.tables,
        )));
        sheet_xml.push_str(&rules_xml(&st.rules, &style_table.dxf));
        sheet_xml.push_str(links_xml);
        sheet_xml.push_str(&crate::sheet_styling::xlsx_page::page_tail_xml(&st.page));
    }
    sheet_xml.push_str(drawing_xml);
    sheet_xml.push_str(legacy_drawing_xml);
    sheet_xml.push_str(table_parts);
    sheet_xml.push_str("</worksheet>");
    sheet_xml
}
