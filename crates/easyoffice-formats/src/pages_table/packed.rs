//! Packed table cell decoding for the Pages reader: direct little-endian reads on the row buffer
//! slice, flag-driven field offsets and decimal128 values. No allocation; every offset
//! computation is checked. Layout numbers live in [`crate::pages_schema::cell`].

use crate::pages_schema::cell;

/// A decimal value `(-1)^negative * coefficient * 10^exponent` (BID decimal128 whose coefficient
/// fits 64 bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Decimal {
    pub negative: bool,
    pub coefficient: u64,
    pub exponent: i32,
}

/// Bias of the BID decimal128 exponent.
const BID_EXPONENT_BIAS: i32 = 6176;
/// Bit position of the exponent inside the high word.
const BID_EXPONENT_SHIFT: u32 = 49;
const BID_EXPONENT_MASK: u64 = 0x3FFF;
/// Mask of the coefficient bits stored in the high word.
const BID_HIGH_COEFFICIENT_MASK: u64 = (1u64 << BID_EXPONENT_SHIFT) - 1;
/// Combination bits that select the large-coefficient / special-value encodings.
const BID_SPECIAL_SHIFT: u32 = 61;
const BID_SPECIAL_MASK: u64 = 3;

impl Decimal {
    /// Decodes 16 little-endian bytes; `None` for coefficients above 64 bits and specials.
    pub fn from_bid128(raw: &[u8]) -> Option<Self> {
        let low = u64::from_le_bytes(raw.get(..8)?.try_into().ok()?);
        let high = u64::from_le_bytes(raw.get(8..16)?.try_into().ok()?);
        if (high >> BID_SPECIAL_SHIFT) & BID_SPECIAL_MASK == BID_SPECIAL_MASK
            || high & BID_HIGH_COEFFICIENT_MASK != 0
        {
            return None;
        }
        let biased = i32::try_from((high >> BID_EXPONENT_SHIFT) & BID_EXPONENT_MASK).ok()?;
        Some(Self {
            negative: high >> 63 == 1,
            coefficient: low,
            exponent: biased - BID_EXPONENT_BIAS,
        })
    }
}

/// Scalar stored in a cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum CellScalar {
    None,
    Decimal(Decimal),
    /// Date seconds since 2001-01-01, or a checkbox value.
    Double(f64),
}

/// Decoded cell header: type, keys and scalar value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PackedCell {
    pub kind: u8,
    pub rich_text_key: Option<u32>,
    pub string_key: Option<u32>,
    pub format_key: Option<u32>,
    pub value: CellScalar,
}

fn read_u32(buf: &[u8], at: usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    Some(u32::from_le_bytes(buf.get(at..end)?.try_into().ok()?))
}

/// Parses the cell stored at `offset` in a row buffer; `None` for unknown layouts.
pub(crate) fn parse_cell(buf: &[u8], offset: usize) -> Option<PackedCell> {
    let head = buf.get(offset..)?;
    match *head.first()? {
        cell::LEGACY_VERSION => parse_legacy(head),
        cell::CURRENT_VERSION => parse_current(head),
        _ => None,
    }
}

fn parse_legacy(head: &[u8]) -> Option<PackedCell> {
    let kind = *head.get(1)?;
    let flags = read_u32(head, cell::LEGACY_FLAGS_OFFSET)?;
    if matches!(
        kind,
        cell::TYPE_NUMBER
            | cell::TYPE_STRING
            | cell::TYPE_DURATION
            | cell::TYPE_ERROR
            | cell::TYPE_BOOL
    ) {
        return parse_legacy_scalar(head, kind, flags);
    }
    let mut rich_text_key = None;
    if kind == cell::TYPE_RICH_TEXT && flags & cell::LEGACY_FLAG_RICH_TEXT_KEY != 0 {
        let below = (flags & (cell::LEGACY_FLAG_RICH_TEXT_KEY - 1)).count_ones() as usize;
        let at = cell::LEGACY_FIELDS_OFFSET.checked_add(below.checked_mul(4)?)?;
        rich_text_key = Some(read_u32(head, at)?);
    }
    Some(PackedCell {
        kind,
        rich_text_key,
        string_key: None,
        format_key: None,
        value: CellScalar::None,
    })
}

/// Size of the field of legacy flag bit `bit`; `None` for bits whose size is unknown.
fn legacy_field_size(bit: u32) -> Option<usize> {
    match bit {
        cell::LEGACY_BIT_DECIMAL => Some(cell::DECIMAL_BYTES),
        cell::LEGACY_BIT_DOUBLE => Some(cell::DOUBLE_BYTES),
        b if cell::LEGACY_FOUR_BYTE_BITS & (1 << b) != 0 => Some(cell::ASSUMED_FIELD_BYTES),
        _ => None,
    }
}

