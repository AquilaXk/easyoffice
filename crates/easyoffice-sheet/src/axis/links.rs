//! In-workbook hyperlink targets that follow a row or column edit and a sheet rename or deletion
//! (PD-218).
//!
//! A target `#Sheet2!B5` is a location in the formula syntax, so the scanners that move the
//! references of a cell's formula move it too (see [`super::rewrite_formula_refs`] and
//! [`super::rewrite_sheet_names`]); a location without a sheet points at the sheet that holds the
//! link. A location whose cells are all deleted, or whose sheet is deleted, becomes `#REF!`.
//!
//! 참조 구현: Univer `packages/sheets-hyper-link/src/controllers/ref-range.controller.ts`
//! (`_registerRange`: a link to a range moves, shrinks or turns to an error with the range through
//! `handleDefaultRangeChangeWithEffectRefCommandsSkipNoInterests`; Apache-2.0, Copyright 2023-present
//! DreamNum Co., Ltd., commit 5a06a7842c8e13fa0646d0733dc3662cf4b330b9). The span rule is the one of
//! `axis/mod.rs`; applying it to the text of a location is this repository's own.

use super::{rewrite_formula_refs, rewrite_sheet_names, AxisEdit, SheetChange};

/// The `(old, new)` pairs of the targets of links held by sheet `own` that `edit` of sheet `target` changes.
pub fn shift_link_targets<'a>(
    targets: impl IntoIterator<Item = &'a str>,
    own: &str,
    target: &str,
    edit: &AxisEdit,
) -> Vec<(String, String)> {
    follow(targets, |location| {
        rewrite_formula_refs(location, own, target, edit, false)
    })
}

/// The `(old, new)` pairs of the targets that `change` of a sheet changes.
pub fn follow_sheet_change_links<'a>(
    targets: impl IntoIterator<Item = &'a str>,
    change: SheetChange<'_>,
) -> Vec<(String, String)> {
    follow(targets, |location| rewrite_sheet_names(location, change))
}

fn follow<'a>(
    targets: impl IntoIterator<Item = &'a str>,
    rewrite: impl Fn(&str) -> Option<String>,
) -> Vec<(String, String)> {
    targets
        .into_iter()
        .filter_map(|target| {
            let location = target.strip_prefix('#')?;
            let next = rewrite(location)?;
            Some((target.to_string(), format!("#{next}")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::axis::{Axis, EditKind};

    fn edit(axis: Axis, kind: EditKind, at: u32, count: u32) -> AxisEdit {
        AxisEdit::new(axis, kind, at, count).unwrap()
    }

    fn moved(targets: &[&str], own: &str, target: &str, e: &AxisEdit) -> Vec<(String, String)> {
        shift_link_targets(targets.iter().copied(), own, target, e)
    }

    fn pair(a: &str, b: &str) -> (String, String) {
        (a.to_string(), b.to_string())
    }

    #[test]
    fn a_row_insert_moves_a_location_and_grows_a_range_it_straddles() {
        let insert = edit(Axis::Row, EditKind::Insert, 2, 3);
        assert_eq!(
            moved(
                &[
                    "#Sheet1!B5",
                    "#Sheet1!B1",
                    "#Sheet1!A1:A9",
                    "#Sheet2!B5",
                    "#Total"
                ],
                "Sheet1",
                "Sheet1",
                &insert
            ),
            vec![
                pair("#Sheet1!B5", "#Sheet1!B8"),
                pair("#Sheet1!A1:A9", "#Sheet1!A1:A12")
            ]
        );
    }

    #[test]
    fn a_location_without_a_sheet_belongs_to_the_sheet_that_holds_the_link() {
        let insert = edit(Axis::Col, EditKind::Insert, 0, 1);
        assert_eq!(
            moved(&["#B5"], "Sheet1", "Sheet1", &insert),
            vec![pair("#B5", "#C5")]
        );
        assert!(moved(&["#B5"], "Sheet2", "Sheet1", &insert).is_empty());
    }

    #[test]
    fn a_deleted_location_becomes_a_reference_error_and_a_range_shrinks() {
        let delete = edit(Axis::Row, EditKind::Delete, 4, 2);
        assert_eq!(
            moved(
                &["#Sheet1!B5", "#Sheet1!B5:B8", "#Sheet1!B9"],
                "Sheet1",
                "Sheet1",
                &delete
            ),
            vec![
                pair("#Sheet1!B5", "##REF!"),
                pair("#Sheet1!B5:B8", "#Sheet1!B5:B6"),
                pair("#Sheet1!B9", "#Sheet1!B7"),
            ]
        );
    }

    #[test]
    fn external_addresses_and_names_never_move() {
        let insert = edit(Axis::Row, EditKind::Insert, 0, 1);
        assert!(moved(
            &["https://a.b/Sheet1!B5", "mailto:me@a.b", "#Total"],
            "Sheet1",
            "Sheet1",
            &insert
        )
        .is_empty());
    }

    #[test]
    fn a_rename_and_a_deletion_of_a_sheet_follow() {
        let targets = [
            "#Old!B5",
            "#'Other sheet'!A1",
            "#B5",
            "#Total",
            "https://a.b/",
        ];
        let rename = follow_sheet_change_links(
            targets,
            SheetChange::Rename {
                from: "Old",
                to: "새 이름",
            },
        );
        assert_eq!(rename, vec![pair("#Old!B5", "#'새 이름'!B5")]);
        let gone = follow_sheet_change_links(targets, SheetChange::Delete { name: "Old" });
        assert_eq!(gone, vec![pair("#Old!B5", "##REF!")]);
    }
}
