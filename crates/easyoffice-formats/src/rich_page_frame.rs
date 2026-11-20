//! Header and footer of every page ([`RichPageFrame`]) in the three document formats: the DOCX
//! `w:hdr`/`w:ftr` parts a section points to, the ODT master page's `style:header`/`style:footer`, and
//! the HWPX `hp:header`/`hp:footer` controls of the first paragraph. The readers live next to each
//! format's body reader; this module holds what the writers and the readers share.

use crate::rich_document::{
    RichBinding, RichBindingKind, RichColumns, RichField, RichPageMargins, RichPaper, RichParagraph,
};
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

/// `w:pgSz` for the section properties: A4 when the document has no paper of its own, `w:orient` when the page
/// is wider than high.
pub(crate) fn docx_paper(paper: Option<&RichPaper>) -> String {
    let paper = paper.copied().unwrap_or(RichPaper::A4);
    let orient = if paper.width > paper.height {
        " w:orient=\"landscape\""
    } else {
        ""
    };
    format!(
        "<w:pgSz w:w=\"{}\" w:h=\"{}\"{orient}/>",
        paper.width, paper.height
    )
}

/// `w:pgMar` for the section properties; empty without margins (the reader's own default applies).
pub(crate) fn docx_page_margins(margins: Option<&RichPageMargins>, binding: RichBinding) -> String {
    margins.map_or_else(String::new, |m| {
        format!(
            "<w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"{}\" w:footer=\"{}\" w:gutter=\"{}\"/>",
            m.top, m.right, m.bottom, m.left, m.header, m.footer, binding.gutter
        )
    })
}

/// `w:cols` for the section properties when `columns` has more than one column; empty otherwise.
pub(crate) fn docx_cols_xml(
    columns: Option<RichColumns>,
    paper: Option<&RichPaper>,
    margins: Option<&RichPageMargins>,
) -> String {
    let Some(cols) = columns.filter(|c| c.count > 1) else {
        return String::new();
    };
    let count = cols.count.min(16);
    let gap = cols.gap_twips;
    if cols.equal {
        return format!("<w:cols w:num=\"{count}\" w:space=\"{gap}\"/>");
    }
    let page_w = paper.copied().unwrap_or(RichPaper::A4).width;
    let side = margins.map_or(2880, |m| m.left.saturating_add(m.right));
    let usable = page_w
        .saturating_sub(side)
        .saturating_sub(u32::from(gap) * u32::from(count - 1))
        .max(u32::from(count) * 144);
    let first_w = (usable / 3).max(144);
    let rest_w = usable.saturating_sub(first_w) / u32::from(count - 1);
    let mut xml = format!("<w:cols w:num=\"{count}\" w:space=\"{gap}\" w:equalWidth=\"0\">");
    for i in 0..count {
        let w = if i == 0 { first_w } else { rest_w.max(144) };
        if i + 1 < count {
            xml.push_str(&format!("<w:col w:w=\"{w}\" w:space=\"{gap}\"/>"));
        } else {
            xml.push_str(&format!("<w:col w:w=\"{w}\"/>"));
        }
    }
    xml.push_str("</w:cols>");
    xml
}

pub(crate) const DOCX_SETTINGS_PART: &str = "word/settings.xml";
pub(crate) const DOCX_SETTINGS_REL_ID: &str = "rIdSettings1";
const DOCX_SETTINGS_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml";

/// `word/settings.xml` for a binding, even/odd header, or tracked-changes setting
/// (`w:evenAndOddHeaders`, `w:mirrorMargins`, `w:gutterAtTop`, `w:trackRevisions`), with the
/// relationship and content-type override; `None` when none apply.
pub(crate) fn docx_settings(
    binding: RichBinding,
    even_odd_different: bool,
    track_changes: bool,
) -> Option<(String, String, String)> {
    let mut flags = String::new();
    if even_odd_different {
        flags.push_str("<w:evenAndOddHeaders/>");
    }
    if track_changes {
        flags.push_str("<w:trackRevisions/>");
    }
    match binding.kind {
        RichBindingKind::Single => {}
        RichBindingKind::Duplex => flags.push_str("<w:mirrorMargins/>"),
        RichBindingKind::Top => flags.push_str("<w:gutterAtTop/>"),
    }
    if flags.is_empty() {
        return None;
    }
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:settings xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">{flags}</w:settings>"
    );
    let rel = format!(
        "<Relationship Id=\"{DOCX_SETTINGS_REL_ID}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings\" Target=\"settings.xml\"/>"
    );
    let content_type = format!(
        "<Override PartName=\"/{DOCX_SETTINGS_PART}\" ContentType=\"{DOCX_SETTINGS_CONTENT_TYPE}\"/>"
    );
    Some((xml, rel, content_type))
}

