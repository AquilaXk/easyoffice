//! The defaults of an RTF (PD-363): the Normal style of the style sheet over `\*\defpap` and
//! `\*\defchp`, the `\deff` typeface of the font table and `\deftab`. The scan reads the header
//! only and stops at the first paragraph; it keeps no text and never recurses.

use std::collections::HashMap;

use super::{DEFAULT_CODE_PAGE, MAX_GROUP_DEPTH};
use crate::code_pages::CodePage;
use crate::rich_defaults::RichDefaults;
use crate::rich_document::{RichAlignment, MAX_PARA_SPACING_TWIPS};

const MAX_TAB_STOP_TWIPS: u32 = 31_680;
const MAX_HALF_POINTS: u16 = 3_276;
const MAX_LINE_240THS: u32 = 2_400;
/// Typeface names a font table may hold before the scan ignores the rest.
const MAX_FONTS: usize = 1_024;
const MAX_FONT_NAME_BYTES: usize = 256;

#[derive(Clone, Copy, PartialEq)]
enum Dest {
    /// The header group or a group the scan does not read.
    Other,
    /// The font table itself; its groups are font entries.
    FontTbl,
    /// One font entry (`{\f0 Name;}`, or the flat entries of the table). A group inside an entry
    /// (`{\*\falt Name}`, `{\*\panose ..}`) is `Other`: its text is not the name.
    FontEntry,
    Stylesheet,
    /// The Normal style: the first entry of the style sheet.
    Normal,
    DefPap,
    DefChp,
}

#[derive(Default)]
struct Layer {
    align: Option<RichAlignment>,
    before: Option<u32>,
    after: Option<u32>,
    line: Option<u32>,
    line_multiple: bool,
    size: Option<u16>,
    font: Option<i32>,
}

impl Layer {
    fn word(&mut self, word: &str, param: Option<i32>) {
        let twips = || {
            param
                .and_then(|v| u32::try_from(v).ok())
                .map(|v| v.min(MAX_PARA_SPACING_TWIPS))
        };
        match word {
            "ql" => self.align = Some(RichAlignment::Left),
            "qc" => self.align = Some(RichAlignment::Center),
            "qr" => self.align = Some(RichAlignment::Right),
            "qj" => self.align = Some(RichAlignment::Justify),
            "sb" => self.before = twips(),
            "sa" => self.after = twips(),
            "sl" => {
                self.line = param
                    .and_then(|v| u32::try_from(v).ok())
                    .filter(|v| *v > 0)
                    .map(|v| v.min(MAX_LINE_240THS));
            }
            "slmult" => self.line_multiple = param != Some(0),
            "fs" => {
                self.size = param
                    .and_then(|v| u16::try_from(v).ok())
                    .filter(|v| (1..=MAX_HALF_POINTS).contains(v));
            }
            "f" => self.font = param,
            _ => {}
        }
    }

    fn over(self, base: Self) -> Self {
        Self {
            align: self.align.or(base.align),
            before: self.before.or(base.before),
            after: self.after.or(base.after),
            // a line height counts as 240ths of a line only together with `\slmult1`
            line: if self.line.is_some() {
                self.line.filter(|_| self.line_multiple)
            } else {
                base.line
            },
            line_multiple: self.line_multiple || base.line_multiple,
            size: self.size.or(base.size),
            font: self.font.or(base.font),
        }
    }
}

struct Group {
    dest: Dest,
    first_word: bool,
}

/// What the scan collected from the header.
#[derive(Default)]
struct Header {
    fonts: HashMap<i32, String>,
    font_idx: i32,
    font_name: String,
    /// `\'hh` bytes of the name not decoded yet.
    name_bytes: Vec<u8>,
    /// `\fcharsetN` of the entry being read.
    entry_charset: Option<u8>,
    /// A byte of the name has no character in its code page: the name is dropped rather than read wrong.
    name_undecodable: bool,
    /// `\ansicpgN` of the header.
    ansi_code_page: Option<u32>,
    default_font: i32,
    tab_stop: Option<u32>,
    normal: Layer,
    def_pap: Layer,
    def_chp: Layer,
    /// A style entry of the style sheet has been seen: only the first one is the Normal style.
    saw_style_entry: bool,
}

impl Header {
    fn layer(&mut self, dest: Dest) -> Option<&mut Layer> {
        match dest {
            Dest::Normal => Some(&mut self.normal),
            Dest::DefPap => Some(&mut self.def_pap),
            Dest::DefChp => Some(&mut self.def_chp),
            Dest::Other | Dest::FontTbl | Dest::FontEntry | Dest::Stylesheet => None,
        }
    }

