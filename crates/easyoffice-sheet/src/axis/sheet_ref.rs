//! Rewrites the sheet names in formula text when a sheet is renamed or deleted.
//!
//! 참조 구현: Univer `packages/sheets-formula/src/controllers/update-formula.controller.ts` lines
//! 575, 649-691 (a reference to a removed sheet becomes the `#REF!` error, a reference to a renamed
//! sheet is written again with the new name), Apache-2.0, Copyright 2023-present DreamNum Co.,
//! Ltd., commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9; and IronCalc `base/src/new_empty.rs` lines
//! 447-527 (`rename_sheet_by_index` reads every formula, renames the sheet in its references and
//! writes it back), Apache-2.0 or MIT, Copyright IronCalc authors, commit
//! 87b684bb04a87c63bdd6343c29036450b1cfd358.
//!
//! Modifications: the references are found by the scanner of [`super::rewrite_formula_refs`]
//! (the text is scanned, not rebuilt from a syntax tree, so the formula's other characters stay as
//! written). A reference without a sheet name points at the sheet that holds the formula and never
//! changes. A deleted sheet's whole reference (`Gone!A1:B2`) becomes one `#REF!`; a renamed
//! sheet's name is quoted only when the formula needs it.

use super::formula::scan_refs;

/// What happened to a sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetChange<'a> {
    Rename { from: &'a str, to: &'a str },
    Delete { name: &'a str },
}

/// Sheet names compare without case (ASCII and the common Unicode folds), as in every file format.
fn same_sheet(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b) || a.to_lowercase() == b.to_lowercase()
}

/// `name` as it is written in front of `!` in a formula.
pub fn quote_sheet_name(name: &str) -> String {
    if easyoffice_formats::sheet_name_needs_quotes(name) {
        format!("'{}'", name.replace('\'', "''"))
    } else {
        name.to_string()
    }
}

/// `text` with the references to the changed sheet rewritten, or `None` when nothing changes.
pub fn rewrite_sheet_names(text: &str, change: SheetChange<'_>) -> Option<String> {
    scan_refs(text, |site| {
        let sheet = site.sheet.as_deref()?;
        match change {
            SheetChange::Rename { from, to } if same_sheet(sheet, from) => {
                let mut prefix = quote_sheet_name(to);
                prefix.push('!');
                let mut out = prefix.clone();
                let Some(unit) = &site.unit else {
                    return Some(out);
                };
                match unit.repeated_prefix() {
                    Some(tail) => {
                        out.push_str(&text[site.unit_start()..tail.start]);
                        out.push_str(&prefix);
                        out.push_str(&text[tail.end..unit.end()]);
                    }
                    None => out.push_str(&text[site.unit_start()..unit.end()]),
                }
                Some(out)
            }
            SheetChange::Delete { name } if same_sheet(sheet, name) && site.unit.is_some() => {
                Some("#REF!".to_string())
            }
            _ => None,
        }
    })
}
