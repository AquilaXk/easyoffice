//! Shape-checked card, bank account, passport, driver's licence, and business
//! registration number matchers used by the PII scrubber.
//!
//! The rules mirror the JS engine (`ui/sheet-utils.js`); `fixtures/pii-scrub-vectors.json`
//! keeps both in agreement. Every matcher inspects a bounded window, so the
//! scrubber stays linear in the input length.

/// Hyphen-separated digit layouts of Korean bank account numbers.
const BANK_ACCOUNT_GROUPS: [&[usize]; 9] = [
    &[3, 2, 6],
    &[3, 3, 6],
    &[3, 6, 3],
    &[3, 6, 5],
    &[4, 3, 6],
    &[4, 2, 7],
    &[6, 2, 6],
    &[3, 4, 4, 2],
    &[3, 6, 2, 3],
];

const CARD_GROUPS: [usize; 4] = [4, 4, 4, 4];
const DRIVER_LICENSE_GROUPS: [usize; 4] = [2, 2, 6, 2];
const BUSINESS_REGISTRATION_GROUPS: [usize; 3] = [3, 2, 5];

fn is_hyphen(b: u8) -> bool {
    b == b'-'
}

fn is_card_separator(b: u8) -> bool {
    b == b'-' || b.is_ascii_whitespace()
}

fn all_digits(bytes: &[u8], from: usize, len: usize) -> bool {
    from + len <= bytes.len() && bytes[from..from + len].iter().all(u8::is_ascii_digit)
}

/// Matches `groups` of digits joined by single separators, followed by a non-digit.
fn match_digit_groups(
    bytes: &[u8],
    idx: usize,
    groups: &[usize],
    is_sep: fn(u8) -> bool,
) -> Option<usize> {
    let mut p = idx;
    for (i, &len) in groups.iter().enumerate() {
        if i > 0 {
            if p >= bytes.len() || !is_sep(bytes[p]) {
                return None;
            }
            p += 1;
        }
        if !all_digits(bytes, p, len) {
            return None;
        }
        p += len;
    }
    if p < bytes.len() && bytes[p].is_ascii_digit() {
        return None;
    }
    Some(p - idx)
}

/// Branchless Luhn doubled-digit lookup table (`(d * 2) - if d * 2 > 9 { 9 } else { 0 }`).
const LUHN_DOUBLE: [u8; 10] = [0, 2, 4, 6, 8, 1, 3, 5, 7, 9];

/// Bitmask for single-letter Korean passport type prefixes (`M`, `S`, `R`, `O`, `D`, `G`).
const PASSPORT_PREFIX_MASK: u32 = (1 << (b'M' - b'A'))
    | (1 << (b'S' - b'A'))
    | (1 << (b'R' - b'A'))
    | (1 << (b'O' - b'A'))
    | (1 << (b'D' - b'A'))
    | (1 << (b'G' - b'A'));

/// Bitmask of valid first digit-group lengths for structured identifiers (`2`, `3`, `4`, `6`).
const VALID_FIRST_GROUP_MASK: u32 = (1 << 2) | (1 << 3) | (1 << 4) | (1 << 6);

/// Luhn (mod 10) checksum over ASCII digits using branchless table lookup.
#[allow(clippy::manual_is_multiple_of)]
pub(crate) fn passes_luhn(digits: &[u8]) -> bool {
    let mut sum = 0u32;
    for (i, &d) in digits.iter().rev().enumerate() {
        let digit = (d - b'0') as usize;
        let v = if (i & 1) != 0 {
            u32::from(LUHN_DOUBLE[digit])
        } else {
            digit as u32
        };
        sum += v;
    }
    sum % 10 == 0
}

/// 13-19 contiguous digits with a card-network first digit (2-6 or 9) and a valid Luhn checksum.
fn match_contiguous_card(bytes: &[u8], idx: usize) -> Option<usize> {
    if !matches!(bytes[idx], b'2'..=b'6' | b'9') {
        return None;
    }
    let mut end = idx;
    while end < bytes.len() && end - idx < 20 && bytes[end].is_ascii_digit() {
        end += 1;
    }
    let len = end - idx;
    if !(13..=19).contains(&len) || (end < bytes.len() && bytes[end].is_ascii_digit()) {
        return None;
    }
    passes_luhn(&bytes[idx..end]).then_some(len)
}

/// `[MSRODG]` + 8 digits, `[MSRODG]` + 3 digits + letter + 4 digits, or 2 letters + 7 digits.
fn match_passport(bytes: &[u8], idx: usize) -> Option<usize> {
    if idx > 0 && bytes[idx - 1].is_ascii_alphanumeric() {
        return None;
    }
    if idx + 9 > bytes.len() {
        return None;
    }
    let first = bytes[idx];
    let is_type = first.is_ascii_uppercase() && ((PASSPORT_PREFIX_MASK >> (first - b'A')) & 1) != 0;
    let legacy = is_type && all_digits(bytes, idx + 1, 8);
    let new_format = is_type
        && all_digits(bytes, idx + 1, 3)
        && bytes[idx + 4].is_ascii_uppercase()
        && all_digits(bytes, idx + 5, 4);
    let two_letter = first.is_ascii_uppercase()
        && bytes[idx + 1].is_ascii_uppercase()
        && all_digits(bytes, idx + 2, 7);
    if !(legacy || new_format || two_letter) {
        return None;
    }
    if idx + 9 < bytes.len() && bytes[idx + 9].is_ascii_alphanumeric() {
        return None;
    }
    Some(9)
}

