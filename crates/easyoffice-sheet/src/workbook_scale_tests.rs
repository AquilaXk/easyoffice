//! A workbook whose sheets each read the sheet before it: values stay right and the cost of
//! evaluating the last sheet grows in step with the number of formulas it depends on.

use super::{WorkbookEngine, WorkbookEvalTarget};
use crate::EvaluatedCellValue;
use std::time::Instant;

const SHEETS: usize = 40;
const ROWS: usize = 200;
/// Twice the formulas: linear work is about 2x, a quadratic pass about 4x.
const MAX_RATIO: f64 = 2.7;

/// `S0!A{r} = r`, `S{k}!A{r} = S{k-1}!A{r} + 1`: every formula names its sheet in its own string.
fn chain_workbook(sheets: usize, rows: usize) -> WorkbookEngine {
    let mut wb = WorkbookEngine::new();
    for k in 0..sheets {
        let name = format!("S{k}");
        wb.add_sheet(name.as_str());
        for r in 1..=rows {
            let raw = if k == 0 {
                r.to_string()
            } else {
                format!("=S{}!A{r}+1", k - 1)
            };
            wb.set_cell(&name, &format!("A{r}"), raw).unwrap();
        }
    }
    wb
}

fn evaluate_sheet_cells(wb: &WorkbookEngine, sheet: &str, rows: usize) -> Vec<EvaluatedCellValue> {
    let addrs: Vec<String> = (1..=rows).map(|r| format!("A{r}")).collect();
    let targets: Vec<WorkbookEvalTarget<'_>> = addrs
        .iter()
        .map(|a1| WorkbookEvalTarget {
            sheet,
            expr: a1.as_str(),
            anchor: None,
        })
        .collect();
    wb.evaluate_targets(&targets).values
}

fn best_of(runs: usize, mut work: impl FnMut()) -> f64 {
    (0..runs)
        .map(|_| {
            let started = Instant::now();
            work();
            started.elapsed().as_secs_f64()
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn a_chain_of_sheets_evaluates_to_the_value_each_step_adds() {
    let rows = 25;
    let wb = chain_workbook(SHEETS, rows);
    for k in [1, SHEETS / 2, SHEETS - 1] {
        let values = evaluate_sheet_cells(&wb, &format!("S{k}"), rows);
        for (i, value) in values.iter().enumerate() {
            let want = (i + 1 + k) as f64;
            assert_eq!(*value, EvaluatedCellValue::Number(want), "S{k}!A{}", i + 1);
        }
    }
}

#[test]
fn sheet_names_resolve_by_identity_not_by_spelling_or_quotes() {
    let mut wb = chain_workbook(3, 2);
    wb.set_cell("S2", "B1", "=s1!A1+'S0'!A2+S1!A1").unwrap();
    wb.set_cell("S2", "B2", "=SUM(s0!A1:A2,'s1'!A1:A2)")
        .unwrap();
    let got = wb.evaluate_targets(&[
        WorkbookEvalTarget {
            sheet: "S2",
            expr: "B1",
            anchor: None,
        },
        WorkbookEvalTarget {
            sheet: "S2",
            expr: "B2",
            anchor: None,
        },
    ]);
    // (A1 of S1 = 2) + (A2 of S0 = 2) + 2, and (1 + 2) + (2 + 3).
    assert_eq!(got.values[0], EvaluatedCellValue::Number(6.0));
    assert_eq!(got.values[1], EvaluatedCellValue::Number(8.0));
}

#[test]
fn evaluating_a_sheet_chain_scales_with_the_formula_count() {
    let small = chain_workbook(SHEETS, ROWS);
    let large = chain_workbook(SHEETS, 2 * ROWS);
    let t_small = best_of(5, || {
        evaluate_sheet_cells(&small, &format!("S{}", SHEETS - 1), ROWS);
    });
    let t_large = best_of(5, || {
        evaluate_sheet_cells(&large, &format!("S{}", SHEETS - 1), 2 * ROWS);
    });
    assert!(
        t_large < t_small * MAX_RATIO,
        "2x the rows took {:.1}x the time ({:.1} ms -> {:.1} ms)",
        t_large / t_small,
        t_small * 1e3,
        t_large * 1e3
    );
}

/// Every formula cell of the workbook as `(sheet, cell)`, sheet by sheet.
fn formula_cells(wb: &WorkbookEngine) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for sheet in wb.sheet_names() {
        let mut cells: Vec<_> = wb
            .sheet(sheet)
            .unwrap()
            .cells
            .iter()
            .filter(|(_, raw)| raw.starts_with('='))
            .map(|(coord, _)| coord.to_a1())
            .collect();
        cells.sort();
        out.extend(cells.into_iter().map(|a1| (sheet.to_string(), a1)));
    }
    out
}

fn evaluate_together(wb: &WorkbookEngine, cells: &[(String, String)]) -> Vec<EvaluatedCellValue> {
    let targets: Vec<WorkbookEvalTarget<'_>> = cells
        .iter()
        .map(|(sheet, a1)| WorkbookEvalTarget {
            sheet,
            expr: a1,
            anchor: None,
        })
        .collect();
    wb.evaluate_targets(&targets).values
}

/// One fresh evaluation per sheet, the way a save evaluated a workbook before it shared one
/// evaluation across sheets.
fn evaluate_sheet_by_sheet(
    wb: &WorkbookEngine,
    cells: &[(String, String)],
) -> Vec<EvaluatedCellValue> {
    let mut out = Vec::with_capacity(cells.len());
    for sheet in wb.sheet_names() {
        let own: Vec<_> = cells.iter().filter(|(s, _)| s == sheet).cloned().collect();
        out.extend(evaluate_together(wb, &own));
    }
    out
}

#[test]
fn one_shared_evaluation_gives_the_values_of_one_evaluation_per_sheet() {
    let mut wb = chain_workbook(6, 8);
    let extra = [
        ("S2", "C1", "=SUM(S1!A1:A8)+S0!A3"),
        ("S3", "C1", "=S2!C1+1"),
        ("S4", "C1", "=S2!C1*2+S3!A2"),
        ("S5", "C1", "=IF(S4!C1>10,\"big\",\"small\")&S1!A1"),
        ("S5", "C2", "=S0!Z9/0"),
        ("S5", "C3", "=NOSUCH!A1+1"),
        // A cycle through two sheets, and a formula that reads it.
        ("S1", "D1", "=S2!D1+1"),
        ("S2", "D1", "=S1!D1+1"),
        ("S3", "D1", "=S2!D1+S1!A1"),
        // A cycle inside one sheet and one that reaches itself.
        ("S4", "D1", "=D2"),
        ("S4", "D2", "=D1"),
        ("S4", "D3", "=D3"),
    ];
    for (sheet, cell, raw) in extra {
        wb.set_cell(sheet, cell, raw).unwrap();
    }
    let cells = formula_cells(&wb);
    assert!(cells.len() > 40);
    let together = evaluate_together(&wb, &cells);
    let apart = evaluate_sheet_by_sheet(&wb, &cells);
    for ((sheet, a1), (t, a)) in cells.iter().zip(together.iter().zip(&apart)) {
        assert_eq!(t, a, "{sheet}!{a1}");
    }
}
