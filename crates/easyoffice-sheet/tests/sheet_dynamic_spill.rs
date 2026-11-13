//! Where arrays go (PD-217): areas, `#SPILL!`, `A1#`, edits inside an area, what a recalculation reports.
#![allow(clippy::unwrap_used)]

mod dynamic_support;
use dynamic_support::new_empty_model;

fn spilled(m: &dynamic_support::Model, cells: &[&str]) -> Vec<String> {
    cells.iter().map(|c| m._get_text(c)).collect()
}

#[test]
fn an_array_fills_the_cells_below_and_to_the_right() {
    let mut m = new_empty_model();
    m._set("B2", "=SEQUENCE(2,3)");
    m.evaluate();
    assert_eq!(
        spilled(&m, &["B2", "C2", "D2", "B3", "C3", "D3", "E2", "B4"]),
        ["1", "2", "3", "4", "5", "6", "", ""]
    );
    let outcome = m.last.as_ref().unwrap();
    assert_eq!(outcome.spills.len(), 1);
    assert_eq!((outcome.spills[0].rows, outcome.spills[0].cols), (2, 3));
    assert!(!outcome.spills[0].blocked);
    // the filled cells are reported with their values
    assert_eq!(outcome.changed.len(), 6);
}

#[test]
fn a_value_in_the_way_blocks_the_array_until_it_is_cleared() {
    let mut m = new_empty_model();
    m._set("A3", "x");
    m._set("A1", "=SEQUENCE(4)");
    m.evaluate();
    assert_eq!(m._get_text("A1"), "#SPILL!");
    assert_eq!(m._get_text("A2"), "");
    assert!(m.last.as_ref().unwrap().spills[0].blocked);
    m._set("A3", "");
    m.evaluate();
    assert_eq!(spilled(&m, &["A1", "A2", "A3", "A4"]), ["1", "2", "3", "4"]);
    // typing into the area blocks it again and gives the cells up
    m._set("A4", "typed");
    m.evaluate();
    assert_eq!(
        spilled(&m, &["A1", "A2", "A3", "A4"]),
        ["#SPILL!", "", "", "typed"]
    );
    m._set("A4", "");
    m.evaluate();
    assert_eq!(spilled(&m, &["A1", "A4"]), ["1", "4"]);
}

#[test]
fn an_array_past_the_sheet_edge_or_over_the_budget_is_spill() {
    let mut m = new_empty_model();
    m._set("D1048575", "=SEQUENCE(3)");
    m._set("C1", "=SEQUENCE(1,16384)");
    m._set("A1", "=SEQUENCE(1048576,2)");
    m._set("E5", "=SEQUENCE(1048576,2)");
    m.evaluate();
    assert_eq!(m._get_text("D1048575"), "#SPILL!");
    assert_eq!(m._get_text("C1"), "#SPILL!");
    // one anchor may take the budget (2,097,152 cells in all), a second one past it may not
    let one = m._get_text("A1");
    let other = m._get_text("E5");
    assert!(
        (one == "1" && other == "#SPILL!") || (one == "#SPILL!" && other == "1"),
        "{one} {other}"
    );
}

#[test]
fn the_first_array_keeps_its_cells_when_two_collide() {
    let mut m = new_empty_model();
    m._set("A1", "=SEQUENCE(3)");
    m.evaluate();
    m._set("A2", "");
    m._set("B1", "=SEQUENCE(1,3)");
    m._set("A3", "=SEQUENCE(1,3)");
    m.evaluate();
    // A3 sits in A1's area: it is a value there, so A1 is blocked by it
    assert_eq!(m._get_text("A1"), "#SPILL!");
    assert_eq!(m._get_text("A3"), "1");
    assert_eq!(m._get_text("B3"), "2");
}

