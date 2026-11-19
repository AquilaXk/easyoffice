//! Word-packed table cell fill, border edges, column-width bounds, and cell image entries.

use crate::rich_image::RichImage;

/// Narrowest allowed table column width in twips (~2.5 mm).
pub const MIN_COL_WIDTH_TWIPS: u32 = 144;
/// Widest allowed table column width in twips (22 in).
pub const MAX_COL_WIDTH_TWIPS: u32 = 31_680;
/// Maximum number of styled cells kept per table.
pub const MAX_STYLED_TABLE_CELLS: usize = 200_000;

const HEX_UPPER: &[u8; 16] = b"0123456789ABCDEF";

/// Appends 6 uppercase RGB hex characters without allocating.
#[inline]
pub(crate) fn write_rgb_hex(rgb: u32, out: &mut String) {
    let mut buf = [0u8; 6];
    let mut i = 0usize;
    while i < 6 {
        let shift = (5 - i) * 4;
        buf[i] = HEX_UPPER[((rgb >> shift) & 0xF) as usize];
        i += 1;
    }
    for b in buf {
        out.push(b as char);
    }
}

/// Parses `"RRGGBB"` or `"#RRGGBB"` into a 24-bit RGB integer without heap allocation.
#[inline]
pub(crate) fn parse_hex_rgb(s: &str) -> Option<u32> {
    let trimmed = s.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);
    let bytes = hex.as_bytes();
    if bytes.len() != 6 {
        return None;
    }
    let mut acc = 0u32;
    for &b in bytes {
        let nibble = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            b'A'..=b'F' => b - b'A' + 10,
            _ => return None,
        };
        acc = (acc << 4) | u32::from(nibble);
    }
    Some(acc)
}

/// Packed 32-bit cell background fill (`0` = none, `(1 << 24) | rgb` = solid RGB).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CellFill(u32);

impl CellFill {
    pub const NONE: Self = Self(0);

    #[inline]
    pub const fn none() -> Self {
        Self(0)
    }

    #[inline]
    pub const fn solid(rgb: u32) -> Self {
        Self((1 << 24) | (rgb & 0x00FF_FFFF))
    }

    #[inline]
    pub const fn is_none(self) -> bool {
        self.0 == 0
    }

    /// Parses `"RRGGBB"` or `"#RRGGBB"`; rejects `"auto"`, `"transparent"`, `"none"`, etc.
    #[inline]
    pub fn from_hex(s: &str) -> Option<Self> {
        parse_hex_rgb(s).map(Self::solid)
    }

    #[inline]
    pub const fn rgb(self) -> Option<u32> {
        if (self.0 >> 24) == 1 {
            Some(self.0 & 0x00FF_FFFF)
        } else {
            None
        }
    }

    /// Writes 6 uppercase hex digits into `out` when solid, with zero extra allocation.
    #[inline]
    pub fn write_hex(self, out: &mut String) {
        if let Some(rgb) = self.rgb() {
            write_rgb_hex(rgb, out);
        }
    }
}

/// Border line style for one cell edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum TableBorderKind {
    #[default]
    Unspecified = 0,
    None = 1,
    Thin = 2,
    Thick = 3,
    Dashed = 4,
    Dotted = 5,
    Double = 6,
}

impl TableBorderKind {
    #[inline]
    pub const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::None,
            2 => Self::Thin,
            3 => Self::Thick,
            4 => Self::Dashed,
            5 => Self::Dotted,
            6 => Self::Double,
            _ => Self::Unspecified,
        }
    }

    #[inline]
    pub const fn wire_name(self) -> Option<&'static str> {
        match self {
            Self::Unspecified => None,
            Self::None => Some("none"),
            Self::Thin => Some("thin"),
            Self::Thick => Some("thick"),
            Self::Dashed => Some("dashed"),
            Self::Dotted => Some("dotted"),
            Self::Double => Some("double"),
        }
    }

    #[inline]
    pub fn from_wire_name(s: &str) -> Option<Self> {
        match s.trim() {
            "none" => Some(Self::None),
            "thin" => Some(Self::Thin),
            "thick" => Some(Self::Thick),
            "dashed" => Some(Self::Dashed),
            "dotted" => Some(Self::Dotted),
            "double" => Some(Self::Double),
            _ => None,
        }
    }
}

/// Packed 32-bit border edge: `0` = unspecified, `(1 << 24)` = hidden, `(kind << 24) | rgb` = line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TableBorderEdge(u32);