/// Splits `2..=3` strictly left-to-right ordered paragraphs (`Left`/`None` < `Center` < `Right`)
/// without block-only layout properties into horizontal `[Left, Center, Right]` slots (`PD-249`).
pub(crate) fn split_horizontal_zones(
    paragraphs: &[RichParagraph],
) -> Option<[Option<&RichParagraph>; 3]> {
    if !(2..=3).contains(&paragraphs.len()) {
        return None;
    }
    let mut zones: [Option<&RichParagraph>; 3] = [None, None, None];
    let mut prev_slot: Option<usize> = None;
    for p in paragraphs {
        if p.heading_level > 0
            || p.list.is_some()
            || p.left_indent_twips.is_some()
            || p.first_line_indent_twips.is_some()
            || p.line_spacing_percent.is_some()
            || p.toc.is_some()
            || p.page_break_before
        {
            return None;
        }
        let slot = match p.alignment {
            None | Some(crate::RichAlignment::Left) => 0,
            Some(crate::RichAlignment::Center) => 1,
            Some(crate::RichAlignment::Right) => 2,
            Some(crate::RichAlignment::Justify) => return None,
        };
        if prev_slot.is_some_and(|prev| slot <= prev) {
            return None;
        }
        zones[slot] = Some(p);
        prev_slot = Some(slot);
    }
    Some(zones)
}

fn push_docx_frame_runs(xml: &mut String, p: &RichParagraph) {
    for run in &p.runs {
        let properties = docx_run_properties(run);
        match run.field {
            Some(field) => xml.push_str(&docx_field_run(field, &properties)),
            None => {
                xml.push_str("<w:r>");
                xml.push_str(&properties);
                crate::rich_writers::push_docx_text(xml, &run.text);
                xml.push_str("</w:r>");
            }
        }
    }
}

fn docx_frame_zones_paragraph(zones: [Option<&RichParagraph>; 3]) -> String {
    let mut xml = String::from("<w:p>");
    if let Some(left) = zones[0] {
        push_docx_frame_runs(&mut xml, left);
    }
    if let Some(center) = zones[1] {
        xml.push_str(
            "<w:r><w:ptab w:alignment=\"center\" w:relativeTo=\"margin\" w:leader=\"none\"/></w:r>",
        );
        push_docx_frame_runs(&mut xml, center);
    }
    if let Some(right) = zones[2] {
        xml.push_str(
            "<w:r><w:ptab w:alignment=\"right\" w:relativeTo=\"margin\" w:leader=\"none\"/></w:r>",
        );
        push_docx_frame_runs(&mut xml, right);
    }
    xml.push_str("</w:p>");
    xml
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
    push_docx_frame_runs(&mut xml, p);
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
    if let Some(zones) = split_horizontal_zones(paragraphs) {
        xml.push_str(&docx_frame_zones_paragraph(zones));
    } else {
        for p in paragraphs {
            xml.push_str(&docx_frame_paragraph(p));
        }
    }
    xml.push_str(&format!("</{root}>"));
    Some(xml)
}

/// What the DOCX package gains for a header and a footer part: `(header reference, footer reference)` for the
/// section properties, the relationships, the content-type overrides.
pub(crate) struct DocxFrameRefs {
    /// The header and footer references of the section the frame belongs to (the first one; later sections inherit them).
    pub section: String,
    /// `<w:titlePg/>` when the first page differs, else empty; it closes the `sectPr` (after `w:cols`).
    pub title_pg: &'static str,
    pub relationships: String,
    pub content_types: String,
}

