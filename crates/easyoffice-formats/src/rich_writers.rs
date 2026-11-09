//! Writers that keep [`RichDocument`] block order and run/paragraph formatting for DOCX
//! (`w:rPr`/`w:pPr`/`w:pStyle` plus `word/styles.xml`), ODT (automatic styles), RTF (control
//! words), Markdown (`**`/`*`/`~~`/`#`), TXT, and PDF (ordered text). User text is always escaped
//! for the target syntax and never parsed.

use crate::pdf_image::pdf_image_object;
use crate::pdf_writer::{
    paginate_pages, PdfImagePage, PdfPageEntry, PDF_IMAGE_MARGIN_PT, PDF_PAGE_HEIGHT_PT,
    PDF_PAGE_WIDTH_PT,
};
use crate::rich_document::{
    RichAlignment, RichBlock, RichDocument, RichListKind, RichParagraph, RichRun,
};
use crate::rich_image::{check_document_images, fit_within};
use crate::rich_image_writers::{
    docx_content_type_defaults, docx_image_paragraph, docx_image_relationship, emu_to_points,
    odt_image_paragraph, MediaParts, DOCX_PICTURE_NAMESPACES, ODT_PICTURE_NAMESPACES,
};
use crate::rich_rtf_table::rtf_table_lines;
use crate::rich_run_writers::{
    docx_run_properties, has_extra_run_format, odt_font_face_decls, odt_text_key,
    odt_text_key_is_plain, odt_text_style_xml, OdtTextKey, RtfRunTables,
};
use crate::rich_table_writers::{push_docx_table, push_odt_table};
use crate::{
    CommonMarkCodec, EasyOfficeError, MarkdownBlock, OdfCodec, PdfDocumentModel, PdfPageModel,
    PdfSerializer, RtfCodec, XmlSanitizer, ZipArchiveCodec,
};

/// `w:spacing/@w:line` value of single line spacing (240ths of a line).
const DOCX_SINGLE_LINE_240THS: u32 = 240;

/// Heading font sizes in half-points for levels 1–6.
const HEADING_HALF_POINTS: [u32; 6] = [32, 28, 26, 24, 22, 22];

pub(crate) fn heading_half_points(level: u8) -> u32 {
    HEADING_HALF_POINTS[usize::from(level.clamp(1, 6)) - 1]
}

// ---------------------------------------------------------------- DOCX

const DOCX_STYLES_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";

fn docx_styles_xml() -> String {
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
            "<w:style w:type=\"paragraph\" w:styleId=\"{id}\"><w:name w:val=\"{name}\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:ind w:left=\"720\" w:hanging=\"360\"/></w:pPr></w:style>"
        ));
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
fn docx_paragraph_layout(p: &RichParagraph) -> String {
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

fn push_docx_paragraph(out: &mut String, p: &RichParagraph) {
    out.push_str("<w:p>");
    let style = if (1..=6).contains(&p.heading_level) {
        Some(format!("Heading{}", p.heading_level))
    } else {
        p.list.map(|kind| match kind {
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
    if style.is_some() || jc.is_some() || !layout.is_empty() {
        out.push_str("<w:pPr>");
        if let Some(s) = style {
            out.push_str(&format!("<w:pStyle w:val=\"{s}\"/>"));
        }
        out.push_str(&layout);
        if let Some(j) = jc {
            out.push_str(&format!("<w:jc w:val=\"{j}\"/>"));
        }
        out.push_str("</w:pPr>");
    }
    for run in &p.runs {
        if run.text.is_empty() {
            continue;
        }
        out.push_str("<w:r>");
        out.push_str(&docx_run_properties(run));
        push_docx_text(out, &run.text);
        out.push_str("</w:r>");
    }
    out.push_str("</w:p>");
}

/// Relationship id offset of the first picture (`rId1` is the style sheet).
const DOCX_FIRST_IMAGE_REL: usize = 2;
const DOCX_MEDIA_DIR: &str = "word/media";

fn docx_content_types(media: &MediaParts) -> String {
    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
            "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>",
            "<Default Extension=\"xml\" ContentType=\"application/xml\"/>{}",
            "<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>",
            "<Override PartName=\"/word/styles.xml\" ContentType=\"{}\"/>",
            "<Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>",
            "</Types>"
        ),
        docx_content_type_defaults(media),
        DOCX_STYLES_CONTENT_TYPE
    )
}

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
    let styles = docx_styles_xml();

    let mut body = String::with_capacity(1024);
    body.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    body.push_str(
        "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"",
    );
    body.push_str(DOCX_PICTURE_NAMESPACES);
    body.push_str("><w:body>");
    let mut media = MediaParts::default();
    let mut image_rels = String::new();
    for block in &doc.blocks {
        match block {
            RichBlock::Paragraph(p) => push_docx_paragraph(&mut body, p),
            RichBlock::Image(img) => {
                let (number, path) = media.add(DOCX_MEDIA_DIR, img);
                let rel_id = format!("rId{}", number + DOCX_FIRST_IMAGE_REL - 1);
                let target = path.strip_prefix("word/").unwrap_or(&path);
                image_rels.push_str(&docx_image_relationship(&rel_id, target));
                body.push_str(&docx_image_paragraph(img, number, &rel_id));
            }
            RichBlock::Table(table) => {
                push_docx_table(&mut body, table);
                // A table cannot end the body or touch another table without a paragraph.
                body.push_str("<w:p/>");
            }
        }
    }
    body.push_str("<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/></w:sectPr>");
    body.push_str("</w:body></w:document>");

    let content_types = docx_content_types(&media);
    let doc_rels = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
            "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>",
            "{}</Relationships>"
        ),
        image_rels
    );

    for xml in [&body, &core_xml, &styles, &doc_rels] {
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
    entries.extend(media.into_entries());
    ZipArchiveCodec::pack_compressed(&entries)
}

