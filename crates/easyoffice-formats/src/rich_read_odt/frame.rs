//! The header and footer of an ODT's first master page and the page margins that place them (`styles.xml`).

use std::borrow::Cow;
use std::collections::HashMap;

use super::{length_twips, OdtWalker, StyleProps};
use crate::rich_document::{
    frame_paragraph, RichColumns, RichListNumber, RichNumberEnclosure, RichNumberFormat,
    MAX_PAGE_FRAME_PARAGRAPHS,
};
use crate::rich_list_writers::LIST_LEVELS;
use crate::xml_stream::{attr, Attrs, XmlEvent, XmlReader};
use crate::{
    EasyOfficeError, RichBinding, RichBindingKind, RichBlock, RichListKind, RichPageFrame,
    RichPageMargins, RichPaper, RichParagraph,
};

#[derive(Clone, Copy)]
pub(super) struct OdtListStyle {
    pub kind: RichListKind,
    pub levels: [Option<RichListNumber>; LIST_LEVELS],
}

impl OdtListStyle {
    pub const fn new(kind: RichListKind) -> Self {
        Self {
            kind,
            levels: [None; LIST_LEVELS],
        }
    }
}

pub(super) fn parse_odt_list_level_number(attrs: &Attrs<'_>) -> (usize, RichListNumber) {
    let level = attr(attrs, "text:level")
        .and_then(|v| v.trim().parse::<usize>().ok())
        .map_or(0, |n| n.saturating_sub(1).min(LIST_LEVELS - 1));
    let format = match attr(attrs, "style:num-format")
        .as_deref()
        .unwrap_or("1")
        .trim()
    {
        "a" => RichNumberFormat::LowerLatin,
        "A" => RichNumberFormat::UpperLatin,
        "i" => RichNumberFormat::LowerRoman,
        "I" => RichNumberFormat::UpperRoman,
        s if s.starts_with('가') => RichNumberFormat::HangulSyllable,
        _ => RichNumberFormat::Decimal,
    };
    let prefix = attr(attrs, "style:num-prefix").unwrap_or_default();
    let suffix = attr(attrs, "style:num-suffix").unwrap_or(Cow::Borrowed("."));
    let enclosure = match (prefix.trim(), suffix.trim()) {
        ("(", ")") => RichNumberEnclosure::Parentheses,
        ("", ")") => RichNumberEnclosure::RightParenthesis,
        _ => RichNumberEnclosure::Dot,
    };
    (level, RichListNumber { format, enclosure })
}

/// Margins, columns and header/footer heights of one `style:page-layout`, in twips.
#[derive(Default, Clone, Copy)]
struct OdtLayout {
    margin_top: Option<i32>,
    margin_bottom: Option<i32>,
    margin_left: Option<i32>,
    margin_right: Option<i32>,
    page_width: Option<i32>,
    page_height: Option<i32>,
    mirrored: bool,
    header_height: i32,
    footer_height: i32,
    columns: Option<RichColumns>,
}

