//! Shapes (`hp:rect`, `hp:ellipse`, `hp:polygon`, `hp:curve`, `hp:arc`, `hp:line`,
//! `hp:connectLine`, `hp:container`, `hp:textart`), captions (`hp:caption`), equations
//! (`hp:equation` -> `hp:script`), table-cell pictures (`hp:tc` -> `hp:pic`) and footnotes/endnotes
//! (`hp:ctrl` -> `hp:footNote` / `hp:endNote`) for the HWPX rich reader (`PD-247`).
//!
//! 참조 구현: `rhwp` (MIT, Copyright (c) 2025-2026 Edward Kim; see `THIRD_PARTY_NOTICES.md`),
//! `src/parser/hwpx/section.rs` (`parse_draw_text` 4464-4568행, `parse_shape_object` 4573-4690행,
//! `parse_container` 4946-5050행, `parse_caption` 2692-2745행, `parse_ctrl_footnote`·`parse_ctrl_endnote`
//! 5642-5778행, `parse_equation` 6835-6920행, `parse_note_pr_children` 1277-1545행).

use std::borrow::Cow;
use std::collections::HashMap;

use super::{push_text, Header, SectionWalker, EMU_PER_HWPUNIT};
use crate::hwp_eqedit::convert_eqedit_to_latex;
use crate::rich_image::safe_package_path;
use crate::xml_stream::{attr, Attrs, XmlEvent, XmlReader};
use crate::{
    EasyOfficeError, RichBlock, RichImage, RichParagraph, RichRun, RichScript, ZipArchiveEntry,
};

/// Binary item id -> package entry, from the `opf:item` manifest of `Contents/content.hpf`.
pub(super) struct HwpxMedia<'a> {
    pub(super) by_id: HashMap<String, &'a ZipArchiveEntry>,
}

impl<'a> HwpxMedia<'a> {
    pub(super) fn load(entries: &'a [ZipArchiveEntry]) -> Result<Self, EasyOfficeError> {
        let mut by_id = HashMap::new();
        let entry_by_path: HashMap<&str, &'a ZipArchiveEntry> =
            entries.iter().map(|e| (e.path.as_str(), e)).collect();
        let Some(&hpf) = entry_by_path.get("Contents/content.hpf") else {
            return Ok(Self { by_id });
        };
        let xml = std::str::from_utf8(&hpf.data).map_err(|_| {
            EasyOfficeError::InvalidUtf8("Contents/content.hpf UTF-8 오류".to_string())
        })?;
        for event in XmlReader::new(xml) {
            let XmlEvent::Start { name, attrs } = event? else {
                continue;
            };
            let (Some(id), Some(href)) = (attr(&attrs, "id"), attr(&attrs, "href")) else {
                continue;
            };
            let entry =
                safe_package_path(&href).and_then(|path| entry_by_path.get(path.as_str()).copied());
            if let (true, Some(entry)) = (name == "opf:item", entry) {
                by_id.insert(id.to_string(), entry);
            }
        }
        Ok(Self { by_id })
    }
}

/// Fields of one `hp:pic`.
#[derive(Default)]
struct PictureFields<'e> {
    item: Option<Cow<'e, str>>,
    size: Option<(u64, u64)>,
    alt: String,
}

fn scan_picture<'e>(events: &[XmlEvent<'e>]) -> PictureFields<'e> {
    let mut fields = PictureFields::default();
    let mut in_comment = false;
    for event in events {
        match event {
            XmlEvent::Start { name, attrs } => match *name {
                "hc:img" if fields.item.is_none() => fields.item = attr(attrs, "binaryItemIDRef"),
                "hp:curSz" | "hp:sz" if fields.size.is_none() => {
                    let side = |key| {
                        attr(attrs, key)
                            .and_then(|v| v.trim().parse::<u64>().ok())
                            .filter(|v| *v > 0)
                    };
                    fields.size = side("width").zip(side("height")).and_then(|(w, h)| {
                        Some((
                            w.checked_mul(EMU_PER_HWPUNIT)?,
                            h.checked_mul(EMU_PER_HWPUNIT)?,
                        ))
                    });
                }
                "hp:shapeComment" => in_comment = true,
                _ => {}
            },
            XmlEvent::Text(text) if in_comment => fields.alt.push_str(text),
            XmlEvent::End(name) if *name == "hp:shapeComment" => in_comment = false,
            _ => {}
        }
    }
    fields
}

