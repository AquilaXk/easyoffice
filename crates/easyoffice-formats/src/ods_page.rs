//! The page setup of a sheet in an ODS file (PD-213): the page layout and master page (`styles.xml`) that a
//! table style names with `style:master-page-name`, and the print ranges (`table:print-ranges` of the table).
//!
//! The format side follows OASIS ODF 1.2: 16.5 `style:page-layout`, 17.2 `style:page-layout-properties`
//! (`fo:page-width`, `fo:page-height`, `style:print-orientation`, `fo:margin-*`, `style:table-centering`,
//! `style:print`, `style:scale-to`, `style:scale-to-X`, `style:scale-to-Y`) and 9.1.1 `table:print-ranges`. The
//! model is `sheet_page.rs`.
//!
//! Modifications: the distances of the header and footer (which an ODF page layout holds only for a header that
//! exists) are kept as the foreign attributes `eo:header-distance` and `eo:footer-distance` of the layout. The
//! rows and columns repeated on every page (`table:table-header-rows`, `table:table-header-columns`) are read
//! wherever they are and written when they start at the first row or column.

use std::collections::HashMap;
use std::fmt::Write;

use crate::rtf_xml_helpers::{extract_attr_from_fragment, extract_balanced_elements};
use crate::sheet_page::{
    PageMargins, PageScale, SheetPage, MAX_FIT_PAGES, MAX_MARGIN_MILS, MAX_PRINT_RANGES,
    MAX_TITLE_COLS, MAX_TITLE_ROWS, PAPER_CHOICES,
};
use crate::sheet_styling::ods_numfmt::EO_NS;
use crate::{CellBox, SheetStyling, XmlSanitizer};

const MM_PER_INCH: f64 = 25.4;
/// Two papers are the same when each side differs by less than this, in millimetres.
const PAPER_TOLERANCE_MM: f64 = 0.6;
/// Most master pages a file may name (one per distinct layout); more are not read.
const MAX_LAYOUTS: usize = 4_096;

/// The page of a sheet without the print area and the repeated lines: what a page layout holds.
fn layout_of(page: &SheetPage) -> SheetPage {
    SheetPage {
        print_area: Vec::new(),
        title_rows: None,
        title_cols: None,
        ..page.clone()
    }
}

/// The distinct page layouts of a workbook in first-use order; a sheet with the default layout names none.
pub(crate) fn layouts(stylings: &[SheetStyling]) -> Vec<SheetPage> {
    let mut out: Vec<SheetPage> = Vec::new();
    for st in stylings {
        let layout = layout_of(&st.page);
        if !layout.is_default() && !out.contains(&layout) {
            out.push(layout);
        }
    }
    out
}

/// The number (1-based) of the master page a sheet uses, 0 for none.
pub(crate) fn master_of(layouts: &[SheetPage], styling: &SheetStyling) -> u16 {
    let layout = layout_of(&styling.page);
    layouts
        .iter()
        .position(|l| *l == layout)
        .and_then(|i| i.checked_add(1))
        .and_then(|n| u16::try_from(n).ok())
        .unwrap_or(0)
}

fn mm(mils: u32) -> String {
    let text = format!("{:.2}", f64::from(mils) * MM_PER_INCH / 1000.0);
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn mils(text: &str) -> Option<u32> {
    let value = text.trim();
    let (number, unit) = value.split_at(
        value
            .find(|c: char| c.is_ascii_alphabetic())
            .unwrap_or(value.len()),
    );
    let n = number
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && *n >= 0.0)?;
    let inches = match unit.trim() {
        "mm" => n / MM_PER_INCH,
        "cm" => n * 10.0 / MM_PER_INCH,
        "in" => n,
        "pt" => n / 72.0,
        _ => return None,
    };
    let m = (inches * 1000.0).round();
    (m <= f64::from(MAX_MARGIN_MILS)).then_some(m as u32)
}

