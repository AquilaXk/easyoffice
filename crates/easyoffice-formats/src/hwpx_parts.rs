//! Static KS X 6101 (OWPML) package parts used by the HWPX writer.
//!
//! `header.xml` defines one font per language slot, the character shapes (`charPr`, id 0 is the
//! default), paragraph shapes (`paraPr`, id 0 is the default), two border fills, and the default
//! style (`style id="0"`) plus optional heading styles; every paragraph and run in
//! `section0.xml` refers to these IDs.

use crate::{RichScript, XmlSanitizer};

pub(crate) const NS_HEAD: &str = "http://www.owpml.org/owpml/2011/head";
pub(crate) const NS_CORE: &str = "http://www.owpml.org/owpml/2011/core";
pub(crate) const NS_SECTION: &str = "http://www.owpml.org/owpml/2011/section";
pub(crate) const NS_PARAGRAPH: &str = "http://www.owpml.org/owpml/2011/paragraph";

/// A4 portrait page in HWPUNIT (1/7200 inch) with 30 mm side margins.
const PAGE_WIDTH: u32 = 59_528;
const PAGE_HEIGHT: u32 = 84_186;
const MARGIN_SIDE: u32 = 8_504;
/// Usable text width of the page above.
pub(crate) const TEXT_WIDTH: u32 = PAGE_WIDTH - 2 * MARGIN_SIDE;
/// Left margin (HWPUNIT) the writer gives list paragraphs; the reader treats it as the list marker.
pub(crate) const LIST_LEFT_MARGIN: u32 = 2_000;
/// Single-spacing-relative line spacing written when a paragraph sets none (percent).
pub(crate) const DEFAULT_LINE_PERCENT: u32 = 160;
/// HWPUNIT is 1/100 point and a twip is 1/20 point, so one twip is five HWPUNIT.
pub(crate) const HWPUNIT_PER_TWIP: u32 = 5;
/// Default cell height for generated tables.
pub(crate) const CELL_HEIGHT: u32 = 1_000;

/// `hh:heading` type of a paragraph that is no list item.
pub(crate) const HEADING_NONE: &str = "NONE";
/// Heading types of list items; each names the one definition below by `idRef="1"`.
pub(crate) const HEADING_BULLET: &str = "BULLET";
pub(crate) const HEADING_NUMBER: &str = "NUMBER";

/// The one numbering (`1.`, `2.`, ... at level 1) and the one bullet (`•`) a list paragraph refers
/// to. Without a definition a layout engine has no marker to draw for the item.
const NUMBERING_XML: &str = concat!(
    "<hh:numberings itemCnt=\"1\"><hh:numbering id=\"1\" start=\"0\">",
    "<hh:paraHead start=\"1\" level=\"1\" align=\"LEFT\" useInstWidth=\"1\" autoIndent=\"1\" widthAdjust=\"0\" textOffsetType=\"PERCENT\" textOffset=\"50\" numFormat=\"DIGIT\" charPrIDRef=\"4294967295\" checkable=\"0\">^1.</hh:paraHead>",
    "</hh:numbering></hh:numberings>"
);
const BULLET_XML: &str = concat!(
    "<hh:bullets itemCnt=\"1\"><hh:bullet id=\"1\" char=\"•\" useImage=\"0\">",
    "<hh:paraHead level=\"0\" align=\"LEFT\" useInstWidth=\"0\" autoIndent=\"1\" widthAdjust=\"0\" textOffsetType=\"PERCENT\" textOffset=\"50\" numFormat=\"DIGIT\" charPrIDRef=\"4294967295\" checkable=\"0\"/>",
    "</hh:bullet></hh:bullets>"
);

const FONT_LANGS: [&str; 7] = [
    "HANGUL", "LATIN", "HANJA", "JAPANESE", "OTHER", "SYMBOL", "USER",
];
pub(crate) const DEFAULT_FONT_FACE: &str = "맑은 고딕";
/// Body text height (1/100 pt) of the default character shape: 11 pt, the editor default.
pub(crate) const DEFAULT_CHAR_HEIGHT: u32 = 1100;

