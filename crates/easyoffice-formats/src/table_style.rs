//! The look of a table (PD-231): the table styles of `styles.xml` (`tableStyles`, ECMA-376 18.8.40
//! `tableStyle`, 18.8.41 `tableStyleElement`) with the differential formats they name (18.8.14 `dxf`),
//! and how a custom style is written back.
//!
//! A table names its style (`SheetTable::style`); the built-in styles (`TableStyleLight1` ..
//! `TableStyleDark11`) are defined by the page, a style that is not built in carries its definition in
//! [`SheetTable::style_def`] so the page draws it and a save writes it again.
//!
//! 참조 구현: Apache POI `poi/src/main/java/org/apache/poi/ss/usermodel/TableStyleType.java` (the
//! element types and their order of application) and `poi-ooxml/src/main/java/org/apache/poi/xssf/usermodel/XSSFTableStyle.java`
//! lines 41-95 (an element names a differential format by `dxfId` and has a stripe `size`), Apache-2.0,
//! Copyright The Apache Software Foundation, commit 47dcff769bb395034dddfd59c4609659b6dbc6a4. Source
//! and license text: `THIRD_PARTY_NOTICES.md`.
//!
//! Modifications: a format keeps what a table can show (fill, text colour, bold, italic, the six
//! border edges with a line of four weights) and drops numbers, alignment and the rest; every colour is
//! resolved to RGB when the file is read; a workbook defines at most [`MAX_TABLE_STYLES`] styles and a
//! style names each element once; the stripe size is 1..=[`MAX_STRIPE`].

use std::collections::HashMap;
use std::fmt::Write;

use crate::rtf_xml_helpers::{
    extract_attr_from_fragment, extract_tag_bodies, extract_tags_or_self_closing_with_attrs,
    extract_tags_with_attrs,
};
use crate::sheet_styling::xlsx_color::{ColorScheme, XlsxColor};
use crate::XmlSanitizer;

/// Custom styles one workbook may define (one per table at most).
pub const MAX_TABLE_STYLES: usize = 1_000;
/// Rows or columns in one stripe (the spreadsheet program offers 1..=9).
pub const MAX_STRIPE: u8 = 9;

/// The parts of a table a style formats, in the order they are applied (a later one wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StyleElement {
    Whole,
    ColumnStripe1,
    ColumnStripe2,
    RowStripe1,
    RowStripe2,
    LastColumn,
    FirstColumn,
    Header,
    Total,
    FirstHeader,
    LastHeader,
    FirstTotal,
    LastTotal,
}

impl StyleElement {
    pub const ALL: [Self; 13] = [
        Self::Whole,
        Self::ColumnStripe1,
        Self::ColumnStripe2,
        Self::RowStripe1,
        Self::RowStripe2,
        Self::LastColumn,
        Self::FirstColumn,
        Self::Header,
        Self::Total,
        Self::FirstHeader,
        Self::LastHeader,
        Self::FirstTotal,
        Self::LastTotal,
    ];

    /// The `type` of `tableStyleElement`.
    pub const fn xlsx(self) -> &'static str {
        match self {
            Self::Whole => "wholeTable",
            Self::ColumnStripe1 => "firstColumnStripe",
            Self::ColumnStripe2 => "secondColumnStripe",
            Self::RowStripe1 => "firstRowStripe",
            Self::RowStripe2 => "secondRowStripe",
            Self::LastColumn => "lastColumn",
            Self::FirstColumn => "firstColumn",
            Self::Header => "headerRow",
            Self::Total => "totalRow",
            Self::FirstHeader => "firstHeaderCell",
            Self::LastHeader => "lastHeaderCell",
            Self::FirstTotal => "firstTotalCell",
            Self::LastTotal => "lastTotalCell",
        }
    }

    /// The key of the page's JSON.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Whole => "w",
            Self::ColumnStripe1 => "c1",
            Self::ColumnStripe2 => "c2",
            Self::RowStripe1 => "r1",
            Self::RowStripe2 => "r2",
            Self::LastColumn => "lc",
            Self::FirstColumn => "fc",
            Self::Header => "h",
            Self::Total => "t",
            Self::FirstHeader => "fh",
            Self::LastHeader => "lh",
            Self::FirstTotal => "ft",
            Self::LastTotal => "lt",
        }
    }

    pub fn from_xlsx(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.xlsx() == name)
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.key() == key)
    }
}

