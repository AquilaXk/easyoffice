use crate::{PackedCellCoordinate, SheetEngine};

#[test]
fn sort_keeps_unquoted_sheet_names_and_shifts_their_references_like_a_copy() {
    let mut s = SheetEngine::new("S");
    s.set_cell("A1", "2").unwrap();
    s.set_cell("B1", "=Q1!B2+Sheet2!A2+'Q1'!C2").unwrap();
    s.set_cell("A2", "1").unwrap();
    s.sort_rows(0, 1, 0, true, None, true).unwrap();
    let raw = s
        .cells
        .get(&PackedCellCoordinate::parse_a1("B2").unwrap())
        .unwrap();
    assert_eq!(raw, "=Q1!B3+Sheet2!A3+'Q1'!C3");
}