/// Returns the byte length of a card / account / passport / licence / business
/// registration number starting at `idx`, if any.
pub(crate) fn match_identifier_at(bytes: &[u8], idx: usize) -> Option<usize> {
    if bytes[idx].is_ascii_uppercase() {
        return match_passport(bytes, idx);
    }
    if !bytes[idx].is_ascii_digit() || (idx > 0 && bytes[idx - 1].is_ascii_digit()) {
        return None;
    }
    // Count the first contiguous digit group once (`<= 7` for grouped identifiers)
    let mut first_end = idx + 1;
    let limit = (idx + 7).min(bytes.len());
    while first_end < limit && bytes[first_end].is_ascii_digit() {
        first_end += 1;
    }
    let first_len = first_end - idx;
    if first_len <= 6
        && (first_end >= bytes.len() || !bytes[first_end].is_ascii_digit())
        && ((VALID_FIRST_GROUP_MASK >> first_len) & 1) != 0
    {
        let grouped = match first_len {
            2 => match_digit_groups(bytes, idx, &DRIVER_LICENSE_GROUPS, is_hyphen),
            3 => match_digit_groups(bytes, idx, &BUSINESS_REGISTRATION_GROUPS, is_hyphen).or_else(
                || {
                    BANK_ACCOUNT_GROUPS
                        .iter()
                        .filter(|g| g[0] == 3)
                        .find_map(|groups| match_digit_groups(bytes, idx, groups, is_hyphen))
                },
            ),
            4 => match_digit_groups(bytes, idx, &CARD_GROUPS, is_card_separator).or_else(|| {
                BANK_ACCOUNT_GROUPS
                    .iter()
                    .filter(|g| g[0] == 4)
                    .find_map(|groups| match_digit_groups(bytes, idx, groups, is_hyphen))
            }),
            6 => match_digit_groups(bytes, idx, &[6, 2, 6], is_hyphen),
            _ => None,
        };
        if grouped.is_some() {
            return grouped;
        }
    }
    match_contiguous_card(bytes, idx)
}

/// Masks every ASCII letter and digit while keeping separators, so the mask has
/// the same UTF-16 length as the matched ASCII identifier.
pub(crate) fn mask_identifier(matched: &str) -> String {
    let mut buf = matched.as_bytes().to_vec();
    for b in &mut buf {
        if b.is_ascii_alphanumeric() {
            *b = b'*';
        }
    }
    // Safety invariant: replacing ASCII alphanumeric bytes with ASCII '*' preserves valid UTF-8.
    String::from_utf8(buf).unwrap_or_else(|_| {
        matched
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { '*' } else { c })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matched(text: &str) -> Option<&str> {
        match_identifier_at(text.as_bytes(), 0).map(|len| &text[..len])
    }

    #[test]
    fn luhn_and_identifier_shapes() {
        assert!(passes_luhn(b"4111111111111111"));
        assert!(passes_luhn(b"378282246310005"));
        assert!(!passes_luhn(b"4111111111111112"));
        assert_eq!(matched("4111111111111111 "), Some("4111111111111111"));
        assert_eq!(matched("378282246310005"), Some("378282246310005"));
        assert_eq!(matched("4111111111111112"), None);
        assert_eq!(matched("1234567890123"), None);
        assert_eq!(matched("41111111111111111111"), None);
        assert_eq!(matched("3333-01-1234567"), Some("3333-01-1234567"));
        assert_eq!(matched("123-456789-01234"), Some("123-456789-01234"));
        assert_eq!(matched("123-456789-01-234"), Some("123-456789-01-234"));
        assert_eq!(matched("110-123-4567890"), None);
        assert_eq!(matched("2026-11-06"), None);
        assert_eq!(matched("S123A4567x"), None);
        assert_eq!(matched("AB1234567"), Some("AB1234567"));
        assert_eq!(matched("X12345678"), None);
        assert_eq!(match_identifier_at(b"9M12345678", 1), None);
        assert_eq!(match_identifier_at(b"9110-123-456789", 1), None);
        assert_eq!(mask_identifier("M123A4567"), "*********");
        assert_eq!(
            mask_identifier("1234 5678-9012 3456"),
            "**** ****-**** ****"
        );
    }

    #[test]
    fn scrub_pii_ipc_masks_issue_examples_and_reports_identifier_count() {
        let payload = "카드 1234-5678-9012-3456 계좌 110-123-456789 여권 M12345678 \
                       면허 11-12-345678-90 사업자 123-45-67890 연락처 010-1234-5678 일자 2026-11-06";
        let out = crate::EasyOfficeIpcRouter::dispatch("scrub_pii", payload).unwrap();
        assert!(out.contains("\"maskedIdentifierNumbers\":5"), "{out}");
        assert!(out.contains("\"maskedPhoneNumbers\":1"), "{out}");
        assert!(out.contains("\"totalMasked\":6"), "{out}");
        for leaked in ["9012-3456", "456789", "M12345678", "345678-90", "67890"] {
            assert!(!out.contains(leaked), "{leaked} leaked: {out}");
        }
        assert!(out.contains("2026-11-06"));
    }
}
