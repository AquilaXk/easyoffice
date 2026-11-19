//! DOCX, ODT and Markdown table markup for [`RichTable`], including merged regions, cell-level
//! hyperlinks, column widths, cell shading/borders, and inline cell pictures.

use std::fmt::Write as _;

use crate::rich_image_writers::{
    docx_image_paragraph, docx_image_relationship, odt_image_paragraph, MediaParts,
};
use crate::rich_link_writers::DocxLinks;
use crate::rich_table::{CellSpan, RichTable, TableCellLink};
use crate::rich_table_style::{
    next_cell_style, slice_cell_images, CellFill, TableBorderEdge, TableBorderKind,
    TableCellBorders, TableCellImage,
};
use crate::rich_writers::{push_docx_text, push_odt_text, StyleTable};
use crate::XmlSanitizer;

pub(crate) struct DocxTableContext<'a> {
    pub(crate) links: &'a mut DocxLinks,
    pub(crate) media: &'a mut MediaParts,
    pub(crate) image_rels: &'a mut String,
    pub(crate) first_image_rel: usize,
    pub(crate) media_dir: &'a str,
}

pub(crate) struct OdtTableStyles {
    pub(crate) col_widths: StyleTable<u32>,
    pub(crate) cell_styles: StyleTable<(CellFill, TableCellBorders)>,
}

impl OdtTableStyles {
    pub(crate) fn new() -> Self {
        Self {
            col_widths: StyleTable::new(),
            cell_styles: StyleTable::new(),
        }
    }

    pub(crate) fn write_automatic_styles(&self, out: &mut String) {
        for (idx, &twips) in self.col_widths.keys.iter().enumerate() {
            let _ = write!(
                out,
                "<style:style style:name=\"TC{}\" style:family=\"table-column\"><style:table-column-properties style:column-width=\"{:.2}pt\"/></style:style>",
                idx + 1,
                f64::from(twips) / 20.0
            );
        }
        for (idx, (fill, borders)) in self.cell_styles.keys.iter().enumerate() {
            let _ = write!(
                out,
                "<style:style style:name=\"TD{}\" style:family=\"table-cell\"><style:table-cell-properties",
                idx + 1
            );
            if !fill.is_none() {
                out.push_str(" fo:background-color=\"#");
                fill.write_hex(out);
                out.push('"');
            }
            if !borders.is_unspecified() {
                for (attr, edge) in [
                    ("fo:border-top", borders.top),
                    ("fo:border-right", borders.right),
                    ("fo:border-bottom", borders.bottom),
                    ("fo:border-left", borders.left),
                ] {
                    push_odt_border_attr(out, attr, edge);
                }
            }
            out.push_str("/></style:style>");
        }
    }
}

fn push_odt_border_attr(out: &mut String, attr: &str, edge: TableBorderEdge) {
    out.push(' ');
    out.push_str(attr);
    out.push_str("=\"");
    match edge.kind() {
        TableBorderKind::Unspecified => out.push_str("0.5pt solid #000000"),
        TableBorderKind::None => out.push_str("none"),
        kind => {
            let spec = match kind {
                TableBorderKind::Thin => "0.75pt solid #",
                TableBorderKind::Thick => "2.25pt solid #",
                TableBorderKind::Dashed => "0.75pt dashed #",
                TableBorderKind::Dotted => "0.75pt dotted #",
                TableBorderKind::Double => "2.25pt double #",
                TableBorderKind::Unspecified | TableBorderKind::None => unreachable!(),
            };
            out.push_str(spec);
            edge.write_hex(out);
        }
    }
    out.push('"');
}

pub(crate) fn slice_cell_links<'a>(
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

fn push_docx_tc_borders(out: &mut String, borders: TableCellBorders) {
    out.push_str("<w:tcBorders>");
    for (tag, edge) in [
        ("w:top", borders.top),
        ("w:left", borders.left),
        ("w:bottom", borders.bottom),
        ("w:right", borders.right),
    ] {
        match edge.kind() {
            TableBorderKind::Unspecified => {}
            TableBorderKind::None => {
                let _ = write!(out, "<{tag} w:val=\"nil\"/>");
            }
            kind => {
                let (val, sz) = match kind {
                    TableBorderKind::Thin => ("single", "4"),
                    TableBorderKind::Thick => ("single", "12"),
                    TableBorderKind::Dashed => ("dashed", "6"),
                    TableBorderKind::Dotted => ("dotted", "6"),
                    TableBorderKind::Double => ("double", "6"),
                    TableBorderKind::Unspecified | TableBorderKind::None => unreachable!(),
                };
                let _ = write!(
                    out,
                    "<{tag} w:val=\"{val}\" w:sz=\"{sz}\" w:space=\"0\" w:color=\""
                );
                edge.write_hex(out);
                out.push_str("\"/>");
            }
        }
    }
    out.push_str("</w:tcBorders>");
}

