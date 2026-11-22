//! The sheet-wide column width of a worksheet (PD-303): XLSX `<sheetFormatPr defaultColWidth baseColWidth>`, kept in
//! [`SheetStyling::default_col_width`](crate::SheetStyling) so a file that is read and written again keeps it.
//!
//! 참조 구현: IronCalc `xlsx/src/import/worksheets.rs` lines 255-270 (`get_default_column_width`: the one `sheetFormatPr`
//! of the sheet, an attribute that is not a number falls back to the program's own default), Apache-2.0 or MIT, Copyright
//! (c) IronCalc contributors, commit 87b684bb04a87c63bdd6343c29036450b1cfd358. The reference reads the width only and
//! writes none. Here both attributes are kept as the file gave them and a value outside the bounds is read as missing
//! (never clamped), following ECMA-376 18.3.1.81 (`baseColWidth`: characters of the maximum digit width, without padding;
//! `defaultColWidth`: characters including the padding, as `<col width>`) and the 255 characters a column may have. No line
//! is copied. The reference's own default of 8.43 characters is the 8 characters of `baseColWidth` below plus the padding
//! (8 * 7 + 5 = 61 px, 64 px once rounded up to 8 px, which is 8.43 characters of a 7 px digit: (64 - 5) / 7).
//!
//! The width of a column without a width of its own is worked out here (PD-305), the rule of the spreadsheet programs:
//! - `defaultColWidth` first, in characters including the padding, as pixels by the `<col width>` formula of ECMA-376
//!   18.3.1.13: `Truncate(((256 * width + Truncate(128 / digit)) / 256) * digit)`;
//! - else `baseColWidth` (8 when missing): `base * digit + 5` pixels (4 px margin padding and 1 px gridline, 18.3.1.81),
//!   rounded up to a multiple of 8 pixels, "so that scrolling across columns and rows is faster" (the vendor support
//!   article "Description of how column widths are determined", KB 214123, retired; kept by web.archive.org), which is
//!   why 8 characters of a 7 px digit are 64 px and not the 61 px of the formula alone.
//!
//! `digit` is the maximum digit width of the workbook's default font (`sheet_digit_width.rs`), 7 px for Calibri 11. The model
//! of a sheet always counts in 7 px digits, the font this crate writes as the default: a reader of a workbook whose default
//! font has another digit width turns the sheet's width into 7 px digits with [`DefaultColWidth::in_model_digits`], so the
//! pixels stay and a save writes a file that says the same.

use std::fmt::Write;

use crate::sheet_styling::sheet_digit_width::{MAX_DIGIT_PX, MODEL_DIGIT_PX};
use crate::sheet_styling::xlsx_styles::px_to_xlsx_width;
use crate::sheet_styling::{SheetStyling, MIN_COLUMN_PX};

/// Characters a default column holds when a sheet says nothing (`baseColWidth` of ECMA-376 18.3.1.81).
pub const DEFAULT_BASE_COL_CHARS: u32 = 8;
/// Margin padding (2 px on each side) and the gridline (1 px) around the characters of a column.
const COL_PADDING_PX: u32 = 5;
/// The width a sheet is drawn at when `baseColWidth` decides it is a multiple of this many pixels.
const COL_PX_ALIGN: u32 = 8;
/// The width of a column without a width of its own in a sheet that says nothing, with a 7 px digit: 8 characters, 64 px.
pub const DEFAULT_COL_PX: u32 = 64;
/// The widest default column in pixels: 255 characters of the model's 7 px digit, what the writer can say.
pub const MAX_DEFAULT_COL_PX: u32 = MAX_BASE_COL_WIDTH * MODEL_DIGIT_PX;

/// The widest a column may be, in characters (ECMA-376 18.3.1.13 `col` `width`, the limit of the spreadsheet programs).
pub const MAX_COL_WIDTH_CHARS: f64 = 255.0;
/// The narrowest default width kept (a hundredth of a character). The lower bound keeps the attribute short: a value like
/// `1e-300` would be written as 300 digits, which the reader's text bound refuses, so it would not come back.
pub const MIN_COL_WIDTH_CHARS: f64 = 0.01;
/// The largest `baseColWidth`, the characters of the maximum digit width a default column holds.
pub const MAX_BASE_COL_WIDTH: u32 = 255;
/// Longest text a width may be written as; a longer one is not a width.
const MAX_WIDTH_TEXT: usize = 32;

/// `defaultColWidth` (characters, padding included) and `baseColWidth` (characters) of a sheet, either of them missing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DefaultColWidth {
    pub width: Option<f64>,
    pub base: Option<u32>,
}

