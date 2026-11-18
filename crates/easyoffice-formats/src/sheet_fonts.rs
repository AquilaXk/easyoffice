//! Per-cell font family table of a sheet or workbook (PD-244).
//!
//! Like [`super::sheet_numfmt`], a [`super::CellStyle`] stays [`Copy`] by holding a 16-bit font id
//! (`0` = workbook default font, `1..=MAX_FONT_FACES` = 1-based entry in [`super::SheetStyling::fonts`]).
//! Equal family names (case-insensitively) share one entry, and untrusted input is bounded by
//! [`MAX_FONT_NAME_CHARS`] and [`MAX_FONT_FACES`].

use std::collections::HashMap;
use std::sync::Arc;

/// Maximum distinct non-default font family names one sheet or workbook table keeps.
pub const MAX_FONT_FACES: usize = 512;
/// Maximum character length of a font family name.
pub const MAX_FONT_NAME_CHARS: usize = 64;

/// Strips optional outer single/double quotes (as written by ODF `fo:font-family="'Malgun Gothic'"` or CSSOM)
/// and checks length, control-character, and CSS/markup delimiter bounds.
pub fn clean_font_name(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    let inner = trimmed
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .or_else(|| trimmed.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
        .map_or(trimmed, str::trim);
    if inner.is_empty() {
        return None;
    }
    let mut count = 0usize;
    for ch in inner.chars() {
        count += 1;
        if count > MAX_FONT_NAME_CHARS
            || ch.is_control()
            || matches!(ch, '"' | '\'' | '\\' | ';' | '{' | '}' | '<' | '>' | '`')
        {
            return None;
        }
    }
    Some(inner)
}

/// Resolves a 1-based `font` id against `fonts`; `0` (default font) and out-of-range ids return `None`.
#[inline]
pub fn resolve_font_name(font: u16, fonts: &[String]) -> Option<&str> {
    let idx = usize::from(font).checked_sub(1)?;
    fonts.get(idx).map(String::as_str)
}

/// Deduplicates non-default font family names in first-use order.
#[derive(Debug, Default)]
pub struct FontTable {
    names: Vec<String>,
    index: HashMap<String, u16>,
    rejected: u32,
}

impl FontTable {
    pub fn lookup(&self, raw: &str) -> Option<u16> {
        let clean = clean_font_name(raw)?;
        self.index.get(&clean.to_lowercase()).copied()
    }

    /// Returns the 1-based id of `raw`, adding it when new. Names breaking bounds or exceeding
    /// [`MAX_FONT_FACES`] increment `rejected` and fall back to `0` (default font).
    pub fn intern(&mut self, raw: &str) -> u16 {
        let Some(clean) = clean_font_name(raw) else {
            self.rejected = self.rejected.saturating_add(1);
            return 0;
        };
        let key = clean.to_lowercase();
        if let Some(&id) = self.index.get(&key) {
            return id;
        }
        if self.names.len() >= MAX_FONT_FACES {
            self.rejected = self.rejected.saturating_add(1);
            return 0;
        }
        let id = (self.names.len() + 1) as u16;
        self.names.push(clean.to_string());
        self.index.insert(key, id);
        id
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn rejected(&self) -> u32 {
        self.rejected
    }

    pub fn finish(self) -> (Arc<Vec<String>>, u32) {
        (Arc::new(self.names), self.rejected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_table_deduplicates_case_insensitively_and_rejects_hostile_names() {
        let mut table = FontTable::default();
        assert_eq!(table.intern("'Malgun Gothic'"), 1);
        assert_eq!(table.intern("malgun gothic"), 1);
        assert_eq!(table.intern("\"Noto Sans KR\""), 2);
        assert_eq!(table.intern(""), 0);
        assert_eq!(table.intern("Bad;Font{}"), 0);
        assert_eq!(table.intern("Bad\nFont"), 0);
        assert_eq!(table.intern(&"A".repeat(MAX_FONT_NAME_CHARS + 1)), 0);
        let (names, rejected) = table.finish();
        assert_eq!(names.as_slice(), ["Malgun Gothic", "Noto Sans KR"]);
        assert_eq!(rejected, 4);
    }
}
