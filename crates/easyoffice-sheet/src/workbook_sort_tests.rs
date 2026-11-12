use crate::{PackedCellCoordinate, SheetEngine};

fn raw(s: &SheetEngine, a1: &str) -> Option<String> {
    s.cells
        .get(&PackedCellCoordinate::parse_a1(a1).unwrap())
        .cloned()
}

#[test]
fn sort_leaves_other_sheet_references_as_written() {
    let mut s = SheetEngine::new("S");
    s.set_cell("A1", "2").unwrap();
    s.set_cell("B1", "=Q1!B2+Sheet2!A2+'Q1'!C2+Q1!A1:B2")
        .unwrap();
    s.set_cell("A2", "1").unwrap();
    s.sort_rows(0, 1, 0, true, None, true).unwrap();
    assert_eq!(raw(&s, "B2").unwrap(), "=Q1!B2+Sheet2!A2+'Q1'!C2+Q1!A1:B2");
}

#[test]
fn sort_moves_references_into_the_sorted_rows_and_keeps_outside_ones() {
    let mut s = SheetEngine::new("S");
    s.set_cell("A1", "head").unwrap();
    s.set_cell("A2", "2").unwrap();
    s.set_cell("B2", "=A2+A1+A9+$A$2+S!A2").unwrap();
    s.set_cell("C2", "=SUM(A2:B2)+SUM(A1:A3)").unwrap();
    s.set_cell("A3", "1").unwrap();
    s.sort_rows(1, 2, 0, true, None, true).unwrap();
    assert_eq!(raw(&s, "B3").unwrap(), "=A3+A1+A9+$A$2+S!A3");
    assert_eq!(raw(&s, "C3").unwrap(), "=SUM(A3:B3)+SUM(A1:A3)");
}

#[test]
fn sort_turns_references_that_leave_the_grid_into_ref_errors() {
    let mut s = SheetEngine::new("S");
    s.set_cell("A1", "9").unwrap();
    s.set_cell("A2", "1").unwrap();
    s.set_cell("B2", "=A1+A2").unwrap();
    s.set_cell("C2", "=SUM(A1:A2)").unwrap();
    s.sort_rows(0, 1, 0, true, None, true).unwrap();
    assert_eq!(raw(&s, "B1").unwrap(), "=#REF!+A1");
    assert_eq!(raw(&s, "C1").unwrap(), "=SUM(#REF!)");
}
