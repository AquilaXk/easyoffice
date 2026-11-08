//! DOCX and ODT table markup for [`RichTable`], including merged regions: OOXML `w:gridSpan` and
//! `w:vMerge`, ODF `table:number-columns-spanned`, `table:number-rows-spanned` and
//! `table:covered-table-cell`.

use crate::rich_table::{CellSpan, RichTable};
use crate::rich_writers::{push_docx_text, push_odt_text};

fn push_docx_cell(out: &mut String, properties: &str, text: &str) {
    out.push_str("<w:tc>");
    if !properties.is_empty() {
        out.push_str("<w:tcPr>");
        out.push_str(properties);
        out.push_str("</w:tcPr>");
    }
    out.push_str("<w:p><w:r>");
    push_docx_text(out, text);
    out.push_str("</w:r></w:p></w:tc>");
}

fn docx_cell_properties(col_span: u32, v_merge: Option<&str>) -> String {
    let mut props = String::new();
    if col_span > 1 {
        props.push_str(&format!("<w:gridSpan w:val=\"{col_span}\"/>"));
    }
    match v_merge {
        Some("restart") => props.push_str("<w:vMerge w:val=\"restart\"/>"),
        Some(_) => props.push_str("<w:vMerge/>"),
        None => {}
    }
    props
}

pub(crate) fn push_docx_table(out: &mut String, table: &RichTable) {
    let spans = table.span_map();
    out.push_str("<w:tbl>");
    crate::ooxml_shared_parts::push_docx_table_properties(out, &table.rows);
    for (r, row) in table.rows.iter().enumerate() {
        out.push_str("<w:tr>");
        let mut c = 0;
        while c < row.len() {
            let state = spans.as_ref().map_or(CellSpan::Plain, |m| m.at(r, c));
            match state {
                CellSpan::Plain => push_docx_cell(out, "", &row[c]),
                CellSpan::Anchor { row_span, col_span } => {
                    let v_merge = (row_span > 1).then_some("restart");
                    push_docx_cell(out, &docx_cell_properties(col_span, v_merge), &row[c]);
                    c += col_span as usize - 1;
                }
                CellSpan::Covered {
                    row_offset,
                    col_offset,
                } => {
                    // Cells right of the anchor are covered by its gridSpan; lower rows repeat
                    // the first column of the region as a vMerge continuation.
                    let anchor = spans
                        .as_ref()
                        .map(|m| m.at(r - row_offset as usize, c - col_offset as usize));
                    if let (0, Some(CellSpan::Anchor { col_span, .. })) = (col_offset, anchor) {
                        push_docx_cell(out, &docx_cell_properties(col_span, Some("continue")), "");
                        c += col_span as usize - 1;
                    }
                }
            }
            c += 1;
        }
        out.push_str("</w:tr>");
    }
    out.push_str("</w:tbl>");
}

pub(crate) fn push_odt_table(body: &mut String, table: &RichTable) {
    let spans = table.span_map();
    let cols = table.column_count().max(1);
    body.push_str(&format!(
        "<table:table><table:table-column table:number-columns-repeated=\"{cols}\"/>"
    ));
    for (r, row) in table.rows.iter().enumerate() {
        body.push_str("<table:table-row>");
        for (c, cell) in row.iter().enumerate() {
            match spans.as_ref().map_or(CellSpan::Plain, |m| m.at(r, c)) {
                CellSpan::Covered { .. } => body.push_str("<table:covered-table-cell/>"),
                CellSpan::Anchor { row_span, col_span } => {
                    body.push_str(&format!(
                        "<table:table-cell table:number-columns-spanned=\"{col_span}\" table:number-rows-spanned=\"{row_span}\" office:value-type=\"string\"><text:p>"
                    ));
                    push_odt_text(body, cell);
                    body.push_str("</text:p></table:table-cell>");
                }
                CellSpan::Plain => {
                    body.push_str("<table:table-cell office:value-type=\"string\"><text:p>");
                    push_odt_text(body, cell);
                    body.push_str("</text:p></table:table-cell>");
                }
            }
        }
        body.push_str("</table:table-row>");
    }
    body.push_str("</table:table>");
}
