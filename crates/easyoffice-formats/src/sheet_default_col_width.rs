//! The sheet-wide column width of a worksheet (PD-303): XLSX `<sheetFormatPr defaultColWidth baseColWidth>`, kept in
//! [`SheetStyling::default_col_width`](crate::SheetStyling) so a file that is read and written again keeps it.
//!
//! 참조 구현: IronCalc `xlsx/src/import/worksheets.rs` lines 255-270 (`get_default_column_width`: the one `sheetFormatPr`
//! of the sheet, an attribute that is not a number falls back to the program's own default), Apache-2.0 or MIT, Copyright
//! (c) IronCalc contributors, commit 87b684bb04a87c63bdd6343c29036450b1cfd358. The reference reads the width only and
//! writes none. Here both attributes are kept as the file gave them and a value outside the bounds is read as missing
//! (never clamped), following ECMA-376 18.3.1.81 (`baseColWidth`: characters of the maximum digit width, without padding;
//! `defaultColWidth`: characters including the padding, as `<col width>`) and the 255 characters a column may have. No line
//! is copied.
//!
//! The page does not draw the columns of a sheet at this width: every column without a width of its own is the 96 px of the
//! window (PD-302), whatever the file says. The value travels through the page and back to the file.

use std::fmt::Write;

/// The widest a column may be, in characters (ECMA-376 18.3.1.13 `col` `width`, the limit of the spreadsheet programs).
pub const MAX_COL_WIDTH_CHARS: f64 = 255.0;
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

/// Whether `width` is a width of a column: a finite number above 0 and up to [`MAX_COL_WIDTH_CHARS`].
fn in_bounds(width: f64) -> bool {
    width.is_finite() && width > 0.0 && width <= MAX_COL_WIDTH_CHARS
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