/// Whether `width` is a default width: a finite number from [`MIN_COL_WIDTH_CHARS`] up to [`MAX_COL_WIDTH_CHARS`]. The reader
/// and the writer share this, so every width the writer takes is read back.
fn in_bounds(width: f64) -> bool {
    width.is_finite() && (MIN_COL_WIDTH_CHARS..=MAX_COL_WIDTH_CHARS).contains(&width)
}

fn short(text: &str) -> &str {
    if text.len() <= MAX_WIDTH_TEXT {
        text.trim()
    } else {
        ""
    }
}

impl DefaultColWidth {
    /// The pair of a `sheetFormatPr`'s two attributes, `None` when neither is a width. An attribute that is not a number,
    /// not finite or out of bounds is missing; the other one is kept.
    pub(crate) fn parse(width: Option<&str>, base: Option<&str>) -> Option<Self> {
        let width = width
            .and_then(|t| short(t).parse::<f64>().ok())
            .filter(|w| in_bounds(*w));
        let base = base
            .and_then(|t| short(t).parse::<u32>().ok())
            .filter(|b| *b <= MAX_BASE_COL_WIDTH);
        (width.is_some() || base.is_some()).then_some(Self { width, base })
    }

    /// Whether the pair holds a value and every value is in bounds: what a writer accepts.
    pub fn within_bounds(&self) -> bool {
        (self.width.is_some() || self.base.is_some())
            && self.width.is_none_or(in_bounds)
            && self.base.is_none_or(|b| b <= MAX_BASE_COL_WIDTH)
    }

    /// The sheet's width as it reads with 7 px digits when the workbook's default font has a `digit_px` wide digit: the
    /// same pixels, as a `defaultColWidth` (the values are kept as the file gave them when the digits are the same).
    pub(crate) fn in_model_digits(widths: Option<Self>, digit_px: u32) -> Option<Self> {
        let digit = digit_px.clamp(1, MAX_DIGIT_PX);
        if digit == MODEL_DIGIT_PX {
            return widths;
        }
        let px = default_col_px(widths, digit);
        Some(Self {
            width: Some(px_to_xlsx_width(px)),
            base: None,
        })
    }

    /// ` baseColWidth="10" defaultColWidth="20"` for the values that are set, in the order of the schema's attributes.
    pub(crate) fn xml_attrs(&self) -> String {
        let mut xml = String::new();
        if let Some(base) = self.base {
            let _ = write!(xml, " baseColWidth=\"{base}\"");
        }
        if let Some(width) = self.width {
            let _ = write!(xml, " defaultColWidth=\"{width}\"");
        }
        xml
    }
}

impl SheetStyling {
    /// The width in pixels of the columns that have none of their own (PD-305): one rule for the page, the positions of
    /// pictures and charts, and the PDF.
    pub fn default_col_px(&self) -> u32 {
        default_col_px(self.default_col_width, MODEL_DIGIT_PX)
    }

    /// The width in pixels of the zero-based `col`: its own width, else the default of the sheet.
    pub fn col_px(&self, col: u32) -> u32 {
        self.columns
            .get(&col)
            .map_or_else(|| self.default_col_px(), |&px| px)
    }
}

/// A width in characters as pixels, the runtime formula of ECMA-376 18.3.1.13 (`<col width>`), with the digit clamped to
/// what a font can have. A width that is not a positive number is 0 px; a huge one saturates, so the caller's clamp decides.
pub(crate) fn chars_to_px(chars: f64, digit_px: u32) -> u32 {
    if !chars.is_finite() || chars <= 0.0 {
        return 0;
    }
    let digit = digit_px.clamp(1, MAX_DIGIT_PX);
    let rounding = f64::from(128 / digit);
    ((256.0 * chars + rounding) / 256.0 * f64::from(digit)).floor() as u32
}

/// `baseColWidth` characters as pixels: the characters and the padding, up to the next multiple of 8 pixels.
fn base_cols_to_px(base: u32, digit: u32) -> u32 {
    (base * digit + COL_PADDING_PX).next_multiple_of(COL_PX_ALIGN)
}