#[test]
fn the_spill_range_operator_follows_the_area() {
    let mut m = new_empty_model();
    m._set("A1", "=SEQUENCE(3)");
    m._set("C1", "=SUM(A1#)");
    m._set("C2", "=ROWS(A1#)");
    m._set("D1", "=A1#*2");
    m._set("B5", "=A1#");
    m.evaluate();
    assert_eq!(spilled(&m, &["C1", "C2"]), ["6", "3"]);
    assert_eq!(spilled(&m, &["D1", "D2", "D3"]), ["2", "4", "6"]);
    assert_eq!(spilled(&m, &["B5", "B6", "B7"]), ["1", "2", "3"]);
    m._set("A1", "=SEQUENCE(5)");
    m.evaluate();
    assert_eq!(spilled(&m, &["C1", "C2"]), ["15", "5"]);
    assert_eq!(spilled(&m, &["D4", "D5"]), ["8", "10"]);
    // a cell that holds no formula has no spill range
    m._set("F1", "7");
    m._set("F2", "=F1#");
    m.evaluate();
    assert_eq!(m._get_text("F2"), "#REF!");
}

#[test]
fn readers_of_an_area_follow_when_it_changes_or_goes() {
    let mut m = new_empty_model();
    m._set("A1", "3");
    m._set("B1", "=SEQUENCE(A1)");
    m._set("D1", "=SUM(B1:B10)");
    m._set("D2", "=COUNTA(B:B)");
    m._set("D3", "=B3+100");
    m.evaluate();
    assert_eq!(spilled(&m, &["D1", "D2", "D3"]), ["6", "3", "103"]);
    m._set("A1", "5");
    m.evaluate();
    assert_eq!(spilled(&m, &["D1", "D2", "D3"]), ["15", "5", "103"]);
    m._set("A1", "2");
    m.evaluate();
    assert_eq!(
        spilled(&m, &["D1", "D2", "D3", "B3"]),
        ["3", "2", "100", ""]
    );
    let outcome = m.last.as_ref().unwrap();
    assert!(outcome
        .changed
        .iter()
        .any(|c| c.coord.to_a1() == "B3" && c.value == EvaluatedCellValue::Empty));
    m._set("B1", "gone");
    m.evaluate();
    assert_eq!(spilled(&m, &["D1", "D2", "B2"]), ["0", "1", ""]);
    assert_eq!(m.last.as_ref().unwrap().spills[0].rows, 0);
}

use easyoffice_sheet::EvaluatedCellValue;

#[test]
fn filter_sort_unique_follow_their_inputs() {
    let mut m = new_empty_model();
    for (i, v) in ["c", "a", "b", "a", "c"].iter().enumerate() {
        m._set(&format!("A{}", i + 1), v);
        m._set(&format!("B{}", i + 1), &format!("{}", (i + 1) * 10));
    }
    m._set("D1", "=SORT(UNIQUE(A1:A5))");
    m._set("F1", "=FILTER(A1:B5, B1:B5>=30)");
    m.evaluate();
    assert_eq!(spilled(&m, &["D1", "D2", "D3", "D4"]), ["a", "b", "c", ""]);
    assert_eq!(
        spilled(&m, &["F1", "G1", "F3", "G3"]),
        ["b", "30", "c", "50"]
    );
    m._set("A2", "z");
    m._set("B2", "99");
    m.evaluate();
    assert_eq!(spilled(&m, &["D1", "D2", "D3", "D4"]), ["a", "b", "c", "z"]);
    assert_eq!(spilled(&m, &["F1", "F2", "F3", "F4"]), ["z", "b", "a", "c"]);
}

#[test]
fn a_lookup_returns_a_whole_row() {
    let mut m = new_empty_model();
    for (r, (k, a, b)) in [("x", "1", "2"), ("y", "3", "4")].iter().enumerate() {
        m._set(&format!("A{}", r + 1), k);
        m._set(&format!("B{}", r + 1), a);
        m._set(&format!("C{}", r + 1), b);
    }
    m._set("E1", "=XLOOKUP(\"y\", A1:A2, B1:C2)");
    m._set("E4", "=XLOOKUP(\"q\", A1:A2, B1:C2, \"none\")");
    m.evaluate();
    assert_eq!(spilled(&m, &["E1", "F1"]), ["3", "4"]);
    assert_eq!(m._get_text("E4"), "none");
}

