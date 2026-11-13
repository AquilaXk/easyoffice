//! Header and footer of every page ([`RichPageFrame`]) in the three document formats: the DOCX
//! `w:hdr`/`w:ftr` parts a section points to, the ODT master page's `style:header`/`style:footer`, and
//! the HWPX `hp:header`/`hp:footer` controls of the first paragraph. The readers live next to each
//! format's body reader; this module holds what the writers and the readers share.

use crate::rich_document::{RichField, RichPageMargins, RichParagraph};
use crate::rich_run_writers::docx_run_properties;

pub(crate) const DOCX_HEADER_PART: &str = "word/header1.xml";
pub(crate) const DOCX_FOOTER_PART: &str = "word/footer1.xml";
pub(crate) const DOCX_HEADER_REL_ID: &str = "rIdHeader1";
pub(crate) const DOCX_FOOTER_REL_ID: &str = "rIdFooter1";
const DOCX_HEADER_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml";
const DOCX_FOOTER_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml";
const DOCX_NAMESPACES: &str = concat!(
    "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" ",
    "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\""
);

/// The field a DOCX instruction (`w:fldSimple/@w:instr`, `w:instrText`) names, ignoring its switches:
/// `PAGE`, `NUMPAGES` or `FILENAME`.
pub(crate) fn field_of_instruction(instruction: &str) -> Option<RichField> {
    let word = instruction.split_whitespace().next()?;
    [
        ("PAGE", RichField::PageNumber),
        ("NUMPAGES", RichField::TotalPages),
        ("FILENAME", RichField::FileName),
    ]
    .into_iter()
    .find(|(name, _)| word.eq_ignore_ascii_case(name))
    .map(|(_, field)| field)
}

/// A field as a `w:fldSimple` around one run (the placeholder text is what a viewer shows before it
/// updates the field).
pub(crate) fn docx_field_run(field: RichField, run_properties: &str) -> String {
    let (instruction, placeholder) = match field {
        RichField::PageNumber => ("PAGE", "1"),
        RichField::TotalPages => ("NUMPAGES", "1"),
        RichField::FileName => ("FILENAME", "document"),
    };
    format!(
        "<w:fldSimple w:instr=\" {instruction} \"><w:r>{run_properties}<w:t>{placeholder}</w:t></w:r></w:fldSimple>"
    )
}

/// `w:pgMar` for the section properties (side margins 1 inch, as the page size has no side margin of its own).
pub(crate) fn docx_page_margins(margins: &RichPageMargins) -> String {
    format!(
        "<w:pgMar w:top=\"{}\" w:right=\"1440\" w:bottom=\"{}\" w:left=\"1440\" w:header=\"{}\" w:footer=\"{}\" w:gutter=\"0\"/>",
        margins.top, margins.bottom, margins.header, margins.footer
    )
}

fn docx_frame_paragraph(p: &RichParagraph) -> String {
    let mut xml = String::from("<w:p>");
    let jc = p.alignment.map(|a| match a {
        crate::RichAlignment::Left => "left",
        crate::RichAlignment::Center => "center",
        crate::RichAlignment::Right => "right",
        crate::RichAlignment::Justify => "both",
    });
    let layout = crate::rich_writers::docx_paragraph_layout(p);
    if jc.is_some() || !layout.is_empty() {
        xml.push_str("<w:pPr>");
        xml.push_str(&layout);
        if let Some(j) = jc {
            xml.push_str(&format!("<w:jc w:val=\"{j}\"/>"));
        }
        xml.push_str("</w:pPr>");
    }
    for run in &p.runs {
        let properties = docx_run_properties(run);
        match run.field {
            Some(field) => xml.push_str(&docx_field_run(field, &properties)),
            None => {
                xml.push_str("<w:r>");
                xml.push_str(&properties);
                crate::rich_writers::push_docx_text(&mut xml, &run.text);
                xml.push_str("</w:r>");
            }
        }
    }
    xml.push_str("</w:p>");
    xml
}

/// `word/header1.xml` or `word/footer1.xml` for the paragraphs, or `None` when there are none.
pub(crate) fn docx_frame_part(paragraphs: &[RichParagraph], header: bool) -> Option<String> {
    if paragraphs.is_empty() {
        return None;
    }
    let root = if header { "w:hdr" } else { "w:ftr" };
    let mut xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><{root} {DOCX_NAMESPACES}>");
    for p in paragraphs {
        xml.push_str(&docx_frame_paragraph(p));
    }
    xml.push_str(&format!("</{root}>"));
    Some(xml)
}

