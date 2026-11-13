//! Writers that keep [`RichDocument`] block order and run/paragraph formatting for DOCX
//! (`w:rPr`/`w:pPr`/`w:pStyle` plus `word/styles.xml`), ODT (automatic styles), Markdown
//! (`**`/`*`/`~~`/`#`), TXT, and PDF (ordered text); RTF lives in `rich_rtf_writer.rs`. User text
//! is always escaped for the target syntax and never parsed.

use crate::pdf_highlight::HighlightPass;
use crate::pdf_image::pdf_image_object;
use crate::pdf_writer::{
    text_entries, PdfImagePage, PdfPageEntry, PdfWriteOptions, PDF_IMAGE_MARGIN_PT,
    PDF_PAGE_HEIGHT_PT, PDF_PAGE_WIDTH_PT,
};
use crate::rich_document::frame_paragraph;
use crate::rich_document::{
    RichAlignment, RichBlock, RichDocument, RichListKind, RichParagraph, TOC_BOOKMARK_PREFIX,
};
use crate::rich_image::{check_document_images, fit_within};
use crate::rich_image_writers::{
    docx_content_types, docx_image_paragraph, docx_image_relationship, emu_to_points,
    odt_image_paragraph, MediaParts, DOCX_PICTURE_NAMESPACES, ODT_PICTURE_NAMESPACES,
};
use crate::rich_link_writers::{
    docx_linked_run_properties, link_groups, DocxLinks, DOCX_LINK_STYLE_XML,
};
use crate::rich_list_writers::{
    docx_num_pr, docx_numbering_xml, level_of, list_kind_of, odt_list_styles_xml, plan_lists,
    OdtListWriter,
};
use crate::rich_page_frame::{
    docx_field_run, docx_frame_part, docx_frame_refs, docx_page_margins, docx_paper, odt_field,
    odt_master_page, odt_styles_xml, DOCX_FOOTER_PART, DOCX_HEADER_PART,
};
use crate::rich_run_writers::{
    docx_run_properties, odt_font_face_decls, odt_text_key, odt_text_key_is_plain,
    odt_text_style_xml, OdtTextKey,
};
use crate::rich_table_writers::{push_docx_table, push_odt_table};
use crate::rich_toc_writers::{
    docx_field_edges, docx_toc_page, docx_toc_style_id, docx_toc_styles_xml, docx_toc_tabs,
    is_toc_block, odt_toc_page, odt_toc_paragraph_style, odt_toc_styles_xml, TocScope,
};
use crate::{
    CommonMarkCodec, EasyOfficeError, MarkdownBlock, OdfCodec, PdfDocumentModel, PdfPageModel,
    PdfSerializer, XmlSanitizer, ZipArchiveCodec,
};

/// `w:spacing/@w:line` value of single line spacing (240ths of a line).
const DOCX_SINGLE_LINE_240THS: u32 = 240;

/// Heading font sizes in half-points for levels 1–6.
const HEADING_HALF_POINTS: [u32; 6] = [32, 28, 26, 24, 22, 22];

pub(crate) fn heading_half_points(level: u8) -> u32 {
    HEADING_HALF_POINTS[usize::from(level.clamp(1, 6)) - 1]
}

// ---------------------------------------------------------------- DOCX

const DOCX_NUMBERING_REL_ID: &str = "rIdNumbering";

fn docx_styles_xml(has_toc: bool) -> String {
    let mut xml = String::from(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
        "<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
        "<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:eastAsia=\"맑은 고딕\" w:cs=\"Calibri\"/>",
        "<w:sz w:val=\"22\"/><w:szCs w:val=\"22\"/><w:lang w:val=\"ko-KR\" w:eastAsia=\"ko-KR\"/></w:rPr></w:rPrDefault>",
        "<w:pPrDefault><w:pPr><w:spacing w:after=\"120\" w:line=\"276\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>",
        "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style>"
    ));
    for level in 1..=6u8 {
        let size = heading_half_points(level);
        xml.push_str(&format!(
            concat!(
                "<w:style w:type=\"paragraph\" w:styleId=\"Heading{l}\"><w:name w:val=\"heading {l}\"/>",
                "<w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/>",
                "<w:pPr><w:keepNext/><w:spacing w:before=\"240\" w:after=\"120\"/><w:outlineLvl w:val=\"{o}\"/></w:pPr>",
                "<w:rPr><w:b/><w:bCs/><w:sz w:val=\"{s}\"/><w:szCs w:val=\"{s}\"/></w:rPr></w:style>"
            ),
            l = level,
            o = level - 1,
            s = size
        ));
    }
    for (id, name) in [("ListBullet", "List Bullet"), ("ListNumber", "List Number")] {
        xml.push_str(&format!(
            "<w:style w:type=\"paragraph\" w:styleId=\"{id}\"><w:name w:val=\"{name}\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:contextualSpacing/></w:pPr></w:style>"
        ));
    }
    xml.push_str(DOCX_LINK_STYLE_XML);
    if has_toc {
        xml.push_str(&docx_toc_styles_xml());
    }
    xml.push_str("<w:style w:type=\"table\" w:default=\"1\" w:styleId=\"TableNormal\"><w:name w:val=\"Normal Table\"/><w:tblPr><w:tblInd w:w=\"0\" w:type=\"dxa\"/><w:tblCellMar><w:left w:w=\"108\" w:type=\"dxa\"/><w:right w:w=\"108\" w:type=\"dxa\"/></w:tblCellMar></w:tblPr></w:style>");
    xml.push_str("</w:styles>");
    xml
}

