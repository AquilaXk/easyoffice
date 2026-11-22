//! The width of the widest digit of a workbook's default font, in CSS pixels at 96 dpi (PD-305). A column's width in
//! characters is a count of these digits (ECMA-376 18.3.1.13 `col` `width`: "the maximum digit width of the numbers 0, 1,
//! 2, ... 9 as rendered in the normal style's font"), so every width a file gives in characters needs it.
//!
//! No font is rendered here. A table gives the widest advance of the digits 0 to 9, in 1/2048 em, of the faces that
//! workbooks use as the default font, and the width is that advance at the font size rounded to whole pixels, as the
//! spreadsheet program lays text out at 96 dpi: `round(advance / 2048 * size_pt * 96 / 72)`. The spec's own example is
//! 7 px for Calibri 11. The advances of the faces on a desktop (Arial, Times New Roman, Courier New, Verdana, Tahoma,
//! Georgia, Trebuchet MS) are read from the `hmtx` and `cmap` tables of the font files; the metric-compatible faces of
//! the same names (Liberation, Tinos, Cousine, Carlito) have the same advances by design. Calibri's 1038 is the advance of
//! its digits, which all have one width ("each digit is the same width for this font", the spec); the metric-compatible
//! Carlito, read from its font file, has 1038 for every digit. Cambria's 1135 is the 0.554 em of its metric-compatible
//! face Caladea (read from the font file, 554 of 1000 for every digit).
//!
//! Malgun Gothic (the default font of the Korean edition of the spreadsheet program) has no font file here. Its 1116 is not
//! measured: a Korean edition user reports the default column of a new workbook (Malgun Gothic 11 pt, 100 %) as 8.38
//! characters, which is 8 characters of an 8 px digit and the padding, `(72 - 5) / 8 = 8.375`, so 11 pt must give 8 px, which
//! `round(advance * 14.667 / 2048)` does for an advance from 1,047 to 1,185. 1116 is the middle of that range, so the other
//! sizes are an estimate (12 pt gives 9 px, 10 pt gives 7 px). Aptos is not in the table: no source gives its digit width.
//!
//! A font that is not in the table is read as Calibri: the workbook is shown in a substitute face on the machine that has
//! not the font, and the size alone cannot say which. The width is never below 1 px and never above [`MAX_DIGIT_PX`], and a
//! size that is not a number, not positive or above the 409 pt a font size may have is the default size of 11 pt, so a
//! hostile `styles.xml` cannot make the width of a column cost or overflow anything.

/// The digit width of the font this crate writes as the default font of a workbook (Calibri 11). The sheet model counts
/// its widths in characters of this width.
pub const MODEL_DIGIT_PX: u32 = 7;
/// The widest digit the table gives: a column of 255 characters of it is still a width a sheet can draw.
pub const MAX_DIGIT_PX: u32 = 64;

const DEFAULT_SIZE_PT: f64 = 11.0;
const MAX_SIZE_PT: f64 = 409.0;
const PX_PER_PT: f64 = 96.0 / 72.0;
const UNITS_PER_EM: f64 = 2048.0;

/// The widest digit of the fallback face, Calibri.
const CALIBRI_ADVANCE: u16 = 1038;
/// An estimate inside what 8 px at 11 pt allows (see the header), not a measured advance.
const MALGUN_ADVANCE: u16 = 1116;

/// Face names (lowercase) and the widest advance of their digits in 1/2048 em.
const DIGIT_ADVANCES: &[(&str, u16)] = &[
    ("calibri", CALIBRI_ADVANCE),
    ("carlito", CALIBRI_ADVANCE),
    ("arial", 1139),
    ("helvetica", 1139),
    ("liberation sans", 1139),
    ("times new roman", 1024),
    ("liberation serif", 1024),
    ("tinos", 1024),
    ("courier new", 1229),
    ("liberation mono", 1229),
    ("cousine", 1229),
    ("verdana", 1302),
    ("tahoma", 1118),
    ("georgia", 1257),
    ("trebuchet ms", 1074),
    ("cambria", 1135),
    ("caladea", 1135),
    ("malgun gothic", MALGUN_ADVANCE),
    ("맑은 고딕", MALGUN_ADVANCE),
];

/// The width of the widest digit of `font` at `size_pt` points, `1..=`[`MAX_DIGIT_PX`] pixels. A missing or unknown
/// font is Calibri and a missing or unusable size is 11 pt.
pub fn max_digit_px(font: Option<&str>, size_pt: Option<f64>) -> u32 {
    let name = font.map_or("", str::trim);
    let advance = DIGIT_ADVANCES
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(name))
        .map_or(CALIBRI_ADVANCE, |&(_, advance)| advance);
    let size = size_pt
        .filter(|s| s.is_finite() && *s > 0.0 && *s <= MAX_SIZE_PT)
        .unwrap_or(DEFAULT_SIZE_PT);
    let px = (f64::from(advance) / UNITS_PER_EM * size * PX_PER_PT).round();
    (px as u32).clamp(1, MAX_DIGIT_PX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spec_example_and_the_common_default_fonts() {
        assert_eq!(max_digit_px(Some("Calibri"), Some(11.0)), MODEL_DIGIT_PX);
        let table = [
            ("Calibri", 10.0, 7),
            ("Calibri", 12.0, 8),
            ("Arial", 10.0, 7),
            ("Arial", 11.0, 8),
            ("Arial", 12.0, 9),
            ("Times New Roman", 10.0, 7),
            ("Times New Roman", 12.0, 8),
            ("Courier New", 10.0, 8),
            ("Verdana", 10.0, 8),
            ("Tahoma", 11.0, 8),
            ("Cambria", 11.0, 8),
            ("Cambria", 10.0, 7),
            // The Korean edition's default: 8.38 characters of an 8 px digit (11 pt); the other sizes are estimates.
            ("Malgun Gothic", 11.0, 8),
            ("맑은 고딕", 11.0, 8),
            ("Malgun Gothic", 10.0, 7),
            ("Malgun Gothic", 12.0, 9),
        ];
        for (font, size, px) in table {
            assert_eq!(max_digit_px(Some(font), Some(size)), px, "{font} {size}");
        }
    }

    #[test]
    fn names_are_matched_without_case_and_padding() {
        assert_eq!(max_digit_px(Some("  ARIAL "), Some(11.0)), 8);
        assert_eq!(max_digit_px(Some("Liberation Sans"), Some(11.0)), 8);
        assert_eq!(max_digit_px(Some("Carlito"), Some(11.0)), 7);
    }

    #[test]
    fn an_unknown_font_is_calibri_and_a_bad_size_is_11_points() {
        assert_eq!(max_digit_px(Some("No Such Face"), Some(11.0)), 7);
        assert_eq!(max_digit_px(None, None), 7);
        for size in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            0.0,
            -3.0,
            409.5,
            1e308,
        ] {
            assert_eq!(max_digit_px(Some("Arial"), Some(size)), 8, "{size}");
        }
    }

    #[test]
    fn the_width_stays_in_range_for_any_size() {
        for font in ["Verdana", "Courier New", "Calibri", "x"] {
            for size in [0.01, 0.5, 1.0, 100.0, 409.0] {
                let px = max_digit_px(Some(font), Some(size));
                assert!((1..=MAX_DIGIT_PX).contains(&px), "{font} {size} {px}");
            }
        }
        assert_eq!(max_digit_px(Some("Verdana"), Some(409.0)), MAX_DIGIT_PX);
        assert_eq!(max_digit_px(Some("Verdana"), Some(0.01)), 1);
    }
}
