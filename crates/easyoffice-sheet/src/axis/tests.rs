use super::*;
use easyoffice_formats::CellBox;
use std::collections::BTreeMap;

fn edit(axis: Axis, kind: EditKind, at1: u32, count: u32) -> AxisEdit {
    AxisEdit::new(axis, kind, at1 - 1, count).unwrap()
}

fn rw(formula: &str, own: &str, target: &str, e: AxisEdit) -> String {
    rewrite_formula_refs(formula, own, target, &e, false).unwrap_or_else(|| formula.to_string())
}

fn same(formula: &str, e: AxisEdit) -> String {
    rw(formula, "S", "S", e)
}

/// One row of the table: the formula, then its text after the edit.
type Case = (&'static str, &'static str);

fn check(e: AxisEdit, cases: &[Case]) {
    for (before, after) in cases {
        assert_eq!(same(before, e), *after, "{before} under {e:?}");
    }
}

#[test]
fn inserting_a_row_pushes_cells_and_widens_straddled_ranges() {
    check(
        edit(Axis::Row, EditKind::Insert, 3, 1),
        &[
            ("=A1", "=A1"),
            ("=A2", "=A2"),
            ("=A3", "=A4"),
            ("=A5", "=A6"),
            ("=$A$3", "=$A$4"),
            ("=A$3+$A3", "=A$4+$A4"),
            ("=SUM(A1:A2)", "=SUM(A1:A2)"),
            ("=SUM(A1:A5)", "=SUM(A1:A6)"),
            ("=SUM(A2:A3)", "=SUM(A2:A4)"),
            ("=SUM(A3:A5)", "=SUM(A4:A6)"),
            ("=SUM($A$1:$A$5)", "=SUM($A$1:$A$6)"),
            ("=SUM(A:A)", "=SUM(A:A)"),
            ("=SUM(A:C)", "=SUM(A:C)"),
            ("=SUM(1:5)", "=SUM(1:6)"),
            ("=SUM(3:3)", "=SUM(4:4)"),
            ("=SUM(1:2)", "=SUM(1:2)"),
            ("=A3&\"A3\"", "=A4&\"A3\""),
            ("=LOG10(A3)+ATAN2(A3,B3)", "=LOG10(A4)+ATAN2(A4,B4)"),
            ("=B2:A5", "=B2:A6"),
        ],
    );
}

#[test]
fn inserting_several_rows_moves_by_the_count() {
    check(
        edit(Axis::Row, EditKind::Insert, 2, 3),
        &[
            ("=A1+A2+A9", "=A1+A5+A12"),
            ("=SUM(A1:A9)", "=SUM(A1:A12)"),
            ("=SUM(A2:A4)", "=SUM(A5:A7)"),
        ],
    );
}

#[test]
fn inserting_a_column_works_the_same_way_on_columns() {
    check(
        edit(Axis::Col, EditKind::Insert, 3, 1),
        &[
            ("=B1", "=B1"),
            ("=C1", "=D1"),
            ("=$C$1+C$1+$C1", "=$D$1+D$1+$D1"),
            ("=SUM(A1:E1)", "=SUM(A1:F1)"),
            ("=SUM(A1:B1)", "=SUM(A1:B1)"),
            ("=SUM(C:C)", "=SUM(D:D)"),
            ("=SUM(A:E)", "=SUM(A:F)"),
            ("=SUM(1:1)", "=SUM(1:1)"),
            ("=SUM(A5:C9)", "=SUM(A5:D9)"),
            ("=Z1+AA1", "=AA1+AB1"),
        ],
    );
}

#[test]
fn deleting_a_row_removes_its_cells_and_shrinks_ranges() {
    check(
        edit(Axis::Row, EditKind::Delete, 3, 1),
        &[
            ("=A2", "=A2"),
            ("=A3", "=#REF!"),
            ("=A4", "=A3"),
            ("=$A$4", "=$A$3"),
            ("=SUM(A1:A5)", "=SUM(A1:A4)"),
            ("=SUM(A3:A5)", "=SUM(A3:A4)"),
            ("=SUM(A1:A3)", "=SUM(A1:A2)"),
            ("=SUM(A3:A3)", "=SUM(#REF!)"),
            ("=SUM(A4:A6)", "=SUM(A3:A5)"),
            ("=SUM(A:A)", "=SUM(A:A)"),
            ("=SUM(3:3)", "=SUM(#REF!)"),
            ("=SUM(3:5)", "=SUM(3:4)"),
            ("=SUM(1:9)", "=SUM(1:8)"),
            ("=A3+B1", "=#REF!+B1"),
        ],
    );
}