/// Number, string, duration and error cells of the version-5 layout. Cells with a flag bit of
/// unknown field size are not read (fail closed).
fn parse_legacy_scalar(head: &[u8], kind: u8, flags: u32) -> Option<PackedCell> {
    if flags & cell::LEGACY_UNVERIFIED_BITS != 0 {
        return None;
    }
    let mut out = PackedCell {
        kind,
        rich_text_key: None,
        string_key: None,
        format_key: None,
        value: CellScalar::None,
    };
    let mut cursor = cell::LEGACY_FIELDS_OFFSET;
    let mut mask = flags & ((1u32 << (cell::LEGACY_LAST_BIT + 1)) - 1);
    while mask != 0 {
        let bit = mask.trailing_zeros();
        mask &= mask - 1;
        let end = cursor.checked_add(legacy_field_size(bit)?)?;
        let field = head.get(cursor..end)?;
        match bit {
            cell::LEGACY_BIT_DECIMAL if kind == cell::TYPE_NUMBER => {
                out.value = decode_scalar(kind, field)?;
            }
            cell::LEGACY_BIT_DOUBLE if matches!(kind, cell::TYPE_DURATION | cell::TYPE_BOOL) => {
                out.value = decode_scalar(kind, field)?;
            }
            cell::LEGACY_BIT_STRING_KEY => out.string_key = Some(read_u32(field, 0)?),
            cell::LEGACY_BIT_RICH_TEXT_KEY => out.rich_text_key = Some(read_u32(field, 0)?),
            b if out.format_key.is_none() && (cell::LEGACY_FORMAT_MASK & (1 << b) != 0) => {
                out.format_key = Some(read_u32(field, 0)?);
            }
            _ => {}
        }
        cursor = end;
    }
    Some(out)
}

/// Byte size of the field selected by flag `bit` for a cell of type `kind`.
fn field_size(bit: u32, kind: u8) -> Option<usize> {
    if bit != cell::VALUE_BIT {
        return Some(cell::ASSUMED_FIELD_BYTES);
    }
    match kind {
        cell::TYPE_NUMBER => Some(cell::DECIMAL_BYTES),
        cell::TYPE_DATE | cell::TYPE_BOOL | cell::TYPE_DURATION => Some(cell::DOUBLE_BYTES),
        _ => None,
    }
}

fn decode_scalar(kind: u8, field: &[u8]) -> Option<CellScalar> {
    if kind == cell::TYPE_NUMBER {
        return Decimal::from_bid128(field).map(CellScalar::Decimal);
    }
    let value = f64::from_le_bytes(field.try_into().ok()?);
    value.is_finite().then_some(CellScalar::Double(value))
}

