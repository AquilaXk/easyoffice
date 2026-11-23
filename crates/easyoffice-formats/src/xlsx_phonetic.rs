//! ECMA-376 SpreadsheetML phonetic guides (`<rPh>` / `<phoneticPr>`): streaming reader, bounded
//! normalization, kana mode conversion, and XML writer (PD-319).
//!
//! 참조 구현 (reference implementation): calamine 0.38.0 (MIT, Copyright (c) 2016 Johann Tuffe),
//! https://github.com/tafia/calamine, `src/xlsx/mod.rs` (`read_shared_strings` at line 350).

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write as _;

use crate::xml_stream::{unescape_cow, Attrs, XmlEvent, XmlReader};
use crate::{EasyOfficeError, XmlSanitizer};

pub const MAX_PHONETIC_RUNS_PER_CELL: usize = 256;
pub const MAX_PHONETIC_TEXT_BYTES: usize = 32_767 * 4;

/// Phonetic character conversion mode (`ST_PhoneticType`, ECMA-376 §18.18.55).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PhoneticKind {
    FullwidthKatakana,
    HalfwidthKatakana,
    Hiragana,
    #[default]
    NoConversion,
}

impl PhoneticKind {
    pub(crate) fn from_xlsx(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("halfwidthKatakana") => Self::HalfwidthKatakana,
            Some("Hiragana" | "hiragana") => Self::Hiragana,
            Some("noConversion") => Self::NoConversion,
            _ => Self::FullwidthKatakana,
        }
    }

    pub(crate) const fn xlsx_type_attr(self) -> Option<&'static str> {
        match self {
            Self::FullwidthKatakana => None,
            Self::HalfwidthKatakana => Some("halfwidthKatakana"),
            Self::Hiragana => Some("Hiragana"),
            Self::NoConversion => Some("noConversion"),
        }
    }

    pub const fn tag(self) -> &'static str {
        match self {
            Self::FullwidthKatakana => "fk",
            Self::HalfwidthKatakana => "hk",
            Self::Hiragana => "h",
            Self::NoConversion => "nc",
        }
    }

    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "fk" => Some(Self::FullwidthKatakana),
            "hk" => Some(Self::HalfwidthKatakana),
            "h" => Some(Self::Hiragana),
            "nc" => Some(Self::NoConversion),
            _ => None,
        }
    }
}

/// Phonetic alignment (`ST_PhoneticAlignment`, ECMA-376 §18.18.54).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PhoneticAlign {
    #[default]
    NoControl,
    Left,
    Center,
    Distributed,
}

impl PhoneticAlign {
    pub(crate) fn from_xlsx(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("left") => Self::Left,
            Some("center") => Self::Center,
            Some("distributed") => Self::Distributed,
            _ => Self::NoControl,
        }
    }

    pub(crate) const fn xlsx_attr(self) -> Option<&'static str> {
        match self {
            Self::NoControl => None,
            Self::Left => Some("left"),
            Self::Center => Some("center"),
            Self::Distributed => Some("distributed"),
        }
    }

    pub const fn tag(self) -> &'static str {
        match self {
            Self::NoControl => "nc",
            Self::Left => "l",
            Self::Center => "c",
            Self::Distributed => "d",
        }
    }

    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "nc" => Some(Self::NoControl),
            "l" => Some(Self::Left),
            "c" => Some(Self::Center),
            "d" => Some(Self::Distributed),
            _ => None,
        }
    }
}

/// One `<rPh sb="..." eb="..."><t>...</t></rPh>` span in UTF-16 code units of base cell text.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PhoneticRun {
    pub sb: u32,
    pub eb: u32,
    pub text: String,
}

/// Validated phonetic metadata bound to a specific base text string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CellPhonetic {
    pub base: String,
    pub runs: Vec<PhoneticRun>,
    pub kind: PhoneticKind,
    pub align: PhoneticAlign,
}

