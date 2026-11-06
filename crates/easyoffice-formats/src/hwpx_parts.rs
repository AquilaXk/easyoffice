//! Static KS X 6101 (OWPML) package parts used by the HWPX writer.
//!
//! `header.xml` defines one font per language slot, one character shape (`charPr id="0"`), one
//! paragraph shape (`paraPr id="0"`), two border fills, and the default style (`style id="0"`);
//! every paragraph and run in `section0.xml` refers to these IDs.

use crate::XmlSanitizer;

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
/// Default cell height for generated tables.
pub(crate) const CELL_HEIGHT: u32 = 1_000;

const FONT_LANGS: [&str; 7] = [
    "HANGUL", "LATIN", "HANJA", "JAPANESE", "OTHER", "SYMBOL", "USER",
];
const DEFAULT_FONT_FACE: &str = "맑은 고딕";

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

/// `Contents/header.xml` with font faces, character/paragraph shapes, and styles.
pub(crate) fn header_xml() -> String {
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
            "<hh:fontface lang=\"{lang}\" fontCnt=\"1\"><hh:font id=\"0\" face=\"{}\" type=\"TTF\" isEmbedded=\"0\"/></hh:fontface>",
            XmlSanitizer::escape(DEFAULT_FONT_FACE)
        ));
    }
    xml.push_str("</hh:fontfaces>");

    xml.push_str("<hh:borderFills itemCnt=\"2\">");
    xml.push_str(&border_fill(1, "NONE"));
    xml.push_str(&border_fill(2, "SOLID"));
    xml.push_str("</hh:borderFills>");

    xml.push_str("<hh:charProperties itemCnt=\"1\">");
    xml.push_str("<hh:charPr id=\"0\" height=\"1000\" textColor=\"#000000\" shadeColor=\"none\" useFontSpace=\"0\" useKerning=\"0\" symMark=\"NONE\" borderFillIDRef=\"1\">");
    xml.push_str(&format!("<hh:fontRef {}/>", per_lang_attrs("0")));
    xml.push_str(&format!("<hh:ratio {}/>", per_lang_attrs("100")));
    xml.push_str(&format!("<hh:spacing {}/>", per_lang_attrs("0")));
    xml.push_str(&format!("<hh:relSz {}/>", per_lang_attrs("100")));
    xml.push_str(&format!("<hh:offset {}/>", per_lang_attrs("0")));
    xml.push_str(concat!(
        "<hh:underline type=\"NONE\" shape=\"SOLID\" color=\"#000000\"/>",
        "<hh:strikeout shape=\"NONE\" color=\"#000000\"/>",
        "<hh:outline type=\"NONE\"/>",
        "<hh:shadow type=\"NONE\" color=\"#C0C0C0\" offsetX=\"10\" offsetY=\"10\"/>",
        "</hh:charPr></hh:charProperties>",
        "<hh:tabProperties itemCnt=\"1\"><hh:tabPr id=\"0\" autoTabLeft=\"0\" autoTabRight=\"0\"/></hh:tabProperties>",
        "<hh:paraProperties itemCnt=\"1\">",
        "<hh:paraPr id=\"0\" tabPrIDRef=\"0\" condense=\"0\" fontLineHeight=\"0\" snapToGrid=\"1\" suppressLineNumbers=\"0\" checked=\"0\">",
        "<hh:align horizontal=\"JUSTIFY\" vertical=\"BASELINE\"/>",
        "<hh:heading type=\"NONE\" idRef=\"0\" level=\"0\"/>",
        "<hh:breakSetting breakLatinWord=\"KEEP_WORD\" breakNonLatinWord=\"KEEP_WORD\" widowOrphan=\"0\" keepWithNext=\"0\" keepLines=\"0\" pageBreakBefore=\"0\" lineWrap=\"BREAK\"/>",
        "<hh:autoSpacing eAsianEng=\"0\" eAsianNum=\"0\"/>",
        "<hh:margin><hc:intent value=\"0\" unit=\"HWPUNIT\"/><hc:left value=\"0\" unit=\"HWPUNIT\"/>",
        "<hc:right value=\"0\" unit=\"HWPUNIT\"/><hc:prev value=\"0\" unit=\"HWPUNIT\"/><hc:next value=\"0\" unit=\"HWPUNIT\"/></hh:margin>",
        "<hh:lineSpacing type=\"PERCENT\" value=\"160\" unit=\"HWPUNIT\"/>",
        "<hh:border borderFillIDRef=\"1\" offsetLeft=\"0\" offsetRight=\"0\" offsetTop=\"0\" offsetBottom=\"0\" connect=\"0\" ignoreMargin=\"0\"/>",
        "</hh:paraPr></hh:paraProperties>",
        "<hh:styles itemCnt=\"1\">",
        "<hh:style id=\"0\" type=\"PARA\" name=\"바탕글\" engName=\"Normal\" paraPrIDRef=\"0\" charPrIDRef=\"0\" nextStyleIDRef=\"0\" langID=\"1042\" lockForm=\"0\"/>",
        "</hh:styles></hh:refList>",
        "<hh:compatibleDocument targetProgram=\"HWP201X\"><hh:layoutCompatibility/></hh:compatibleDocument>",
        "<hh:docOption><hh:linkinfo path=\"\" pageInherit=\"0\" footnoteInherit=\"0\"/></hh:docOption>",
        "</hh:head>"
    ));
    xml
}

