//! Pivot limits (PD-171): every cell read has its own step budget, the answer's size is checked while
//! the groups are collected, and the pass stops at its deadline.
#![allow(clippy::unwrap_used)]

use easyoffice_sheet::{
    CellRange, PivotAggregation, PivotError, PivotSpec, SheetEngine, WorkbookEngine,
};
use std::time::Duration;

fn spec(range: &str, col: Option<usize>, value: usize) -> PivotSpec {
    PivotSpec {
        range: CellRange::parse(range).unwrap(),
        header: false,
        row: 0,
        col,
        value,
        agg: PivotAggregation::Sum,
    }
}

/// `rows` rows of `A` = key, `B` = `=SUM(D1:D400)`; `D1:D400` are ones.
fn heavy_sheet(rows: u32) -> WorkbookEngine {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("S");
    for r in 1..=400 {
        wb.set_cell("S", &format!("D{r}"), "1").unwrap();
    }
    for r in 1..=rows {
        wb.set_cell("S", &format!("A{r}"), "k").unwrap();
        wb.set_cell("S", &format!("B{r}"), "=SUM(D1:D400)").unwrap();
    }
    wb
}

#[test]
fn each_value_cell_starts_with_its_own_step_budget() {
    // Together the cells need more steps than one cell may take.
    let rows = 8_000;
    let wb = heavy_sheet(rows);
    let pivot = wb
        .compute_pivot_within(
            "S",
            &spec("A1:B8000", None, 1),
            1000,
            Duration::from_secs(60),
        )
        .unwrap();
    assert_eq!(pivot.grand_total, f64::from(rows) * 400.0);
    let alone: &SheetEngine = wb.sheet("S").unwrap();
    let pivot = alone.compute_pivot_grid(
        &CellRange::parse("A1:B8000").unwrap(),
        false,
        0,
        None,
        1,
        PivotAggregation::Sum,
    );
    assert_eq!(pivot.grand_total, f64::from(rows) * 400.0);
}

#[test]
fn the_size_of_the_answer_is_refused_before_a_grid_is_built() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("S");
    for r in 1..=3000u32 {
        wb.set_cell("S", &format!("A{r}"), format!("r{r}")).unwrap();
        wb.set_cell("S", &format!("B{r}"), format!("c{r}")).unwrap();
        wb.set_cell("S", &format!("C{r}"), "1").unwrap();
    }
    let err = wb
        .compute_pivot_within(
            "S",
            &spec("A1:C3000", Some(1), 2),
            250_000,
            Duration::from_secs(60),
        )
        .unwrap_err();
    assert_eq!(err, PivotError::TooLarge);
}

#[test]
fn a_pass_past_its_deadline_is_an_error_not_a_partial_answer() {
    let wb = heavy_sheet(2000);
    let err = wb
        .compute_pivot_within("S", &spec("A1:B2000", None, 1), 1000, Duration::ZERO)
        .unwrap_err();
    assert_eq!(err, PivotError::TimedOut);
    assert_eq!(
        wb.compute_pivot_within("None", &spec("A1:B2", None, 1), 10, Duration::from_secs(1))
            .unwrap_err(),
        PivotError::NoSheet
    );
}