/// The weight of a border line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TableLine {
    Thin,
    Medium,
    Thick,
    Double,
}

impl TableLine {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Thin => "thin",
            Self::Medium => "medium",
            Self::Thick => "thick",
            Self::Double => "double",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "thin" => Some(Self::Thin),
            "medium" => Some(Self::Medium),
            "thick" => Some(Self::Thick),
            "double" => Some(Self::Double),
            _ => None,
        }
    }

    /// A line style of `border` (ECMA-376 18.18.3): the four weights are kept, the lighter styles
    /// (`hair`, dotted, dashed) are drawn thin and `none` is no line.
    fn from_xlsx(style: &str) -> Option<Self> {
        match style {
            "" | "none" => None,
            "medium" | "mediumDashed" | "mediumDashDot" | "mediumDashDotDot" | "slantDashDot" => {
                Some(Self::Medium)
            }
            "thick" => Some(Self::Thick),
            "double" => Some(Self::Double),
            _ => Some(Self::Thin),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TableEdge {
    pub line: TableLine,
    pub color: u32,
}

/// The edges of a format, in this order: top, bottom, left, right, horizontal (between rows) and
/// vertical (between columns).
pub const EDGE_KEYS: [&str; 6] = ["t", "b", "l", "r", "h", "v"];
const EDGE_TAGS: [&str; 6] = ["top", "bottom", "left", "right", "horizontal", "vertical"];

/// What one element of a style sets; a part it leaves out is the one an element below it sets.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct StyleFormat {
    pub fill: Option<u32>,
    pub color: Option<u32>,
    pub bold: bool,
    pub italic: bool,
    pub edges: [Option<TableEdge>; 6],
    /// Rows or columns in a stripe (1 when not given); only the stripe elements use it.
    pub stripe: u8,
}

impl StyleFormat {
    pub fn is_empty(&self) -> bool {
        self.fill.is_none()
            && self.color.is_none()
            && !self.bold
            && !self.italic
            && self.edges.iter().all(Option::is_none)
    }
}

/// A custom table style: each element it formats, once.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct TableStyleDef {
    pub elements: Vec<(StyleElement, StyleFormat)>,
}

impl TableStyleDef {
    pub fn within_bounds(&self) -> bool {
        self.elements.len() <= StyleElement::ALL.len()
            && self.elements.iter().enumerate().all(|(i, (e, f))| {
                f.stripe <= MAX_STRIPE && self.elements[..i].iter().all(|(o, _)| o != e)
            })
    }
}

/// Whether `name` is one of the 60 built-in table styles (`TableStyleLight1` .. `TableStyleLight21`,
/// `TableStyleMedium1` .. `28`, `TableStyleDark1` .. `11`), which the page defines.
pub fn is_builtin_table_style(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("TableStyle") else {
        return false;
    };
    let (count, digits) = if let Some(d) = rest.strip_prefix("Light") {
        (21, d)
    } else if let Some(d) = rest.strip_prefix("Medium") {
        (28, d)
    } else if let Some(d) = rest.strip_prefix("Dark") {
        (11, d)
    } else {
        return false;
    };
    !digits.starts_with('0')
        && digits.len() <= 2
        && digits.bytes().all(|b| b.is_ascii_digit())
        && digits
            .parse::<u32>()
            .is_ok_and(|n| (1..=count).contains(&n))
}

// ---- reading ---------------------------------------------------------------------------------------------

fn color_of(colors: &ColorScheme, fragment: &str, tags: &[&str]) -> Option<u32> {
    tags.iter().find_map(|tag| {
        let found = extract_tags_or_self_closing_with_attrs(fragment, tag);
        match found.first().map(|(attrs, _)| colors.resolve(attrs)) {
            Some(XlsxColor::Rgb(rgb)) => Some(rgb),
            _ => None,
        }
    })
}

