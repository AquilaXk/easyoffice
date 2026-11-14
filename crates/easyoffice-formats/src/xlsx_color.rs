//! SpreadsheetML colours of fills and fonts (PD-227): the `color` / `fgColor` / `bgColor` attribute
//! set (ECMA-376 18.3.1.15 and 18.8.19: `rgb`, `theme` with `tint`, `indexed`, `auto`) as
//! `0xRRGGBB`, the palette of `indexed` colours (18.8.27, the default table and the workbook's
//! `<colors><indexedColors>` override) and the theme colours of the workbook.
//!
//! 참조 구현: IronCalc `base/src/colors.rs` lines 125-165 (`get_indexed_color`, the 64-entry default
//! indexed palette; index 64 and over fall back to black there) and `xlsx/src/import/styles.rs` lines
//! 15-45 (`parse_indexed_colors`, the `rgbColor` list of `indexedColors` as ARGB or RGB, a malformed
//! entry falls back to the default) and 217-245 (a fill takes `fgColor` and falls back to `bgColor`),
//! MIT OR Apache-2.0, Copyright (c) IronCalc contributors, commit
//! 53c00566515e3d67bf9b7bd2205db16771ea66e8.
//!
//! Modifications: the table is a `const [u32; 64]` of packed RGB, not strings; the override is applied
//! entry by entry on top of the default (a short list keeps the rest), is limited to 64 entries and
//! ignores a malformed entry; index 64 (system foreground) is no colour and 65 (system background) is
//! white, as ECMA-376 18.8.27 says, anything above is refused instead of read as black; the tint is
//! the float HLS luminance shift of `xlsx_view::apply_tint` (the reference rounds the luminance to a
//! whole percent, which moves a lightened accent colour by a step that the spreadsheet program does
//! not); every attribute is read through `str::get`, so a multi-byte value cannot split a character.

use crate::rtf_xml_helpers::{
    extract_attr_from_fragment, extract_tag_bodies, extract_tags_or_self_closing_with_attrs,
};
use crate::sheet_styling::xlsx_view::{apply_tint, DEFAULT_THEME};

/// What a colour element says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum XlsxColor {
    /// No colour of its own: automatic, no element, or the system foreground.
    Auto,
    Rgb(u32),
    /// A colour the reader cannot turn into RGB: a number or tint out of range, a malformed value.
    Unknown,
}

impl XlsxColor {
    /// The RGB word, or `None` for automatic and unreadable colours.
    pub(crate) const fn rgb(self) -> Option<u32> {
        match self {
            Self::Rgb(rgb) => Some(rgb),
            Self::Auto | Self::Unknown => None,
        }
    }
}

/// The default `indexed` palette (ECMA-376 18.8.27): entries 0..=7 repeat 8..=15.
pub(crate) const DEFAULT_INDEXED: [u32; 64] = [
    0x000000, 0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00, 0xFF00FF, 0x00FFFF, 0x000000,
    0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00, 0xFF00FF, 0x00FFFF, 0x800000, 0x008000,
    0x000080, 0x808000, 0x800080, 0x008080, 0xC0C0C0, 0x808080, 0x9999FF, 0x993366, 0xFFFFCC,
    0xCCFFFF, 0x660066, 0xFF8080, 0x0066CC, 0xCCCCFF, 0x000080, 0xFF00FF, 0xFFFF00, 0x00FFFF,
    0x800080, 0x800000, 0x008080, 0x0000FF, 0x00CCFF, 0xCCFFFF, 0xCCFFCC, 0xFFFF99, 0x99CCFF,
    0xFF99CC, 0xCC99FF, 0xFFCC99, 0x3366FF, 0x33CCCC, 0x99CC00, 0xFFCC00, 0xFF9900, 0xFF6600,
    0x666699, 0x969696, 0x003366, 0x339966, 0x003300, 0x333300, 0x993300, 0x993366, 0x333399,
    0x333333,
];
/// `indexed` 64: the system foreground colour; 65: the system background colour.
const SYSTEM_FOREGROUND: usize = 64;
const SYSTEM_BACKGROUND: usize = 65;
/// Entries of an `indexedColors` override a reader looks at.
const MAX_PALETTE_ENTRIES: usize = 64;

