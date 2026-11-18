//! DOCX, ODT and Markdown table markup for [`RichTable`], including merged regions and cell-level
//! hyperlinks: OOXML `w:gridSpan`, `w:vMerge` and `w:hyperlink`, ODF `table:number-columns-spanned`,
//! `table:number-rows-spanned`, `table:covered-table-cell` and `text:a`, and CommonMark `[text](url)`.

use crate::rich_link_writers::DocxLinks;
use crate::rich_table::{CellSpan, RichTable, TableCellLink};
use crate::rich_writers::{push_docx_text, push_odt_text};
use crate::XmlSanitizer;

fn slice_cell_links<'a>(
    links: &'a [TableCellLink],
    cursor: &mut usize,
    row: usize,
    col: usize,
) -> &'a [TableCellLink] {
    let target = (row as u32, col as u32);
    while *cursor < links.len() && (links[*cursor].row, links[*cursor].col) < target {
        *cursor += 1;
    }
    let start = *cursor;
    while *cursor < links.len() && (links[*cursor].row, links[*cursor].col) == target {
        *cursor += 1;
    }
    &links[start..*cursor]
}

fn push_docx_cell(
    out: &mut String,
    properties: &str,
    text: &str,
    cell_links: &[TableCellLink],
    docx_links: &mut DocxLinks,
) {
    out.push_str("<w:tc>");
    if !properties.is_empty() {
        out.push_str("<w:tcPr>");
        out.push_str(properties);
        out.push_str("</w:tcPr>");
    }
    if cell_links.is_empty() {
        out.push_str("<w:p><w:r>");
        push_docx_text(out, text);
        out.push_str("</w:r></w:p></w:tc>");
        return;
    }
    out.push_str("<w:p>");
    let mut pos = 0usize;
    for link in cell_links {
        let (s, e) = (link.start as usize, link.end as usize);
        if pos < s {
            out.push_str("<w:r>");
            push_docx_text(out, &text[pos..s]);
            out.push_str("</w:r>");
        }
        match link.url.strip_prefix('#') {
            Some(anchor) => out.push_str(&format!(
                "<w:hyperlink w:anchor=\"{}\" w:history=\"1\">",
                XmlSanitizer::escape(anchor)
            )),
            None => out.push_str(&format!(
                "<w:hyperlink r:id=\"{}\" w:history=\"1\">",
                docx_links.rel_id(&link.url)
            )),
        }
        out.push_str("<w:r><w:rPr><w:rStyle w:val=\"Hyperlink\"/></w:rPr>");
        push_docx_text(out, &text[s..e]);
        out.push_str("</w:r></w:hyperlink>");
        pos = e;
    }
    if pos < text.len() {
        out.push_str("<w:r>");
        push_docx_text(out, &text[pos..]);
        out.push_str("</w:r>");
    }
    out.push_str("</w:p></w:tc>");
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

pub(crate) fn push_docx_table(out: &mut String, table: &RichTable, docx_links: &mut DocxLinks) {
    let spans = table.span_map();
    let links = table.validated_links();
    let mut link_cursor = 0usize;
    out.push_str("<w:tbl>");
    crate::ooxml_shared_parts::push_docx_table_properties(out, &table.rows);
    for (r, row) in table.rows.iter().enumerate() {
        out.push_str("<w:tr>");
        let mut c = 0;
        while c < row.len() {
            let state = spans.as_ref().map_or(CellSpan::Plain, |m| m.at(r, c));
            match state {
                CellSpan::Plain => {
                    let cl = slice_cell_links(&links, &mut link_cursor, r, c);
                    push_docx_cell(out, "", &row[c], cl, docx_links);
                }
                CellSpan::Anchor { row_span, col_span } => {
                    let v_merge = (row_span > 1).then_some("restart");
                    let cl = slice_cell_links(&links, &mut link_cursor, r, c);
                    push_docx_cell(
                        out,
                        &docx_cell_properties(col_span, v_merge),
                        &row[c],
                        cl,
                        docx_links,
                    );
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
                        push_docx_cell(
                            out,
                            &docx_cell_properties(col_span, Some("continue")),
                            "",
                            &[],
                            docx_links,
                        );
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

fn push_odt_cell_content(body: &mut String, cell: &str, cell_links: &[TableCellLink]) {
    if cell_links.is_empty() {
        push_odt_text(body, cell);
        return;
    }
    let mut pos = 0usize;
    for link in cell_links {
        let (s, e) = (link.start as usize, link.end as usize);
        if pos < s {
            push_odt_text(body, &cell[pos..s]);
        }
        body.push_str(&format!(
            "<text:a xlink:type=\"simple\" xlink:href=\"{}\">",
            XmlSanitizer::escape(&link.url)
        ));
        push_odt_text(body, &cell[s..e]);
        body.push_str("</text:a>");
        pos = e;
    }
    if pos < cell.len() {
        push_odt_text(body, &cell[pos..]);
    }
}

pub(crate) fn push_odt_table(body: &mut String, table: &RichTable) {
    let spans = table.span_map();
    let links = table.validated_links();
    let mut link_cursor = 0usize;
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
                    let cl = slice_cell_links(&links, &mut link_cursor, r, c);
                    body.push_str(&format!(
                        "<table:table-cell table:number-columns-spanned=\"{col_span}\" table:number-rows-spanned=\"{row_span}\" office:value-type=\"string\"><text:p>"
                    ));
                    push_odt_cell_content(body, cell, cl);
                    body.push_str("</text:p></table:table-cell>");
                }
                CellSpan::Plain => {
                    let cl = slice_cell_links(&links, &mut link_cursor, r, c);
                    body.push_str("<table:table-cell office:value-type=\"string\"><text:p>");
                    push_odt_cell_content(body, cell, cl);
                    body.push_str("</text:p></table:table-cell>");
                }
            }
        }
        body.push_str("</table:table-row>");
    }
    body.push_str("</table:table>");
}

fn escape_md_link_url(url: &str) -> String {
    let mut out = String::with_capacity(url.len());
    for ch in url.chars() {
        match ch {
            '(' => out.push_str("%28"),
            ')' => out.push_str("%29"),
            '\\' => out.push_str("%5C"),
            c => out.push(c),
        }
    }
    out
}

pub(crate) fn markdown_table_rows(
    table: &RichTable,
    escape_cell: fn(&str) -> String,
) -> Vec<Vec<String>> {
    let links = table.validated_links();
    if links.is_empty() {
        return table
            .rows
            .iter()
            .map(|row| row.iter().map(|c| escape_cell(c)).collect())
            .collect();
    }
    let mut link_cursor = 0usize;
    table
        .rows
        .iter()
        .enumerate()
        .map(|(r, row)| {
            row.iter()
                .enumerate()
                .map(|(c, cell)| {
                    let cl = slice_cell_links(&links, &mut link_cursor, r, c);
                    if cl.is_empty() {
                        return escape_cell(cell);
                    }
                    let mut out = String::with_capacity(cell.len() + 32);
                    let mut pos = 0usize;
                    for link in cl {
                        let (s, e) = (link.start as usize, link.end as usize);
                        if pos < s {
                            out.push_str(&escape_cell(&cell[pos..s]));
                        }
                        out.push('[');
                        out.push_str(&escape_cell(&cell[s..e]));
                        out.push_str("](");
                        out.push_str(&escape_md_link_url(&link.url));
                        out.push(')');
                        pos = e;
                    }
                    if pos < cell.len() {
                        out.push_str(&escape_cell(&cell[pos..]));
                    }
                    out
                })
                .collect()
        })
        .collect()
}
