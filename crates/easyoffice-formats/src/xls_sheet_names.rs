//! Sheet names a workbook file accepts (BIFF8 `.xls` and OOXML `.xlsx` share the rules).

/// Most UTF-16 units a sheet name may have.
pub const MAX_SHEET_NAME_UNITS: usize = 31;
const FORBIDDEN: [char; 7] = [':', '\\', '/', '?', '*', '[', ']'];

/// The longest prefix of `text` that fits `max_units` UTF-16 units without splitting a character.
fn truncate_utf16_units(text: &str, max_units: usize) -> String {
    let mut units = 0;
    text.chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= max_units
        })
        .collect()
}

/// Sheet name the workbook format reserves for its change history (compared ignoring case).
const RESERVED: &str = "history";

/// [`truncate_utf16_units`] without an apostrophe left at the cut: a sheet name may not end in one.
fn fit(text: &str, max_units: usize) -> String {
    truncate_utf16_units(text, max_units)
        .trim_end_matches('\'')
        .to_string()
}

/// At most 31 UTF-16 units, none of `: \ / ? * [ ]` (replaced by `_`), no leading or trailing
/// apostrophe, not empty, not `History`, unique ignoring case. A repeated name gets `_2`, `_3`,
/// ... within the 31 units. Names that already comply are returned unchanged, so a caller can
/// compare to report what was renamed.
pub(crate) fn sanitize_sheet_names<S: AsRef<str>>(names: &[S]) -> Vec<String> {
    let mut used = std::collections::HashSet::with_capacity(names.len() + 1);
    used.insert(RESERVED.to_string());
    names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let replaced: String = name
                .as_ref()
                .chars()
                .map(|c| if FORBIDDEN.contains(&c) { '_' } else { c })
                .collect();
            let trimmed = replaced.trim_matches('\'');
            let base = if trimmed.trim().is_empty() {
                format!("Sheet{}", i + 1)
            } else {
                trimmed.to_string()
            };
            let mut candidate = fit(&base, MAX_SHEET_NAME_UNITS);
            let mut n = 1u32;
            while !used.insert(candidate.to_lowercase()) {
                n += 1;
                let suffix = format!("_{n}");
                candidate = fit(&base, MAX_SHEET_NAME_UNITS - suffix.len()) + &suffix;
            }
            candidate
        })
        .collect()
}

/// Why a name cannot be a sheet's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetNameProblem {
    Empty,
    TooLong,
    /// One of `: \ / ? * [ ]` or a control character (an XML part cannot hold one).
    ForbiddenChar(char),
    /// Starts or ends with an apostrophe.
    EdgeApostrophe,
    /// The name the format keeps for its change history.
    Reserved,
}

/// The first rule `name` breaks; the same rules [`sanitize_sheet_names`] makes a name comply with.
/// Whether another sheet has the name is for the caller (it knows the other sheets).
pub fn sheet_name_problem(name: &str) -> Option<SheetNameProblem> {
    if name.trim().is_empty() {
        return Some(SheetNameProblem::Empty);
    }
    if name.encode_utf16().count() > MAX_SHEET_NAME_UNITS {
        return Some(SheetNameProblem::TooLong);
    }
    if let Some(c) = name
        .chars()
        .find(|c| FORBIDDEN.contains(c) || c.is_control())
    {
        return Some(SheetNameProblem::ForbiddenChar(c));
    }
    if name.starts_with('\'') || name.ends_with('\'') {
        return Some(SheetNameProblem::EdgeApostrophe);
    }
    name.eq_ignore_ascii_case(RESERVED)
        .then_some(SheetNameProblem::Reserved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_rule_names_its_problem() {
        let long = "가".repeat(32);
        let edge_units = "😀".repeat(16);
        let cases: &[(&str, Option<SheetNameProblem>)] = &[
            ("매출", None),
            ("Sheet 1", None),
            (
                "가나다라마바사아자차카타파하가나다라마바사아자차카타파하가나다",
                None,
            ),
            ("", Some(SheetNameProblem::Empty)),
            ("   ", Some(SheetNameProblem::Empty)),
            (long.as_str(), Some(SheetNameProblem::TooLong)),
            (edge_units.as_str(), Some(SheetNameProblem::TooLong)),
            ("a/b", Some(SheetNameProblem::ForbiddenChar('/'))),
            ("a\\b", Some(SheetNameProblem::ForbiddenChar('\\'))),
            ("a?", Some(SheetNameProblem::ForbiddenChar('?'))),
            ("*a", Some(SheetNameProblem::ForbiddenChar('*'))),
            ("[a]", Some(SheetNameProblem::ForbiddenChar('['))),
            ("a:b", Some(SheetNameProblem::ForbiddenChar(':'))),
            ("a\u{0}b", Some(SheetNameProblem::ForbiddenChar('\u{0}'))),
            ("a\tb", Some(SheetNameProblem::ForbiddenChar('\t'))),
            ("'a", Some(SheetNameProblem::EdgeApostrophe)),
            ("a'", Some(SheetNameProblem::EdgeApostrophe)),
            ("a'b", None),
            ("History", Some(SheetNameProblem::Reserved)),
            ("hIsToRy", Some(SheetNameProblem::Reserved)),
        ];
        for (name, want) in cases {
            assert_eq!(sheet_name_problem(name), *want, "{name:?}");
        }
        // 31 units exactly is fine, 32 is not (a surrogate pair counts two).
        assert_eq!(sheet_name_problem(&"a".repeat(31)), None);
        assert_eq!(
            sheet_name_problem(&format!("{}😀", "a".repeat(30))),
            Some(SheetNameProblem::TooLong)
        );
    }

    #[test]
    fn a_sanitized_name_always_passes_the_check() {
        let hostile = [
            "",
            "'",
            "a/b/c",
            "History",
            &"x".repeat(500),
            "😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀",
            "a'",
        ];
        for name in sanitize_sheet_names(&hostile) {
            assert_eq!(sheet_name_problem(&name), None, "{name:?}");
        }
    }
}