/// `styles.xml` with one page layout (`Mpm<N>`) and master page (`Mp<N>`) per layout and, when a cell holds a
/// hyperlink, the named text styles its `text:a` uses; `None` without either.
pub(crate) fn styles_xml(layouts: &[SheetPage], links: bool) -> Option<String> {
    if layouts.is_empty() && !links {
        return None;
    }
    let mut automatic = String::new();
    let mut masters = String::new();
    for (i, page) in layouts.iter().enumerate() {
        let n = i + 1;
        let (w, h) = page.paper_mm();
        let (w, h) = if page.landscape { (h, w) } else { (w, h) };
        let m = page.margins.unwrap_or(PageMargins::NORMAL);
        let mut props = format!(
            "fo:page-width=\"{w}mm\" fo:page-height=\"{h}mm\" style:print-orientation=\"{}\" fo:margin-top=\"{}mm\" fo:margin-bottom=\"{}mm\" fo:margin-left=\"{}mm\" fo:margin-right=\"{}mm\" eo:header-distance=\"{}mm\" eo:footer-distance=\"{}mm\"",
            if page.landscape { "landscape" } else { "portrait" },
            mm(m.top),
            mm(m.bottom),
            mm(m.left),
            mm(m.right),
            mm(m.header),
            mm(m.footer)
        );
        let centering = match (page.center_horizontal, page.center_vertical) {
            (true, true) => "both",
            (true, false) => "horizontal",
            (false, true) => "vertical",
            (false, false) => "none",
        };
        let _ = write!(props, " style:table-centering=\"{centering}\"");
        let mut print = Vec::new();
        if page.headings {
            print.push("headers");
        }
        if page.grid_lines {
            print.push("grid");
        }
        if !print.is_empty() {
            let _ = write!(props, " style:print=\"{}\"", print.join(" "));
        }
        match page.scale {
            PageScale::Percent(p) => {
                let _ = write!(props, " style:scale-to=\"{p}%\"");
            }
            PageScale::Fit { width, height } => {
                let _ = write!(
                    props,
                    " style:scale-to-X=\"{width}\" style:scale-to-Y=\"{height}\""
                );
            }
        }
        let _ = write!(
            automatic,
            "<style:page-layout style:name=\"Mpm{n}\"><style:page-layout-properties {props}/></style:page-layout>"
        );
        let _ = write!(
            masters,
            "<style:master-page style:name=\"Mp{n}\" style:page-layout-name=\"Mpm{n}\"></style:master-page>"
        );
    }
    Some(format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><office:document-styles ",
            "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
            "xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" ",
            "xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" xmlns:eo=\"{}\" office:version=\"1.2\">",
            "{}<office:automatic-styles>{}</office:automatic-styles><office:master-styles>{}</office:master-styles>",
            "</office:document-styles>"
        ),
        EO_NS,
        if links {
            crate::sheet_styling::ods_links::LINK_STYLES_XML
        } else {
            ""
        },
        automatic,
        masters
    ))
}

