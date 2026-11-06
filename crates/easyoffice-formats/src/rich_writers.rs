//! Writers that keep [`RichDocument`] block order and run/paragraph formatting for DOCX
//! (`w:rPr`/`w:pPr`/`w:pStyle` plus `word/styles.xml`), ODT (automatic styles), RTF (control
//! words), Markdown (`**`/`*`/`~~`/`#`), TXT, and PDF (ordered text). User text is always escaped
//! for the target syntax and never parsed.

use crate::rich_document::{RichAlignment, RichBlock, RichDocument, RichListKind, RichParagraph};
use crate::{
    CommonMarkCodec, EasyOfficeError, MarkdownBlock, OdfCodec, PdfDocumentModel, PdfPageModel,
    PdfSerializer, RtfCodec, XmlSanitizer, ZipArchiveCodec,
};

/// Heading font sizes in half-points for levels 1–6.
const HEADING_HALF_POINTS: [u32; 6] = [32, 28, 26, 24, 22, 22];

fn heading_half_points(level: u8) -> u32 {
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

fn push_docx_text(out: &mut String, text: &str) {
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
    if style.is_some() || jc.is_some() {
        out.push_str("<w:pPr>");
        if let Some(s) = style {
            out.push_str(&format!("<w:pStyle w:val=\"{s}\"/>"));
        }
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
        if run.bold || run.italic || run.strike || run.underline {
            out.push_str("<w:rPr>");
            if run.bold {
                out.push_str("<w:b/>");
            }
            if run.italic {
                out.push_str("<w:i/>");
            }
            if run.strike {
                out.push_str("<w:strike/>");
            }
            if run.underline {
                out.push_str("<w:u w:val=\"single\"/>");
            }
            out.push_str("</w:rPr>");
        }
        push_docx_text(out, &run.text);
        out.push_str("</w:r>");
    }
    out.push_str("</w:p>");
}

fn push_docx_table(out: &mut String, rows: &[Vec<String>]) {
    out.push_str("<w:tbl>");
    crate::ooxml_shared_parts::push_docx_table_properties(out, rows);
    for row in rows {
        out.push_str("<w:tr>");
        for cell in row {
            out.push_str("<w:tc><w:p><w:r>");
            push_docx_text(out, cell);
            out.push_str("</w:r></w:p></w:tc>");
        }
        out.push_str("</w:tr>");
    }
    out.push_str("</w:tbl>");
}

pub(crate) fn pack_docx_rich(doc: &RichDocument) -> Result<Vec<u8>, EasyOfficeError> {
    let content_types = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
            "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>",
            "<Default Extension=\"xml\" ContentType=\"application/xml\"/>",
            "<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>",
            "<Override PartName=\"/word/styles.xml\" ContentType=\"{}\"/>",
            "<Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>",
            "</Types>"
        ),
        DOCX_STYLES_CONTENT_TYPE
    );
    let rels = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>",
        "<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/>",
        "</Relationships>"
    );
    let doc_rels = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>",
        "</Relationships>"
    );
    let core_xml = crate::ooxml_shared_parts::core_properties_xml(&doc.display_title());
    let styles = docx_styles_xml();

    let mut body = String::with_capacity(1024);
    body.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    body.push_str("<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>");
    for block in &doc.blocks {
        match block {
            RichBlock::Paragraph(p) => push_docx_paragraph(&mut body, p),
            RichBlock::Table(rows) => {
                push_docx_table(&mut body, rows);
                // A table cannot end the body or touch another table without a paragraph.
                body.push_str("<w:p/>");
            }
        }
    }
    body.push_str("<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/></w:sectPr>");
    body.push_str("</w:body></w:document>");

    for xml in [&body, &core_xml, &styles] {
        XmlSanitizer::validate_safe_xml(xml)?;
    }

    ZipArchiveCodec::pack(&[
        (
            "[Content_Types].xml".to_string(),
            content_types.into_bytes(),
        ),
        ("_rels/.rels".to_string(), rels.as_bytes().to_vec()),
        ("word/document.xml".to_string(), body.into_bytes()),
        (
            "word/_rels/document.xml.rels".to_string(),
            doc_rels.as_bytes().to_vec(),
        ),
        ("word/styles.xml".to_string(), styles.into_bytes()),
        ("docProps/core.xml".to_string(), core_xml.into_bytes()),
    ])
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