impl CellPhonetic {
    /// Validates non-overlapping UTF-16 boundary runs against `base` and drops malformed entries.
    pub fn new(
        base: String,
        mut runs: Vec<PhoneticRun>,
        kind: PhoneticKind,
        align: PhoneticAlign,
    ) -> Option<Self> {
        if base.is_empty() || runs.is_empty() {
            return None;
        }
        if runs.windows(2).any(|w| w[0].sb > w[1].sb) {
            runs.sort_by_key(|r| (r.sb, r.eb));
        }
        let mut kept = Vec::with_capacity(runs.len().min(MAX_PHONETIC_RUNS_PER_CELL));
        let mut chars = base.chars();
        let mut pos = 0u32;
        let mut prev_eb = 0u32;
        let mut total_bytes = 0usize;
        for run in runs {
            if run.sb >= run.eb || run.sb < prev_eb {
                continue;
            }
            let mut probe = chars.clone();
            let mut p_pos = pos;
            if !advance_to_utf16(&mut probe, &mut p_pos, run.sb) {
                continue;
            }
            let mut end_probe = probe.clone();
            let mut end_pos = p_pos;
            if !advance_to_utf16(&mut end_probe, &mut end_pos, run.eb) {
                continue;
            }
            total_bytes = total_bytes.saturating_add(run.text.len());
            if total_bytes > MAX_PHONETIC_TEXT_BYTES {
                break;
            }
            chars = end_probe;
            pos = end_pos;
            prev_eb = run.eb;
            kept.push(run);
            if kept.len() >= MAX_PHONETIC_RUNS_PER_CELL {
                break;
            }
        }
        (!kept.is_empty()).then_some(Self {
            base,
            runs: kept,
            kind,
            align,
        })
    }

    #[inline]
    pub fn matches_base(&self, raw: &str) -> bool {
        !raw.is_empty() && self.base == raw
    }

    #[inline]
    pub fn byte_len(&self) -> usize {
        self.base.len() + self.runs.iter().map(|r| r.text.len() + 16).sum::<usize>()
    }

    /// Evaluates the combined `PHONETIC(...)` reading by substituting `<rPh>` spans and converting
    /// kana according to [`Self::kind`].
    pub fn reading(&self) -> String {
        let mut out = String::with_capacity(self.base.len().saturating_mul(2));
        let mut u16_pos = 0u32;
        let mut run_idx = 0usize;
        let mut span_start: Option<usize> = None;
        let mut iter = self.base.char_indices().peekable();
        while let Some(&(byte_idx, _ch)) = iter.peek() {
            while let Some(run) = self.runs.get(run_idx) {
                if u16_pos < run.sb {
                    break;
                }
                if u16_pos == run.sb {
                    if let Some(start) = span_start.take() {
                        push_converted_kana(&mut out, &self.base[start..byte_idx], self.kind);
                    }
                    push_converted_kana(&mut out, &run.text, self.kind);
                    while let Some(&(_, inner)) = iter.peek() {
                        if u16_pos >= run.eb {
                            break;
                        }
                        u16_pos += inner.len_utf16() as u32;
                        iter.next();
                    }
                    run_idx += 1;
                    continue;
                }
                run_idx += 1;
            }
            let Some(&(next_byte, next_ch)) = iter.peek() else {
                break;
            };
            if self.runs.get(run_idx).is_some_and(|r| u16_pos == r.sb) {
                continue;
            }
            span_start.get_or_insert(next_byte);
            u16_pos += next_ch.len_utf16() as u32;
            iter.next();
            if out.len() > MAX_PHONETIC_TEXT_BYTES {
                break;
            }
        }
        if let Some(start) = span_start {
            push_converted_kana(&mut out, &self.base[start..], self.kind);
        }
        out
    }
}

fn advance_to_utf16(chars: &mut std::str::Chars<'_>, pos: &mut u32, target: u32) -> bool {
    while *pos < target {
        let Some(ch) = chars.next() else {
            return false;
        };
        let Some(next) = pos.checked_add(ch.len_utf16() as u32) else {
            return false;
        };
        *pos = next;
    }
    *pos == target
}