fn switched_on(font: &str, tag: &str) -> bool {
    extract_tags_or_self_closing_with_attrs(font, tag)
        .first()
        .is_some_and(|(attrs, _)| {
            !matches!(
                extract_attr_from_fragment(attrs, "val"),
                Some("0" | "false")
            )
        })
}

/// The table-relevant parts of one `dxf` body.
fn read_format(dxf: &str, colors: &ColorScheme) -> StyleFormat {
    let mut format = StyleFormat {
        stripe: 1,
        ..StyleFormat::default()
    };
    if let Some(font) = extract_tag_bodies(dxf, "font").into_iter().next() {
        format.bold = switched_on(font, "b");
        format.italic = switched_on(font, "i");
        format.color = color_of(colors, font, &["color"]);
    }
    if let Some(fill) = extract_tag_bodies(dxf, "fill").into_iter().next() {
        if !fill.contains("patternType=\"none\"") {
            format.fill = color_of(colors, fill, &["bgColor", "fgColor"]);
        }
    }
    if let Some(border) = extract_tag_bodies(dxf, "border").into_iter().next() {
        for (slot, tag) in format.edges.iter_mut().zip(EDGE_TAGS) {
            let found = extract_tags_or_self_closing_with_attrs(border, tag);
            let Some((attrs, body)) = found.first() else {
                continue;
            };
            let line = extract_attr_from_fragment(attrs, "style").and_then(TableLine::from_xlsx);
            *slot = line.map(|line| TableEdge {
                line,
                color: color_of(colors, body, &["color"]).unwrap_or(0),
            });
        }
    }
    format
}

/// The custom table styles of `styles_xml` by name: those that are not built in, in file order, up to
/// [`MAX_TABLE_STYLES`]; an element that names a missing `dxf`, repeats an element or has a stripe over
/// [`MAX_STRIPE`] is left out.
pub(crate) fn parse_table_styles(
    styles_xml: &str,
    dxfs: &[&str],
    colors: &ColorScheme,
) -> HashMap<String, TableStyleDef> {
    let mut out = HashMap::new();
    let Some(block) = extract_tag_bodies(styles_xml, "tableStyles")
        .into_iter()
        .next()
    else {
        return out;
    };
    // A format is read once however many elements name its `dxf`.
    let mut formats: HashMap<usize, StyleFormat> = HashMap::new();
    for (attrs, body) in extract_tags_with_attrs(block, "tableStyle") {
        if out.len() >= MAX_TABLE_STYLES {
            break;
        }
        let Some(name) = extract_attr_from_fragment(attrs, "name").map(XmlSanitizer::unescape)
        else {
            continue;
        };
        if is_builtin_table_style(&name) || name.is_empty() || out.contains_key(&name) {
            continue;
        }
        let mut def = TableStyleDef::default();
        for (attrs, _) in extract_tags_or_self_closing_with_attrs(body, "tableStyleElement")
            .into_iter()
            .take(StyleElement::ALL.len() * 2)
        {
            let Some(element) =
                extract_attr_from_fragment(attrs, "type").and_then(StyleElement::from_xlsx)
            else {
                continue;
            };
            let Some(id) = extract_attr_from_fragment(attrs, "dxfId")
                .and_then(|id| id.trim().parse::<usize>().ok())
                .filter(|id| *id < dxfs.len())
            else {
                continue;
            };
            if def.elements.iter().any(|(e, _)| *e == element) {
                continue;
            }
            let mut format = formats
                .entry(id)
                .or_insert_with(|| read_format(dxfs[id], colors))
                .clone();
            if let Some(size) =
                extract_attr_from_fragment(attrs, "size").and_then(|s| s.trim().parse::<u8>().ok())
            {
                format.stripe = size.clamp(1, MAX_STRIPE);
            }
            if !format.is_empty() {
                def.elements.push((element, format));
            }
        }
        out.insert(name, def);
    }
    out
}

// ---- writing ---------------------------------------------------------------------------------------------

fn edge_xml(xml: &mut String, tag: &str, edge: Option<TableEdge>) {
    if let Some(edge) = edge {
        let _ = write!(
            xml,
            "<{tag} style=\"{}\"><color rgb=\"FF{:06X}\"/></{tag}>",
            edge.line.name(),
            edge.color
        );
    }
}