type OdtTextKey = (bool, bool, bool, bool);
type OdtParaKey = (Option<RichAlignment>, u8, bool);

fn push_odt_text(out: &mut String, text: &str) {
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

pub(crate) fn pack_odt_rich(doc: &RichDocument) -> Result<Vec<u8>, EasyOfficeError> {
    let mut para_styles: StyleTable<OdtParaKey> = StyleTable::new();
    let mut text_styles: StyleTable<OdtTextKey> = StyleTable::new();
    let mut body = String::with_capacity(1024);
    for block in &doc.blocks {
        match block {
            RichBlock::Paragraph(p) => {
                let level = if (1..=6).contains(&p.heading_level) {
                    p.heading_level
                } else {
                    0
                };
                let key = (p.alignment, level, p.list.is_some());
                let style = if key == (None, 0, false) {
                    None
                } else {
                    Some(para_styles.intern(&key))
                };
                let style_attr = style
                    .map(|s| format!(" text:style-name=\"P{s}\""))
                    .unwrap_or_default();
                if level > 0 {
                    body.push_str(&format!(
                        "<text:h text:outline-level=\"{level}\"{style_attr}>"
                    ));
                } else {
                    body.push_str(&format!("<text:p{style_attr}>"));
                }
                for run in &p.runs {
                    let key = (run.bold, run.italic, run.underline, run.strike);
                    if key == (false, false, false, false) {
                        push_odt_text(&mut body, &run.text);
                    } else {
                        let t = text_styles.intern(&key);
                        body.push_str(&format!("<text:span text:style-name=\"T{t}\">"));
                        push_odt_text(&mut body, &run.text);
                        body.push_str("</text:span>");
                    }
                }
                body.push_str(if level > 0 { "</text:h>" } else { "</text:p>" });
            }
            RichBlock::Table(rows) => {
                let cols = rows.iter().map(Vec::len).max().unwrap_or(0).max(1);
                body.push_str(&format!(
                    "<table:table><table:table-column table:number-columns-repeated=\"{cols}\"/>"
                ));
                for row in rows {
                    body.push_str("<table:table-row>");
                    for cell in row {
                        body.push_str("<table:table-cell office:value-type=\"string\"><text:p>");
                        push_odt_text(&mut body, cell);
                        body.push_str("</text:p></table:table-cell>");
                    }
                    body.push_str("</table:table-row>");
                }
                body.push_str("</table:table>");
            }
        }
    }

    let mut styles = String::from("<office:automatic-styles>");
    for (idx, (align, level, is_list)) in para_styles.keys.iter().enumerate() {
        styles.push_str(&format!(
            "<style:style style:name=\"P{}\" style:family=\"paragraph\"><style:paragraph-properties",
            idx + 1
        ));
        if let Some(a) = align {
            let v = match a {
                RichAlignment::Left => "start",
                RichAlignment::Center => "center",
                RichAlignment::Right => "end",
                RichAlignment::Justify => "justify",
            };
            styles.push_str(&format!(" fo:text-align=\"{v}\""));
        }
        if *is_list {
            styles.push_str(" fo:margin-left=\"0.5in\"");
        }
        styles.push_str("/>");
        if *level > 0 {
            styles.push_str(&format!(
                "<style:text-properties fo:font-size=\"{}pt\" fo:font-weight=\"bold\"/>",
                heading_half_points(*level) / 2
            ));
        }
        styles.push_str("</style:style>");
    }
    for (idx, (bold, italic, underline, strike)) in text_styles.keys.iter().enumerate() {
        styles.push_str(&format!(
            "<style:style style:name=\"T{}\" style:family=\"text\"><style:text-properties",
            idx + 1
        ));
        if *bold {
            styles.push_str(" fo:font-weight=\"bold\"");
        }
        if *italic {
            styles.push_str(" fo:font-style=\"italic\"");
        }
        if *underline {
            styles.push_str(" style:text-underline-style=\"solid\" style:text-underline-width=\"auto\" style:text-underline-color=\"font-color\"");
        }
        if *strike {
            styles.push_str(" style:text-line-through-style=\"solid\"");
        }
        styles.push_str("/></style:style>");
    }
    styles.push_str("</office:automatic-styles>");

    let content = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-content ",
            "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
            "xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" ",
            "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" ",
            "xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" ",
            "xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" ",
            "office:version=\"1.2\" title=\"{}\">{}<office:body><office:text>{}</office:text></office:body></office:document-content>"
        ),
        XmlSanitizer::escape(&doc.display_title()),
        styles,
        body
    );
    XmlSanitizer::validate_safe_xml(&content)?;
    ZipArchiveCodec::pack(&[
        (
            "mimetype".to_string(),
            OdfCodec::MIMETYPE_ODT.as_bytes().to_vec(),
        ),
        (
            "META-INF/manifest.xml".to_string(),
            OdfCodec::build_manifest(OdfCodec::MIMETYPE_ODT).into_bytes(),
        ),
        ("content.xml".to_string(), content.into_bytes()),
    ])
}

