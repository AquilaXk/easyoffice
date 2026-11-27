//! Character state of one RTF group (resolved against the font and color tables when text is emitted).

use crate::RichScript;

#[derive(Clone, Default)]
pub(super) struct CharState {
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) underline: bool,
    pub(super) strike: bool,
    pub(super) size: Option<u16>,
    pub(super) font: Option<i32>,
    /// `\af`: the associated font, the East Asian face when it is not the Latin one (PD-376).
    pub(super) font_ea: Option<i32>,
    pub(super) color: Option<usize>,
    pub(super) highlight: Option<usize>,
    pub(super) script: RichScript,
    pub(super) spacing: Option<i16>,
    pub(super) advanced: crate::CharAdvanced,
}
