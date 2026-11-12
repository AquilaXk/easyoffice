//! Shared character format and run helpers of the DOCX, HWPX, and ODT readers. The XML event
//! reader lives in [`crate::xml_stream`].

use crate::rich_document::{RichToc, MAX_TOC_LEVEL};
use crate::{RichRun, RichScript};

/// Boolean OOXML-style toggle value: absent or anything but `0`/`false`/`off`/`none` is true.
pub(crate) fn toggle_on(value: Option<&str>) -> bool {
    !matches!(
        value.map(str::trim),
        Some("0" | "false" | "off" | "none" | "NONE")
    )
}

/// Character format shared by every reader: the four toggles plus size, font, and colors.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CharFmt {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub size_half_pt: Option<u16>,
    pub font: Option<String>,
    pub color: Option<String>,
    pub highlight: Option<String>,
    pub script: RichScript,
}

impl CharFmt {
    #[inline]
    pub(crate) fn style_bits(&self) -> u8 {
        let script_bits = match self.script {
            RichScript::Normal => 0,
            RichScript::Superscript => 1 << 4,
            RichScript::Subscript => 2 << 4,
        };
        u8::from(self.bold)
            | (u8::from(self.italic) << 1)
            | (u8::from(self.underline) << 2)
            | (u8::from(self.strike) << 3)
            | script_bits
    }

    #[inline]
    fn matches_run(&self, run: &RichRun) -> bool {
        let run_script_bits = match run.script {
            RichScript::Normal => 0,
            RichScript::Superscript => 1 << 4,
            RichScript::Subscript => 2 << 4,
        };
        let run_bits = u8::from(run.bold)
            | (u8::from(run.italic) << 1)
            | (u8::from(run.underline) << 2)
            | (u8::from(run.strike) << 3)
            | run_script_bits;
        self.style_bits() == run_bits
            && self.size_half_pt == run.size_half_pt
            && self.font == run.font
            && self.color == run.color
            && self.highlight == run.highlight
    }
}

/// Appends `text` as a run, merging it into the previous run when the format is identical.
pub(crate) fn push_run(runs: &mut Vec<RichRun>, text: &str, fmt: &CharFmt) {
    push_linked_run(runs, text, fmt, None);
}

/// [`push_run`] for text inside a hyperlink: runs merge only when they share the same link.
pub(crate) fn push_linked_run(
    runs: &mut Vec<RichRun>,
    text: &str,
    fmt: &CharFmt,
    link: Option<&str>,
) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = runs.last_mut() {
        if fmt.matches_run(last) && last.link.as_deref() == link {
            last.text.push_str(text);
            return;
        }
    }
    runs.push(RichRun {
        link: link.map(str::to_string),
        text: text.to_string(),
        bold: fmt.bold,
        italic: fmt.italic,
        underline: fmt.underline,
        strike: fmt.strike,
        size_half_pt: fmt.size_half_pt,
        font: fmt.font.clone(),
        color: fmt.color.clone(),
        highlight: fmt.highlight.clone(),
        script: fmt.script,
    });
}

/// Table-of-contents role from a style name or id: `TOC 2`, `TOC2`, `toc 2`, `Contents_20_2`, `Contents 2`
/// (entry level 1–9) and `TOC Heading`, `Contents_20_Heading` (the block title).
pub(crate) fn toc_role_from_name(name: &str) -> Option<RichToc> {
    let s = name.trim();
    let rest = ["toc", "contents"].iter().find_map(|p| {
        s.get(..p.len())
            .filter(|h| h.eq_ignore_ascii_case(p))
            .map(|_| &s[p.len()..])
    })?;
    let tail = rest
        .trim_start_matches(|c: char| c.is_whitespace() || c == '_')
        .trim_start_matches("20")
        .trim_matches(|c: char| c.is_whitespace() || c == '_');
    if tail.eq_ignore_ascii_case("heading") {
        return Some(RichToc::Title);
    }
    tail.parse::<u8>()
        .ok()
        .filter(|n| (1..=MAX_TOC_LEVEL).contains(n))
        .map(RichToc::Entry)
}

/// Heading level from a style name such as `heading 2`, `Heading2`, `제목 3`, `개요 1`, `Title`.
pub(crate) fn heading_level_from_name(name: &str) -> u8 {
    let s = name.trim_matches(|c: char| c.is_whitespace() || c == '_' || c == '-');
    if s.eq_ignore_ascii_case("title") {
        return 1;
    }
    let rest = if s
        .get(..7)
        .is_some_and(|p| p.eq_ignore_ascii_case("heading") || p.eq_ignore_ascii_case("outline"))
    {
        Some(&s[7..])
    } else {
        s.strip_prefix("제목").or_else(|| s.strip_prefix("개요"))
    };
    if let Some(tail) = rest {
        let digits = tail.trim_matches(|c: char| c.is_whitespace() || c == '_' || c == '-');
        if let Ok(n) = digits.parse::<u8>() {
            return n.clamp(1, 6);
        }
    }
    0
}