#[test]
fn deleting_several_rows_handles_every_overlap() {
    check(
        edit(Axis::Row, EditKind::Delete, 2, 3),
        &[
            ("=SUM(A1:A6)", "=SUM(A1:A3)"),
            ("=SUM(A1:A3)", "=SUM(A1:A1)"),
            ("=SUM(A3:A6)", "=SUM(A2:A3)"),
            ("=SUM(A2:A4)", "=SUM(#REF!)"),
            ("=SUM(A3:A4)", "=SUM(#REF!)"),
            ("=SUM(A4:A5)", "=SUM(A2:A2)"),
            ("=A5", "=A2"),
            ("=A6", "=A3"),
            ("=A1", "=A1"),
        ],
    );
}

#[test]
fn deleting_columns() {
    check(
        edit(Axis::Col, EditKind::Delete, 2, 1),
        &[
            ("=B1", "=#REF!"),
            ("=C1", "=B1"),
            ("=SUM(A:C)", "=SUM(A:B)"),
            ("=SUM(B:B)", "=SUM(#REF!)"),
            ("=SUM(A1:C3)", "=SUM(A1:B3)"),
            ("=SUM(2:2)", "=SUM(2:2)"),
        ],
    );
}

#[test]
fn whole_axis_and_open_ended_references_keep_their_ends() {
    let rows = AxisEdit::new(Axis::Row, EditKind::Insert, 0, 2).unwrap();
    assert_eq!(same("=SUM(A:A)", rows), "=SUM(A:A)");
    assert_eq!(same("=SUM(A1:A1048576)", rows), "=SUM(A1:A1048576)");
    assert_eq!(same("=SUM(A2:A1048576)", rows), "=SUM(A4:A1048576)");
    let del = AxisEdit::new(Axis::Row, EditKind::Delete, 0, 2).unwrap();
    assert_eq!(same("=SUM(A:A)", del), "=SUM(A:A)");
    assert_eq!(same("=SUM(A5:A1048576)", del), "=SUM(A3:A1048576)");
    // A cell pushed off the sheet is lost.
    assert_eq!(same("=A1048576", rows), "=#REF!");
    let cols = AxisEdit::new(Axis::Col, EditKind::Insert, 0, 1).unwrap();
    assert_eq!(same("=XFD1", cols), "=#REF!");
    assert_eq!(same("=SUM(1:1)", cols), "=SUM(1:1)");
}

#[test]
fn only_references_to_the_edited_sheet_move() {
    let e = edit(Axis::Row, EditKind::Insert, 2, 1);
    assert_eq!(rw("=A2+S!A2+T!A2", "S", "S", e), "=A3+S!A3+T!A2");
    assert_eq!(rw("=A2+S!A2+T!A2", "T", "S", e), "=A2+S!A3+T!A2");
    assert_eq!(rw("=SUM(S!A1:A9)", "T", "S", e), "=SUM(S!A1:A10)");
    assert_eq!(
        rw("='My ''Sheet'!A2+Q1!A2", "T", "My 'Sheet", e),
        "='My ''Sheet'!A3+Q1!A2"
    );
    assert_eq!(rw("=Q1!A2", "T", "q1", e), "=Q1!A3");
    assert_eq!(rw("=시트1!A2+A2", "T", "시트1", e), "=시트1!A3+A2");
    assert_eq!(rw("=S!A1:S!A9", "T", "S", e), "=S!A1:S!A10");
    let del = edit(Axis::Row, EditKind::Delete, 2, 1);
    assert_eq!(rw("=S!A2+B1", "T", "S", del), "=#REF!+B1");
    assert_eq!(
        rw("='My Sheet'!A2:A3", "T", "My Sheet", del),
        "='My Sheet'!A2:A2"
    );
}