/// What the DOCX package gains for a header and a footer part: `(header reference, footer reference)` for the
/// section properties, the relationships, the content-type overrides.
pub(crate) struct DocxFrameRefs {
    pub section: String,
    pub relationships: String,
    pub content_types: String,
}

pub(crate) fn docx_frame_refs(header: bool, footer: bool) -> DocxFrameRefs {
    let mut refs = DocxFrameRefs {
        section: String::new(),
        relationships: String::new(),
        content_types: String::new(),
    };
    for (present, tag, id, part, kind, content_type) in [
        (
            header,
            "w:headerReference",
            DOCX_HEADER_REL_ID,
            DOCX_HEADER_PART,
            "header",
            DOCX_HEADER_CONTENT_TYPE,
        ),
        (
            footer,
            "w:footerReference",
            DOCX_FOOTER_REL_ID,
            DOCX_FOOTER_PART,
            "footer",
            DOCX_FOOTER_CONTENT_TYPE,
        ),
    ] {
        if !present {
            continue;
        }
        refs.section
            .push_str(&format!("<{tag} w:type=\"default\" r:id=\"{id}\"/>"));
        refs.relationships.push_str(&format!(
            "<Relationship Id=\"{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}\" Target=\"{}\"/>",
            part.trim_start_matches("word/")
        ));
        refs.content_types.push_str(&format!(
            "<Override PartName=\"/{part}\" ContentType=\"{content_type}\"/>"
        ));
    }
    refs
}

/// An ODF field element for a run (a `text:span` around it when the run has a text style).
pub(crate) fn odt_field(field: RichField, run_text_style: Option<usize>) -> String {
    let element = match field {
        RichField::PageNumber => {
            "<text:page-number text:select-page=\"current\">1</text:page-number>"
        }
        RichField::TotalPages => "<text:page-count>1</text:page-count>",
        RichField::FileName => {
            "<text:file-name text:display=\"name-and-extension\">document</text:file-name>"
        }
    };
    match run_text_style {
        Some(t) => format!("<text:span text:style-name=\"T{t}\">{element}</text:span>"),
        None => element.to_string(),
    }
}

/// Twips as an ODF point length (`36pt`, `7.5pt`): exact to a twip.
fn odt_pt(twips: u32) -> String {
    let (whole, hundredths) = (twips / 20, twips % 20 * 5);
    if hundredths == 0 {
        format!("{whole}pt")
    } else {
        format!("{whole}.{hundredths:02}pt")
    }
}

/// The page layout and master page that give an ODT its header and footer: `(page layout, master styles)`.
/// The layout's top margin is the distance to the header, the header's minimum height reaches the body (and
/// the same for the footer); a part that is absent leaves its margin as the distance to the body.
pub(crate) fn odt_master_page(
    header: Option<&str>,
    footer: Option<&str>,
    margins: Option<&RichPageMargins>,
) -> (String, String) {
    let m = margins.copied().unwrap_or(RichPageMargins {
        top: 1191,
        bottom: 1191,
        header: 709,
        footer: 709,
    });
    let (margin_top, header_height) = match header {
        Some(_) => (m.header, m.top.saturating_sub(m.header)),
        None => (m.top, 0),
    };
    let (margin_bottom, footer_height) = match footer {
        Some(_) => (m.footer, m.bottom.saturating_sub(m.footer)),
        None => (m.bottom, 0),
    };
    let layout = format!(
        concat!(
            "<style:page-layout style:name=\"pm1\"><style:page-layout-properties fo:page-width=\"21cm\" ",
            "fo:page-height=\"29.7cm\" style:print-orientation=\"portrait\" fo:margin-top=\"{}\" ",
            "fo:margin-bottom=\"{}\" fo:margin-left=\"2cm\" fo:margin-right=\"2cm\"/>",
            "<style:header-style><style:header-footer-properties fo:min-height=\"{}\" fo:margin-bottom=\"0pt\"/></style:header-style>",
            "<style:footer-style><style:header-footer-properties fo:min-height=\"{}\" fo:margin-top=\"0pt\"/></style:footer-style>",
            "</style:page-layout>"
        ),
        odt_pt(margin_top),
        odt_pt(margin_bottom),
        odt_pt(header_height),
        odt_pt(footer_height)
    );
    let mut master =
        String::from("<office:master-styles><style:master-page style:name=\"Standard\" style:page-layout-name=\"pm1\">");
    if let Some(xml) = header {
        master.push_str(&format!("<style:header>{xml}</style:header>"));
    }
    if let Some(xml) = footer {
        master.push_str(&format!("<style:footer>{xml}</style:footer>"));
    }
    master.push_str("</style:master-page></office:master-styles>");
    (layout, master)
}

