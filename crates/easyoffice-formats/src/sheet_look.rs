//! Cell alignment, text wrapping and borders (PD-207): the model types and the vocabulary of the
//! two file formats (`alignment` and `border` of `styles.xml`, the cell and paragraph properties
//! of an ODS automatic style).
//!
//! A border edge is one 32-bit word (line kind in bits 24..=26, RGB in the low 24 bits), a cell's
//! four edges are four words, so a whole look compares, hashes and deduplicates as plain words.

/// Horizontal alignment; `General` follows the value (numbers right, text left).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum HAlign {
    #[default]
    General = 0,
    Left = 1,
    Center = 2,
    Right = 3,
}

/// Vertical alignment; the spreadsheet default is the bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum VAlign {
    #[default]
    Bottom = 0,
    Top = 1,
    Center = 2,
}

/// The line kinds of a border edge the editor can draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum BorderLine {
    #[default]
    None = 0,
    Thin = 1,
    Thick = 2,
    Dashed = 3,
    Dotted = 4,
    Double = 5,
}

const LINE_SHIFT: u32 = 24;
const RGB_MASK: u32 = 0x00FF_FFFF;
/// The color of an edge written without one (`auto` in a spreadsheet file).
pub const BORDER_DEFAULT_RGB: u32 = 0;

/// One border edge: a line kind and an RGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BorderEdge(u32);

impl BorderEdge {
    pub const NONE: Self = Self(0);

    /// An edge of `line` in `rgb` (bits above 24 are dropped); `None` drops the color too, so
    /// every "no edge" is the same word.
    pub const fn new(line: BorderLine, rgb: u32) -> Self {
        match line {
            BorderLine::None => Self(0),
            _ => Self(((line as u32) << LINE_SHIFT) | (rgb & RGB_MASK)),
        }
    }

    pub const fn line(self) -> BorderLine {
        match self.0 >> LINE_SHIFT {
            1 => BorderLine::Thin,
            2 => BorderLine::Thick,
            3 => BorderLine::Dashed,
            4 => BorderLine::Dotted,
            5 => BorderLine::Double,
            _ => BorderLine::None,
        }
    }

    pub const fn rgb(self) -> u32 {
        self.0 & RGB_MASK
    }

    pub const fn is_none(self) -> bool {
        self.0 == 0
    }
}

/// The four edges of a cell, top, right, bottom, left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CellBorders {
    pub top: BorderEdge,
    pub right: BorderEdge,
    pub bottom: BorderEdge,
    pub left: BorderEdge,
}

impl CellBorders {
    pub const fn is_none(&self) -> bool {
        self.top.is_none() && self.right.is_none() && self.bottom.is_none() && self.left.is_none()
    }
}

impl HAlign {
    /// `horizontal` of a SpreadsheetML `alignment`; kinds the editor has no equivalent for keep
    /// their nearest one (`justify` and `distributed` read as left, `centerContinuous` as center).
    pub(crate) fn from_xlsx(value: &str) -> Self {
        match value {
            "left" | "justify" | "distributed" => Self::Left,
            "center" | "centerContinuous" => Self::Center,
            "right" => Self::Right,
            _ => Self::General,
        }
    }

    pub(crate) const fn xlsx(self) -> Option<&'static str> {
        match self {
            Self::General => None,
            Self::Left => Some("left"),
            Self::Center => Some("center"),
            Self::Right => Some("right"),
        }
    }

    /// `fo:text-align` of an ODS paragraph style.
    pub(crate) fn from_ods(value: &str) -> Self {
        match value {
            "left" | "start" | "justify" => Self::Left,
            "center" => Self::Center,
            "right" | "end" => Self::Right,
            _ => Self::General,
        }
    }

    pub(crate) const fn ods(self) -> Option<&'static str> {
        self.xlsx()
    }

    /// The value of the editor's save payload (`ha`).
    pub fn from_payload(value: &str) -> Option<Self> {
        match value {
            "left" => Some(Self::Left),
            "center" => Some(Self::Center),
            "right" => Some(Self::Right),
            _ => None,
        }
    }

    /// The payload word of an explicit alignment.
    pub const fn payload(self) -> Option<&'static str> {
        self.xlsx()
    }
}