/// Streaming collector for `<rPh>` and `<phoneticPr>` inside one `<si>` or `<is>` element.
#[derive(Default)]
pub(crate) struct PhoneticScan {
    runs: Vec<PhoneticRun>,
    seen_rph: usize,
    active: Option<(u32, u32)>,
    run_text: String,
    in_t: bool,
    total_bytes: usize,
    saw_pr: bool,
    kind: PhoneticKind,
    align: PhoneticAlign,
}

impl PhoneticScan {
    pub(crate) fn is_empty(&self) -> bool {
        self.runs.is_empty() && !self.saw_pr
    }

    pub(crate) fn clear(&mut self) {
        self.runs.clear();
        self.seen_rph = 0;
        self.active = None;
        self.run_text.clear();
        self.in_t = false;
        self.total_bytes = 0;
        self.saw_pr = false;
        self.kind = PhoneticKind::NoConversion;
        self.align = PhoneticAlign::NoControl;
    }

    pub(crate) fn start_rph(&mut self, attrs: &Attrs<'_>) -> Result<(), EasyOfficeError> {
        self.seen_rph += 1;
        if self.seen_rph > MAX_PHONETIC_RUNS_PER_CELL {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "XLSX 셀당 후리가나(rPh) 실행 구간 수가 허용 상한을 초과했습니다.".to_string(),
            ));
        }
        let (mut sb, mut eb) = (None, None);
        for (name, value) in attrs.iter_raw() {
            let plain = name.strip_prefix("eo:").unwrap_or(name);
            match plain {
                "sb" => sb = unescape_cow(value).trim().parse::<u32>().ok(),
                "eb" => eb = unescape_cow(value).trim().parse::<u32>().ok(),
                _ => {}
            }
        }
        self.run_text.clear();
        self.in_t = false;
        self.active = sb.zip(eb).filter(|&(s, e)| s < e);
        Ok(())
    }

    pub(crate) fn start_t(&mut self) {
        self.in_t = true;
    }

    pub(crate) fn push_text(&mut self, text: &str) -> Result<(), EasyOfficeError> {
        if !self.in_t {
            return Ok(());
        }
        let next = self.total_bytes.saturating_add(text.len());
        if next > MAX_PHONETIC_TEXT_BYTES {
            return Err(EasyOfficeError::ResourceBudgetExceeded(
                "XLSX 셀 후리가나(rPh) 누적 바이트가 허용 상한을 초과했습니다.".to_string(),
            ));
        }
        self.total_bytes = next;
        if self.active.is_some() {
            self.run_text.push_str(text);
        }
        Ok(())
    }

    pub(crate) fn end_t(&mut self) {
        self.in_t = false;
    }

    pub(crate) fn end_rph(&mut self) {
        self.in_t = false;
        if let Some((sb, eb)) = self.active.take() {
            let raw = std::mem::take(&mut self.run_text);
            let decoded = decode_xlsx_escapes(Cow::Owned(raw));
            self.runs.push(PhoneticRun {
                sb,
                eb,
                text: decoded,
            });
        } else {
            self.run_text.clear();
        }
    }

    pub(crate) fn on_phonetic_pr(&mut self, attrs: &Attrs<'_>) {
        self.saw_pr = true;
        let (mut kind_attr, mut align_attr) = (None, None);
        for (name, value) in attrs.iter_raw() {
            let plain = name.strip_prefix("eo:").unwrap_or(name);
            match plain {
                "type" => kind_attr = Some(unescape_cow(value)),
                "alignment" => align_attr = Some(unescape_cow(value)),
                _ => {}
            }
        }
        self.kind = PhoneticKind::from_xlsx(kind_attr.as_deref());
        self.align = PhoneticAlign::from_xlsx(align_attr.as_deref());
    }

    pub(crate) fn finish(self, base: &str) -> Option<CellPhonetic> {
        if self.runs.is_empty() || base.is_empty() {
            return None;
        }
        let kind = if self.saw_pr {
            self.kind
        } else {
            PhoneticKind::NoConversion
        };
        CellPhonetic::new(base.to_string(), self.runs, kind, self.align)
    }

    pub(crate) fn finish_reset(&mut self, base: &str) -> Option<CellPhonetic> {
        let taken = std::mem::take(self);
        taken.finish(base)
    }
}

