//! Text effects of the 글꼴 tab beyond bold, italic, single underline and single strikethrough (PD-376): double strikethrough,
//! small capitals, all capitals, hidden text, the underline style and colour, and the emphasis mark.
//!
//! Reference implementation: docx-rs (https://github.com/bokuweb/docx-rs, MIT, Copyright (c) 2020 bokuweb), the run property
//! elements `w:caps`, `w:dstrike`, `w:vanish` and `w:u` (`docx-core/src/documents/elements/{caps,dstrike,vanish,underline}.rs`)
//! and how its run property reader dispatches on them (`docx-core/src/reader/run_property.rs` lines 95-180). Its `w:u` keeps the
//! value as an unchecked string and drops `w:color`; here the 17 names of `ST_Underline` are an index, the colour is read, and
//! every read value is checked. The other formats' words follow their specifications: ODF `style:text-underline-*`,
//! `style:text-line-through-type`, `style:text-emphasize`, `fo:font-variant`, `fo:text-transform`, `text:display`, and the RTF
//! control words `\ul*`, `\striked`, `\caps`, `\scaps`, `\v`, `\accdot`, `\acccomma`.
//!
//! DOCX, ODT and RTF carry them (the RTF underline colour has no word here). HWPX carries the double strikethrough, the
//! underline shape and colour and the emphasis mark (`rich_char_fx_hwpx.rs`, PD-379), not the capitals or hidden text; Pages
//! drops them.

use crate::rich_xml::toggle_on;
use crate::xml_stream::{attr, Attrs};

/// The `ST_Underline` names that have a line (`single` first); the index is [`TextFx::underline_style`].
const UNDERLINE_NAMES: [&str; 17] = [
    "single",
    "words",
    "double",
    "thick",
    "dotted",
    "dottedHeavy",
    "dash",
    "dashedHeavy",
    "dashLong",
    "dashLongHeavy",
    "dotDash",
    "dashDotHeavy",
    "dotDotDash",
    "dashDotDotHeavy",
    "wave",
    "wavyHeavy",
    "wavyDouble",
];
/// ODF `(style:text-underline-style, style:text-underline-type, style:text-underline-width, skips white space)` per name.
const ODT_UNDERLINE: [(&str, &str, &str, bool); 17] = [
    ("solid", "single", "auto", false),
    ("solid", "single", "auto", true),
    ("solid", "double", "auto", false),
    ("solid", "single", "bold", false),
    ("dotted", "single", "auto", false),
    ("dotted", "single", "bold", false),
    ("dash", "single", "auto", false),
    ("dash", "single", "bold", false),
    ("long-dash", "single", "auto", false),
    ("long-dash", "single", "bold", false),
    ("dot-dash", "single", "auto", false),
    ("dot-dash", "single", "bold", false),
    ("dot-dot-dash", "single", "auto", false),
    ("dot-dot-dash", "single", "bold", false),
    ("wave", "single", "auto", false),
    ("wave", "single", "bold", false),
    ("wave", "double", "auto", false),
];
/// The RTF underline control word per name.
const RTF_UNDERLINE: [&str; 17] = [
    "ul",
    "ulw",
    "uldb",
    "ulth",
    "uld",
    "ulthd",
    "uldash",
    "ulthdash",
    "ulldash",
    "ulthldash",
    "uldashd",
    "ulthdashd",
    "uldashdd",
    "ulthdashdd",
    "ulwave",
    "ulhwave",
    "ululdbwave",
];
/// Emphasis marks: none, dot, comma, circle, dot below (`w:em`).
const EMPHASIS_DOCX: [&str; 5] = ["none", "dot", "comma", "circle", "underDot"];
const EMPHASIS_ODT: [&str; 5] = [
    "none",
    "dot above",
    "accent above",
    "circle above",
    "dot below",
];

/// Largest emphasis index and underline style index; a packed value beyond them is refused.
pub const MAX_EMPHASIS: u8 = 4;
pub const MAX_UNDERLINE_STYLE: u8 = 16;
/// Largest packed effects value: 4 flag bits, 3 emphasis bits, 5 underline style bits.
pub const MAX_PACKED_FX: u32 = (1 << 12) - 1;

const FLAG_DOUBLE_STRIKE: u32 = 1;
const FLAG_SMALL_CAPS: u32 = 2;
const FLAG_ALL_CAPS: u32 = 4;
const FLAG_HIDDEN: u32 = 8;
const EMPHASIS_SHIFT: u32 = 4;
const UNDERLINE_SHIFT: u32 = 7;