impl TableBorderEdge {
    pub const UNSPECIFIED: Self = Self(0);
    pub const NONE: Self = Self((TableBorderKind::None as u32) << 24);

    #[inline]
    pub const fn new(kind: TableBorderKind, rgb: u32) -> Self {
        match kind {
            TableBorderKind::Unspecified => Self::UNSPECIFIED,
            TableBorderKind::None => Self::NONE,
            k => Self(((k as u32) << 24) | (rgb & 0x00FF_FFFF)),
        }
    }

    #[inline]
    pub const fn kind(self) -> TableBorderKind {
        TableBorderKind::from_u8((self.0 >> 24) as u8)
    }

    #[inline]
    pub const fn rgb(self) -> u32 {
        self.0 & 0x00FF_FFFF
    }

    #[inline]
    pub const fn is_unspecified(self) -> bool {
        (self.0 >> 24) == 0
    }

    /// Writes 6 uppercase hex digits of the edge RGB color into `out` without allocating.
    #[inline]
    pub fn write_hex(self, out: &mut String) {
        write_rgb_hex(self.rgb(), out);
    }
}

/// Per-cell 4-edge borders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TableCellBorders {
    pub top: TableBorderEdge,
    pub right: TableBorderEdge,
    pub bottom: TableBorderEdge,
    pub left: TableBorderEdge,
}

impl TableCellBorders {
    #[inline]
    pub const fn is_unspecified(&self) -> bool {
        self.top.is_unspecified()
            && self.right.is_unspecified()
            && self.bottom.is_unspecified()
            && self.left.is_unspecified()
    }
}

/// Shading and border overrides for cell `(row, col)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TableCellStyleEntry {
    pub row: u32,
    pub col: u32,
    pub fill: CellFill,
    pub borders: TableCellBorders,
}

/// Inline picture placed inside cell `(row, col)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableCellImage {
    pub row: u32,
    pub col: u32,
    pub image: RichImage,
}

/// Binary-searches or advances a cursor across `(row, col)`-sorted style entries.
#[inline]
pub(crate) fn next_cell_style(
    styles: &[TableCellStyleEntry],
    cursor: &mut usize,
    row: usize,
    col: usize,
) -> Option<(CellFill, TableCellBorders)> {
    let target = (row as u32, col as u32);
    while *cursor < styles.len() && (styles[*cursor].row, styles[*cursor].col) < target {
        *cursor += 1;
    }
    let entry = styles.get(*cursor)?;
    if (entry.row, entry.col) == target {
        *cursor += 1;
        Some((entry.fill, entry.borders))
    } else {
        None
    }
}

/// Advances a cursor across `(row, col)`-sorted cell images, returning the slice at `(row, col)`.
#[inline]
pub(crate) fn slice_cell_images<'a>(
    images: &'a [&'a TableCellImage],
    cursor: &mut usize,
    row: usize,
    col: usize,
) -> &'a [&'a TableCellImage] {
    let target = (row as u32, col as u32);
    while *cursor < images.len() && (images[*cursor].row, images[*cursor].col) < target {
        *cursor += 1;
    }
    let start = *cursor;
    while *cursor < images.len() && (images[*cursor].row, images[*cursor].col) == target {
        *cursor += 1;
    }
    &images[start..*cursor]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_fill_and_border_edge_pack_and_format_without_alloc() {
        assert!(CellFill::default().is_none());
        assert_eq!(CellFill::default().rgb(), None);
        assert!(CellFill::from_hex("auto").is_none());
        assert!(CellFill::from_hex("transparent").is_none());
        assert!(CellFill::from_hex("none").is_none());
        assert!(CellFill::from_hex("#12345").is_none());
        let fill = CellFill::from_hex("#a1b2c3").unwrap();
        assert_eq!(fill.rgb(), Some(0xA1B2C3));
        let mut hex = String::new();
        fill.write_hex(&mut hex);
        assert_eq!(hex, "A1B2C3");

        let edge = TableBorderEdge::new(TableBorderKind::Double, 0x1E40AF);
        assert_eq!(edge.kind(), TableBorderKind::Double);
        assert_eq!(edge.rgb(), 0x1E40AF);
        assert!(!edge.is_unspecified());
        hex.clear();
        edge.write_hex(&mut hex);
        assert_eq!(hex, "1E40AF");

        assert!(TableBorderEdge::UNSPECIFIED.is_unspecified());
        assert_eq!(TableBorderEdge::NONE.kind(), TableBorderKind::None);
        assert!(TableCellBorders::default().is_unspecified());
    }
}
