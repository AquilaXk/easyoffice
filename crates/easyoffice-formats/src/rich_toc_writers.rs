//! Table-of-contents blocks for the DOCX and ODT writers: the paragraph styles of the title and the
//! nine entry levels, and the container a run of consecutive TOC paragraphs sits in (DOCX
//! `w:sdt` with `docPartGallery` "Table of Contents", ODT `text:table-of-content`). The readers
//! recover the role from the style name alone, so a file from another word processor reads the same.

use crate::rich_document::{RichBlock, RichToc, MAX_TOC_LEVEL};

/// Indent added per entry level, in twips (about 0.4 cm).
const TOC_INDENT_TWIPS: u32 = 220;

const DOCX_TOC_OPEN: &str = "<w:sdt><w:sdtPr><w:docPartObj><w:docPartGallery w:val=\"Table of Contents\"/><w:docPartUnique/></w:docPartObj></w:sdtPr><w:sdtContent>";
const DOCX_TOC_CLOSE: &str = "</w:sdtContent></w:sdt>";
const ODT_TOC_CLOSE: &str = "</text:index-body></text:table-of-content>";
const DOCX_FIELD_END: &str = "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>";

pub(crate) fn is_toc_block(block: &RichBlock) -> bool {
    matches!(block, RichBlock::Paragraph(p) if p.toc.is_some())
}

/// The `TOC \\o "1-N" \\h` field a word processor can update: it begins in the first entry paragraph and ends
/// in the last one, with the entries as its result.
fn docx_field_begin(levels: u8) -> String {
    format!(
        "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> TOC \\o \"1-{levels}\" \\h </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>"
    )
}

fn is_entry(block: Option<&RichBlock>) -> bool {
    matches!(block, Some(RichBlock::Paragraph(p)) if matches!(p.toc, Some(RichToc::Entry(_))))
}

/// Field markup to put before and after the runs of the paragraph at `index` (`(begin, end)`, both empty
/// unless it is the first or the last entry of a block).
pub(crate) fn docx_field_edges(blocks: &[RichBlock], index: usize) -> (String, &'static str) {
    if !is_entry(blocks.get(index)) {
        return (String::new(), "");
    }
    let first = index == 0 || !is_entry(blocks.get(index - 1));
    let last = !is_entry(blocks.get(index + 1));
    (
        if first {
            docx_field_begin(entry_levels_from(blocks, index))
        } else {
            String::new()
        },
        if last { DOCX_FIELD_END } else { "" },
    )
}

/// Deepest entry level of the entries that run on from `index` (at least 1).
pub(crate) fn entry_levels_from(blocks: &[RichBlock], index: usize) -> u8 {
    blocks[index..]
        .iter()
        .take_while(|b| is_toc_block(b))
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => match p.toc {
                Some(RichToc::Entry(level)) => Some(level),
                _ => None,
            },
            _ => None,
        })
        .max()
        .unwrap_or(1)
}

/// `text:table-of-content` opening with the source an ODT editor rebuilds the block from: one entry
/// template per level (a link around the heading text).
fn odt_toc_open(levels: u8) -> String {
    let mut xml = format!(
        "<text:table-of-content text:name=\"Table of Contents1\"><text:table-of-content-source text:outline-level=\"{levels}\">"
    );
    for level in 1..=levels {
        xml.push_str(&format!(
            "<text:table-of-content-entry-template text:outline-level=\"{level}\" text:style-name=\"{}\"><text:index-entry-link-start/><text:index-entry-text/><text:index-entry-link-end/></text:table-of-content-entry-template>",
            odt_toc_style_name(RichToc::Entry(level))
        ));
    }
    xml.push_str("</text:table-of-content-source><text:index-body>");
    xml
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
    open_xml: fn(u8) -> String,
    close_xml: &'static str,
}

impl TocScope {
    pub(crate) fn docx() -> Self {
        Self::new(|_| DOCX_TOC_OPEN.to_string(), DOCX_TOC_CLOSE)
    }

    pub(crate) fn odt() -> Self {
        Self::new(odt_toc_open, ODT_TOC_CLOSE)
    }

    fn new(open_xml: fn(u8) -> String, close_xml: &'static str) -> Self {
        Self {
            open: false,
            open_xml,
            close_xml,
        }
    }

    /// Call before each block (`index` into `blocks`), and once after the last block with `index` past the end.
    pub(crate) fn enter(&mut self, out: &mut String, blocks: &[RichBlock], index: usize) {
        let in_toc = blocks.get(index).is_some_and(is_toc_block);
        if in_toc == self.open {
            return;
        }
        if in_toc {
            out.push_str(&(self.open_xml)(entry_levels_from(blocks, index)));
        } else {
            out.push_str(self.close_xml);
        }
        self.open = in_toc;
    }
}
