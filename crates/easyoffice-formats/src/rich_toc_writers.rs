//! Table-of-contents blocks for the DOCX and ODT writers: the paragraph styles of the title and the
//! nine entry levels, and the container a run of consecutive TOC paragraphs sits in (DOCX
//! `w:sdt` with `docPartGallery` "Table of Contents", ODT `text:table-of-content`). The readers
//! recover the role from the style name alone, so a file from another word processor reads the same.

use crate::rich_document::{RichBlock, RichToc, MAX_TOC_LEVEL};

/// Indent added per entry level, in twips (about 0.4 cm).
const TOC_INDENT_TWIPS: u32 = 220;

const DOCX_TOC_OPEN: &str = "<w:sdt><w:sdtPr><w:docPartObj><w:docPartGallery w:val=\"Table of Contents\"/><w:docPartUnique/></w:docPartObj></w:sdtPr><w:sdtContent>";
const DOCX_TOC_CLOSE: &str = "</w:sdtContent></w:sdt>";
const ODT_TOC_OPEN: &str =
    "<text:table-of-content text:name=\"Table of Contents1\"><text:index-body>";
const ODT_TOC_CLOSE: &str = "</text:index-body></text:table-of-content>";

pub(crate) fn is_toc_block(block: &RichBlock) -> bool {
    matches!(block, RichBlock::Paragraph(p) if p.toc.is_some())
}

pub(crate) fn docx_toc_style_id(role: RichToc) -> String {
    match role {
        RichToc::Title => "TOCHeading".to_string(),
        RichToc::Entry(level) => format!("TOC{level}"),
    }
}

pub(crate) fn odt_toc_style_name(role: RichToc) -> String {
    match role {
        RichToc::Title => "Contents_20_Heading".to_string(),
        RichToc::Entry(level) => format!("Contents_20_{level}"),
    }
}

fn entry_levels() -> impl Iterator<Item = u8> {
    1..=MAX_TOC_LEVEL
}

/// `w:style` elements of the TOC paragraph styles.
pub(crate) fn docx_toc_styles_xml() -> String {
    let mut xml = String::from(
        "<w:style w:type=\"paragraph\" w:styleId=\"TOCHeading\"><w:name w:val=\"TOC Heading\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:pPr><w:keepNext/><w:spacing w:before=\"240\" w:after=\"120\"/></w:pPr><w:rPr><w:b/><w:bCs/><w:sz w:val=\"32\"/><w:szCs w:val=\"32\"/></w:rPr></w:style>",
    );
    for level in entry_levels() {
        xml.push_str(&format!(
            "<w:style w:type=\"paragraph\" w:styleId=\"TOC{level}\"><w:name w:val=\"toc {level}\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:pPr><w:spacing w:after=\"60\"/><w:ind w:left=\"{}\"/></w:pPr></w:style>",
            u32::from(level - 1) * TOC_INDENT_TWIPS
        ));
    }
    xml
}

/// Named `style:style` elements of the TOC paragraph styles (ODT automatic styles).
pub(crate) fn odt_toc_styles_xml() -> String {
    let mut xml = format!(
        "<style:style style:name=\"{}\" style:family=\"paragraph\"><style:text-properties fo:font-size=\"16pt\" fo:font-weight=\"bold\"/></style:style>",
        odt_toc_style_name(RichToc::Title)
    );
    for level in entry_levels() {
        xml.push_str(&format!(
            "<style:style style:name=\"{}\" style:family=\"paragraph\"><style:paragraph-properties fo:margin-left=\"{}pt\"/></style:style>",
            odt_toc_style_name(RichToc::Entry(level)),
            u32::from(level - 1) * TOC_INDENT_TWIPS / 20
        ));
    }
    xml
}

/// Opens and closes the TOC container as consecutive TOC paragraphs start and stop.
pub(crate) struct TocScope {
    open: bool,
    open_xml: &'static str,
    close_xml: &'static str,
}

impl TocScope {
    pub(crate) fn docx() -> Self {
        Self::new(DOCX_TOC_OPEN, DOCX_TOC_CLOSE)
    }

    pub(crate) fn odt() -> Self {
        Self::new(ODT_TOC_OPEN, ODT_TOC_CLOSE)
    }

    fn new(open_xml: &'static str, close_xml: &'static str) -> Self {
        Self {
            open: false,
            open_xml,
            close_xml,
        }
    }

    /// Call before each block with whether it is a TOC paragraph, and once after the last block with `false`.
    pub(crate) fn enter(&mut self, out: &mut String, in_toc: bool) {
        if in_toc != self.open {
            out.push_str(if in_toc {
                self.open_xml
            } else {
                self.close_xml
            });
            self.open = in_toc;
        }
    }
}
