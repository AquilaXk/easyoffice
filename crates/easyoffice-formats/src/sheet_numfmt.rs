//! Cell number format codes of a sheet (PD-193).
//!
//! A cell carries a 16-bit format id, not a string: `0` is General, `1..164` is a built-in id of
//! ECMA-376 18.8.30, `164 + n` is entry `n` of the sheet's custom code table
//! ([`SheetStyling::num_fmts`](crate::SheetStyling)). Equal codes share one table entry, so a
//! sheet of 100 000 formatted cells holds a handful of strings. XLSX `numFmts`/`numFmtId` and ODS
//! `number:*-style` read and write through this table.
//!
//! Reference implementation: the built-in table follows ECMA-376 18.8.30, the same list the
//! reference format-code library carries (Apache-2.0, see `THIRD_PARTY_NOTICES.md`). Ids that
//! name a locale-dependent code (14, 27..36, 50..58) are read with the Korean default of the
//! editor and never written, so a file opened elsewhere keeps its meaning.
//!
//! Untrusted input is bounded: a code is at most [`MAX_NUM_FMT_CODE_BYTES`] bytes with at most
//! [`MAX_FORMAT_SECTIONS`] sections and no control characters, and a sheet holds at most
//! [`MAX_NUM_FMTS`] distinct custom codes. A code that breaks a bound reads as General and is
//! counted, so the open dialog can say how many were replaced.

use std::collections::HashMap;
use std::sync::Arc;

/// First id of the custom code table; lower ids are built-in.
pub const CUSTOM_BASE: u16 = 164;
/// Longest format code (the spreadsheet programs' own limit is 255 characters).
pub const MAX_NUM_FMT_CODE_BYTES: usize = 255;
/// Sections of one code (positive;negative;zero;text).
pub const MAX_FORMAT_SECTIONS: usize = 4;
/// Distinct custom codes of one sheet.
pub const MAX_NUM_FMTS: usize = 512;

/// `(id, code)` for the ids a reader accepts, sorted by id. `*` and `_x` follow the format-code
/// syntax of the sheet engine's `text_format`.
const BUILTIN: &[(u16, &str)] = &[
    (1, "0"),
    (2, "0.00"),
    (3, "#,##0"),
    (4, "#,##0.00"),
    (9, "0%"),
    (10, "0.00%"),
    (11, "0.00E+00"),
    (12, "# ?/?"),
    (13, "# ??/??"),
    (14, "yyyy-mm-dd"),
    (15, "d-mmm-yy"),
    (16, "d-mmm"),
    (17, "mmm-yy"),
    (18, "h:mm AM/PM"),
    (19, "h:mm:ss AM/PM"),
    (20, "h:mm"),
    (21, "h:mm:ss"),
    (22, "yyyy-mm-dd h:mm"),
    (37, "#,##0_);(#,##0)"),
    (38, "#,##0_);[Red](#,##0)"),
    (39, "#,##0.00_);(#,##0.00)"),
    (40, "#,##0.00_);[Red](#,##0.00)"),
    (41, "_(* #,##0_);_(* (#,##0);_(* \"-\"_);_(@_)"),
    (
        42,
        "_(\"$\"* #,##0_);_(\"$\"* (#,##0);_(\"$\"* \"-\"_);_(@_)",
    ),
    (43, "_(* #,##0.00_);_(* (#,##0.00);_(* \"-\"??_);_(@_)"),
    (
        44,
        "_(\"$\"* #,##0.00_);_(\"$\"* (#,##0.00);_(\"$\"* \"-\"??_);_(@_)",
    ),
    (45, "mm:ss"),
    (46, "[h]:mm:ss"),
    (47, "mmss.0"),
    (48, "##0.0E+0"),
    (49, "@"),
];

/// Ids whose code depends on the program's locale; they are read, never written.
const fn is_locale_dependent(id: u16) -> bool {
    id == 14 || (27 <= id && id <= 36) || (50 <= id && id <= 58)
}

/// Code of a built-in id; `None` for General (`0`), for an id the table does not define and for
/// custom ids.
pub fn builtin_code(id: u16) -> Option<&'static str> {
    BUILTIN
        .binary_search_by_key(&id, |&(key, _)| key)
        .ok()
        .map(|at| BUILTIN[at].1)
}

/// Whether `id` is `0` or a built-in id the table defines.
pub fn is_known_builtin(id: u16) -> bool {
    id == 0 || builtin_code(id).is_some()
}

/// Id of a locale-neutral built-in whose code is exactly `code`.
fn builtin_id_of(code: &str) -> Option<u16> {
    BUILTIN
        .iter()
        .find(|&&(id, text)| !is_locale_dependent(id) && text == code)
        .map(|&(id, _)| id)
}

/// Whether `code` is the General format.
pub fn is_general(code: &str) -> bool {
    code.trim().eq_ignore_ascii_case("general")
}

/// Whether a format code is within the bounds above (length, sections, control characters).
pub fn code_within_bounds(code: &str) -> bool {
    if code.len() > MAX_NUM_FMT_CODE_BYTES || code.chars().any(char::is_control) {
        return false;
    }
    let mut sections = 1usize;
    let (mut quoted, mut escaped) = (false, false);
    for ch in code.chars() {
        if escaped {
            escaped = false;
        } else if ch == '\\' || ch == '_' || ch == '*' {
            escaped = !quoted;
        } else if ch == '"' {
            quoted = !quoted;
        } else if ch == ';' && !quoted {
            sections += 1;
        }
    }
    sections <= MAX_FORMAT_SECTIONS
}