    /// Decodes the `\'hh` bytes collected so far with the code page of the entry's charset, or
    /// `\ansicpg`, or the engine's Korean default.
    fn flush_name_bytes(&mut self) {
        if self.name_bytes.is_empty() {
            return;
        }
        let bytes = std::mem::take(&mut self.name_bytes);
        let page = self
            .entry_charset
            .and_then(CodePage::from_rtf_charset)
            .or_else(|| CodePage::from_number(self.ansi_code_page.unwrap_or(DEFAULT_CODE_PAGE)));
        match page.map(|p| p.decode_bytes_lossy(&bytes)) {
            Some(text) if !text.contains('\u{FFFD}') => self.push_name(&text),
            _ => self.name_undecodable = true,
        }
    }

    fn push_name(&mut self, text: &str) {
        if self.font_name.len() + text.len() <= MAX_FONT_NAME_BYTES {
            self.font_name.push_str(text);
        }
    }

    fn push_name_char(&mut self, ch: char) {
        self.flush_name_bytes();
        let mut buf = [0u8; 4];
        self.push_name(ch.encode_utf8(&mut buf));
    }

    fn end_font_entry(&mut self) {
        self.flush_name_bytes();
        let name = self.font_name.trim().to_string();
        self.font_name.clear();
        self.entry_charset = None;
        let undecodable = std::mem::take(&mut self.name_undecodable);
        // a new index stops at the cap; an index already there can still be redefined
        let room = self.fonts.len() < MAX_FONTS || self.fonts.contains_key(&self.font_idx);
        if !name.is_empty() && !undecodable && room {
            self.fonts.insert(self.font_idx, name);
        }
    }
}

/// The destination a group opens with its first control word.
fn dest_of(word: &str, parent: Dest) -> Option<Dest> {
    match (word, parent) {
        ("fonttbl", _) => Some(Dest::FontTbl),
        ("stylesheet", _) => Some(Dest::Stylesheet),
        ("defpap", _) => Some(Dest::DefPap),
        ("defchp", _) => Some(Dest::DefChp),
        _ => None,
    }
}

