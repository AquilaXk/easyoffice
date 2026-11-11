//! Moves the relative references of a formula, as when a rule written for the top-left cell of its
//! range is evaluated for another cell of the range.

/// Reference pieces of a word such as `$B12`: `(col_abs, letters, row_abs, digits)`.
fn split_reference(word: &str) -> Option<(bool, &str, bool, &str)> {
    let (col_abs, rest) = match word.strip_prefix('$') {
        Some(r) => (true, r),
        None => (false, word),
    };
    let letters_end = rest.find(|c: char| !c.is_ascii_alphabetic())?;
    let (letters, rest) = rest.split_at(letters_end);
    if letters.is_empty() || letters.len() > 3 {
        return None;
    }
    let (row_abs, digits) = match rest.strip_prefix('$') {
        Some(r) => (true, r),
        None => (false, rest),
    };
    if digits.is_empty() || digits.len() > 7 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((col_abs, letters, row_abs, digits))
}

fn column_number(letters: &str) -> i64 {
    letters.bytes().fold(0i64, |n, b| {
        n * 26 + i64::from(b.to_ascii_uppercase() - b'A' + 1)
    })
}

fn push_column(out: &mut String, mut col: i64) {
    let mut buf = Vec::new();
    while col > 0 {
        col -= 1;
        buf.push(b'A' + (col % 26) as u8);
        col /= 26;
    }
    out.extend(buf.iter().rev().map(|&b| b as char));
}

/// `formula` with every relative reference moved by `(dr, dc)` rows and columns; `None` when a
/// reference would leave the sheet.
pub(crate) fn shift_formula(formula: &str, dr: i64, dc: i64) -> Option<String> {
    if dr == 0 && dc == 0 {
        return Some(formula.to_string());
    }
    let mut out = String::with_capacity(formula.len() + 4);
    let mut rest = formula;
    while let Some(first) = rest.chars().next() {
        if first == '"' || first == '\'' {
            // Skip a quoted run; a doubled quote stays inside it.
            let bytes = rest.as_bytes();
            let mut i = 1;
            while i < bytes.len() {
                if bytes[i] == first as u8 {
                    if bytes.get(i + 1) == Some(&(first as u8)) {
                        i += 2;
                        continue;
                    }
                    break;
                }
                i += 1;
            }
            let end = (i + 1).min(rest.len());
            out.push_str(&rest[..end]);
            rest = &rest[end..];
        } else if first.is_ascii_alphanumeric() || first == '_' || first == '$' {
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))
                .unwrap_or(rest.len());
            let (word, tail) = rest.split_at(end);
            let is_call = tail.starts_with('(') || tail.starts_with('!');
            match split_reference(word).filter(|_| !is_call) {
                Some((col_abs, letters, row_abs, digits)) => {
                    let col = column_number(letters) + if col_abs { 0 } else { dc };
                    let row = digits.parse::<i64>().ok()? + if row_abs { 0 } else { dr };
                    if !(1..=16_384).contains(&col) || !(1..=1_048_576).contains(&row) {
                        return None;
                    }
                    if col_abs {
                        out.push('$');
                    }
                    push_column(&mut out, col);
                    if row_abs {
                        out.push('$');
                    }
                    out.push_str(&row.to_string());
                }
                None => out.push_str(word),
            }
            rest = tail;
        } else {
            out.push(first);
            rest = &rest[first.len_utf8()..];
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::shift_formula;

    #[test]
    fn relative_parts_move_and_absolute_parts_stay() {
        assert_eq!(shift_formula("A1>B$2", 3, 1).as_deref(), Some("B4>C$2"));
        assert_eq!(shift_formula("$A1+A$1", 2, 2).as_deref(), Some("$A3+C$1"));
        assert_eq!(
            shift_formula("SUM(A1:B2)", 1, 0).as_deref(),
            Some("SUM(A2:B3)")
        );
        assert_eq!(
            shift_formula("Sheet2!A1=\"A1\"", 1, 0).as_deref(),
            Some("Sheet2!A2=\"A1\"")
        );
        assert_eq!(shift_formula("'Q1'!A1", 1, 0).as_deref(), Some("'Q1'!A2"));
        assert_eq!(
            shift_formula("LOG10(A1)", 1, 0).as_deref(),
            Some("LOG10(A2)")
        );
        assert_eq!(shift_formula("AA9+Z1", 0, 1).as_deref(), Some("AB9+AA1"));
    }

    #[test]
    fn leaving_the_sheet_is_refused() {
        assert_eq!(shift_formula("A1", -1, 0), None);
        assert_eq!(shift_formula("A1", 0, -1), None);
        assert_eq!(shift_formula("XFD1", 0, 1), None);
        assert_eq!(shift_formula("A1", 0, 0).as_deref(), Some("A1"));
    }
}