#[test]
fn text_that_only_looks_like_a_reference_is_left_alone() {
    let e = edit(Axis::Row, EditKind::Insert, 1, 1);
    for text in [
        "=\"A1:B2\"&\"x\"\"A3\"",
        "=PI()*1.5E3",
        "=A1B+TRUE+FALSE",
        "=NAME1(1)",
        "=#REF!+#N/A",
    ] {
        assert_eq!(same(text, e), text);
    }
    assert_eq!(same("=ROW(A1)", e), "=ROW(A2)");
}

#[test]
fn a_rule_anchor_reference_can_follow_the_cell_that_replaces_it() {
    let del = edit(Axis::Row, EditKind::Delete, 2, 1);
    assert_eq!(rewrite_formula_refs("=$A2>5", "S", "S", &del, true), None);
    assert_eq!(
        rewrite_formula_refs("=$A2>5", "S", "S", &del, false).as_deref(),
        Some("=#REF!>5")
    );
    let ranged = rewrite_formula_refs("=COUNTIF($A$2:$A$9,$A2)>1", "S", "S", &del, true).unwrap();
    assert_eq!(ranged, "=COUNTIF($A$2:$A$8,$A2)>1");
}

#[test]
fn spans_move_like_the_cells_inside_them() {
    // Oracle: a span's new ends are the new positions of its first and last surviving cell
    // (and, for an insertion, of its first and last cell).
    for at in 0..9u32 {
        for count in 1..4u32 {
            for kind in [EditKind::Insert, EditKind::Delete] {
                let e = AxisEdit::new(Axis::Row, kind, at, count).unwrap();
                for s in 0..10u32 {
                    for t in s..10u32 {
                        let survivors: Vec<u32> = (s..=t).filter_map(|i| e.map_index(i)).collect();
                        let expected = match kind {
                            EditKind::Insert => {
                                Some((e.map_index(s).unwrap(), e.map_index(t).unwrap()))
                            }
                            EditKind::Delete => survivors
                                .first()
                                .zip(survivors.last())
                                .map(|(&a, &b)| (a, b)),
                        };
                        let got = e.shift_span(s, t);
                        assert_eq!(got, expected, "{kind:?} at {at} x{count}: {s}..={t}");
                    }
                }
            }
        }
    }
}

#[test]
fn unmap_inverts_map() {
    for kind in [EditKind::Insert, EditKind::Delete] {
        let e = AxisEdit::new(Axis::Col, kind, 5, 3).unwrap();
        for i in 0..40u32 {
            if let Some(n) = e.map_index(i) {
                assert_eq!(e.unmap_index(n), Some(i));
            }
        }
    }
}

#[test]
fn edits_are_bounded() {
    assert!(AxisEdit::new(Axis::Row, EditKind::Insert, 0, 0).is_err());
    assert!(AxisEdit::new(Axis::Row, EditKind::Insert, 0, MAX_ROWS_PER_EDIT + 1).is_err());
    assert!(AxisEdit::new(Axis::Col, EditKind::Delete, 0, MAX_COLS_PER_EDIT + 1).is_err());
    assert!(AxisEdit::new(Axis::Row, EditKind::Insert, 1_048_576, 1).is_err());
    assert!(AxisEdit::new(Axis::Row, EditKind::Delete, 1_048_575, 2).is_err());
    assert!(AxisEdit::new(Axis::Row, EditKind::Delete, 1_048_575, 1).is_ok());
}

