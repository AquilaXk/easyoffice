//! Measurement harness (not run by default):
//! `cargo test -p easyoffice-sheet --release -- --ignored --nocapture recalc_bench`
//!
//! Compares what an edit costs on the full path (rebuild the workbook from every cell and
//! evaluate every formula, as the stateless `evaluate_sheet` command does) with the session path.

use super::*;
use crate::WorkbookEvalTarget;
use std::time::Instant;

fn raw_cells(rows: u32) -> Vec<(String, String)> {
    let mut cells = Vec::with_capacity(rows as usize * 4 + 1);
    for r in 1..=rows {
        cells.push((format!("A{r}"), r.to_string()));
        cells.push((format!("B{r}"), format!("=A{r}*2")));
        cells.push((format!("C{r}"), format!("=B{r}+A{r}")));
        let prev = if r == 1 {
            "0".to_string()
        } else {
            format!("D{}", r - 1)
        };
        cells.push((format!("D{r}"), format!("={prev}+C{r}")));
    }
    cells.push(("F1".into(), format!("=SUM(C1:C{rows})")));
    cells
}

fn build(cells: &[(String, String)]) -> WorkbookEngine {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    for (a1, raw) in cells {
        wb.set_cell("Sheet1", a1, raw.as_str()).unwrap();
    }
    wb
}

fn full_path(cells: &[(String, String)]) -> (f64, usize) {
    let start = Instant::now();
    let wb = build(cells);
    let addrs: Vec<&str> = cells
        .iter()
        .filter(|(_, raw)| raw.starts_with('='))
        .map(|(a1, _)| a1.as_str())
        .collect();
    let targets: Vec<WorkbookEvalTarget<'_>> = addrs
        .iter()
        .map(|&expr| WorkbookEvalTarget {
            sheet: "Sheet1",
            expr,
            anchor: None,
        })
        .collect();
    let batch = wb.evaluate_targets(&targets);
    (start.elapsed().as_secs_f64() * 1e3, batch.values.len())
}

#[test]
#[ignore = "measurement harness"]
fn recalc_bench() {
    for rows in [3_334u32, 33_334] {
        let cells = raw_cells(rows);
        let (full_ms, formulas) = full_path(&cells);
        let (full_ms2, _) = full_path(&cells);
        let full_ms = full_ms.min(full_ms2);

        let t = Instant::now();
        let mut session = RecalcSession::new(build(&cells)).unwrap();
        let open_first = session.recalculate();
        let open_ms = t.elapsed().as_secs_f64() * 1e3;

        // Leaf edit: touches one B, one C, one D chain tail from that row down, and the SUM.
        let report = |label: &str, row: u32, session: &mut RecalcSession| {
            let mut best = f64::MAX;
            let mut last = None;
            for round in 0..5 {
                let start = Instant::now();
                session
                    .set_cell(
                        "Sheet1",
                        PackedCellCoordinate::parse_a1(&format!("A{row}")).unwrap(),
                        &(1_000 + round).to_string(),
                    )
                    .unwrap();
                let out = session.recalculate();
                best = best.min(start.elapsed().as_secs_f64() * 1e6);
                last = Some((out.recomputed, out.changed.len()));
            }
            let (recomputed, changed) = last.unwrap();
            println!(
                "  {label}: edit A{row} -> {best:.0} us, recomputed {recomputed}, reported {changed}"
            );
        };
        println!(
            "formulas={formulas} cells={} | full path (rebuild+evaluate) {full_ms:.1} ms | session open+first recalc {open_ms:.1} ms ({} values)",
            cells.len(),
            open_first.changed.len()
        );
        report("tail edit (few dependents)", rows, &mut session);
        report("middle edit (half the D chain)", rows / 2, &mut session);
        report("head edit (whole D chain)", 1, &mut session);
    }
}

/// Graph memory under the crowded-dependents pattern (33 formulas per group read the same 40
/// cells): run with `/usr/bin/time -l` to see the peak RSS next to the estimate.
#[test]
#[ignore = "measurement harness"]
fn graph_memory_bench() {
    let formulas: u32 = std::env::var("BENCH_FORMULAS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000);
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("S");
    let raw: usize = (0..formulas)
        .map(|i| {
            let text = super::tests::bundled_formula(i);
            let len = text.len();
            wb.set_cell("S", &format!("A{}", i + 1), text).unwrap();
            len
        })
        .sum();
    let start = Instant::now();
    let mut session = RecalcSession::new(wb).unwrap();
    session.recalculate();
    let f = session.footprint();
    println!(
        "formulas={formulas} raw={} MB edges={} graph_estimate={} MB open+recalc={:.0} ms",
        raw >> 20,
        f.edges,
        f.graph_bytes >> 20,
        start.elapsed().as_secs_f64() * 1e3
    );
}

/// Row insertion on a sheet of 100,000 rows and 100,000 formulas (`cargo test -p easyoffice-sheet
/// --release --lib -- --ignored --nocapture axis_edit_bench`): the edit alone (cells move,
/// formula text and compiled formulas follow) and the whole session call (graph rebuilt, every
/// formula recalculated, changed values listed).
#[test]
#[ignore = "measurement harness"]
fn axis_edit_bench() {
    use crate::axis::{Axis, AxisBudget, AxisEdit, EditKind};
    use std::time::Duration;
    let rows = 100_000u32;
    let build_sheet = || {
        let mut wb = WorkbookEngine::new();
        wb.add_sheet("Sheet1");
        wb.add_sheet("Other");
        for r in 1..=rows {
            wb.set_cell("Sheet1", &format!("A{r}"), r.to_string())
                .unwrap();
            wb.set_cell("Sheet1", &format!("B{r}"), format!("=A{r}*2"))
                .unwrap();
        }
        wb.set_cell("Sheet1", "D1", format!("=SUM(B1:B{rows})"))
            .unwrap();
        wb.set_cell("Other", "A1", format!("=Sheet1!A{rows}+SUM(Sheet1!A:A)"))
            .unwrap();
        wb
    };
    for (label, at_row) in [
        ("row 1 (every formula moves)", 1u32),
        ("row 50,000", 50_000),
    ] {
        let edit = AxisEdit::new(Axis::Row, EditKind::Insert, at_row - 1, 1).unwrap();
        let mut wb = build_sheet();
        let t = Instant::now();
        let report = wb
            .apply_axis_edit("Sheet1", edit, AxisBudget::within(Duration::from_secs(30)))
            .unwrap();
        let engine_ms = t.elapsed().as_secs_f64() * 1e3;

        let mut session = RecalcSession::new(build_sheet()).unwrap();
        session.recalculate();
        let t = Instant::now();
        let (_, outcome) = session
            .apply_axis_edit("Sheet1", edit, AxisBudget::within(Duration::from_secs(30)))
            .unwrap();
        let session_ms = t.elapsed().as_secs_f64() * 1e3;
        println!(
            "insert 1 row at {label}: formulas={} rewritten={} | edit alone {engine_ms:.1} ms | session call (edit + graph rebuild + recalc) {session_ms:.1} ms, changed values {}",
            rows + 2,
            report.rewritten.len(),
            outcome.changed.len()
        );
    }
}
