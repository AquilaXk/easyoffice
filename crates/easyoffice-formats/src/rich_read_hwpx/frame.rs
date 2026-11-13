//! Header and footer controls of a HWPX section (`hp:header` / `hp:footer` inside `hp:ctrl`): the first
//! control of each kind that applies to both odd and even pages becomes the page frame, the others are
//! reported.

use std::borrow::Cow;

use super::{Header, SectionWalker};
use crate::hwpx_parts::HWPUNIT_PER_TWIP;
use crate::rich_document::{frame_paragraph, MAX_PAGE_FRAME_PARAGRAPHS};
use crate::rich_page_frame::margins_from_hwpx;
use crate::xml_stream::{attr, XmlEvent};
use crate::{RichBinding, RichBindingKind, RichBlock, RichField, RichPaper, RichRun};

/// Header and footer controls that are not kept.
#[derive(Default)]
pub(super) struct FrameLeftOut {
    /// Odd-page, even-page controls.
    pub(super) pages: bool,
    /// A second `BOTH` control of the same kind (another section).
    pub(super) sections: bool,
    /// Tables, pictures and shapes inside the header or footer.
    pub(super) objects: bool,
    /// More paragraphs than [`MAX_PAGE_FRAME_PARAGRAPHS`].
    pub(super) long: bool,
}

/// HWPUNIT as twips, to the nearest twip (the reference editor's A4 is 59528 by 84188 HWPUNIT).
fn to_twips(hwpunit: u32) -> u32 {
    hwpunit.saturating_add(HWPUNIT_PER_TWIP / 2) / HWPUNIT_PER_TWIP
}

impl SectionWalker {
    /// The page setup of a section (`hp:secPr > hp:pagePr` and its `hp:margin`, events span `hp:secPr` to its
    /// end); the first section that gives a setup that fits its paper wins. The paper is stored upright and
    /// swapped when the page is landscape (`NARROWLY`, `rhwp` `src/parser/hwpx/section.rs:450-458`).
    pub(super) fn on_section_properties(&mut self, events: &[XmlEvent<'_>]) {
        if self.margins.is_some() {
            return;
        }
        let (mut paper, mut landscape) = (None, false);
        let mut kind = RichBindingKind::Single;
        for event in events {
            let XmlEvent::Start { name, attrs } = event else {
                continue;
            };
            let value = |key: &str| attr(attrs, key).and_then(|v| v.trim().parse::<u32>().ok());
            if *name == "hp:pagePr" {
                landscape =
                    attr(attrs, "landscape").is_some_and(|v| v.eq_ignore_ascii_case("NARROWLY"));
                kind = match attr(attrs, "gutterType").as_deref() {
                    Some("LEFT_RIGHT") => RichBindingKind::Duplex,
                    Some("TOP_BOTTOM") => RichBindingKind::Top,
                    _ => RichBindingKind::Single,
                };
                if let (Some(width), Some(height)) = (value("width"), value("height")) {
                    paper = Some((to_twips(width), to_twips(height)));
                }
                continue;
            }
            if *name != "hp:margin" {
                continue;
            }
            if let (Some(header), Some(footer), Some(top), Some(bottom), Some(left), Some(right)) = (
                value("header"),
                value("footer"),
                value("top"),
                value("bottom"),
                value("left"),
                value("right"),
            ) {
                let margins = margins_from_hwpx((header, footer, top, bottom), (left, right));
                let paper = paper.map(|(w, h)| {
                    let (short, long) = (w.min(h), w.max(h));
                    if landscape {
                        RichPaper {
                            width: long,
                            height: short,
                        }
                    } else {
                        RichPaper {
                            width: short,
                            height: long,
                        }
                    }
                });
                let binding = RichBinding {
                    kind,
                    gutter: to_twips(value("gutter").unwrap_or(0)),
                };
                if paper.is_none_or(|p| p.holds_bound(&margins, binding)) {
                    self.margins = Some(margins);
                    self.paper = paper;
                    self.binding = binding;
                }
            }
            return;
        }
    }

    /// A page-number field of a header or footer paragraph, in the format of the open run.
    pub(super) fn push_page_field(&mut self, header: &Header, field: RichField) {
        self.flush_run(header);
        let fmt = self
            .runs
            .last()
            .and_then(|run| header.chars.get(&run.char_pr))
            .cloned()
            .unwrap_or_default();
        if let Some(top) = self.paras.last_mut() {
            top.runs.push(RichRun {
                bold: fmt.bold,
                italic: fmt.italic,
                underline: fmt.underline,
                strike: fmt.strike,
                size_half_pt: fmt.size_half_pt,
                font: fmt.font,
                color: fmt.color,
                highlight: fmt.highlight,
                script: fmt.script,
                field: Some(field),
                ..RichRun::default()
            });
        }
    }

    /// Reads the `hp:header` or `hp:footer` of a control (events span `hp:ctrl` to its end) into the page
    /// frame; returns false when the control holds neither.
    pub(super) fn on_frame_control(&mut self, events: &[XmlEvent<'_>], header: &Header) -> bool {
        let Some((at, is_header, apply)) = events.iter().enumerate().find_map(|(i, e)| match e {
            XmlEvent::Start {
                name: name @ ("hp:header" | "hp:footer"),
                attrs,
            } => Some((
                i,
                *name == "hp:header",
                attr(attrs, "applyPageType").map_or(String::new(), Cow::into_owned),
            )),
            _ => None,
        }) else {
            return false;
        };
        let kind = usize::from(!is_header);
        if !matches!(apply.as_str(), "" | "BOTH") {
            self.left_out.pages = true;
            return true;
        }
        if self.frame_found[kind] {
            self.left_out.sections = true;
            return true;
        }
        self.frame_found[kind] = true;
        let mut walker = SectionWalker::default();
        let mut blocks = Vec::new();
        let mut depth = 0usize;
        for event in &events[at..] {
            match event {
                XmlEvent::Start {
                    name: "hp:autoNum",
                    attrs,
                } if matches!(
                    attr(attrs, "numType").as_deref(),
                    Some("PAGE" | "TOTAL_PAGE")
                ) =>
                {
                    depth += 1;
                    let total = attr(attrs, "numType").as_deref() == Some("TOTAL_PAGE");
                    walker.push_page_field(
                        header,
                        if total {
                            RichField::TotalPages
                        } else {
                            RichField::PageNumber
                        },
                    );
                }
                XmlEvent::Start { name, attrs } => {
                    depth += 1;
                    if matches!(
                        *name,
                        "hp:pic"
                            | "hp:equation"
                            | "hp:ole"
                            | "hp:rect"
                            | "hp:ellipse"
                            | "hp:line"
                            | "hp:container"
                    ) {
                        self.left_out.objects = true;
                    }
                    walker.on_start(name, attrs, header, &mut blocks);
                }
                XmlEvent::Text(text) => walker.on_text(text),
                XmlEvent::End(name) => {
                    depth = depth.saturating_sub(1);
                    walker.on_end(name, header, &mut blocks);
                    if depth == 0 {
                        break;
                    }
                }
            }
        }
        let mut paragraphs = Vec::new();
        for block in blocks {
            match block {
                RichBlock::Paragraph(p) => paragraphs.push(frame_paragraph(&p)),
                _ => self.left_out.objects = true,
            }
        }
        if paragraphs.len() > MAX_PAGE_FRAME_PARAGRAPHS {
            paragraphs.truncate(MAX_PAGE_FRAME_PARAGRAPHS);
            self.left_out.long = true;
        }
        if is_header {
            self.frame.header = paragraphs;
        } else {
            self.frame.footer = paragraphs;
        }
        true
    }
}