pub(super) fn read_rtf_defaults(text: &str) -> RichDefaults {
    let bytes = text.as_bytes();
    let mut header = Header::default();
    let mut stack: Vec<Group> = vec![Group {
        dest: Dest::Other,
        first_word: false,
    }];
    let mut i = 0;
    while i < bytes.len() {
        if !text.is_char_boundary(i) {
            // an escape that ended inside a multi-byte character
            i += 1;
            continue;
        }
        let dest = stack.last().map_or(Dest::Other, |g| g.dest);
        match bytes[i] {
            b'{' => {
                if stack.len() >= MAX_GROUP_DEPTH {
                    break;
                }
                // a group inside the style sheet is a style entry; inside the font table, a font
                let child = match dest {
                    Dest::FontTbl => Dest::FontEntry,
                    Dest::Stylesheet if !header.saw_style_entry => Dest::Normal,
                    _ => Dest::Other,
                };
                stack.push(Group {
                    dest: child,
                    first_word: true,
                });
                i += 1;
            }
            b'}' => {
                if matches!(dest, Dest::FontTbl | Dest::FontEntry) {
                    header.end_font_entry();
                }
                if dest == Dest::Normal {
                    header.saw_style_entry = true;
                }
                if stack.len() > 1 {
                    stack.pop();
                }
                i += 1;
            }
            b'\\' => {
                let Some(&next) = bytes.get(i + 1) else { break };
                if !next.is_ascii_alphabetic() {
                    // `\*`, `\'hh`, `\\`: an escape is no control word; `\*` keeps the first word open
                    let escape_len = if next == b'\'' { 4 } else { 2 };
                    if dest == Dest::FontEntry {
                        match next {
                            b'\'' => {
                                let hex = text
                                    .get(i + 2..i + 4)
                                    .and_then(|h| u8::from_str_radix(h, 16).ok());
                                match hex {
                                    Some(byte) if header.name_bytes.len() < MAX_FONT_NAME_BYTES => {
                                        header.name_bytes.push(byte);
                                    }
                                    _ => header.name_undecodable = true,
                                }
                            }
                            b'\\' | b'{' | b'}' => header.push_name_char(next as char),
                            _ => {}
                        }
                    }
                    i += escape_len;
                    continue;
                }
                let start = i + 1;
                let mut end = start;
                while bytes.get(end).is_some_and(u8::is_ascii_alphabetic) {
                    end += 1;
                }
                let word = &text[start..end];
                let digits_from = end;
                if bytes.get(end) == Some(&b'-') {
                    end += 1;
                }
                while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                    end += 1;
                }
                let param = text[digits_from..end].parse::<i32>().ok();
                if bytes.get(end) == Some(&b' ') {
                    end += 1;
                }
                i = end;
                let group = stack.last_mut().expect("the stack keeps its root group");
                if dest == Dest::FontEntry && word == "u" {
                    // `\uN?`: a UTF-16 unit and one fallback character
                    let unit = param.map(|v| v as u16);
                    if let Some(ch) = unit.and_then(|u| char::from_u32(u32::from(u))) {
                        header.push_name_char(ch);
                    }
                    i += text[i..].chars().next().map_or(0, char::len_utf8);
                    continue;
                }
                let first = std::mem::replace(&mut group.first_word, false);
                if first {
                    if let Some(dest) = dest_of(word, dest) {
                        group.dest = dest;
                        continue;
                    }
                }
                if matches!(word, "pard" | "par" | "trowd") {
                    break;
                }
                match (dest, word) {
                    (Dest::Other, "deff") if stack.len() == 2 => {
                        header.default_font = param.unwrap_or(0);
                    }
                    (Dest::Other, "deftab") if stack.len() == 2 => {
                        header.tab_stop = param
                            .and_then(|v| u32::try_from(v).ok())
                            .filter(|v| (1..=MAX_TAB_STOP_TWIPS).contains(v));
                    }
                    (Dest::Other, "ansicpg") if stack.len() == 2 => {
                        header.ansi_code_page = param.and_then(|v| u32::try_from(v).ok());
                    }
                    (Dest::FontTbl, "f") => {
                        // the flat form `{\fonttbl\f0 Name;\f1 Name;}`: the entries are the table's own text
                        if let Some(entry) = stack.last_mut() {
                            entry.dest = Dest::FontEntry;
                        }
                        header.font_idx = param.unwrap_or(0);
                    }
                    (Dest::FontEntry, "f") => {
                        header.end_font_entry();
                        header.font_idx = param.unwrap_or(0);
                    }
                    (Dest::FontEntry, "fcharset") => {
                        header.entry_charset = param.and_then(|v| u8::try_from(v).ok());
                    }
                    (Dest::Normal | Dest::DefPap | Dest::DefChp, _) => {
                        // a style entry numbered other than 0 is not the Normal style
                        if dest == Dest::Normal && word == "s" && param != Some(0) {
                            if let Some(group) = stack.last_mut() {
                                group.dest = Dest::Other;
                            }
                            header.normal = Layer::default();
                            continue;
                        }
                        if let Some(layer) = header.layer(dest) {
                            layer.word(word, param);
                        }
                    }
                    _ => {}
                }
            }
            b'\r' | b'\n' => i += 1,
            _ => {
                // text: a typeface name runs to `;`
                if dest == Dest::FontEntry {
                    let ch = text[i..].chars().next().unwrap_or(' ');
                    if ch == ';' {
                        header.end_font_entry();
                    } else {
                        header.push_name_char(ch);
                    }
                    i += ch.len_utf8();
                } else {
                    i += text[i..].chars().next().map_or(1, char::len_utf8);
                }
            }
        }
    }
    finish(header)
}

