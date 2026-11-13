//! Measurement harness for the defined names (PD-214), not run by default:
//! `cargo test -p easyoffice-sheet --release --test names_bench -- --ignored --nocapture`
//!
//! Three workloads of 100,000 formulas each read the same cell: written as a direct reference
//! (`=A1*$K$1`), written with one name (`=A1*Rate`), and written with its own name each (`=n_1*2`, 10,000
//! names). Every step is timed in milliseconds: building the workbook (compiling the formulas), opening a
//! session (dependency graph and first recalculation), one edit of the cell they read, and, for names, the
//! redefinition of a name.

use easyoffice_formats::DefinedName;
use easyoffice_sheet::{RecalcSession, WorkbookEngine};
use std::time::Instant;

const FORMULAS: u32 = 100_000;
const NAMES: u32 = 10_000;

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1e3
}

fn def(name: &str, refers_to: &str) -> DefinedName {
    DefinedName {
        name: name.to_string(),
        refers_to: refers_to.to_string(),
        ..DefinedName::default()
    }
}

fn build(formula: impl Fn(u32) -> String, names: &[DefinedName]) -> (WorkbookEngine, f64, f64) {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    let started = Instant::now();
    wb.set_names_lenient(names);
    let names_ms = ms(started);
    wb.set_cell("Sheet1", "K1", "3").unwrap();
    let started = Instant::now();
    for r in 1..=FORMULAS {
        wb.set_cell("Sheet1", &format!("A{r}"), r.to_string())
            .unwrap();
        wb.set_cell("Sheet1", &format!("B{r}"), formula(r)).unwrap();
    }
    (wb, names_ms, ms(started))
}

fn run(
    label: &str,
    formula: impl Fn(u32) -> String,
    names: &[DefinedName],
    redefine: Option<&[DefinedName]>,
) {
    let (wb, names_ms, build_ms) = build(formula, names);
    let started = Instant::now();
    let mut session = RecalcSession::new(wb).unwrap();
    let graph_ms = ms(started);
    let started = Instant::now();
    let first = session.recalculate();
    let first_ms = ms(started);
    let k1 = easyoffice_sheet::PackedCellCoordinate::parse_a1("K1").unwrap();
    session.set_cell("Sheet1", k1, "4").unwrap();
    let started = Instant::now();
    let edit = session.recalculate();
    let edit_ms = ms(started);
    let redefine_ms = redefine.map(|defs| {
        let started = Instant::now();
        session.set_names(defs).unwrap();
        let out = session.recalculate();
        (ms(started), out.recomputed)
    });
    println!(
        "{label}: names {names_ms:.1} | compile {build_ms:.1} | graph {graph_ms:.1} | first recalc {first_ms:.1} ({} cells) | edit K1 {edit_ms:.1} ({} cells){}",
        first.recomputed,
        edit.recomputed,
        redefine_ms.map_or(String::new(), |(t, n)| format!(" | redefine {t:.1} ({n} cells)")),
    );
}

#[test]
#[ignore = "measurement harness"]
fn names_bench() {
    run("direct   ", |r| format!("=A{r}*$K$1"), &[], None);
    let rate = [def("Rate_", "Sheet1!$K$1")];
    let rate_after = [def("Rate_", "Sheet1!$K$1+1")];
    run(
        "one name ",
        |r| format!("=A{r}*Rate_"),
        &rate,
        Some(&rate_after),
    );
    let many: Vec<DefinedName> = (0..NAMES)
        .map(|i| def(&format!("n_{i}"), &format!("Sheet1!$A${}", i + 1)))
        .collect();
    let mut changed = many.clone();
    changed[0] = def("n_0", "Sheet1!$A$2");
    run(
        "10k names",
        |r| format!("=n_{}*2", (r - 1) % NAMES),
        &many,
        Some(&changed),
    );
}