pub(crate) fn push_docx_text(out: &mut String, text: &str) {
    for (l_idx, line) in text.split('\n').enumerate() {
        if l_idx > 0 {
            out.push_str("<w:br/>");
        }
        for (t_idx, seg) in line.split('\t').enumerate() {
            if t_idx > 0 {
                out.push_str("<w:tab/>");
            }
            if !seg.is_empty() {
                out.push_str("<w:t xml:space=\"preserve\">");
                out.push_str(&XmlSanitizer::escape(seg));
                out.push_str("</w:t>");
            }
        }
    }
}

/// WordprocessingML `w:spacing` (line height, 240 = single) and `w:ind` for a paragraph, in
/// schema order; empty when the paragraph inherits both.
pub(crate) fn docx_paragraph_layout(p: &RichParagraph) -> String {
    let mut xml = String::new();
    if let Some(percent) = p.line_spacing_percent {
        let line = u32::from(percent) * DOCX_SINGLE_LINE_240THS / 100;
        xml.push_str(&format!(
            "<w:spacing w:line=\"{line}\" w:lineRule=\"auto\"/>"
        ));
    }
    if p.left_indent_twips.is_some() || p.first_line_indent_twips.is_some() {
        xml.push_str("<w:ind");
        if let Some(left) = p.left_indent_twips {
            xml.push_str(&format!(" w:left=\"{left}\""));
        }
        match p.first_line_indent_twips {
            Some(first) if first >= 0 => xml.push_str(&format!(" w:firstLine=\"{first}\"")),
            Some(hanging) => {
                xml.push_str(&format!(" w:hanging=\"{}\"", hanging.unsigned_abs()));
            }
            None => {}
        }
        xml.push_str("/>");
    }
    xml
}

/// `run` is the list (see `rich_list_writers`) a list paragraph belongs to.
fn push_docx_paragraph(
    out: &mut String,
    p: &RichParagraph,
    run: Option<usize>,
    links: &mut DocxLinks,
    field: &(String, &str),
) {
    out.push_str("<w:p>");
    let style = if let Some(role) = p.toc {
        Some(docx_toc_style_id(role))
    } else if (1..=6).contains(&p.heading_level) {
        Some(format!("Heading{}", p.heading_level))
    } else {
        list_kind_of(p).map(|kind| match kind {
            RichListKind::Bullet => "ListBullet".to_string(),
            RichListKind::Ordered => "ListNumber".to_string(),
        })
    };
    let jc = p.alignment.map(|a| match a {
        RichAlignment::Left => "left",
        RichAlignment::Center => "center",
        RichAlignment::Right => "right",
        RichAlignment::Justify => "both",
    });
    let layout = docx_paragraph_layout(p);
    let tabs = docx_toc_tabs(p);
    if style.is_some()
        || jc.is_some()
        || !layout.is_empty()
        || !tabs.is_empty()
        || p.page_break_before
    {
        out.push_str("<w:pPr>");
        if let Some(s) = style {
            out.push_str(&format!("<w:pStyle w:val=\"{s}\"/>"));
        }
        if p.page_break_before {
            out.push_str("<w:pageBreakBefore/>");
        }
        if let Some(run) = run {
            out.push_str(&docx_num_pr(run, p));
        }
        out.push_str(&tabs);
        out.push_str(&layout);
        if let Some(j) = jc {
            out.push_str(&format!("<w:jc w:val=\"{j}\"/>"));
        }
        out.push_str("</w:pPr>");
    }
    out.push_str(&field.0);
    if let Some(number) = p.bookmark {
        out.push_str(&format!(
            "<w:bookmarkStart w:id=\"{number}\" w:name=\"{TOC_BOOKMARK_PREFIX}{number}\"/>"
        ));
    }
    for (link, group) in link_groups(&p.runs) {
        match link.map(|url| url.strip_prefix('#').ok_or(url)) {
            Some(Ok(anchor)) => out.push_str(&format!(
                "<w:hyperlink w:anchor=\"{anchor}\" w:history=\"1\">"
            )),
            Some(Err(url)) => out.push_str(&format!(
                "<w:hyperlink r:id=\"{}\" w:history=\"1\">",
                links.rel_id(url)
            )),
            None => {}
        }
        for run in group {
            let properties = docx_run_properties(run);
            if let Some(field) = run.field {
                out.push_str(&docx_field_run(field, &properties));
                continue;
            }
            out.push_str("<w:r>");
            out.push_str(&if link.is_some() {
                docx_linked_run_properties(&properties)
            } else {
                properties
            });
            push_docx_text(out, &run.text);
            out.push_str("</w:r>");
        }
        if link.is_some() {
            out.push_str("</w:hyperlink>");
        }
    }
    out.push_str(&docx_toc_page(p));
    if let Some(number) = p.bookmark {
        out.push_str(&format!("<w:bookmarkEnd w:id=\"{number}\"/>"));
    }
    out.push_str(field.1);
    out.push_str("</w:p>");
}