/// Appends `<si><t...>...</t>...</si>` (including `<rPh>` and `<phoneticPr>` when valid).
pub(crate) fn push_si_xml(out: &mut String, raw: &str, phonetic: Option<&CellPhonetic>) {
    let valid_ph = phonetic.filter(|ph| ph.matches_base(raw));
    out.push_str("<si>");
    push_text_elem(out, raw, false);
    if let Some(ph) = valid_ph {
        push_phonetic_elements(out, ph);
    }
    out.push_str("</si>");
}

/// Appends ` t="inlineStr"><is><t xml:space="preserve">...</t>...</is></c>`.
pub(crate) fn push_inline_is_xml(out: &mut String, raw: &str, phonetic: Option<&CellPhonetic>) {
    let valid_ph = phonetic.filter(|ph| ph.matches_base(raw));
    out.push_str(" t=\"inlineStr\"><is>");
    push_text_elem(out, raw, true);
    if let Some(ph) = valid_ph {
        push_phonetic_elements(out, ph);
    }
    out.push_str("</is></c>");
}

fn push_text_elem(out: &mut String, s: &str, always_preserve: bool) {
    let preserve = always_preserve
        || s.starts_with(char::is_whitespace)
        || s.ends_with(char::is_whitespace)
        || s.contains(['\n', '\r']);
    if preserve {
        out.push_str("<t xml:space=\"preserve\">");
    } else {
        out.push_str("<t>");
    }
    out.push_str(&XmlSanitizer::escape(s));
    out.push_str("</t>");
}

fn push_phonetic_elements(out: &mut String, ph: &CellPhonetic) {
    for run in &ph.runs {
        let _ = write!(out, "<rPh sb=\"{}\" eb=\"{}\">", run.sb, run.eb);
        push_text_elem(out, &run.text, false);
        out.push_str("</rPh>");
    }
    out.push_str("<phoneticPr fontId=\"0\"");
    if let Some(ty) = ph.kind.xlsx_type_attr() {
        let _ = write!(out, " type=\"{ty}\"");
    }
    if let Some(align) = ph.align.xlsx_attr() {
        let _ = write!(out, " alignment=\"{align}\"");
    }
    out.push_str("/>");
}

/// Decodes ECMA-376 `_xHHHH_` UTF-16 escape sequences (including surrogate pairs) in XLSX strings.
pub(crate) fn decode_xlsx_escapes(s: Cow<'_, str>) -> String {
    if !s.as_bytes().contains(&b'_') || !s.contains("_x") {
        return s.into_owned();
    }
    let mut out = String::with_capacity(s.len());
    decode_xlsx_escapes_into(&s, &mut out);
    out
}

/// Appends `s` to `out` with its `_xHHHH_` escapes decoded.
pub(crate) fn decode_xlsx_escapes_into(s: &str, out: &mut String) {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut high_surrogate: Option<u16> = None;
    while i < bytes.len() {
        if i + 7 <= bytes.len() && bytes[i] == b'_' && bytes[i + 1] == b'x' && bytes[i + 6] == b'_'
        {
            if let Ok(hex_str) = std::str::from_utf8(&bytes[i + 2..i + 6]) {
                if let Ok(unit) = u16::from_str_radix(hex_str, 16) {
                    if (0xD800..=0xDBFF).contains(&unit) {
                        if high_surrogate.replace(unit).is_some() {
                            out.push(char::REPLACEMENT_CHARACTER);
                        }
                        i += 7;
                        continue;
                    } else if (0xDC00..=0xDFFF).contains(&unit) {
                        if let Some(hi) = high_surrogate.take() {
                            let cp = 0x10000
                                + (((u32::from(hi) - 0xD800) << 10) | (u32::from(unit) - 0xDC00));
                            out.push(char::from_u32(cp).unwrap_or(char::REPLACEMENT_CHARACTER));
                            i += 7;
                            continue;
                        }
                    } else {
                        if high_surrogate.take().is_some() {
                            out.push(char::REPLACEMENT_CHARACTER);
                        }
                        out.push(
                            char::from_u32(u32::from(unit)).unwrap_or(char::REPLACEMENT_CHARACTER),
                        );
                        i += 7;
                        continue;
                    }
                }
            }
        }
        if high_surrogate.take().is_some() {
            out.push(char::REPLACEMENT_CHARACTER);
        }
        let ch = s[i..].chars().next().unwrap_or(char::REPLACEMENT_CHARACTER);
        out.push(ch);
        i += ch.len_utf8();
    }
    if high_surrogate.is_some() {
        out.push(char::REPLACEMENT_CHARACTER);
    }
}

