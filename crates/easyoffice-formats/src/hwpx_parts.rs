//! Static KS X 6101 (OWPML) package parts used by the HWPX writer.
//!
//! `header.xml` defines one font per language slot, the character shapes (`charPr`, id 0 is the
//! default), paragraph shapes (`paraPr`, id 0 is the default), two border fills, and the default
//! style (`style id="0"`) plus optional heading styles; every paragraph and run in
//! `section0.xml` refers to these IDs.

use crate::rich_table_style::{CellFill, TableBorderEdge, TableBorderKind, TableCellBorders};
use crate::{
    RichBinding, RichBindingKind, RichColumns, RichLineSpacing, RichPageMargins, RichPaper,
    RichScript, XmlSanitizer,
};

pub(crate) const MAX_HWPX_BORDER_FILLS: usize = 4_096;

pub(crate) const NS_HEAD: &str = "http://www.owpml.org/owpml/2011/head";
pub(crate) const NS_CORE: &str = "http://www.owpml.org/owpml/2011/core";
pub(crate) const NS_SECTION: &str = "http://www.owpml.org/owpml/2011/section";
pub(crate) const NS_PARAGRAPH: &str = "http://www.owpml.org/owpml/2011/paragraph";

/// A4 portrait page in HWPUNIT (1/7200 inch) with 30 mm side, 20 mm top, and 15 mm bottom/header/footer margins.
const PAGE_WIDTH: u32 = 59_528;
const PAGE_HEIGHT: u32 = 84_188;
const MARGIN_SIDE: u32 = 8_504;
const MARGIN_TOP: u32 = 5_668;
const MARGIN_OTHER: u32 = 4_252;
/// Left margin (HWPUNIT) the writer gives list paragraphs; the reader treats it as the list marker.
pub(crate) const LIST_LEFT_MARGIN: u32 = 2_000;
/// Single-spacing-relative line spacing written when a paragraph sets none (percent).
pub(crate) const DEFAULT_LINE_PERCENT: u32 = 160;
/// The same as a line spacing, which a paragraph that sets none gets and the reader leaves unset.
pub(crate) const DEFAULT_LINE_SPACING: RichLineSpacing =
    RichLineSpacing::Multiple(DEFAULT_LINE_PERCENT as u16);
/// HWPUNIT is 1/100 point and a twip is 1/20 point, so one twip is five HWPUNIT.
pub(crate) const HWPUNIT_PER_TWIP: u32 = 5;
/// Default cell height for generated tables.
pub(crate) const CELL_HEIGHT: u32 = 1_000;

/// `hh:heading` type of a paragraph that is no list item.
pub(crate) const HEADING_NONE: &str = "NONE";
/// `hh:heading` type of an outline heading paragraph.
pub(crate) const HEADING_OUTLINE: &str = "OUTLINE";
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
pub(crate) const DEFAULT_FONT_FACE: &str = "함초롬바탕";
/// Body text height (1/100 pt) of the default character shape: 10 pt, the HWP default.
pub(crate) const DEFAULT_CHAR_HEIGHT: u32 = 1000;

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

fn push_border_edge_xml(xml: &mut String, side: &str, edge: TableBorderEdge) {
    let (line_type, width, colored) = match edge.kind() {
        TableBorderKind::Unspecified => ("SOLID", "0.12 mm", false),
        TableBorderKind::None => ("NONE", "0.12 mm", false),
        TableBorderKind::Thin => ("SOLID", "0.12 mm", true),
        TableBorderKind::Thick => ("SOLID", "0.4 mm", true),
        TableBorderKind::Dashed => ("DASH", "0.15 mm", true),
        TableBorderKind::Dotted => ("DOT", "0.15 mm", true),
        TableBorderKind::Double => ("DOUBLE_SLIM", "0.4 mm", true),
    };
    xml.push_str("<hh:");
    xml.push_str(side);
    xml.push_str(" type=\"");
    xml.push_str(line_type);
    xml.push_str("\" width=\"");
    xml.push_str(width);
    xml.push_str("\" color=\"#");
    if colored {
        edge.write_hex(xml);
    } else {
        xml.push_str("000000");
    }
    xml.push_str("\"/>");
}

