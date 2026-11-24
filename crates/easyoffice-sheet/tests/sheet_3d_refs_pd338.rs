//! PD-338: 3D sheet-span references (`Sheet1:Sheet3!A1`, `'1월:3월'!B2:C3`, `'Sheet 1':'Sheet 3'!A1`),
//! variadic/positional/array-shape/info functions, incremental recalc, and sheet/axis rewrites.
#![allow(clippy::unwrap_used)]

use easyoffice_formats::{
    parse_sqref, CellBorders, CfKind, CfRule, CfStyle, DefinedName, SheetRules,
};
use easyoffice_sheet::{
    axis::{rewrite_formula_refs, rewrite_sheet_names, Axis, AxisEdit, EditKind, SheetChange},
    EvaluatedCellValue, FormulaError, PackedCellCoordinate, RecalcSession, WorkbookEngine,
};

fn setup_quarterly_workbook() -> WorkbookEngine {
    let mut wb = WorkbookEngine::new();
    {
        let s = wb.add_sheet("1월");
        s.set_cell("A1", "10").unwrap();
        s.set_cell("B1", "20").unwrap();
        s.set_cell("A2", "TRUE").unwrap();
        s.set_cell("B2", "메모").unwrap();
    }
    {
        let s = wb.add_sheet("2월");
        s.set_cell("A1", "30").unwrap();
        s.set_cell("B1", "메모").unwrap();
        s.set_cell("A2", "TRUE").unwrap();
        s.set_cell("B2", "FALSE").unwrap();
    }
    {
        let s = wb.add_sheet("3월");
        s.set_cell("A1", "50").unwrap();
        s.set_cell("B1", "60").unwrap();
        s.set_cell("A2", "1").unwrap();
        s.set_cell("B2", "TRUE").unwrap();
    }
    {
        let s = wb.add_sheet("요약");
        s.set_cell("A1", "0").unwrap();
    }
    wb
}

fn num(v: EvaluatedCellValue) -> f64 {
    match v {
        EvaluatedCellValue::Number(n) => n,
        other => panic!("expected Number, got {other:?}"),
    }
}

