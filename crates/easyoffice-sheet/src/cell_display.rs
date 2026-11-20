//! Display text of a stored cell value for the places that cannot ask the grid: the PDF and the
//! exports of a sheet (PD-235). A cell with a number format shows what `format_cell_value` gives,
//! the same text the grid and the `TEXT` function show; a cell without one shows its General text.

use std::borrow::Cow;
use std::rc::Rc;

use crate::text_format::{
    compiled_format, format_cell_with, format_typed_cell_with, CompiledFormat,
};

/// Significant digits a General decimal shows (the page's rule, `formatGeneral` in `sheet-grid.js`).
const GENERAL_DIGITS: usize = 10;

/// A cell text a number format made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellText {
    pub text: String,
    /// Colour of the format section as `0xRRGGBB`.
    pub color: Option<u32>,
    /// Byte offset in `text` and the character `*x` repeats over the free width of the cell.
    pub fill: Option<(usize, char)>,
    /// The cell holds a number, which a page sets at the right.
    pub numeric: bool,
}

impl CellText {
    /// The text with the fill character once, for output that has no width (CSV).
    pub fn flat(&self) -> String {
        match self.fill {
            Some((at, ch)) if self.text.is_char_boundary(at) => {
                let mut out = String::with_capacity(self.text.len() + ch.len_utf8());
                out.push_str(&self.text[..at]);
                out.push(ch);
                out.push_str(&self.text[at..]);
                out
            }
            _ => self.text.clone(),
        }
    }
}

/// A pre-compiled number format handle so a sheet pass resolves each `num_fmt` id once (`O(1)`).
#[derive(Clone)]
pub struct CompiledNumFmt(Rc<CompiledFormat>);

impl CompiledNumFmt {
    /// Formats `raw` with this compiled format, parsing numeric text at most once.
    pub fn format(&self, raw: &str) -> Option<CellText> {
        let (shown, numeric) = format_cell_with(&self.0, raw)?;
        Some(CellText {
            text: shown.text,
            color: shown.color.map(|c| c.0),
            fill: shown.fill,
            numeric,
        })
    }

    /// Formats `raw` honoring its [`easyoffice_formats::CellValueKind`] (PD-241).
    pub fn format_typed(
        &self,
        raw: &str,
        kind: easyoffice_formats::CellValueKind,
    ) -> Option<CellText> {
        let (shown, numeric) = format_typed_cell_with(&self.0, raw, kind)?;
        Some(CellText {
            text: shown.text,
            color: shown.color.map(|c| c.0),
            fill: shown.fill,
            numeric,
        })
    }

    /// Whether finite `num` renders through a date or time section under this compiled format.
    #[inline]
    pub fn shows_date(&self, num: f64) -> bool {
        self.0.shows_date(num)
    }
}

/// Pre-compiles `code` for repeated [`CompiledNumFmt::format`] calls across a sheet.
pub fn compile_for_page(code: &str) -> CompiledNumFmt {
    CompiledNumFmt(compiled_format(code))
}

/// The display of `raw` under the number format `code`; `None` when the cell keeps its General
/// display (see `format_cell_value`).
pub fn format_for_page(code: &str, raw: &str) -> Option<CellText> {
    compile_for_page(code).format(raw)
}

/// [`format_for_page`] honoring `kind` so text-typed numeric strings ignore numeric sections.
pub fn format_for_page_typed(
    code: &str,
    raw: &str,
    kind: easyoffice_formats::CellValueKind,
) -> Option<CellText> {
    compile_for_page(code).format_typed(raw, kind)
}

/// General display of a stored value: a long decimal (float noise such as `0.30000000000000004`)
/// shows at most 10 significant digits, anything else as it is. The grid applies the same rule.
pub fn general_display(raw: &str) -> Cow<'_, str> {
    let unsigned = raw.strip_prefix('-').unwrap_or(raw);
    let Some((whole, fraction)) = unsigned.split_once('.') else {
        return Cow::Borrowed(raw);
    };
    let digits_only = |t: &str| t.bytes().all(|b| b.is_ascii_digit());
    if fraction.is_empty() || !digits_only(whole) || !digits_only(fraction) {
        return Cow::Borrowed(raw);
    }
    let significant = whole
        .bytes()
        .chain(fraction.bytes())
        .skip_while(|b| *b == b'0')
        .count();
    if significant <= GENERAL_DIGITS {
        return Cow::Borrowed(raw);
    }
    let Some(rounded) = raw
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .and_then(|n| format!("{:.*e}", GENERAL_DIGITS - 1, n).parse::<f64>().ok())
    else {
        return Cow::Borrowed(raw);
    };
    Cow::Owned(if rounded != 0.0 && rounded.abs() < 1e-6 {
        format!("{rounded:e}")
    } else {
        rounded.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_decimals_show_ten_digits_and_short_ones_stay() {
        for (raw, want) in [
            ("0.30000000000000004", "0.3"),
            ("1266.66666666667", "1266.666667"),
            ("-5.057996968497839", "-5.057996968"),
            ("0.1234567890", "0.1234567890"),
            ("12.5", "12.5"),
            ("1e-7", "1e-7"),
            ("0.000001234567890123", "0.00000123456789"),
            ("0.0000012345678956", "0.000001234567896"),
            ("abc", "abc"),
            ("1.2.3", "1.2.3"),
            ("123456789012", "123456789012"),
            ("-", "-"),
        ] {
            assert_eq!(general_display(raw), want, "{raw}");
        }
    }

    #[test]
    fn a_number_format_gives_text_color_and_fill() {
        let shown = format_for_page("_($* #,##0.00_);[Red]($* #,##0.00)", "-1234.5").unwrap();
        assert_eq!(shown.color, Some(0xFF0000));
        assert!(shown.numeric && shown.fill.is_some());
        assert!(shown.flat().contains('$'));
        let plain = format_for_page("0.00", "3.14159").unwrap();
        assert_eq!(
            (plain.text.as_str(), plain.color, plain.numeric),
            ("3.14", None, true)
        );
        assert!(!format_for_page("@", "007").unwrap().numeric);
        assert!(format_for_page("0.00", "abc").is_none());
    }

    #[test]
    fn a_number_in_a_text_format_cell_shows_as_text_does() {
        use easyoffice_formats::CellValueKind::Number;
        for raw in [
            "1e10",
            "10000000000",
            "1.5E-7",
            "123456789012345678",
            "0.30000000000000004",
            "-42",
        ] {
            let num: f64 = raw.parse().unwrap();
            let text_function = crate::format_number_with_code(num, "@").unwrap();
            let shown = format_for_page_typed("@", raw, Number).unwrap();
            assert_eq!(shown.text, text_function, "{raw}");
        }
        assert_eq!(
            format_for_page_typed("@", "1e10", Number).unwrap().text,
            "10000000000"
        );
        let text = easyoffice_formats::CellValueKind::Text;
        assert_eq!(
            format_for_page_typed("@", "1e10", text).unwrap().text,
            "1e10"
        );
        assert_eq!(format_for_page("@", "007").unwrap().text, "007");
    }
}
