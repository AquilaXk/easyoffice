//! The header and footer of an ODT's first master page and the page margins that place them (`styles.xml`).

use std::borrow::Cow;
use std::collections::HashMap;

use super::{length_twips, OdtWalker, StyleProps};
use crate::rich_document::{frame_paragraph, MAX_PAGE_FRAME_PARAGRAPHS};
use crate::xml_stream::{attr, XmlEvent, XmlReader};
use crate::{
    EasyOfficeError, RichBlock, RichListKind, RichPageFrame, RichPageMargins, RichParagraph,
};

/// Margins and header/footer heights of one `style:page-layout`, in twips.
#[derive(Default, Clone, Copy)]
struct OdtLayout {
    margin_top: Option<i32>,
    margin_bottom: Option<i32>,
    header_height: i32,
    footer_height: i32,
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
                    layout.margin_top = attr(&attrs, "fo:margin-top")
                        .as_deref()
                        .and_then(length_twips);
                    layout.margin_bottom = attr(&attrs, "fo:margin-bottom")
                        .as_deref()
                        .and_then(length_twips);
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
    };
    margins.within_bounds().then_some(margins)
}

/// The header and footer of the first master page in `styles.xml`; the left and first-page variants of
/// the master page are not read.
pub(super) fn read_page_frame(
    xml: &str,
    styles: &HashMap<String, StyleProps>,
    list_styles: &HashMap<String, RichListKind>,
    warnings: &mut Vec<String>,
) -> Result<RichPageFrame, EasyOfficeError> {
    let mut frame = RichPageFrame::default();
    let mut reader = XmlReader::new(xml);
    let (mut masters, mut variants) = (0usize, false);
    let mut open: Option<(bool, OdtWalker<'_>)> = None;
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
                    "style:header-left" | "style:footer-left" | "style:header-first"
                    | "style:footer-first",
                ..
            } => {
                variants |= masters == 1;
                reader.skip_subtree()?;
            }
            XmlEvent::Start {
                name: name @ ("style:header" | "style:footer"),
                ..
            } if masters == 1 && open.is_none() => {
                let mut walker = OdtWalker::new(styles, list_styles);
                walker.frame_mode = true;
                open = Some((name == "style:header", walker));
            }
            XmlEvent::Start { name, ref attrs } => {
                if let Some((_, walker)) = open.as_mut() {
                    if walker.on_start(name, attrs) {
                        reader.skip_subtree()?;
                    }
                }
            }
            XmlEvent::Text(text) => {
                if let Some((_, walker)) = open.as_mut() {
                    walker.push_inline(&text);
                }
            }
            XmlEvent::End(name @ ("style:header" | "style:footer")) if open.is_some() => {
                let Some((header, mut walker)) = open.take() else {
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
                    warnings.push("머리말·꼬리말의 표·그림은 가져오지 않았습니다.".to_string());
                }
                if paragraphs.len() > MAX_PAGE_FRAME_PARAGRAPHS {
                    paragraphs.truncate(MAX_PAGE_FRAME_PARAGRAPHS);
                    warnings.push(format!(
                        "머리말·꼬리말은 문단 {MAX_PAGE_FRAME_PARAGRAPHS}개까지만 가져왔습니다."
                    ));
                }
                debug_assert_eq!(name == "style:header", header);
                if header {
                    frame.header = paragraphs;
                } else {
                    frame.footer = paragraphs;
                }
            }
            XmlEvent::End(name) => {
                if let Some((_, walker)) = open.as_mut() {
                    walker.on_end(name);
                }
            }
        }
    }
    if variants {
        warnings.push("첫 쪽·왼쪽 쪽 머리말·꼬리말은 가져오지 않았습니다.".to_string());
    }
    if !frame.is_empty() {
        let layouts = read_odt_layouts(xml)?;
        let layout = layout_name.and_then(|n| layouts.get(&n).copied());
        frame.margins =
            layout.and_then(|l| odt_margins(l, !frame.header.is_empty(), !frame.footer.is_empty()));
    }
    Ok(frame)
}
