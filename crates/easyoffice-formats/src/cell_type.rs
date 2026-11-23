//! Explicit spreadsheet cell value kinds and sparse delta helpers (PD-241).
//!
//! 참조 구현 (reference implementation): calamine 0.38.0 (MIT, Copyright (c) 2016 Johann Tuffe),
//! https://github.com/tafia/calamine, `src/datatype.rs` (`Data` variants) and `src/xlsx/mod.rs`
//! (`ST_CellType` dispatch), together with ECMA-376 Part 1 §18.18.11 (`ST_CellType`: `n`, `s`,
//! `b`, `e`, `str`, `inlineStr`), OASIS ODF 1.3 §19.385 (`office:value-type`), and MS-XLS §2.4.24
//! / §2.5.97 (`BoolErr` / `BErr`).
//!
//! Optimization: [`CellValueKind`] is a 1-byte `#[repr(u8)]` tag, and [`SheetStyling::cell_types`]
//! stores an entry in `FastU64Map<CellValueKind>` keyed by packed `(row << 32) | col` *only* when a
//! cell's explicit kind differs from [`CellValueKind::infer`], keeping zero map overhead on normal
//! numeric, string, boolean, error, and formula cells.

use crate::cell_numbers::numeric_cell_value;

/// Explicit value type of a populated spreadsheet cell.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CellValueKind {
    Number = 1,
    Text = 2,
    Boolean = 3,
    Error = 4,
    Formula = 5,
}

impl CellValueKind {
    /// Short wire tag (`"n"`, `"s"`, `"b"`, `"e"`, `"f"`) matching ECMA-376 `ST_CellType`.
    #[inline]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Number => "n",
            Self::Text => "s",
            Self::Boolean => "b",
            Self::Error => "e",
            Self::Formula => "f",
        }
    }

    /// Parses a short wire tag; returns `None` on unknown tags so callers fail closed.
    #[inline]
    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "n" => Some(Self::Number),
            "s" => Some(Self::Text),
            "b" => Some(Self::Boolean),
            "e" => Some(Self::Error),
            "f" => Some(Self::Formula),
            _ => None,
        }
    }

    /// Infers the default cell value kind from `raw` via a 1-byte prefix guard before float parsing.
    #[inline]
    pub fn infer(raw: &str) -> Self {
        match raw.as_bytes().first() {
            None => Self::Text,
            Some(b'=') => Self::Formula,
            Some(b'#') => {
                if is_error_literal(raw) {
                    Self::Error
                } else {
                    Self::Text
                }
            }
            Some(b'T' | b'F') => {
                if raw == "TRUE" || raw == "FALSE" {
                    Self::Boolean
                } else {
                    Self::Text
                }
            }
            Some(b'-' | b'.' | b'0'..=b'9') => {
                if numeric_cell_value(raw).is_some() {
                    Self::Number
                } else {
                    Self::Text
                }
            }
            Some(_) => Self::Text,
        }
    }
}

/// Packs zero-based `(row, col)` into a single 64-bit lookup key.
#[inline]
pub const fn pack_cell_pos(row: u32, col: u32) -> u64 {
    ((row as u64) << 32) | (col as u64)
}

/// Unpacks a 64-bit key from [`pack_cell_pos`] into zero-based `(row, col)`.
#[inline]
pub const fn unpack_cell_pos(packed: u64) -> (u32, u32) {
    ((packed >> 32) as u32, packed as u32)
}

/// Standard spreadsheet error literals recognized in OOXML (`ST_CellType` `e`), ODF, and BIFF8.
#[inline]
pub fn is_error_literal(raw: &str) -> bool {
    matches!(
        raw,
        "#NULL!"
            | "#DIV/0!"
            | "#VALUE!"
            | "#REF!"
            | "#NAME?"
            | "#NUM!"
            | "#N/A"
            | "#SPILL!"
            | "#CALC!"
            | "#CONNECT!"
            | "#BLOCKED!"
            | "#GETTING_DATA"
            | "#ERROR!"
    )
}

/// MS-XLS §2.5.97 `BErr` byte code for a standard BIFF8 error literal.
#[inline]
pub(crate) fn biff8_error_code(raw: &str) -> Option<u8> {
    match raw {
        "#NULL!" => Some(0x00),
        "#DIV/0!" => Some(0x07),
        "#VALUE!" => Some(0x0F),
        "#REF!" => Some(0x17),
        "#NAME?" => Some(0x1D),
        "#NUM!" => Some(0x24),
        "#N/A" => Some(0x2A),
        "#GETTING_DATA" => Some(0x2B),
        _ => None,
    }
}

/// Canonical display text of an MS-XLS §2.5.97 `BErr` error code.
#[inline]
pub(crate) const fn biff8_error_str(code: u8) -> &'static str {
    match code {
        0x00 => "#NULL!",
        0x07 => "#DIV/0!",
        0x0F => "#VALUE!",
        0x17 => "#REF!",
        0x1D => "#NAME?",
        0x24 => "#NUM!",
        0x2A => "#N/A",
        0x2B => "#GETTING_DATA",
        _ => "#ERROR!",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_classifies_canonical_literals_and_rejects_ambiguous_text() {
        assert_eq!(CellValueKind::infer("42"), CellValueKind::Number);
        assert_eq!(CellValueKind::infer("-0.25"), CellValueKind::Number);
        assert_eq!(CellValueKind::infer("03045"), CellValueKind::Text);
        assert_eq!(CellValueKind::infer("+821012345678"), CellValueKind::Text);
        assert_eq!(CellValueKind::infer("TRUE"), CellValueKind::Boolean);
        assert_eq!(CellValueKind::infer("FALSE"), CellValueKind::Boolean);
        assert_eq!(CellValueKind::infer("true"), CellValueKind::Text);
        assert_eq!(CellValueKind::infer("#N/A"), CellValueKind::Error);
        assert_eq!(CellValueKind::infer("#DIV/0!"), CellValueKind::Error);
        assert_eq!(CellValueKind::infer("#BLOCKED!"), CellValueKind::Error);
        assert_eq!(CellValueKind::infer("#CONNECT!"), CellValueKind::Error);
        assert_eq!(CellValueKind::infer("#TAG"), CellValueKind::Text);
        assert_eq!(CellValueKind::infer("=SUM(A1:A3)"), CellValueKind::Formula);
        assert_eq!(CellValueKind::infer(" 42"), CellValueKind::Text);
    }

    #[test]
    fn tags_and_packed_positions_roundtrip() {
        for kind in [
            CellValueKind::Number,
            CellValueKind::Text,
            CellValueKind::Boolean,
            CellValueKind::Error,
            CellValueKind::Formula,
        ] {
            assert_eq!(CellValueKind::from_tag(kind.tag()), Some(kind));
        }
        assert_eq!(CellValueKind::from_tag("str"), None);
        assert_eq!(
            unpack_cell_pos(pack_cell_pos(1_048_575, 16_383)),
            (1_048_575, 16_383)
        );
    }
}
