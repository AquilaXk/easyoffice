//! `GETPIVOTDATA` (PD-271): a value of a registered pivot table, worked out with the pivot engine's
//! rules (aggregations, spoiling errors, case-folded keys, filters).
//!
//! No reference engine has a body for the function, so the cases follow the function documentation:
//! `#REF!` for a reference outside any pivot table, for a field that is not shown and for data that
//! does not exist.
#![allow(clippy::unwrap_used)]

use easyoffice_sheet::{
    CellRange, EvaluatedCellValue as V, PivotAggregation, PivotFieldFilter, PivotTableDef,
    PivotValueDef, WorkbookEngine,
};

fn range(text: &str) -> CellRange {
    CellRange::parse(text).unwrap()
}

/// Sheet `Data` holds the rows, sheet `Report` holds a pivot table at A3:D6 (region by quarter, the
/// sum and the average of the sales).
fn workbook(extra: &[(&str, &str)]) -> WorkbookEngine {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Data");
    wb.add_sheet("Report");
    let rows = [
        ("Region", "Quarter", "Sales"),
        ("East", "Q1", "10"),
        ("east", "Q2", "20"),
        ("West", "Q1", "5"),
        ("West", "Q1", "7"),
        ("West", "Q2", "1"),
        ("North", "Q1", "100"),
    ];
    for (i, (a, b, c)) in rows.iter().enumerate() {
        let r = i + 1;
        wb.set_cell("Data", &format!("A{r}"), *a).unwrap();
        wb.set_cell("Data", &format!("B{r}"), *b).unwrap();
        wb.set_cell("Data", &format!("C{r}"), *c).unwrap();
    }
    wb.set_pivot_tables(
        "Report",
        vec![PivotTableDef {
            location: range("A3:D6"),
            source_sheet: "Data".to_string(),
            source: range("A1:C7"),
            has_header: true,
            row_fields: vec![0],
            col_fields: vec![1],
            values: vec![
                PivotValueDef {
                    column: 2,
                    aggregation: PivotAggregation::Sum,
                    caption: "합계: Sales".to_string(),
                },
                PivotValueDef {
                    column: 2,
                    aggregation: PivotAggregation::Average,
                    caption: "평균: Sales".to_string(),
                },
            ],
            // the report leaves the North region out
            filters: vec![PivotFieldFilter {
                col_offset: 0,
                allowed_values: vec!["East".to_string(), "West".to_string()],
                allow_blank: false,
            }],
        }],
    )
    .unwrap();
    for (cell, formula) in extra {
        wb.set_cell("Report", cell, *formula).unwrap();
    }
    wb
}

fn eval(formula: &str) -> V {
    let wb = workbook(&[("F1", formula)]);
    wb.evaluate_cell("Report", "F1")
}

fn num(n: f64) -> V {
    V::Number(n)
}

#[test]
fn getpivotdata_gives_totals_and_items() {
    // the grand total of a value field: its caption, or the header of its source column
    assert_eq!(eval("=GETPIVOTDATA(\"합계: Sales\",A3)"), num(43.0));
    assert_eq!(eval("=GETPIVOTDATA(\"Sales\",A3)"), num(43.0));
    // an item of a row field, in any letter case; the key is case-folded as the pivot engine's
    assert_eq!(
        eval("=GETPIVOTDATA(\"합계: Sales\",B4,\"Region\",\"East\")"),
        num(30.0)
    );
    assert_eq!(
        eval("=GETPIVOTDATA(\"합계: Sales\",B4,\"region\",\"EAST\")"),
        num(30.0)
    );
    assert_eq!(
        eval("=GETPIVOTDATA(\"평균: Sales\",A3,\"Region\",\"West\")"),
        V::Number(13.0 / 3.0)
    );
    // a field of each axis
    assert_eq!(
        eval("=GETPIVOTDATA(\"합계: Sales\",A3,\"Region\",\"West\",\"Quarter\",\"Q1\")"),
        num(12.0)
    );
    assert_eq!(
        eval("=GETPIVOTDATA(\"합계: Sales\",A3,\"Quarter\",\"Q2\")"),
        num(21.0)
    );
    // an item that is a cell's value
    let mut wb = workbook(&[("F1", "=GETPIVOTDATA(\"합계: Sales\",A3,\"Quarter\",H1)")]);
    wb.set_cell("Report", "H1", "Q1").unwrap();
    assert_eq!(wb.evaluate_cell("Report", "F1"), num(22.0));
    // any cell of the table will do, and a reference on another sheet
    wb.set_cell("Data", "F1", "=GETPIVOTDATA(\"Sales\",Report!D6)")
        .unwrap();
    assert_eq!(wb.evaluate_cell("Data", "F1"), num(43.0));
}