#[test]
fn syntax_variants_and_variadic_functions_work_across_sheet_spans() {
    let mut wb = WorkbookEngine::new();
    for (name, v1, v2) in [
        ("Sheet1", "10", "2"),
        ("Sheet 2", "20", "4"),
        ("Sheet3", "30", "6"),
        ("Summary", "999", "0"),
    ] {
        let s = wb.add_sheet(name);
        s.set_cell("A1", v1).unwrap();
        s.set_cell("B1", v2).unwrap();
    }

    assert_eq!(
        wb.evaluate_formula_on_sheet("Summary", "=SUM(Sheet1:Sheet3!A1)"),
        EvaluatedCellValue::Number(60.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Summary", "=SUM(Sheet3:Sheet1!A1)"),
        EvaluatedCellValue::Number(60.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Summary", "=SUM('Sheet1:Sheet3'!A1:B1)"),
        EvaluatedCellValue::Number(72.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Summary", "=SUM('Sheet 2':'Sheet3'!$A$1:B$1)"),
        EvaluatedCellValue::Number(60.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Summary", "=SUM(Sheet1:Sheet3!A:B)"),
        EvaluatedCellValue::Number(72.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Summary", "=SUM(Sheet1:Sheet3!1:1)"),
        EvaluatedCellValue::Number(72.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Summary", "=SUM('Sheet 2':'Sheet 2'!A1)"),
        EvaluatedCellValue::Number(20.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Summary", "=AVERAGE(Sheet1:Sheet3!A1)"),
        EvaluatedCellValue::Number(20.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Summary", "=COUNT(Sheet1:Sheet3!A1:B1)"),
        EvaluatedCellValue::Number(6.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Summary", "=MIN(Sheet1:Sheet3!B1)"),
        EvaluatedCellValue::Number(2.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Summary", "=MAX(Sheet1:Sheet3!A1)"),
        EvaluatedCellValue::Number(30.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Summary", "=PRODUCT(Sheet1:Sheet3!B1)"),
        EvaluatedCellValue::Number(48.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Summary", "=SUMSQ(Sheet1:Sheet3!B1)"),
        EvaluatedCellValue::Number(56.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Summary", "=MEDIAN(Sheet1:Sheet3!A1)"),
        EvaluatedCellValue::Number(20.0)
    );
    assert!(
        (num(wb.evaluate_formula_on_sheet("Summary", "=STDEV.S(Sheet1:Sheet3!A1)")) - 10.0).abs()
            < 1e-12
    );
    assert!(
        (num(wb.evaluate_formula_on_sheet("Summary", "=VAR.S(Sheet1:Sheet3!A1)")) - 100.0).abs()
            < 1e-12
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Summary", "=DEVSQ(Sheet1:Sheet3!A1)"),
        EvaluatedCellValue::Number(200.0)
    );
    assert!(
        (num(wb.evaluate_formula_on_sheet("Summary", "=AVEDEV(Sheet1:Sheet3!A1)")) - (20.0 / 3.0))
            .abs()
            < 1e-12
    );
}

#[test]
fn korean_quoted_spans_a_functions_logical_positional_and_array_shape() {
    let wb = setup_quarterly_workbook();

    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=SUM('1월:3월'!A1:B1)"),
        EvaluatedCellValue::Number(170.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=COUNTA('1월:3월'!A1:B1)"),
        EvaluatedCellValue::Number(6.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=COUNT('1월:3월'!A1:B1)"),
        EvaluatedCellValue::Number(5.0)
    );
    assert!(
        (num(wb.evaluate_formula_on_sheet("요약", "=AVERAGEA('1월:3월'!A1:B1)")) - (170.0 / 6.0))
            .abs()
            < 1e-12
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=MINA('1월:3월'!A1:B1)"),
        EvaluatedCellValue::Number(0.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=MAXA('1월:3월'!A1:B1)"),
        EvaluatedCellValue::Number(60.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=AND('1월:3월'!A2)"),
        EvaluatedCellValue::Boolean(true)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=OR('1월:3월'!B2)"),
        EvaluatedCellValue::Boolean(true)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=XOR('1월:3월'!A2)"),
        EvaluatedCellValue::Boolean(true)
    );

    // Positional 3D statistical functions
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=SMALL('1월:3월'!A1:B1, 2)"),
        EvaluatedCellValue::Number(20.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=LARGE('1월:3월'!A1:B1, 2)"),
        EvaluatedCellValue::Number(50.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=PERCENTILE.INC('1월:3월'!A1:B1, 0.5)"),
        EvaluatedCellValue::Number(30.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=QUARTILE.INC('1월:3월'!A1:B1, 1)"),
        EvaluatedCellValue::Number(20.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=TRIMMEAN('1월:3월'!A1:B1, 0.4)"),
        EvaluatedCellValue::Number(100.0 / 3.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=RANK.EQ(30, '1월:3월'!A1:B1)"),
        EvaluatedCellValue::Number(3.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=RANK.AVG(30, '1월:3월'!A1:B1, 1)"),
        EvaluatedCellValue::Number(3.0)
    );

    // Info functions
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=ISREF('1월:3월'!A1:B2)"),
        EvaluatedCellValue::Boolean(true)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=ISREF('없음:3월'!A1)"),
        EvaluatedCellValue::Boolean(false)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=SHEETS('1월:3월'!A1:B2)"),
        EvaluatedCellValue::Number(3.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=SHEETS('3월:1월'!B2)"),
        EvaluatedCellValue::Number(3.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=SHEETS('2월:2월'!A1)"),
        EvaluatedCellValue::Number(1.0)
    );
}

#[test]
fn array_shape_functions_and_let_and_defined_names_support_3d_refs() {
    let mut wb = setup_quarterly_workbook();
    wb.set_names(&[DefinedName {
        name: "분기매출".to_string(),
        scope: None,
        refers_to: "='1월:3월'!$A$1:$B$1".to_string(),
        comment: String::new(),
        hidden: false,
    }])
    .unwrap();

    // LET binding + VSTACK / HSTACK / TOCOL / TOROW
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=SUM(분기매출)"),
        EvaluatedCellValue::Number(170.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=LET(r, '1월:3월'!A1:B1, SUM(r))"),
        EvaluatedCellValue::Number(170.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=ROWS(VSTACK('1월:3월'!A1:B1))"),
        EvaluatedCellValue::Number(3.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=INDEX(VSTACK('1월:3월'!A1:B1), 3, 2)"),
        EvaluatedCellValue::Number(60.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=COLUMNS(HSTACK('1월:3월'!A1:B1))"),
        EvaluatedCellValue::Number(6.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=INDEX(HSTACK('1월:3월'!A1:B1), 1, 5)"),
        EvaluatedCellValue::Number(50.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=INDEX(TOCOL('1월:3월'!A1:B1), 4, 1)"),
        EvaluatedCellValue::Text("메모".to_string())
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=INDEX(TOROW('1월:3월'!A1:B1), 1, 6)"),
        EvaluatedCellValue::Number(60.0)
    );
}

#[test]
fn error_contracts_and_literal_colon_sheet_coexist() {
    let mut wb = setup_quarterly_workbook();
    wb.add_sheet("Q1:Q2").set_cell("A1", "77").unwrap();

    // Literal sheet containing a colon still resolves normally
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "='Q1:Q2'!A1"),
        EvaluatedCellValue::Number(77.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=ABS('Q1:Q2'!A1)*(-1)+100"),
        EvaluatedCellValue::Number(23.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=SHEETS('Q1:Q2'!A1)"),
        EvaluatedCellValue::Number(1.0)
    );

    // Missing endpoint -> #REF!; bare or unsupported function on valid 3D span -> #VALUE!
    for f in [
        "='없음:3월'!A1",
        "=SUM('1월:없음'!A1)",
        "=SUM(Sheet1:Sheet3!A1)",
        "=SHEETS('1월:없음'!A1)",
        "=SUM('1월:3월'!A1#)",
        "=SUM('1월'!A1:'3월'!B1)",
    ] {
        assert_eq!(
            wb.evaluate_formula_on_sheet("요약", f),
            EvaluatedCellValue::Error(FormulaError::Ref),
            "{f}"
        );
    }

    for f in [
        "='1월:3월'!A1",
        "='1월:3월'!A1+1",
        "=ABS('1월:3월'!A1)",
        "=COUNTBLANK('1월:3월'!A1:B1)",
        "=SUMIF('1월:3월'!A1:B1, \">0\")",
        "=VLOOKUP(10, '1월:3월'!A1:B2, 2, FALSE)",
        "=ROW('1월:3월'!A1)",
        "=ROWS('1월:3월'!A1:B2)",
        "=SHEET('1월:3월'!A1)",
    ] {
        assert_eq!(
            wb.evaluate_formula_on_sheet("요약", f),
            EvaluatedCellValue::Error(FormulaError::Value),
            "{f}"
        );
    }

    // Lazy IF branch does not evaluate unchosen 3D span
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=IF(TRUE, 42, '1월:3월'!A1)"),
        EvaluatedCellValue::Number(42.0)
    );
}

#[test]
fn recalc_session_batch_dag_and_axis_sheet_rewrites() {
    let mut wb = setup_quarterly_workbook();
    wb.set_cell("요약", "B1", "=SUM('1월:3월'!A1)").unwrap();

    // Batch DAG evaluates interior sheet edit
    assert_eq!(
        wb.evaluate_cell("요약", "B1"),
        EvaluatedCellValue::Number(90.0)
    );
    wb.set_cell("2월", "A1", "100").unwrap();
    assert_eq!(
        wb.evaluate_cell("요약", "B1"),
        EvaluatedCellValue::Number(160.0)
    );

    // RecalcSession tracks interior sheet mutations incrementally
    let mut session = RecalcSession::new(wb).unwrap();
    let out1 = session.recalculate();
    assert!(out1
        .changed
        .iter()
        .any(|c| c.coord == PackedCellCoordinate::parse_a1("B1").unwrap()));
    session
        .set_cell("2월", PackedCellCoordinate::parse_a1("A1").unwrap(), "40")
        .unwrap();
    let _out2 = session.recalculate();
    assert_eq!(
        session.workbook().evaluate_cell("요약", "B1"),
        EvaluatedCellValue::Number(100.0)
    );

    // Axis rewrite: single-sheet row/col edit leaves cross-sheet 3D refs intact, CF shifts with formula
    let edit = AxisEdit::new(Axis::Row, EditKind::Insert, 0, 2).unwrap();
    assert_eq!(
        rewrite_formula_refs("=SUM(Sheet1:Sheet3!A1)", "Sheet1", "Sheet1", &edit, false),
        None
    );

    // Sheet rename and delete rewrites preserve 3D span syntax
    assert_eq!(
        rewrite_sheet_names(
            "=SUM(Sheet1:Sheet3!A1:B2)",
            SheetChange::Rename {
                from: "Sheet3",
                to: "3분기 합계"
            }
        )
        .as_deref(),
        Some("=SUM('Sheet1:3분기 합계'!A1:B2)")
    );
    assert_eq!(
        rewrite_sheet_names(
            "=SUM('Sheet 1:Sheet3'!A1)",
            SheetChange::Rename {
                from: "Sheet 1",
                to: "Sheet1"
            }
        )
        .as_deref(),
        Some("=SUM(Sheet1:Sheet3!A1)")
    );
    assert_eq!(
        rewrite_sheet_names(
            "=SUM('Sheet 1':'Sheet 3'!A1)",
            SheetChange::Rename {
                from: "Sheet 3",
                to: "끝'시트"
            }
        )
        .as_deref(),
        Some("=SUM('Sheet 1:끝''시트'!A1)")
    );
    assert_eq!(
        rewrite_sheet_names(
            "=SUM(Sheet1:Sheet3!A1:B2)",
            SheetChange::Delete { name: "Sheet3" }
        )
        .as_deref(),
        Some("=SUM(#REF!)")
    );
    // Deleting an interior sheet leaves the outer span intact
    assert_eq!(
        rewrite_sheet_names(
            "=SUM(Sheet1:Sheet3!A1:B2)",
            SheetChange::Delete { name: "Sheet2" }
        ),
        None
    );

    // Relative 3D CF rule template shifts down across target range
    let mut wb_cf = setup_quarterly_workbook();
    wb_cf.set_cell("1월", "C1", "10").unwrap();
    wb_cf.set_cell("3월", "C1", "20").unwrap();
    wb_cf.set_cell("1월", "C2", "0").unwrap();
    let rules = SheetRules {
        cf: vec![CfRule {
            ranges: parse_sqref("D1:D2"),
            priority: 1,
            stop_if_true: false,
            kind: CfKind::Expression("SUM('1월:3월'!C1)>=20".to_string()),
            style: CfStyle {
                bg: Some(0xFF_E0_E0),
                fg: None,
                flags: 0,
                border: CellBorders::NONE,
            },
        }],
        ..SheetRules::default()
    };
    let visuals = wb_cf.evaluate_rules("요약", rules);
    assert_eq!(visuals.len(), 1);
    assert_eq!(
        visuals[0].coord,
        PackedCellCoordinate::parse_a1("D1").unwrap()
    );
}
