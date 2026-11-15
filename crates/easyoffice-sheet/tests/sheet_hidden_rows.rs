//! `SUBTOTAL` and the rows a sheet hides (PD-232): the function numbers 1 to 11 leave out the rows a filter hid, 101 to 111 leave out
//! every hidden row; a change of the hidden rows calculates only the formulas that use `SUBTOTAL`.
#![allow(clippy::unwrap_used)]

mod dynamic_support;
use dynamic_support::{new_empty_model, Model};
use easyoffice_sheet::{Axis, AxisBudget, AxisEdit, EditKind, WorkbookEngine, WorkbookEvalTarget};

/// Six regional totals in A1:A6 (1,625 together, 1,125 without the second and third) with every function on them.
fn book() -> Model {
    let mut m = new_empty_model();
    for (i, v) in [440, 285, 215, 310, 185, 190].iter().enumerate() {
        m._set(&format!("A{}", i + 1), &v.to_string());
    }
    m._set("B1", "=SUBTOTAL(109,A1:A6)");
    m._set("B2", "=SUBTOTAL(9,A1:A6)");
    m._set("B3", "=SUBTOTAL(103,A1:A6)");
    m._set("B4", "=SUBTOTAL(3,A1:A6)");
    m._set("B5", "=SUBTOTAL(101,A1:A6)");
    m._set("B6", "=SUBTOTAL(104,A1:A6)");
    m._set("C1", "=SUM(A1:A6)");
    m.evaluate();
    m
}

fn shown(m: &Model, cells: &[&str]) -> Vec<String> {
    cells.iter().map(|c| m._get_text(c)).collect()
}

fn hide(m: &mut Model, hidden: &[(u32, u32)], filtered: &[(u32, u32)]) {
    m.session()
        .set_sheet_hidden_rows("Sheet1", hidden.to_vec(), filtered.to_vec())
        .unwrap();
    m.evaluate();
}

#[test]
fn without_hidden_rows_every_function_counts_every_row() {
    let m = book();
    assert_eq!(
        shown(&m, &["B1", "B2", "B3", "B4", "B5", "C1"]),
        ["1625", "1625", "6", "6", "270.833333333333", "1625"]
    );
}

#[test]
fn rows_a_filter_hid_are_left_out_by_both_kinds_of_function_number() {
    let mut m = book();
    hide(&mut m, &[], &[(1, 2)]);
    // 440 + 310 + 185 + 190: the sum of the rows that are shown.
    assert_eq!(
        shown(&m, &["B1", "B2", "B3", "B4", "B5", "B6"]),
        ["1125", "1125", "4", "4", "281.25", "440"]
    );
    assert_eq!(
        m._get_text("C1"),
        "1625",
        "SUM does not know what is hidden"
    );
}

#[test]
fn rows_a_user_hid_are_left_out_by_the_100s_only() {
    let mut m = book();
    hide(&mut m, &[(1, 2)], &[]);
    assert_eq!(
        shown(&m, &["B1", "B2", "B3", "B4", "B5"]),
        ["1125", "1625", "4", "6", "281.25"]
    );
}

#[test]
fn a_row_hidden_both_ways_counts_as_filtered_and_the_variance_numbers_skip_hidden_rows() {
    let mut m = book();
    m._set("D1", "=SUBTOTAL(110,A1:A6)");
    m._set("D2", "=SUBTOTAL(10,A1:A6)");
    m._set("D3", "=SUBTOTAL(111,A1:A6)");
    m._set("D4", "=SUBTOTAL(11,A1:A6)");
    m.evaluate();
    let before = shown(&m, &["D1", "D2", "D3", "D4"]);
    assert_eq!(before[0], before[1]);
    hide(&mut m, &[(1, 2)], &[(2, 2)]);
    // Row 3 is both: filtered. Row 2 is only hidden: the 100s leave it out, the others do not.
    assert_eq!(m._get_text("B1"), "1125");
    assert_eq!(m._get_text("B2"), "1410");
    let after = shown(&m, &["D1", "D2", "D3", "D4"]);
    assert_ne!(after[0], after[1], "110 skips the hidden row, 10 does not");
    assert_ne!(after[2], after[3], "111 skips the hidden row, 11 does not");
}

#[test]
fn showing_the_rows_again_gives_the_values_back_and_only_the_subtotals_are_calculated() {
    let mut m = book();
    hide(&mut m, &[], &[(1, 2)]);
    let outcome = m.last.as_ref().unwrap();
    // The six SUBTOTAL cells and nothing else: the SUM beside them is not calculated again.
    assert_eq!(outcome.recomputed, 6);
    assert!(outcome.changed.iter().all(|c| c.coord.col() == 1));
    hide(&mut m, &[], &[]);
    assert_eq!(m._get_text("B1"), "1625");
    assert_eq!(m.last.as_ref().unwrap().recomputed, 6);
    // The same rows again change nothing.
    hide(&mut m, &[], &[]);
    assert_eq!(m.last.as_ref().unwrap().recomputed, 0);
}

