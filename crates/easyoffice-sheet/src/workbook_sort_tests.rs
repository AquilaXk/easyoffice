use crate::{PackedCellCoordinate, SheetEngine, SortKey};

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
    s.sort_rows(0, 1, &[SortKey::asc(0)], false).unwrap();
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
    s.sort_rows(1, 2, &[SortKey::asc(0)], false).unwrap();
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
    s.sort_rows(0, 1, &[SortKey::asc(0)], false).unwrap();
    assert_eq!(raw(&s, "B1").unwrap(), "=#REF!+A1");
    assert_eq!(raw(&s, "C1").unwrap(), "=SUM(#REF!)");
}

fn column(s: &SheetEngine, col: char, rows: u32) -> Vec<String> {
    (1..=rows)
        .map(|r| s.evaluate_cell(&format!("{col}{r}")).to_display_string())
        .collect()
}

#[test]
fn sort_orders_by_several_keys_and_keeps_rows_together() {
    let mut s = SheetEngine::new("S");
    for (i, (dept, pay, name)) in [
        ("영업", "300", "다"),
        ("개발", "500", "가"),
        ("영업", "300", "나"),
        ("개발", "700", "라"),
        ("영업", "100", "마"),
    ]
    .iter()
    .enumerate()
    {
        let r = i + 1;
        s.set_cell(&format!("A{r}"), *dept).unwrap();
        s.set_cell(&format!("B{r}"), *pay).unwrap();
        s.set_cell(&format!("C{r}"), *name).unwrap();
    }
    s.sort_rows(0, 4, &[SortKey::asc(0), SortKey::desc(1)], false)
        .unwrap();
    assert_eq!(column(&s, 'A', 5), ["개발", "개발", "영업", "영업", "영업"]);
    assert_eq!(column(&s, 'B', 5), ["700", "500", "300", "300", "100"]);
    // Equal keys (영업, 300) keep their order, and the names travel with their rows.
    assert_eq!(column(&s, 'C', 5), ["라", "가", "다", "나", "마"]);
}

#[test]
fn sort_puts_blank_keys_last_and_is_case_aware_on_request() {
    let mut s = SheetEngine::new("S");
    for (r, v) in ["b", "", "A", "a", "B"].iter().enumerate() {
        if !v.is_empty() {
            s.set_cell(&format!("A{}", r + 1), *v).unwrap();
        }
        s.set_cell(&format!("B{}", r + 1), r.to_string()).unwrap();
    }
    s.sort_rows(0, 4, &[SortKey::asc(0)], true).unwrap();
    assert_eq!(column(&s, 'A', 5), ["a", "A", "b", "B", ""]);
    assert_eq!(column(&s, 'B', 5), ["3", "2", "0", "4", "1"]);
    s.sort_rows(0, 4, &[SortKey::desc(0)], false).unwrap();
    assert_eq!(column(&s, 'A', 5), ["b", "B", "a", "A", ""]);
}

#[test]
fn sort_refuses_bad_keys() {
    let mut s = SheetEngine::new("S");
    s.set_cell("A1", "1").unwrap();
    assert!(s.sort_rows(0, 0, &[], false).is_err());
    assert!(s.sort_rows(2, 1, &[SortKey::asc(0)], false).is_err());
    assert!(s.sort_rows(0, 0, &[SortKey::asc(16_384)], false).is_err());
    let too_many = vec![SortKey::asc(0); 65];
    assert!(s.sort_rows(0, 0, &too_many, false).is_err());
    let max = vec![SortKey::asc(0); 64];
    assert!(s.sort_rows(0, 0, &max, false).is_ok());
}
