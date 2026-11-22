//! The default font of a workbook (PD-306): the font of the Normal cell style, whose widest digit is the unit of every column
//! width in characters (ECMA-376 18.3.1.13 `col@width`, 18.3.1.81 `sheetFormatPr`).
//!
//! A workbook that is new, or comes from a CSV, TSV or an ODS without a font of its own, has the font of the Korean edition of
//! the spreadsheet program: Malgun Gothic 11 pt, an 8 px digit, so a default column is 8.38 characters (72 px). That pair is
//! not stored: [`SheetStyling::default_font`](crate::SheetStyling) is `None` for it. A workbook that has another Normal font
//! (Calibri 11, Arial 12, ...) keeps it as `Some`: the writer says the same font again and the widths in characters stay the
//! pixels the file meant. Every sheet of a workbook has the same value; the writer reads the first one, like `date1904`.
//!
//! A name is bounded as every font name is ([`clean_font_name`]: 64 characters, no control or markup characters) and a size is
//! a finite number above 0 and up to 409 pt, so a hostile `styles.xml` or request cannot make the font cost or overflow
//! anything. A value out of bounds is not a font: the caller reads the default as missing.

use crate::sheet_styling::sheet_digit_width::max_digit_px;
use crate::sheet_styling::{clean_font_name, SheetStyling};

/// The name of the standard default font, as the Korean edition writes it into `styles.xml`.
pub const STANDARD_FONT_NAME: &str = "맑은 고딕";
/// The size of the standard default font in points.
pub const STANDARD_FONT_SIZE_PT: f64 = 11.0;
/// The widest digit of the standard default font in pixels (`max_digit_px` of the two values above, checked by a test).
pub const STANDARD_DIGIT_PX: u32 = 8;
/// The largest font size kept, the largest a spreadsheet program allows.
pub const MAX_FONT_SIZE_PT: f64 = 409.0;
/// Two sizes closer than this are the same size (a size is written with two decimals).
const SAME_SIZE_PT: f64 = 0.005;
/// The face a Normal style without a name is read as: the first spreadsheet program's default.
const NAMELESS_FACE: &str = "Calibri";

/// A workbook's default font: its name, its size and the width of its widest digit.
#[derive(Debug, Clone, PartialEq)]
pub struct DefaultFont {
    name: String,
    size_pt: f64,
    digit_px: u32,
}

impl DefaultFont {
    /// The font `name` at `size_pt`, `None` when the name is not a font name or the size is not a number from above 0 to
    /// [`MAX_FONT_SIZE_PT`]. A font that is the standard one is a font here too; see [`DefaultFont::kept`].
    pub fn new(name: &str, size_pt: f64) -> Option<Self> {
        let name = clean_font_name(name)?;
        if !size_pt.is_finite() || size_pt <= 0.0 || size_pt > MAX_FONT_SIZE_PT {
            return None;
        }
        Some(Self {
            digit_px: max_digit_px(Some(name), Some(size_pt)),
            name: name.to_string(),
            size_pt,
        })
    }

    /// What a reader keeps of the Normal style's font: `None` when it is the standard default font (the writer says that one
    /// without being told), else the font. A missing or unusable name is read as Calibri and a missing or unusable size as 11 pt,
    /// as [`max_digit_px`] does.
    pub fn kept(name: Option<&str>, size_pt: Option<f64>) -> Option<Self> {
        let size = size_pt.filter(|s| s.is_finite() && *s > 0.0 && *s <= MAX_FONT_SIZE_PT);
        let font = Self::new(
            name.and_then(clean_font_name).unwrap_or(NAMELESS_FACE),
            size.unwrap_or(STANDARD_FONT_SIZE_PT),
        )?;
        font.non_standard()
    }

    /// The font, or `None` when it is the standard default font: what a styling stores (the writer says the standard one by
    /// itself).
    pub fn non_standard(self) -> Option<Self> {
        (!self.is_standard()).then_some(self)
    }