/// Half-width katakana block `U+FF61..=U+FF9F` mapped to full-width equivalents.
const HALF_KANA_FULL: [char; 63] = [
    '\u{3002}', '\u{300C}', '\u{300D}', '\u{3001}', '\u{30FB}', '\u{30F2}', '\u{30A1}', '\u{30A3}',
    '\u{30A5}', '\u{30A7}', '\u{30A9}', '\u{30E3}', '\u{30E5}', '\u{30E7}', '\u{30C3}', '\u{30FC}',
    '\u{30A2}', '\u{30A4}', '\u{30A6}', '\u{30A8}', '\u{30AA}', '\u{30AB}', '\u{30AD}', '\u{30AF}',
    '\u{30B1}', '\u{30B3}', '\u{30B5}', '\u{30B7}', '\u{30B9}', '\u{30BB}', '\u{30BD}', '\u{30BF}',
    '\u{30C1}', '\u{30C4}', '\u{30C6}', '\u{30C8}', '\u{30CA}', '\u{30CB}', '\u{30CC}', '\u{30CD}',
    '\u{30CE}', '\u{30CF}', '\u{30D2}', '\u{30D5}', '\u{30D8}', '\u{30DB}', '\u{30DE}', '\u{30DF}',
    '\u{30E0}', '\u{30E1}', '\u{30E2}', '\u{30E4}', '\u{30E6}', '\u{30E8}', '\u{30E9}', '\u{30EA}',
    '\u{30EB}', '\u{30EC}', '\u{30ED}', '\u{30EF}', '\u{30F3}', '\u{309B}', '\u{309C}',
];

fn combine_dakuten(prev: char) -> Option<char> {
    match prev {
        '\u{FF73}' => Some('\u{30F4}'), // ｳ + ﾞ -> ヴ
        '\u{FF76}'..='\u{FF84}' => HALF_KANA_FULL
            .get((prev as usize) - 0xFF61)
            .and_then(|&full| char::from_u32(full as u32 + 1)), // ｶ..ﾄ -> ガ..ド
        '\u{FF8A}'..='\u{FF8E}' => HALF_KANA_FULL
            .get((prev as usize) - 0xFF61)
            .and_then(|&full| char::from_u32(full as u32 + 1)), // ﾊ..ﾎ -> バ..ボ
        '\u{FF9C}' => Some('\u{30F7}'), // ﾜ + ﾞ -> ヷ
        '\u{FF66}' => Some('\u{30FA}'), // ｦ + ﾞ -> ヺ
        _ => None,
    }
}

fn combine_handakuten(prev: char) -> Option<char> {
    if ('\u{FF8A}'..='\u{FF8E}').contains(&prev) {
        HALF_KANA_FULL
            .get((prev as usize) - 0xFF61)
            .and_then(|&full| char::from_u32(full as u32 + 2))
    } else {
        None
    }
}