/// The width in pixels of a column that has none of its own, in a sheet with `widths` (none: the sheet says nothing) of a
/// workbook whose default font has a `digit_px` wide digit. A value out of bounds does not count, as when it is read.
/// The result is `MIN_COLUMN_PX..=MAX_DEFAULT_COL_PX` whatever the input.
pub fn default_col_px(widths: Option<DefaultColWidth>, digit_px: u32) -> u32 {
    let digit = digit_px.clamp(1, MAX_DIGIT_PX);
    let px = match widths {
        Some(DefaultColWidth {
            width: Some(chars), ..
        }) if in_bounds(chars) => chars_to_px(chars, digit),
        Some(DefaultColWidth {
            base: Some(base), ..
        }) if base <= MAX_BASE_COL_WIDTH => base_cols_to_px(base, digit),
        _ => base_cols_to_px(DEFAULT_BASE_COL_CHARS, digit),
    };
    px.clamp(MIN_COLUMN_PX, MAX_DEFAULT_COL_PX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widths(width: Option<f64>, base: Option<u32>) -> Option<DefaultColWidth> {
        Some(DefaultColWidth { width, base })
    }

    #[test]
    fn a_sheet_that_says_nothing_is_the_default_of_the_digit() {
        // 8 characters: 8 * digit + 5 px up to a multiple of 8 (61 -> 64 for the 7 px digit of Calibri 11).
        let px: Vec<u32> = (5..=10).map(|d| default_col_px(None, d)).collect();
        assert_eq!(px, [48, 56, 64, 72, 80, 88]);
        assert_eq!(default_col_px(None, MODEL_DIGIT_PX), DEFAULT_COL_PX);
    }

    #[test]
    fn default_col_width_comes_first_then_base_col_width() {
        // The spec's example: 8.7109375 characters of a 7 px digit are 61 px (no rounding up to 8).
        assert_eq!(default_col_px(widths(Some(8.7109375), None), 7), 61);
        assert_eq!(default_col_px(widths(Some(9.140625), Some(20)), 7), 64);
        assert_eq!(default_col_px(widths(Some(20.7109375), Some(10)), 7), 145);
        // Only the base: the characters and the padding, up to 8 px (10 * 7 + 5 = 75 -> 80).
        assert_eq!(default_col_px(widths(None, Some(10)), 7), 80);
        assert_eq!(default_col_px(widths(None, Some(0)), 7), 8);
        assert_eq!(default_col_px(widths(None, Some(255)), 7), 1_785);
    }

    #[test]
    fn a_value_out_of_bounds_does_not_count_and_the_result_stays_in_range() {
        for bad in [f64::NAN, f64::INFINITY, -1.0, 0.0, 0.0099, 255.01, 1e308] {
            assert_eq!(default_col_px(widths(Some(bad), None), 7), 64, "{bad}");
            assert_eq!(default_col_px(widths(Some(bad), Some(10)), 7), 80, "{bad}");
        }
        assert_eq!(default_col_px(widths(None, Some(256)), 7), 64);
        assert_eq!(default_col_px(widths(None, Some(u32::MAX)), 7), 64);
        for digit in [0, 1, 64, 65, u32::MAX] {
            for w in [
                None,
                widths(Some(255.0), Some(255)),
                widths(Some(0.01), None),
            ] {
                let px = default_col_px(w, digit);
                assert!(
                    (MIN_COLUMN_PX..=MAX_DEFAULT_COL_PX).contains(&px),
                    "{digit} {w:?}"
                );
            }
        }
    }

    #[test]
    fn chars_to_px_follows_the_runtime_formula_of_the_spec() {
        assert_eq!(chars_to_px(8.7109375, 7), 61);
        assert_eq!(chars_to_px(0.0, 7), 0);
        assert_eq!(chars_to_px(f64::NAN, 7), 0);
        assert_eq!(chars_to_px(f64::INFINITY, 7), 0);
        assert_eq!(chars_to_px(f64::MAX, 7), u32::MAX);
        // The writer's width of a pixel count reads back as the same pixels, for every width a column may have.
        for px in 1..=2_000u32 {
            assert_eq!(chars_to_px(px_to_xlsx_width(px), 7), px, "{px}");
        }
    }

    #[test]
    fn other_digits_are_turned_into_the_models_pixels() {
        assert_eq!(DefaultColWidth::in_model_digits(None, 7), None);
        let kept = widths(Some(20.5), Some(9));
        assert_eq!(DefaultColWidth::in_model_digits(kept, 7), kept);
        for digit in [5, 6, 8, 9, 12, 64] {
            for w in [None, widths(None, Some(10)), widths(Some(12.25), Some(3))] {
                let wanted = default_col_px(w, digit);
                let model = DefaultColWidth::in_model_digits(w, digit);
                assert!(model.is_some_and(|m| m.within_bounds()), "{digit} {w:?}");
                assert_eq!(
                    default_col_px(model, MODEL_DIGIT_PX),
                    wanted,
                    "{digit} {w:?}"
                );
            }
        }
    }
}
