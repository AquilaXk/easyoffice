//! Behavioural characterization for the slot-indexed memo, 2-bit DFS state, formula column index
//! and cached recalculation plan (#135). The same assertions hold on the pre-optimization engine,
//! so they pin "semantics identical" rather than the new internals.

use easyoffice_sheet::{
    EvaluatedCellValue as V, FormulaError, PackedCellCoordinate, SheetEngine, WorkbookEngine,
};

fn num(v: f64) -> V {
    V::Number(v)
}

fn at(sheet: &SheetEngine, a1: &str) -> V {
    sheet.evaluate_cell(a1)
}

fn all(sheet: &SheetEngine) -> std::collections::HashMap<PackedCellCoordinate, V> {
    sheet.recalculate_all()
}

fn get(map: &std::collections::HashMap<PackedCellCoordinate, V>, a1: &str) -> Option<V> {
    map.get(&PackedCellCoordinate::parse_a1(a1).unwrap())
        .cloned()
}

#[test]
fn recalculation_is_stable_across_repeats_and_follows_every_kind_of_edit() {
    let mut sheet = SheetEngine::new("Plan");
    sheet.set_cell("A1", "10").unwrap();
    sheet.set_cell("A2", "=A1*2").unwrap();
    sheet.set_cell("A3", "=SUM(A1:A2)").unwrap();
    let first = all(&sheet);
    assert_eq!(get(&first, "A3"), Some(num(30.0)));
    assert_eq!(all(&sheet), first, "cached replay equals the first run");
    assert_eq!(all(&sheet), first);

    sheet.set_cell("A1", "100").unwrap();
    let edited = all(&sheet);
    assert_eq!(get(&edited, "A3"), Some(num(300.0)));
    assert_eq!(at(&sheet, "A3"), num(300.0));

    sheet.set_cell("A4", "=A3+1").unwrap();
    assert_eq!(get(&all(&sheet), "A4"), Some(num(301.0)));

    sheet.set_cell("A2", "").unwrap();
    let cleared = all(&sheet);
    assert_eq!(get(&cleared, "A2"), None);
    assert_eq!(get(&cleared, "A3"), Some(num(100.0)));
    assert_eq!(get(&cleared, "A4"), Some(num(101.0)));

    sheet.set_cell("A1", "=A4").unwrap();
    let cyclic = all(&sheet);
    assert_eq!(get(&cyclic, "A1"), Some(V::Error(FormulaError::Cycle)));
    assert_eq!(all(&sheet), cyclic, "cycle results replay identically");
    sheet.set_cell("A1", "5").unwrap();
    assert_eq!(get(&all(&sheet), "A4"), Some(num(6.0)));
}

#[test]
fn replay_matches_first_run_for_cycles_errors_and_text() {
    let mut sheet = SheetEngine::new("Mixed");
    sheet.set_cell("A1", "=B1").unwrap();
    sheet.set_cell("B1", "=A1").unwrap();
    sheet.set_cell("C1", "=A1+1").unwrap();
    sheet.set_cell("D1", "=1/0").unwrap();
    sheet.set_cell("E1", "=IFERROR(D1, \"x\")&\"y\"").unwrap();
    sheet.set_cell("F1", "label").unwrap();
    sheet.set_cell("G1", "=COUNTA(A1:F1)").unwrap();
    let first = all(&sheet);
    assert_eq!(get(&first, "A1"), Some(V::Error(FormulaError::Cycle)));
    assert_eq!(get(&first, "D1"), Some(V::Error(FormulaError::DivByZero)));
    assert_eq!(get(&first, "E1"), Some(V::Text("xy".to_string())));
    assert_eq!(all(&sheet), first);
    assert_eq!(all(&sheet), first);
}

#[test]
fn deep_dependency_chain_beyond_recursion_budget_still_evaluates() {
    let mut sheet = SheetEngine::new("Chain");
    sheet.set_cell("A1", "1").unwrap();
    for r in 2..=5_000u32 {
        sheet
            .set_cell(&format!("A{r}"), format!("=A{}+1", r - 1))
            .unwrap();
    }
    assert_eq!(at(&sheet, "A5000"), num(5_000.0));
    let results = all(&sheet);
    assert_eq!(get(&results, "A5000"), Some(num(5_000.0)));
    assert_eq!(get(&all(&sheet), "A2500"), Some(num(2_500.0)));
}