/// `AARRGGBB` or `RRGGBB` as `0xRRGGBB`.
pub(crate) fn parse_argb(text: &str) -> Option<u32> {
    let hex = match text.len() {
        8 => text.get(2..)?,
        6 => text,
        _ => return None,
    };
    hex.bytes()
        .all(|b| b.is_ascii_hexdigit())
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

/// The theme and the `indexed` palette of one workbook.
#[derive(Debug, Clone)]
pub(crate) struct ColorScheme {
    theme: [u32; 12],
    indexed: [u32; 64],
}

impl ColorScheme {
    /// The theme given and the default indexed palette.
    pub(crate) const fn new(theme: [u32; 12]) -> Self {
        Self {
            theme,
            indexed: DEFAULT_INDEXED,
        }
    }

    /// The theme given and the palette of `styles_xml` (its `indexedColors` over the default).
    pub(crate) fn from_styles(theme: [u32; 12], styles_xml: &str) -> Self {
        let mut scheme = Self::new(theme);
        let Some(colors) = extract_tag_bodies(styles_xml, "colors").into_iter().next() else {
            return scheme;
        };
        let Some(list) = extract_tag_bodies(colors, "indexedColors")
            .into_iter()
            .next()
        else {
            return scheme;
        };
        let entries = extract_tags_or_self_closing_with_attrs(list, "rgbColor");
        for (slot, (attrs, _)) in scheme
            .indexed
            .iter_mut()
            .zip(entries.into_iter().take(MAX_PALETTE_ENTRIES))
        {
            if let Some(rgb) = extract_attr_from_fragment(attrs, "rgb").and_then(parse_argb) {
                *slot = rgb;
            }
        }
        scheme
    }

    /// The colour a `color`-like element with the attributes `attrs` says. `rgb` wins over `theme`,
    /// that over `indexed`; none of them (or `auto`) is automatic.
    pub(crate) fn resolve(&self, attrs: &str) -> XlsxColor {
        if let Some(rgb) = extract_attr_from_fragment(attrs, "rgb") {
            return parse_argb(rgb).map_or(XlsxColor::Unknown, XlsxColor::Rgb);
        }
        if let Some(index) = extract_attr_from_fragment(attrs, "theme") {
            let tint = extract_attr_from_fragment(attrs, "tint")
                .map_or(Some(0.0), |t| t.trim().parse::<f64>().ok())
                .filter(|t| t.is_finite() && (-1.0..=1.0).contains(t));
            let base = index
                .trim()
                .parse::<usize>()
                .ok()
                .and_then(|i| self.theme.get(i));
            return match (base, tint) {
                (Some(&base), Some(tint)) => XlsxColor::Rgb(apply_tint(base, tint)),
                _ => XlsxColor::Unknown,
            };
        }
        if let Some(index) = extract_attr_from_fragment(attrs, "indexed") {
            return match index.trim().parse::<usize>() {
                Ok(SYSTEM_FOREGROUND) => XlsxColor::Auto,
                Ok(SYSTEM_BACKGROUND) => XlsxColor::Rgb(0xFFFFFF),
                Ok(i) => self
                    .indexed
                    .get(i)
                    .map_or(XlsxColor::Unknown, |&rgb| XlsxColor::Rgb(rgb)),
                Err(_) => XlsxColor::Unknown,
            };
        }
        XlsxColor::Auto
    }

    /// The hyperlink colour of the theme (the "Hyperlink" cell style uses it).
    pub(crate) const fn hyperlink(&self) -> u32 {
        self.theme[10]
    }
}

impl Default for ColorScheme {
    fn default() -> Self {
        Self::new(DEFAULT_THEME)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scheme() -> ColorScheme {
        ColorScheme::default()
    }

    #[test]
    fn rgb_theme_tint_and_indexed_colours_resolve() {
        let s = scheme();
        assert_eq!(s.resolve("rgb=\"FF112233\""), XlsxColor::Rgb(0x112233));
        assert_eq!(s.resolve("rgb=\"abcdef\""), XlsxColor::Rgb(0xABCDEF));
        assert_eq!(s.resolve("theme=\"4\""), XlsxColor::Rgb(0x4472C4));
        assert_eq!(
            s.resolve("theme=\"0\" tint=\"-0.5\""),
            XlsxColor::Rgb(0x808080)
        );
        assert_eq!(s.resolve("indexed=\"10\""), XlsxColor::Rgb(0xFF0000));
        assert_eq!(s.resolve("indexed=\"2\""), XlsxColor::Rgb(0xFF0000));
        assert_eq!(s.resolve("indexed=\"64\""), XlsxColor::Auto);
        assert_eq!(s.resolve("indexed=\"65\""), XlsxColor::Rgb(0xFFFFFF));
        assert_eq!(s.resolve("auto=\"1\""), XlsxColor::Auto);
        assert_eq!(s.resolve(""), XlsxColor::Auto);
    }

    #[test]
    fn a_lightened_accent_colour_is_within_a_step_of_the_spreadsheet_value() {
        // Accent 4 of the 2007 default theme, 80% lighter: the spreadsheet program shows E5E0EC (its
        // integer luminance rounds the red channel down; the reference's whole-percent luminance gives
        // E5DFEC). The float shift is within one step of either.
        let accent = ColorScheme::new([
            0xFFFFFF, 0x000000, 0xEEECE1, 0x1F497D, 0x4F81BD, 0xC0504D, 0x9BBB59, 0x8064A2,
            0x4BACC6, 0xF79646, 0x0000FF, 0x800080,
        ]);
        let XlsxColor::Rgb(got) = accent.resolve("theme=\"7\" tint=\"0.79998168889431442\"") else {
            panic!("a theme colour resolves");
        };
        for (shift, want) in [(16, 0xE5), (8, 0xE0), (0, 0xEC)] {
            let channel = (got >> shift) & 0xFF;
            assert!(channel.abs_diff(want) <= 1, "{got:06X}");
        }
    }

    #[test]
    fn bad_values_are_unknown_and_never_panic() {
        let s = scheme();
        for attrs in [
            "rgb=\"12345\"",
            "rgb=\"GGGGGG\"",
            "rgb=\"aéxxxxx\"",
            "rgb=\"éééé\"",
            "theme=\"12\"",
            "theme=\"-1\"",
            "theme=\"4\" tint=\"1.5\"",
            "theme=\"4\" tint=\"NaN\"",
            "theme=\"4\" tint=\"x\"",
            "indexed=\"66\"",
            "indexed=\"-1\"",
            "indexed=\"99999999999999999999\"",
        ] {
            assert_eq!(s.resolve(attrs), XlsxColor::Unknown, "{attrs}");
        }
    }

    #[test]
    fn the_palette_override_replaces_entries_in_order_and_keeps_the_rest() {
        let xml = concat!(
            "<styleSheet><colors><indexedColors>",
            "<rgbColor rgb=\"FF000000\"/><rgbColor rgb=\"FFFFFFFF\"/><rgbColor rgb=\"FF010203\"/>",
            "<rgbColor rgb=\"bad\"/><rgbColor rgb=\"040506\"/>",
            "</indexedColors></colors></styleSheet>"
        );
        let s = ColorScheme::from_styles(DEFAULT_THEME, xml);
        assert_eq!(s.resolve("indexed=\"2\""), XlsxColor::Rgb(0x010203));
        assert_eq!(
            s.resolve("indexed=\"3\""),
            XlsxColor::Rgb(0x00FF00),
            "malformed keeps the default"
        );
        assert_eq!(s.resolve("indexed=\"4\""), XlsxColor::Rgb(0x040506));
        assert_eq!(s.resolve("indexed=\"5\""), XlsxColor::Rgb(0xFFFF00));
        assert_eq!(s.resolve("indexed=\"63\""), XlsxColor::Rgb(0x333333));
    }

    #[test]
    fn a_huge_palette_override_is_cut_at_64_entries() {
        let mut xml = String::from("<styleSheet><colors><indexedColors>");
        for _ in 0..100_000 {
            xml.push_str("<rgbColor rgb=\"FF123456\"/>");
        }
        xml.push_str("</indexedColors></colors></styleSheet>");
        let s = ColorScheme::from_styles(DEFAULT_THEME, &xml);
        assert_eq!(s.resolve("indexed=\"63\""), XlsxColor::Rgb(0x123456));
        assert_eq!(s.resolve("indexed=\"65\""), XlsxColor::Rgb(0xFFFFFF));
    }
}