/// The page layouts of `styles.xml` by name.
fn read_odt_layouts(xml: &str) -> Result<HashMap<String, OdtLayout>, EasyOfficeError> {
    let mut layouts = HashMap::new();
    let mut open: Option<(String, OdtLayout)> = None;
    // 1 inside `style:header-style`, 2 inside `style:footer-style`.
    let mut part = 0u8;
    for event in XmlReader::new(xml) {
        match event? {
            XmlEvent::Start {
                name: "style:page-layout",
                attrs,
            } => {
                open = attr(&attrs, "style:name").map(|n| (n.into_owned(), OdtLayout::default()));
            }
            XmlEvent::Start {
                name: "style:header-style",
                ..
            } => part = 1,
            XmlEvent::Start {
                name: "style:footer-style",
                ..
            } => part = 2,
            XmlEvent::Start {
                name: "style:page-layout-properties",
                attrs,
            } => {
                if let Some((_, layout)) = open.as_mut() {
                    let length = |key: &str| attr(&attrs, key).as_deref().and_then(length_twips);
                    layout.margin_top = length("fo:margin-top");
                    layout.margin_bottom = length("fo:margin-bottom");
                    layout.margin_left = length("fo:margin-left");
                    layout.margin_right = length("fo:margin-right");
                    layout.page_width = length("fo:page-width");
                    layout.page_height = length("fo:page-height");
                    layout.mirrored =
                        attr(&attrs, "style:page-usage").as_deref() == Some("mirrored");
                }
            }
            XmlEvent::Start {
                name: "style:columns",
                attrs,
            } if part == 0 => {
                if let Some((_, layout)) = open.as_mut() {
                    let count = attr(&attrs, "fo:column-count")
                        .and_then(|v| v.trim().parse::<u16>().ok())
                        .map_or(1, |n| n.clamp(1, 16) as u8);
                    if count > 1 {
                        let gap = attr(&attrs, "fo:column-gap")
                            .as_deref()
                            .and_then(length_twips)
                            .map_or(720, |t| t.max(0).min(i32::from(u16::MAX)) as u16);
                        layout.columns = Some(RichColumns {
                            count,
                            equal: true,
                            gap_twips: gap,
                        });
                    }
                }
            }
            XmlEvent::Start {
                name: "style:header-footer-properties",
                attrs,
            } => {
                if let Some((_, layout)) = open.as_mut() {
                    let length = |key: &str| {
                        attr(&attrs, key)
                            .as_deref()
                            .and_then(length_twips)
                            .unwrap_or(0)
                            .max(0)
                    };
                    match part {
                        1 => {
                            layout.header_height =
                                length("fo:min-height") + length("fo:margin-bottom")
                        }
                        2 => {
                            layout.footer_height = length("fo:min-height") + length("fo:margin-top")
                        }
                        _ => {}
                    }
                }
            }
            XmlEvent::End("style:header-style" | "style:footer-style") => part = 0,
            XmlEvent::End("style:page-layout") => {
                if let Some((name, layout)) = open.take() {
                    layouts.insert(name, layout);
                }
            }
            _ => {}
        }
    }
    Ok(layouts)
}

/// The margins of a master page's layout in the page-frame form: the layout's top margin is the distance to
/// the header and the body starts below the header's height (likewise the bottom); a part that is absent
/// has no height. `None` when the layout gives no margins or they are out of bounds.
fn odt_margins(layout: OdtLayout, has_header: bool, has_footer: bool) -> Option<RichPageMargins> {
    let twips = |v: i32| u32::try_from(v).ok();
    let top_margin = twips(layout.margin_top?)?;
    let bottom_margin = twips(layout.margin_bottom?)?;
    let margins = RichPageMargins {
        header: top_margin,
        top: top_margin.checked_add(twips(if has_header { layout.header_height } else { 0 })?)?,
        footer: bottom_margin,
        bottom: bottom_margin.checked_add(twips(if has_footer {
            layout.footer_height
        } else {
            0
        })?)?,
        left: twips(layout.margin_left?)?,
        right: twips(layout.margin_right?)?,
    };
    margins.within_bounds().then_some(margins)
}

/// The page setup of a master page's layout: the margins, and the paper when the layout gives both sides and the
/// margins fit on it (a setup that does not fit is dropped whole).
fn odt_setup(
    layout: OdtLayout,
    has_header: bool,
    has_footer: bool,
) -> (Option<RichPageMargins>, Option<RichPaper>) {
    let Some(margins) = odt_margins(layout, has_header, has_footer) else {
        return (None, None);
    };
    let paper = match (layout.page_width, layout.page_height) {
        (Some(w), Some(h)) => match (u32::try_from(w), u32::try_from(h)) {
            (Ok(width), Ok(height)) if width > 0 && height > 0 => Some(RichPaper { width, height }),
            _ => None,
        },
        _ => None,
    };
    match paper {
        Some(p) if !p.holds(&margins) => (None, None),
        _ => (Some(margins), paper),
    }
}

