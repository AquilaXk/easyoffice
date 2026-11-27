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
//!
//! A 3D span (`Start:End!A1`) follows the rule of the reference spreadsheet app for a deleted
//! endpoint (its help page on 3D references across worksheets): the values of the deleted
//! sheet leave the calculation and the span keeps the sheets that are still between its ends. The
//! first sheet of the span is replaced by the next sheet in tab order and the last one by the
//! previous sheet; only a span with no sheet left becomes `#REF!`. The tab order comes with the
//! change; without the sheets in it the span cannot be followed and fails closed to `#REF!`.

use super::formula::scan_refs;

/// What happened to a sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetChange<'a> {
    Rename {
        from: &'a str,
        to: &'a str,
    },
    /// `tab_order` is every sheet name of the workbook in tab order, the deleted sheet included.
    Delete {
        name: &'a str,
        tab_order: &'a [&'a str],
    },
}

/// A 3D span after one of its sheets was deleted.
enum SpanAfterDelete {
    Unchanged,
    /// No sheet of the span is left.
    Gone,
    /// The sheets that remain; equal ends are one sheet.
    Span {
        start: String,
        end: String,
    },
}

fn span_after_delete(start: &str, end: &str, gone: &str, tab_order: &[&str]) -> SpanAfterDelete {
    let hit_start = same_sheet(start, gone);
    let hit_end = same_sheet(end, gone);
    if !hit_start && !hit_end {
        return SpanAfterDelete::Unchanged;
    }
    let position = |name: &str| tab_order.iter().position(|t| same_sheet(t, name));
    let (Some(from), Some(to)) = (position(start), position(end)) else {
        return SpanAfterDelete::Gone;
    };
    let (first, last) = (from.min(to), from.max(to));
    if first == last {
        return SpanAfterDelete::Gone;
    }
    let deleted = if hit_start { from } else { to };
    let heir = if deleted == first {
        first + 1
    } else {
        last - 1
    };
    let heir = tab_order[heir];
    let (start, end) = if hit_start {
        (heir, end)
    } else {
        (start, heir)
    };
    SpanAfterDelete::Span {
        start: start.to_string(),
        end: end.to_string(),
    }
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

/// Formats a 3D sheet span prefix body without `!`, quoting as `'Start:End'` when needed.
fn quote_sheet_span(start: &str, end: &str) -> String {
    if !easyoffice_formats::sheet_name_needs_quotes(start)
        && !easyoffice_formats::sheet_name_needs_quotes(end)
    {
        format!("{start}:{end}")
    } else {
        format!(
            "'{}:{}'",
            start.replace('\'', "''"),
            end.replace('\'', "''")
        )
    }
}

/// `text` with the references to the changed sheet rewritten, or `None` when nothing changes.
pub fn rewrite_sheet_names(text: &str, change: SheetChange<'_>) -> Option<String> {
    scan_refs(text, |site| {
        if let Some(whole) = site.quoted_whole.as_deref() {
            match change {
                SheetChange::Rename { from, to }
                    if from.contains(':') && same_sheet(whole, from) =>
                {
                    let mut out = quote_sheet_name(to);
                    out.push('!');
                    if let Some(unit) = &site.unit {
                        out.push_str(&text[site.unit_start()..unit.end()]);
                    }
                    return Some(out);
                }
                SheetChange::Delete { name, .. }
                    if name.contains(':') && same_sheet(whole, name) && site.unit.is_some() =>
                {
                    return Some("#REF!".to_string());
                }
                _ => {}
            }
        }
        let start = site.sheet.as_deref()?;
        if let Some(end) = site.end_sheet.as_deref() {
            return match change {
                SheetChange::Rename { from, to }
                    if same_sheet(start, from) || same_sheet(end, from) =>
                {
                    let new_start = if same_sheet(start, from) { to } else { start };
                    let new_end = if same_sheet(end, from) { to } else { end };
                    let mut out = quote_sheet_span(new_start, new_end);
                    out.push('!');
                    if let Some(unit) = &site.unit {
                        out.push_str(&text[site.unit_start()..unit.end()]);
                    }
                    Some(out)
                }
                SheetChange::Delete { name, tab_order } => {
                    let unit = site.unit.as_ref()?;
                    let mut out = match span_after_delete(start, end, name, tab_order) {
                        SpanAfterDelete::Unchanged => return None,
                        SpanAfterDelete::Gone => return Some("#REF!".to_string()),
                        SpanAfterDelete::Span { start, end } if same_sheet(&start, &end) => {
                            quote_sheet_name(&start)
                        }
                        SpanAfterDelete::Span { start, end } => quote_sheet_span(&start, &end),
                    };
                    out.push('!');
                    out.push_str(&text[site.unit_start()..unit.end()]);
                    Some(out)
                }
                _ => None,
            };
        }
        match change {
            SheetChange::Rename { from, to } if same_sheet(start, from) => {
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
            SheetChange::Delete { name, .. } if same_sheet(start, name) && site.unit.is_some() => {
                Some("#REF!".to_string())
            }
            _ => None,
        }
    })
}