// ---------------------------------------------------------------- ODT

/// Interned automatic-style keys, in first-use order.
struct StyleTable<K: PartialEq + Clone> {
    keys: Vec<K>,
}

impl<K: PartialEq + Clone> StyleTable<K> {
    fn new() -> Self {
        Self { keys: Vec::new() }
    }

    fn intern(&mut self, key: &K) -> usize {
        if let Some(pos) = self.keys.iter().position(|k| k == key) {
            return pos + 1;
        }
        self.keys.push(key.clone());
        self.keys.len()
    }
}

/// Automatic paragraph style identity.
#[derive(Clone, PartialEq, Default)]
struct OdtParaKey {
    align: Option<RichAlignment>,
    level: u8,
    list: Option<RichListKind>,
    left_twips: Option<u32>,
    first_line_twips: Option<i32>,
    line_percent: Option<u16>,
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
        list: if level > 0 { None } else { p.list },
        left_twips: p.left_indent_twips,
        first_line_twips: p.first_line_indent_twips,
        line_percent: p.line_spacing_percent,
    };
    let style_attr = if key == OdtParaKey::default() {
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
    for run in &p.runs {
        let key = odt_text_key(run);
        if odt_text_key_is_plain(&key) {
            push_odt_text(body, &run.text);
        } else {
            let t = text_styles.intern(&key);
            body.push_str(&format!("<text:span text:style-name=\"T{t}\">"));
            push_odt_text(body, &run.text);
            body.push_str("</text:span>");
        }
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
        list,
        left_twips,
        first_line_twips,
        line_percent,
    } = key;
    let list_attr = match list {
        Some(RichListKind::Ordered) => " style:list-style-name=\"L_Ordered\"",
        Some(RichListKind::Bullet) => " style:list-style-name=\"L_Bullet\"",
        None => "",
    };
    let mut styles = format!(
        "<style:style style:name=\"P{number}\" style:family=\"paragraph\"{list_attr}><style:paragraph-properties"
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
    } else if list.is_some() {
        styles.push_str(" fo:margin-left=\"0.5in\"");
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
    let mut media = MediaParts::default();
    let mut para_styles: StyleTable<OdtParaKey> = StyleTable::new();
    let mut text_styles: StyleTable<OdtTextKey> = StyleTable::new();
    let mut body = String::with_capacity(1024);
    for block in &doc.blocks {
        match block {
            RichBlock::Paragraph(p) => {
                push_odt_paragraph(&mut body, p, &mut para_styles, &mut text_styles);
            }
            RichBlock::Table(table) => push_odt_table(&mut body, table),
            RichBlock::Image(img) => {
                let (number, path) = media.add(ODT_MEDIA_DIR, img);
                body.push_str(&odt_image_paragraph(img, number, &path));
            }
        }
    }

    let mut styles = String::from("<office:automatic-styles>");
    for (idx, key) in para_styles.keys.iter().enumerate() {
        styles.push_str(&odt_para_style_xml(idx + 1, key));
    }
    for (idx, key) in text_styles.keys.iter().enumerate() {
        styles.push_str(&odt_text_style_xml(idx + 1, key));
    }
    styles.push_str("</office:automatic-styles>");
    let styles = format!("{}{styles}", odt_font_face_decls(&text_styles.keys));

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
    let manifest = OdfCodec::build_manifest_with_media(OdfCodec::MIMETYPE_ODT, &media.listing());
    let mut entries = vec![
        (
            "mimetype".to_string(),
            OdfCodec::MIMETYPE_ODT.as_bytes().to_vec(),
        ),
        ("META-INF/manifest.xml".to_string(), manifest.into_bytes()),
        ("content.xml".to_string(), content.into_bytes()),
    ];
    entries.extend(media.into_entries());
    ZipArchiveCodec::pack_compressed(&entries)
}

// ---------------------------------------------------------------- RTF

/// Formatting control words the RTF writer emits (each followed by one delimiter space).
const RTF_FORMAT_WORDS: [&str; 20] = [
    "pard",
    "ql",
    "qc",
    "qr",
    "qj",
    "b",
    "i",
    "ul",
    "ulnone",
    "strike",
    "fs",
    "li",
    "pnlvlblt",
    "pnlvlbody",
    "outlinelevel",
    "plain",
    "s",
    "f",
    "cf",
    "highlight",
];

/// Removes the formatting control words written by [`encode_rtf_rich`] (alignment, bold, italic,
/// underline, strike, size, indent, outline level) so the line decoder sees escaped text only.
/// Escapes (`\\`, `\{`, `\}`), `\uN?`, and `\line` are kept for the text unescaper.
pub(crate) fn strip_rtf_format_words(input: &str) -> String {
    let rest = input.strip_prefix(' ').unwrap_or(input);
    let bytes = rest.as_bytes();
    let mut out = String::with_capacity(rest.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            let ch = rest[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        let word_start = i + 1;
        let mut j = word_start;
        while j < bytes.len() && bytes[j].is_ascii_lowercase() {
            j += 1;
        }
        let word = &rest[word_start..j];
        if word.is_empty() {
            // Control symbol (`\\`, `\{`, `\}`): keep both bytes.
            let end = (i + 2).min(bytes.len());
            out.push_str(&rest[i..end]);
            i = end;
            continue;
        }
        if !RTF_FORMAT_WORDS.contains(&word) {
            out.push_str(&rest[i..j]);
            i = j;
            continue;
        }
        let mut k = j;
        if k < bytes.len() && bytes[k] == b'-' {
            k += 1;
        }
        while k < bytes.len() && bytes[k].is_ascii_digit() {
            k += 1;
        }
        if k < bytes.len() && bytes[k] == b' ' {
            k += 1;
        }
        i = k;
    }
    out
}

fn rtf_alignment_word(a: RichAlignment) -> &'static str {
    match a {
        RichAlignment::Left => "\\ql",
        RichAlignment::Center => "\\qc",
        RichAlignment::Right => "\\qr",
        RichAlignment::Justify => "\\qj",
    }
}

/// One RTF paragraph line (`\pard ... \par`).
fn rtf_paragraph_line(p: &RichParagraph, tables: &RtfRunTables) -> String {
    let mut line = String::from("\\pard");
    if let Some(a) = p.alignment {
        line.push_str(rtf_alignment_word(a));
    }
    if (1..=6).contains(&p.heading_level) {
        line.push_str(&format!(
            "\\outlinelevel{}\\fs{}",
            p.heading_level - 1,
            heading_half_points(p.heading_level)
        ));
    } else if let Some(list_kind) = p.list {
        line.push_str(match list_kind {
            RichListKind::Bullet => "\\pnlvlblt\\li720",
            RichListKind::Ordered => "\\pnlvlbody\\li720",
        });
    }
    line.push(' ');
    for run in &p.runs {
        push_rtf_run(&mut line, run, tables);
    }
    line.push_str("\\par");
    line
}

fn push_rtf_run(line: &mut String, run: &RichRun, tables: &RtfRunTables) {
    let toggles = [
        (run.bold, "\\b ", "\\b0 "),
        (run.italic, "\\i ", "\\i0 "),
        (run.underline, "\\ul ", "\\ulnone "),
        (run.strike, "\\strike ", "\\strike0 "),
    ];
    for (on, open, _) in toggles {
        if on {
            line.push_str(open);
        }
    }
    line.push_str(&tables.run_words(run));
    line.push_str(&RtfCodec::escape_rtf_text(&run.text));
    if has_extra_run_format(run) {
        line.push_str("\\plain ");
        return;
    }
    for (on, _, close) in toggles {
        if on {
            line.push_str(close);
        }
    }
}

pub(crate) fn encode_rtf_rich(doc: &RichDocument) -> String {
    let mut lines: Vec<String> = Vec::with_capacity(doc.blocks.len() + 4);
    lines.push("{\\rtf1\\ansi\\deff0".to_string());
    lines.push(format!(
        "{{\\info{{\\title {}}}}}",
        RtfCodec::escape_rtf_text(&doc.display_title())
    ));
    let tables = RtfRunTables::collect(doc.blocks.iter().flat_map(|b| match b {
        RichBlock::Paragraph(p) => p.runs.iter(),
        RichBlock::Table(_) | RichBlock::Image(_) => [].iter(),
    }));
    lines.extend(tables.header_lines());
    let mut table_idx = 0usize;
    for block in &doc.blocks {
        match block {
            RichBlock::Paragraph(p) => lines.push(rtf_paragraph_line(p, &tables)),
            RichBlock::Image(img) => {
                let stand_in = RichParagraph {
                    runs: vec![RichRun::plain(img.fallback_text())],
                    ..RichParagraph::default()
                };
                lines.push(rtf_paragraph_line(&stand_in, &tables));
            }
            RichBlock::Table(table) => {
                lines.extend(rtf_table_lines(table, table_idx));
                table_idx += 1;
            }
        }
    }
    lines.push("}".to_string());
    lines.join("\n")
}

// ---------------------------------------------------------------- Markdown / TXT / PDF

fn escape_markdown_inline(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for ch in text.chars() {
        if matches!(
            ch,
            '\\' | '`' | '*' | '_' | '~' | '[' | ']' | '<' | '>' | '|' | '#' | '!'
        ) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
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
                    parts.push(CommonMarkCodec::render(&[MarkdownBlock::Table(
                        table.rows.clone(),
                    )]));
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
    entries.extend(
        paginate_pages(vec![page])
            .into_iter()
            .map(PdfPageEntry::Text),
    );
}

/// Text pages with every embeddable picture on a page of its own, in document order; pictures
/// the PDF writer cannot embed are printed as alternative text.
fn pdf_entries(doc: &RichDocument) -> Vec<PdfPageEntry> {
    let mut entries = Vec::new();
    let mut lines = Vec::new();
    let mut stamp = doc.official_agency.clone();
    for block in &doc.blocks {
        let embedded = match block {
            RichBlock::Image(img) => pdf_image_object(img).map(|object| (img, object)),
            _ => None,
        };
        let Some((img, image)) = embedded else {
            push_plain_lines(block, " | ", &mut lines);
            continue;
        };
        flush_pdf_text(&mut entries, &mut lines, &mut stamp);
        let (cx, cy) = img.display_emu();
        let (width_pt, height_pt) = fit_within(
            emu_to_points(cx),
            emu_to_points(cy),
            PDF_IMAGE_MAX_WIDTH_PT,
            PDF_IMAGE_MAX_HEIGHT_PT,
        );
        entries.push(PdfPageEntry::Image(PdfImagePage {
            page_number: 0,
            image,
            width_pt,
            height_pt,
        }));
    }
    flush_pdf_text(&mut entries, &mut lines, &mut stamp);
    for (idx, entry) in entries.iter_mut().enumerate() {
        match entry {
            PdfPageEntry::Text(page) => page.page_number = idx + 1,
            PdfPageEntry::Image(page) => page.page_number = idx + 1,
        }
    }
    entries
}

pub(crate) fn serialize_pdf_rich(doc: &RichDocument) -> Vec<u8> {
    if doc.blocks.iter().any(|b| matches!(b, RichBlock::Image(_))) {
        let entries = pdf_entries(doc);
        if entries.iter().any(|e| matches!(e, PdfPageEntry::Image(_))) {
            return PdfSerializer::serialize_entries(&doc.display_title(), None, &entries);
        }
    }
    PdfSerializer::serialize(&PdfDocumentModel {
        title: doc.display_title(),
        pages: vec![PdfPageModel {
            page_number: 1,
            rotation_degrees: 0,
            body_text: render_plain_text(doc, " | ", false),
            approval_stamp: doc.official_agency.clone(),
        }],
        watermark_text: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtf_format_word_stripping_keeps_escapes_and_unicode() {
        assert_eq!(
            strip_rtf_format_words("\\qc\\outlinelevel0\\fs32 \\b A\\\\b\\b0 \\u-21504?\\line x"),
            "A\\\\b\\u-21504?\\line x"
        );
        assert_eq!(strip_rtf_format_words(" plain"), "plain");
    }

    #[test]
    fn markdown_escapes_block_starters_and_inline_markers() {
        assert_eq!(escape_markdown_line_start("1. 항목"), "1\\. 항목");
        assert_eq!(escape_markdown_line_start("- 항목"), "\\- 항목");
        assert_eq!(escape_markdown_inline("*a*#"), "\\*a\\*\\#");
    }
}
