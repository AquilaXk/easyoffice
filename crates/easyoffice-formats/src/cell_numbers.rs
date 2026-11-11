//! Which cell texts the sheet writers store as numbers (PD-172).
//!
//! A sheet cell reaches the writers as text. A number type keeps only what an IEEE double holds,
//! so text that looks like a number but would change as one stays text: an ID with a leading zero
//! (`03045`), a phone number (`+8210...`), a code of more than 15 significant digits. The writers
//! of XLSX, ODS and XLS share this rule so one cell is the same type in every format.

/// Significant decimal digits an IEEE double keeps exactly (`f64::DIGITS`).
const MAX_SIGNIFICANT_DIGITS: usize = f64::DIGITS as usize;

/// The value of `text` when it is a number that survives being stored as a double: finite, no
/// leading `+`, no leading zero before another digit, at most 15 significant digits.
pub(crate) fn numeric_cell_value(text: &str) -> Option<f64> {
    let first = *text.as_bytes().first()?;
    if !(first.is_ascii_digit() || matches!(first, b'-' | b'.')) {
        return None;
    }
    let value = text.parse::<f64>().ok().filter(|n| n.is_finite())?;
    let unsigned = text.strip_prefix('-').unwrap_or(text);
    let bytes = unsigned.as_bytes();
    if bytes.first() == Some(&b'0') && bytes.get(1).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    let mantissa = unsigned.split(['e', 'E']).next().unwrap_or(unsigned);
    let significant = mantissa
        .bytes()
        .filter(u8::is_ascii_digit)
        .skip_while(|d| *d == b'0')
        .count();
    (significant <= MAX_SIGNIFICANT_DIGITS).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::numeric_cell_value;

    #[test]
    fn numbers_a_double_keeps_are_numeric() {
        for text in [
            "0",
            "42",
            "-0.5",
            ".5",
            "5.",
            "0.000123",
            "1.5e3",
            "123456789012345",
            "12.50",
        ] {
            assert!(numeric_cell_value(text).is_some(), "{text}");
        }
    }

    #[test]
    fn text_a_double_would_change_stays_text() {
        for text in [
            "",
            "03045",
            "-007",
            "00.5",
            "+5",
            "+821012345678",
            "1234567890123456",
            "12345678901234567890",
            "abc",
            "inf",
            "NaN",
            "1e999",
            " 5",
            "5 ",
        ] {
            assert!(numeric_cell_value(text).is_none(), "{text:?}");
        }
    }
}
