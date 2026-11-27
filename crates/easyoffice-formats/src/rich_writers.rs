//! Writers that keep [`RichDocument`] block order and run/paragraph formatting for DOCX
//! (`w:rPr`/`w:pPr`/`w:pStyle` plus `word/styles.xml`), ODT (automatic styles), Markdown
//! (`**`/`*`/`~~`/`#`), TXT, and PDF (ordered text); RTF lives in `rich_rtf_writer.rs`. User text
//! is always escaped for the target syntax and never parsed.

use crate::docx_notes_writer::{
    build_docx_note_parts, push_docx_paragraph_runs, push_docx_text_tagged,
};
use crate::odt_notes_writer::{push_odt_paragraph_runs, OdtNotesContext};
use crate::pdf_highlight::HighlightPass;
use crate::pdf_image::pdf_image_object;
use crate::pdf_writer::{
    text_entries, PdfImagePage, PdfPageEntry, PdfWriteOptions, PDF_IMAGE_MARGIN_PT,
    PDF_PAGE_HEIGHT_PT, PDF_PAGE_WIDTH_PT,
};
use crate::rich_defaults::RichDefaults;
use crate::rich_document::{
    RichAlignment, RichBlock, RichColumns, RichDocument, RichLineSpacing, RichListKind,
    RichParagraph, MAX_PARA_SPACING_TWIPS, TOC_BOOKMARK_PREFIX,
};
use crate::rich_image::{check_document_images, fit_within};
use crate::rich_image_writers::{
    docx_content_types, docx_image_paragraph, docx_image_relationship, emu_to_points, MediaParts,
    DOCX_PICTURE_NAMESPACES,
};
use crate::rich_link_writers::{DocxLinks, DOCX_LINK_STYLE_XML};
use crate::rich_list_writers::{
    docx_num_pr, docx_numbering_xml, has_docx_numbering, list_kind_of, plan_lists,
};
pub(crate) use crate::rich_markdown_writer::render_markdown_rich;
use crate::rich_notes::{collect_note_ordinals, is_deleted_run, RichNoteKind};
use crate::rich_page_frame::{
    docx_cols_xml, docx_frame_parts_and_refs, docx_page_margins, docx_paper, docx_settings,
    page_text_width_twips, DOCX_SETTINGS_PART,
};
use crate::rich_rtf_writer::line_spacing_240ths;
use crate::rich_run_writers::OdtTextKey;
use crate::rich_table_writers::{push_docx_table, DocxTableContext};
use crate::rich_toc_writers::{
    docx_field_edges_cached, docx_toc_page, docx_toc_style_id, docx_toc_styles_xml, docx_toc_tabs,
    is_toc_block, odt_toc_page, odt_toc_paragraph_style, TocScope,
};
use crate::{
    EasyOfficeError, PdfDocumentModel, PdfPageModel, PdfSerializer, XmlSanitizer, ZipArchiveCodec,
};

/// Heading font sizes in half-points for levels 1–6.
const HEADING_HALF_POINTS: [u32; 6] = [32, 28, 26, 24, 22, 22];

pub(crate) fn heading_half_points(level: u8) -> u32 {
    HEADING_HALF_POINTS[usize::from(level.clamp(1, 6)) - 1]
}

// ---------------------------------------------------------------- DOCX

const DOCX_NUMBERING_REL_ID: &str = "rIdNumbering";

/// `<w:rFonts>` of the document defaults: the Latin face for `ascii`/`hAnsi`, the East Asian one
/// for `eastAsia`; none when the file names no typeface.
fn docx_default_fonts(defaults: &RichDefaults) -> String {
    let mut attrs = String::new();
    if let Some(latin) = defaults.font.as_deref() {
        let f = XmlSanitizer::escape(latin);
        attrs.push_str(&format!(" w:ascii=\"{f}\" w:hAnsi=\"{f}\""));
    }
    if let Some(asian) = defaults.east_asian_font.as_deref() {
        attrs.push_str(&format!(" w:eastAsia=\"{}\"", XmlSanitizer::escape(asian)));
    }
    format!("<w:rFonts{attrs} w:cs=\"Times New Roman\"/>")
}

