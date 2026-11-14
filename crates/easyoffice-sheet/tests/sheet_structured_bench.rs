//! Measurement harness for the structured references (PD-225), not run by default:
//! `cargo test -p easyoffice-sheet --release --test sheet_structured_bench -- --ignored --nocapture`
//!
//! A table of 11 columns and 50,000 data rows. Its last column is calculated from two others on every row
//! (`=[@c1]+[@c2]` against `=A2+B2`) and 200 formulas sum a column (`=SUM(T[c3])` against `=SUM($C$2:$C$50001)`).
//! Every step is timed in milliseconds: compiling the cells, opening a session (dependency graph and first
//! recalculation), one edit of a cell the formulas read, the table growing by a row (the formulas that name it are
//! compiled again) and a row inserted inside it.

use easyoffice_formats::{CellBox, SheetTable};
use easyoffice_sheet::{
    Axis, AxisBudget, AxisEdit, EditKind, PackedCellCoordinate, RecalcSession, WorkbookEngine,
};
use std::time::Instant;

const ROWS: u32 = 50_000;
const SUMS: u32 = 200;

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1e3
}

fn table(rows: u32) -> SheetTable {
    let mut t = SheetTable::new("T", CellBox::parse(&format!("A1:K{}", rows + 1)).unwrap());
    t.columns = (1..=11).map(|i| format!("c{i}")).collect();
    t
}

fn build(structured: bool) -> WorkbookEngine {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    wb.set_tables("Sheet1", vec![table(ROWS)]).unwrap();
    for c in 0..11u8 {
        let col = (b'A' + c) as char;
        wb.set_cell("Sheet1", &format!("{col}1"), format!("c{}", c + 1))
            .unwrap();
    }
    for r in 2..=ROWS + 1 {
        for c in 0..10u8 {
            let col = (b'A' + c) as char;
            wb.set_cell(
                "Sheet1",
                &format!("{col}{r}"),
                (r + u32::from(c)).to_string(),
            )
            .unwrap();
        }
        let formula = if structured {
            "=[@c1]+[@c2]".to_string()
        } else {
            format!("=A{r}+B{r}")
        };
        wb.set_cell("Sheet1", &format!("K{r}"), formula).unwrap();
    }
    for i in 0..SUMS {
        let formula = if structured {
            "=SUM(T[c3])".to_string()
        } else {
            format!("=SUM($C$2:$C${})", ROWS + 1)
        };
        wb.set_cell("Sheet1", &format!("N{}", i + 1), formula)
            .unwrap();
    }
    wb
}

fn run(label: &str, structured: bool) {
    let started = Instant::now();
    let wb = build(structured);
    let build_ms = ms(started);
    let started = Instant::now();
    let mut session = RecalcSession::new(wb).unwrap();
    let graph_ms = ms(started);
    let started = Instant::now();
    let first = session.recalculate();
    let first_ms = ms(started);
    let a2 = PackedCellCoordinate::parse_a1("A2").unwrap();
    let c9 = PackedCellCoordinate::parse_a1("C9").unwrap();
    session.set_cell("Sheet1", a2, "7").unwrap();
    session.set_cell("Sheet1", c9, "7").unwrap();
    let started = Instant::now();
    let edit = session.recalculate();
    let edit_ms = ms(started);
    let started = Instant::now();
    session
        .set_sheet_table_defs("Sheet1", vec![table(ROWS + 1)])
        .unwrap();
    let grow = session.recalculate();
    let grow_ms = ms(started);
    let started = Instant::now();
    let insert = AxisEdit::new(Axis::Row, EditKind::Insert, 4, 1).unwrap();
    session
        .apply_axis_edit("Sheet1", insert, AxisBudget::default())
        .unwrap();
    let insert_ms = ms(started);
    println!(
        "{label}: compile {build_ms:.1} | open {graph_ms:.1} | first recalc {first_ms:.1} ({} cells) | edit {edit_ms:.1} ({} cells) | table grows {grow_ms:.1} ({} cells) | insert row {insert_ms:.1}",
        first.recomputed, edit.recomputed, grow.recomputed
    );
}

#[test]
#[ignore = "measurement harness"]
fn structured_bench() {
    for _ in 0..3 {
        run("plain     ", false);
        run("structured", true);
    }
}