/// Relationship id offset of the first picture (`rId1` is the style sheet).
const DOCX_FIRST_IMAGE_REL: usize = 2;
const DOCX_MEDIA_DIR: &str = "word/media";

pub(crate) fn pack_docx_rich(doc: &RichDocument) -> Result<Vec<u8>, EasyOfficeError> {
    check_document_images(doc)?;
    let rels = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>",
        "<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/>",
        "</Relationships>"
    );
    let core_xml = crate::ooxml_shared_parts::core_properties_xml(&doc.display_title());
    let styles = docx_styles_xml(doc.blocks.iter().any(is_toc_block));
    let lists = plan_lists(&doc.blocks);

    let mut body = String::with_capacity(1024);
    body.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    body.push_str(
        "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"",
    );
    body.push_str(DOCX_PICTURE_NAMESPACES);
    body.push_str("><w:body>");
    let mut media = MediaParts::new();
    let mut image_rels = String::new();
    let mut links = DocxLinks::default();
    let mut toc_scope = TocScope::docx();
    for (index, block) in doc.blocks.iter().enumerate() {
        toc_scope.enter(&mut body, &doc.blocks, index);
        match block {
            RichBlock::Paragraph(p) => {
                push_docx_paragraph(
                    &mut body,
                    p,
                    lists.block_run[index],
                    &mut links,
                    &docx_field_edges(&doc.blocks, index),
                );
            }
            RichBlock::Image(img) => {
                let picture = media.add(DOCX_MEDIA_DIR, img);
                let rel_id = format!("rId{}", picture.part + DOCX_FIRST_IMAGE_REL - 1);
                if picture.first_use {
                    let target = picture.path.strip_prefix("word/").unwrap_or(&picture.path);
                    image_rels.push_str(&docx_image_relationship(&rel_id, target));
                }
                body.push_str(&docx_image_paragraph(img, picture.ordinal, &rel_id));
            }
            RichBlock::Table(table) => {
                push_docx_table(&mut body, table);
                // A table cannot end the body or touch another table without a paragraph.
                body.push_str("<w:p/>");
            }
        }
    }
    toc_scope.enter(&mut body, &doc.blocks, doc.blocks.len());
    let header_part = docx_frame_part(&doc.page_frame.header, true);
    let footer_part = docx_frame_part(&doc.page_frame.footer, false);
    let frame = docx_frame_refs(header_part.is_some(), footer_part.is_some());
    body.push_str(&format!(
        "<w:sectPr>{}{}{}</w:sectPr>",
        frame.section,
        docx_paper(doc.page_frame.paper.as_ref()),
        docx_page_margins(doc.page_frame.margins.as_ref())
    ));
    body.push_str("</w:body></w:document>");

    let has_lists = !lists.runs.is_empty();
    let numbering = has_lists.then(|| docx_numbering_xml(&lists));
    let content_types = docx_content_types(&media, has_lists, &frame.content_types);
    let numbering_rel = if has_lists {
        format!(
            "<Relationship Id=\"{DOCX_NUMBERING_REL_ID}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering\" Target=\"numbering.xml\"/>"
        )
    } else {
        String::new()
    };
    let doc_rels = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
            "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>",
            "{}{}{}{}</Relationships>"
        ),
        numbering_rel,
        image_rels,
        links.relationships(),
        frame.relationships
    );

    for xml in [&body, &core_xml, &styles, &doc_rels]
        .into_iter()
        .chain(numbering.as_ref())
        .chain(header_part.as_ref())
        .chain(footer_part.as_ref())
    {
        XmlSanitizer::validate_safe_xml(xml)?;
    }

    let mut entries = vec![
        (
            "[Content_Types].xml".to_string(),
            content_types.into_bytes(),
        ),
        ("_rels/.rels".to_string(), rels.as_bytes().to_vec()),
        ("word/document.xml".to_string(), body.into_bytes()),
        (
            "word/_rels/document.xml.rels".to_string(),
            doc_rels.into_bytes(),
        ),
        ("word/styles.xml".to_string(), styles.into_bytes()),
        ("docProps/core.xml".to_string(), core_xml.into_bytes()),
    ];
    if let Some(numbering) = numbering {
        entries.push(("word/numbering.xml".to_string(), numbering.into_bytes()));
    }
    for (part, xml) in [
        (DOCX_HEADER_PART, header_part),
        (DOCX_FOOTER_PART, footer_part),
    ] {
        if let Some(xml) = xml {
            entries.push((part.to_string(), xml.into_bytes()));
        }
    }
    entries.extend(media.into_entries());
    ZipArchiveCodec::pack_compressed(&entries)
}