#[test]
fn randarray_is_calculated_again_every_time() {
    let mut m = new_empty_model();
    m._set("A1", "=RANDARRAY(3)");
    m.evaluate();
    let first = spilled(&m, &["A1", "A2", "A3"]);
    m.evaluate();
    let second = spilled(&m, &["A1", "A2", "A3"]);
    assert_ne!(first, second);
    assert!(second.iter().all(|v| v.parse::<f64>().is_ok()));
}

#[test]
fn a_ring_of_arrays_that_read_each_others_cells_is_circular() {
    let mut m = new_empty_model();
    m._set("A1", "=SEQUENCE(IF(ISBLANK(B2), 2, 1))");
    m._set("B1", "=SEQUENCE(IF(ISBLANK(A2), 1, 2))");
    m.evaluate();
    let out = m.last.as_ref().unwrap();
    // A1 is two cells while B2 is blank, B1 is two cells while A2 is not: no value settles
    assert!(out.circular);
    // the arrays that kept moving show the circular-reference error and give their cells up
    assert!(m._get_text("A1") == "#NUM!" || m._get_text("B1") == "#NUM!");
    assert_eq!(spilled(&m, &["A2", "B2"]), ["", ""]);
}

#[test]
fn a_column_reader_sees_the_filled_cells() {
    let mut m = new_empty_model();
    m._set("A1", "=SEQUENCE(2000)");
    m._set("B1", "=SUM(A:A)");
    m._set("B2", "=COUNTIF(A:A, \">1000\")");
    m._set("B3", "=SUMIF(A:A, \"<=10\")");
    m.evaluate();
    assert_eq!(spilled(&m, &["B1", "B2", "B3"]), ["2001000", "1000", "55"]);
}

#[test]
fn a_request_without_a_session_settles_arrays_too() {
    use easyoffice_sheet::{SheetEngine, WorkbookEngine, WorkbookEvalTarget};
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    for (cell, v) in [
        ("A1", "3"),
        ("A2", "1"),
        ("A3", "2"),
        ("C1", "=SORT(A1:A3)"),
        ("E1", "=SUM(C1#)"),
        ("E2", "=C3+100"),
    ] {
        wb.set_cell("Sheet1", cell, v).unwrap();
    }
    let target = |expr| WorkbookEvalTarget {
        sheet: "Sheet1",
        expr,
        anchor: None,
    };
    let targets = [
        target("C1"),
        target("C2"),
        target("C3"),
        target("E1"),
        target("E2"),
        target("=TRANSPOSE(A1:A3)"),
    ];
    let batch = wb.evaluate_targets(&targets);
    let shown: Vec<String> = batch.values.iter().map(|v| v.to_display_string()).collect();
    assert_eq!(shown, ["1", "2", "3", "6", "103", "3"]);
    assert_eq!(batch.spills.len(), 1);
    let area = &batch.spills[0];
    assert_eq!((area.rows, area.cols, area.blocked), (3, 1, false));
    assert_eq!(area.values.len(), 3);

    // the same through a standalone sheet (what a save calculates)
    let mut sheet = SheetEngine::new("Sheet1");
    for (cell, v) in [("A1", "3"), ("A2", "1"), ("C1", "=SORT(A1:A2)")] {
        sheet.set_cell(cell, v).unwrap();
    }
    let (values, timed_out) = sheet.recalculate_all_within(None);
    assert!(!timed_out);
    let at = |a1: &str| {
        values[&easyoffice_sheet::PackedCellCoordinate::parse_a1(a1).unwrap()].to_display_string()
    };
    assert_eq!((at("C1"), at("C2")), ("1".to_string(), "3".to_string()));
}