#[test]
fn ten_thousand_rows_with_wide_ranges_match_closed_form_values() {
    let mut sheet = SheetEngine::new("Big");
    let rows = 10_000u32;
    for r in 1..=rows {
        sheet
            .set_cell(&format!("A{r}"), format!("{}", r * 3))
            .unwrap();
        sheet
            .set_cell(&format!("B{r}"), format!("=A{r}*2"))
            .unwrap();
        sheet
            .set_cell(&format!("C{r}"), format!("=SUM(A{r}:B{r})"))
            .unwrap();
    }
    sheet.set_cell("E1", "=SUM(B1:B10000)").unwrap();
    sheet.set_cell("E2", "=SUM(C1:C10000)").unwrap();
    sheet.set_cell("E3", "=COUNT(A1:C10000)").unwrap();
    let n = f64::from(rows);
    let sum_a = 3.0 * n * (n + 1.0) / 2.0;
    for _ in 0..2 {
        let results = all(&sheet);
        assert_eq!(get(&results, "E1"), Some(num(2.0 * sum_a)));
        assert_eq!(get(&results, "E2"), Some(num(3.0 * sum_a)));
        assert_eq!(get(&results, "E3"), Some(num(3.0 * n)));
        assert_eq!(get(&results, "C9999"), Some(num(3.0 * 9_999.0 * 3.0)));
    }
    assert_eq!(at(&sheet, "E2"), num(3.0 * sum_a));
}

#[test]
fn large_range_dependencies_are_found_by_the_column_index_including_cycles() {
    let mut sheet = SheetEngine::new("Idx");
    for r in 1..=600u32 {
        sheet.set_cell(&format!("B{r}"), format!("={r}*2")).unwrap();
    }
    sheet.set_cell("D1", "=SUM(B1:B600)").unwrap();
    sheet.set_cell("D2", "=D1+1").unwrap();
    assert_eq!(at(&sheet, "D2"), num(600.0 * 601.0 + 1.0));
    // A formula inside its own large range is circular.
    sheet.set_cell("B300", "=SUM(B1:B600)").unwrap();
    assert_eq!(at(&sheet, "B300"), V::Error(FormulaError::Cycle));
    let results = all(&sheet);
    assert_eq!(get(&results, "B300"), Some(V::Error(FormulaError::Cycle)));
    assert_eq!(all(&sheet), results);
}

#[test]
fn cells_outside_the_dense_window_use_the_overflow_paths() {
    let mut sheet = SheetEngine::new("Far");
    sheet.set_cell("Z1", "7").unwrap();
    sheet.set_cell("AA1", "=Z1*6").unwrap();
    sheet.set_cell("B20000", "5").unwrap();
    sheet.set_cell("C20000", "=B20000+AA1").unwrap();
    sheet.set_cell("XFD1048576", "=C20000+1").unwrap();
    assert_eq!(at(&sheet, "XFD1048576"), num(48.0));
    let results = all(&sheet);
    assert_eq!(get(&results, "XFD1048576"), Some(num(48.0)));
    assert_eq!(get(&all(&sheet), "C20000"), Some(num(47.0)));
}

#[test]
fn cross_sheet_dependencies_and_cycles_use_sheet_ids_not_names() {
    let mut wb = WorkbookEngine::new();
    {
        let s1 = wb.add_sheet("Data");
        for r in 1..=700u32 {
            s1.set_cell(&format!("A{r}"), format!("{r}")).unwrap();
            s1.set_cell(&format!("B{r}"), format!("=A{r}*2")).unwrap();
        }
    }
    {
        let s2 = wb.add_sheet("Summary");
        s2.set_cell("A1", "=SUM(Data!B1:B700)").unwrap();
        s2.set_cell("A2", "=data!B700+A1").unwrap();
        s2.set_cell("A3", "=Summary!A2*1").unwrap();
        s2.set_cell("B1", "=Loop!A1").unwrap();
    }
    wb.add_sheet("Loop").set_cell("A1", "=Summary!B1").unwrap();
    let total = 700.0 * 701.0;
    assert_eq!(wb.evaluate_cell("Summary", "A1"), num(total));
    assert_eq!(wb.evaluate_cell("Summary", "A3"), num(1_400.0 + total));
    assert_eq!(
        wb.evaluate_cell("Summary", "B1"),
        V::Error(FormulaError::Cycle)
    );
    assert_eq!(
        wb.evaluate_cell("Loop", "A1"),
        V::Error(FormulaError::Cycle)
    );
}