/// `w:pPr` of the document defaults: spacing and, when it is not the start edge, `w:jc`.
fn docx_default_paragraph(defaults: &RichDefaults) -> String {
    let jc = match defaults.alignment {
        Some(RichAlignment::Justify) => "<w:jc w:val=\"both\"/>",
        Some(RichAlignment::Center) => "<w:jc w:val=\"center\"/>",
        Some(RichAlignment::Right) => "<w:jc w:val=\"right\"/>",
        Some(RichAlignment::Left) | None => "",
    };
    let before = match defaults.space_before_twips {
        0 => String::new(),
        twips => format!(" w:before=\"{twips}\""),
    };
    format!(
        "<w:spacing{before} w:after=\"{}\" w:line=\"{}\" w:lineRule=\"auto\"/>{jc}",
        defaults.space_after_twips, defaults.line_240ths
    )
}

fn docx_styles_xml(has_toc: bool, defaults: &RichDefaults) -> String {
    let mut xml = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
            "<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
            "<w:docDefaults><w:rPrDefault><w:rPr>{fonts}",
            "<w:kern w:val=\"2\"/><w:sz w:val=\"{sz}\"/><w:szCs w:val=\"22\"/><w:lang w:val=\"en-US\" w:eastAsia=\"ko-KR\" w:bidi=\"ar-SA\"/></w:rPr></w:rPrDefault>",
            "<w:pPrDefault><w:pPr>{ppr}</w:pPr></w:pPrDefault></w:docDefaults>",
            "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style>"
        ),
        fonts = docx_default_fonts(defaults),
        sz = defaults.size_half_points,
        ppr = docx_default_paragraph(defaults),
    );
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
    xml.push_str("<w:style w:type=\"table\" w:default=\"1\" w:styleId=\"TableNormal\"><w:name w:val=\"Normal Table\"/><w:pPr><w:spacing w:after=\"0\" w:line=\"240\" w:lineRule=\"auto\"/><w:jc w:val=\"left\"/></w:pPr><w:tblPr><w:tblInd w:w=\"0\" w:type=\"dxa\"/><w:tblCellMar><w:left w:w=\"108\" w:type=\"dxa\"/><w:right w:w=\"108\" w:type=\"dxa\"/></w:tblCellMar></w:tblPr></w:style>");
    xml.push_str("</w:styles>");
    xml
}

pub(crate) fn push_docx_text(out: &mut String, text: &str) {
    push_docx_text_tagged(out, text, "w:t");
}

/// `w:line` and `w:lineRule` of a line spacing: `auto` counts 240ths of a line, `atLeast` and
/// `exact` twips; the between-lines rule has no DOCX form and is written as its equal multiple.
pub(crate) fn docx_line_attributes(spacing: RichLineSpacing) -> String {
    let (line, rule) = match spacing {
        RichLineSpacing::AtLeast(twips) => (u32::from(twips), "atLeast"),
        RichLineSpacing::Exact(twips) => (u32::from(twips), "exact"),
        other => (line_spacing_240ths(other.multiple_percent()), "auto"),
    };
    format!(" w:line=\"{line}\" w:lineRule=\"{rule}\"")
}