/// A sheet name as a range prefix in `table:print-ranges`: quoted when it holds anything but letters, digits
/// and `_`, with `'` doubled.
fn range_sheet(name: &str) -> String {
    let plain = !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_');
    if plain {
        name.to_string()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

/// ` table:print-ranges="…"` for a sheet with a print area, empty without one.
pub(crate) fn print_ranges_attr(sheet_name: &str, page: &SheetPage) -> String {
    if page.print_area.is_empty() {
        return String::new();
    }
    let sheet = range_sheet(sheet_name);
    let corner = |r: u32, c: u32| {
        let mut buf = [0u8; 4];
        format!(
            "{sheet}.{}{}",
            crate::rtf_xml_helpers::write_col_letters(&mut buf, c as usize),
            r + 1
        )
    };
    let ranges: Vec<String> = page
        .print_area
        .iter()
        .map(|b| format!("{}:{}", corner(b.row0, b.col0), corner(b.row1, b.col1)))
        .collect();
    format!(
        " table:print-ranges=\"{}\"",
        XmlSanitizer::escape(&ranges.join(" "))
    )
}

/// The part of `text` after the sheet prefix (the last `.` outside quotes), as the range text.
fn range_tail(text: &str) -> &str {
    let mut quoted = false;
    let mut at = None;
    for (i, c) in text.char_indices() {
        match c {
            '\'' => quoted = !quoted,
            '.' if !quoted => at = Some(i),
            _ => {}
        }
    }
    at.map_or(text, |i| &text[i + 1..])
}

fn parse_print_ranges(text: &str) -> Vec<CellBox> {
    let mut out = Vec::new();
    let (mut start, mut quoted) = (0, false);
    let mut push = |token: &str| {
        let token = token.trim();
        if token.is_empty() {
            return;
        }
        let (a, b) = token
            .split_once(':')
            .map_or((token, token), |(a, b)| (a, b));
        let (a, b) = (range_tail(a), range_tail(b));
        if let Some(r) = CellBox::parse(&format!("{a}:{b}")) {
            out.push(r);
        }
    };
    for (i, c) in text.char_indices() {
        match c {
            '\'' => quoted = !quoted,
            ' ' if !quoted => {
                push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    push(&text[start..]);
    out
}

fn paper_code(width_mm: f64, height_mm: f64) -> Option<u16> {
    let (short, long) = (width_mm.min(height_mm), width_mm.max(height_mm));
    PAPER_CHOICES.into_iter().find(|&code| {
        let probe = SheetPage {
            paper: Some(code),
            ..SheetPage::default()
        };
        let (w, h) = probe.paper_mm();
        (w - short).abs() < PAPER_TOLERANCE_MM && (h - long).abs() < PAPER_TOLERANCE_MM
    })
}

/// A page layout's properties as the layout of a sheet (the margins when all four are given, the paper when it is
/// one the page setup lists).
fn layout_from(props: &str) -> SheetPage {
    let get = |name: &str| extract_attr_from_fragment(props, name).map(XmlSanitizer::unescape);
    let length_mm = |name: &str| {
        get(name)
            .and_then(|v| mils(&v))
            .map(|m| f64::from(m) * MM_PER_INCH / 1000.0)
    };
    let mut page = SheetPage {
        landscape: get("style:print-orientation").as_deref() == Some("landscape"),
        ..SheetPage::default()
    };
    if let (Some(w), Some(h)) = (length_mm("fo:page-width"), length_mm("fo:page-height")) {
        page.paper = paper_code(w, h);
    }
    let side = |name: &str| get(name).and_then(|v| mils(&v));
    if let (Some(top), Some(bottom), Some(left), Some(right)) = (
        side("fo:margin-top"),
        side("fo:margin-bottom"),
        side("fo:margin-left"),
        side("fo:margin-right"),
    ) {
        // The usual margins are what a sheet without margins prints with.
        page.margins = Some(PageMargins {
            left,
            right,
            top,
            bottom,
            header: side("eo:header-distance").unwrap_or(PageMargins::NORMAL.header),
            footer: side("eo:footer-distance").unwrap_or(PageMargins::NORMAL.footer),
        })
        .filter(|m| *m != PageMargins::NORMAL);
    }
    match get("style:table-centering").as_deref() {
        Some("both") => (page.center_horizontal, page.center_vertical) = (true, true),
        Some("horizontal") => page.center_horizontal = true,
        Some("vertical") => page.center_vertical = true,
        _ => {}
    }
    for word in get("style:print").unwrap_or_default().split_whitespace() {
        match word {
            "headers" => page.headings = true,
            "grid" => page.grid_lines = true,
            _ => {}
        }
    }
    let pages = |name: &str| {
        get(name)
            .and_then(|v| v.trim().parse::<u16>().ok())
            .filter(|n| *n <= MAX_FIT_PAGES)
    };
    page.scale = match (pages("style:scale-to-X"), pages("style:scale-to-Y")) {
        (Some(width), Some(height)) => PageScale::Fit { width, height },
        _ => get("style:scale-to")
            .and_then(|v| v.trim().trim_end_matches('%').parse::<u16>().ok())
            .filter(|p| {
                (crate::sheet_page::MIN_PAGE_SCALE..=crate::sheet_page::MAX_PAGE_SCALE).contains(p)
            })
            .map_or(PageScale::Percent(100), PageScale::Percent),
    };
    page
}

/// The opening tag of `style:page-layout-properties` inside a page layout.
fn layout_properties(inner: &str) -> Option<&str> {
    let at = inner.find("<style:page-layout-properties")?;
    let end = inner[at..].find('>')?;
    Some(&inner[at..at + end + 1])
}

/// Rows (`table:table-row`) or columns (`table:table-column`) in `fragment`, counting each element's repeat.
fn count_lines(fragment: &str, tag: &str, repeat: &str) -> u64 {
    let mut total = 0u64;
    let mut rest = fragment;
    while let Some(at) = rest.find(tag) {
        let after = &rest[at + tag.len()..];
        let Some(end) = after.find('>') else { break };
        let head = &after[..end];
        // `table:table-row-group` and `table:table-rows` are not rows.
        if head.is_empty() || head.starts_with([' ', '/', '\n', '\t']) {
            let add = extract_attr_from_fragment(head, repeat)
                .and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(1)
                .max(1);
            total = total.saturating_add(add);
        }
        rest = &after[end..];
    }
    total
}

/// The rows or columns a table repeats on every page (`table:table-header-rows` or `table:table-header-columns`) as a
/// zero-based inclusive run; `None` without one or over `max` lines.
fn header_run(inner: &str, wrapper: &str, tag: &str, repeat: &str, max: u32) -> Option<(u32, u32)> {
    let open = format!("<{wrapper}>");
    let at = inner.find(&open)?;
    let body_start = at + open.len();
    let body_end = body_start + inner[body_start..].find(&format!("</{wrapper}>"))?;
    let before = count_lines(&inner[..at], tag, repeat);
    let within = count_lines(&inner[body_start..body_end], tag, repeat);
    if within == 0 || within > u64::from(max) || before > u64::from(u32::MAX) / 2 {
        return None;
    }
    let first = u32::try_from(before).ok()?;
    let last = before
        .checked_add(within)?
        .checked_sub(1)
        .and_then(|end| u32::try_from(end).ok())?;
    Some((first, last))
}

/// The wrappers that write `title` lines at the start of a table (`None` for a run that starts later).
pub(crate) fn title_end(run: Option<(u32, u32)>) -> Option<u32> {
    run.filter(|(first, _)| *first == 0)
        .and_then(|(_, last)| last.checked_add(1))
}

/// Gives each sheet its page layout (through its table style's master page) and its print ranges. `styles` is
/// `styles.xml` when the file has one; the table styles are in `content`.
pub(crate) fn read_pages(
    content: &str,
    styles: Option<&str>,
    stylings: &mut [(String, SheetStyling)],
) {
    let masters: HashMap<String, String> = styles
        .map(|xml| {
            extract_balanced_elements(xml, "style:master-page")
                .into_iter()
                .filter_map(|(attrs, _)| {
                    Some((
                        extract_attr_from_fragment(attrs, "style:name")
                            .map(XmlSanitizer::unescape)?,
                        extract_attr_from_fragment(attrs, "style:page-layout-name")
                            .map(XmlSanitizer::unescape)?,
                    ))
                })
                .take(MAX_LAYOUTS)
                .collect()
        })
        .unwrap_or_default();
    let layouts: HashMap<String, SheetPage> = styles
        .map(|xml| {
            extract_balanced_elements(xml, "style:page-layout")
                .into_iter()
                .filter_map(|(attrs, inner)| {
                    let name = extract_attr_from_fragment(attrs, "style:name")
                        .map(XmlSanitizer::unescape)?;
                    Some((name, layout_from(layout_properties(inner)?)))
                })
                .take(MAX_LAYOUTS)
                .collect()
        })
        .unwrap_or_default();
    let table_master: HashMap<String, String> = extract_balanced_elements(content, "style:style")
        .into_iter()
        .filter(|(attrs, _)| extract_attr_from_fragment(attrs, "style:family") == Some("table"))
        .filter_map(|(attrs, _)| {
            Some((
                extract_attr_from_fragment(attrs, "style:name").map(XmlSanitizer::unescape)?,
                extract_attr_from_fragment(attrs, "style:master-page-name")
                    .map(XmlSanitizer::unescape)?,
            ))
        })
        .collect();
    for (attrs, inner) in extract_balanced_elements(content, "table:table") {
        let Some(name) =
            extract_attr_from_fragment(attrs, "table:name").map(XmlSanitizer::unescape)
        else {
            continue;
        };
        let Some((_, styling)) = stylings.iter_mut().find(|(n, _)| *n == name) else {
            continue;
        };
        let layout = extract_attr_from_fragment(attrs, "table:style-name")
            .and_then(|s| table_master.get(&XmlSanitizer::unescape(s)))
            .and_then(|m| masters.get(m))
            .and_then(|l| layouts.get(l));
        if let Some(layout) = layout {
            styling.page = layout.clone();
        }
        styling.page.title_rows = header_run(
            inner,
            "table:table-header-rows",
            "<table:table-row",
            "table:number-rows-repeated",
            MAX_TITLE_ROWS,
        );
        styling.page.title_cols = header_run(
            inner,
            "table:table-header-columns",
            "<table:table-column",
            "table:number-columns-repeated",
            MAX_TITLE_COLS,
        );
        if let Some(ranges) = extract_attr_from_fragment(attrs, "table:print-ranges") {
            let boxes = parse_print_ranges(&XmlSanitizer::unescape(ranges));
            if boxes.len() <= MAX_PRINT_RANGES {
                styling.page.print_area = boxes;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_ranges_round_trip_with_quoted_sheet_names() {
        let page = SheetPage {
            print_area: vec![
                CellBox {
                    row0: 0,
                    col0: 0,
                    row1: 9,
                    col1: 3,
                },
                CellBox::cell(14, 27),
            ],
            ..SheetPage::default()
        };
        for name in ["Sheet1", "내 시트", "It's"] {
            let attr = print_ranges_attr(name, &page);
            let value = attr
                .trim_start_matches(" table:print-ranges=\"")
                .trim_end_matches('"');
            assert_eq!(
                parse_print_ranges(&XmlSanitizer::unescape(value)),
                page.print_area,
                "{name}: {attr}"
            );
        }
        assert_eq!(print_ranges_attr("A", &SheetPage::default()), "");
    }

    #[test]
    fn a_layout_reads_back_what_it_wrote() {
        let page = SheetPage {
            landscape: true,
            paper: Some(crate::sheet_page::PAPER_A3),
            margins: Some(PageMargins {
                left: 500,
                right: 600,
                top: 800,
                bottom: 900,
                ..PageMargins::NORMAL
            }),
            scale: PageScale::Fit {
                width: 2,
                height: 0,
            },
            center_horizontal: true,
            headings: true,
            grid_lines: true,
            ..SheetPage::default()
        };
        let xml = styles_xml(std::slice::from_ref(&page), false).unwrap();
        let (_, inner) = extract_balanced_elements(&xml, "style:page-layout").remove(0);
        assert_eq!(layout_from(layout_properties(inner).unwrap()), page);
        assert_eq!(mils("297mm"), Some(11_693));
        assert_eq!(mils("1cm"), Some(394));
        assert_eq!(mils("bad"), None);
        assert_eq!(mils("-1cm"), None);
        assert_eq!(mils("99999in"), None);
    }

    #[test]
    fn hostile_repeat_overflow_and_max_title_end_do_not_panic() {
        let huge = format!(
            r#"<table:table-row table:number-rows-repeated="{max}"/><table:table-row table:number-rows-repeated="{max}"/>"#,
            max = u64::MAX
        );
        assert_eq!(
            count_lines(&huge, "<table:table-row", "table:number-rows-repeated"),
            u64::MAX
        );
        assert_eq!(title_end(Some((0, u32::MAX))), None);

        let before = u64::from(u32::MAX) / 2;
        let xml = format!(
            r#"<table:table-row table:number-rows-repeated="{before}"/><table:table-header-rows><table:table-row table:number-rows-repeated="{within}"/></table:table-header-rows>"#,
            within = u64::from(u32::MAX)
        );
        assert_eq!(
            header_run(
                &xml,
                "table:table-header-rows",
                "<table:table-row",
                "table:number-rows-repeated",
                u32::MAX
            ),
            None
        );
    }
}
