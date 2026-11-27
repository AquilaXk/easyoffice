//! Character state of one RTF group (resolved against the font and color tables when text is emitted).
//!
//! Reference: the RTF 1.9.1 specification (text type switches `\loch`, `\hich`, `\dbch`; direction switches `\ltrch`,
//! `\rtlch`; associated properties `\af`). rtf.js (https://github.com/tbluemel/rtf.js, version 3.0.9, MIT, Copyright (c) 2015
//! Thomas Bluemel) keeps `\loch`, `\hich` and `\dbch` as one group-scoped "character type" that a child group inherits
//! (`src/rtfjs/parser/Containers.ts` lines 74-96, `src/rtfjs/parser/destinations/RtfDestination.ts` lines 68-70); that
//! shape is followed here, no source is copied. The `rtf-parser` crate (0.5.0) has no word for any of them, and rtf.js
//! reads no `\af`.

use crate::RichScript;

/// The text type a `\loch`, `\hich` or `\dbch` switch names; it holds until another switch, the end of the group or `\plain`.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum TextType {
    /// Low ANSI text (`\loch`, the start of a document): the font is `\f`.
    #[default]
    Low,
    /// High ANSI text (`\hich`): the Latin face of text above 0x7F, which this model keeps in `\f`.
    High,
    /// Double byte (East Asian) text (`\dbch`): the font is `\af`.
    DoubleByte,
}

#[derive(Clone, Default)]
pub(super) struct CharState {
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) underline: bool,
    pub(super) strike: bool,
    pub(super) size: Option<u16>,
    pub(super) font: Option<i32>,
    /// `\af` while `\dbch` is in force: the East Asian face (PD-380).
    font_ea: Option<i32>,
    /// `\af` with no text type switch in force: the associated face of the run, which is the East Asian one when no `\dbch\af`
    /// names it (what PD-376 wrote and read, and what a reader that sorts text by script uses for Hangul).
    font_assoc: Option<i32>,
    text_type: TextType,
    /// `\rtlch` is in force (`\ltrch` ends it): `\af` is then the complex script face, which this model has no place for.
    rtl: bool,
    pub(super) color: Option<usize>,
    pub(super) highlight: Option<usize>,
    pub(super) script: RichScript,
    pub(super) spacing: Option<i16>,
    pub(super) advanced: crate::CharAdvanced,
}

impl CharState {
    /// Applies `\loch`, `\hich`, `\dbch`, `\ltrch`, `\rtlch` or `\af`; any other word is ignored.
    pub(super) fn apply_script_word(&mut self, word: &str, param: Option<i32>) {
        match word {
            "loch" => self.text_type = TextType::Low,
            "hich" => self.text_type = TextType::High,
            "dbch" => self.text_type = TextType::DoubleByte,
            "ltrch" => self.rtl = false,
            "rtlch" => self.rtl = true,
            "af" => match (self.rtl, self.text_type) {
                (true, _) | (false, TextType::High) => {}
                (false, TextType::DoubleByte) => self.font_ea = param,
                (false, TextType::Low) => self.font_assoc = param,
            },
            _ => {}
        }
    }

    /// The font id of the East Asian face: the one `\dbch\af` names, else the associated one.
    pub(super) fn east_font(&self) -> Option<i32> {
        self.font_ea.or(self.font_assoc)
    }
}