impl VAlign {
    pub(crate) fn from_xlsx(value: &str) -> Self {
        match value {
            "top" | "justify" | "distributed" => Self::Top,
            "center" => Self::Center,
            _ => Self::Bottom,
        }
    }

    pub(crate) const fn xlsx(self) -> Option<&'static str> {
        match self {
            Self::Bottom => None,
            Self::Top => Some("top"),
            Self::Center => Some("center"),
        }
    }

    /// `style:vertical-align` of an ODS cell style.
    pub(crate) fn from_ods(value: &str) -> Self {
        match value {
            "top" => Self::Top,
            "middle" => Self::Center,
            _ => Self::Bottom,
        }
    }

    pub(crate) const fn ods(self) -> Option<&'static str> {
        match self {
            Self::Bottom => None,
            Self::Top => Some("top"),
            Self::Center => Some("middle"),
        }
    }

    /// The value of the editor's save payload (`va`).
    pub fn from_payload(value: &str) -> Option<Self> {
        match value {
            "top" => Some(Self::Top),
            "middle" => Some(Self::Center),
            "bottom" => Some(Self::Bottom),
            _ => None,
        }
    }

    /// The payload word of an explicit alignment (the bottom is the default and has none).
    pub const fn payload(self) -> Option<&'static str> {
        self.ods()
    }
}

impl BorderLine {
    /// The `style` of a SpreadsheetML border side, folded onto the kinds the editor draws.
    pub(crate) fn from_xlsx(value: &str) -> Self {
        match value {
            "thin" | "hair" => Self::Thin,
            "medium" | "thick" => Self::Thick,
            "dashed" | "dashDot" | "dashDotDot" | "mediumDashed" | "mediumDashDot"
            | "mediumDashDotDot" | "slantDashDot" => Self::Dashed,
            "dotted" => Self::Dotted,
            "double" => Self::Double,
            _ => Self::None,
        }
    }

    pub(crate) const fn xlsx(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Thin => Some("thin"),
            Self::Thick => Some("medium"),
            Self::Dashed => Some("dashed"),
            Self::Dotted => Some("dotted"),
            Self::Double => Some("double"),
        }
    }

    /// The value of the editor's save payload (`bd`).
    pub fn from_payload(value: &str) -> Option<Self> {
        match value {
            "thin" => Some(Self::Thin),
            "thick" => Some(Self::Thick),
            "dashed" => Some(Self::Dashed),
            "dotted" => Some(Self::Dotted),
            "double" => Some(Self::Double),
            _ => None,
        }
    }

    pub const fn payload(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Thin => Some("thin"),
            Self::Thick => Some("thick"),
            Self::Dashed => Some("dashed"),
            Self::Dotted => Some("dotted"),
            Self::Double => Some("double"),
        }
    }
}

/// `rrggbb` (six hex digits) as an RGB word; `None` for anything else.
pub fn parse_hex_rgb(text: &str) -> Option<u32> {
    if text.len() != 6 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(text, 16).ok()
}

/// The color of a SpreadsheetML `color` element: an `rgb` of `RRGGBB` or `AARRGGBB`; a theme,
/// indexed or automatic color reads as the default (black).
pub(crate) fn xlsx_color_rgb(attrs: &str) -> u32 {
    crate::rtf_xml_helpers::extract_attr_from_fragment(attrs, "rgb")
        .and_then(crate::sheet_styling::xlsx_color::parse_argb)
        .unwrap_or(BORDER_DEFAULT_RGB)
}