/// The `<dxf>` that stands for one element.
fn dxf_xml(format: &StyleFormat) -> String {
    let mut xml = String::from("<dxf>");
    if format.bold || format.italic || format.color.is_some() {
        xml.push_str("<font>");
        if format.bold {
            xml.push_str("<b/>");
        }
        if format.italic {
            xml.push_str("<i/>");
        }
        if let Some(color) = format.color {
            let _ = write!(xml, "<color rgb=\"FF{color:06X}\"/>");
        }
        xml.push_str("</font>");
    }
    if let Some(fill) = format.fill {
        let _ = write!(
            xml,
            "<fill><patternFill patternType=\"solid\"><fgColor rgb=\"FF{fill:06X}\"/><bgColor rgb=\"FF{fill:06X}\"/></patternFill></fill>"
        );
    }
    if format.edges.iter().any(Option::is_some) {
        xml.push_str("<border>");
        // The order of `CT_Border`: left, right, top, bottom, vertical, horizontal.
        for (index, tag) in [
            (2, "left"),
            (3, "right"),
            (0, "top"),
            (1, "bottom"),
            (5, "vertical"),
            (4, "horizontal"),
        ] {
            edge_xml(&mut xml, tag, format.edges[index]);
        }
        xml.push_str("</border>");
    }
    xml.push_str("</dxf>");
    xml
}

