//! Sheet names a BIFF8 (`.xls`) workbook accepts.

const MAX_SHEET_NAME_UNITS: usize = 31;
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

/// At most 31 UTF-16 units, none of `: \ / ? * [ ]` (replaced by `_`), no leading or trailing
/// apostrophe, not empty, unique ignoring case. A repeated name gets `_2`, `_3`, ... within the 31
/// units. Names that already comply are returned unchanged, so a caller can compare to report
/// what was renamed.
pub(crate) fn sanitize_sheet_names<S: AsRef<str>>(names: &[S]) -> Vec<String> {
    let mut used = std::collections::HashSet::with_capacity(names.len());
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
            let mut candidate = truncate_utf16_units(&base, MAX_SHEET_NAME_UNITS);
            let mut n = 1u32;
            while !used.insert(candidate.to_lowercase()) {
                n += 1;
                let suffix = format!("_{n}");
                candidate =
                    truncate_utf16_units(&base, MAX_SHEET_NAME_UNITS - suffix.len()) + &suffix;
            }
            candidate
        })
        .collect()
}