/// Section properties (`hp:secPr`) plus the column control; must sit in the first run of the
/// first paragraph of each section.
pub(crate) fn section_properties_xml() -> String {
    format!(
        concat!(
            "<hp:secPr id=\"\" textDirection=\"HORIZONTAL\" spaceColumns=\"1134\" tabStop=\"8000\" tabStopVal=\"4000\" tabStopUnit=\"HWPUNIT\" outlineShapeIDRef=\"0\" memoShapeIDRef=\"0\" textVerticalWidthHead=\"0\" masterPageCnt=\"0\">",
            "<hp:grid lineGrid=\"0\" charGrid=\"0\" wonggojiFormat=\"0\"/>",
            "<hp:startNum pageStartsOn=\"BOTH\" page=\"0\" pic=\"0\" tbl=\"0\" equation=\"0\"/>",
            "<hp:visibility hideFirstHeader=\"0\" hideFirstFooter=\"0\" hideFirstMasterPage=\"0\" border=\"SHOW_ALL\" fill=\"SHOW_ALL\" hideFirstPageNum=\"0\" hideFirstEmptyLine=\"0\" showLineNumber=\"0\"/>",
            "<hp:pagePr landscape=\"WIDELY\" width=\"{w}\" height=\"{h}\" gutterType=\"LEFT_ONLY\">",
            "<hp:margin header=\"4252\" footer=\"4252\" gutter=\"0\" left=\"{m}\" right=\"{m}\" top=\"5668\" bottom=\"4252\"/>",
            "</hp:pagePr></hp:secPr>",
            "<hp:ctrl><hp:colPr id=\"\" type=\"NEWSPAPER\" layout=\"LEFT\" colCount=\"1\" sameSz=\"1\" sameGap=\"0\"/></hp:ctrl>"
        ),
        w = PAGE_WIDTH,
        h = PAGE_HEIGHT,
        m = MARGIN_SIDE
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
pub(crate) fn manifest_xml(mime: &str, parts: &[&str]) -> String {
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><odf:manifest xmlns:odf=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\"><odf:file-entry odf:full-path=\"/\" odf:media-type=\"{mime}\"/>"
    );
    for part in parts {
        xml.push_str(&format!(
            "<odf:file-entry odf:full-path=\"{part}\" odf:media-type=\"application/xml\"/>"
        ));
    }
    xml.push_str("</odf:manifest>");
    xml
}