fn parse_current(head: &[u8]) -> Option<PackedCell> {
    let kind = *head.get(1)?;
    let flags = read_u32(head, cell::FLAGS_OFFSET)?;
    let mut out = PackedCell {
        kind,
        rich_text_key: None,
        string_key: None,
        format_key: None,
        value: CellScalar::None,
    };
    let mut cursor = cell::FIELDS_OFFSET;
    let mut mask = flags & ((1u32 << cell::KNOWN_FLAG_BITS) - 1);
    while mask != 0 {
        let bit = mask.trailing_zeros();
        mask &= mask - 1;
        let size = field_size(bit, kind)?;
        let end = cursor.checked_add(size)?;
        let field = head.get(cursor..end)?;
        match bit {
            cell::VALUE_BIT => out.value = decode_scalar(kind, field)?,
            cell::RICH_TEXT_KEY_BIT => out.rich_text_key = Some(read_u32(field, 0)?),
            cell::FORMAT_KEY_BIT => out.format_key = Some(read_u32(field, 0)?),
            _ => {}
        }
        cursor = end;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bid(coefficient: u64, exponent: i32, negative: bool) -> [u8; 16] {
        let mut raw = [0u8; 16];
        raw[..8].copy_from_slice(&coefficient.to_le_bytes());
        let biased = u64::try_from(exponent + BID_EXPONENT_BIAS).unwrap();
        let high = (biased << BID_EXPONENT_SHIFT) | (u64::from(negative) << 63);
        raw[8..].copy_from_slice(&high.to_le_bytes());
        raw
    }

    #[test]
    fn decimal128_round_trips_sign_and_exponent() {
        let d = Decimal::from_bid128(&bid(12345, -1, false)).unwrap();
        assert_eq!((d.coefficient, d.exponent, d.negative), (12345, -1, false));
        let n = Decimal::from_bid128(&bid(7, 3, true)).unwrap();
        assert_eq!((n.coefficient, n.exponent, n.negative), (7, 3, true));
    }

    #[test]
    fn wide_coefficients_and_specials_are_rejected() {
        let mut raw = bid(1, 0, false);
        raw[8] = 1;
        assert!(Decimal::from_bid128(&raw).is_none());
        raw[15] = 0x78;
        assert!(Decimal::from_bid128(&raw).is_none());
        assert!(Decimal::from_bid128(&[0u8; 8]).is_none());
    }

    #[test]
    fn truncated_cells_are_none() {
        let mut cell_bytes = vec![cell::CURRENT_VERSION, cell::TYPE_RICH_TEXT, 0, 0];
        cell_bytes.extend_from_slice(&(1u32 << cell::RICH_TEXT_KEY_BIT).to_le_bytes());
        assert!(parse_cell(&cell_bytes, 0).is_none());
        cell_bytes.extend_from_slice(&9u32.to_le_bytes());
        assert_eq!(parse_cell(&cell_bytes, 0).unwrap().rich_text_key, Some(9));
        assert!(parse_cell(&cell_bytes, usize::MAX).is_none());
    }

    /// A Pages 14.4 number cell: decimal 12345 x 10^-1, flags 0x3061 (decimal, cell style, text
    /// style, suggestion, number format) and format key 2.
    #[test]
    fn version_5_number_cells_read_value_and_format_key() {
        let mut bytes = vec![cell::LEGACY_VERSION, cell::TYPE_NUMBER, 0, 0, 0, 0, 0, 0];
        bytes.extend_from_slice(&0x3061u32.to_le_bytes());
        bytes.extend_from_slice(&bid(12345, -1, false));
        for key in [4u32, 3, 1, 2] {
            bytes.extend_from_slice(&key.to_le_bytes());
        }
        let cell = parse_cell(&bytes, 0).unwrap();
        assert_eq!(cell.format_key, Some(2));
        let CellScalar::Decimal(d) = cell.value else {
            panic!("decimal value")
        };
        assert_eq!((d.coefficient, d.exponent), (12345, -1));
    }

    #[test]
    fn version_5_cells_with_unknown_field_sizes_are_not_read() {
        let mut bytes = vec![cell::LEGACY_VERSION, cell::TYPE_STRING, 0, 0, 0, 0, 0, 0];
        bytes.extend_from_slice(&(1u32 << 2 | 1 << 3).to_le_bytes());
        bytes.extend_from_slice(&[0u8; 16]);
        assert!(parse_cell(&bytes, 0).is_none());
        let mut ok = vec![cell::LEGACY_VERSION, cell::TYPE_STRING, 0, 0, 0, 0, 0, 0];
        ok.extend_from_slice(&(1u32 << 3).to_le_bytes());
        ok.extend_from_slice(&7u32.to_le_bytes());
        assert_eq!(parse_cell(&ok, 0).unwrap().string_key, Some(7));
        ok.truncate(14);
        assert!(parse_cell(&ok, 0).is_none());
    }

    #[test]
    fn version_5_bool_and_checkbox_cells_read_double_value_and_control_format_key() {
        let mut bytes = vec![cell::LEGACY_VERSION, cell::TYPE_BOOL, 0, 0, 0, 0, 0, 0];
        let flags = (1u32 << cell::LEGACY_BIT_DOUBLE) | (1u32 << 5) | (1u32 << 17);
        bytes.extend_from_slice(&flags.to_le_bytes());
        bytes.extend_from_slice(&1.0f64.to_le_bytes());
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&9u32.to_le_bytes());
        let parsed = parse_cell(&bytes, 0).unwrap();
        assert_eq!(parsed.kind, cell::TYPE_BOOL);
        assert_eq!(parsed.value, CellScalar::Double(1.0));
        assert_eq!(parsed.format_key, Some(9));
    }

    #[test]
    fn current_version_duration_cells_read_double_value_and_format_key() {
        let mut bytes = vec![cell::CURRENT_VERSION, cell::TYPE_DURATION, 0, 0];
        let flags = (1u32 << cell::VALUE_BIT) | (1u32 << cell::FORMAT_KEY_BIT);
        bytes.extend_from_slice(&flags.to_le_bytes());
        bytes.extend_from_slice(&5400.0f64.to_le_bytes());
        bytes.extend_from_slice(&7u32.to_le_bytes());
        let parsed = parse_cell(&bytes, 0).unwrap();
        assert_eq!(parsed.kind, cell::TYPE_DURATION);
        assert_eq!(parsed.value, CellScalar::Double(5400.0));
        assert_eq!(parsed.format_key, Some(7));
    }
}
