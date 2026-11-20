//! PD-263: in the 1904 date system a serial is 1,462 lower for the same day; the calendar functions, `TEXT`, the weekday and the
//! period rules follow it, and the 1900 system is untouched (including its day 60 and day 0).
#![allow(clippy::unwrap_used)]

use easyoffice_sheet::{enter_date_system, EvaluatedCellValue, SheetEngine};

fn eval(engine: &SheetEngine, formula: &str) -> EvaluatedCellValue {
    engine.evaluate_formula_str(formula)
}

fn num(n: f64) -> EvaluatedCellValue {
    EvaluatedCellValue::Number(n)
}

fn text(s: &str) -> EvaluatedCellValue {
    EvaluatedCellValue::Text(s.to_string())
}

#[test]
fn the_1904_system_counts_from_1904() {
    let mut engine = SheetEngine::new("S");
    engine.set_cell("A1", "44462").unwrap();
    {
        let _system = enter_date_system(true);
        assert_eq!(eval(&engine, "=DATE(2026,11,10)"), num(44874.0));
        assert_eq!(eval(&engine, "=TEXT(0,\"yyyy-mm-dd\")"), text("1904-01-01"));
        assert_eq!(
            eval(&engine, "=TEXT(A1,\"yyyy-mm-dd\")"),
            text("2025-09-24")
        );
        assert_eq!(eval(&engine, "=YEAR(A1)"), num(2025.0));
        assert_eq!(eval(&engine, "=DATEVALUE(\"2026-11-10\")"), num(44874.0));
        assert_eq!(eval(&engine, "=WEEKDAY(DATE(2026,11,10))"), num(3.0));
        assert_eq!(eval(&engine, "=WEEKDAY(DATE(2026,11,10),2)"), num(2.0));
        assert_eq!(
            eval(&engine, "=TEXT(DATE(2026,11,10),\"aaaa\")"),
            text("화요일")
        );
        assert_eq!(eval(&engine, "=EDATE(DATE(2026,11,10),1)"), num(44904.0));
        assert_eq!(eval(&engine, "=DAY(EOMONTH(DATE(2026,2,1),0))"), num(28.0));
        assert!(
            matches!(eval(&engine, "=TODAY()"), EvaluatedCellValue::Number(n) if n > 40_000.0 && n < 50_000.0)
        );
    }
    // The guard puts the 1900 system back.
    assert_eq!(eval(&engine, "=DATE(2026,11,10)"), num(46336.0));
    assert_eq!(
        eval(&engine, "=TEXT(A1,\"yyyy-mm-dd\")"),
        text("2021-09-23")
    );
}

#[test]
fn the_1900_system_keeps_day_60_and_day_0() {
    let engine = SheetEngine::new("S");
    assert_eq!(
        eval(&engine, "=TEXT(60,\"yyyy-mm-dd\")"),
        text("1900-02-29")
    );
    assert_eq!(eval(&engine, "=TEXT(0,\"yyyy-mm-dd\")"), text("1900-01-00"));
    assert_eq!(eval(&engine, "=WEEKDAY(DATE(2026,11,10))"), num(3.0));
}

/// The 1904 system has no day before 1904-01-01 and ends 1,462 days before the 1900 system does.
#[test]
fn the_1904_system_has_its_own_first_and_last_day() {
    let engine = SheetEngine::new("S");
    let _system = enter_date_system(true);
    assert_eq!(eval(&engine, "=DATE(1904,1,1)"), num(0.0));
    assert!(matches!(
        eval(&engine, "=DATE(1903,12,31)"),
        EvaluatedCellValue::Error(_)
    ));
    assert_eq!(eval(&engine, "=DATE(9999,12,31)"), num(2_957_003.0));
    assert!(matches!(
        eval(&engine, "=WEEKDAY(2957004)"),
        EvaluatedCellValue::Error(_)
    ));
    assert_eq!(eval(&engine, "=WEEKDAY(2957003)"), num(6.0));
}