#[test]
fn a_formula_around_a_subtotal_and_one_on_another_sheet_follow_the_hidden_rows() {
    let mut m = book();
    m._set("E1", "=ROUND(SUBTOTAL(109,A1:A6)/2,0)");
    m._set("E2", "=B1+1");
    m._set("Sheet2!A1", "=SUBTOTAL(109,Sheet1!A1:A6)");
    m.evaluate();
    assert_eq!(
        shown(&m, &["E1", "E2", "Sheet2!A1"]),
        ["813", "1626", "1625"]
    );
    hide(&mut m, &[], &[(1, 2)]);
    assert_eq!(
        shown(&m, &["E1", "E2", "Sheet2!A1"]),
        ["563", "1126", "1125"]
    );
}

#[test]
fn a_nested_subtotal_cell_and_a_row_outside_the_range_are_as_before() {
    let mut m = new_empty_model();
    for (i, v) in [1, 2, 3, 4].iter().enumerate() {
        m._set(&format!("A{}", i + 1), &v.to_string());
    }
    m._set("A5", "=SUBTOTAL(109,A1:A4)");
    m._set("B1", "=SUBTOTAL(109,A1:A5)");
    m.evaluate();
    assert_eq!(m._get_text("B1"), "10");
    hide(&mut m, &[(0, 0)], &[(8, 20)]);
    assert_eq!(m._get_text("B1"), "9");
}

#[test]
fn a_column_range_across_transposed_and_sparse_cells_skips_by_row() {
    let mut m = new_empty_model();
    m._set("A1", "1");
    m._set("A100", "2");
    m._set("A1000", "4");
    m._set("B1", "=SUBTOTAL(109,A1:A1000)");
    m._set("G1", "=SUBTOTAL(109,A1:E3)");
    m.evaluate();
    assert_eq!(shown(&m, &["B1", "G1"]), ["7", "1"]);
    hide(&mut m, &[(99, 99)], &[(0, 0)]);
    assert_eq!(shown(&m, &["B1", "G1"]), ["4", "0"]);
}

#[test]
fn an_inserted_or_deleted_row_moves_the_hidden_rows_with_the_cells() {
    let mut m = book();
    hide(&mut m, &[], &[(1, 2)]);
    let edit = AxisEdit::new(Axis::Row, EditKind::Insert, 0, 1).unwrap();
    m.session()
        .apply_axis_edit("Sheet1", edit, AxisBudget::default())
        .unwrap();
    // The values moved down a row, and so did the hidden rows 2 and 3: the sum is still of the shown rows.
    assert_eq!(m._get_text("A3"), "285");
    assert_eq!(m._get_text("B2"), "1125");
    let edit = AxisEdit::new(Axis::Row, EditKind::Delete, 2, 1).unwrap();
    m.session()
        .apply_axis_edit("Sheet1", edit, AxisBudget::default())
        .unwrap();
    assert_eq!(
        m._get_text("B2"),
        "1125",
        "the filtered 285 left with its row, 215 stays hidden"
    );
}

#[test]
fn a_workbook_without_a_session_leaves_the_hidden_rows_out_of_a_subtotal() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("S");
    for (i, v) in [440, 285, 215].iter().enumerate() {
        wb.set_cell("S", &format!("A{}", i + 1), v.to_string())
            .unwrap();
    }
    wb.set_cell("S", "B1", "=SUBTOTAL(109,A1:A3)".to_string())
        .unwrap();
    wb.set_cell("S", "B2", "=SUBTOTAL(9,A1:A3)".to_string())
        .unwrap();
    wb.sheet_mut("S")
        .unwrap()
        .set_hidden_rows(vec![(0, 0)], vec![(1, 1)]);
    let target = |cell| WorkbookEvalTarget {
        sheet: "S",
        expr: cell,
        anchor: None,
    };
    let batch = wb.evaluate_targets(&[target("B1"), target("B2")]);
    let shown: Vec<String> = batch.values.iter().map(|v| v.to_display_string()).collect();
    assert_eq!(shown, ["215", "655"]);
}

#[test]
fn aggregate_options_that_ignore_hidden_rows_leave_out_both_kinds() {
    let mut m = book();
    m._set("F1", "=AGGREGATE(9,5,A1:A6)");
    m._set("F2", "=AGGREGATE(9,4,A1:A6)");
    m._set("F3", "=AGGREGATE(4,1,A1:A6)");
    m.evaluate();
    assert_eq!(shown(&m, &["F1", "F2", "F3"]), ["1625", "1625", "440"]);
    hide(&mut m, &[(0, 0)], &[(3, 3)]);
    // Option 5 ignores the hidden row 1 and the filtered row 4; option 4 ignores nothing.
    assert_eq!(shown(&m, &["F1", "F2", "F3"]), ["875", "1625", "285"]);
}