fn custom_border_fill_xml(id: usize, fill: CellFill, borders: TableCellBorders) -> String {
    let mut xml = format!(
        "<hh:borderFill id=\"{id}\" threeD=\"0\" shadow=\"0\" centerLine=\"NONE\" breakCellSeparateLine=\"0\"><hh:slash type=\"NONE\" Crooked=\"0\" isCounter=\"0\"/><hh:backSlash type=\"NONE\" Crooked=\"0\" isCounter=\"0\"/>"
    );
    push_border_edge_xml(&mut xml, "leftBorder", borders.left);
    push_border_edge_xml(&mut xml, "rightBorder", borders.right);
    push_border_edge_xml(&mut xml, "topBorder", borders.top);
    push_border_edge_xml(&mut xml, "bottomBorder", borders.bottom);
    xml.push_str("<hh:diagonal type=\"SOLID\" width=\"0.1 mm\" color=\"#000000\"/>");
    if !fill.is_none() {
        xml.push_str("<hc:fillBrush><hc:winBrush faceColor=\"#");
        fill.write_hex(&mut xml);
        xml.push_str("\" hatchColor=\"#999999\" alpha=\"0\"/></hc:fillBrush>");
    }
    xml.push_str("</hh:borderFill>");
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
    /// The strikethrough is the double line (`DOUBLE_SLIM`); only meaningful with `strike`.
    pub strike_double: bool,
    pub height: u32,
    pub font: usize,
    /// Font table index of the East Asian (Hangul, Hanja, Japanese) slots; the same as `font` for one font.
    pub font_east: usize,
    pub text_color: Option<u32>,
    pub shade_color: Option<u32>,
    pub script: RichScript,
    /// Character spacing (`hh:spacing`) in percent of the font size.
    pub spacing_percent: i8,
    /// Character width ratio (`hh:ratio`) in percent, 10 to 200.
    pub ratio_percent: u16,
    /// Relative position (`hh:offset`) in percent of the font size, -100 to 100.
    pub offset_percent: i8,
    /// Pair kerning (`useKerning`).
    pub kerning: bool,
}

impl Default for HwpxCharShape {
    fn default() -> Self {
        Self {
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            strike_double: false,
            height: DEFAULT_CHAR_HEIGHT,
            font: 0,
            font_east: 0,
            text_color: None,
            shade_color: None,
            script: RichScript::Normal,
            spacing_percent: 0,
            ratio_percent: 100,
            offset_percent: 0,
            kerning: false,
        }
    }
}

/// Paragraph shape (`hh:paraPr`): OWPML horizontal alignment, list heading type/id/level, and left margin (HWPUNIT).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct HwpxParaShape {
    pub align: Option<&'static str>,
    pub heading_type: &'static str,
    pub heading_id_ref: u32,
    pub heading_level: u8,
    pub checked: bool,
    pub left_margin: u32,
    pub right_margin: u32,
    /// First-line indent (`hc:intent`), HWPUNIT; negative for a hanging indent.
    pub intent: i32,
    /// Line spacing (`hh:lineSpacing`).
    pub line_spacing: RichLineSpacing,
}