#[test]
fn merges_follow_the_edit_and_vanish_when_empty() {
    let merge = CellBox {
        row0: 1,
        col0: 0,
        row1: 3,
        col1: 1,
    };
    let ins = |at, n| AxisEdit::new(Axis::Row, EditKind::Insert, at, n).unwrap();
    let del = |at, n| AxisEdit::new(Axis::Row, EditKind::Delete, at, n).unwrap();
    assert_eq!(shift_boxes(&[merge], &ins(0, 2), true)[0].row0, 3);
    let widened = shift_boxes(&[merge], &ins(2, 1), true);
    assert_eq!((widened[0].row0, widened[0].row1), (1, 4));
    assert_eq!(shift_boxes(&[merge], &ins(4, 1), true), vec![merge]);
    let narrowed = shift_boxes(&[merge], &del(2, 1), true);
    assert_eq!((narrowed[0].row0, narrowed[0].row1), (1, 2));
    assert!(shift_boxes(&[merge], &del(1, 3), true).is_empty());
    // Two rows deleted from a two-row merge leave nothing; one row leaves a single row.
    let pair = CellBox {
        row0: 1,
        col0: 0,
        row1: 2,
        col1: 0,
    };
    assert!(shift_boxes(&[pair], &del(1, 1), true).is_empty());
    assert_eq!(shift_boxes(&[pair], &del(1, 1), false).len(), 1);
    // The other axis is untouched.
    let cols = AxisEdit::new(Axis::Col, EditKind::Insert, 5, 1).unwrap();
    assert_eq!(shift_boxes(&[merge], &cols, true), vec![merge]);
}

#[test]
fn hidden_runs_split_around_inserted_rows_and_close_over_deleted_ones() {
    let ins = |at, n| AxisEdit::new(Axis::Row, EditKind::Insert, at, n).unwrap();
    let del = |at, n| AxisEdit::new(Axis::Row, EditKind::Delete, at, n).unwrap();
    assert_eq!(shift_runs(&[(2, 5)], &ins(0, 2)), vec![(4, 7)]);
    assert_eq!(shift_runs(&[(2, 5)], &ins(3, 2)), vec![(2, 2), (5, 7)]);
    assert_eq!(shift_runs(&[(2, 5)], &ins(2, 1)), vec![(3, 6)]);
    assert_eq!(shift_runs(&[(2, 5)], &ins(6, 1)), vec![(2, 5)]);
    assert_eq!(shift_runs(&[(2, 3), (6, 7)], &del(4, 2)), vec![(2, 5)]);
    assert_eq!(shift_runs(&[(2, 5)], &del(3, 2)), vec![(2, 3)]);
    assert!(shift_runs(&[(2, 3)], &del(1, 4)).is_empty());
}

#[test]
fn widths_follow_column_edits_only() {
    let widths: BTreeMap<u32, u32> = [(0, 120), (2, 80), (5, 200)].into();
    let ins = AxisEdit::new(Axis::Col, EditKind::Insert, 2, 2).unwrap();
    let moved = shift_widths(&widths, &ins);
    assert_eq!(
        moved.into_iter().collect::<Vec<_>>(),
        vec![(0, 120), (4, 80), (7, 200)]
    );
    let del = AxisEdit::new(Axis::Col, EditKind::Delete, 2, 1).unwrap();
    assert_eq!(
        shift_widths(&widths, &del).into_iter().collect::<Vec<_>>(),
        vec![(0, 120), (4, 200)]
    );
    let rows = AxisEdit::new(Axis::Row, EditKind::Insert, 0, 1).unwrap();
    assert_eq!(shift_widths(&widths, &rows), widths);
}

fn renamed(formula: &str, from: &str, to: &str) -> String {
    rewrite_sheet_names(formula, SheetChange::Rename { from, to })
        .unwrap_or_else(|| formula.to_string())
}

fn after_delete(formula: &str, name: &str) -> String {
    rewrite_sheet_names(formula, SheetChange::Delete { name })
        .unwrap_or_else(|| formula.to_string())
}

#[test]
fn renaming_a_sheet_rewrites_only_references_that_name_it() {
    let cases: &[(&str, &str)] = &[
        ("=Old!A1", "=New!A1"),
        ("=old!A1+Other!A1", "=New!A1+Other!A1"),
        ("=SUM(Old!A1:B2)", "=SUM(New!A1:B2)"),
        ("=SUM(Old!A1:Old!B2)", "=SUM(New!A1:New!B2)"),
        ("='Old'!$A$1", "=New!$A$1"),
        ("=A1+B2", "=A1+B2"),
        ("=\"Old!A1\"", "=\"Old!A1\""),
        ("=Older!A1", "=Older!A1"),
        ("=LOG10(Old!A1)", "=LOG10(New!A1)"),
        ("=Old!A:A", "=New!A:A"),
        ("=Old!1:3", "=New!1:3"),
    ];
    for (before, after) in cases {
        assert_eq!(renamed(before, "Old", "New"), *after, "{before}");
    }
}

