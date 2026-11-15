//! Measurement harness for the hidden rows (PD-232), not run by default:
//! `cargo test -p easyoffice-sheet --release --test sheet_hidden_rows_bench -- --ignored --nocapture`
//!
//! A sheet of 100,000 rows with a number in column A, a formula in column B (`=A2*2`, 100,000 of them) and 20
//! `SUBTOTAL(109, ...)` formulas over column A. The hidden rows change (every other row, one run of 50,000 rows, none) and the
//! time of the answer is printed together with the number of formulas that were calculated.

use easyoffice_sheet::{PackedCellCoordinate, RecalcSession, WorkbookEngine};
use std::time::Instant;

const ROWS: u32 = 100_000;
const SUBTOTALS: u32 = 20;

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1e3
}

#[test]
#[ignore = "measurement"]
fn the_cost_of_a_change_of_the_hidden_rows() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("S");
    for r in 1..=ROWS {
        wb.set_cell("S", &format!("A{r}"), r.to_string()).unwrap();
        wb.set_cell("S", &format!("B{r}"), format!("=A{r}*2"))
            .unwrap();
    }
    for k in 0..SUBTOTALS {
        wb.set_cell(
            "S",
            &format!("D{}", k + 1),
            format!("=SUBTOTAL(109,A1:A{ROWS})"),
        )
        .unwrap();
    }
    let start = Instant::now();
    let mut session = RecalcSession::new(wb).unwrap();
    let first = session.recalculate();
    println!(
        "open: {:.1} ms, {} formulas calculated",
        ms(start),
        first.recomputed
    );
    let coord = PackedCellCoordinate::parse_a1("D1").unwrap();
    let alternate: Vec<(u32, u32)> = (0..ROWS / 2).map(|i| (2 * i, 2 * i)).collect();
    for (label, hidden, filtered) in [
        ("every other row filtered", vec![], alternate.clone()),
        ("every other row hidden by the user", alternate, vec![]),
        ("one run of 50,000 rows filtered", vec![], vec![(0, 49_999)]),
        ("none", vec![], vec![]),
        ("none again", vec![], vec![]),
    ] {
        let start = Instant::now();
        session
            .set_sheet_hidden_rows("S", hidden, filtered)
            .unwrap();
        let outcome = session.recalculate();
        println!(
            "{label}: {:.1} ms, {} formulas calculated, D1 = {}",
            ms(start),
            outcome.recomputed,
            session.cell_value("S", coord).to_display_string()
        );
    }
}
