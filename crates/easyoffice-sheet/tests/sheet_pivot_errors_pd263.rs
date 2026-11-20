//! PD-263: a pivot value field that holds an error shows that error in Sum, Average, Min and Max
//! (the cell, the row and column totals and the grand total it feeds), and Count counts it.
#![allow(clippy::unwrap_used)]

use easyoffice_sheet::{
    pivot_error_of, CellRange, DynamicPivotSpec, FormulaError, PivotAggregation, WorkbookEngine,
};
use std::time::Duration;

fn workbook() -> WorkbookEngine {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("S");
    for (r, (key, val)) in [("a", "10"), ("a", "=1/0"), ("b", "5"), ("b", "text")]
        .iter()
        .enumerate()
    {
        wb.set_cell("S", &format!("A{}", r + 2), *key).unwrap();
        wb.set_cell("S", &format!("B{}", r + 2), *val).unwrap();
    }
    wb.set_cell("S", "A1", "Key").unwrap();
    wb.set_cell("S", "B1", "Val").unwrap();
    wb
}

fn dynamic(wb: &WorkbookEngine, agg: PivotAggregation) -> easyoffice_sheet::DynamicPivotSummary {
    let spec = DynamicPivotSpec {
        range: CellRange::parse("A1:B5").unwrap(),
        header: true,
        row_fields: vec![0],
        col_fields: vec![],
        filter_fields: vec![],
        value_fields: vec![(1, agg)],
        filters: vec![],
    };
    wb.compute_dynamic_pivot_within("S", &spec, 1000, Duration::from_secs(10))
        .unwrap()
}

#[test]
fn error_values_show_as_errors_in_sum_average_min_max() {
    let wb = workbook();
    for agg in [
        PivotAggregation::Sum,
        PivotAggregation::Average,
        PivotAggregation::Min,
        PivotAggregation::Max,
    ] {
        let p = dynamic(&wb, agg);
        let key = |k: &str| p.row_tuples.iter().position(|t| t[0] == k).unwrap();
        let (a, b) = (key("a"), key("b"));
        assert_eq!(
            pivot_error_of(p.values_matrix[a][0][0]),
            Some(FormulaError::DivByZero),
            "{agg:?} a"
        );
        assert_eq!(
            pivot_error_of(p.row_totals[a][0]),
            Some(FormulaError::DivByZero),
            "{agg:?} total a"
        );
        assert_eq!(pivot_error_of(p.values_matrix[b][0][0]), None, "{agg:?} b");
        assert_eq!(
            pivot_error_of(p.grand_totals[0]),
            Some(FormulaError::DivByZero),
            "{agg:?} grand"
        );
    }
}

#[test]
fn count_counts_the_error_and_the_text() {
    let p = dynamic(&workbook(), PivotAggregation::Count);
    let key = |k: &str| p.row_tuples.iter().position(|t| t[0] == k).unwrap();
    assert_eq!(p.values_matrix[key("a")][0][0], 2.0);
    assert_eq!(p.values_matrix[key("b")][0][0], 2.0);
    assert_eq!(p.grand_totals[0], 4.0);
}

#[test]
fn the_single_value_pivot_reports_errors_too() {
    let wb = workbook();
    let p = wb.sheet("S").unwrap().compute_pivot_grid(
        &CellRange::parse("A1:B5").unwrap(),
        true,
        0,
        None,
        1,
        PivotAggregation::Sum,
    );
    let a = p.row_keys.iter().position(|k| k == "a").unwrap();
    assert_eq!(
        pivot_error_of(p.row_totals[a]),
        Some(FormulaError::DivByZero)
    );
    assert_eq!(pivot_error_of(p.grand_total), Some(FormulaError::DivByZero));
}