/// The effects of one run; everything is off (zero, `None`) when the run inherits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TextFx {
    pub double_strike: bool,
    pub small_caps: bool,
    pub all_caps: bool,
    pub hidden: bool,
    /// 0 none, 1 dot, 2 comma, 3 circle, 4 dot below.
    pub emphasis: u8,
    /// Index of the underline name (0 is a single line); only meaningful while the run is underlined.
    pub underline_style: u8,
    /// Underline colour as 0xRRGGBB; `None` is the text colour (자동).
    pub underline_color: Option<u32>,
}

fn index_of(names: &[&str], name: Option<&str>) -> Option<usize> {
    let name = name?.trim();
    names.iter().position(|n| *n == name)
}

fn hex_color(text: &str) -> Option<u32> {
    let hex = text.trim().trim_start_matches('#');
    (hex.len() == 6)
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

impl TextFx {
    pub fn is_none(&self) -> bool {
        *self == Self::default()
    }

    /// The effects of `self` added to `inherited`: a flag is on when either has it, and a value (emphasis, underline style and
    /// colour) is `self`'s when it names one.
    pub(crate) fn or(self, inherited: Self) -> Self {
        Self {
            double_strike: self.double_strike || inherited.double_strike,
            small_caps: self.small_caps || inherited.small_caps,
            all_caps: self.all_caps || inherited.all_caps,
            hidden: self.hidden || inherited.hidden,
            emphasis: if self.emphasis == 0 {
                inherited.emphasis
            } else {
                self.emphasis
            },
            underline_style: if self.underline_style == 0 {
                inherited.underline_style
            } else {
                self.underline_style
            },
            underline_color: self.underline_color.or(inherited.underline_color),
        }
    }

    /// The flags, emphasis and underline style as one integer (`MAX_PACKED_FX` at most); the colour travels on its own.
    pub fn pack(&self) -> u32 {
        let flag = |on: bool, bit: u32| if on { bit } else { 0 };
        flag(self.double_strike, FLAG_DOUBLE_STRIKE)
            | flag(self.small_caps, FLAG_SMALL_CAPS)
            | flag(self.all_caps, FLAG_ALL_CAPS)
            | flag(self.hidden, FLAG_HIDDEN)
            | u32::from(self.emphasis) << EMPHASIS_SHIFT
            | u32::from(self.underline_style) << UNDERLINE_SHIFT
    }

    /// The effects of a packed integer and an optional colour; `None` when the emphasis or underline style is out of range
    /// or the integer has bits beyond [`MAX_PACKED_FX`].
    pub fn unpack(packed: u32, underline_color: Option<u32>) -> Option<Self> {
        let emphasis = (packed >> EMPHASIS_SHIFT & 0b111) as u8;
        let underline_style = (packed >> UNDERLINE_SHIFT & 0b1_1111) as u8;
        (packed <= MAX_PACKED_FX
            && emphasis <= MAX_EMPHASIS
            && underline_style <= MAX_UNDERLINE_STYLE
            && underline_color.is_none_or(|c| c <= 0xFF_FFFF))
        .then_some(Self {
            double_strike: packed & FLAG_DOUBLE_STRIKE != 0,
            small_caps: packed & FLAG_SMALL_CAPS != 0,
            all_caps: packed & FLAG_ALL_CAPS != 0,
            hidden: packed & FLAG_HIDDEN != 0,
            emphasis,
            underline_style,
            underline_color,
        })
    }

    /// The `w:u` of an underlined run: the style name and the colour.
    pub(crate) fn docx_underline(&self) -> String {
        let name = UNDERLINE_NAMES[usize::from(self.underline_style.min(MAX_UNDERLINE_STYLE))];
        match self.underline_color {
            Some(color) => format!("<w:u w:val=\"{name}\" w:color=\"{color:06X}\"/>"),
            None => format!("<w:u w:val=\"{name}\"/>"),
        }
    }

    /// `w:caps` and `w:smallCaps`, which go before `w:strike` in `w:rPr`.
    pub(crate) fn docx_caps(&self) -> String {
        let mut xml = String::new();
        if self.all_caps {
            xml.push_str("<w:caps/>");
        }
        if self.small_caps {
            xml.push_str("<w:smallCaps/>");
        }
        xml
    }

    /// `w:vanish`, after `w:snapToGrid`.
    pub(crate) fn docx_vanish(&self) -> &'static str {
        if self.hidden {
            "<w:vanish/>"
        } else {
            ""
        }
    }

    /// `w:em`, after `w:vertAlign`.
    pub(crate) fn docx_emphasis(&self) -> String {
        match self.emphasis.min(MAX_EMPHASIS) {
            0 => String::new(),
            n => format!("<w:em w:val=\"{}\"/>", EMPHASIS_DOCX[usize::from(n)]),
        }
    }

    /// Applies a DOCX run property (`w:caps`, `w:smallCaps`, `w:vanish`, `w:dstrike`, `w:em`, and the style and colour of
    /// `w:u`); any other is ignored.
    pub(crate) fn apply_docx_property(&mut self, name: &str, attrs: &Attrs<'_>) {
        let val = attr(attrs, "w:val");
        let val = val.as_deref();
        match name {
            "w:caps" => self.all_caps = toggle_on(val),
            "w:smallCaps" => self.small_caps = toggle_on(val),
            "w:vanish" => self.hidden = toggle_on(val),
            "w:dstrike" => self.double_strike = toggle_on(val),
            "w:em" => {
                self.emphasis = index_of(&EMPHASIS_DOCX, val).map_or(0, |i| i as u8);
            }
            "w:u" => {
                self.underline_style = index_of(&UNDERLINE_NAMES, val).map_or(0, |i| i as u8);
                self.underline_color = attr(attrs, "w:color").as_deref().and_then(hex_color);
            }
            _ => {}
        }
    }

    /// The underline attributes of an ODT text style for an underlined run (each with a leading space).
    pub(crate) fn odt_underline(&self) -> String {
        let (style, kind, width, skip) =
            ODT_UNDERLINE[usize::from(self.underline_style.min(MAX_UNDERLINE_STYLE))];
        let color = self
            .underline_color
            .map_or_else(|| "font-color".to_string(), |c| format!("#{c:06X}"));
        let mode = if skip {
            " style:text-underline-mode=\"skip-white-space\""
        } else {
            ""
        };
        format!(
            " style:text-underline-style=\"{style}\" style:text-underline-type=\"{kind}\" style:text-underline-width=\"{width}\" style:text-underline-color=\"{color}\"{mode}"
        )
    }

    /// The other ODT text properties of the effects (each with a leading space).
    pub(crate) fn odt_attrs(&self) -> String {
        let mut xml = String::new();
        if self.double_strike {
            xml.push_str(
                " style:text-line-through-style=\"solid\" style:text-line-through-type=\"double\"",
            );
        }
        if self.small_caps {
            xml.push_str(" fo:font-variant=\"small-caps\"");
        }
        if self.all_caps {
            xml.push_str(" fo:text-transform=\"uppercase\"");
        }
        if self.hidden {
            xml.push_str(" text:display=\"none\"");
        }
        if self.emphasis != 0 {
            xml.push_str(&format!(
                " style:text-emphasize=\"{}\"",
                EMPHASIS_ODT[usize::from(self.emphasis.min(MAX_EMPHASIS))]
            ));
        }
        xml
    }

    /// The effects of an ODT `style:text-properties` element.
    pub(crate) fn from_odt(attrs: &Attrs<'_>) -> Self {
        let value = |key: &str| attr(attrs, key);
        let line_through = value("style:text-line-through-style");
        let underline = (
            value("style:text-underline-style"),
            value("style:text-underline-type"),
            value("style:text-underline-width"),
            value("style:text-underline-mode"),
        );
        let wanted = (
            underline.0.as_deref().unwrap_or("solid"),
            underline.1.as_deref().unwrap_or("single"),
            underline.2.as_deref().unwrap_or("auto"),
            underline.3.as_deref() == Some("skip-white-space"),
        );
        Self {
            double_strike: value("style:text-line-through-type").as_deref() == Some("double")
                && !matches!(line_through.as_deref(), None | Some("none")),
            small_caps: value("fo:font-variant").as_deref() == Some("small-caps"),
            all_caps: value("fo:text-transform").as_deref() == Some("uppercase"),
            hidden: value("text:display").as_deref() == Some("none"),
            emphasis: index_of(&EMPHASIS_ODT, value("style:text-emphasize").as_deref())
                .map_or(0, |i| i as u8),
            underline_style: ODT_UNDERLINE
                .iter()
                .position(|row| *row == wanted)
                .map_or(0, |i| i as u8),
            underline_color: value("style:text-underline-color")
                .as_deref()
                .and_then(hex_color),
        }
    }

    /// The RTF control words of the effects (each followed by a space). The underline word goes after `\ul`, so a reader
    /// that keeps the last one ends on the style.
    pub(crate) fn rtf_words(&self, underlined: bool) -> String {
        let mut out = String::new();
        if underlined && self.underline_style != 0 {
            let word = RTF_UNDERLINE[usize::from(self.underline_style.min(MAX_UNDERLINE_STYLE))];
            out.push_str(&format!("\\{word} "));
        }
        if self.double_strike {
            out.push_str("\\striked1 ");
        }
        if self.small_caps {
            out.push_str("\\scaps ");
        }
        if self.all_caps {
            out.push_str("\\caps ");
        }
        if self.hidden {
            out.push_str("\\v ");
        }
        match self.emphasis {
            1 => out.push_str("\\accdot "),
            2 => out.push_str("\\acccomma "),
            _ => {}
        }
        out
    }

    /// Applies an RTF underline word; returns whether it was one.
    pub(crate) fn apply_rtf_underline(&mut self, word: &str) -> bool {
        match RTF_UNDERLINE.iter().position(|w| *w == word) {
            Some(i) => {
                self.underline_style = i as u8;
                true
            }
            None => false,
        }
    }

    /// Applies an RTF effect word (`\striked`, `\scaps`, `\caps`, `\v`, `\accdot`, `\acccomma`, `\accnone`) with its
    /// parameter; any other is ignored.
    pub(crate) fn apply_rtf_word(&mut self, word: &str, on: bool) {
        match word {
            "striked" => self.double_strike = on,
            "scaps" => self.small_caps = on,
            "caps" => self.all_caps = on,
            "v" => self.hidden = on,
            "accdot" => self.emphasis = 1,
            "acccomma" => self.emphasis = 2,
            "accnone" => self.emphasis = 0,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_round_trips_and_refuses_out_of_range_fields() {
        let fx = TextFx {
            double_strike: true,
            small_caps: false,
            all_caps: true,
            hidden: true,
            emphasis: 4,
            underline_style: 16,
            underline_color: Some(0xFF_00FF),
        };
        assert_eq!(TextFx::unpack(fx.pack(), fx.underline_color), Some(fx));
        assert_eq!(TextFx::unpack(0, None), Some(TextFx::default()));
        assert_eq!(TextFx::unpack(MAX_PACKED_FX + 1, None), None);
        assert_eq!(TextFx::unpack(5 << EMPHASIS_SHIFT, None), None);
        assert_eq!(TextFx::unpack(17 << UNDERLINE_SHIFT, None), None);
        assert_eq!(TextFx::unpack(0, Some(0x100_0000)), None);
        assert_eq!(TextFx::unpack(u32::MAX, None), None);
    }

    #[test]
    fn the_tables_have_one_row_per_underline_style_and_rows_are_distinct() {
        for (i, row) in ODT_UNDERLINE.iter().enumerate() {
            assert_eq!(
                ODT_UNDERLINE.iter().position(|r| r == row),
                Some(i),
                "odt {i}"
            );
        }
        for (i, word) in RTF_UNDERLINE.iter().enumerate() {
            assert_eq!(
                RTF_UNDERLINE.iter().position(|w| w == word),
                Some(i),
                "rtf {i}"
            );
        }
        assert_eq!(UNDERLINE_NAMES.len(), usize::from(MAX_UNDERLINE_STYLE) + 1);
    }

    #[test]
    fn rtf_words_put_the_underline_style_after_ul_and_read_back() {
        let fx = TextFx {
            underline_style: 2,
            double_strike: true,
            small_caps: true,
            emphasis: 2,
            ..TextFx::default()
        };
        assert_eq!(fx.rtf_words(true), "\\uldb \\striked1 \\scaps \\acccomma ");
        assert_eq!(fx.rtf_words(false), "\\striked1 \\scaps \\acccomma ");
        let mut back = TextFx::default();
        assert!(back.apply_rtf_underline("uldb"));
        assert!(!back.apply_rtf_underline("b"));
        back.apply_rtf_word("striked", true);
        back.apply_rtf_word("scaps", true);
        back.apply_rtf_word("acccomma", true);
        assert_eq!(back, fx);
    }
}