    /// The face name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The size in points.
    pub fn size_pt(&self) -> f64 {
        self.size_pt
    }

    /// The width in pixels of the widest digit: the unit of the widths in characters.
    pub fn digit_px(&self) -> u32 {
        self.digit_px
    }

    fn is_standard(&self) -> bool {
        self.name.eq_ignore_ascii_case(STANDARD_FONT_NAME)
            && (self.size_pt - STANDARD_FONT_SIZE_PT).abs() < SAME_SIZE_PT
    }
}

impl SheetStyling {
    /// The face name of the workbook's default font.
    pub fn default_font_name(&self) -> &str {
        self.default_font
            .as_ref()
            .map_or(STANDARD_FONT_NAME, DefaultFont::name)
    }

    /// The size in points of the workbook's default font.
    pub fn default_font_size_pt(&self) -> f64 {
        self.default_font
            .as_ref()
            .map_or(STANDARD_FONT_SIZE_PT, DefaultFont::size_pt)
    }

    /// The width in pixels of the widest digit of the workbook's default font (PD-306), the unit of widths in characters.
    pub fn digit_px(&self) -> u32 {
        self.default_font
            .as_ref()
            .map_or(STANDARD_DIGIT_PX, DefaultFont::digit_px)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_standard_digit_is_the_one_of_the_standard_font() {
        assert_eq!(
            max_digit_px(Some(STANDARD_FONT_NAME), Some(STANDARD_FONT_SIZE_PT)),
            STANDARD_DIGIT_PX
        );
        let plain = SheetStyling::default();
        assert_eq!(plain.digit_px(), STANDARD_DIGIT_PX);
        assert_eq!(plain.default_font_name(), "맑은 고딕");
        assert_eq!(plain.default_font_size_pt(), 11.0);
    }

    #[test]
    fn a_reader_keeps_every_font_but_the_standard_one() {
        assert_eq!(DefaultFont::kept(Some("맑은 고딕"), Some(11.0)), None);
        assert_eq!(DefaultFont::kept(Some(" 맑은 고딕 "), Some(11.001)), None);
        let calibri = DefaultFont::kept(Some("Calibri"), Some(11.0)).unwrap();
        assert_eq!(
            (calibri.name(), calibri.size_pt(), calibri.digit_px()),
            ("Calibri", 11.0, 7)
        );
        // The English name of the same face is another name: the file keeps what it says.
        let malgun = DefaultFont::kept(Some("Malgun Gothic"), Some(11.0)).unwrap();
        assert_eq!(malgun.digit_px(), 8);
        let arial = DefaultFont::kept(Some("Arial"), Some(12.0)).unwrap();
        assert_eq!(arial.digit_px(), 9);
        assert_eq!(
            DefaultFont::kept(Some("맑은 고딕"), Some(12.0))
                .unwrap()
                .digit_px(),
            9
        );
    }

    #[test]
    fn a_missing_or_hostile_name_or_size_reads_as_the_first_programs_default() {
        // No name: Calibri; no size: 11 pt.
        let nameless = DefaultFont::kept(None, None).unwrap();
        assert_eq!((nameless.name(), nameless.size_pt()), ("Calibri", 11.0));
        let long = "A".repeat(65);
        for name in [long.as_str(), "", "Bad;Font", "Bad\nFont", "<b>"] {
            assert_eq!(
                DefaultFont::kept(Some(name), Some(11.0)).unwrap().name(),
                "Calibri",
                "{name:?}"
            );
            assert!(DefaultFont::new(name, 11.0).is_none(), "{name:?}");
        }
        for size in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            0.0,
            -1.0,
            409.5,
            1e308,
        ] {
            assert!(DefaultFont::new("Arial", size).is_none(), "{size}");
            let kept = DefaultFont::kept(Some("Arial"), Some(size)).unwrap();
            assert_eq!(kept.size_pt(), 11.0, "{size}");
        }
        assert!(DefaultFont::new("Arial", 409.0).is_some());
    }
}