/// WordprocessingML `w:spacing` (line height, 240 = single) and `w:ind` for a paragraph, in
/// schema order; empty when the paragraph inherits both.
pub(crate) fn docx_paragraph_layout(p: &RichParagraph) -> String {
    let mut xml = String::new();
    if p.space_before_twips.is_some() || p.space_after_twips.is_some() || p.line_spacing.is_some() {
        xml.push_str("<w:spacing");
        if let Some(before) = p.space_before_twips {
            xml.push_str(&format!(
                " w:before=\"{}\"",
                before.min(MAX_PARA_SPACING_TWIPS)
            ));
        }
        if let Some(after) = p.space_after_twips {
            xml.push_str(&format!(
                " w:after=\"{}\"",
                after.min(MAX_PARA_SPACING_TWIPS)
            ));
        }
        if let Some(spacing) = p.line_spacing {
            xml.push_str(&docx_line_attributes(spacing));
        }
        xml.push_str("/>");
    }
    if p.left_indent_twips.is_some()
        || p.right_indent_twips.is_some()
        || p.first_line_indent_twips.is_some()
    {
        xml.push_str("<w:ind");
        if let Some(left) = p.left_indent_twips {
            xml.push_str(&format!(" w:left=\"{left}\""));
        }
        if let Some(right) = p.right_indent_twips {
            xml.push_str(&format!(" w:right=\"{right}\""));
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
    sect_pr: Option<&str>,
) {
    out.push_str("<w:p>");
    let list_kind = list_kind_of(p);
    let is_task = list_kind == Some(RichListKind::Task) && p.toc.is_none();
    let style = if let Some(role) = p.toc {
        Some(docx_toc_style_id(role))
    } else if (1..=6).contains(&p.heading_level) {
        Some(format!("Heading{}", p.heading_level))
    } else {
        list_kind.and_then(|kind| match kind {
            RichListKind::Bullet => Some("ListBullet".to_string()),
            RichListKind::Ordered => Some("ListNumber".to_string()),
            RichListKind::Task => None,
        })
    };
    let jc = p.alignment.map(|a| match a {
        RichAlignment::Left => "left",
        RichAlignment::Center => "center",
        RichAlignment::Right => "right",
        RichAlignment::Justify => "both",
    });
    let mut layout = docx_paragraph_layout(p);
    if is_task && p.left_indent_twips.is_none() && p.first_line_indent_twips.is_none() {
        let left = 720 * (u32::from(p.list_level.min(8)) + 1);
        layout.push_str(&format!("<w:ind w:left=\"{left}\" w:hanging=\"360\"/>"));
    }
    let tabs = docx_toc_tabs(p);
    let num_run = run.filter(|_| !is_task);
    if style.is_some()
        || num_run.is_some()
        || jc.is_some()
        || !layout.is_empty()
        || !tabs.is_empty()
        || p.page_break_before
        || sect_pr.is_some()
    {
        out.push_str("<w:pPr>");
        if let Some(s) = style {
            out.push_str(&format!("<w:pStyle w:val=\"{s}\"/>"));
        }
        if p.page_break_before {
            out.push_str("<w:pageBreakBefore/>");
        }
        if let Some(run) = num_run {
            out.push_str(&docx_num_pr(run, p));
        }
        out.push_str(&tabs);
        out.push_str(&layout);
        if let Some(j) = jc {
            out.push_str(&format!("<w:jc w:val=\"{j}\"/>"));
        }
        if let Some(sp) = sect_pr {
            out.push_str(sp);
        }
        out.push_str("</w:pPr>");
    }
    out.push_str(&field.0);
    if let Some(number) = p.bookmark {
        out.push_str(&format!(
            "<w:bookmarkStart w:id=\"{number}\" w:name=\"{TOC_BOOKMARK_PREFIX}{number}\"/>"
        ));
    }
    if is_task {
        let (val, glyph) = if p.task_checked {
            ("1", "\u{2612}")
        } else {
            ("0", "\u{2610}")
        };
        out.push_str(&format!(
            concat!(
                "<w:sdt><w:sdtPr><w14:checkbox><w14:checked w14:val=\"{val}\"/>",
                "<w14:checkedState w14:val=\"2612\" w14:font=\"MS Gothic\"/>",
                "<w14:uncheckedState w14:val=\"2610\" w14:font=\"MS Gothic\"/>",
                "</w14:checkbox></w:sdtPr><w:sdtContent><w:r><w:t>{glyph}</w:t></w:r></w:sdtContent></w:sdt>",
                "<w:r><w:t xml:space=\"preserve\"> </w:t></w:r>"
            ),
            val = val,
            glyph = glyph,
        ));
    }
    push_docx_paragraph_runs(out, p, links);
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
    let entries = docx_rich_entries(doc)?;
    ZipArchiveCodec::pack_compressed(&entries)
}

pub(crate) fn docx_rich_entries(
    doc: &RichDocument,
) -> Result<Vec<(String, Vec<u8>)>, EasyOfficeError> {
    check_document_images(doc)?;
    let rels = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>",
        "<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/>",
        "</Relationships>"
    );
    let core_xml = crate::ooxml_shared_parts::core_properties_xml(&doc.display_title());
    let defaults = RichDefaults::of(doc);
    let styles = docx_styles_xml(doc.blocks.iter().any(is_toc_block), &defaults);
    let lists = plan_lists(&doc.blocks);

    let mut body = String::with_capacity(1024);
    body.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    body.push_str(
        "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\"",
    );
    body.push_str(DOCX_PICTURE_NAMESPACES);
    body.push_str("><w:body>");
    let mut media = MediaParts::new();
    let mut image_rels = String::new();
    let mut links = DocxLinks::default();
    let mut toc_scope = TocScope::docx();
    let mut toc_field_cache = None;
    let pg_sz = docx_paper(doc.page_frame.paper.as_ref());
    let pg_mar = docx_page_margins(doc.page_frame.margins.as_ref(), doc.page_frame.binding);
    let mut section_end_cols: Vec<Option<Option<RichColumns>>> = vec![None; doc.blocks.len()];
    let mut seen_later_para: Option<Option<RichColumns>> = None;
    let mut last_para_cols: Option<RichColumns> = None;
    for (idx, block) in doc.blocks.iter().enumerate().rev() {
        if let RichBlock::Paragraph(p) = block {
            if seen_later_para.is_none() {
                last_para_cols = p.columns;
            } else if seen_later_para != Some(p.columns) {
                section_end_cols[idx] = Some(p.columns);
            }
            seen_later_para = Some(p.columns);
        }
    }
    let text_width_twips = page_text_width_twips(&doc.page_frame);
    let (frame_entries, mut frame) = docx_frame_parts_and_refs(&doc.page_frame);
    // The frame belongs to the first section: its references and `titlePg` go on the `sectPr` that ends it, the later sections
    // inherit them (a section with none of its own would show no header at all).
    let first_section_end = section_end_cols.iter().position(Option::is_some);
    for (index, block) in doc.blocks.iter().enumerate() {
        toc_scope.enter(&mut body, &doc.blocks, index);
        match block {
            RichBlock::Paragraph(p) => {
                let edges = docx_field_edges_cached(&doc.blocks, index, &mut toc_field_cache);
                let inline_sect = section_end_cols[index].map(|cols| {
                    let cols_xml = docx_cols_xml(
                        cols,
                        doc.page_frame.paper.as_ref(),
                        doc.page_frame.margins.as_ref(),
                    );
                    let (refs, title_pg) = if first_section_end == Some(index) {
                        (frame.section.as_str(), frame.title_pg)
                    } else {
                        ("", "")
                    };
                    format!("<w:sectPr>{refs}<w:type w:val=\"continuous\"/>{pg_sz}{pg_mar}{cols_xml}{title_pg}</w:sectPr>")
                });
                push_docx_paragraph(
                    &mut body,
                    p,
                    lists.block_run[index],
                    &mut links,
                    &edges,
                    inline_sect.as_deref(),
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
                let mut ctx = DocxTableContext {
                    links: &mut links,
                    media: &mut media,
                    image_rels: &mut image_rels,
                    first_image_rel: DOCX_FIRST_IMAGE_REL,
                    media_dir: DOCX_MEDIA_DIR,
                    text_width_twips,
                };
                push_docx_table(&mut body, table, &mut ctx);
                // A table cannot end the body or touch another table without a paragraph.
                body.push_str("<w:p/>");
            }
        }
    }
    toc_scope.enter(&mut body, &doc.blocks, doc.blocks.len());
    let note_parts = build_docx_note_parts(doc)?;
    frame.relationships.push_str(&note_parts.relationships);
    frame.content_types.push_str(&note_parts.content_types);
    let settings = docx_settings(
        doc.page_frame.binding,
        doc.page_frame.even_odd_different,
        doc.track_changes,
        defaults.tab_stop_twips,
    );
    frame.relationships.push_str(&settings.1);
    frame.content_types.push_str(&settings.2);
    let last_cols_xml = docx_cols_xml(
        last_para_cols,
        doc.page_frame.paper.as_ref(),
        doc.page_frame.margins.as_ref(),
    );
    let (refs, title_pg) = if first_section_end.is_none() {
        (frame.section.as_str(), frame.title_pg)
    } else {
        ("", "")
    };
    body.push_str(&format!(
        "<w:sectPr>{refs}{pg_sz}{pg_mar}{last_cols_xml}{title_pg}</w:sectPr>"
    ));
    body.push_str("</w:body></w:document>");

    let has_lists = has_docx_numbering(&lists);
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
    {
        XmlSanitizer::validate_safe_xml(xml)?;
    }
    for (_, part_bytes) in &frame_entries {
        if let Ok(xml) = std::str::from_utf8(part_bytes) {
            XmlSanitizer::validate_safe_xml(xml)?;
        }
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
    // The settings part holds fixed flags only, so it needs no scan.
    entries.push((DOCX_SETTINGS_PART.to_string(), settings.0.into_bytes()));
    entries.extend(note_parts.entries);
    entries.extend(frame_entries);
    entries.extend(media.into_entries());
    Ok(entries)
}

// ---------------------------------------------------------------- ODT

/// Interned automatic-style keys, in first-use order.
pub(crate) struct StyleTable<K: Eq + std::hash::Hash + Clone> {
    pub(crate) keys: Vec<K>,
    pub(crate) index: std::collections::HashMap<K, usize>,
}

impl<K: Eq + std::hash::Hash + Clone> StyleTable<K> {
    pub(crate) fn new() -> Self {
        Self {
            keys: Vec::new(),
            index: std::collections::HashMap::new(),
        }
    }

    pub(crate) fn intern(&mut self, key: &K) -> usize {
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
pub(crate) struct OdtParaKey {
    pub(crate) align: Option<RichAlignment>,
    pub(crate) level: u8,
    pub(crate) left_twips: Option<u32>,
    pub(crate) right_twips: Option<u32>,
    pub(crate) first_line_twips: Option<i32>,
    pub(crate) line_spacing: Option<RichLineSpacing>,
    pub(crate) space_before_twips: Option<u32>,
    pub(crate) space_after_twips: Option<u32>,
    pub(crate) page_break: bool,
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

pub(crate) fn push_odt_paragraph(
    body: &mut String,
    p: &RichParagraph,
    para_styles: &mut StyleTable<OdtParaKey>,
    text_styles: &mut StyleTable<OdtTextKey>,
    notes_ctx: &mut OdtNotesContext,
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
        right_twips: p.right_indent_twips,
        first_line_twips: p.first_line_indent_twips,
        line_spacing: p.line_spacing,
        space_before_twips: p.space_before_twips.map(|v| v.min(MAX_PARA_SPACING_TWIPS)),
        space_after_twips: p.space_after_twips.map(|v| v.min(MAX_PARA_SPACING_TWIPS)),
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
    push_odt_paragraph_runs(body, p, text_styles, notes_ctx);
    body.push_str(&odt_toc_page(p));
    if let Some(number) = p.bookmark {
        body.push_str(&format!(
            "<text:bookmark-end text:name=\"{TOC_BOOKMARK_PREFIX}{number}\"/>"
        ));
    }
    body.push_str(if level > 0 { "</text:h>" } else { "</text:p>" });
}

/// Twips as a point length with at most two decimals (`36pt`, `7.5pt`).
pub(crate) fn odt_twips_to_pt(twips: i64) -> String {
    let sign = if twips < 0 { "-" } else { "" };
    let abs = twips.unsigned_abs();
    let (whole, tenths_of_twip) = (abs / 20, abs % 20 * 5);
    if tenths_of_twip == 0 {
        format!("{sign}{whole}pt")
    } else {
        format!("{sign}{whole}.{tenths_of_twip:02}pt")
    }
}

/// `style:style` XML of one paragraph style (1-based `number`).
pub(crate) fn odt_para_style_xml(number: usize, key: &OdtParaKey) -> String {
    let OdtParaKey {
        align,
        level,
        left_twips,
        right_twips,
        first_line_twips,
        line_spacing,
        space_before_twips,
        space_after_twips,
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
    if let Some(before) = space_before_twips {
        styles.push_str(&format!(
            " fo:margin-top=\"{}\"",
            odt_twips_to_pt(i64::from((*before).min(MAX_PARA_SPACING_TWIPS)))
        ));
    }
    if let Some(after) = space_after_twips {
        styles.push_str(&format!(
            " fo:margin-bottom=\"{}\"",
            odt_twips_to_pt(i64::from((*after).min(MAX_PARA_SPACING_TWIPS)))
        ));
    }
    if let Some(left) = left_twips {
        styles.push_str(&format!(
            " fo:margin-left=\"{}\"",
            odt_twips_to_pt(i64::from(*left))
        ));
    }
    if let Some(right) = right_twips {
        styles.push_str(&format!(
            " fo:margin-right=\"{}\"",
            odt_twips_to_pt(i64::from(*right))
        ));
    }
    if let Some(first) = first_line_twips {
        styles.push_str(&format!(
            " fo:text-indent=\"{}\"",
            odt_twips_to_pt(i64::from(*first))
        ));
    }
    match line_spacing {
        Some(RichLineSpacing::Multiple(percent)) => {
            styles.push_str(&format!(" fo:line-height=\"{percent}%\""));
        }
        Some(RichLineSpacing::Exact(twips)) => styles.push_str(&format!(
            " fo:line-height=\"{}\"",
            odt_twips_to_pt(i64::from(*twips))
        )),
        Some(RichLineSpacing::AtLeast(twips)) => styles.push_str(&format!(
            " style:line-height-at-least=\"{}\"",
            odt_twips_to_pt(i64::from(*twips))
        )),
        Some(RichLineSpacing::Between(twips)) => styles.push_str(&format!(
            " style:line-spacing=\"{}\"",
            odt_twips_to_pt(i64::from(*twips))
        )),
        None => {}
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

pub(crate) use crate::odt_rich_writer::pack_odt_rich;

// ---------------------------------------------------------------- Markdown / TXT / PDF

fn paragraph_plain_with_notes(
    p: &RichParagraph,
    ordinals: &std::collections::HashMap<(RichNoteKind, u32), usize>,
) -> String {
    let mut out = String::new();
    for run in &p.runs {
        if is_deleted_run(run) {
            continue;
        }
        if let Some(key) = run.note_ref {
            if let Some(&ord) = ordinals.get(&key) {
                out.push_str(&format!("[{ord}]"));
            }
        } else {
            out.push_str(&run.text);
        }
    }
    out
}

/// Plain-text lines of one block: paragraph text, alt text of a picture, table rows joined by
/// `cell_sep`.
fn push_plain_lines(
    block: &RichBlock,
    cell_sep: &str,
    ordinals: &std::collections::HashMap<(RichNoteKind, u32), usize>,
    lines: &mut Vec<String>,
) {
    match block {
        RichBlock::Paragraph(p) => {
            let text = paragraph_plain_with_notes(p, ordinals);
            if p.list == Some(RichListKind::Task) && !(1..=6).contains(&p.heading_level) {
                let indent = "  ".repeat(usize::from(p.list_level.min(8)));
                let mark = if p.task_checked {
                    "\u{2611}"
                } else {
                    "\u{2610}"
                };
                lines.push(format!("{indent}{mark} {text}"));
            } else {
                lines.push(text);
            }
        }
        RichBlock::Image(img) => lines.push(img.fallback_text().to_string()),
        RichBlock::Table(table) => {
            for row in table.rows.iter().filter(|r| !r.is_empty()) {
                lines.push(row.join(cell_sep));
            }
        }
    }
}

fn push_plain_notes_appendix(ordered: &[(crate::RichNote, usize)], lines: &mut Vec<String>) {
    for (note, ord) in ordered {
        let single_line = note.text.replace('\n', " ");
        lines.push(format!("[{ord}] {single_line}"));
    }
}

/// Ordered plain text: optional title line, paragraph text, table rows joined by `cell_sep`.
pub(crate) fn render_plain_text(doc: &RichDocument, cell_sep: &str, with_title: bool) -> String {
    let (ordered_notes, ordinals) = collect_note_ordinals(doc);
    let mut lines = Vec::with_capacity(doc.blocks.len() + ordered_notes.len() + 1);
    if with_title {
        lines.push(doc.display_title());
    }
    for block in &doc.blocks {
        push_plain_lines(block, cell_sep, &ordinals, &mut lines);
    }
    push_plain_notes_appendix(&ordered_notes, &mut lines);
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

fn push_pdf_image_page(
    entries: &mut Vec<PdfPageEntry>,
    img: &crate::RichImage,
    image: crate::pdf_image::PdfImageObject,
) {
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

/// Text pages with every embeddable picture on a page of its own, in document order; pictures
/// the PDF writer cannot embed are printed as alternative text.
fn pdf_entries(
    doc: &RichDocument,
    mut stamp: Option<String>,
    pass: &mut Option<HighlightPass<'_>>,
) -> Vec<PdfPageEntry> {
    let (ordered_notes, ordinals) = collect_note_ordinals(doc);
    let mut entries = Vec::new();
    let mut lines = Vec::new();
    for block in &doc.blocks {
        match block {
            RichBlock::Image(img) => {
                if let Some(image) = pdf_image_object(img) {
                    flush_pdf_text(&mut entries, &mut lines, &mut stamp, pass);
                    push_pdf_image_page(&mut entries, img, image);
                } else {
                    push_plain_lines(block, " | ", &ordinals, &mut lines);
                }
            }
            RichBlock::Table(table) => {
                push_plain_lines(block, " | ", &ordinals, &mut lines);
                for ci in table.validated_cell_images() {
                    if let Some(image) = pdf_image_object(&ci.image) {
                        flush_pdf_text(&mut entries, &mut lines, &mut stamp, pass);
                        push_pdf_image_page(&mut entries, &ci.image, image);
                    } else {
                        lines.push(ci.image.fallback_text().to_string());
                    }
                }
            }
            RichBlock::Paragraph(_) => {
                push_plain_lines(block, " | ", &ordinals, &mut lines);
            }
        }
    }
    push_plain_notes_appendix(&ordered_notes, &mut lines);
    flush_pdf_text(&mut entries, &mut lines, &mut stamp, pass);
    for (idx, entry) in entries.iter_mut().enumerate() {
        match entry {
            PdfPageEntry::Text(page, _) => page.page_number = idx + 1,
            PdfPageEntry::Image(page) => page.page_number = idx + 1,
            PdfPageEntry::Laid(page) => page.page_number = idx + 1,
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
    let has_images = doc.blocks.iter().any(|b| match b {
        RichBlock::Image(_) => true,
        RichBlock::Table(t) => !t.cell_images.is_empty(),
        RichBlock::Paragraph(_) => false,
    });
    if has_images {
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
    if highlight.is_none() && stamp.is_none() {
        if let Some(bytes) = crate::pdf_laid::laid_document_pdf(doc, options) {
            return (bytes, Vec::new());
        }
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
