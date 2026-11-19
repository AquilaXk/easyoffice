use std::borrow::Cow;
use std::collections::HashMap;

use crate::rich_document::{highlight_name_to_hex, normalize_hex_color, RichColumns, RichToc};
use crate::rich_read_numbering::{Numbering, LIST_LEVELS};
use crate::rich_xml::{
    heading_level_from_name, take_toc_page, toc_role_from_name, toggle_on, CharFmt,
};
use crate::xml_stream::{attr, Attrs, XmlEvent, XmlReader};
use crate::{
    EasyOfficeError, RichAlignment, RichImage, RichListKind, RichParagraph, RichRun, RichScript,
};

pub(crate) const MAX_STYLE_INHERIT_DEPTH: usize = 16;

#[derive(Default)]
pub(super) struct ParaState {
    pub(super) runs: Vec<RichRun>,
    pub(super) style_id: Option<String>,
    pub(super) jc: Option<String>,
    pub(super) num_id: Option<String>,
    pub(super) ilvl: Option<usize>,
    pub(super) outline: Option<u8>,
    pub(super) images: Vec<RichImage>,
    pub(super) left_indent: Option<u32>,
    pub(super) first_line: Option<i32>,
    pub(super) line_percent: Option<u16>,
    pub(super) bookmark: Option<u32>,
    pub(super) tab_leader: Option<String>,
    pub(super) page_break_before: bool,
    pub(super) page_break_after: bool,
    pub(super) sect_end: Option<Option<RichColumns>>,
    pub(super) zones: Vec<(String, Vec<RichRun>)>,
    pub(super) task_checkbox: Option<bool>,
    pub(super) strip_task_space: bool,
    /// Runs before this index are known to hold only blank text (see [`Self::runs_are_blank`]).
    pub(super) blank_prefix: usize,
    /// A run with text was seen, so the paragraph's runs are not all blank.
    pub(super) has_text_run: bool,
}

impl ParaState {
    /// Whether every run so far is blank. Each call scans only the runs added since the last one, so
    /// a page break after each of N blank runs costs O(N), not O(N^2) (PD-262). The last run is never
    /// cached because later text can still be appended to it.
    pub(super) fn runs_are_blank(&mut self) -> bool {
        if self.runs.len() < self.blank_prefix {
            self.blank_prefix = 0;
            self.has_text_run = false;
        }
        if self.has_text_run {
            return false;
        }
        let stable = self.runs.len().saturating_sub(1);
        while self.blank_prefix < stable {
            if !self.runs[self.blank_prefix].text.trim().is_empty() {
                self.has_text_run = true;
                return false;
            }
            self.blank_prefix += 1;
        }
        self.runs.last().is_none_or(|r| r.text.trim().is_empty())
    }

    /// Takes the runs and forgets what was known about them.
    pub(super) fn take_runs(&mut self) -> Vec<RichRun> {
        self.blank_prefix = 0;
        self.has_text_run = false;
        std::mem::take(&mut self.runs)
    }
}

#[inline]
fn starts_with_ignore_ascii_case(s: &str, prefix: &str) -> bool {
    s.len() >= prefix.len() && s.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

pub(super) struct Resolver<'a> {
    pub(super) styles: &'a HashMap<String, DocxStyle>,
    pub(super) numbering: &'a Numbering,
}

impl Resolver<'_> {
    pub(super) fn resolve_style_chain(&self, start_id: &str) -> (u8, Option<RichListKind>) {
        let mut heading = 0u8;
        let mut list_from_style = None;
        let mut cur_id = Some(start_id);
        for _ in 0..MAX_STYLE_INHERIT_DEPTH {
            let Some(id) = cur_id else {
                break;
            };
            if heading == 0 {
                heading = heading_level_from_name(id);
            }
            let Some(st) = self.styles.get(id) else {
                break;
            };
            if let Some(name) = st.name.as_deref() {
                if heading == 0 {
                    heading = heading_level_from_name(name);
                }
                if list_from_style.is_none() {
                    if starts_with_ignore_ascii_case(name, "list bullet") {
                        list_from_style = Some(RichListKind::Bullet);
                    } else if starts_with_ignore_ascii_case(name, "list number") {
                        list_from_style = Some(RichListKind::Ordered);
                    }
                }
            }
            if heading == 0 {
                if let Some(lvl) = st.outline_lvl.filter(|&l| l < 9) {
                    heading = (lvl + 1).min(6);
                }
            }
            cur_id = st.based_on.as_deref();
        }
        (heading, list_from_style)
    }

    fn toc_role(&self, style_id: &str) -> Option<RichToc> {
        toc_role_from_name(style_id).or_else(|| {
            let name = self.styles.get(style_id)?.name.as_deref()?;
            toc_role_from_name(name)
        })
    }

    pub(super) fn finish(&self, mut p: ParaState) -> RichParagraph {
        let toc = p.style_id.as_deref().and_then(|id| self.toc_role(id));
        let (mut heading, style_list) = p
            .style_id
            .as_deref()
            .map_or((0, None), |id| self.resolve_style_chain(id));
        if toc.is_some() {
            heading = 0;
        } else if heading == 0 {
            if let Some(level) = p.outline.filter(|l| *l < 9) {
                heading = (level + 1).min(6);
            }
        }
        let level = p.ilvl.unwrap_or(0).min(LIST_LEVELS - 1);
        let numbered = p
            .num_id
            .as_deref()
            .filter(|id| *id != "0")
            .map(|id| self.numbering.level(id, level));
        let (mut list, mut list_number) = match (heading, style_list, numbered) {
            (1.., ..) => (None, None),
            (_, Some(kind), def) => (
                Some(kind),
                def.filter(|d| d.kind == kind).and_then(|d| d.number),
            ),
            (_, None, Some(def)) => (Some(def.kind), def.number),
            (_, None, None) => (None, None),
        };
        let mut task_checked = false;
        let mut list_level = if list.is_some() { level as u8 } else { 0 };
        if heading == 0 && toc.is_none() {
            if let Some(checked) = p.task_checkbox {
                list = Some(RichListKind::Task);
                task_checked = checked;
                list_number = None;
                list_level = if p.ilvl.is_some() {
                    level as u8
                } else if p.first_line == Some(-360)
                    && p.left_indent
                        .is_some_and(|l| (720..=6480).contains(&l) && l % 720 == 0)
                {
                    let recovered = ((p.left_indent.unwrap_or(720) / 720) - 1) as u8;
                    p.left_indent = None;
                    p.first_line = None;
                    recovered
                } else {
                    0
                };
            }
        }
        let toc_page = match toc {
            Some(RichToc::Entry(_)) => take_toc_page(&mut p.runs),
            _ => None,
        };
        RichParagraph {
            list_level,
            list_number,
            task_checked,
            alignment: p.jc.as_deref().and_then(RichAlignment::parse),
            runs: p.runs,
            heading_level: heading,
            list,
            left_indent_twips: p.left_indent,
            first_line_indent_twips: p.first_line,
            line_spacing_percent: p.line_percent,
            toc,
            bookmark: p.bookmark,
            toc_page,
            toc_dots: toc_page.is_some() && p.tab_leader.as_deref().is_none_or(|l| l == "dot"),
            page_break_before: p.page_break_before,
            ..RichParagraph::default()
        }
    }
}