fn per_lang_attrs(value: &str) -> String {
    format!(
        "hangul=\"{value}\" latin=\"{value}\" hanja=\"{value}\" japanese=\"{value}\" other=\"{value}\" symbol=\"{value}\" user=\"{value}\""
    )
}

fn border_fill(id: u32, line_type: &str) -> String {
    let mut xml = format!(
        "<hh:borderFill id=\"{id}\" threeD=\"0\" shadow=\"0\" centerLine=\"NONE\" breakCellSeparateLine=\"0\"><hh:slash type=\"NONE\" Crooked=\"0\" isCounter=\"0\"/><hh:backSlash type=\"NONE\" Crooked=\"0\" isCounter=\"0\"/>"
    );
    for side in ["leftBorder", "rightBorder", "topBorder", "bottomBorder"] {
        xml.push_str(&format!(
            "<hh:{side} type=\"{line_type}\" width=\"0.12 mm\" color=\"#000000\"/>"
        ));
    }
    xml.push_str(
        "<hh:diagonal type=\"SOLID\" width=\"0.1 mm\" color=\"#000000\"/></hh:borderFill>",
    );
    xml
}

/// Character shape (`hh:charPr`) written to `header.xml`; `height` is in 1/100 pt, `font` indexes
/// the font table (0 is the default face), and the colors are `0xRRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct HwpxCharShape {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub height: u32,
    pub font: usize,
    pub text_color: Option<u32>,
    pub shade_color: Option<u32>,
    pub script: RichScript,
}

impl Default for HwpxCharShape {
    fn default() -> Self {
        Self {
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            height: DEFAULT_CHAR_HEIGHT,
            font: 0,
            text_color: None,
            shade_color: None,
            script: RichScript::Normal,
        }
    }
}

/// Paragraph shape (`hh:paraPr`): OWPML horizontal alignment, list heading type, and left margin (HWPUNIT).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct HwpxParaShape {
    pub align: Option<&'static str>,
    pub heading_type: &'static str,
    pub left_margin: u32,
    /// First-line indent (`hc:intent`), HWPUNIT; negative for a hanging indent.
    pub intent: i32,
    /// Line spacing in percent of single spacing.
    pub line_percent: u32,
}

impl Default for HwpxParaShape {
    fn default() -> Self {
        Self {
            align: None,
            heading_type: HEADING_NONE,
            left_margin: 0,
            intent: 0,
            line_percent: DEFAULT_LINE_PERCENT,
        }
    }
}

/// Paragraph style (`hh:style`) referencing a paragraph and a character shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HwpxStyle {
    pub name: String,
    pub eng_name: String,
    pub para_pr: usize,
    pub char_pr: usize,
}

fn char_pr_xml(id: usize, shape: &HwpxCharShape) -> String {
    let shade = shape
        .shade_color
        .map_or_else(|| "none".to_string(), |c| format!("#{c:06X}"));
    let mut xml = format!(
        "<hh:charPr id=\"{id}\" height=\"{}\" textColor=\"#{:06X}\" shadeColor=\"{shade}\" useFontSpace=\"0\" useKerning=\"0\" symMark=\"NONE\" borderFillIDRef=\"1\">",
        shape.height,
        shape.text_color.unwrap_or(0)
    );
    xml.push_str(&format!(
        "<hh:fontRef {}/>",
        per_lang_attrs(&shape.font.to_string())
    ));
    xml.push_str(&format!("<hh:ratio {}/>", per_lang_attrs("100")));
    xml.push_str(&format!("<hh:spacing {}/>", per_lang_attrs("0")));
    xml.push_str(&format!("<hh:relSz {}/>", per_lang_attrs("100")));
    xml.push_str(&format!("<hh:offset {}/>", per_lang_attrs("0")));
    if shape.italic {
        xml.push_str("<hh:italic/>");
    }
    if shape.bold {
        xml.push_str("<hh:bold/>");
    }
    xml.push_str(&format!(
        "<hh:underline type=\"{}\" shape=\"SOLID\" color=\"#000000\"/>",
        if shape.underline { "BOTTOM" } else { "NONE" }
    ));
    xml.push_str(&format!(
        "<hh:strikeout shape=\"{}\" color=\"#000000\"/>",
        if shape.strike { "SOLID" } else { "NONE" }
    ));
    xml.push_str(concat!(
        "<hh:outline type=\"NONE\"/>",
        "<hh:shadow type=\"NONE\" color=\"#C0C0C0\" offsetX=\"10\" offsetY=\"10\"/>",
    ));
    match shape.script {
        RichScript::Normal => {}
        RichScript::Superscript => xml.push_str("<hh:supscript/>"),
        RichScript::Subscript => xml.push_str("<hh:subscript/>"),
    }
    xml.push_str("</hh:charPr>");
    xml
}