// ---------------------------------------------------------------- RTF

/// Formatting control words the RTF writer emits (each followed by one delimiter space).
const RTF_FORMAT_WORDS: [&str; 15] = [
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
    "outlinelevel",
    "plain",
    "s",
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

pub(crate) fn encode_rtf_rich(doc: &RichDocument) -> String {
    let mut lines: Vec<String> = Vec::with_capacity(doc.blocks.len() + 4);
    lines.push("{\\rtf1\\ansi\\deff0".to_string());
    lines.push(format!(
        "{{\\info{{\\title {}}}}}",
        RtfCodec::escape_rtf_text(&doc.display_title())
    ));
    let mut table_idx = 0usize;
    for block in &doc.blocks {
        match block {
            RichBlock::Paragraph(p) => {
                let mut line = String::from("\\pard");
                if let Some(a) = p.alignment {
                    line.push_str(match a {
                        RichAlignment::Left => "\\ql",
                        RichAlignment::Center => "\\qc",
                        RichAlignment::Right => "\\qr",
                        RichAlignment::Justify => "\\qj",
                    });
                }
                if (1..=6).contains(&p.heading_level) {
                    line.push_str(&format!(
                        "\\outlinelevel{}\\fs{}",
                        p.heading_level - 1,
                        heading_half_points(p.heading_level)
                    ));
                } else if p.list.is_some() {
                    line.push_str("\\li720");
                }
                line.push(' ');
                for run in &p.runs {
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
                    line.push_str(&RtfCodec::escape_rtf_text(&run.text));
                    for (on, _, close) in toggles {
                        if on {
                            line.push_str(close);
                        }
                    }
                }
                line.push_str("\\par");
                lines.push(line);
            }
            RichBlock::Table(rows) => {
                for row in rows {
                    let cells = row
                        .iter()
                        .map(|c| format!("\\cellx {}\\cell", RtfCodec::escape_rtf_text(c)))
                        .collect::<Vec<_>>()
                        .join(" ");
                    lines.push(format!(
                        "{{\\trowd{{\\*\\tblidx {table_idx}}} {cells}\\row}}"
                    ));
                }
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
    let mut parts: Vec<String> = vec![format!(
        "# {}",
        escape_markdown_inline(&doc.display_title())
    )];
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
            RichBlock::Table(rows) => {
                flush(&mut parts, &mut list_buf);
                ordinal = 0;
                if rows.iter().any(|r| !r.is_empty()) {
                    parts.push(CommonMarkCodec::render(&[MarkdownBlock::Table(
                        rows.clone(),
                    )]));
                }
            }
        }
    }
    flush(&mut parts, &mut list_buf);
    parts.join("\n\n")
}

/// Ordered plain text: optional title line, paragraph text, table rows joined by `cell_sep`.
pub(crate) fn render_plain_text(doc: &RichDocument, cell_sep: &str, with_title: bool) -> String {
    let mut lines = Vec::with_capacity(doc.blocks.len() + 1);
    if with_title {
        lines.push(doc.display_title());
    }
    for block in &doc.blocks {
        match block {
            RichBlock::Paragraph(p) => lines.push(p.plain_text()),
            RichBlock::Table(rows) => {
                for row in rows.iter().filter(|r| !r.is_empty()) {
                    lines.push(row.join(cell_sep));
                }
            }
        }
    }
    lines.join("\n")
}

pub(crate) fn serialize_pdf_rich(doc: &RichDocument) -> Vec<u8> {
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