pub(crate) fn docx_frame_parts_and_refs(
    frame: &crate::RichPageFrame,
) -> (Vec<(String, Vec<u8>)>, DocxFrameRefs) {
    let mut entries = Vec::new();
    let mut refs = DocxFrameRefs {
        section: String::new(),
        title_pg: "",
        relationships: String::new(),
        content_types: String::new(),
    };
    let slots = [
        (
            &frame.header[..],
            true,
            true,
            "default",
            DOCX_HEADER_REL_ID,
            DOCX_HEADER_PART,
        ),
        (
            &frame.first_header[..],
            frame.first_page_different,
            true,
            "first",
            "rIdHeader2",
            "word/header2.xml",
        ),
        (
            &frame.even_header[..],
            frame.even_odd_different,
            true,
            "even",
            "rIdHeader3",
            "word/header3.xml",
        ),
        (
            &frame.footer[..],
            true,
            false,
            "default",
            DOCX_FOOTER_REL_ID,
            DOCX_FOOTER_PART,
        ),
        (
            &frame.first_footer[..],
            frame.first_page_different,
            false,
            "first",
            "rIdFooter2",
            "word/footer2.xml",
        ),
        (
            &frame.even_footer[..],
            frame.even_odd_different,
            false,
            "even",
            "rIdFooter3",
            "word/footer3.xml",
        ),
    ];
    for (paragraphs, enabled, is_header, ref_type, id, part) in slots {
        if !enabled {
            continue;
        }
        let Some(xml) = docx_frame_part(paragraphs, is_header) else {
            continue;
        };
        let (tag, kind, content_type) = if is_header {
            ("w:headerReference", "header", DOCX_HEADER_CONTENT_TYPE)
        } else {
            ("w:footerReference", "footer", DOCX_FOOTER_CONTENT_TYPE)
        };
        refs.section
            .push_str(&format!("<{tag} w:type=\"{ref_type}\" r:id=\"{id}\"/>"));
        refs.relationships.push_str(&format!(
            "<Relationship Id=\"{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}\" Target=\"{}\"/>",
            part.trim_start_matches("word/")
        ));
        refs.content_types.push_str(&format!(
            "<Override PartName=\"/{part}\" ContentType=\"{content_type}\"/>"
        ));
        entries.push((part.to_string(), xml.into_bytes()));
    }
    if frame.first_page_different {
        refs.title_pg = "<w:titlePg/>";
    }
    (entries, refs)
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

pub(crate) struct OdtMasterInput<'a> {
    pub header: Option<&'a str>,
    pub footer: Option<&'a str>,
    pub first_header: Option<&'a str>,
    pub first_footer: Option<&'a str>,
    pub even_header: Option<&'a str>,
    pub even_footer: Option<&'a str>,
    pub first_page_different: bool,
    pub even_odd_different: bool,
    pub margins: Option<&'a RichPageMargins>,
    pub paper: Option<&'a RichPaper>,
    pub binding: RichBinding,
    pub columns: Option<RichColumns>,
}

/// The page layout and master page that give an ODT its page setup, columns, header and footer:
/// `(page layout, master styles)`.
pub(crate) fn odt_master_page(inp: OdtMasterInput<'_>) -> (String, String) {
    let m = inp.margins.copied().unwrap_or(RichPageMargins {
        top: 1191,
        bottom: 1191,
        header: 709,
        footer: 709,
        left: 1134,
        right: 1134,
    });
    let paper = inp.paper.copied().unwrap_or(RichPaper::A4);
    let orientation = if paper.width > paper.height {
        "landscape"
    } else {
        "portrait"
    };
    let any_hdr = inp.header.is_some() || inp.first_header.is_some() || inp.even_header.is_some();
    let any_ftr = inp.footer.is_some() || inp.first_footer.is_some() || inp.even_footer.is_some();
    let (margin_top, header_height) = if any_hdr {
        (m.header, m.top.saturating_sub(m.header))
    } else {
        (m.top, 0)
    };
    let (margin_bottom, footer_height) = if any_ftr {
        (m.footer, m.bottom.saturating_sub(m.footer))
    } else {
        (m.bottom, 0)
    };
    let usage = if inp.binding.kind == RichBindingKind::Duplex {
        " style:page-usage=\"mirrored\""
    } else {
        ""
    };
    let cols_xml = match inp.columns.filter(|c| c.count > 1) {
        Some(cols) => format!(
            "<style:columns fo:column-count=\"{}\" fo:column-gap=\"{}\"/>",
            cols.count.min(16),
            odt_pt(u32::from(cols.gap_twips))
        ),
        None => String::new(),
    };
    let layout = format!(
        concat!(
            "<style:page-layout style:name=\"pm1\"><style:page-layout-properties fo:page-width=\"{}\" ",
            "fo:page-height=\"{}\" style:print-orientation=\"{}\" fo:margin-top=\"{}\" ",
            "fo:margin-bottom=\"{}\" fo:margin-left=\"{}\" fo:margin-right=\"{}\"{}>{}</style:page-layout-properties>",
            "<style:header-style><style:header-footer-properties fo:min-height=\"{}\" fo:margin-bottom=\"0pt\"/></style:header-style>",
            "<style:footer-style><style:header-footer-properties fo:min-height=\"{}\" fo:margin-top=\"0pt\"/></style:footer-style>",
            "</style:page-layout>"
        ),
        odt_pt(paper.width),
        odt_pt(paper.height),
        orientation,
        odt_pt(margin_top),
        odt_pt(margin_bottom),
        odt_pt(m.left),
        odt_pt(m.right),
        usage,
        cols_xml,
        odt_pt(header_height),
        odt_pt(footer_height)
    );
    let push_variant = |master: &mut String, tag: &str, enabled: bool, body: Option<&str>| {
        if !enabled {
            return;
        }
        match body {
            Some(xml) => master.push_str(&format!("<{tag}>{xml}</{tag}>")),
            None => master.push_str(&format!("<{tag}/>")),
        }
    };
    let mut master =
        String::from("<office:master-styles><style:master-page style:name=\"Standard\" style:page-layout-name=\"pm1\">");
    if let Some(xml) = inp.header {
        master.push_str(&format!("<style:header>{xml}</style:header>"));
    }
    push_variant(
        &mut master,
        "style:header-left",
        inp.even_odd_different,
        inp.even_header,
    );
    push_variant(
        &mut master,
        "style:header-first",
        inp.first_page_different,
        inp.first_header,
    );
    if let Some(xml) = inp.footer {
        master.push_str(&format!("<style:footer>{xml}</style:footer>"));
    }
    push_variant(
        &mut master,
        "style:footer-left",
        inp.even_odd_different,
        inp.even_footer,
    );
    push_variant(
        &mut master,
        "style:footer-first",
        inp.first_page_different,
        inp.first_footer,
    );
    master.push_str("</style:master-page></office:master-styles>");
    (layout, master)
}