fn para_pr_xml(id: usize, shape: &HwpxParaShape) -> String {
    let align_xml = match shape.align {
        Some(a) => format!("<hh:align horizontal=\"{a}\" vertical=\"BASELINE\"/>"),
        None => "<hh:align vertical=\"BASELINE\"/>".to_string(),
    };
    format!(
        concat!(
            "<hh:paraPr id=\"{id}\" tabPrIDRef=\"0\" condense=\"0\" fontLineHeight=\"0\" snapToGrid=\"1\" suppressLineNumbers=\"0\" checked=\"0\">",
            "{align}",
            "<hh:heading type=\"{heading_type}\" idRef=\"{heading_ref}\" level=\"0\"/>",
            "<hh:breakSetting breakLatinWord=\"KEEP_WORD\" breakNonLatinWord=\"KEEP_WORD\" widowOrphan=\"0\" keepWithNext=\"0\" keepLines=\"0\" pageBreakBefore=\"0\" lineWrap=\"BREAK\"/>",
            "<hh:autoSpacing eAsianEng=\"0\" eAsianNum=\"0\"/>",
            "<hh:margin><hc:intent value=\"{intent}\" unit=\"HWPUNIT\"/><hc:left value=\"{left}\" unit=\"HWPUNIT\"/>",
            "<hc:right value=\"0\" unit=\"HWPUNIT\"/><hc:prev value=\"0\" unit=\"HWPUNIT\"/><hc:next value=\"0\" unit=\"HWPUNIT\"/></hh:margin>",
            "<hh:lineSpacing type=\"PERCENT\" value=\"{line}\" unit=\"HWPUNIT\"/>",
            "<hh:border borderFillIDRef=\"1\" offsetLeft=\"0\" offsetRight=\"0\" offsetTop=\"0\" offsetBottom=\"0\" connect=\"0\" ignoreMargin=\"0\"/>",
            "</hh:paraPr>"
        ),
        id = id,
        align = align_xml,
        heading_type = shape.heading_type,
        heading_ref = u8::from(shape.heading_type != HEADING_NONE),
        left = shape.left_margin,
        intent = shape.intent,
        line = shape.line_percent
    )
}

/// `Contents/header.xml` with the default shapes only (`charPr`/`paraPr`/`style` id 0).
pub(crate) fn header_xml() -> String {
    header_xml_with(
        &[HwpxCharShape::default()],
        &[HwpxParaShape::default()],
        &[],
        &[],
    )
}

