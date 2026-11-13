//! Defined names inside a session: a change of a name or of a cell a name reads recomputes exactly
//! the formulas that use it, and the values equal a full recalculation.

use super::*;
use crate::axis::{Axis, AxisBudget, AxisEdit, EditKind};
use crate::WorkbookEvalTarget;
use easyoffice_formats::DefinedName;

fn at(a1: &str) -> PackedCellCoordinate {
    PackedCellCoordinate::parse_a1(a1).unwrap()
}

fn def(name: &str, refers_to: &str) -> DefinedName {
    DefinedName {
        name: name.to_string(),
        refers_to: refers_to.to_string(),
        ..DefinedName::default()
    }
}

fn session() -> RecalcSession {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    wb.add_sheet("Data");
    for (cell, raw) in [
        ("A1", "1"),
        ("A2", "2"),
        ("A3", "3"),
        ("C1", "=SUM(Prices)"),
        ("C2", "=Rate2*10"),
        ("C3", "=C1+1"),
        ("C4", "=Sheet1!A1"),
    ] {
        wb.set_cell("Sheet1", cell, raw).unwrap();
    }
    let mut s = RecalcSession::new(wb).unwrap();
    s.set_names(&[
        def("Prices", "Sheet1!$A$1:$A$3"),
        def("Rate2", "Sheet1!$A$2"),
    ])
    .unwrap();
    s.recalculate();
    s
}

fn shown(s: &RecalcSession, cell: &str) -> String {
    s.cell_value("Sheet1", at(cell)).to_display_string()
}

fn matches_full(s: &RecalcSession) {
    let wb = s.workbook();
    let sheet = wb.sheet("Sheet1").unwrap();
    for (&coord, raw) in sheet.raw_cells() {
        if raw.starts_with('=') {
            let a1 = coord.to_a1();
            let full = wb
                .evaluate_targets(&[WorkbookEvalTarget {
                    sheet: "Sheet1",
                    expr: &a1,
                    anchor: None,
                }])
                .values[0]
                .to_display_string();
            assert_eq!(shown(s, &a1), full, "{a1}");
        }
    }
}

#[test]
fn names_set_after_the_formulas_were_entered_compute_them() {
    let s = session();
    assert_eq!(shown(&s, "C1"), "6");
    assert_eq!(shown(&s, "C2"), "20");
    assert_eq!(shown(&s, "C3"), "7");
    matches_full(&s);
}

#[test]
fn editing_a_cell_a_name_reads_recomputes_the_formulas_that_use_the_name() {
    let mut s = session();
    s.set_cell("Sheet1", at("A3"), "30").unwrap();
    let out = s.recalculate();
    assert_eq!(shown(&s, "C1"), "33");
    assert_eq!(shown(&s, "C3"), "34");
    assert_eq!(out.recomputed, 2);
    matches_full(&s);
}

#[test]
fn redefining_a_name_recomputes_what_uses_it_and_leaves_the_rest() {
    let mut s = session();
    s.set_names(&[
        def("Prices", "Sheet1!$A$2:$A$3"),
        def("Rate2", "Sheet1!$A$2"),
    ])
    .unwrap();
    let out = s.recalculate();
    assert_eq!(shown(&s, "C1"), "5");
    assert_eq!(shown(&s, "C3"), "6");
    assert_eq!(out.recomputed, 2);
    let changed: Vec<String> = out.changed.iter().map(|c| c.coord.to_a1()).collect();
    assert_eq!(changed, ["C1", "C3"]);
    matches_full(&s);
    s.set_names(&[]).unwrap();
    s.recalculate();
    assert_eq!(shown(&s, "C1"), "#NAME?");
    assert_eq!(shown(&s, "C2"), "#NAME?");
    assert_eq!(shown(&s, "C3"), "#NAME?");
    assert_eq!(shown(&s, "C4"), "1");
    matches_full(&s);
}

#[test]
fn a_name_that_turns_a_formula_into_a_constant_and_back_keeps_the_graph_right() {
    let mut s = session();
    s.set_names(&[def("Prices", "5"), def("Rate2", "Sheet1!$A$2")])
        .unwrap();
    s.recalculate();
    assert_eq!(shown(&s, "C1"), "5");
    assert_eq!(shown(&s, "C3"), "6");
    s.set_names(&[
        def("Prices", "Sheet1!$A$1:$A$3"),
        def("Rate2", "Sheet1!$A$2"),
    ])
    .unwrap();
    s.set_cell("Sheet1", at("A1"), "100").unwrap();
    s.recalculate();
    assert_eq!(shown(&s, "C1"), "105");
    assert_eq!(shown(&s, "C3"), "106");
    matches_full(&s);
}

#[test]
fn an_invalid_set_of_names_changes_nothing() {
    let mut s = session();
    assert!(s.set_names(&[def("A1", "1")]).is_err());
    assert_eq!(s.workbook().names().len(), 2);
    s.set_cell("Sheet1", at("A1"), "9").unwrap();
    s.recalculate();
    assert_eq!(shown(&s, "C1"), "14");
}

#[test]
fn a_row_insertion_in_the_session_moves_the_names_and_recomputes() {
    let mut s = session();
    let edit = AxisEdit::new(Axis::Row, EditKind::Insert, 0, 1).unwrap();
    let (report, _) = s
        .apply_axis_edit("Sheet1", edit, AxisBudget::default())
        .unwrap();
    assert!(report.names_changed);
    assert_eq!(s.workbook().names()[0].refers_to, "Sheet1!$A$2:$A$4");
    assert_eq!(shown(&s, "C2"), "6");
    s.set_cell("Sheet1", at("A4"), "10").unwrap();
    s.recalculate();
    assert_eq!(shown(&s, "C2"), "13");
    matches_full(&s);
}

#[test]
fn the_budget_counts_what_the_names_expand_to() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    for i in 1..=200 {
        wb.set_cell("Sheet1", &format!("B{i}"), "=Wide").unwrap();
    }
    let mut s = RecalcSession::new(wb).unwrap();
    let before = s.footprint().ast_bytes;
    let wide = vec!["Sheet1!$A$1"; 100].join("+");
    s.set_names(&[def("Wide", &wide)]).unwrap();
    assert!(s.footprint().ast_bytes > before + 200 * 100 * 32);
}