fn push_converted_kana(out: &mut String, input: &str, kind: PhoneticKind) {
    if kind == PhoneticKind::NoConversion || input.is_ascii() {
        out.push_str(input);
        return;
    }
    let mut full = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        let cp = c as u32;
        if (0x3041..=0x3096).contains(&cp) {
            full.push(char::from_u32(cp + 0x60).unwrap_or(c));
        } else if cp == 0x309D {
            full.push('\u{30FD}');
        } else if cp == 0x309E {
            full.push('\u{30FE}');
        } else if (0xFF65..=0xFF9D).contains(&cp) {
            if matches!(chars.peek(), Some('\u{FF9E}')) {
                if let Some(voiced) = combine_dakuten(c) {
                    chars.next();
                    full.push(voiced);
                    continue;
                }
            } else if matches!(chars.peek(), Some('\u{FF9F}')) {
                if let Some(semi) = combine_handakuten(c) {
                    chars.next();
                    full.push(semi);
                    continue;
                }
            }
            full.push(HALF_KANA_FULL[(cp - 0xFF61) as usize]);
        } else if cp == 0xFF9E {
            full.push('\u{309B}');
        } else if cp == 0xFF9F {
            full.push('\u{309C}');
        } else {
            full.push(c);
        }
    }
    match kind {
        PhoneticKind::FullwidthKatakana | PhoneticKind::NoConversion => out.push_str(&full),
        PhoneticKind::Hiragana => {
            for c in full.chars() {
                let cp = c as u32;
                if (0x30A1..=0x30F6).contains(&cp) {
                    out.push(char::from_u32(cp - 0x60).unwrap_or(c));
                } else if cp == 0x30FD {
                    out.push('\u{309D}');
                } else if cp == 0x30FE {
                    out.push('\u{309E}');
                } else {
                    out.push(c);
                }
            }
        }
        PhoneticKind::HalfwidthKatakana => {
            for c in full.chars() {
                push_halfwidth_katakana(out, c);
            }
        }
    }
}

fn push_halfwidth_katakana(out: &mut String, c: char) {
    let cp = c as u32;
    let (base_cp, mark) = match cp {
        0x30AC | 0x30AE | 0x30B0 | 0x30B2 | 0x30B4 | 0x30B6 | 0x30B8 | 0x30BA | 0x30BC | 0x30BE
        | 0x30C0 | 0x30C2 | 0x30C5 | 0x30C7 | 0x30C9 | 0x30D0 | 0x30D3 | 0x30D6 | 0x30D9
        | 0x30DC => (cp - 1, Some('\u{FF9E}')),
        0x30D1 | 0x30D4 | 0x30D7 | 0x30DA | 0x30DD => (cp - 2, Some('\u{FF9F}')),
        0x30F4 => (0x30A6, Some('\u{FF9E}')),
        0x30F7 => (0x30EF, Some('\u{FF9E}')),
        0x30FA => (0x30F2, Some('\u{FF9E}')),
        _ => (cp, None),
    };
    if let Some(pos) = HALF_KANA_FULL[4..]
        .iter()
        .position(|&f| f as u32 == base_cp)
    {
        if let Some(half) = char::from_u32(0xFF65 + pos as u32) {
            out.push(half);
            if let Some(m) = mark {
                out.push(m);
            }
            return;
        }
    }
    out.push(c);
}

/// The shared string table of `xl/sharedStrings.xml`: all strings in one arena plus optional phonetics.
#[derive(Default)]
pub(crate) struct SharedStrings {
    text: String,
    ends: Vec<usize>,
    phonetics: HashMap<usize, CellPhonetic>,
}

impl SharedStrings {
    pub(crate) fn get(&self, index: usize) -> Option<&str> {
        let end = *self.ends.get(index)?;
        let start = index.checked_sub(1).map_or(0, |prev| self.ends[prev]);
        Some(&self.text[start..end])
    }

    pub(crate) fn phonetic(&self, index: usize) -> Option<&CellPhonetic> {
        self.phonetics.get(&index)
    }

    fn close_item(&mut self, start: usize) {
        if self.text[start..].contains("_x") {
            let escaped = self.text.split_off(start);
            decode_xlsx_escapes_into(&escaped, &mut self.text);
        }
        self.ends.push(self.text.len());
    }