/// `Contents/header.xml` with font faces, the given character/paragraph shape tables (index =
/// ID; index 0 is the default), and the default style followed by `extra_styles`. `extra_fonts`
/// are declared after the default face (font ids 1..).
pub(crate) fn header_xml_with(
    char_shapes: &[HwpxCharShape],
    para_shapes: &[HwpxParaShape],
    extra_styles: &[HwpxStyle],
    extra_fonts: &[String],
) -> String {
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><hh:head xmlns:hh=\"{NS_HEAD}\" xmlns:hc=\"{NS_CORE}\" version=\"1.4\" secCnt=\"1\">"
    );
    xml.push_str(
        "<hh:beginNum page=\"1\" footnote=\"1\" endnote=\"1\" pic=\"1\" tbl=\"1\" equation=\"1\"/>",
    );
    xml.push_str("<hh:refList>");

    xml.push_str(&format!("<hh:fontfaces itemCnt=\"{}\">", FONT_LANGS.len()));
    for lang in FONT_LANGS {
        xml.push_str(&format!(
            "<hh:fontface lang=\"{lang}\" fontCnt=\"{}\">",
            extra_fonts.len() + 1
        ));
        for (id, face) in std::iter::once(DEFAULT_FONT_FACE)
            .chain(extra_fonts.iter().map(String::as_str))
            .enumerate()
        {
            xml.push_str(&format!(
                "<hh:font id=\"{id}\" face=\"{}\" type=\"TTF\" isEmbedded=\"0\"/>",
                XmlSanitizer::escape(face)
            ));
        }
        xml.push_str("</hh:fontface>");
    }
    xml.push_str("</hh:fontfaces>");

    xml.push_str("<hh:borderFills itemCnt=\"2\">");
    xml.push_str(&border_fill(1, "NONE"));
    xml.push_str(&border_fill(2, "SOLID"));
    xml.push_str("</hh:borderFills>");

    xml.push_str(&format!(
        "<hh:charProperties itemCnt=\"{}\">",
        char_shapes.len()
    ));
    for (id, shape) in char_shapes.iter().enumerate() {
        xml.push_str(&char_pr_xml(id, shape));
    }
    xml.push_str("</hh:charProperties>");
    xml.push_str("<hh:tabProperties itemCnt=\"1\"><hh:tabPr id=\"0\" autoTabLeft=\"0\" autoTabRight=\"0\"/></hh:tabProperties>");
    let uses = |heading: &str| {
        para_shapes
            .iter()
            .any(|shape| shape.heading_type == heading)
    };
    if uses(HEADING_NUMBER) {
        xml.push_str(NUMBERING_XML);
    }
    if uses(HEADING_BULLET) {
        xml.push_str(BULLET_XML);
    }
    xml.push_str(&format!(
        "<hh:paraProperties itemCnt=\"{}\">",
        para_shapes.len()
    ));
    for (id, shape) in para_shapes.iter().enumerate() {
        xml.push_str(&para_pr_xml(id, shape));
    }
    xml.push_str("</hh:paraProperties>");
    xml.push_str(&format!(
        "<hh:styles itemCnt=\"{}\">",
        extra_styles.len() + 1
    ));
    xml.push_str("<hh:style id=\"0\" type=\"PARA\" name=\"바탕글\" engName=\"Normal\" paraPrIDRef=\"0\" charPrIDRef=\"0\" nextStyleIDRef=\"0\" langID=\"1042\" lockForm=\"0\"/>");
    for (idx, style) in extra_styles.iter().enumerate() {
        xml.push_str(&format!(
            "<hh:style id=\"{}\" type=\"PARA\" name=\"{}\" engName=\"{}\" paraPrIDRef=\"{}\" charPrIDRef=\"{}\" nextStyleIDRef=\"0\" langID=\"1042\" lockForm=\"0\"/>",
            idx + 1,
            XmlSanitizer::escape(&style.name),
            XmlSanitizer::escape(&style.eng_name),
            style.para_pr,
            style.char_pr
        ));
    }
    xml.push_str(concat!(
        "</hh:styles></hh:refList>",
        "<hh:compatibleDocument targetProgram=\"HWP201X\"><hh:layoutCompatibility/></hh:compatibleDocument>",
        "<hh:docOption><hh:linkinfo path=\"\" pageInherit=\"0\" footnoteInherit=\"0\"/></hh:docOption>",
        "</hh:head>"
    ));
    xml
}

