//! Measurements of dynamic arrays (PD-217), release build:
//! `cargo test -p easyoffice-sheet --release --test sheet_dynamic_bench -- --ignored --nocapture --test-threads=1`
#![allow(clippy::unwrap_used)]

use easyoffice_sheet::{CellChange, PackedCellCoordinate, RecalcSession, WorkbookEngine};
use std::time::Instant;

const ROWS: u32 = 100_000;

fn data_workbook(formulas: &[(&str, &str)]) -> WorkbookEngine {
    let mut wb = WorkbookEngine::new();
    let sheet = wb.add_sheet("Sheet1");
    for r in 0..ROWS {
        let n = (u64::from(r) * 2_654_435_761 % 1_000_003) as f64;
        sheet.set_cell_coord(PackedCellCoordinate::new(r, 0).unwrap(), format!("{n}"));
        sheet.set_cell_coord(
            PackedCellCoordinate::new(r, 1).unwrap(),
            format!("k{}", r % 100),
        );
        sheet.set_cell_coord(
            PackedCellCoordinate::new(r, 2).unwrap(),
            format!("{}", r % 997),
        );
    }
    for (a1, f) in formulas {
        sheet.set_cell_coord(
            PackedCellCoordinate::parse_a1(a1).unwrap(),
            (*f).to_string(),
        );
    }
    wb
}

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

fn first_calculation(label: &str, formula: &str) {
    let wb = data_workbook(&[("E1", formula)]);
    let mut best = f64::MAX;
    let mut cells = 0;
    for _ in 0..3 {
        let mut session = RecalcSession::new(wb.clone()).unwrap();
        let started = Instant::now();
        let outcome = session.recalculate();
        best = best.min(ms(started));
        cells = outcome.changed.len();
    }
    println!("{label}: {best:.1} ms (open + first calculation, {cells} cells reported)");
}

#[test]
#[ignore = "measurement"]
fn one_hundred_thousand_rows() {
    first_calculation(
        "FILTER 100,000 rows x 3, about half kept",
        "=FILTER(A1:C100000, A1:A100000>500000)",
    );
    first_calculation("SORT 100,000 rows x 3 by column 1", "=SORT(A1:C100000, 1)");
    first_calculation(
        "SORT 100,000 rows x 3 by column 2 descending",
        "=SORT(A1:C100000, 2, -1)",
    );
    first_calculation(
        "UNIQUE 100,000 values (100 distinct)",
        "=UNIQUE(B1:B100000)",
    );
    first_calculation("UNIQUE 100,000 rows x 3", "=UNIQUE(A1:C100000)");
    first_calculation(
        "SORTBY 100,000 rows",
        "=SORTBY(A1:C100000, C1:C100000, -1, A1:A100000, 1)",
    );
    first_calculation("SEQUENCE 100,000", "=SEQUENCE(100000)");
}

#[test]
#[ignore = "measurement"]
fn ten_thousand_cells_spilled_and_recalculated() {
    let mut wb = WorkbookEngine::new();
    let sheet = wb.add_sheet("Sheet1");
    sheet.set_cell_coord(
        PackedCellCoordinate::parse_a1("A1").unwrap(),
        "10000".to_string(),
    );
    sheet.set_cell_coord(
        PackedCellCoordinate::parse_a1("B1").unwrap(),
        "=SEQUENCE(A1)".to_string(),
    );
    sheet.set_cell_coord(
        PackedCellCoordinate::parse_a1("D1").unwrap(),
        "=SUM(B1#)".to_string(),
    );
    sheet.set_cell_coord(
        PackedCellCoordinate::parse_a1("D2").unwrap(),
        "=SUM(B1:B10000)".to_string(),
    );
    sheet.set_cell_coord(
        PackedCellCoordinate::parse_a1("D3").unwrap(),
        "=B5000+1".to_string(),
    );
    let mut session = RecalcSession::new(wb).unwrap();
    let started = Instant::now();
    session.recalculate();
    println!(
        "open + first calculation of a 10,000 cell spill with three readers: {:.2} ms",
        ms(started)
    );
    let mut times = Vec::new();
    for i in 0..40 {
        let size = if i % 2 == 0 { 9_999 } else { 10_000 };
        session
            .apply(&[CellChange {
                sheet: "Sheet1".to_string(),
                coord: PackedCellCoordinate::parse_a1("A1").unwrap(),
                raw: size.to_string(),
            }])
            .unwrap();
        let started = Instant::now();
        let outcome = session.recalculate();
        times.push(ms(started));
        assert!(!outcome.timed_out);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!(
        "recalculate after the array grows or shrinks by one cell (10,000 cells, 3 readers): min {:.2} ms, median {:.2} ms, max {:.2} ms",
        times[0],
        times[times.len() / 2],
        times[times.len() - 1]
    );
    // an edit that changes every value of the array
    let mut sheet_times = Vec::new();
    for i in 0..40 {
        session
            .apply(&[CellChange {
                sheet: "Sheet1".to_string(),
                coord: PackedCellCoordinate::parse_a1("B1").unwrap(),
                raw: format!("=SEQUENCE(A1, 1, {i})"),
            }])
            .unwrap();
        let started = Instant::now();
        session.recalculate();
        sheet_times.push(ms(started));
    }
    sheet_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!(
        "recalculate after every value of the 10,000 cell array changes: min {:.2} ms, median {:.2} ms",
        sheet_times[0],
        sheet_times[sheet_times.len() / 2]
    );
}

#[test]
#[ignore = "measurement"]
fn the_ordinary_path_is_not_slower() {
    // A chain of 100,003 formulas, as in PD-215: session open and first calculation, then one edit at the head.
    let mut wb = WorkbookEngine::new();
    let sheet = wb.add_sheet("Sheet1");
    sheet.set_cell_coord(PackedCellCoordinate::new(0, 0).unwrap(), "1".to_string());
    for r in 1..100_003u32 {
        sheet.set_cell_coord(PackedCellCoordinate::new(r, 0).unwrap(), format!("=A{r}+1"));
    }
    let mut best = f64::MAX;
    for _ in 0..5 {
        let mut session = RecalcSession::new(wb.clone()).unwrap();
        let started = Instant::now();
        session.recalculate();
        best = best.min(ms(started));
    }
    println!("100,003 formula chain, session open + first calculation (best of 5): {best:.1} ms");
}