/// Applies a run-property element (`w:b`, `w:sz`, ...) to `fmt`.
pub(super) fn apply_run_property(fmt: &mut CharFmt, name: &str, attrs: &Attrs<'_>) {
    let val = attr(attrs, "w:val");
    let val = val.as_deref();
    match name {
        "w:b" => fmt.bold = toggle_on(val),
        "w:i" => fmt.italic = toggle_on(val),
        "w:strike" | "w:dstrike" => fmt.strike = toggle_on(val),
        "w:u" => fmt.underline = toggle_on(val),
        "w:rFonts" => {
            fmt.font = ["w:ascii", "w:hAnsi", "w:eastAsia"]
                .iter()
                .find_map(|k| attr(attrs, k))
                .filter(|f| !f.trim().is_empty())
                .map(Cow::into_owned);
        }
        "w:sz" => fmt.size_half_pt = val.and_then(|v| v.parse::<u16>().ok()).filter(|v| *v > 0),
        "w:color" => fmt.color = val.and_then(normalize_hex_color),
        "w:highlight" => fmt.highlight = val.and_then(highlight_name_to_hex),
        "w:vertAlign" => {
            fmt.script = match val {
                Some("superscript") => RichScript::Superscript,
                Some("subscript") => RichScript::Subscript,
                _ => RichScript::Normal,
            };
        }
        "w:shd" => {
            if let Some(fill) = attr(attrs, "w:fill")
                .as_deref()
                .and_then(normalize_hex_color)
            {
                fmt.highlight = Some(fill);
            }
        }
        _ => {}
    }
}

#[derive(Default, Clone, Debug)]
pub(crate) struct DocxStyle {
    pub(crate) name: Option<String>,
    pub(crate) based_on: Option<String>,
    pub(crate) outline_lvl: Option<u8>,
}

/// styleId -> style metadata (`w:name`, `w:basedOn`, `w:outlineLvl`).
/// Adapted from BSD-2-Clause and MIT reference implementations (see `THIRD_PARTY_NOTICES.md`).
pub(crate) fn read_styles(xml: &str) -> Result<HashMap<String, DocxStyle>, EasyOfficeError> {
    let mut out: HashMap<String, DocxStyle> = HashMap::new();
    let mut current: Option<String> = None;
    for ev in XmlReader::new(xml) {
        match ev? {
            XmlEvent::Start {
                name: "w:style",
                attrs,
            } => {
                if let Some(id) = attr(&attrs, "w:styleId") {
                    out.entry(id.to_string()).or_default();
                    current = Some(id.into_owned());
                } else {
                    current = None;
                }
            }
            XmlEvent::Start {
                name: "w:name",
                attrs,
            } => {
                if let (Some(id), Some(val)) = (current.as_ref(), attr(&attrs, "w:val")) {
                    out.entry(id.clone()).or_default().name = Some(val.to_string());
                }
            }
            XmlEvent::Start {
                name: "w:basedOn",
                attrs,
            } => {
                if let (Some(id), Some(val)) = (current.as_ref(), attr(&attrs, "w:val")) {
                    out.entry(id.clone()).or_default().based_on = Some(val.to_string());
                }
            }
            XmlEvent::Start {
                name: "w:outlineLvl",
                attrs,
            } => {
                if let (Some(id), Some(val)) = (current.as_ref(), attr(&attrs, "w:val")) {
                    if let Ok(lvl) = val.parse::<u8>() {
                        out.entry(id.clone()).or_default().outline_lvl = Some(lvl);
                    }
                }
            }
            XmlEvent::End("w:style") => current = None,
            _ => {}
        }
    }
    Ok(out)
}
