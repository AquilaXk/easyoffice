//! Arrays and tables (PD-224): an array whose anchor is in a table, or whose area crosses one, is
//! `#SPILL!`; one that ends in a single cell is an ordinary formula there.
#![allow(clippy::unwrap_used)]

mod dynamic_support;
use dynamic_support::new_empty_model;
use easyoffice_formats::CellBox;
use easyoffice_sheet::{Axis, AxisBudget, AxisEdit, EditKind, WorkbookEngine, WorkbookEvalTarget};

fn boxes(list: &[&str]) -> Vec<CellBox> {
    list.iter().map(|r| CellBox::parse(r).unwrap()).collect()
}

fn texts(m: &dynamic_support::Model, cells: &[&str]) -> Vec<String> {
    cells.iter().map(|c| m._get_text(c)).collect()
}

#[test]
fn an_array_in_a_table_is_spill_and_one_beside_it_spills() {
    let mut m = new_empty_model();
    m.session()
        .set_sheet_tables("Sheet1", boxes(&["B2:C6"]))
        .unwrap();
    // The anchor is in the table; the second one is outside, in column E.
    m._set("B3", "=SEQUENCE(2)");
    m._set("E1", "=SEQUENCE(3)");
    // One cell: an ordinary formula, also in a table.
    m._set("C3", "=SUM(SEQUENCE(3))");
    m.evaluate();
    assert_eq!(
        texts(&m, &["B3", "B4", "E1", "E3", "C3"]),
        ["#SPILL!", "", "1", "3", "6"]
    );
    assert!(m.last.as_ref().unwrap().spills.iter().any(|s| s.blocked));
    // The table goes: the array spills.
    m.session().set_sheet_tables("Sheet1", Vec::new()).unwrap();
    m.evaluate();
    assert_eq!(texts(&m, &["B3", "B4"]), ["1", "2"]);
}

#[test]
fn an_area_that_runs_into_a_table_is_blocked_and_a_table_made_over_it_blocks_it() {
    let mut m = new_empty_model();
    m.session()
        .set_sheet_tables("Sheet1", boxes(&["A4:B6"]))
        .unwrap();
    m._set("A1", "=SEQUENCE(5)");
    m._set("D1", "=SEQUENCE(5)");
    m.evaluate();
    // A1:A5 reaches the table header at A4; D1:D5 is beside it.
    assert_eq!(
        texts(&m, &["A1", "A2", "D1", "D5"]),
        ["#SPILL!", "", "1", "5"]
    );
    // An area that already spilled is blocked by a table made over it.
    m.session()
        .set_sheet_tables("Sheet1", boxes(&["D3:E4"]))
        .unwrap();
    m.evaluate();
    assert_eq!(texts(&m, &["D1", "D3"]), ["#SPILL!", ""]);
    // A table of one cell blocks too.
    let mut one = new_empty_model();
    one.session()
        .set_sheet_tables("Sheet1", boxes(&["A2"]))
        .unwrap();
    one._set("A1", "=SEQUENCE(3)");
    one.evaluate();
    assert_eq!(one._get_text("A1"), "#SPILL!");
}

#[test]
fn tables_of_another_sheet_do_not_block_and_merges_and_tables_are_independent() {
    let mut m = new_empty_model();
    m.session()
        .set_sheet_tables("Sheet2", boxes(&["A1:B3"]))
        .unwrap();
    m.session()
        .set_sheet_merges("Sheet1", boxes(&["C1:C2"]))
        .unwrap();
    m._set("A1", "=SEQUENCE(3)");
    m.evaluate();
    assert_eq!(texts(&m, &["A1", "A3"]), ["1", "3"]);
    // Telling the session the merges again leaves the tables alone, and the other way round.
    m.session()
        .set_sheet_tables("Sheet1", boxes(&["A2:A3"]))
        .unwrap();
    m.session().set_sheet_merges("Sheet1", Vec::new()).unwrap();
    m.evaluate();
    assert_eq!(m._get_text("A1"), "#SPILL!");
}

#[test]
fn a_row_edit_moves_the_table_the_array_may_not_cross() {
    let mut m = new_empty_model();
    m.session()
        .set_sheet_tables("Sheet1", boxes(&["A6:B8"]))
        .unwrap();
    m._set("A1", "=SEQUENCE(4)");
    m.evaluate();
    assert_eq!(m._get_text("A4"), "4");
    let edit = AxisEdit::new(Axis::Row, EditKind::Delete, 3, 2).unwrap();
    m.session()
        .apply_axis_edit("Sheet1", edit, AxisBudget::default())
        .unwrap();
    assert_eq!(texts(&m, &["A1", "A2"]), ["#SPILL!", ""]);
    // A table of one cell survives the edit that moves it.
    let mut one = new_empty_model();
    one.session()
        .set_sheet_tables("Sheet1", boxes(&["A9"]))
        .unwrap();
    one._set("A1", "=SEQUENCE(3)");
    one.evaluate();
    let edit = AxisEdit::new(Axis::Row, EditKind::Delete, 2, 6).unwrap();
    one.session()
        .apply_axis_edit("Sheet1", edit, AxisBudget::default())
        .unwrap();
    assert_eq!(one._get_text("A1"), "#SPILL!");
}

#[test]
fn a_workbook_without_a_session_places_arrays_around_tables() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("S");
    for (cell, v) in [("A1", "=SEQUENCE(3)"), ("E1", "=SEQUENCE(2)")] {
        wb.set_cell("S", cell, v.to_string()).unwrap();
    }
    wb.sheet_mut("S")
        .unwrap()
        .set_spill_tables(boxes(&["E1:F4"]));
    let target = |cell| WorkbookEvalTarget {
        sheet: "S",
        expr: cell,
        anchor: None,
    };
    let batch = wb.evaluate_targets(&[target("A3"), target("E1")]);
    let shown: Vec<String> = batch.values.iter().map(|v| v.to_display_string()).collect();
    assert_eq!(shown, ["3", "#SPILL!"]);
}
