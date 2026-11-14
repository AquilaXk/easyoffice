//! Unit tests for single- and multi-criteria range aggregation.

use super::*;
use crate::{EvaluatedCellValue, SheetEngine};

#[test]
fn text_criteria_keep_surrounding_spaces() {
    let mut s = SheetEngine::new("SpaceCrit");
    s.set_cell("A1", " ").unwrap();
    s.set_cell("A2", "x").unwrap();
    s.set_cell("A3", "  ").unwrap();
    assert_eq!(
        s.evaluate_formula_str("=COUNTIF(A1:A3, \" \")"),
        EvaluatedCellValue::Number(1.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=COUNTIF(A1:A3, \"=  \")"),
        EvaluatedCellValue::Number(1.0)
    );
    s.set_cell("B1", "5").unwrap();
    assert_eq!(
        s.evaluate_formula_str("=COUNTIF(B1:B1, \" >= 5 \")"),
        EvaluatedCellValue::Number(0.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=COUNTIF(B1:B1, \">= 5 \")"),
        EvaluatedCellValue::Number(1.0)
    );
}

#[test]
fn cell_index_sparse_range_queries_and_invalidation() {
    let mut s = SheetEngine::new("SparseTest");
    s.set_cell("A5", "10").unwrap();
    s.set_cell("A500", "25").unwrap();
    s.set_cell("A2000", "40").unwrap();
    s.set_cell("B5", "100").unwrap();
    s.set_cell("B500", "250").unwrap();
    s.set_cell("B2000", "400").unwrap();

    assert_eq!(
        s.evaluate_formula_str("=SUM(A1:B5000)"),
        EvaluatedCellValue::Number(825.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=SUMIF(A1:A5000, \">15\", B1:B5000)"),
        EvaluatedCellValue::Number(650.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=COUNTIF(A1:A5000, \">=25\")"),
        EvaluatedCellValue::Number(2.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=SUMIFS(B1:B5000, A1:A5000, \">10\", B1:B5000, \"<300\")"),
        EvaluatedCellValue::Number(250.0)
    );

    // Editing a cell invalidates the cached CellIndex.
    s.set_cell("A1200", "30").unwrap();
    s.set_cell("B1200", "200").unwrap();
    assert_eq!(
        s.evaluate_formula_str("=SUMIFS(B1:B5000, A1:A5000, \">10\", B1:B5000, \"<300\")"),
        EvaluatedCellValue::Number(450.0)
    );
}

#[test]
fn q2_sparse_col_avoids_dense_scan_when_other_cols_populated() {
    let mut s = SheetEngine::new("SparseQ2");
    // Populate 8000 rows in Z so whole-sheet `cells.len() * 2` > 15_000 (defeating
    // whole-sheet heuristics while A1:A15000 has only 2 populated cells).
    for r in 1..=8000u32 {
        s.set_cell(&format!("Z{r}"), r.to_string()).unwrap();
    }
    s.set_cell("A5", "10").unwrap();
    s.set_cell("A20", "=\"\"").unwrap();

    assert_eq!(
        s.evaluate_formula_str("=COUNT(A1:A15000)"),
        EvaluatedCellValue::Number(1.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=COUNTA(A1:A15000)"),
        EvaluatedCellValue::Number(2.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=COUNTBLANK(A1:A15000)"),
        EvaluatedCellValue::Number(14999.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=COUNTIF(A1:A15000, \">5\")"),
        EvaluatedCellValue::Number(1.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=COUNTIF(A1:A15000, \"\")"),
        EvaluatedCellValue::Number(14999.0)
    );

    // 150 calls stay under MAX_FORMULA_LEN (8192) while dense scans would hit 2.25M > MAX_EVAL_STEPS.
    for (expr, expected) in [
        ("COUNT(A1:A15000)", 150.0),
        ("COUNTA(A1:A15000)", 300.0),
        ("COUNTBLANK(A1:A15000)", 150.0 * 14999.0),
        ("COUNTIF(A1:A15000, \">5\")", 150.0),
        ("COUNTIFS(Z1:Z15000, \">0\", B1:B15000, \">0\")", 0.0),
    ] {
        let joined = (0..150).map(|_| expr).collect::<Vec<_>>().join("+");
        assert_eq!(
            s.evaluate_formula_str(&format!("={joined}")),
            EvaluatedCellValue::Number(expected),
            "expr={expr}"
        );
    }

    // Array condition evaluation in sparse range (`array_eval::read_ref_grid`): `"" > 5` is TRUE in spreadsheet comparison order (10 and "" => 2 hits).
    let arr_cond = (0..40)
        .map(|_| "SUM(--(A1:A15000>5))")
        .collect::<Vec<_>>()
        .join("+");
    assert_eq!(
        s.evaluate_formula_str(&format!("={arr_cond}")),
        EvaluatedCellValue::Number(80.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=SUM(--(A1:A15000=10))"),
        EvaluatedCellValue::Number(1.0)
    );

    // Spilled cells committed via RecalcSession remain accurately counted in sparse ranges.
    let mut wb = crate::WorkbookEngine::new();
    wb.add_sheet("SpillQ2");
    wb.set_cell("SpillQ2", "A10", "=SEQUENCE(5)").unwrap();
    wb.set_cell("SpillQ2", "B1", "=COUNT(A1:A3500)").unwrap();
    wb.set_cell("SpillQ2", "B2", "=COUNTA(A1:A3500)").unwrap();
    wb.set_cell("SpillQ2", "B3", "=COUNTBLANK(A1:A3500)")
        .unwrap();
    wb.set_cell("SpillQ2", "B4", "=COUNTIF(A1:A3500, \">2\")")
        .unwrap();
    let mut session = crate::RecalcSession::new(wb).unwrap();
    session.recalculate();
    let a1 = |a: &str| PackedCellCoordinate::parse_a1(a).unwrap();
    assert_eq!(
        session.cell_value("SpillQ2", a1("B1")),
        EvaluatedCellValue::Number(5.0)
    );
    assert_eq!(
        session.cell_value("SpillQ2", a1("B2")),
        EvaluatedCellValue::Number(5.0)
    );
    assert_eq!(
        session.cell_value("SpillQ2", a1("B3")),
        EvaluatedCellValue::Number(3495.0)
    );
    assert_eq!(
        session.cell_value("SpillQ2", a1("B4")),
        EvaluatedCellValue::Number(3.0)
    );
}

#[test]
fn countif_matches_numeric_tolerance_coerced_text_and_booleans_in_large_range() {
    let mut s = SheetEngine::new("CritTolTest");
    s.set_cell("A1", "=0.1+0.2").unwrap();
    s.set_cell("A2", "=\"10\"").unwrap();
    s.set_cell("A3", "=TRUE").unwrap();
    for r in 4..=200 {
        s.set_cell(&format!("A{r}"), "xyz").unwrap();
    }
    assert_eq!(
        s.evaluate_formula_str("=COUNTIF(A1:A200, 0.3)"),
        EvaluatedCellValue::Number(1.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=COUNTIF(A1:A200, 10)"),
        EvaluatedCellValue::Number(1.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=COUNTIF(A1:A200, \"TRUE\")"),
        EvaluatedCellValue::Number(1.0)
    );
}

#[test]
fn stale_formula_and_vlookup_cache_isolation_on_edit() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    wb.set_cell("Sheet1", "A1", "=B1").unwrap();
    wb.set_cell("Sheet1", "B1", "10").unwrap();
    for r in 2..=200 {
        wb.set_cell("Sheet1", &format!("B{r}"), "999").unwrap();
    }
    wb.set_cell("Sheet1", "C1", "=COUNTIF(A1:A200, 10)")
        .unwrap();
    wb.set_cell("Sheet1", "C2", "=VLOOKUP(10, A1:B200, 2, FALSE)")
        .unwrap();

    assert_eq!(
        wb.evaluate_cell("Sheet1", "C1"),
        EvaluatedCellValue::Number(1.0)
    );
    assert_eq!(
        wb.evaluate_cell("Sheet1", "C2"),
        EvaluatedCellValue::Number(10.0)
    );

    // Edit B1 to 20
    wb.set_cell("Sheet1", "B1", "20").unwrap();
    wb.set_cell("Sheet1", "C3", "=COUNTIF(A1:A200, 20)")
        .unwrap();
    wb.set_cell("Sheet1", "C4", "=VLOOKUP(20, A1:B200, 2, FALSE)")
        .unwrap();

    assert_eq!(
        wb.evaluate_cell("Sheet1", "C1"),
        EvaluatedCellValue::Number(0.0)
    );
    assert_eq!(
        wb.evaluate_cell("Sheet1", "C3"),
        EvaluatedCellValue::Number(1.0)
    );
    assert_eq!(
        wb.evaluate_cell("Sheet1", "C4"),
        EvaluatedCellValue::Number(20.0)
    );
}
