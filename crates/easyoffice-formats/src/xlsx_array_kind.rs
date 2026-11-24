//! Classifies `<f t="array" ref="...">` formulas as spilling dynamic/matrix arrays vs. legacy
//! scalar-broadcast CSE formulas (PD-330).
//!
//! Legacy spreadsheet files may stamp `<f t="array" ref="A1:A5">1+2</f>` without a dynamic-array
//! cell metadata attribute (`cm`). Only formulas backed by `cm` metadata, `_xlfn.`/`_xlws.`
//! prefixes, or an explicit dynamic/matrix array function call spill and own their subordinate cells;
//! plain scalar CSE formulas preserve their cached subordinate `<v>` values.

use std::cmp::Ordering;

/// Sorted uppercase function names that produce multi-cell dynamic or legacy CSE array results.
const DYNAMIC_ARRAY_FUNCS: [&str; 36] = [
    "BYCOL",
    "BYROW",
    "CHOOSECOLS",
    "CHOOSEROWS",
    "DROP",
    "EXPAND",
    "FILTER",
    "FREQUENCY",
    "GROUPBY",
    "GROWTH",
    "HSTACK",
    "LINEST",
    "LOGEST",
    "MAKEARRAY",
    "MAP",
    "MINVERSE",
    "MMULT",
    "MUNIT",
    "PIVOTBY",
    "RANDARRAY",
    "SCAN",
    "SEQUENCE",
    "SORT",
    "SORTBY",
    "TAKE",
    "TEXTSPLIT",
    "TOCOL",
    "TOROW",
    "TRANSPOSE",
    "TREND",
    "UNIQUE",
    "VSTACK",
    "WRAPCOLS",
    "WRAPROWS",
    "XLOOKUP",
    "XMATCH",
];

#[inline]
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.')
}

#[inline]
fn cmp_ascii_upper(a: &[u8], b: &[u8]) -> Ordering {
    for (&ca, &cb) in a.iter().zip(b) {
        match ca.to_ascii_uppercase().cmp(&cb.to_ascii_uppercase()) {
            Ordering::Equal => {}
            ord => return ord,
        }
    }
    a.len().cmp(&b.len())
}

#[inline]
fn contains_xl_prefix_ci(ident: &[u8]) -> bool {
    const P1: &[u8] = b"_XLFN.";
    const P2: &[u8] = b"_XLWS.";
    ident
        .windows(P1.len())
        .any(|w| w.eq_ignore_ascii_case(P1) || w.eq_ignore_ascii_case(P2))
}

#[inline]
fn skip_quoted(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == quote {
            if bytes.get(i + 1) == Some(&quote) {
                i += 2;
                continue;
            }
            return i + 1;
        }
        i += 1;
    }
    bytes.len()
}

/// Returns `true` when `body` (raw `<f>` text) invokes a dynamic/matrix array function or uses an
/// `_xlfn.` / `_xlws.` future/worksheet-only function prefix outside quotes and structured refs.
pub(crate) fn is_dynamic_array_formula(body: &str) -> bool {
    let bytes = body.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if matches!(b, b'"' | b'\'') {
            i = skip_quoted(bytes, i);
            continue;
        }
        if b == b'[' {
            i = crate::group_end(body, i).unwrap_or(bytes.len());
            continue;
        }
        if is_ident_byte(b) {
            let start = i;
            i += 1;
            while i < bytes.len() && is_ident_byte(bytes[i]) {
                i += 1;
            }
            let ident = &bytes[start..i];
            if contains_xl_prefix_ci(ident) {
                return true;
            }
            let mut look = i;
            while look < bytes.len() && bytes[look].is_ascii_whitespace() {
                look += 1;
            }
            if bytes.get(look) == Some(&b'(')
                && DYNAMIC_ARRAY_FUNCS
                    .binary_search_by(|name| cmp_ascii_upper(name.as_bytes(), ident))
                    .is_ok()
            {
                return true;
            }
            continue;
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_array_list_is_sorted_and_complete() {
        for pair in DYNAMIC_ARRAY_FUNCS.windows(2) {
            assert!(pair[0] < pair[1], "unsorted: {} >= {}", pair[0], pair[1]);
        }
        for name in DYNAMIC_ARRAY_FUNCS {
            assert!(is_dynamic_array_formula(&format!("{name}(A1)")), "{name}");
            assert!(
                is_dynamic_array_formula(&format!("{}(A1)", name.to_ascii_lowercase())),
                "lowercase {name}"
            );
        }
    }

    #[test]
    fn distinguishes_dynamic_arrays_from_scalar_cse_and_literals() {
        assert!(is_dynamic_array_formula("_xlfn.SEQUENCE(10)"));
        assert!(is_dynamic_array_formula("_xlfn._xlws.SORT(A1:A5)"));
        assert!(is_dynamic_array_formula("1+TRANSPOSE(A1:C1)"));
        assert!(is_dynamic_array_formula("SEQUENCE (3, 2)"));

        assert!(!is_dynamic_array_formula("1+2"));
        assert!(!is_dynamic_array_formula("A1+B1"));
        assert!(!is_dynamic_array_formula("SUM(D1:D5)"));
        assert!(!is_dynamic_array_formula("IF(A1=\"SEQUENCE\",1,2)"));
        assert!(!is_dynamic_array_formula("'SORT'!A1+1"));
        assert!(!is_dynamic_array_formula("Table1[FILTER]+1"));
        assert!(!is_dynamic_array_formula("SORTED(A1:A5)"));
    }
}
