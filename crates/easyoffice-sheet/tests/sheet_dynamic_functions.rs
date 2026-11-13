//! `MAP`, `REDUCE`, `SCAN`, `BYROW`, `BYCOL`, `MAKEARRAY` and the array forms of ordinary functions
//! (PD-217). The cases for `MAP`/`REDUCE`/`SCAN`/`BYROW`/`BYCOL`/`MAKEARRAY` follow the examples of the
//! spreadsheet program's function help; the reference engine's own tests for them are in
//! `sheet_dynamic_lambda.rs`.
#![allow(clippy::unwrap_used)]

mod dynamic_support;
use dynamic_support::new_empty_model;

fn column(model: &dynamic_support::Model, first: &str, count: usize) -> Vec<String> {
    let (col, row) = first.split_at(1);
    let row: usize = row.parse().unwrap();
    (0..count)
        .map(|i| model._get_text(&format!("{col}{}", row + i)))
        .collect()
}

#[test]
fn map_applies_a_lambda_to_every_element() {
    let mut m = new_empty_model();
    for (i, v) in ["1", "2", "3"].iter().enumerate() {
        m._set(&format!("A{}", i + 1), v);
    }
    m._set("C1", "=MAP(A1:A3, LAMBDA(x, x*10))");
    m._set("D1", "=MAP(A1:A3, A1:A3, LAMBDA(a, b, a+b))");
    m.evaluate();
    assert_eq!(column(&m, "C1", 3), ["10", "20", "30"]);
    assert_eq!(column(&m, "D1", 3), ["2", "4", "6"]);
}

#[test]
fn reduce_and_scan_fold_in_order() {
    let mut m = new_empty_model();
    for (i, v) in ["1", "2", "3", "4"].iter().enumerate() {
        m._set(&format!("A{}", i + 1), v);
    }
    m._set("B1", "=REDUCE(0, A1:A4, LAMBDA(acc, v, acc+v))");
    m._set("B2", "=REDUCE(A1:A4, LAMBDA(acc, v, acc*v))");
    m._set("C1", "=SCAN(0, A1:A4, LAMBDA(acc, v, acc+v))");
    m._set("D1", "=SCAN(A1:A4, LAMBDA(acc, v, acc*v))");
    m.evaluate();
    assert_eq!(m._get_text("B1"), "10");
    assert_eq!(m._get_text("B2"), "24");
    assert_eq!(column(&m, "C1", 4), ["1", "3", "6", "10"]);
    assert_eq!(column(&m, "D1", 4), ["1", "2", "6", "24"]);
}

#[test]
fn byrow_and_bycol_hand_over_one_line_at_a_time() {
    let mut m = new_empty_model();
    for (cell, v) in [("A1", "1"), ("B1", "2"), ("A2", "3"), ("B2", "4")] {
        m._set(cell, v);
    }
    m._set("D1", "=BYROW(A1:B2, LAMBDA(r, SUM(r)))");
    m._set("F1", "=BYCOL(A1:B2, LAMBDA(c, MAX(c)))");
    m.evaluate();
    assert_eq!(column(&m, "D1", 2), ["3", "7"]);
    assert_eq!(m._get_text("F1"), "3");
    assert_eq!(m._get_text("G1"), "4");
}

#[test]
fn makearray_calls_the_lambda_with_row_and_column() {
    let mut m = new_empty_model();
    m._set("A1", "=MAKEARRAY(2, 3, LAMBDA(r, c, r*10+c))");
    m.evaluate();
    assert_eq!(m._get_text("A1"), "11");
    assert_eq!(m._get_text("C1"), "13");
    assert_eq!(m._get_text("A2"), "21");
    assert_eq!(m._get_text("C2"), "23");
}

#[test]
fn a_lambda_that_returns_an_array_is_calc_error() {
    let mut m = new_empty_model();
    m._set("A1", "=MAP(SEQUENCE(2), LAMBDA(x, SEQUENCE(2)))");
    m._set("A4", "=LAMBDA(x, x)");
    m.evaluate();
    assert_eq!(m._get_text("A1"), "#CALC!");
    assert_eq!(m._get_text("A4"), "#CALC!");
}

#[test]
fn deep_lambda_recursion_ends_with_num_error() {
    let mut m = new_empty_model();
    // A lambda that passes itself on calls itself without end.
    m._set("A1", "=LET(f, LAMBDA(g, g(g)), f(f))");
    m._set(
        "A2",
        "=LET(f, LAMBDA(g, n, IF(n=0, 0, 1+g(g, n-1))), f(f, 30))",
    );
    m._set(
        "A3",
        "=LET(f, LAMBDA(g, n, IF(n=0, 0, 1+g(g, n-1))), f(f, 500))",
    );
    m.evaluate();
    assert_eq!(m._get_text("A1"), "#NUM!");
    assert_eq!(m._get_text("A2"), "30");
    assert_eq!(m._get_text("A3"), "#NUM!");
}

#[test]
fn array_constants_and_elementwise_operators() {
    let mut m = new_empty_model();
    m._set("A1", "={1,2,3;4,5,6}*2");
    m._set("A4", "=SUM({1,2,3;4,5,6})");
    m._set("A5", "=TRANSPOSE({1,2,3})");
    m._set("E1", "=UPPER({\"a\",\"b\"})");
    m.evaluate();
    assert_eq!(m._get_text("A1"), "2");
    assert_eq!(m._get_text("C1"), "6");
    assert_eq!(m._get_text("A2"), "8");
    assert_eq!(m._get_text("C2"), "12");
    assert_eq!(m._get_text("E1"), "A");
    assert_eq!(m._get_text("F1"), "B");
    assert_eq!(m._get_text("A4"), "21");
    assert_eq!(column(&m, "A5", 3), ["1", "2", "3"]);
}