#[test]
fn a_new_name_is_quoted_when_the_formula_needs_it() {
    let cases: &[(&str, &str)] = &[
        ("매출 합계", "='매출 합계'!A1"),
        ("It's", "='It''s'!A1"),
        ("2024", "='2024'!A1"),
        ("A1", "='A1'!A1"),
        ("TRUE", "='TRUE'!A1"),
        ("월별", "='월별'!A1"),
        ("Plain_1", "=Plain_1!A1"),
    ];
    for (to, after) in cases {
        assert_eq!(renamed("=Old!A1", "Old", to), *after, "{to}");
    }
    // A quoted old name with a space and a doubled quote is found.
    assert_eq!(renamed("='It''s a'!B2", "It's a", "X"), "=X!B2");
    assert_eq!(
        renamed("=SUM('2월'!A1:B3)", "2월", "봄"),
        "=SUM('봄'!A1:B3)"
    );
}

#[test]
fn deleting_a_sheet_turns_its_references_into_ref_errors() {
    let cases: &[(&str, &str)] = &[
        ("=Gone!A1", "=#REF!"),
        ("=Gone!A1+B2", "=#REF!+B2"),
        ("=SUM(Gone!A1:B2)", "=SUM(#REF!)"),
        ("=SUM(Gone!A1:Gone!B2)", "=SUM(#REF!)"),
        ("=SUM(Gone!A:A,Keep!A:A)", "=SUM(#REF!,Keep!A:A)"),
        ("='Gone'!$A$1*2", "=#REF!*2"),
        ("=Keep!A1", "=Keep!A1"),
        ("=A1", "=A1"),
        ("=\"Gone!A1\"", "=\"Gone!A1\""),
    ];
    for (before, after) in cases {
        assert_eq!(after_delete(before, "Gone"), *after, "{before}");
    }
    assert_eq!(after_delete("=SUM('두 번'!B1:B3)", "두 번"), "=SUM(#REF!)");
}

#[test]
fn a_rewrite_that_changes_nothing_answers_none() {
    let rename = SheetChange::Rename { from: "A", to: "B" };
    assert_eq!(rewrite_sheet_names("=C!A1", rename), None);
    assert_eq!(
        rewrite_sheet_names("=A1", SheetChange::Delete { name: "A" }),
        None
    );
}

#[test]
fn a_structured_reference_is_left_as_written_by_an_edit_and_a_rename() {
    // Its brackets name columns (`B3`, `A5` look like cells but are not), so nothing in them moves (PD-225).
    check(
        edit(Axis::Row, EditKind::Insert, 3, 2),
        &[
            ("=A5+Sales[A5]", "=A7+Sales[A5]"),
            (
                "=SUM(Sales[[#This Row],[B3]])+B3",
                "=SUM(Sales[[#This Row],[B3]])+B5",
            ),
            ("=[@A5]*A5", "=[@A5]*A7"),
            (
                "=Sales[[#Headers],[#Data],[A3]:[B9]]",
                "=Sales[[#Headers],[#Data],[A3]:[B9]]",
            ),
            ("=\"Sales[A5]\"&A5", "=\"Sales[A5]\"&A7"),
            ("=Sales[it'[3']]+A3", "=Sales[it'[3']]+A5"),
        ],
    );
    check(
        edit(Axis::Col, EditKind::Delete, 1, 1),
        &[("=Sales[A1]+A1+B1", "=Sales[A1]+#REF!+A1")],
    );
    let renamed = rewrite_sheet_names(
        "=Sales[Q1]+Old!A1+Sales[[#This Row],[Old]]",
        SheetChange::Rename {
            from: "Old",
            to: "New sheet",
        },
    );
    assert_eq!(
        renamed.as_deref(),
        Some("=Sales[Q1]+'New sheet'!A1+Sales[[#This Row],[Old]]")
    );
    assert_eq!(
        rewrite_sheet_names("=Sales[Old]", SheetChange::Delete { name: "Old" }),
        None
    );
}