/// Reads the header/footer slots (default, first-page, left/even-page) and page columns of the
/// first master page in `styles.xml`; extra master pages are reported.
pub(super) fn read_page_frame(
    xml: &str,
    styles: &HashMap<String, StyleProps>,
    list_styles: &HashMap<String, OdtListStyle>,
    warnings: &mut Vec<String>,
) -> Result<(RichPageFrame, Option<RichColumns>), EasyOfficeError> {
    let mut frame = RichPageFrame::default();
    let mut reader = XmlReader::new(xml);
    let (mut masters, mut other_masters) = (0usize, false);
    let mut open: Option<(&'static str, OdtWalker<'_>)> = None;
    let mut layout_name: Option<String> = None;
    while let Some(event) = reader.next_event()? {
        match event {
            XmlEvent::Start {
                name: "style:master-page",
                attrs,
            } => {
                masters += 1;
                if masters == 1 {
                    layout_name = attr(&attrs, "style:page-layout-name").map(Cow::into_owned);
                }
            }
            XmlEvent::Start {
                name:
                    tag @ ("style:header" | "style:footer" | "style:header-left" | "style:footer-left"
                    | "style:header-first" | "style:footer-first"),
                ref attrs,
            } if open.is_none() => {
                if masters != 1 {
                    other_masters = true;
                    reader.skip_subtree()?;
                    continue;
                }
                if attr(attrs, "style:display").as_deref() == Some("false") {
                    reader.skip_subtree()?;
                    continue;
                }
                let slot = match tag {
                    "style:header" => "header",
                    "style:footer" => "footer",
                    "style:header-left" => {
                        frame.even_odd_different = true;
                        "even_header"
                    }
                    "style:footer-left" => {
                        frame.even_odd_different = true;
                        "even_footer"
                    }
                    "style:header-first" => {
                        frame.first_page_different = true;
                        "first_header"
                    }
                    _ => {
                        frame.first_page_different = true;
                        "first_footer"
                    }
                };
                let mut walker = OdtWalker::new(styles, list_styles);
                walker.frame_mode = true;
                open = Some((slot, walker));
            }
            XmlEvent::Start { name, ref attrs } => {
                if let Some((_, walker)) = open.as_mut() {
                    if walker.on_start(name, attrs)? {
                        reader.skip_subtree()?;
                    }
                }
            }
            XmlEvent::Text(text) => {
                if let Some((_, walker)) = open.as_mut() {
                    walker.push_inline(&text)?;
                }
            }
            XmlEvent::End(
                "style:header" | "style:footer" | "style:header-left" | "style:footer-left"
                | "style:header-first" | "style:footer-first",
            ) if open.is_some() => {
                let Some((slot, mut walker)) = open.take() else {
                    continue;
                };
                warnings.append(&mut walker.warnings);
                let mut paragraphs: Vec<RichParagraph> = walker
                    .blocks
                    .iter()
                    .filter_map(|b| match b {
                        RichBlock::Paragraph(p) => Some(frame_paragraph(p)),
                        _ => None,
                    })
                    .collect();
                if walker
                    .blocks
                    .iter()
                    .any(|b| !matches!(b, RichBlock::Paragraph(_)))
                {
                    let note = "머리말·꼬리말의 표·그림은 가져오지 않았습니다.".to_string();
                    if !warnings.contains(&note) {
                        warnings.push(note);
                    }
                }
                if paragraphs.len() > MAX_PAGE_FRAME_PARAGRAPHS {
                    paragraphs.truncate(MAX_PAGE_FRAME_PARAGRAPHS);
                    warnings.push(format!(
                        "머리말·꼬리말은 문단 {MAX_PAGE_FRAME_PARAGRAPHS}개까지만 가져왔습니다."
                    ));
                }
                match slot {
                    "header" => frame.header = paragraphs,
                    "footer" => frame.footer = paragraphs,
                    "even_header" => frame.even_header = paragraphs,
                    "even_footer" => frame.even_footer = paragraphs,
                    "first_header" => frame.first_header = paragraphs,
                    _ => frame.first_footer = paragraphs,
                }
            }
            XmlEvent::End(name) => {
                if let Some((_, walker)) = open.as_mut() {
                    walker.on_end(name)?;
                }
            }
        }
    }
    if other_masters {
        warnings.push("구역마다 다른 머리말·꼬리말은 첫 구역의 것만 가져왔습니다.".to_string());
    }
    let layouts = read_odt_layouts(xml)?;
    let mut columns = None;
    if let Some(layout) = layout_name.and_then(|n| layouts.get(&n).copied()) {
        columns = layout.columns;
        (frame.margins, frame.paper) =
            odt_setup(layout, frame.has_any_header(), frame.has_any_footer());
        if frame.margins.is_some() && layout.mirrored {
            frame.binding = RichBinding {
                kind: RichBindingKind::Duplex,
                gutter: 0,
            };
        }
    }
    Ok((frame, columns))
}
