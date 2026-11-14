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

#[test]
#[ignore = "measurement"]
fn readers_through_indirect_and_offset() {
    // 50,000 formulas that compute their reference (PD-222): session open and first calculation, then the calculation after a
    // one-cell edit (they are volatile, so all of them are calculated again), best of 5.
    let mut wb = data_workbook(&[]);
    let sheet = wb.sheet_mut("Sheet1").unwrap();
    for r in 0..50_000u32 {
        let f = if r % 2 == 0 {
            format!("=INDIRECT(\"A\"&{})", r + 1)
        } else {
            format!("=OFFSET(A1,{r},0)")
        };
        sheet.set_cell_coord(PackedCellCoordinate::new(r, 4).unwrap(), f);
    }
    let (mut first, mut again) = (f64::MAX, f64::MAX);
    for _ in 0..5 {
        let mut session = RecalcSession::new(wb.clone()).unwrap();
        let started = Instant::now();
        session.recalculate();
        first = first.min(ms(started));
        session
            .set_cell("Sheet1", PackedCellCoordinate::parse_a1("A1").unwrap(), "7")
            .unwrap();
        let started = Instant::now();
        session.recalculate();
        again = again.min(ms(started));
    }
    println!("50,000 INDIRECT/OFFSET readers: first calculation {first:.1} ms, after an edit {again:.1} ms");
}

#[test]
#[ignore = "measurement"]
fn arrays_beside_many_merged_ranges() {
    // 10,000 arrays of two cells in column A and 100,000 merged ranges in columns E:F (PD-222): session open and first
    // calculation, best of 5, without and with the merges.
    let build = |merged: bool| {
        let mut session_wb = WorkbookEngine::new();
        let sheet = session_wb.add_sheet("Sheet1");
        for k in 0..10_000u32 {
            sheet.set_cell_coord(
                PackedCellCoordinate::new(k * 3, 0).unwrap(),
                "=SEQUENCE(2)".to_string(),
            );
        }
        if merged {
            let boxes: Vec<easyoffice_formats::CellBox> = (0..100_000u32)
                .map(|r| {
                    easyoffice_formats::CellBox::parse(&format!("E{}:F{}", r + 1, r + 1)).unwrap()
                })
                .collect();
            sheet.set_spill_merges(boxes);
        }
        session_wb
    };
    for merged in [false, true] {
        let wb = build(merged);
        let mut best = f64::MAX;
        for _ in 0..5 {
            let mut session = RecalcSession::new(wb.clone()).unwrap();
            let started = Instant::now();
            let outcome = session.recalculate();
            best = best.min(ms(started));
            assert_eq!(outcome.spills.len(), 10_000);
        }
        println!(
            "10,000 arrays, {} merged ranges: {best:.1} ms",
            if merged { "100,000" } else { "no" }
        );
    }
}

#[test]
#[ignore = "measurement"]
fn arrays_beside_merges_with_one_as_tall_as_the_sheet() {
    // 50,000 arrays of two cells, 100,000 small merged ranges and one merged range the height of the sheet.
    let mut wb = WorkbookEngine::new();
    let sheet = wb.add_sheet("Sheet1");
    for k in 0..50_000u32 {
        sheet.set_cell_coord(
            PackedCellCoordinate::new(k * 3, 0).unwrap(),
            "=SEQUENCE(2)".to_string(),
        );
    }
    let mut boxes: Vec<easyoffice_formats::CellBox> = (0..100_000u32)
        .map(|r| easyoffice_formats::CellBox::parse(&format!("E{}:F{}", r + 1, r + 1)).unwrap())
        .collect();
    boxes.push(easyoffice_formats::CellBox::parse("Z1:Z1048576").unwrap());
    sheet.set_spill_merges(boxes);
    let mut best = f64::MAX;
    for _ in 0..3 {
        let mut session = RecalcSession::new(wb.clone()).unwrap();
        let started = Instant::now();
        let outcome = session.recalculate();
        best = best.min(ms(started));
        assert_eq!(outcome.spills.len(), 50_000);
    }
    println!("50,000 arrays, 100,000 small merges and one as tall as the sheet: {best:.1} ms");
}
