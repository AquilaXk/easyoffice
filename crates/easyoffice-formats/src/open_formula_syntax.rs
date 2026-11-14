//! Conversion from standard spreadsheet formulas to OpenFormula syntax (ODF 1.3 Part 4 OpenFormula).
//!
//! Linear single-pass token scanning: sheet prefixes and cell references are
//! converted to `[Sheet.A1]` or `[.A1]` without scanning ahead repeatedly.

pub(crate) fn to_open_formula_syntax_impl(formula_without_eq: &str) -> String {
    let bytes = formula_without_eq.as_bytes();
    let mut out = String::with_capacity(formula_without_eq.len() + 16);
    let mut i = 0usize;
    let mut in_quotes = false;

    while i < bytes.len() {
        if bytes[i] == b'"' {
            in_quotes = !in_quotes;
            out.push('"');
            i += 1;
            continue;
        }
        if in_quotes {
            let ch = formula_without_eq[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        if bytes[i] == b'[' {
            let mut j = i + 1;
            let mut in_sq = false;
            while j < bytes.len() {
                if bytes[j] == b'\'' {
                    in_sq = !in_sq;
                } else if bytes[j] == b']' && !in_sq {
                    j += 1;
                    break;
                }
                j += formula_without_eq[j..].chars().next().unwrap().len_utf8();
            }
            out.push_str(&formula_without_eq[i..j]);
            i = j;
            continue;
        }

        // Skip identifier interiors in O(1) before attempting reference conversion.
        let prev_is_ident = i > 0 && {
            let ch = formula_without_eq[..i].chars().next_back().unwrap();
            ch.is_alphanumeric() || ch == '_' || ch == '.'
        };
        if !prev_is_ident {
            if let Some((consumed, formatted)) =
                try_convert_a1_or_sheet_ref(&formula_without_eq[i..])
            {
                out.push_str(&formatted);
                i += consumed;
                continue;
            }
        }

        let ch = formula_without_eq[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Scans an optional sheet prefix: `'Sheet 1'!` or `Sheet1!`.
/// Uses a forward bounded token scan instead of `s.find('!')` to guarantee O(L) time.
fn scan_optional_sheet_prefix(s: &str) -> (Option<&str>, usize) {
    let bytes = s.as_bytes();
    if bytes.first() == Some(&b'\'') {
        let mut idx = 1usize;
        while idx < bytes.len() {
            if bytes[idx] == b'\'' {
                if idx + 1 < bytes.len() && bytes[idx + 1] == b'\'' {
                    idx += 2;
                    continue;
                }
                if idx + 1 < bytes.len() && bytes[idx + 1] == b'!' {
                    return (Some(&s[..=idx]), idx + 2);
                }
                break;
            }
            idx += s[idx..].chars().next().unwrap().len_utf8();
        }
    } else {
        // Forward token scan: only consume consecutive alphanumeric/underscore characters
        let mut idx = 0usize;
        for (byte_offset, ch) in s.char_indices() {
            if ch.is_alphanumeric() || ch == '_' {
                idx = byte_offset + ch.len_utf8();
            } else if ch == '!' && idx > 0 {
                return (Some(&s[..idx]), idx + 1);
            } else {
                break;
            }
        }
    }
    (None, 0)
}

fn try_convert_a1_or_sheet_ref(s: &str) -> Option<(usize, String)> {
    let bytes = s.as_bytes();
    let (sheet_prefix, pos) = scan_optional_sheet_prefix(s);
    let (c1_len, c1_str) = scan_single_a1_coord(&bytes[pos..])?;
    let mut end_pos = pos + c1_len;
    if end_pos < bytes.len() && bytes[end_pos] == b'(' {
        return None;
    }

    if end_pos < bytes.len() && bytes[end_pos] == b':' {
        let rhs_slice = &s[end_pos + 1..];
        let (rhs_sheet, rhs_pos) = scan_optional_sheet_prefix(rhs_slice);
        if let Some((c2_len, c2_str)) = scan_single_a1_coord(&rhs_slice.as_bytes()[rhs_pos..]) {
            let after_range = end_pos + 1 + rhs_pos + c2_len;
            if after_range >= bytes.len() || !bytes[after_range].is_ascii_alphanumeric() {
                end_pos = after_range;
                let left_part = match sheet_prefix {
                    Some(sh) => format!("{sh}.{c1_str}"),
                    None => format!(".{c1_str}"),
                };
                let right_part = match rhs_sheet {
                    Some(rsh) if Some(rsh) != sheet_prefix => format!("{rsh}.{c2_str}"),
                    _ => format!(".{c2_str}"),
                };
                return Some((end_pos, format!("[{left_part}:{right_part}]")));
            }
        }
    }

    let formatted = if let Some(sh) = sheet_prefix {
        format!("[{sh}.{c1_str}]")
    } else {
        format!("[.{c1_str}]")
    };
    Some((end_pos, formatted))
}

pub(crate) fn scan_single_a1_coord(bytes: &[u8]) -> Option<(usize, String)> {
    let mut idx = 0usize;
    let mut out = String::new();
    if idx < bytes.len() && bytes[idx] == b'$' {
        out.push('$');
        idx += 1;
    }
    let col_start = idx;
    while idx < bytes.len() && bytes[idx].is_ascii_alphabetic() {
        out.push(bytes[idx] as char);
        idx += 1;
    }
    let col_len = idx - col_start;
    if col_len == 0 || col_len > 3 {
        return None;
    }
    if idx < bytes.len() && bytes[idx] == b'$' {
        out.push('$');
        idx += 1;
    }
    let row_start = idx;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        out.push(bytes[idx] as char);
        idx += 1;
    }
    if idx == row_start || idx - row_start > 7 {
        return None;
    }
    if idx < bytes.len() && (bytes[idx].is_ascii_alphanumeric() || bytes[idx] == b'_') {
        return None;
    }
    Some((idx, out))
}
