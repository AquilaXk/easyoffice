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
    let mut rich_text_key = None;
    if kind == cell::TYPE_RICH_TEXT && flags & cell::LEGACY_FLAG_RICH_TEXT_KEY != 0 {
        let below = (flags & (cell::LEGACY_FLAG_RICH_TEXT_KEY - 1)).count_ones() as usize;
        let at = cell::LEGACY_FIELDS_OFFSET.checked_add(below.checked_mul(4)?)?;
        rich_text_key = Some(read_u32(head, at)?);
    }
    Some(PackedCell {
        kind,
        rich_text_key,
        format_key: None,
        value: CellScalar::None,
    })
}

/// Byte size of the field selected by flag `bit` for a cell of type `kind`.
fn field_size(bit: u32, kind: u8) -> Option<usize> {
    if bit != cell::VALUE_BIT {
        return Some(cell::ASSUMED_FIELD_BYTES);
    }
    match kind {
        cell::TYPE_NUMBER => Some(cell::DECIMAL_BYTES),
        cell::TYPE_DATE | cell::TYPE_BOOL => Some(cell::DOUBLE_BYTES),
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
        format_key: None,
        value: CellScalar::None,
    };
    let mut cursor = cell::FIELDS_OFFSET;
    for bit in 0..cell::KNOWN_FLAG_BITS {
        if flags & (1 << bit) == 0 {
            continue;
        }
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
}
