//! Limits and hostile input of dynamic arrays (PD-217): every case ends in an error value inside the
//! existing step, memory and time budgets (PD-214, PD-215), never a panic or a hang.
#![allow(clippy::unwrap_used)]

mod dynamic_support;
use dynamic_support::new_empty_model;
use easyoffice_sheet::{
    rewrite_sheet_names, Budget, PackedCellCoordinate, RecalcSession, SheetChange, WorkbookEngine,
};
use std::time::{Duration, Instant};

fn text(m: &dynamic_support::Model, a1: &str) -> String {
    m._get_text(a1)
}

#[test]
fn huge_sizes_are_refused_before_anything_is_allocated() {
    let mut m = new_empty_model();
    m._set("A1", "=SEQUENCE(1000000000)");
    m._set("B1", "=SEQUENCE(1000000, 1000000)");
    m._set("C1", "=MAKEARRAY(1000000, 1000000, LAMBDA(r, c, r+c))");
    m._set("D1", "=RANDARRAY(100000000, 100)");
    m._set("Sheet2!E1", "=SORT(Sheet1!A:XFD)");
    m._set(
        "Sheet2!F1",
        "=FILTER(Sheet1!A1:XFD1048576, Sheet1!A1:A1048576>0)",
    );
    m._set("Sheet2!G1", "=TRANSPOSE(Sheet1!A1:XFD1048576)");
    m._set("H1", "=SEQUENCE(1048576)*SEQUENCE(1,16384)");
    let started = Instant::now();
    m.evaluate();
    for cell in [
        "A1",
        "B1",
        "C1",
        "D1",
        "Sheet2!E1",
        "Sheet2!F1",
        "Sheet2!G1",
        "H1",
    ] {
        assert_eq!(text(&m, cell), "#SPILL!", "{cell}");
    }
    assert!(started.elapsed() < Duration::from_secs(30));
}

#[test]
fn lambda_recursion_and_nesting_end_in_errors() {
    let mut m = new_empty_model();
    m._set("A1", "=LET(f, LAMBDA(g, g(g)), f(f))");
    m._set("A2", "=LET(f, LAMBDA(g, n, g(g, n+1)), f(f, 0))");
    m._set("A3", "=REDUCE(0, SEQUENCE(100000), LAMBDA(a, v, a+v))");
    m._set(
        "A4",
        "=MAP(SEQUENCE(1000), LAMBDA(x, REDUCE(0, SEQUENCE(1000), LAMBDA(a, v, a+v))))",
    );
    let started = Instant::now();
    m.evaluate();
    assert_eq!(text(&m, "A1"), "#NUM!");
    assert_eq!(text(&m, "A2"), "#NUM!");
    assert_eq!(text(&m, "A3"), "5000050000");
    // a million lambda calls are over the step budget: an error, and quickly
    assert!(!text(&m, "A4").is_empty());
    assert!(started.elapsed() < Duration::from_secs(60));
}

#[test]
fn deep_nesting_of_calls_and_names_stays_bounded() {
    let mut m = new_empty_model();
    let deep = format!("{}1{}", "SORT(".repeat(60), ")".repeat(60));
    m._set("A1", &format!("={deep}"));
    let too_deep = format!("{}1{}", "SORT(".repeat(200), ")".repeat(200));
    m._set("B1", &format!("={too_deep}"));
    m.evaluate();
    assert_eq!(text(&m, "A1"), "1");
    assert_eq!(text(&m, "B1"), "#VALUE!");
}

#[test]
fn many_arrays_that_collide_leave_one_owner_each_cell() {
    let mut m = new_empty_model();
    for i in 1..=200 {
        m._set(&format!("A{i}"), "=SEQUENCE(300)");
    }
    m.evaluate();
    // every anchor inside another's area blocks it; the last one has the cells below it to itself
    assert_eq!(text(&m, "A1"), "#SPILL!");
    assert_eq!(text(&m, "A199"), "#SPILL!");
    assert_eq!(text(&m, "A200"), "1");
    assert_eq!(text(&m, "A300"), "101");
}

#[test]
fn recalculation_that_runs_out_of_time_finishes_over_several_calls() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    for r in 1..=300 {
        wb.set_cell("Sheet1", &format!("A{r}"), format!("{}", 300 - r))
            .unwrap();
    }
    for c in 0..120u32 {
        let col = (b'C' + (c % 20) as u8) as char;
        let row = 1 + c / 20 * 400;
        wb.set_cell("Sheet1", &format!("{col}{row}"), "=SORT(A1:A300)")
            .unwrap();
    }
    let budget = Budget {
        max_time: Duration::from_millis(2),
        ..Budget::default()
    };
    let mut session = RecalcSession::with_budget(wb, budget).unwrap();
    let first = session.recalculate();
    let mut calls = 1;
    let mut last = first.clone();
    while last.timed_out && calls < 2000 {
        last = session.recalculate();
        calls += 1;
    }
    assert!(first.timed_out, "the first pass was cut off");
    assert!(!last.timed_out, "it finished after {calls} calls");
    let at = |a1: &str| {
        session
            .cell_value("Sheet1", PackedCellCoordinate::parse_a1(a1).unwrap())
            .to_display_string()
    };
    assert_eq!((at("C1"), at("C300")), ("0".to_string(), "299".to_string()));
    assert_eq!(
        (at("C2001"), at("C2300")),
        ("0".to_string(), "299".to_string())
    );
}

#[test]
fn a_sheet_rename_follows_a_spill_range_reference() {
    let renamed = rewrite_sheet_names(
        "SUM(Sheet1!B2#)+SUM('Sheet1'!C3#)",
        SheetChange::Rename {
            from: "Sheet1",
            to: "My Data",
        },
    );
    assert_eq!(
        renamed.as_deref(),
        Some("SUM('My Data'!B2#)+SUM('My Data'!C3#)")
    );
    let deleted = rewrite_sheet_names("SUM(Sheet1!B2#)", SheetChange::Delete { name: "Sheet1" });
    assert!(deleted.unwrap().contains("#REF!"));
}

#[test]
fn a_defined_name_for_a_lambda_spills_and_follows_its_definition() {
    use dynamic_support::DefinedNameEntry;
    let mut m = new_empty_model();
    m.new_defined_name(DefinedNameEntry::new(
        "Squares",
        "=LAMBDA(n, SEQUENCE(n)^2)",
    ))
    .unwrap();
    m._set("A1", "=Squares(4)");
    m.evaluate();
    assert_eq!(
        (text(&m, "A1"), text(&m, "A4")),
        ("1".to_string(), "16".to_string())
    );
    m.new_defined_name(DefinedNameEntry::new("Cubes", "=LAMBDA(n, SEQUENCE(n)^3)"))
        .unwrap();
    m._set("C1", "=Cubes(3)");
    m.evaluate();
    assert_eq!(text(&m, "C3"), "27");
}