// ---------------------------------------------------------------- ODT

/// Interned automatic-style keys, in first-use order.
struct StyleTable<K: Eq + std::hash::Hash + Clone> {
    keys: Vec<K>,
    index: std::collections::HashMap<K, usize>,
}

impl<K: Eq + std::hash::Hash + Clone> StyleTable<K> {
    fn new() -> Self {
        Self {
            keys: Vec::new(),
            index: std::collections::HashMap::new(),
        }
    }

    fn intern(&mut self, key: &K) -> usize {
        if let Some(&pos) = self.index.get(key) {
            return pos;
        }
        self.keys.push(key.clone());
        let pos = self.keys.len();
        self.index.insert(key.clone(), pos);
        pos
    }
}

/// Automatic paragraph style identity.
#[derive(Clone, PartialEq, Eq, Hash, Default)]
struct OdtParaKey {
    align: Option<RichAlignment>,
    level: u8,
    left_twips: Option<u32>,
    first_line_twips: Option<i32>,
    line_percent: Option<u16>,
    page_break: bool,
}

pub(crate) fn push_odt_text(out: &mut String, text: &str) {
    for (l_idx, line) in text.split('\n').enumerate() {
        if l_idx > 0 {
            out.push_str("<text:line-break/>");
        }
        for (t_idx, seg) in line.split('\t').enumerate() {
            if t_idx > 0 {
                out.push_str("<text:tab/>");
            }
            out.push_str(&XmlSanitizer::escape(seg));
        }
    }
}

fn push_odt_paragraph(
    body: &mut String,
    p: &RichParagraph,
    para_styles: &mut StyleTable<OdtParaKey>,
    text_styles: &mut StyleTable<OdtTextKey>,
) {
    let level = if (1..=6).contains(&p.heading_level) {
        p.heading_level
    } else {
        0
    };
    let key = OdtParaKey {
        align: p.alignment,
        level,
        left_twips: p.left_indent_twips,
        first_line_twips: p.first_line_indent_twips,
        line_percent: p.line_spacing_percent,
        page_break: p.page_break_before,
    };
    let style_attr = if let Some(role) = p.toc {
        format!(" text:style-name=\"{}\"", odt_toc_paragraph_style(role, p))
    } else if key == OdtParaKey::default() {
        String::new()
    } else {
        format!(" text:style-name=\"P{}\"", para_styles.intern(&key))
    };
    if level > 0 {
        body.push_str(&format!(
            "<text:h text:outline-level=\"{level}\"{style_attr}>"
        ));
    } else {
        body.push_str(&format!("<text:p{style_attr}>"));
    }
    if let Some(number) = p.bookmark {
        body.push_str(&format!(
            "<text:bookmark-start text:name=\"{TOC_BOOKMARK_PREFIX}{number}\"/>"
        ));
    }
    for (link, group) in link_groups(&p.runs) {
        if let Some(url) = link {
            body.push_str(&format!(
                "<text:a xlink:type=\"simple\" xlink:href=\"{}\">",
                XmlSanitizer::escape(url)
            ));
        }
        for run in group {
            let key = odt_text_key(run);
            if let Some(field) = run.field {
                let style = (!odt_text_key_is_plain(&key)).then(|| text_styles.intern(&key));
                body.push_str(&odt_field(field, style));
            } else if odt_text_key_is_plain(&key) {
                push_odt_text(body, &run.text);
            } else {
                let t = text_styles.intern(&key);
                body.push_str(&format!("<text:span text:style-name=\"T{t}\">"));
                push_odt_text(body, &run.text);
                body.push_str("</text:span>");
            }
        }
        if link.is_some() {
            body.push_str("</text:a>");
        }
    }
    body.push_str(&odt_toc_page(p));
    if let Some(number) = p.bookmark {
        body.push_str(&format!(
            "<text:bookmark-end text:name=\"{TOC_BOOKMARK_PREFIX}{number}\"/>"
        ));
    }
    body.push_str(if level > 0 { "</text:h>" } else { "</text:p>" });
}