impl Default for HwpxParaShape {
    fn default() -> Self {
        Self {
            align: None,
            heading_type: HEADING_NONE,
            heading_id_ref: 0,
            heading_level: 0,
            checked: false,
            left_margin: 0,
            right_margin: 0,
            intent: 0,
            line_spacing: DEFAULT_LINE_SPACING,
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
        "<hh:charPr id=\"{id}\" height=\"{}\" textColor=\"#{:06X}\" shadeColor=\"{shade}\" useFontSpace=\"0\" useKerning=\"{}\" symMark=\"NONE\" borderFillIDRef=\"1\">",
        shape.height,
        shape.text_color.unwrap_or(0),
        u8::from(shape.kerning)
    );
    // The Hangul, Hanja and Japanese slots take the East Asian face, the others the Latin one (the same for one font).
    let (latin, east) = (shape.font.to_string(), shape.font_east.to_string());
    xml.push_str(&format!(
        "<hh:fontRef hangul=\"{east}\" latin=\"{latin}\" hanja=\"{east}\" japanese=\"{east}\" other=\"{latin}\" symbol=\"{latin}\" user=\"{latin}\"/>"
    ));
    xml.push_str(&format!(
        "<hh:ratio {}/>",
        per_lang_attrs(&shape.ratio_percent.to_string())
    ));
    xml.push_str(&format!(
        "<hh:spacing {}/>",
        per_lang_attrs(&shape.spacing_percent.to_string())
    ));
    xml.push_str(&format!("<hh:relSz {}/>", per_lang_attrs("100")));
    xml.push_str(&format!(
        "<hh:offset {}/>",
        per_lang_attrs(&shape.offset_percent.to_string())
    ));
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
        match (shape.strike, shape.strike_double) {
            (false, _) => "NONE",
            (true, false) => "SOLID",
            (true, true) => "DOUBLE_SLIM",
        }
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
        None if id == 0 => "<hh:align horizontal=\"JUSTIFY\" vertical=\"BASELINE\"/>".to_string(),
        None => "<hh:align vertical=\"BASELINE\"/>".to_string(),
    };
    let hwpunits = |twips: u16| u32::from(twips) * HWPUNIT_PER_TWIP;
    let (line_type, line) = match shape.line_spacing {
        RichLineSpacing::Multiple(percent) => ("PERCENT", u32::from(percent)),
        RichLineSpacing::AtLeast(twips) => ("AT_LEAST", hwpunits(twips)),
        RichLineSpacing::Exact(twips) => ("FIXED", hwpunits(twips)),
        RichLineSpacing::Between(twips) => ("BETWEEN_LINES", hwpunits(twips)),
    };
    let tab_pr = u8::from(shape.heading_type == HEADING_OUTLINE);
    let heading_ref = if shape.heading_type == HEADING_NONE || shape.heading_type == HEADING_OUTLINE
    {
        0
    } else {
        shape.heading_id_ref.max(1)
    };
    format!(
        concat!(
            "<hh:paraPr id=\"{id}\" tabPrIDRef=\"{tab_pr}\" condense=\"0\" fontLineHeight=\"0\" snapToGrid=\"1\" suppressLineNumbers=\"0\" checked=\"{checked}\">",
            "{align}",
            "<hh:heading type=\"{heading_type}\" idRef=\"{heading_ref}\" level=\"{heading_level}\"/>",
            "<hh:breakSetting breakLatinWord=\"KEEP_WORD\" breakNonLatinWord=\"KEEP_WORD\" widowOrphan=\"0\" keepWithNext=\"0\" keepLines=\"0\" pageBreakBefore=\"0\" lineWrap=\"BREAK\"/>",
            "<hh:autoSpacing eAsianEng=\"0\" eAsianNum=\"0\"/>",
            "<hh:margin><hc:intent value=\"{intent}\" unit=\"HWPUNIT\"/><hc:left value=\"{left}\" unit=\"HWPUNIT\"/>",
            "<hc:right value=\"{right}\" unit=\"HWPUNIT\"/><hc:prev value=\"0\" unit=\"HWPUNIT\"/><hc:next value=\"0\" unit=\"HWPUNIT\"/></hh:margin>",
            "<hh:lineSpacing type=\"{line_type}\" value=\"{line}\" unit=\"HWPUNIT\"/>",
            "<hh:border borderFillIDRef=\"1\" offsetLeft=\"0\" offsetRight=\"0\" offsetTop=\"0\" offsetBottom=\"0\" connect=\"0\" ignoreMargin=\"0\"/>",
            "</hh:paraPr>"
        ),
        id = id,
        tab_pr = tab_pr,
        checked = u8::from(shape.checked),
        align = align_xml,
        heading_type = shape.heading_type,
        heading_ref = heading_ref,
        heading_level = shape.heading_level,
        left = shape.left_margin,
        right = shape.right_margin,
        intent = shape.intent,
        line_type = line_type,
        line = line
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
    header_xml_with_lists(char_shapes, para_shapes, extra_styles, extra_fonts, None)
}

/// [`header_xml_with`] with explicit `hh:numberings` XML when `numberings_xml` is `Some`.
#[allow(dead_code)]
pub(crate) fn header_xml_with_lists(
    char_shapes: &[HwpxCharShape],
    para_shapes: &[HwpxParaShape],
    extra_styles: &[HwpxStyle],
    extra_fonts: &[String],
    numberings_xml: Option<&str>,
) -> String {
    header_xml_with_all(
        char_shapes,
        para_shapes,
        extra_styles,
        extra_fonts,
        numberings_xml,
        &[],
    )
}

pub(crate) fn header_xml_with_all(
    char_shapes: &[HwpxCharShape],
    para_shapes: &[HwpxParaShape],
    extra_styles: &[HwpxStyle],
    extra_fonts: &[String],
    numberings_xml: Option<&str>,
    extra_border_fills: &[(CellFill, TableCellBorders)],
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

    xml.push_str(&format!(
        "<hh:borderFills itemCnt=\"{}\">",
        2 + extra_border_fills.len()
    ));
    xml.push_str(&border_fill(1, "NONE"));
    xml.push_str(&border_fill(2, "SOLID"));
    for (idx, &(fill, borders)) in extra_border_fills.iter().enumerate() {
        xml.push_str(&custom_border_fill_xml(idx + 3, fill, borders));
    }
    xml.push_str("</hh:borderFills>");

    xml.push_str(&format!(
        "<hh:charProperties itemCnt=\"{}\">",
        char_shapes.len()
    ));
    for (id, shape) in char_shapes.iter().enumerate() {
        xml.push_str(&char_pr_xml(id, shape));
    }
    xml.push_str("</hh:charProperties>");
    xml.push_str("<hh:tabProperties itemCnt=\"3\"><hh:tabPr id=\"0\" autoTabLeft=\"0\" autoTabRight=\"0\"/><hh:tabPr id=\"1\" autoTabLeft=\"1\" autoTabRight=\"0\"/><hh:tabPr id=\"2\" autoTabLeft=\"0\" autoTabRight=\"1\"/></hh:tabProperties>");
    let max_bullet_id = para_shapes
        .iter()
        .filter(|shape| shape.heading_type == HEADING_BULLET)
        .map(|shape| shape.heading_id_ref.max(1))
        .max()
        .unwrap_or(0);
    match numberings_xml {
        Some(custom) if !custom.is_empty() => xml.push_str(custom),
        _ => xml.push_str(NUMBERING_XML),
    }
    if max_bullet_id > 0 {
        if max_bullet_id == 1 {
            xml.push_str(BULLET_XML);
        } else {
            xml.push_str(&crate::rich_list_writers::hwpx_bullets_xml(max_bullet_id));
        }
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

/// The page as `hp:pagePr` keeps it: the paper in HWPUNIT stored upright (the reader of the reference editor
/// swaps the sides when the page is landscape, `rhwp` `src/parser/hwpx/section.rs:450-458`: `NARROWLY` is
/// landscape, `WIDELY` portrait) and the side margins.
fn page_box(
    margins: Option<&RichPageMargins>,
    paper: Option<&RichPaper>,
) -> (u32, u32, bool, u32, u32) {
    let (width, height, landscape) = paper.map_or((PAGE_WIDTH, PAGE_HEIGHT, false), |p| {
        let (short, long) = (p.width.min(p.height), p.width.max(p.height));
        (
            short * HWPUNIT_PER_TWIP,
            long * HWPUNIT_PER_TWIP,
            p.width > p.height,
        )
    });
    let (left, right) = match margins {
        Some(m) if *m != RichPageMargins::HWP_DEFAULT => {
            (m.left * HWPUNIT_PER_TWIP, m.right * HWPUNIT_PER_TWIP)
        }
        _ => (MARGIN_SIDE, MARGIN_SIDE),
    };
    (width, height, landscape, left, right)
}

/// `gutterType` of `hp:pagePr` for a binding (`rhwp` `src/parser/hwpx/section.rs`: one-sided, facing pages, top).
pub(crate) fn gutter_type(kind: RichBindingKind) -> &'static str {
    match kind {
        RichBindingKind::Single => "LEFT_ONLY",
        RichBindingKind::Duplex => "LEFT_RIGHT",
        RichBindingKind::Top => "TOP_BOTTOM",
    }
}

/// Usable text width of the page in HWPUNIT (the width of a table that spans the page).
pub(crate) fn text_width(
    margins: Option<&RichPageMargins>,
    paper: Option<&RichPaper>,
    binding: RichBinding,
) -> u32 {
    let (width, height, landscape, left, right) = page_box(margins, paper);
    let shown = if landscape { height } else { width };
    let gutter = if binding.kind == RichBindingKind::Top {
        0
    } else {
        binding.gutter * HWPUNIT_PER_TWIP
    };
    shown
        .saturating_sub(left)
        .saturating_sub(right)
        .saturating_sub(gutter)
        .max(1)
}

/// Column definition control (`hp:colPr`) inside a paragraph run.
pub(crate) fn hwpx_col_ctrl_xml(columns: Option<RichColumns>) -> String {
    let (count, same_sz, gap_hwp) = match columns {
        Some(c) if c.count > 1 => (
            c.count.min(16),
            u8::from(c.equal),
            u32::from(c.gap_twips) * HWPUNIT_PER_TWIP,
        ),
        _ => (1, 1, 0),
    };
    format!(
        "<hp:ctrl><hp:colPr id=\"\" type=\"NEWSPAPER\" layout=\"LEFT\" colCount=\"{count}\" sameSz=\"{same_sz}\" sameGap=\"{gap_hwp}\"/></hp:ctrl>"
    )
}

/// Section properties (`hp:secPr`) plus the column control; must sit in the first run of the
/// first paragraph of each section.
pub(crate) fn section_properties_xml(
    margins: Option<&RichPageMargins>,
    paper: Option<&RichPaper>,
    binding: RichBinding,
    columns: Option<RichColumns>,
) -> String {
    let (header, footer, top, bottom) = match margins {
        Some(m) if *m != RichPageMargins::HWP_DEFAULT => {
            crate::rich_page_frame::hwpx_margin_values(m)
        }
        _ => (MARGIN_OTHER, MARGIN_OTHER, MARGIN_TOP, MARGIN_OTHER),
    };
    let (width, height, landscape, left, right) = page_box(margins, paper);
    let orientation = if landscape { "NARROWLY" } else { "WIDELY" };
    let gutter = binding.gutter * HWPUNIT_PER_TWIP;
    let gutter_type = gutter_type(binding.kind);
    let col_ctrl = hwpx_col_ctrl_xml(columns);
    format!(
        concat!(
            "<hp:secPr id=\"\" textDirection=\"HORIZONTAL\" spaceColumns=\"1134\" tabStop=\"8000\" tabStopVal=\"4000\" tabStopUnit=\"HWPUNIT\" outlineShapeIDRef=\"1\" memoShapeIDRef=\"0\" textVerticalWidthHead=\"0\" masterPageCnt=\"0\">",
            "<hp:grid lineGrid=\"0\" charGrid=\"0\" wonggojiFormat=\"0\"/>",
            "<hp:startNum pageStartsOn=\"BOTH\" page=\"0\" pic=\"0\" tbl=\"0\" equation=\"0\"/>",
            "<hp:visibility hideFirstHeader=\"0\" hideFirstFooter=\"0\" hideFirstMasterPage=\"0\" border=\"SHOW_ALL\" fill=\"SHOW_ALL\" hideFirstPageNum=\"0\" hideFirstEmptyLine=\"0\" showLineNumber=\"0\"/>",
            "<hp:lineNumberShape restartType=\"0\" countBy=\"0\" distance=\"0\" startNumber=\"0\"/>",
            "<hp:pagePr landscape=\"{orientation}\" width=\"{width}\" height=\"{height}\" gutterType=\"{gutter_type}\">",
            "<hp:margin header=\"{header}\" footer=\"{footer}\" gutter=\"{gutter}\" left=\"{left}\" right=\"{right}\" top=\"{top}\" bottom=\"{bottom}\"/>",
            "</hp:pagePr>",
            "<hp:footNotePr><hp:autoNumFormat type=\"DIGIT\" userChar=\"\" prefixChar=\"\" suffixChar=\")\" supscript=\"0\"/><hp:noteLine length=\"-1\" type=\"SOLID\" width=\"0.12 mm\" color=\"#000000\"/><hp:noteSpacing betweenNotes=\"283\" belowLine=\"567\" aboveLine=\"850\"/><hp:numbering type=\"CONTINUOUS\" newNum=\"1\"/><hp:placement place=\"EACH_COLUMN\" beneathText=\"0\"/></hp:footNotePr>",
            "<hp:endNotePr><hp:autoNumFormat type=\"DIGIT\" userChar=\"\" prefixChar=\"\" suffixChar=\")\" supscript=\"0\"/><hp:noteLine length=\"14692344\" type=\"SOLID\" width=\"0.12 mm\" color=\"#000000\"/><hp:noteSpacing betweenNotes=\"0\" belowLine=\"567\" aboveLine=\"850\"/><hp:numbering type=\"CONTINUOUS\" newNum=\"1\"/><hp:placement place=\"END_OF_DOCUMENT\" beneathText=\"0\"/></hp:endNotePr>",
            "<hp:pageBorderFill type=\"BOTH\" borderFillIDRef=\"1\" textBorder=\"PAPER\" headerInside=\"0\" footerInside=\"0\" fillArea=\"PAPER\"><hp:offset left=\"1417\" right=\"1417\" top=\"1417\" bottom=\"1417\"/></hp:pageBorderFill>",
            "<hp:pageBorderFill type=\"EVEN\" borderFillIDRef=\"1\" textBorder=\"PAPER\" headerInside=\"0\" footerInside=\"0\" fillArea=\"PAPER\"><hp:offset left=\"1417\" right=\"1417\" top=\"1417\" bottom=\"1417\"/></hp:pageBorderFill>",
            "<hp:pageBorderFill type=\"ODD\" borderFillIDRef=\"1\" textBorder=\"PAPER\" headerInside=\"0\" footerInside=\"0\" fillArea=\"PAPER\"><hp:offset left=\"1417\" right=\"1417\" top=\"1417\" bottom=\"1417\"/></hp:pageBorderFill>",
            "</hp:secPr>{col_ctrl}"
        ),
        orientation = orientation,
        width = width,
        height = height,
        gutter_type = gutter_type,
        gutter = gutter,
        header = header,
        footer = footer,
        left = left,
        right = right,
        top = top,
        bottom = bottom,
        col_ctrl = col_ctrl
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