/// Section properties (`hp:secPr`) plus the column control; must sit in the first run of the
/// first paragraph of each section.
pub(crate) fn section_properties_xml(margins: Option<&crate::RichPageMargins>) -> String {
    let (header, footer, top, bottom) = margins.map_or(
        (4252, 4252, 5668, 4252),
        crate::rich_page_frame::hwpx_margin_values,
    );
    format!(
        concat!(
            "<hp:secPr id=\"\" textDirection=\"HORIZONTAL\" spaceColumns=\"1134\" tabStop=\"8000\" tabStopVal=\"4000\" tabStopUnit=\"HWPUNIT\" outlineShapeIDRef=\"0\" memoShapeIDRef=\"0\" textVerticalWidthHead=\"0\" masterPageCnt=\"0\">",
            "<hp:grid lineGrid=\"0\" charGrid=\"0\" wonggojiFormat=\"0\"/>",
            "<hp:startNum pageStartsOn=\"BOTH\" page=\"0\" pic=\"0\" tbl=\"0\" equation=\"0\"/>",
            "<hp:visibility hideFirstHeader=\"0\" hideFirstFooter=\"0\" hideFirstMasterPage=\"0\" border=\"SHOW_ALL\" fill=\"SHOW_ALL\" hideFirstPageNum=\"0\" hideFirstEmptyLine=\"0\" showLineNumber=\"0\"/>",
            "<hp:pagePr landscape=\"WIDELY\" width=\"{w}\" height=\"{h}\" gutterType=\"LEFT_ONLY\">",
            "<hp:margin header=\"{header}\" footer=\"{footer}\" gutter=\"0\" left=\"{m}\" right=\"{m}\" top=\"{top}\" bottom=\"{bottom}\"/>",
            "</hp:pagePr></hp:secPr>",
            "<hp:ctrl><hp:colPr id=\"\" type=\"NEWSPAPER\" layout=\"LEFT\" colCount=\"1\" sameSz=\"1\" sameGap=\"0\"/></hp:ctrl>"
        ),
        w = PAGE_WIDTH,
        h = PAGE_HEIGHT,
        m = MARGIN_SIDE,
        header = header,
        footer = footer,
        top = top,
        bottom = bottom
    )
}

/// `version.xml` (document format version record).
pub(crate) const VERSION_XML: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
    "<hv:HCFVersion xmlns:hv=\"http://www.owpml.org/owpml/2011/version\" tagetApplication=\"WORDPROCESSOR\" ",
    "major=\"5\" minor=\"1\" micro=\"0\" buildNumber=\"0\" os=\"1\" xmlVersion=\"1.4\" application=\"EasyOffice\" appVersion=\"1.0.0\"/>"
);

/// `settings.xml` (application settings: caret position).
pub(crate) const SETTINGS_XML: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
    "<ha:HWPApplicationSetting xmlns:ha=\"http://www.owpml.org/owpml/2011/app\" ",
    "xmlns:config=\"urn:oasis:names:tc:opendocument:xmlns:config:1.0\">",
    "<ha:CaretPosition listIDRef=\"0\" paraIDRef=\"0\" pos=\"0\"/></ha:HWPApplicationSetting>"
);

/// `META-INF/container.xml` pointing at the OPF package document.
pub(crate) const CONTAINER_XML: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>",
    "<ocf:container xmlns:ocf=\"urn:oasis:names:tc:opendocument:xmlns:container\">",
    "<ocf:rootfiles><ocf:rootfile full-path=\"Contents/content.hpf\" media-type=\"application/hwpml-package+xml\"/></ocf:rootfiles>",
    "</ocf:container>"
);

/// `META-INF/manifest.xml` listing every part in the package.
pub(crate) fn manifest_xml(mime: &str, parts: &[&str], media: &[(String, &str)]) -> String {
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><odf:manifest xmlns:odf=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\"><odf:file-entry odf:full-path=\"/\" odf:media-type=\"{mime}\"/>"
    );
    for part in parts {
        xml.push_str(&format!(
            "<odf:file-entry odf:full-path=\"{part}\" odf:media-type=\"application/xml\"/>"
        ));
    }
    for (part, media_type) in media {
        xml.push_str(&format!(
            "<odf:file-entry odf:full-path=\"{part}\" odf:media-type=\"{media_type}\"/>"
        ));
    }
    xml.push_str("</odf:manifest>");
    xml
}