/// Twips as a point length with at most two decimals (`36pt`, `7.5pt`).
fn odt_twips_to_pt(twips: i64) -> String {
    let sign = if twips < 0 { "-" } else { "" };
    let abs = twips.abs();
    let (whole, tenths_of_twip) = (abs / 20, abs % 20 * 5);
    if tenths_of_twip == 0 {
        format!("{sign}{whole}pt")
    } else {
        format!("{sign}{whole}.{tenths_of_twip:02}pt")
    }
}

/// `style:style` XML of one paragraph style (1-based `number`).
fn odt_para_style_xml(number: usize, key: &OdtParaKey) -> String {
    let OdtParaKey {
        align,
        level,
        left_twips,
        first_line_twips,
        line_percent,
        page_break,
    } = key;
    let mut styles = format!(
        "<style:style style:name=\"P{number}\" style:family=\"paragraph\"><style:paragraph-properties"
    );
    if let Some(a) = align {
        let v = match a {
            RichAlignment::Left => "start",
            RichAlignment::Center => "center",
            RichAlignment::Right => "end",
            RichAlignment::Justify => "justify",
        };
        styles.push_str(&format!(" fo:text-align=\"{v}\""));
    }
    if let Some(left) = left_twips {
        styles.push_str(&format!(
            " fo:margin-left=\"{}\"",
            odt_twips_to_pt(i64::from(*left))
        ));
    }
    if let Some(first) = first_line_twips {
        styles.push_str(&format!(
            " fo:text-indent=\"{}\"",
            odt_twips_to_pt(i64::from(*first))
        ));
    }
    if let Some(percent) = line_percent {
        styles.push_str(&format!(" fo:line-height=\"{percent}%\""));
    }
    if *page_break {
        styles.push_str(" fo:break-before=\"page\"");
    }
    styles.push_str("/>");
    if *level > 0 {
        styles.push_str(&format!(
            "<style:text-properties fo:font-size=\"{}pt\" fo:font-weight=\"bold\"/>",
            heading_half_points(*level) / 2
        ));
    }
    styles.push_str("</style:style>");
    styles
}

const ODT_MEDIA_DIR: &str = "Pictures";