pub(super) const MAX_EQUATION_BYTES: usize = 64 * 1024;
pub(super) const MAX_HWPX_NEST_DEPTH: u8 = 16;
pub(super) const MAX_HWPX_NOTES: usize = 10_000;

const EMPTY_SHAPE_WARNING: &str = "글이 없는 도형은 가져오지 않았습니다.";
pub(super) const OLE_SKIP_WARNING: &str = "OLE 포함 개체는 가져오지 않았습니다.";

const HANGUL_SYLLABLES: [char; 14] = [
    '가', '나', '다', '라', '마', '바', '사', '아', '자', '차', '카', '타', '파', '하',
];
const NOTE_SYMBOLS: [&str; 4] = ["*", "\u{2020}", "\u{2021}", "\u{00A7}"];
const ROMAN_PAIRS: [(usize, &str); 13] = [
    (1000, "m"),
    (900, "cm"),
    (500, "d"),
    (400, "cd"),
    (100, "c"),
    (90, "xc"),
    (50, "l"),
    (40, "xl"),
    (10, "x"),
    (9, "ix"),
    (5, "v"),
    (4, "iv"),
    (1, "i"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum NoteNumKind {
    #[default]
    Digit,
    CircledDigit,
    RomanUpper,
    RomanLower,
    LatinUpper,
    LatinLower,
    HangulSyllable,
    Symbol,
}

impl NoteNumKind {
    pub(super) fn from_owpml(raw: &str) -> Self {
        match raw.trim() {
            "CIRCLED_DIGIT" | "CIRCLED_NUMBER" => Self::CircledDigit,
            "ROMAN_CAPITAL" => Self::RomanUpper,
            "ROMAN_SMALL" => Self::RomanLower,
            "LATIN_CAPITAL" => Self::LatinUpper,
            "LATIN_SMALL" => Self::LatinLower,
            "HANGUL_SYLLABLE" => Self::HangulSyllable,
            "SYMBOL" | "USER_CHAR" => Self::Symbol,
            _ => Self::Digit,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct NoteFormatSpec {
    pub kind: NoteNumKind,
    pub prefix: Option<char>,
    pub suffix: Option<char>,
    pub user_char: Option<char>,
    pub restart_each_section: bool,
}

impl Default for NoteFormatSpec {
    fn default() -> Self {
        Self {
            kind: NoteNumKind::Digit,
            prefix: None,
            suffix: Some(')'),
            user_char: None,
            restart_each_section: false,
        }
    }
}

/// Decodes an OWPML decoration character attribute (`prefixChar`, `suffixChar`, `userChar`),
/// accepting either decimal UTF-16 code units (`"41"` = `')'`, `"47928"` = `'문'`) or literal
/// single-character strings (`")"`).
pub(super) fn decode_note_deco(raw: Option<&str>, default: Option<char>) -> Option<char> {
    let Some(s) = raw.map(str::trim) else {
        return default;
    };
    if s.is_empty() {
        return None;
    }
    if s.bytes().all(|b| b.is_ascii_digit()) {
        let code = s.parse::<u32>().ok()?;
        if code < 32 {
            return None;
        }
        return char::from_u32(code);
    }
    s.chars().next()
}

fn format_roman(mut n: usize, upper: bool) -> String {
    if n == 0 || n > 3_999 {
        return n.to_string();
    }
    let mut out = String::new();
    for (val, sym) in ROMAN_PAIRS {
        while n >= val {
            out.push_str(sym);
            n -= val;
        }
    }
    if upper {
        out.make_ascii_uppercase();
    }
    out
}

fn format_note_core(kind: NoteNumKind, number: usize, user_char: Option<char>) -> String {
    if let Some(uc) = user_char {
        return uc.to_string();
    }
    let n = number.max(1);
    match kind {
        NoteNumKind::Digit => n.to_string(),
        NoteNumKind::CircledDigit => {
            if (1..=20).contains(&n) {
                char::from_u32(0x2460 + (n as u32 - 1))
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| n.to_string())
            } else {
                n.to_string()
            }
        }
        NoteNumKind::RomanLower => format_roman(n, false),
        NoteNumKind::RomanUpper => format_roman(n, true),
        NoteNumKind::LatinLower => {
            let ch = (b'a' + ((n - 1) % 26) as u8) as char;
            ch.to_string()
        }
        NoteNumKind::LatinUpper => {
            let ch = (b'A' + ((n - 1) % 26) as u8) as char;
            ch.to_string()
        }
        NoteNumKind::HangulSyllable => {
            HANGUL_SYLLABLES[(n - 1) % HANGUL_SYLLABLES.len()].to_string()
        }
        NoteNumKind::Symbol => {
            let k = n - 1;
            NOTE_SYMBOLS[k % NOTE_SYMBOLS.len()].repeat((k / NOTE_SYMBOLS.len() + 1).min(32))
        }
    }
}

fn build_note_label(attrs: &Attrs<'_>, spec: &NoteFormatSpec, fallback_number: usize) -> String {
    let num = attr(attrs, "number")
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(fallback_number);
    let prefix = match attr(attrs, "prefixChar") {
        Some(raw) => decode_note_deco(Some(&raw), None),
        None => spec.prefix,
    };
    let user_char = match attr(attrs, "userChar") {
        Some(raw) => decode_note_deco(Some(&raw), None),
        None => spec.user_char,
    };
    let suffix = if attr(attrs, "userChar").is_some() && attr(attrs, "suffixChar").is_none() {
        None
    } else {
        match attr(attrs, "suffixChar") {
            Some(raw) => decode_note_deco(Some(&raw), None),
            None => spec.suffix,
        }
    };
    let mut out = String::new();
    if let Some(p) = prefix {
        out.push(p);
    }
    out.push_str(&format_note_core(spec.kind, num, user_char));
    if let Some(s) = suffix {
        out.push(s);
    }
    out
}

/// Exclusive end index of the balanced subtree starting at `events[start]`.
pub(super) fn subtree_end(events: &[XmlEvent<'_>], start: usize) -> usize {
    let mut depth = 0usize;
    for (idx, ev) in events[start..].iter().enumerate() {
        match ev {
            XmlEvent::Start { .. } => depth += 1,
            XmlEvent::End(_) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return start + idx + 1;
                }
            }
            XmlEvent::Text(_) => {}
        }
    }
    events.len()
}

pub(super) const fn is_drawing_shape(name: &str) -> bool {
    matches!(
        name.as_bytes(),
        b"hp:rect"
            | b"hp:ellipse"
            | b"hp:line"
            | b"hp:arc"
            | b"hp:polygon"
            | b"hp:curve"
            | b"hp:connectLine"
            | b"hp:container"
            | b"hp:textart"
    )
}

fn warn_once(warnings: &mut Vec<String>, note: impl Into<String>) {
    let note = note.into();
    if !warnings.iter().any(|w| w == &note) {
        warnings.push(note);
    }
}

fn plain_text_of_blocks(blocks: &[RichBlock]) -> String {
    let mut parts = Vec::new();
    for b in blocks {
        if let RichBlock::Paragraph(p) = b {
            let t = p.plain_text();
            if !t.trim().is_empty() {
                parts.push(t);
            }
        }
    }
    parts.join(" ")
}

impl SectionWalker {
    /// Walks an arbitrary `<hp:subList>` slice with `nest_depth + 1`, sharing all document budgets
    /// and note collections.
    pub(super) fn walk_sublist(
        &mut self,
        events: &[XmlEvent<'_>],
        header: &Header,
        media: &HwpxMedia<'_>,
        warnings: &mut Vec<String>,
        in_note: bool,
    ) -> Result<Vec<RichBlock>, EasyOfficeError> {
        if self.nest_depth >= MAX_HWPX_NEST_DEPTH {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "HWPX 중첩 도형·각주 깊이 한도를 넘었습니다.".to_string(),
            ));
        }
        let mut child = SectionWalker {
            budget: std::mem::take(&mut self.budget),
            text_budget: std::mem::take(&mut self.text_budget),
            footnotes: std::mem::take(&mut self.footnotes),
            endnotes: std::mem::take(&mut self.endnotes),
            footnote_spec: self.footnote_spec.clone(),
            endnote_spec: self.endnote_spec.clone(),
            next_footnote: self.next_footnote,
            next_endnote: self.next_endnote,
            total_notes: self.total_notes,
            dropped_merges: self.dropped_merges,
            nest_depth: self.nest_depth + 1,
            in_note,
            ..Default::default()
        };
        let mut out = Vec::new();
        let res = child.walk_slice(events, header, media, &mut out, warnings);
        self.budget = child.budget;
        self.text_budget = child.text_budget;
        self.footnotes = child.footnotes;
        self.endnotes = child.endnotes;
        self.next_footnote = child.next_footnote;
        self.next_endnote = child.next_endnote;
        self.total_notes = child.total_notes;
        self.dropped_merges = child.dropped_merges;
        res?;
        Ok(out)
    }

    /// Processes a balanced slice of XML events using the same dispatch rules as `read_section`.
    pub(super) fn walk_slice(
        &mut self,
        events: &[XmlEvent<'_>],
        header: &Header,
        media: &HwpxMedia<'_>,
        blocks: &mut Vec<RichBlock>,
        warnings: &mut Vec<String>,
    ) -> Result<(), EasyOfficeError> {
        let mut i = 0usize;
        while i < events.len() {
            match &events[i] {
                XmlEvent::Start { name: "hp:pic", .. } => {
                    let end = subtree_end(events, i);
                    self.on_picture(&events[i..end], header, media, blocks, warnings)?;
                    i = end;
                }
                XmlEvent::Start {
                    name: "hp:equation",
                    ..
                } => {
                    let end = subtree_end(events, i);
                    self.on_equation(&events[i..end], header, media, blocks, warnings)?;
                    i = end;
                }
                XmlEvent::Start {
                    name: "hp:ctrl", ..
                } => {
                    let end = subtree_end(events, i);
                    let sub = &events[i..end];
                    if !self.on_frame_control(sub, header)?
                        && !self.on_note_control(sub, header, media, warnings)?
                    {
                        self.on_ctrl(sub, header)?;
                    }
                    i = end;
                }
                XmlEvent::Start {
                    name: "hp:secPr", ..
                } => {
                    let end = subtree_end(events, i);
                    self.on_section_properties(&events[i..end]);
                    i = end;
                }
                XmlEvent::Start {
                    name: "hp:caption", ..
                } if self.tbl_depth == 1 && !self.in_cell => {
                    let end = subtree_end(events, i);
                    let cap_blocks =
                        self.extract_caption_blocks(&events[i..end], header, media, warnings)?;
                    self.table_captions.extend(cap_blocks);
                    i = end;
                }
                XmlEvent::Start { name, .. } if is_drawing_shape(name) => {
                    let end = subtree_end(events, i);
                    self.on_shape_or_container(&events[i..end], header, media, blocks, warnings)?;
                    i = end;
                }
                XmlEvent::Start {
                    name: "hp:ole" | "hp:switch",
                    ..
                } => {
                    let end = subtree_end(events, i);
                    warn_once(warnings, OLE_SKIP_WARNING);
                    i = end;
                }
                XmlEvent::Start { name, attrs } => {
                    self.on_start(name, attrs, header, blocks)?;
                    i += 1;
                }
                XmlEvent::Text(text) => {
                    self.on_text(text)?;
                    i += 1;
                }
                XmlEvent::End(name) => {
                    self.on_end(name, header, blocks)?;
                    i += 1;
                }
            }
        }
        Ok(())
    }

    pub(super) fn extract_caption_blocks(
        &mut self,
        caption_events: &[XmlEvent<'_>],
        header: &Header,
        media: &HwpxMedia<'_>,
        warnings: &mut Vec<String>,
    ) -> Result<Vec<RichBlock>, EasyOfficeError> {
        let mut out = Vec::new();
        let mut i = 1usize;
        let last = caption_events.len().saturating_sub(1);
        while i < last {
            if let XmlEvent::Start {
                name: "hp:subList", ..
            } = &caption_events[i]
            {
                let end = subtree_end(caption_events, i);
                if end > i + 2 {
                    out.extend(self.walk_sublist(
                        &caption_events[i + 1..end - 1],
                        header,
                        media,
                        warnings,
                        self.in_note,
                    )?);
                }
                i = end;
            } else {
                i += 1;
            }
        }
        Ok(out)
    }

    fn route_auxiliary_blocks(&mut self, aux: Vec<RichBlock>, blocks: &mut Vec<RichBlock>) {
        if aux.is_empty() {
            return;
        }
        if self.tbl_depth > 0 {
            for b in aux {
                match b {
                    RichBlock::Paragraph(p) => self.cell_paras.push(p.runs),
                    RichBlock::Table(t) => {
                        for row in t.rows {
                            self.cell_paras.push(vec![RichRun::plain(row.join("\t"))]);
                        }
                    }
                    RichBlock::Image(img) => {
                        if self.tbl_depth == 1 && self.in_cell {
                            self.cell_images.push(img);
                        } else {
                            self.table_images.push(img);
                        }
                    }
                }
            }
        } else if let Some(top) = self.paras.last_mut() {
            top.after_blocks.extend(aux);
        } else {
            blocks.extend(aux);
        }
    }

    /// Reads an `hp:pic` into the open paragraph, or into `table_images` when inside a table cell.
    pub(super) fn on_picture(
        &mut self,
        events: &[XmlEvent<'_>],
        header: &Header,
        media: &HwpxMedia<'_>,
        blocks: &mut Vec<RichBlock>,
        warnings: &mut Vec<String>,
    ) -> Result<(), EasyOfficeError> {
        let fields = scan_picture(events);
        let mut captions = Vec::new();
        let mut i = 1usize;
        let last = events.len().saturating_sub(1);
        while i < last {
            if let XmlEvent::Start {
                name: "hp:caption", ..
            } = &events[i]
            {
                let end = subtree_end(events, i);
                captions.extend(self.extract_caption_blocks(
                    &events[i..end],
                    header,
                    media,
                    warnings,
                )?);
                i = end;
            } else {
                i += 1;
            }
        }

        let entry = fields.item.as_deref().and_then(|id| media.by_id.get(id));
        let Some(entry) = entry else {
            warn_once(
                warnings,
                "찾을 수 없거나 외부에 있는 그림은 가져오지 않았습니다.",
            );
            self.route_auxiliary_blocks(captions, blocks);
            return Ok(());
        };

        self.budget.charge(entry.data.len())?;
        let alt: Cow<'_, str> = if fields.alt.trim().is_empty() && !captions.is_empty() {
            Cow::Owned(plain_text_of_blocks(&captions))
        } else {
            Cow::Borrowed(fields.alt.as_str())
        };
        let mut image = match RichImage::new(entry.data.clone(), alt.as_ref()) {
            Ok(image) => image,
            Err(err) => {
                warn_once(
                    warnings,
                    format!("그림 '{}'을 가져오지 않았습니다: {err}", entry.path),
                );
                self.route_auxiliary_blocks(captions, blocks);
                return Ok(());
            }
        };
        if let Some((w, h)) = fields.size {
            let _ = image.set_display_emu(w, h);
        }
        if self.tbl_depth == 1 && self.in_cell {
            self.cell_images.push(image);
        } else if self.tbl_depth > 0 {
            self.table_images.push(image);
        } else if let Some(top) = self.paras.last_mut() {
            top.images.push(image);
        } else {
            blocks.push(RichBlock::Image(image));
        }
        self.route_auxiliary_blocks(captions, blocks);
        Ok(())
    }

    /// Reads `<hp:equation>` (`<hp:script>` -> `$$LaTeX$$`) into the active run/paragraph/cell.
    pub(super) fn on_equation(
        &mut self,
        events: &[XmlEvent<'_>],
        header: &Header,
        media: &HwpxMedia<'_>,
        blocks: &mut Vec<RichBlock>,
        warnings: &mut Vec<String>,
    ) -> Result<(), EasyOfficeError> {
        let mut script = String::new();
        let mut in_script = false;
        let mut captions = Vec::new();
        let mut i = 1usize;
        let last = events.len().saturating_sub(1);
        while i < last {
            match &events[i] {
                XmlEvent::Start {
                    name: "hp:script", ..
                } => {
                    in_script = true;
                    i += 1;
                }
                XmlEvent::End("hp:script") => {
                    in_script = false;
                    i += 1;
                }
                XmlEvent::Text(t) if in_script => {
                    if script.len().saturating_add(t.len()) > MAX_EQUATION_BYTES {
                        return Err(EasyOfficeError::ResourceBudgetExceeded(
                            "HWPX 수식 스크립트 크기 한도를 넘었습니다.".to_string(),
                        ));
                    }
                    script.push_str(t);
                    i += 1;
                }
                XmlEvent::Start {
                    name: "hp:caption", ..
                } if !in_script => {
                    let end = subtree_end(events, i);
                    captions.extend(self.extract_caption_blocks(
                        &events[i..end],
                        header,
                        media,
                        warnings,
                    )?);
                    i = end;
                }
                _ => i += 1,
            }
        }

        let trimmed = script.trim();
        if !trimmed.is_empty() {
            let latex = convert_eqedit_to_latex(trimmed)?;
            let formatted = format!("$${latex}$$");
            if !self.runs.is_empty() {
                push_text(&mut self.runs, &formatted, &mut self.text_budget)?;
            } else if let Some(top) = self.paras.last_mut() {
                self.text_budget.charge_run(formatted.len(), "HWPX 수식")?;
                top.runs.push(RichRun::plain(formatted));
            } else {
                self.text_budget
                    .charge_block(formatted.len(), "HWPX 수식")?;
                blocks.push(RichBlock::Paragraph(RichParagraph {
                    runs: vec![RichRun::plain(formatted)],
                    ..RichParagraph::default()
                }));
            }
        }
        self.route_auxiliary_blocks(captions, blocks);
        Ok(())
    }

    fn collect_shape_blocks(
        &mut self,
        events: &[XmlEvent<'_>],
        header: &Header,
        media: &HwpxMedia<'_>,
        warnings: &mut Vec<String>,
        container_depth: u8,
    ) -> Result<Vec<RichBlock>, EasyOfficeError> {
        if container_depth > MAX_HWPX_NEST_DEPTH {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "HWPX 묶음 개체 중첩 깊이 한도를 넘었습니다.".to_string(),
            ));
        }
        let Some(XmlEvent::Start { name, attrs }) = events.first() else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        if *name == "hp:textart" {
            if let Some(txt) = attr(attrs, "text") {
                let clean_txt = txt.trim();
                if !clean_txt.is_empty() {
                    self.text_budget
                        .charge_block(clean_txt.len(), "HWPX 글맵시")?;
                    out.push(RichBlock::Paragraph(RichParagraph {
                        runs: vec![RichRun::plain(clean_txt.to_string())],
                        ..RichParagraph::default()
                    }));
                }
            }
        }

        let is_container = *name == "hp:container";
        let mut i = 1usize;
        let last = events.len().saturating_sub(1);
        while i < last {
            match &events[i] {
                XmlEvent::Start {
                    name: "hp:drawText",
                    ..
                } => {
                    let end = subtree_end(events, i);
                    let mut j = i + 1;
                    let draw_last = end.saturating_sub(1);
                    while j < draw_last {
                        if let XmlEvent::Start {
                            name: "hp:subList", ..
                        } = &events[j]
                        {
                            let sub_end = subtree_end(events, j);
                            if sub_end > j + 2 {
                                out.extend(self.walk_sublist(
                                    &events[j + 1..sub_end - 1],
                                    header,
                                    media,
                                    warnings,
                                    self.in_note,
                                )?);
                            }
                            j = sub_end;
                        } else {
                            j += 1;
                        }
                    }
                    i = end;
                }
                XmlEvent::Start {
                    name: "hp:caption", ..
                } => {
                    let end = subtree_end(events, i);
                    out.extend(self.extract_caption_blocks(
                        &events[i..end],
                        header,
                        media,
                        warnings,
                    )?);
                    i = end;
                }
                XmlEvent::Start { name: "hp:pic", .. } if is_container => {
                    let end = subtree_end(events, i);
                    self.on_picture(&events[i..end], header, media, &mut out, warnings)?;
                    i = end;
                }
                XmlEvent::Start { name: child, .. } if is_container && is_drawing_shape(child) => {
                    let end = subtree_end(events, i);
                    out.extend(self.collect_shape_blocks(
                        &events[i..end],
                        header,
                        media,
                        warnings,
                        container_depth + 1,
                    )?);
                    i = end;
                }
                _ => i += 1,
            }
        }
        Ok(out)
    }

    pub(super) fn on_shape_or_container(
        &mut self,
        events: &[XmlEvent<'_>],
        header: &Header,
        media: &HwpxMedia<'_>,
        blocks: &mut Vec<RichBlock>,
        warnings: &mut Vec<String>,
    ) -> Result<(), EasyOfficeError> {
        let shape_blocks = self.collect_shape_blocks(events, header, media, warnings, 0)?;
        if shape_blocks.is_empty() {
            warn_once(warnings, EMPTY_SHAPE_WARNING);
            return Ok(());
        }
        if self.tbl_depth > 0 {
            self.route_auxiliary_blocks(shape_blocks, blocks);
            return Ok(());
        }
        self.flush_text_before_table(header, blocks)?;
        blocks.extend(shape_blocks);
        Ok(())
    }

    /// Reads `<hp:footNote>` or `<hp:endNote>` inside `<hp:ctrl>`, inserting the superscript mark
    /// at the anchor and appending the formatted note paragraphs to `footnotes` / `endnotes`.
    pub(super) fn on_note_control(
        &mut self,
        events: &[XmlEvent<'_>],
        header: &Header,
        media: &HwpxMedia<'_>,
        warnings: &mut Vec<String>,
    ) -> Result<bool, EasyOfficeError> {
        let Some((at, is_endnote, note_attrs)) =
            events.iter().enumerate().find_map(|(idx, ev)| match ev {
                XmlEvent::Start {
                    name: name @ ("hp:footNote" | "hp:endNote"),
                    attrs,
                } => Some((idx, *name == "hp:endNote", *attrs)),
                _ => None,
            })
        else {
            return Ok(false);
        };

        self.total_notes = self.total_notes.saturating_add(1);
        if self.total_notes > MAX_HWPX_NOTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "HWPX 각주·미주 수가 한도를 넘었습니다.".to_string(),
            ));
        }

        let label = if is_endnote {
            self.next_endnote += 1;
            build_note_label(&note_attrs, &self.endnote_spec, self.next_endnote)
        } else {
            self.next_footnote += 1;
            build_note_label(&note_attrs, &self.footnote_spec, self.next_footnote)
        };

        self.flush_run(header)?;
        if let Some(top) = self.paras.last_mut() {
            self.text_budget.charge_run(label.len(), "HWPX 각주 표식")?;
            top.runs.push(RichRun {
                text: label.clone(),
                script: RichScript::Superscript,
                ..RichRun::default()
            });
        }

        let note_end = subtree_end(events, at);
        let mut note_blocks = Vec::new();
        let mut j = at + 1;
        let last = note_end.saturating_sub(1);
        while j < last {
            if let XmlEvent::Start {
                name: "hp:subList", ..
            } = &events[j]
            {
                let sub_end = subtree_end(events, j);
                if sub_end > j + 2 {
                    note_blocks.extend(self.walk_sublist(
                        &events[j + 1..sub_end - 1],
                        header,
                        media,
                        warnings,
                        true,
                    )?);
                }
                j = sub_end;
            } else {
                j += 1;
            }
        }

        let mut lead = vec![
            RichRun {
                text: label,
                script: RichScript::Superscript,
                ..RichRun::default()
            },
            RichRun::plain(" "),
        ];
        match note_blocks.first_mut() {
            Some(RichBlock::Paragraph(p)) => {
                lead.append(&mut p.runs);
                p.runs = lead;
            }
            _ => note_blocks.insert(
                0,
                RichBlock::Paragraph(RichParagraph {
                    runs: lead,
                    ..RichParagraph::default()
                }),
            ),
        }

        if is_endnote {
            self.endnotes.extend(note_blocks);
        } else {
            self.footnotes.extend(note_blocks);
        }
        Ok(true)
    }
}
