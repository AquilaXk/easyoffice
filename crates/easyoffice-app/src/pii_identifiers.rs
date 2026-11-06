//! Shape-checked card, bank account, passport, driver's licence, and business
//! registration number matchers used by the PII scrubber.
//!
//! The rules mirror the JS engine (`ui/sheet-utils.js`) and the Swift regex rules
//! in `DocumentSecurityAuditor.swift`; `fixtures/pii-scrub-vectors.json` keeps
//! all three in agreement. Every matcher inspects a bounded window, so the
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

/// Luhn (mod 10) checksum over ASCII digits.
pub(crate) fn passes_luhn(digits: &[u8]) -> bool {
    let mut sum = 0u32;
    for (i, &d) in digits.iter().rev().enumerate() {
        let mut v = u32::from(d - b'0');
        if i % 2 == 1 {
            v *= 2;
            if v > 9 {
                v -= 9;
            }
        }
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
    let is_type = matches!(bytes[idx], b'M' | b'S' | b'R' | b'O' | b'D' | b'G');
    let legacy = is_type && all_digits(bytes, idx + 1, 8);
    let new_format = is_type
        && all_digits(bytes, idx + 1, 3)
        && bytes[idx + 4].is_ascii_uppercase()
        && all_digits(bytes, idx + 5, 4);
    let two_letter = bytes[idx].is_ascii_uppercase()
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
    match_digit_groups(bytes, idx, &CARD_GROUPS, is_card_separator)
        .or_else(|| match_digit_groups(bytes, idx, &DRIVER_LICENSE_GROUPS, is_hyphen))
        .or_else(|| match_digit_groups(bytes, idx, &BUSINESS_REGISTRATION_GROUPS, is_hyphen))
        .or_else(|| {
            BANK_ACCOUNT_GROUPS
                .iter()
                .find_map(|groups| match_digit_groups(bytes, idx, groups, is_hyphen))
        })
        .or_else(|| match_contiguous_card(bytes, idx))
}

/// Masks every ASCII letter and digit while keeping separators, so the mask has
/// the same UTF-16 length as the matched ASCII identifier.
pub(crate) fn mask_identifier(matched: &str) -> String {
    matched
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { '*' } else { c })
        .collect()
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