pub(crate) fn pack_odt_rich(doc: &RichDocument) -> Result<Vec<u8>, EasyOfficeError> {
    check_document_images(doc)?;
    let mut media = MediaParts::new();
    let mut para_styles: StyleTable<OdtParaKey> = StyleTable::new();
    let mut text_styles: StyleTable<OdtTextKey> = StyleTable::new();
    let mut body = String::with_capacity(1024);
    let lists = plan_lists(&doc.blocks);
    let mut open_lists = OdtListWriter::default();
    let mut toc_scope = TocScope::odt();
    // The header and footer go first so their styles are in the tables the body shares; styles.xml
    // and content.xml both write the whole set.
    let [header_xml, footer_xml] =
        [&doc.page_frame.header, &doc.page_frame.footer].map(|paragraphs| {
            (!paragraphs.is_empty()).then(|| {
                let mut xml = String::new();
                for p in paragraphs {
                    push_odt_paragraph(
                        &mut xml,
                        &frame_paragraph(p),
                        &mut para_styles,
                        &mut text_styles,
                    );
                }
                xml
            })
        });
    for (index, block) in doc.blocks.iter().enumerate() {
        toc_scope.enter(&mut body, &doc.blocks, index);
        match (block, lists.block_run[index]) {
            (RichBlock::Paragraph(p), Some(run)) => {
                open_lists.enter(&mut body, &lists, run, level_of(p));
                push_odt_paragraph(&mut body, p, &mut para_styles, &mut text_styles);
                continue;
            }
            _ => open_lists.leave(&mut body),
        }
        match block {
            RichBlock::Paragraph(p) => {
                push_odt_paragraph(&mut body, p, &mut para_styles, &mut text_styles);
            }
            RichBlock::Table(table) => push_odt_table(&mut body, table),
            RichBlock::Image(img) => {
                let picture = media.add(ODT_MEDIA_DIR, img);
                body.push_str(&odt_image_paragraph(img, picture.ordinal, &picture.path));
            }
        }
    }
    open_lists.leave(&mut body);
    toc_scope.enter(&mut body, &doc.blocks, doc.blocks.len());

    let mut run_styles = String::new();
    for (idx, key) in para_styles.keys.iter().enumerate() {
        run_styles.push_str(&odt_para_style_xml(idx + 1, key));
    }
    for (idx, key) in text_styles.keys.iter().enumerate() {
        run_styles.push_str(&odt_text_style_xml(idx + 1, key));
    }
    let mut styles = String::from("<office:automatic-styles>");
    if doc.blocks.iter().any(is_toc_block) {
        styles.push_str(&odt_toc_styles_xml());
    }
    styles.push_str(&run_styles);
    styles.push_str(&odt_list_styles_xml(&lists));
    styles.push_str("</office:automatic-styles>");
    let font_decls = odt_font_face_decls(&text_styles.keys);
    let styles = format!("{font_decls}{styles}");

    let content = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content ",
            "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
            "xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" ",
            "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" ",
            "xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" ",
            "xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" ",
            "xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" {}",
            "office:version=\"1.2\" title=\"{}\">{}<office:body><office:text>{}</office:text></office:body></office:document-content>"
        ),
        ODT_PICTURE_NAMESPACES,
        XmlSanitizer::escape(&doc.display_title()),
        styles,
        body
    );
    XmlSanitizer::validate_safe_xml(&content)?;
    let page = &doc.page_frame;
    let has_frame = header_xml.is_some()
        || footer_xml.is_some()
        || page.margins.is_some()
        || page.paper.is_some();
    let mut listing = media.listing();
    if has_frame {
        listing.insert(0, ("styles.xml".to_string(), "text/xml"));
    }
    let manifest = OdfCodec::build_manifest_with_media(OdfCodec::MIMETYPE_ODT, &listing);
    let mut entries = vec![
        (
            "mimetype".to_string(),
            OdfCodec::MIMETYPE_ODT.as_bytes().to_vec(),
        ),
        ("META-INF/manifest.xml".to_string(), manifest.into_bytes()),
        ("content.xml".to_string(), content.into_bytes()),
    ];
    if has_frame {
        let (layout, master) = odt_master_page(
            header_xml.as_deref(),
            footer_xml.as_deref(),
            page.margins.as_ref(),
            page.paper.as_ref(),
        );
        let styles_xml = odt_styles_xml(&font_decls, &run_styles, &layout, &master);
        XmlSanitizer::validate_safe_xml(&styles_xml)?;
        entries.push(("styles.xml".to_string(), styles_xml.into_bytes()));
    }
    entries.extend(media.into_entries());
    ZipArchiveCodec::pack_compressed(&entries)
}

// ---------------------------------------------------------------- Markdown / TXT / PDF

fn escape_markdown_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let mut prev = None;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        // `^` opens a superscript and `==` a highlight in the reader; a lone `=` is plain text.
        let doubled_equals = ch == '=' && (prev == Some('=') || chars.peek() == Some(&'='));
        if doubled_equals
            || matches!(
                ch,
                '\\' | '`' | '*' | '_' | '~' | '^' | '[' | ']' | '<' | '>' | '|' | '#' | '!'
            )
        {
            out.push('\\');
        }
        out.push(ch);
        prev = Some(ch);
    }
    out
}

/// Table cell text as Markdown: inline marks escaped, except `|`, which the table renderer
/// escapes itself.
fn escape_markdown_cell(text: &str) -> String {
    escape_markdown_inline(text).replace("\\|", "|")
}

/// Escapes characters that would start a block construct at the beginning of a line.
fn escape_markdown_line_start(line: &str) -> String {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];
    if let Some(first) = trimmed.chars().next() {
        if matches!(first, '-' | '+' | '=') {
            return format!("{indent}\\{trimmed}");
        }
        let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
        if digits > 0 {
            if let Some(marker @ ('.' | ')')) = trimmed[digits..].chars().next() {
                return format!(
                    "{indent}{}\\{marker}{}",
                    &trimmed[..digits],
                    &trimmed[digits + 1..]
                );
            }
        }
    }
    line.to_string()
}

