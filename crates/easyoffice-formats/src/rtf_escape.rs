//! RTF escape sequence decoding on UTF-16 units.
//!
//! `\uN` is followed by `\ucN` fallback units (RTF 1.9.1; one unless a `\ucN` says otherwise): a
//! plain unit, a `\'hh` byte, a control word or symbol, or a whole `{...}` group each count as one
//! and are dropped.

pub(crate) const RTF_BACKSLASH: u16 = 0x5C;
const RTF_OPEN_BRACE: u16 = 0x7B;
const RTF_CLOSE_BRACE: u16 = 0x7D;
/// UTF-16 units of the `line` control word.
const RTF_LINE_WORD: [u16; 4] = [b'l' as u16, b'i' as u16, b'n' as u16, b'e' as u16];
/// UTF-16 units of the `uc` control word.
const RTF_UC_WORD: [u16; 2] = [b'u' as u16, b'c' as u16];

/// `\uc1` is in force until a `\ucN` says otherwise.
pub(crate) const DEFAULT_UNICODE_SKIP: usize = 1;

/// Decodes the RTF escape starting at the backslash at `i` (`\\`, `\{`, `\}`, `\line`,
/// `\ucN`, `\uN` with its fallback). `unicode_skip` is the caller's `\ucN` state. Returns the
/// produced UTF-16 units (at most one) and the next index.
pub(crate) fn decode_rtf_escape(
    units: &[u16],
    i: usize,
    unicode_skip: &mut usize,
) -> Option<(Option<u16>, usize)> {
    let next = *units.get(i + 1)?;
    if matches!(next, RTF_BACKSLASH | RTF_OPEN_BRACE | RTF_CLOSE_BRACE) {
        return Some((Some(next), i + 2));
    }
    if next == b'\'' as u16 {
        let hex_u8 = |u: u16| {
            u8::try_from(u)
                .ok()?
                .is_ascii_hexdigit()
                .then(|| char::from(u as u8).to_digit(16).unwrap_or(0) as u8)
        };
        if let (Some(h1), Some(l1)) = (
            units.get(i + 2).copied().and_then(hex_u8),
            units.get(i + 3).copied().and_then(hex_u8),
        ) {
            let b1 = (h1 << 4) | l1;
            if units.get(i + 4) == Some(&RTF_BACKSLASH) && units.get(i + 5) == Some(&(b'\'' as u16))
            {
                if let (Some(h2), Some(l2)) = (
                    units.get(i + 6).copied().and_then(hex_u8),
                    units.get(i + 7).copied().and_then(hex_u8),
                ) {
                    let b2 = (h2 << 4) | l2;
                    if let Some(ch) = crate::cp949::decode_cp949_pair(b1, b2) {
                        return Some((Some(ch as u16), i + 8));
                    }
                }
            }
            return Some((Some(u16::from(b1)), i + 4));
        }
    }
    let rest = &units[i + 1..];
    if rest.starts_with(&RTF_LINE_WORD) {
        let mut end = i + 1 + RTF_LINE_WORD.len();
        if units.get(end) == Some(&(b' ' as u16)) {
            end += 1;
        }
        return Some((Some(b'\n' as u16), end));
    }
    if rest.starts_with(&RTF_UC_WORD) {
        let (count, end) = read_param(units, i + 1 + RTF_UC_WORD.len())?;
        *unicode_skip = usize::try_from(count).unwrap_or(0);
        return Some((None, end));
    }
    if next == b'u' as u16
        && units
            .get(i + 2)
            .is_some_and(|&u| u == b'-' as u16 || (b'0' as u16..=b'9' as u16).contains(&u))
    {
        return decode_rtf_unicode(units, i + 2, *unicode_skip);
    }
    None
}

/// Reads the signed decimal at `start` and the single space that ends the control word.
fn read_param(units: &[u16], start: usize) -> Option<(i32, usize)> {
    let neg = units.get(start) == Some(&(b'-' as u16));
    let digits = start + usize::from(neg);
    let mut end = digits;
    let mut acc: i32 = 0;
    while let Some(&digit) = units
        .get(end)
        .filter(|d| (b'0' as u16..=b'9' as u16).contains(d))
    {
        acc = acc
            .wrapping_mul(10)
            .wrapping_add((digit - b'0' as u16) as i32);
        end += 1;
    }
    if end == digits {
        return None;
    }
    if units.get(end) == Some(&(b' ' as u16)) {
        end += 1;
    }
    Some((if neg { acc.wrapping_neg() } else { acc }, end))
}

/// Decodes the signed decimal of `\uN` starting at `num_start` and drops its `unicode_skip` fallback units.
fn decode_rtf_unicode(
    units: &[u16],
    num_start: usize,
    unicode_skip: usize,
) -> Option<(Option<u16>, usize)> {
    let (value, after_number) = read_param(units, num_start)?;
    Some((
        Some((value as i16) as u16),
        skip_fallback(units, after_number, unicode_skip),
    ))
}

/// Index after `count` fallback units starting at `from`, never past the end of the text or of the
/// group that holds them.
fn skip_fallback(units: &[u16], mut from: usize, count: usize) -> usize {
    for _ in 0..count {
        from = match units.get(from) {
            None => break,
            Some(&RTF_CLOSE_BRACE) => break,
            Some(&RTF_OPEN_BRACE) => group_end(units, from),
            Some(&RTF_BACKSLASH) => escape_end(units, from),
            Some(_) => from + 1,
        };
    }
    from
}

/// Index after the control word or symbol that starts with the backslash at `at`.
fn escape_end(units: &[u16], at: usize) -> usize {
    let is_letter = |u: &u16| u8::try_from(*u).is_ok_and(|b| b.is_ascii_alphabetic());
    let is_hex = |u: &u16| u8::try_from(*u).is_ok_and(|b| b.is_ascii_hexdigit());
    match units.get(at + 1) {
        None => at + 1,
        Some(&quote) if quote == b'\'' as u16 => {
            if units.get(at + 2).is_some_and(is_hex) && units.get(at + 3).is_some_and(is_hex) {
                at + 4
            } else {
                at + 2
            }
        }
        Some(letter) if is_letter(letter) => {
            let word_end = at + 1 + units[at + 1..].iter().take_while(|u| is_letter(u)).count();
            read_param(units, word_end).map_or_else(
                || word_end + usize::from(units.get(word_end) == Some(&(b' ' as u16))),
                |(_, end)| end,
            )
        }
        Some(_) => at + 2,
    }
}

/// Index after the `{...}` group that opens at `at` (the end of the text when it is never closed).
fn group_end(units: &[u16], at: usize) -> usize {
    let mut depth = 0usize;
    let mut i = at;
    while i < units.len() {
        match units[i] {
            RTF_BACKSLASH => i += 1,
            RTF_OPEN_BRACE => depth += 1,
            RTF_CLOSE_BRACE => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    units.len()
}