    #[cfg(test)]
    pub(crate) fn from_strings(strings: &[String]) -> Self {
        let mut table = Self::default();
        for s in strings {
            table.text.push_str(s);
            table.ends.push(table.text.len());
        }
        table
    }
}

const MAX_SST_ITEMS: usize = 1 << 20;
const MAX_SST_TOTAL_BYTES: usize = 128 * 1024 * 1024;

/// Reads every `si` item: base text outside `rPh` and validated `<rPh>` / `<phoneticPr>` runs.
pub(crate) fn read_shared_strings(xml: &str) -> Result<SharedStrings, EasyOfficeError> {
    let mut table = SharedStrings::default();
    let mut reader = XmlReader::unbounded(xml);
    let mut item_start: Option<usize> = None;
    let mut phonetic_depth = 0usize;
    let mut in_text = false;
    let mut ph_scan = PhoneticScan::default();
    let mut sst_ph_bytes = 0usize;
    while let Some(event) = reader.next_event()? {
        match event {
            XmlEvent::Start { name: "si", .. } if reader.last_start_was_empty() => {
                if table.ends.len() >= MAX_SST_ITEMS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "XLSX 공유 문자열(SST) 항목 수가 상한을 초과했습니다.".to_string(),
                    ));
                }
                table.ends.push(table.text.len());
            }
            XmlEvent::Start { name: "si", .. } => {
                if table.ends.len() >= MAX_SST_ITEMS {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "XLSX 공유 문자열(SST) 항목 수가 상한을 초과했습니다.".to_string(),
                    ));
                }
                item_start = Some(table.text.len());
                phonetic_depth = 0;
                ph_scan.clear();
            }
            XmlEvent::Start {
                name: "rPh",
                ref attrs,
            } if item_start.is_some() => {
                phonetic_depth += 1;
                if phonetic_depth == 1 {
                    ph_scan.start_rph(attrs)?;
                }
            }
            XmlEvent::End("rPh") => {
                if phonetic_depth == 1 {
                    ph_scan.end_rph();
                }
                phonetic_depth = phonetic_depth.saturating_sub(1);
            }
            XmlEvent::Start {
                name: "phoneticPr",
                ref attrs,
            } if item_start.is_some() && phonetic_depth == 0 => {
                ph_scan.on_phonetic_pr(attrs);
            }
            XmlEvent::Start { name: "t", .. }
                if item_start.is_some()
                    && phonetic_depth == 0
                    && !reader.last_start_was_empty() =>
            {
                in_text = true;
            }
            XmlEvent::Start { name: "t", .. }
                if item_start.is_some()
                    && phonetic_depth == 1
                    && !reader.last_start_was_empty() =>
            {
                ph_scan.start_t();
            }
            XmlEvent::End("t") => {
                in_text = false;
                ph_scan.end_t();
            }
            XmlEvent::Text(text) if in_text => {
                if table.text.len().saturating_add(text.len()) > MAX_SST_TOTAL_BYTES {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "XLSX 공유 문자열(SST) 누적 바이트가 상한(128 MiB)을 초과했습니다."
                            .to_string(),
                    ));
                }
                table.text.push_str(&text);
            }
            XmlEvent::Text(text) if item_start.is_some() && phonetic_depth == 1 => {
                sst_ph_bytes = sst_ph_bytes.saturating_add(text.len());
                if table.text.len().saturating_add(sst_ph_bytes) > MAX_SST_TOTAL_BYTES {
                    return Err(EasyOfficeError::ResourceBudgetExceeded(
                        "XLSX 공유 문자열(SST) 누적 바이트가 상한(128 MiB)을 초과했습니다."
                            .to_string(),
                    ));
                }
                ph_scan.push_text(&text)?;
            }
            XmlEvent::End("si") => {
                if let Some(start) = item_start.take() {
                    let idx = table.ends.len();
                    table.close_item(start);
                    if !ph_scan.is_empty() {
                        if let Some(ph) = table.get(idx).and_then(|base| ph_scan.finish_reset(base))
                        {
                            table.phonetics.insert(idx, ph);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(table)
}