#[test]
fn getpivotdata_follows_the_source_when_it_changes() {
    let mut wb = workbook(&[("F1", "=GETPIVOTDATA(\"합계: Sales\",A3)")]);
    assert_eq!(wb.evaluate_cell("Report", "F1"), num(43.0));
    wb.set_cell("Data", "C2", "110").unwrap();
    assert_eq!(wb.evaluate_cell("Report", "F1"), num(143.0));
}

#[test]
fn getpivotdata_is_ref_error_where_the_documentation_says() {
    for formula in [
        // not in a pivot table
        "=GETPIVOTDATA(\"합계: Sales\",F9)",
        "=GETPIVOTDATA(\"합계: Sales\",Data!A1)",
        // a value field the table does not have
        "=GETPIVOTDATA(\"Profit\",A3)",
        // a field that is not on an axis of the table, or not a field at all
        "=GETPIVOTDATA(\"합계: Sales\",A3,\"Sales\",10)",
        "=GETPIVOTDATA(\"합계: Sales\",A3,\"Nope\",\"x\")",
        // an item no row has, an item the table's filter leaves out
        "=GETPIVOTDATA(\"합계: Sales\",A3,\"Region\",\"South\")",
        "=GETPIVOTDATA(\"합계: Sales\",A3,\"Region\",\"North\")",
        // a combination without data
        "=GETPIVOTDATA(\"합계: Sales\",A3,\"Region\",\"East\",\"Quarter\",\"Q9\")",
    ] {
        assert_eq!(
            eval(formula),
            V::Error(easyoffice_sheet::FormulaError::Ref),
            "{formula}"
        );
    }
    // pairs have to be complete
    for formula in [
        "=GETPIVOTDATA(\"합계: Sales\")",
        "=GETPIVOTDATA(\"합계: Sales\",A3,\"Region\")",
    ] {
        assert_eq!(
            eval(formula),
            V::Error(easyoffice_sheet::FormulaError::Value),
            "{formula}"
        );
    }
    assert_eq!(
        eval("=GETPIVOTDATA(#N/A,A3)"),
        V::Error(easyoffice_sheet::FormulaError::Na)
    );
}

#[test]
fn getpivotdata_without_registered_tables_is_ref_error() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("S");
    wb.set_cell("S", "A1", "Key").unwrap();
    wb.set_cell("S", "A2", "x").unwrap();
    wb.set_cell("S", "C1", "=GETPIVOTDATA(\"Sales\",A1)")
        .unwrap();
    assert_eq!(
        wb.evaluate_cell("S", "C1"),
        V::Error(easyoffice_sheet::FormulaError::Ref)
    );
}

#[test]
fn set_pivot_tables_checks_the_sheet_and_the_bound() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("S");
    let table = || PivotTableDef {
        location: range("A1:B2"),
        source_sheet: "S".to_string(),
        source: range("A1:B2"),
        has_header: true,
        row_fields: vec![0],
        col_fields: vec![],
        values: vec![],
        filters: vec![],
    };
    assert!(wb.set_pivot_tables("Missing", vec![table()]).is_err());
    assert!(wb.set_pivot_tables("S", vec![table(); 129]).is_err());
    assert!(wb.set_pivot_tables("S", vec![table(); 128]).is_ok());
    // setting a sheet's tables replaces its earlier ones
    assert!(wb.set_pivot_tables("s", vec![table(); 128]).is_ok());
}

#[test]
fn getpivotdata_spoils_like_the_pivot_engine_and_fails_closed_on_a_cycle() {
    let mut wb = workbook(&[("F1", "=GETPIVOTDATA(\"합계: Sales\",A3)")]);
    wb.set_cell("Data", "C3", "=1/0").unwrap();
    // an error value in the data spoils a sum, as in the pivot (PD-263)
    assert_eq!(
        wb.evaluate_cell("Report", "F1"),
        V::Error(easyoffice_sheet::FormulaError::DivByZero)
    );
    // a source that reads the formula's own cell is a cycle, not a hang
    let mut cyc = workbook(&[]);
    cyc.set_cell("Data", "C2", "=Data!F1").unwrap();
    cyc.set_cell("Data", "F1", "=GETPIVOTDATA(\"Sales\",Report!A3)")
        .unwrap();
    let v = cyc.evaluate_cell("Data", "F1");
    assert!(matches!(v, V::Error(_)), "{v:?}");
}
