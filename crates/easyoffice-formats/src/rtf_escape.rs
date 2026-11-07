//! RTF escape sequence decoding on UTF-16 units.

pub(crate) const RTF_BACKSLASH: u16 = 0x5C;
const RTF_OPEN_BRACE: u16 = 0x7B;
const RTF_CLOSE_BRACE: u16 = 0x7D;
/// UTF-16 units of the `line` control word.
const RTF_LINE_WORD: [u16; 4] = [b'l' as u16, b'i' as u16, b'n' as u16, b'e' as u16];

/// Decodes the RTF escape starting at the backslash at `i` (`\\`, `\{`, `\}`, `\line`,
/// `\uN?`). Returns the produced UTF-16 units (at most one) and the next index.
pub(crate) fn decode_rtf_escape(units: &[u16], i: usize) -> Option<(Option<u16>, usize)> {
    let next = *units.get(i + 1)?;
    if matches!(next, RTF_BACKSLASH | RTF_OPEN_BRACE | RTF_CLOSE_BRACE) {
        return Some((Some(next), i + 2));
    }
    let rest = &units[i + 1..];
    if rest.starts_with(&RTF_LINE_WORD) {
        let mut end = i + 1 + RTF_LINE_WORD.len();
        if units.get(end) == Some(&(b' ' as u16)) {
            end += 1;
        }
        return Some((Some(b'\n' as u16), end));
    }
    if next == b'u' as u16 {
        return decode_rtf_unicode(units, i + 2);
    }
    None
}

/// Decodes the signed decimal of `\uN?` starting at `num_start`.
fn decode_rtf_unicode(units: &[u16], mut num_start: usize) -> Option<(Option<u16>, usize)> {
    let neg = units.get(num_start) == Some(&(b'-' as u16));
    if neg {
        num_start += 1;
    }
    let mut num_end = num_start;
    let mut acc: i32 = 0;
    while let Some(&digit) = units
        .get(num_end)
        .filter(|d| (b'0' as u16..=b'9' as u16).contains(d))
    {
        acc = acc
            .wrapping_mul(10)
            .wrapping_add((digit - b'0' as u16) as i32);
        num_end += 1;
    }
    if num_end == num_start {
        return None;
    }
    let signed_val = if neg { -acc } else { acc };
    let mut next_i = num_end;
    if units.get(next_i) == Some(&(b'?' as u16)) {
        next_i += 1;
    }
    Some((Some((signed_val as i16) as u16), next_i))
}