/// `styles.xml` of an ODT with a page setup, a header or a footer.
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

/// The `hp:ctrl` of a header or footer for the first paragraph of a HWPX section; `apply` is `"BOTH"`, `"ODD"` or `"EVEN"`.
pub(crate) fn hwpx_frame_control_typed(
    header: bool,
    id: u32,
    apply: &str,
    paragraphs: &str,
) -> String {
    let tag = if header { "hp:header" } else { "hp:footer" };
    format!(
        concat!(
            "<hp:ctrl><{tag} id=\"{id}\" applyPageType=\"{apply}\">",
            "<hp:subList id=\"\" textDirection=\"HORIZONTAL\" lineWrap=\"BREAK\" vertAlign=\"TOP\" ",
            "linkListIDRef=\"0\" linkListNextIDRef=\"0\" textWidth=\"0\" textHeight=\"0\" ",
            "hasTextRef=\"0\" hasNumRef=\"0\">{paragraphs}</hp:subList></{tag}></hp:ctrl>"
        ),
        tag = tag,
        id = id,
        apply = apply,
        paragraphs = paragraphs
    )
}

/// `<hp:ctrl><hp:pageHiding .../></hp:ctrl>` hiding header/footer on the first page (`rhwp` `section.rs:2914`).
pub(crate) fn hwpx_page_hiding_control(hide_header: bool, hide_footer: bool) -> String {
    format!(
        "<hp:ctrl><hp:pageHiding hideHeader=\"{}\" hideFooter=\"{}\" hideMasterPage=\"0\" hideBorder=\"0\" hideFill=\"0\" hidePageNum=\"0\"/></hp:ctrl>",
        u8::from(hide_header),
        u8::from(hide_footer),
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

/// The inverse of [`hwpx_margin_values`] for the `(header, footer, top, bottom)` of a `hp:margin` and its side
/// margins `(left, right)`, all in HWPUNIT.
pub(crate) fn margins_from_hwpx(
    (header, footer, top, bottom): (u32, u32, u32, u32),
    (left, right): (u32, u32),
) -> RichPageMargins {
    RichPageMargins {
        top: top.saturating_add(header) / HWPUNIT_PER_TWIP,
        bottom: bottom.saturating_add(footer) / HWPUNIT_PER_TWIP,
        header: top / HWPUNIT_PER_TWIP,
        footer: footer / HWPUNIT_PER_TWIP,
        left: left / HWPUNIT_PER_TWIP,
        right: right / HWPUNIT_PER_TWIP,
    }
}
