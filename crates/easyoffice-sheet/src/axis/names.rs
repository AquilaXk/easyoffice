//! Defined names that follow a row or column edit and a sheet rename or deletion (PD-214).
//!
//! A name's definition is formula text, so the scanners that move the references of a cell's formula
//! move the ones of a definition too (see [`super::rewrite_formula_refs`] and
//! [`super::rewrite_sheet_names`]); a reference without a sheet points at the sheet the name is
//! scoped to, or at the first sheet for a workbook name. A reference whose cells are all deleted
//! becomes `#REF!`. The name of a deleted sheet's scope goes with the sheet; a renamed sheet's
//! scope takes the new name.

use super::{rewrite_formula_refs, rewrite_sheet_names, AxisEdit, SheetChange};
use easyoffice_formats::DefinedName;

/// `defs` with their references moved for `edit` of `target`, or `None` when nothing moves.
pub fn shift_names(
    defs: &[DefinedName],
    first_sheet: &str,
    target: &str,
    edit: &AxisEdit,
) -> Option<Vec<DefinedName>> {
    let mut changed = false;
    let moved = defs
        .iter()
        .map(|def| {
            let own = def.scope.as_deref().unwrap_or(first_sheet);
            match rewrite_formula_refs(&def.refers_to, own, target, edit, false) {
                Some(refers_to) => {
                    changed = true;
                    DefinedName {
                        refers_to,
                        ..def.clone()
                    }
                }
                None => def.clone(),
            }
        })
        .collect();
    changed.then_some(moved)
}

/// `defs` after `change` of a sheet, or `None` when nothing changes.
pub fn follow_sheet_change_names(
    defs: &[DefinedName],
    change: SheetChange<'_>,
) -> Option<Vec<DefinedName>> {
    let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b) || a.to_lowercase() == b.to_lowercase();
    let mut changed = false;
    let mut out = Vec::with_capacity(defs.len());
    for def in defs {
        let mut def = def.clone();
        match (change, def.scope.as_deref()) {
            (SheetChange::Delete { name }, Some(scope)) if same(scope, name) => {
                changed = true;
                continue;
            }
            (SheetChange::Rename { from, to }, Some(scope)) if same(scope, from) => {
                def.scope = Some(to.to_string());
                changed = true;
            }
            _ => {}
        }
        if let Some(refers_to) = rewrite_sheet_names(&def.refers_to, change) {
            def.refers_to = refers_to;
            changed = true;
        }
        out.push(def);
    }
    changed.then_some(out)
}
