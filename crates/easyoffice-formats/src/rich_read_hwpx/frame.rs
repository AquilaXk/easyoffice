//! Header and footer controls of a HWPX section (`hp:header` / `hp:footer` inside `hp:ctrl`): the first
//! control of each kind that applies to both odd and even pages becomes the page frame, the others are
//! reported.

use std::borrow::Cow;

use super::objects::{decode_note_deco, NoteNumKind};
use super::{Header, SectionWalker};
use crate::hwpx_parts::HWPUNIT_PER_TWIP;
use crate::rich_document::{
    frame_paragraph, safe_link_target, RichColumns, MAX_PAGE_FRAME_PARAGRAPHS,
};
use crate::rich_page_frame::margins_from_hwpx;
use crate::xml_stream::{attr, XmlEvent};
use crate::{
    EasyOfficeError, RichBinding, RichBindingKind, RichBlock, RichField, RichPaper, RichRun,
};

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
        let first_setup = self.margins.is_none();
        let (mut paper, mut landscape) = (None, false);
        let mut kind = RichBindingKind::Single;
        let mut note_target: Option<bool> = None;
        for event in events {
            match event {
                XmlEvent::End("hp:footNotePr" | "hp:endNotePr") => {
                    note_target = None;
                    continue;
                }
                XmlEvent::Start { name, attrs } => {
                    if *name == "hp:footNotePr" {
                        note_target = Some(false);
                        continue;
                    }
                    if *name == "hp:endNotePr" {
                        note_target = Some(true);
                        continue;
                    }
                    if let Some(is_end) = note_target {
                        let spec = if is_end {
                            &mut self.endnote_spec
                        } else {
                            &mut self.footnote_spec
                        };
                        if *name == "hp:autoNumFormat" {
                            if let Some(t) = attr(attrs, "type") {
                                spec.kind = NoteNumKind::from_owpml(&t);
                            }
                            if let Some(p) = attr(attrs, "prefixChar") {
                                spec.prefix = decode_note_deco(Some(&p), None);
                            }
                            if let Some(s) = attr(attrs, "suffixChar") {
                                spec.suffix = decode_note_deco(Some(&s), None);
                            }
                            if let Some(u) = attr(attrs, "userChar") {
                                spec.user_char = decode_note_deco(Some(&u), None);
                            }
                        } else if *name == "hp:numbering" {
                            let restart = matches!(
                                attr(attrs, "type").as_deref(),
                                Some("ON_SECTION" | "ON_PAGE")
                            );
                            spec.restart_each_section = restart;
                            let new_num = attr(attrs, "newNum")
                                .and_then(|v| v.trim().parse::<usize>().ok())
                                .filter(|n| *n > 0);
                            if restart {
                                let base = new_num.unwrap_or(1).saturating_sub(1);
                                if is_end {
                                    self.next_endnote = base;
                                } else {
                                    self.next_footnote = base;
                                }
                            } else if first_setup {
                                if let Some(start) = new_num {
                                    let base = start.saturating_sub(1);
                                    if is_end {
                                        self.next_endnote = base;
                                    } else {
                                        self.next_footnote = base;
                                    }
                                }
                            }
                        }
                        continue;
                    }
                    if !first_setup {
                        continue;
                    }
                    if *name == "hp:visibility"
                        && (attr(attrs, "hideFirstHeader").as_deref() == Some("1")
                            || attr(attrs, "hideFirstFooter").as_deref() == Some("1"))
                    {
                        self.frame.first_page_different = true;
                        continue;
                    }
                    let value =
                        |key: &str| attr(attrs, key).and_then(|v| v.trim().parse::<u32>().ok());
                    if *name == "hp:pagePr" {
                        landscape = attr(attrs, "landscape")
                            .is_some_and(|v| v.eq_ignore_ascii_case("NARROWLY"));
                        kind = match attr(attrs, "gutterType").as_deref() {
                            Some("LEFT_RIGHT") => RichBindingKind::Duplex,
                            Some("TOP_BOTTOM") => RichBindingKind::Top,
                            _ => RichBindingKind::Single,
                        };
                        if let (Some(width), Some(height)) = (value("width"), value("height")) {
                            let (short_hw, long_hw) = (width.min(height), width.max(height));
                            paper = Some(
                                if short_hw.abs_diff(59_528) <= 3 && long_hw.abs_diff(84_188) <= 3 {
                                    (RichPaper::A4.width, RichPaper::A4.height)
                                } else {
                                    (to_twips(width), to_twips(height))
                                },
                            );
                        }
                        continue;
                    }
                    if *name != "hp:margin" {
                        continue;
                    }
                    if let (
                        Some(header),
                        Some(footer),
                        Some(top),
                        Some(bottom),
                        Some(left),
                        Some(right),
                    ) = (
                        value("header"),
                        value("footer"),
                        value("top"),
                        value("bottom"),
                        value("left"),
                        value("right"),
                    ) {
                        let margins =
                            margins_from_hwpx((header, footer, top, bottom), (left, right));
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
                        if paper
                            .unwrap_or(RichPaper::A4)
                            .holds_bound(&margins, binding)
                        {
                            self.margins = Some(margins);
                            self.paper = paper;
                            self.binding = binding;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// A page-number field of a header or footer paragraph, in the format of the open run.
    pub(super) fn push_page_field(
        &mut self,
        header: &Header,
        field: RichField,
    ) -> Result<(), EasyOfficeError> {
        self.flush_run(header)?;
        let fmt = self
            .runs
            .last()
            .and_then(|run| header.chars.get(&run.char_pr))
            .cloned()
            .unwrap_or_default();
        if let Some(top) = self.paras.last_mut() {
            self.text_budget.charge_run(0, "HWPX 페이지 필드")?;
            top.runs.push(RichRun {
                bold: fmt.bold,
                italic: fmt.italic,
                underline: fmt.underline,
                strike: fmt.strike,
                size_half_pt: fmt.size_half_pt,
                font: fmt.font,
                font_east_asia: fmt.font_east_asia,
                color: fmt.color,
                highlight: fmt.highlight,
                script: fmt.script,
                letter_spacing_twips: fmt.letter_spacing_twips,
                advanced: fmt.advanced,
                field: Some(field),
                ..RichRun::default()
            });
        }
        Ok(())
    }

    /// Reads the `hp:header` or `hp:footer` of a control (events span `hp:ctrl` to its end) into the page
    /// frame; returns false when the control holds neither.
    pub(super) fn on_frame_control(
        &mut self,
        events: &[XmlEvent<'_>],
        header: &Header,
    ) -> Result<bool, EasyOfficeError> {
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
            return Ok(false);
        };
        let base = usize::from(!is_header);
        let slot = match apply.as_str() {
            "" | "BOTH" => base,
            "ODD" => 2 + base,
            "EVEN" => 4 + base,
            _ => {
                self.left_out.pages = true;
                return Ok(true);
            }
        };
        if self.frame_found[slot] {
            self.left_out.sections = true;
            return Ok(true);
        }
        self.frame_found[slot] = true;
        if slot >= 2 {
            self.frame.even_odd_different = true;
        }
        let mut walker = SectionWalker {
            text_budget: self.text_budget.clone(),
            ..Default::default()
        };
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
                    )?;
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
                    walker.on_start(name, attrs, header, &mut blocks)?;
                }
                XmlEvent::Text(text) => walker.on_text(text)?,
                XmlEvent::End(name) => {
                    depth = depth.saturating_sub(1);
                    walker.on_end(name, header, &mut blocks)?;
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
        match (slot, is_header) {
            (4, _) => self.frame.even_header = paragraphs,
            (5, _) => self.frame.even_footer = paragraphs,
            (0, _) if self.frame_found[2] => {}
            (1, _) if self.frame_found[3] => {}
            (_, true) => self.frame.header = paragraphs,
            (_, false) => self.frame.footer = paragraphs,
        }
        self.text_budget = walker.text_budget;
        Ok(true)
    }

    /// Reads inline controls (`HYPERLINK` field marks, `hp:colPr` column changes, `hp:pageHiding`).
    pub(super) fn on_ctrl(
        &mut self,
        events: &[XmlEvent<'_>],
        header: &Header,
    ) -> Result<(), EasyOfficeError> {
        let (mut begin, mut in_command, mut command) = (false, false, String::new());
        for event in events {
            match event {
                XmlEvent::Start {
                    name: "hp:fieldBegin",
                    attrs,
                } if attr(attrs, "type").as_deref() == Some("HYPERLINK") => {
                    self.flush_run(header)?;
                    begin = true;
                }
                XmlEvent::Start {
                    name: "hp:fieldEnd",
                    ..
                } => {
                    self.flush_run(header)?;
                    self.link = None;
                }
                XmlEvent::Start {
                    name: "hp:stringParam",
                    attrs,
                } => {
                    in_command = begin && attr(attrs, "name").as_deref() == Some("Command");
                }
                XmlEvent::Start {
                    name: "hp:colPr",
                    attrs,
                } => {
                    if let Some(c) =
                        attr(attrs, "colCount").and_then(|v| v.trim().parse::<u8>().ok())
                    {
                        self.current_columns = if c >= 2 {
                            let equal = attr(attrs, "sameSz").as_deref() != Some("0");
                            let gap_hwp = attr(attrs, "sameGap")
                                .and_then(|v| v.trim().parse::<u32>().ok())
                                .unwrap_or(0);
                            let gap_twips = u16::try_from(to_twips(gap_hwp)).unwrap_or(u16::MAX);
                            Some(RichColumns {
                                count: c.min(16),
                                equal,
                                gap_twips,
                            })
                        } else {
                            None
                        };
                        if self.tbl_depth == 0 {
                            if let Some(top) = self.paras.last_mut() {
                                top.columns = self.current_columns;
                            }
                        }
                    }
                }
                XmlEvent::Start {
                    name: "hp:pageHiding",
                    attrs,
                } if attr(attrs, "hideHeader").as_deref() == Some("1")
                    || attr(attrs, "hideFooter").as_deref() == Some("1") =>
                {
                    self.frame.first_page_different = true;
                }
                XmlEvent::Text(text) if in_command => command.push_str(text),
                XmlEvent::End("hp:stringParam") => in_command = false,
                _ => {}
            }
        }
        if begin {
            self.link = link_from_command(&command);
        }
        Ok(())
    }
}

/// The address of a hyperlink field's `Command`: the plain address, `HYPERLINK "address"`, or the form the
/// word processor writes (`http\://host/path;1;0;0;`, colons escaped and four numbers behind a `;`).
pub(super) fn link_from_command(command: &str) -> Option<String> {
    let command = command.trim();
    let quoted = command
        .strip_prefix("HYPERLINK")
        .map(str::trim_start)
        .and_then(|rest| rest.strip_prefix('"'))
        .and_then(|rest| rest.strip_suffix('"'));
    let address = quoted.map_or_else(|| strip_link_flags(command), str::to_string);
    safe_link_target(&address).map(str::to_string)
}

/// `address;n;n;n;` with backslash escapes becomes `address`; anything else is returned as it is.
fn strip_link_flags(command: &str) -> String {
    let flags = command.strip_suffix(';').and_then(|rest| {
        let mut parts = rest.rsplitn(4, ';');
        let numbers = [parts.next()?, parts.next()?, parts.next()?];
        let address = parts.next()?;
        numbers
            .iter()
            .all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            .then_some(address)
    });
    let Some(address) = flags else {
        return command.to_string();
    };
    let mut out = String::with_capacity(address.len());
    let mut chars = address.chars();
    while let Some(c) = chars.next() {
        out.push(if c == '\\' {
            chars.next().unwrap_or(c)
        } else {
            c
        });
    }
    out
}