fn finish(header: Header) -> RichDefaults {
    let fallback = RichDefaults::default();
    let layer = header.normal.over(header.def_pap).over(header.def_chp);
    let font = layer.font.unwrap_or(header.default_font);
    RichDefaults {
        font: header.fonts.get(&font).cloned(),
        east_asian_font: None,
        size_half_points: layer.size.unwrap_or(fallback.size_half_points),
        alignment: layer.align.map_or(fallback.alignment, |a| {
            (a != RichAlignment::Left).then_some(a)
        }),
        space_before_twips: layer.before.unwrap_or(fallback.space_before_twips),
        space_after_twips: layer.after.unwrap_or(fallback.space_after_twips),
        line_240ths: layer.line.unwrap_or(fallback.line_240ths),
        tab_stop_twips: header.tab_stop.unwrap_or(fallback.tab_stop_twips),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_normal_style_wins_over_defpap_and_the_font_comes_from_the_table() {
        let rtf = concat!(
            "{\\rtf1\\ansi\\deff1{\\fonttbl{\\f0\\fnil\\fcharset0 Calibri;}{\\f1\\fswiss Arial;}}",
            "{\\*\\defchp \\fs22}{\\*\\defpap \\sa200\\sl276\\slmult1}",
            "{\\stylesheet{\\s0\\qc\\sa0\\f0 Normal;}{\\s1 heading 1;}}\\deftab720\\pard text\\par}"
        );
        let d = read_rtf_defaults(rtf);
        assert_eq!(d.font.as_deref(), Some("Calibri"));
        assert_eq!(
            (d.size_half_points, d.space_after_twips, d.line_240ths),
            (22, 0, 276)
        );
        assert_eq!(
            (d.alignment, d.tab_stop_twips),
            (Some(RichAlignment::Center), 720)
        );
    }

    #[test]
    fn a_file_without_a_header_gets_the_format_defaults() {
        assert_eq!(
            read_rtf_defaults("{\\rtf1 hello\\par}"),
            RichDefaults::default()
        );
    }

    fn font_of(table: &str) -> Option<String> {
        read_rtf_defaults(&format!("{{\\rtf1\\ansi\\deff0{table}\\pard x\\par}}")).font
    }

    #[test]
    fn text_of_a_group_inside_a_font_entry_is_not_the_name() {
        assert_eq!(
            font_of("{\\fonttbl{\\f0\\fswiss Arial{\\*\\falt Helvetica};}}").as_deref(),
            Some("Arial")
        );
        assert_eq!(
            font_of("{\\fonttbl{\\f0\\fnil{\\*\\panose 020b0604020202020204}Calibri;}}").as_deref(),
            Some("Calibri")
        );
    }

    #[test]
    fn the_flat_font_table_form_reads_its_entries() {
        assert_eq!(
            font_of("{\\fonttbl\\f0\\fswiss Arial;\\f1\\froman Times;}").as_deref(),
            Some("Arial")
        );
    }

    #[test]
    fn code_page_bytes_of_a_name_are_decoded_with_the_charset_of_the_font() {
        // 맑은 고딕 in code page 949
        let name = "\\'b8\\'bc\\'c0\\'ba \\'b0\\'ed\\'b5\\'f1";
        assert_eq!(
            font_of(&format!("{{\\fonttbl{{\\f0\\fnil\\fcharset129 {name};}}}}")).as_deref(),
            Some("맑은 고딕")
        );
        // no charset: `\ansicpg` decides
        let rtf = format!(
            "{{\\rtf1\\ansi\\ansicpg949\\deff0{{\\fonttbl{{\\f0 {name};}}}}\\pard x\\par}}"
        );
        assert_eq!(read_rtf_defaults(&rtf).font.as_deref(), Some("맑은 고딕"));
        // a code page without a table: the name is dropped, not guessed
        let rtf = format!(
            "{{\\rtf1\\ansi\\ansicpg99999\\deff0{{\\fonttbl{{\\f0 {name};}}}}\\pard x\\par}}"
        );
        assert_eq!(read_rtf_defaults(&rtf).font, None);
    }

    #[test]
    fn a_font_table_at_the_cap_still_redefines_its_indexes_but_takes_no_new_one() {
        let mut table = String::from("{\\fonttbl");
        for i in 0..MAX_FONTS + 10 {
            table.push_str(&format!("{{\\f{i} Face{i};}}"));
        }
        // index 0 is redefined after the table is full
        table.push_str("{\\f0 Redefined;}{\\f2000 Late;}}");
        let rtf = format!("{{\\rtf1\\deff2000{table}\\pard x\\par}}");
        assert_eq!(read_rtf_defaults(&rtf).font, None);
        let rtf = format!("{{\\rtf1\\deff0{table}\\pard x\\par}}");
        assert_eq!(read_rtf_defaults(&rtf).font.as_deref(), Some("Redefined"));
    }

    #[test]
    fn only_the_header_is_read() {
        let rtf = "{\\rtf1\\deff0{\\fonttbl{\\f0 Arial;}}\\pard\\qj\\sa999 text\\par}";
        let d = read_rtf_defaults(rtf);
        assert_eq!(
            (d.font.as_deref(), d.alignment, d.space_after_twips),
            (Some("Arial"), None, 0)
        );
    }
}