fn render_markdown_runs(p: &RichParagraph) -> String {
    let mut out = String::new();
    for run in &p.runs {
        for (l_idx, line) in run.text.split('\n').enumerate() {
            if l_idx > 0 {
                out.push_str("\\\n");
            }
            let core = line.trim();
            if core.is_empty() {
                out.push_str(line);
                continue;
            }
            let lead = &line[..line.len() - line.trim_start().len()];
            let trail = &line[line.trim_end().len()..];
            let mut piece = escape_markdown_inline(core);
            if run.italic {
                piece = format!("*{piece}*");
            }
            if run.bold {
                piece = format!("**{piece}**");
            }
            if run.strike {
                piece = format!("~~{piece}~~");
            }
            if run.underline {
                piece = format!("<u>{piece}</u>");
            }
            out.push_str(lead);
            out.push_str(&piece);
            out.push_str(trail);
        }
    }
    out.split('\n')
        .map(escape_markdown_line_start)
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn render_markdown_rich(doc: &RichDocument) -> String {
    // A document that already opens with a level-1 heading (e.g. an opened Markdown file) keeps
    // that heading as its title instead of gaining a duplicate title line.
    let opens_with_h1 = matches!(
        doc.blocks.first(),
        Some(RichBlock::Paragraph(p)) if p.heading_level == 1
    );
    let mut parts: Vec<String> = if opens_with_h1 {
        Vec::new()
    } else {
        vec![format!(
            "# {}",
            escape_markdown_inline(&doc.display_title())
        )]
    };
    let mut list_buf: Vec<String> = Vec::new();
    let mut ordinal = 0usize;
    let flush = |parts: &mut Vec<String>, list_buf: &mut Vec<String>| {
        if !list_buf.is_empty() {
            parts.push(list_buf.join("\n"));
            list_buf.clear();
        }
    };
    for block in &doc.blocks {
        match block {
            RichBlock::Paragraph(p) if (1..=6).contains(&p.heading_level) => {
                flush(&mut parts, &mut list_buf);
                ordinal = 0;
                let hashes = "#".repeat(usize::from(p.heading_level));
                parts.push(format!("{hashes} {}", render_markdown_runs(p)));
            }
            RichBlock::Paragraph(p) if p.list.is_some() => {
                let marker = if p.list == Some(RichListKind::Ordered) {
                    ordinal += 1;
                    format!("{ordinal}.")
                } else {
                    "-".to_string()
                };
                list_buf.push(format!("{marker} {}", render_markdown_runs(p)));
            }
            RichBlock::Paragraph(p) => {
                flush(&mut parts, &mut list_buf);
                ordinal = 0;
                let text = render_markdown_runs(p);
                if !text.trim().is_empty() {
                    parts.push(text);
                }
            }
            RichBlock::Image(img) => {
                flush(&mut parts, &mut list_buf);
                ordinal = 0;
                parts.push(escape_markdown_line_start(&escape_markdown_inline(
                    img.fallback_text(),
                )));
            }
            RichBlock::Table(table) => {
                flush(&mut parts, &mut list_buf);
                ordinal = 0;
                if table.rows.iter().any(|r| !r.is_empty()) {
                    let rows = table
                        .rows
                        .iter()
                        .map(|row| row.iter().map(|c| escape_markdown_cell(c)).collect())
                        .collect();
                    parts.push(CommonMarkCodec::render(&[MarkdownBlock::Table(rows)]));
                }
            }
        }
    }
    flush(&mut parts, &mut list_buf);
    parts.join("\n\n")
}

/// Plain-text lines of one block: paragraph text, alt text of a picture, table rows joined by
/// `cell_sep`.
fn push_plain_lines(block: &RichBlock, cell_sep: &str, lines: &mut Vec<String>) {
    match block {
        RichBlock::Paragraph(p) => lines.push(p.plain_text()),
        RichBlock::Image(img) => lines.push(img.fallback_text().to_string()),
        RichBlock::Table(table) => {
            for row in table.rows.iter().filter(|r| !r.is_empty()) {
                lines.push(row.join(cell_sep));
            }
        }
    }
}

/// Ordered plain text: optional title line, paragraph text, table rows joined by `cell_sep`.
pub(crate) fn render_plain_text(doc: &RichDocument, cell_sep: &str, with_title: bool) -> String {
    let mut lines = Vec::with_capacity(doc.blocks.len() + 1);
    if with_title {
        lines.push(doc.display_title());
    }
    for block in &doc.blocks {
        push_plain_lines(block, cell_sep, &mut lines);
    }
    lines.join("\n")
}

/// Widest / tallest picture area on a PDF page (A4 minus margins), in points.
const PDF_IMAGE_MAX_WIDTH_PT: u64 = PDF_PAGE_WIDTH_PT - 2 * PDF_IMAGE_MARGIN_PT;
const PDF_IMAGE_MAX_HEIGHT_PT: u64 = PDF_PAGE_HEIGHT_PT - 2 * PDF_IMAGE_MARGIN_PT;

fn flush_pdf_text(
    entries: &mut Vec<PdfPageEntry>,
    lines: &mut Vec<String>,
    stamp: &mut Option<String>,
    pass: &mut Option<HighlightPass<'_>>,
) {
    if lines.is_empty() {
        return;
    }
    let page = PdfPageModel {
        page_number: 0,
        rotation_degrees: 0,
        body_text: lines.join("\n"),
        approval_stamp: stamp.take(),
    };
    lines.clear();
    entries.extend(text_entries(&page, 1, pass.as_mut()));
}

/// Text pages with every embeddable picture on a page of its own, in document order; pictures
/// the PDF writer cannot embed are printed as alternative text.
fn pdf_entries(
    doc: &RichDocument,
    mut stamp: Option<String>,
    pass: &mut Option<HighlightPass<'_>>,
) -> Vec<PdfPageEntry> {
    let mut entries = Vec::new();
    let mut lines = Vec::new();
    for block in &doc.blocks {
        let embedded = match block {
            RichBlock::Image(img) => pdf_image_object(img).map(|object| (img, object)),
            _ => None,
        };
        let Some((img, image)) = embedded else {
            push_plain_lines(block, " | ", &mut lines);
            continue;
        };
        flush_pdf_text(&mut entries, &mut lines, &mut stamp, pass);
        let (cx, cy) = img.display_emu();
        let (width_pt, height_pt) = fit_within(
            emu_to_points(cx),
            emu_to_points(cy),
            PDF_IMAGE_MAX_WIDTH_PT,
            PDF_IMAGE_MAX_HEIGHT_PT,
        );
        entries.push(PdfPageEntry::Image(PdfImagePage {
            page_number: 0,
            rotation_degrees: 0,
            image,
            width_pt,
            height_pt,
        }));
    }
    flush_pdf_text(&mut entries, &mut lines, &mut stamp, pass);
    for (idx, entry) in entries.iter_mut().enumerate() {
        match entry {
            PdfPageEntry::Text(page, _) => page.page_number = idx + 1,
            PdfPageEntry::Image(page) => page.page_number = idx + 1,
        }
    }
    entries
}

/// The PDF of a document, and notes for the caller (a highlight phrase that was not found, or
/// occurrences left out over the annotation limit).
pub(crate) fn serialize_pdf_rich(
    doc: &RichDocument,
    options: &PdfWriteOptions,
) -> (Vec<u8>, Vec<String>) {
    let stamp = options
        .approval_stamp
        .clone()
        .or_else(|| doc.official_agency.clone());
    let highlight = options.highlight.as_ref().filter(|h| !h.text.is_empty());
    let mut pass = highlight.map(HighlightPass::new);
    if doc.blocks.iter().any(|b| matches!(b, RichBlock::Image(_))) {
        let mut entries = pdf_entries(doc, stamp.clone(), &mut pass);
        if entries.iter().any(|e| matches!(e, PdfPageEntry::Image(_))) {
            options.rotate(&mut entries);
            let bytes = PdfSerializer::serialize_entries(
                &doc.display_title(),
                options.watermark.as_deref(),
                &entries,
            );
            return (bytes, pass.map(|p| p.warnings()).unwrap_or_default());
        }
        pass = highlight.map(HighlightPass::new);
    }
    let page = PdfPageModel {
        page_number: 1,
        rotation_degrees: i32::from(options.rotation_degrees),
        body_text: render_plain_text(doc, " | ", false),
        approval_stamp: stamp,
    };
    if let Some(pass) = pass.as_mut() {
        let entries = text_entries(&page, 1, Some(pass));
        let bytes = PdfSerializer::serialize_entries(
            &doc.display_title(),
            options.watermark.as_deref(),
            &entries,
        );
        return (bytes, pass.warnings());
    }
    let bytes = PdfSerializer::serialize(&PdfDocumentModel {
        title: doc.display_title(),
        pages: vec![page],
        watermark_text: options.watermark.clone(),
    });
    (bytes, Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_escapes_block_starters_and_inline_markers() {
        assert_eq!(escape_markdown_line_start("1. 항목"), "1\\. 항목");
        assert_eq!(escape_markdown_line_start("- 항목"), "\\- 항목");
        assert_eq!(escape_markdown_inline("*a*#"), "\\*a\\*\\#");
    }
}