/// `styles.xml` of an ODT with a header or footer.
pub(crate) fn odt_styles_xml(
    font_decls: &str,
    automatic_styles: &str,
    page_layout: &str,
    master_styles: &str,
) -> String {
    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-styles ",
            "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
            "xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" ",
            "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" ",
            "xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" ",
            "xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" ",
            "office:version=\"1.2\">{}<office:automatic-styles>{}{}</office:automatic-styles>{}</office:document-styles>"
        ),
        font_decls, automatic_styles, page_layout, master_styles
    )
}

/// The `hp:ctrl` of a header or footer for the first paragraph of a HWPX section; `paragraphs` is the
/// finished `hp:p` XML of the sub-list.
pub(crate) fn hwpx_frame_control(header: bool, id: u32, paragraphs: &str) -> String {
    let tag = if header { "hp:header" } else { "hp:footer" };
    format!(
        concat!(
            "<hp:ctrl><{tag} id=\"{id}\" applyPageType=\"BOTH\">",
            "<hp:subList id=\"\" textDirection=\"HORIZONTAL\" lineWrap=\"BREAK\" vertAlign=\"TOP\" ",
            "linkListIDRef=\"0\" linkListNextIDRef=\"0\" textWidth=\"0\" textHeight=\"0\" ",
            "hasTextRef=\"0\" hasNumRef=\"0\">{paragraphs}</hp:subList></{tag}></hp:ctrl>"
        ),
        tag = tag,
        id = id,
        paragraphs = paragraphs
    )
}

/// The control of a page-number or page-count field inside a header or footer paragraph (`numType` is `PAGE`
/// or `TOTAL_PAGE`).
pub(crate) fn hwpx_number_control(num_type: &str) -> String {
    format!(
        concat!(
            "<hp:ctrl><hp:autoNum num=\"1\" numType=\"{}\">",
            "<hp:autoNumFormat type=\"DIGIT\" userChar=\"\" prefixChar=\"\" suffixChar=\"\" supscript=\"0\"/>",
            "</hp:autoNum></hp:ctrl>"
        ),
        num_type
    )
}

/// HWPUNIT per twip (1/7200 inch against 1/1440 inch).
const HWPUNIT_PER_TWIP: u32 = 5;

/// The page margins as `hp:margin` values `(header, footer, top, bottom)` in HWPUNIT: the reference
/// editor's top margin is the distance to the header and its header margin the header area's height, its
/// footer margin the distance to the footer and its bottom margin the footer area's height
/// (`rhwp` `src/model/page.rs:238-269`).
pub(crate) fn hwpx_margin_values(m: &RichPageMargins) -> (u32, u32, u32, u32) {
    (
        m.top.saturating_sub(m.header) * HWPUNIT_PER_TWIP,
        m.footer * HWPUNIT_PER_TWIP,
        m.header * HWPUNIT_PER_TWIP,
        m.bottom.saturating_sub(m.footer) * HWPUNIT_PER_TWIP,
    )
}

/// The inverse of [`hwpx_margin_values`] for the `(header, footer, top, bottom)` of a `hp:margin`.
pub(crate) fn margins_from_hwpx(
    header: u32,
    footer: u32,
    top: u32,
    bottom: u32,
) -> RichPageMargins {
    RichPageMargins {
        top: top.saturating_add(header) / HWPUNIT_PER_TWIP,
        bottom: bottom.saturating_add(footer) / HWPUNIT_PER_TWIP,
        header: top / HWPUNIT_PER_TWIP,
        footer: footer / HWPUNIT_PER_TWIP,
    }
}