#[allow(clippy::too_many_arguments)]
fn push_docx_cell(
    out: &mut String,
    tc_w: Option<u32>,
    col_span: u32,
    v_merge: Option<&str>,
    cell_style: Option<(CellFill, TableCellBorders)>,
    text: &str,
    cell_links: &[TableCellLink],
    cell_images: &[&TableCellImage],
    ctx: &mut DocxTableContext<'_>,
) {
    out.push_str("<w:tc>");
    let has_borders = cell_style.is_some_and(|(_, b)| !b.is_unspecified());
    let has_fill = cell_style.is_some_and(|(f, _)| !f.is_none());
    if tc_w.is_some() || col_span > 1 || v_merge.is_some() || has_borders || has_fill {
        out.push_str("<w:tcPr>");
        if let Some(w) = tc_w {
            let _ = write!(out, "<w:tcW w:w=\"{w}\" w:type=\"dxa\"/>");
        }
        if col_span > 1 {
            let _ = write!(out, "<w:gridSpan w:val=\"{col_span}\"/>");
        }
        match v_merge {
            Some("restart") => out.push_str("<w:vMerge w:val=\"restart\"/>"),
            Some(_) => out.push_str("<w:vMerge/>"),
            None => {}
        }
        if let Some((_, borders)) = cell_style.filter(|(_, b)| !b.is_unspecified()) {
            push_docx_tc_borders(out, borders);
        }
        if let Some((fill, _)) = cell_style.filter(|(f, _)| !f.is_none()) {
            out.push_str("<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"");
            fill.write_hex(out);
            out.push_str("\"/>");
        }
        out.push_str("</w:tcPr>");
    }
    if !text.is_empty() || !cell_links.is_empty() || cell_images.is_empty() {
        if cell_links.is_empty() {
            out.push_str("<w:p><w:r>");
            push_docx_text(out, text);
            out.push_str("</w:r></w:p>");
        } else {
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
                    Some(anchor) => {
                        let _ = write!(
                            out,
                            "<w:hyperlink w:anchor=\"{}\" w:history=\"1\">",
                            XmlSanitizer::escape(anchor)
                        );
                    }
                    None => {
                        let _ = write!(
                            out,
                            "<w:hyperlink r:id=\"{}\" w:history=\"1\">",
                            ctx.links.rel_id(&link.url)
                        );
                    }
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
            out.push_str("</w:p>");
        }
    }
    for ci in cell_images {
        let picture = ctx.media.add(ctx.media_dir, &ci.image);
        let rel_id = format!("rId{}", picture.part + ctx.first_image_rel - 1);
        if picture.first_use {
            let target = picture.path.strip_prefix("word/").unwrap_or(&picture.path);
            ctx.image_rels
                .push_str(&docx_image_relationship(&rel_id, target));
        }
        out.push_str(&docx_image_paragraph(&ci.image, picture.ordinal, &rel_id));
    }
    out.push_str("</w:tc>");
}

fn spanned_width_twips(col_widths: Option<&[u32]>, col: usize, col_span: u32) -> Option<u32> {
    let widths = col_widths?;
    let end = col.checked_add(col_span as usize)?;
    widths
        .get(col..end)
        .map(|slice| slice.iter().copied().fold(0u32, u32::saturating_add))
}

pub(crate) fn push_docx_table(out: &mut String, table: &RichTable, ctx: &mut DocxTableContext<'_>) {
    let spans = table.span_map();
    let links = table.validated_links();
    let styles = table.validated_cell_styles();
    let images = table.validated_cell_images();
    let col_widths = table.validated_col_widths();
    let mut link_cursor = 0usize;
    let mut style_cursor = 0usize;
    let mut image_cursor = 0usize;
    out.push_str("<w:tbl>");
    crate::ooxml_shared_parts::push_docx_table_properties_with_widths(
        out,
        &table.rows,
        col_widths.as_deref(),
    );
    for (r, row) in table.rows.iter().enumerate() {
        out.push_str("<w:tr>");
        let mut c = 0;
        while c < row.len() {
            let state = spans.as_ref().map_or(CellSpan::Plain, |m| m.at(r, c));
            match state {
                CellSpan::Plain => {
                    let tc_w = spanned_width_twips(col_widths.as_deref(), c, 1);
                    let cl = slice_cell_links(&links, &mut link_cursor, r, c);
                    let st = next_cell_style(&styles, &mut style_cursor, r, c);
                    let ci = slice_cell_images(&images, &mut image_cursor, r, c);
                    push_docx_cell(out, tc_w, 1, None, st, &row[c], cl, ci, ctx);
                }
                CellSpan::Anchor { row_span, col_span } => {
                    let v_merge = (row_span > 1).then_some("restart");
                    let tc_w = spanned_width_twips(col_widths.as_deref(), c, col_span);
                    let cl = slice_cell_links(&links, &mut link_cursor, r, c);
                    let st = next_cell_style(&styles, &mut style_cursor, r, c);
                    let ci = slice_cell_images(&images, &mut image_cursor, r, c);
                    push_docx_cell(out, tc_w, col_span, v_merge, st, &row[c], cl, ci, ctx);
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
                        let tc_w = spanned_width_twips(col_widths.as_deref(), c, col_span);
                        push_docx_cell(
                            out,
                            tc_w,
                            col_span,
                            Some("continue"),
                            None,
                            "",
                            &[],
                            &[],
                            ctx,
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
        let _ = write!(
            body,
            "<text:a xlink:type=\"simple\" xlink:href=\"{}\">",
            XmlSanitizer::escape(&link.url)
        );
        push_odt_text(body, &cell[s..e]);
        body.push_str("</text:a>");
        pos = e;
    }
    if pos < cell.len() {
        push_odt_text(body, &cell[pos..]);
    }
}

pub(crate) fn push_odt_table(
    body: &mut String,
    table: &RichTable,
    table_styles: &mut OdtTableStyles,
    media: &mut MediaParts,
) {
    let spans = table.span_map();
    let links = table.validated_links();
    let styles = table.validated_cell_styles();
    let images = table.validated_cell_images();
    let col_widths = table.validated_col_widths();
    let mut link_cursor = 0usize;
    let mut style_cursor = 0usize;
    let mut image_cursor = 0usize;
    let cols = table.column_count().max(1);
    body.push_str("<table:table>");
    if let Some(widths) = col_widths {
        let mut idx = 0usize;
        while idx < widths.len() {
            let w = widths[idx];
            let mut run = 1usize;
            while idx + run < widths.len() && widths[idx + run] == w {
                run += 1;
            }
            let style_id = table_styles.col_widths.intern(&w);
            if run > 1 {
                let _ = write!(
                    body,
                    "<table:table-column table:style-name=\"TC{style_id}\" table:number-columns-repeated=\"{run}\"/>"
                );
            } else {
                let _ = write!(
                    body,
                    "<table:table-column table:style-name=\"TC{style_id}\"/>"
                );
            }
            idx += run;
        }
    } else {
        let _ = write!(
            body,
            "<table:table-column table:number-columns-repeated=\"{cols}\"/>"
        );
    }
    for (r, row) in table.rows.iter().enumerate() {
        body.push_str("<table:table-row>");
        for (c, cell) in row.iter().enumerate() {
            match spans.as_ref().map_or(CellSpan::Plain, |m| m.at(r, c)) {
                CellSpan::Covered { .. } => body.push_str("<table:covered-table-cell/>"),
                CellSpan::Anchor { row_span, col_span } => {
                    let cl = slice_cell_links(&links, &mut link_cursor, r, c);
                    let st = next_cell_style(&styles, &mut style_cursor, r, c);
                    let ci = slice_cell_images(&images, &mut image_cursor, r, c);
                    let _ = write!(
                        body,
                        "<table:table-cell table:number-columns-spanned=\"{col_span}\" table:number-rows-spanned=\"{row_span}\""
                    );
                    if let Some(key) = st {
                        let sid = table_styles.cell_styles.intern(&key);
                        let _ = write!(body, " table:style-name=\"TD{sid}\"");
                    }
                    body.push_str(" office:value-type=\"string\">");
                    push_odt_cell_body(body, cell, cl, ci, media);
                    body.push_str("</table:table-cell>");
                }
                CellSpan::Plain => {
                    let cl = slice_cell_links(&links, &mut link_cursor, r, c);
                    let st = next_cell_style(&styles, &mut style_cursor, r, c);
                    let ci = slice_cell_images(&images, &mut image_cursor, r, c);
                    body.push_str("<table:table-cell");
                    if let Some(key) = st {
                        let sid = table_styles.cell_styles.intern(&key);
                        let _ = write!(body, " table:style-name=\"TD{sid}\"");
                    }
                    body.push_str(" office:value-type=\"string\">");
                    push_odt_cell_body(body, cell, cl, ci, media);
                    body.push_str("</table:table-cell>");
                }
            }
        }
        body.push_str("</table:table-row>");
    }
    body.push_str("</table:table>");
}

fn push_odt_cell_body(
    body: &mut String,
    cell: &str,
    cell_links: &[TableCellLink],
    cell_images: &[&TableCellImage],
    media: &mut MediaParts,
) {
    if !cell.is_empty() || !cell_links.is_empty() || cell_images.is_empty() {
        body.push_str("<text:p>");
        push_odt_cell_content(body, cell, cell_links);
        body.push_str("</text:p>");
    }
    for ci in cell_images {
        let picture = media.add("Pictures", &ci.image);
        body.push_str(&odt_image_paragraph(
            &ci.image,
            picture.ordinal,
            &picture.path,
        ));
    }
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