/// The `tableStyles` element for `styles` (name and definition, each name once) and the `<dxf>` entries
/// its elements name, whose ids start at `first_dxf`.
pub(crate) fn table_styles_xml<'a>(
    styles: impl IntoIterator<Item = (&'a str, &'a TableStyleDef)>,
    first_dxf: usize,
) -> (String, Vec<String>) {
    let mut dxfs: Vec<String> = Vec::new();
    let mut body = String::new();
    let mut count = 0usize;
    for (name, def) in styles.into_iter().take(MAX_TABLE_STYLES) {
        count += 1;
        let _ = write!(
            body,
            "<tableStyle name=\"{}\" pivot=\"0\" count=\"{}\">",
            XmlSanitizer::escape(name),
            def.elements.len()
        );
        for (element, format) in &def.elements {
            let _ = write!(
                body,
                "<tableStyleElement type=\"{}\" dxfId=\"{}\"",
                element.xlsx(),
                first_dxf + dxfs.len()
            );
            if format.stripe > 1 {
                let _ = write!(body, " size=\"{}\"", format.stripe);
            }
            body.push_str("/>");
            dxfs.push(dxf_xml(format));
        }
        body.push_str("</tableStyle>");
    }
    if count == 0 {
        return (String::new(), dxfs);
    }
    let xml = format!(
        "<tableStyles count=\"{count}\" defaultTableStyle=\"TableStyleMedium2\" defaultPivotStyle=\"PivotStyleLight16\">{body}</tableStyles>"
    );
    (xml, dxfs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet_styling::xlsx_filter_dxf::dxf_bodies;

    #[test]
    fn built_in_names_are_the_sixty_the_spec_lists() {
        for ok in [
            "TableStyleLight1",
            "TableStyleLight21",
            "TableStyleMedium28",
            "TableStyleDark11",
            "TableStyleMedium2",
        ] {
            assert!(is_builtin_table_style(ok), "{ok}");
        }
        for bad in [
            "TableStyleLight22",
            "TableStyleMedium29",
            "TableStyleDark12",
            "TableStyleLight0",
            "TableStyleLight01",
            "TableStyle",
            "PivotStyleLight1",
            "Mine",
            "TableStyleMedium",
            "TableStyleLight-1",
        ] {
            assert!(!is_builtin_table_style(bad), "{bad}");
        }
        let all = (1..=21)
            .map(|n| format!("TableStyleLight{n}"))
            .chain((1..=28).map(|n| format!("TableStyleMedium{n}")))
            .chain((1..=11).map(|n| format!("TableStyleDark{n}")));
        assert_eq!(all.filter(|n| is_builtin_table_style(n)).count(), 60);
    }

    const STYLES: &str = concat!(
        "<styleSheet><dxfs count=\"4\">",
        "<dxf><font><b/><color rgb=\"FFFFFFFF\"/></font><fill><patternFill><bgColor rgb=\"FF1F4E78\"/></patternFill></fill></dxf>",
        "<dxf><fill><patternFill patternType=\"solid\"><fgColor theme=\"4\" tint=\"0.8\"/><bgColor theme=\"4\" tint=\"0.8\"/></patternFill></fill></dxf>",
        "<dxf><border><top style=\"double\"><color rgb=\"FF000000\"/></top><bottom style=\"hair\"><color indexed=\"10\"/></bottom><horizontal style=\"thin\"><color auto=\"1\"/></horizontal></border></dxf>",
        "<dxf><font><i/><b val=\"0\"/></font></dxf>",
        "</dxfs><tableStyles count=\"3\" defaultTableStyle=\"TableStyleMedium2\" defaultPivotStyle=\"PivotStyleLight16\">",
        "<tableStyle name=\"Mine\" pivot=\"0\" count=\"4\">",
        "<tableStyleElement type=\"headerRow\" dxfId=\"0\"/>",
        "<tableStyleElement type=\"firstRowStripe\" dxfId=\"1\" size=\"2\"/>",
        "<tableStyleElement type=\"totalRow\" dxfId=\"2\"/>",
        "<tableStyleElement type=\"headerRow\" dxfId=\"3\"/>",
        "</tableStyle>",
        "<tableStyle name=\"TableStyleMedium2\" pivot=\"0\" count=\"1\"><tableStyleElement type=\"headerRow\" dxfId=\"0\"/></tableStyle>",
        "<tableStyle name=\"Broken\" pivot=\"0\" count=\"2\"><tableStyleElement type=\"headerRow\" dxfId=\"99\"/><tableStyleElement type=\"nope\" dxfId=\"0\"/></tableStyle>",
        "</tableStyles></styleSheet>"
    );

    #[test]
    fn a_custom_style_reads_its_elements_with_colours_and_stripe_size() {
        let styles = parse_table_styles(STYLES, &dxf_bodies(STYLES), &ColorScheme::default());
        assert!(
            !styles.contains_key("TableStyleMedium2"),
            "a built-in name is the page's"
        );
        let mine = &styles["Mine"];
        let header = mine
            .elements
            .iter()
            .find(|(e, _)| *e == StyleElement::Header)
            .unwrap();
        assert_eq!(header.1.fill, Some(0x1F4E78));
        assert_eq!(header.1.color, Some(0xFFFFFF));
        assert!(header.1.bold, "the first element of a kind wins");
        assert_eq!(
            mine.elements
                .iter()
                .filter(|(e, _)| *e == StyleElement::Header)
                .count(),
            1
        );
        let stripe = mine
            .elements
            .iter()
            .find(|(e, _)| *e == StyleElement::RowStripe1)
            .unwrap();
        assert_eq!(stripe.1.stripe, 2);
        assert_eq!(
            stripe.1.fill,
            Some(crate::sheet_styling::xlsx_view::apply_tint(0x4472C4, 0.8))
        );
        let total = mine
            .elements
            .iter()
            .find(|(e, _)| *e == StyleElement::Total)
            .unwrap();
        assert_eq!(
            total.1.edges[0],
            Some(TableEdge {
                line: TableLine::Double,
                color: 0
            })
        );
        assert_eq!(
            total.1.edges[1],
            Some(TableEdge {
                line: TableLine::Thin,
                color: 0xFF0000
            }),
            "hair is drawn thin"
        );
        assert_eq!(
            total.1.edges[4],
            Some(TableEdge {
                line: TableLine::Thin,
                color: 0
            })
        );
        assert!(styles["Broken"].elements.is_empty());
        assert!(mine.within_bounds());
    }

    #[test]
    fn a_written_style_reads_back_the_same() {
        let styles = parse_table_styles(STYLES, &dxf_bodies(STYLES), &ColorScheme::default());
        let mine = &styles["Mine"];
        let (xml, dxfs) = table_styles_xml([("Mine", mine)], 5);
        assert_eq!(dxfs.len(), mine.elements.len());
        assert!(xml.contains("dxfId=\"5\"") && xml.contains("dxfId=\"7\""));
        // The same file shape: the dxfs placed from id 5 on.
        let padding = "<dxf/>".repeat(5); // empty entries still take ids
        let file = format!(
            "<styleSheet><dxfs>{padding}{}</dxfs>{xml}</styleSheet>",
            dxfs.concat()
        );
        let again = parse_table_styles(&file, &dxf_bodies(&file), &ColorScheme::default());
        let strip = |def: &TableStyleDef| {
            let mut e = def.elements.clone();
            e.sort_by_key(|(e, _)| StyleElement::ALL.iter().position(|a| a == e));
            e
        };
        assert_eq!(strip(&again["Mine"]), strip(mine));
        assert_eq!(table_styles_xml([], 0).0, "");
    }

    #[test]
    fn conditional_formats_and_table_styles_count_an_empty_dxf_the_same_way() {
        let xml = concat!(
            "<styleSheet><dxfs count=\"3\"><dxf/><dxf><font><b/></font><fill><patternFill><bgColor rgb=\"FFFF0000\"/></patternFill></fill></dxf><dxf/></dxfs>",
            "<tableStyles><tableStyle name=\"Red\"><tableStyleElement type=\"headerRow\" dxfId=\"1\"/><tableStyleElement type=\"totalRow\" dxfId=\"2\"/></tableStyle></tableStyles></styleSheet>"
        );
        let list = dxf_bodies(xml);
        assert_eq!(list.len(), 3);
        let rule = crate::sheet_styling::sheet_rules::xlsx_rules::parse_dxf_styles(&list);
        assert_eq!(
            rule[1].bg,
            Some(0xFF0000),
            "a conditional format that names entry 1"
        );
        let styles = parse_table_styles(xml, &list, &ColorScheme::default());
        let header = &styles["Red"].elements[0];
        assert_eq!(
            (header.0, header.1.fill),
            (StyleElement::Header, Some(0xFF0000)),
            "a table style that names entry 1"
        );
        assert_eq!(
            styles["Red"].elements.len(),
            1,
            "an empty entry sets nothing"
        );
    }

    #[test]
    fn one_big_dxf_named_by_every_element_of_a_thousand_styles_is_read_once() {
        let filler =
            "<border><top style=\"thin\"><color rgb=\"FF000000\"/></top></border>".repeat(16_000);
        let mut xml =
            format!("<styleSheet><dxfs><dxf><font><b/></font>{filler}</dxf></dxfs><tableStyles>");
        for n in 0..MAX_TABLE_STYLES {
            let _ = write!(xml, "<tableStyle name=\"S{n}\">");
            for e in StyleElement::ALL {
                let _ = write!(
                    xml,
                    "<tableStyleElement type=\"{}\" dxfId=\"0\"/>",
                    e.xlsx()
                );
            }
            xml.push_str("</tableStyle>");
        }
        xml.push_str("</tableStyles></styleSheet>");
        assert!(xml.len() > 1_000_000);
        let started = std::time::Instant::now();
        let styles = parse_table_styles(&xml, &dxf_bodies(&xml), &ColorScheme::default());
        assert_eq!(styles.len(), MAX_TABLE_STYLES);
        assert!(styles["S999"].elements.iter().all(|(_, f)| f.bold));
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_flood_of_styles_is_cut_at_the_bound() {
        let mut xml = String::from("<styleSheet><dxfs><dxf><fill><patternFill><bgColor rgb=\"FF010203\"/></patternFill></fill></dxf></dxfs><tableStyles>");
        for n in 0..(MAX_TABLE_STYLES + 500) {
            let _ = write!(xml, "<tableStyle name=\"S{n}\"><tableStyleElement type=\"wholeTable\" dxfId=\"0\"/></tableStyle>");
        }
        xml.push_str("</tableStyles></styleSheet>");
        assert_eq!(
            parse_table_styles(&xml, &dxf_bodies(&xml), &ColorScheme::default()).len(),
            MAX_TABLE_STYLES
        );
    }
}
