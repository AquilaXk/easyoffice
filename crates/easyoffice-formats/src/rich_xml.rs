//! Shared character format and run helpers of the DOCX, HWPX, and ODT readers. The XML event
//! reader lives in [`crate::xml_stream`].

use crate::rich_document::{RichToc, MAX_TOC_LEVEL, MAX_TOC_PAGE};
use crate::rich_notes::RichRevision;
use crate::xml_stream::{attr, Attrs};
use crate::{EasyOfficeError, RichRun, RichScript};

/// Deepest open-paragraph stack a reader keeps (a paragraph inside a text box inside a paragraph, and
/// so on). Real documents stay in single digits; a stream of unclosed paragraph tags does not (PD-262).
pub(crate) const MAX_OPEN_PARAGRAPHS: usize = 64;

/// Refuses to open another paragraph when `open` paragraphs are already open.
pub(crate) fn check_paragraph_depth(open: usize) -> Result<(), EasyOfficeError> {
    if open >= MAX_OPEN_PARAGRAPHS {
        return Err(EasyOfficeError::ResourceBudgetExceeded(format!(
            "열린 문단이 허용 깊이({MAX_OPEN_PARAGRAPHS})를 초과했습니다."
        )));
    }
    Ok(())
}

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
    pub font_east_asia: Option<String>,
    pub color: Option<String>,
    pub highlight: Option<String>,
    pub script: RichScript,
    pub letter_spacing_twips: Option<i16>,
    pub advanced: crate::CharAdvanced,
}

impl From<&RichRun> for CharFmt {
    /// The character format of a run, to merge it back through [`push_run`].
    fn from(run: &RichRun) -> Self {
        Self {
            bold: run.bold,
            italic: run.italic,
            underline: run.underline,
            strike: run.strike,
            size_half_pt: run.size_half_pt,
            font: run.font.clone(),
            font_east_asia: run.font_east_asia.clone(),
            color: run.color.clone(),
            highlight: run.highlight.clone(),
            script: run.script,
            letter_spacing_twips: run.letter_spacing_twips,
            advanced: run.advanced,
        }
    }
}

impl CharFmt {
    /// Sets the Latin and the East Asian font a file names (`None`: it names none). A file that names only one gives it to
    /// both, and a pair that is the same font is one font, so a document with a single font keeps its single value (PD-375).
    pub(crate) fn set_fonts(&mut self, latin: Option<String>, east_asia: Option<String>) {
        match (latin, east_asia) {
            (None, east) => {
                self.font = east;
                self.font_east_asia = None;
            }
            (latin, east) => {
                self.font_east_asia = east.filter(|e| Some(e) != latin.as_ref());
                self.font = latin;
            }
        }
    }

    /// [`Self::set_fonts`] on a format built with no fonts.
    pub(crate) fn with_fonts(mut self, latin: Option<String>, east_asia: Option<String>) -> Self {
        self.set_fonts(latin, east_asia);
        self
    }

    /// Sets the fonts from the attributes of one element: the first non-empty of `latin_keys` and of `east_keys`
    /// (`unquote` strips the single quotes an ODT font family may carry).
    pub(crate) fn set_fonts_from_attrs(
        &mut self,
        attrs: &Attrs<'_>,
        (latin_keys, east_keys): (&[&str], &[&str]),
        unquote: bool,
    ) {
        let named = |keys: &[&str]| {
            keys.iter().find_map(|k| {
                let name = attr(attrs, k)?;
                let name = if unquote {
                    name.trim().trim_matches('\'')
                } else {
                    name.trim()
                };
                (!name.is_empty()).then(|| name.to_string())
            })
        };
        self.set_fonts(named(latin_keys), named(east_keys));
    }

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
            && self.font_east_asia == run.font_east_asia
            && self.color == run.color
            && self.highlight == run.highlight
            && self.letter_spacing_twips == run.letter_spacing_twips
            && self.advanced == run.advanced
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
    push_annotated_run(runs, text, fmt, link, None, None);
}

/// [`push_linked_run`] preserving `comment_id` and `revision`; runs never merge across distinct
/// `note_ref`, `comment_id`, or `revision` boundaries.
pub(crate) fn push_annotated_run(
    runs: &mut Vec<RichRun>,
    text: &str,
    fmt: &CharFmt,
    link: Option<&str>,
    comment_id: Option<u32>,
    revision: Option<&RichRevision>,
) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = runs.last_mut() {
        if last.field.is_none()
            && last.note_ref.is_none()
            && last.comment_id == comment_id
            && last.revision.as_ref() == revision
            && fmt.matches_run(last)
            && last.link.as_deref() == link
        {
            last.text.push_str(text);
            return;
        }
    }
    runs.push(RichRun {
        field: None,
        link: link.map(str::to_string),
        text: text.to_string(),
        bold: fmt.bold,
        italic: fmt.italic,
        underline: fmt.underline,
        strike: fmt.strike,
        size_half_pt: fmt.size_half_pt,
        font: fmt.font.clone(),
        font_east_asia: fmt.font_east_asia.clone(),
        font_face: None,
        color: fmt.color.clone(),
        highlight: fmt.highlight.clone(),
        script: fmt.script,
        letter_spacing_twips: fmt.letter_spacing_twips,
        advanced: fmt.advanced,
        note_ref: None,
        comment_id,
        revision: revision.cloned(),
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
    // `Contents_20_2` is "Contents 2" with ODF's `_20_` for the space; a trailing word (`Plain`) is a variant.
    let tail = rest
        .split(|c: char| c.is_whitespace() || c == '_')
        .find(|t| !t.is_empty() && *t != "20")?;
    if tail.eq_ignore_ascii_case("heading") {
        return Some(RichToc::Title);
    }
    tail.parse::<u8>()
        .ok()
        .filter(|n| (1..=MAX_TOC_LEVEL).contains(n))
        .map(RichToc::Entry)
}

/// Splits a trailing `<tab><digits>` (the page number of a table-of-contents entry, 1 to 7 digits) off the
/// runs and returns the number.
pub(crate) fn take_toc_page(runs: &mut Vec<RichRun>) -> Option<u32> {
    let text: String = runs.iter().map(|r| r.text.as_str()).collect();
    let tab = text.rfind('\t')?;
    let digits = &text[tab + 1..];
    let page = digits
        .parse::<u32>()
        .ok()
        .filter(|n| (1..=MAX_TOC_PAGE).contains(n) && digits.bytes().all(|b| b.is_ascii_digit()))?;
    let mut drop = text[tab..].chars().count();
    while drop > 0 {
        let last = runs.last_mut()?;
        let len = last.text.chars().count();
        if len <= drop {
            drop -= len;
            runs.pop();
        } else {
            let keep: String = last.text.chars().take(len - drop).collect();
            last.text = keep;
            drop = 0;
        }
    }
    Some(page)
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
