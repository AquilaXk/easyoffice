//! The defaults of an RTF (PD-361): the Normal style of the style sheet over `\*\defpap` and
//! `\*\defchp`, the `\deff` typeface of the font table and `\deftab`. The scan reads the header
//! only and stops at the first paragraph; it keeps no text and never recurses.

use std::collections::HashMap;

use super::MAX_GROUP_DEPTH;
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
    FontTbl,
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
    /// The name holds `\'hh` bytes of a code page: it is dropped rather than read wrong.
    name_has_code_page_bytes: bool,
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
            Dest::Other | Dest::FontTbl | Dest::Stylesheet => None,
        }
    }

    fn end_font_entry(&mut self) {
        let name = self.font_name.trim().to_string();
        self.font_name.clear();
        let undecodable = std::mem::take(&mut self.name_has_code_page_bytes);
        if !name.is_empty() && !undecodable && self.fonts.len() < MAX_FONTS {
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
                    Dest::FontTbl => Dest::FontTbl,
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
                if dest == Dest::FontTbl {
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
                    i += match next {
                        b'\'' => 4,
                        _ => 2,
                    };
                    if dest == Dest::FontTbl {
                        match next {
                            b'\'' => header.name_has_code_page_bytes = true,
                            b'\\' | b'{' | b'}' if header.font_name.len() < MAX_FONT_NAME_BYTES => {
                                header.font_name.push(next as char);
                            }
                            _ => {}
                        }
                    }
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
                if dest == Dest::FontTbl && word == "u" {
                    // `\uN?`: a UTF-16 unit and one fallback character
                    let unit = param.map(|v| v as u16);
                    if let Some(ch) = unit.and_then(|u| char::from_u32(u32::from(u))) {
                        if header.font_name.len() < MAX_FONT_NAME_BYTES {
                            header.font_name.push(ch);
                        }
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
                    (Dest::FontTbl, "f") => {
                        header.end_font_entry();
                        header.font_idx = param.unwrap_or(0);
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
                if dest == Dest::FontTbl {
                    let rest = &text[i..];
                    let ch = rest.chars().next().unwrap_or(' ');
                    if ch == ';' {
                        header.end_font_entry();
                    } else if header.font_name.len() < MAX_FONT_NAME_BYTES {
                        header.font_name.push(ch);
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