/// `0.74pt solid #112233` (an `fo:border*` value) as an edge: the line kind from the style word
/// and the width, the color from the hex word.
pub(crate) fn ods_border_edge(value: &str) -> BorderEdge {
    let mut width_pt = 0.0f64;
    let mut line = BorderLine::None;
    let mut rgb = BORDER_DEFAULT_RGB;
    for word in value.split_whitespace() {
        if let Some(hex) = word.strip_prefix('#') {
            rgb = parse_hex_rgb(hex).unwrap_or(BORDER_DEFAULT_RGB);
        } else if let Some(number) = word.strip_suffix("pt") {
            width_pt = number.parse().unwrap_or(0.0);
        } else {
            line = match word {
                "solid" => BorderLine::Thin,
                "double" => BorderLine::Double,
                "dotted" => BorderLine::Dotted,
                "dashed" | "dot-dash" | "dot-dot-dash" => BorderLine::Dashed,
                _ => BorderLine::None,
            };
        }
    }
    if line == BorderLine::Thin && width_pt >= 1.2 {
        line = BorderLine::Thick;
    }
    BorderEdge::new(line, rgb)
}

/// An edge as an `fo:border*` value (`none` for no edge).
pub(crate) fn ods_border_value(edge: BorderEdge) -> String {
    let (width, word) = match edge.line() {
        BorderLine::None => return "none".to_string(),
        BorderLine::Thin => ("0.74pt", "solid"),
        BorderLine::Thick => ("2.25pt", "solid"),
        BorderLine::Dashed => ("0.74pt", "dashed"),
        BorderLine::Dotted => ("0.74pt", "dotted"),
        BorderLine::Double => ("1.5pt", "double"),
    };
    format!("{width} {word} #{:06x}", edge.rgb())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_packs_line_and_color_and_none_has_no_color() {
        let edge = BorderEdge::new(BorderLine::Double, 0xFF12_3456);
        assert_eq!(edge.line(), BorderLine::Double);
        assert_eq!(edge.rgb(), 0x12_3456);
        assert_eq!(
            BorderEdge::new(BorderLine::None, 0xABCDEF),
            BorderEdge::NONE
        );
    }

    #[test]
    fn hex_colors_are_exactly_six_digits() {
        assert_eq!(parse_hex_rgb("00ff7f"), Some(0x00FF7F));
        for bad in ["", "fff", "00ff7", "00ff7f0", "00ff7g", "#00ff7"] {
            assert_eq!(parse_hex_rgb(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn xlsx_color_reads_argb_rgb_and_falls_back_to_black() {
        assert_eq!(xlsx_color_rgb(r#" rgb="FF0000FF""#), 0x0000FF);
        assert_eq!(xlsx_color_rgb(r#" rgb="336699""#), 0x336699);
        assert_eq!(xlsx_color_rgb(r#" theme="1""#), 0);
        assert_eq!(xlsx_color_rgb(r#" rgb="zzzzzzzz""#), 0);
    }

    #[test]
    fn ods_edge_folds_width_and_style() {
        assert_eq!(
            ods_border_edge("0.74pt solid #ff0000"),
            BorderEdge::new(BorderLine::Thin, 0xFF0000)
        );
        assert_eq!(
            ods_border_edge("2.25pt solid #000000").line(),
            BorderLine::Thick
        );
        assert_eq!(
            ods_border_edge("1pt dashed #000000").line(),
            BorderLine::Dashed
        );
        assert_eq!(ods_border_edge("none"), BorderEdge::NONE);
        assert_eq!(ods_border_edge("garbage #zzzzzz"), BorderEdge::NONE);
    }

    #[test]
    fn ods_value_round_trips_every_line() {
        for line in [
            BorderLine::Thin,
            BorderLine::Thick,
            BorderLine::Dashed,
            BorderLine::Dotted,
            BorderLine::Double,
        ] {
            let edge = BorderEdge::new(line, 0x0A0B0C);
            assert_eq!(ods_border_edge(&ods_border_value(edge)), edge, "{line:?}");
        }
    }
}