/// The code of a cell's format id: a built-in code or an entry of `custom`; `None` for General
/// and for an id that points nowhere.
pub fn resolve_code(num_fmt: u16, custom: &[String]) -> Option<&str> {
    if num_fmt >= CUSTOM_BASE {
        return custom
            .get(usize::from(num_fmt - CUSTOM_BASE))
            .map(String::as_str);
    }
    builtin_code(num_fmt)
}

/// Workbook-wide format ids of one sheet's cell format ids, resolved once per distinct id: a
/// writer that numbers the codes of a whole workbook asks for every styled cell, and a hash of the
/// code text per cell would be the cost of the format.
pub(crate) struct IdMemo(Vec<u16>);

const UNSET: u16 = u16::MAX;

impl IdMemo {
    pub(crate) const fn new() -> Self {
        Self(Vec::new())
    }

    /// The id for the cell id `num_fmt` of `styling`; `id_of` maps a code to its workbook-wide id
    /// and runs once per distinct cell id.
    pub(crate) fn get(
        &mut self,
        num_fmt: u16,
        styling: &crate::SheetStyling,
        id_of: impl FnOnce(&str) -> u16,
    ) -> u16 {
        if num_fmt == 0 {
            return 0;
        }
        let at = usize::from(num_fmt);
        if self.0.len() <= at {
            self.0.resize(at + 1, UNSET);
        }
        if self.0[at] == UNSET {
            self.0[at] = resolve_code(num_fmt, &styling.num_fmts).map_or(0, id_of);
        }
        self.0[at]
    }
}

/// Builds the custom code table of a sheet or workbook while ids are assigned.
#[derive(Debug, Default)]
pub struct NumFmtTable {
    codes: Vec<String>,
    index: HashMap<String, u16>,
    rejected: u32,
}

impl NumFmtTable {
    /// Id of `code` when it needs no new entry: `0` for General, a built-in id for an exact
    /// built-in code, or the id of an entry already in the table.
    pub fn lookup(&self, code: &str) -> Option<u16> {
        if is_general(code) {
            return Some(0);
        }
        builtin_id_of(code).or_else(|| self.index.get(code).copied())
    }

    /// Id of `code`, adding an entry when it is new. A code over a bound, or one past the table
    /// size, is counted and reads as General.
    pub fn intern(&mut self, code: &str) -> u16 {
        if let Some(id) = self.lookup(code) {
            return id;
        }
        if !code_within_bounds(code) || self.codes.len() >= MAX_NUM_FMTS {
            self.rejected += 1;
            return 0;
        }
        let id = CUSTOM_BASE + self.codes.len() as u16;
        self.codes.push(code.to_string());
        self.index.insert(code.to_string(), id);
        id
    }

    /// Counts one format the reader could not take (an undefined built-in id).
    pub fn reject(&mut self) {
        self.rejected += 1;
    }

    pub fn codes(&self) -> &[String] {
        &self.codes
    }

    pub fn rejected(&self) -> u32 {
        self.rejected
    }

    /// The finished table, shared by every sheet of a workbook, and the rejected count.
    pub fn finish(self) -> (Arc<Vec<String>>, u32) {
        (Arc::new(self.codes), self.rejected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_table_is_sorted_and_resolves() {
        let ids: Vec<u16> = BUILTIN.iter().map(|&(id, _)| id).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted, "binary search needs ascending ids");
        assert_eq!(builtin_code(2), Some("0.00"));
        assert_eq!(builtin_code(49), Some("@"));
        assert_eq!(builtin_code(5), None);
        assert_eq!(builtin_code(0), None);
        assert!(is_known_builtin(0) && is_known_builtin(14) && !is_known_builtin(27));
    }

    #[test]
    fn intern_shares_entries_and_maps_builtins() {
        let mut table = NumFmtTable::default();
        assert_eq!(table.intern("General"), 0);
        assert_eq!(table.intern("0.00"), 2);
        assert_eq!(table.intern("@"), 49);
        // A locale-dependent built-in code is written as a custom entry.
        assert_eq!(table.intern("yyyy-mm-dd"), CUSTOM_BASE);
        assert_eq!(table.intern("\"₩\"#,##0"), CUSTOM_BASE + 1);
        assert_eq!(table.intern("\"₩\"#,##0"), CUSTOM_BASE + 1);
        assert_eq!(table.codes().len(), 2);
        assert_eq!(table.rejected(), 0);
    }

    #[test]
    fn bounds_reject_long_sectioned_and_control_codes() {
        assert!(code_within_bounds("0.00;[Red]-0.00;0;@"));
        assert!(!code_within_bounds("0;0;0;0;0"));
        assert!(code_within_bounds("\"a;b;c;d;e\"0"));
        assert!(!code_within_bounds(&"0".repeat(MAX_NUM_FMT_CODE_BYTES + 1)));
        assert!(!code_within_bounds("0\u{0}0"));
        let mut table = NumFmtTable::default();
        assert_eq!(table.intern("0;0;0;0;0"), 0);
        assert_eq!(table.rejected(), 1);
    }

    #[test]
    fn the_table_is_bounded() {
        let mut table = NumFmtTable::default();
        for n in 0..MAX_NUM_FMTS + 10 {
            table.intern(&format!("0.0\"{n}\""));
        }
        assert_eq!(table.codes().len(), MAX_NUM_FMTS);
        assert_eq!(table.rejected(), 10);
    }

    #[test]
    fn resolve_reads_builtin_and_custom() {
        let custom = vec!["0.0\"x\"".to_string()];
        assert_eq!(resolve_code(2, &custom), Some("0.00"));
        assert_eq!(resolve_code(CUSTOM_BASE, &custom), Some("0.0\"x\""));
        assert_eq!(resolve_code(CUSTOM_BASE + 1, &custom), None);
        assert_eq!(resolve_code(0, &custom), None);
    }
}
