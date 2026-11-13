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

fn grid_text(
    m: &dynamic_support::Model,
    first: &str,
    rows: usize,
    cols: usize,
) -> Vec<Vec<String>> {
    let (col, row) = first.split_at(1);
    let (col, row) = (col.as_bytes()[0], row.parse::<usize>().unwrap());
    (0..rows)
        .map(|r| {
            (0..cols)
                .map(|c| m._get_text(&format!("{}{}", (col + c as u8) as char, row + r)))
                .collect()
        })
        .collect()
}

fn filled(values: &[&str]) -> dynamic_support::Model {
    let mut m = new_empty_model();
    // A1:C3 = 1..9
    for (i, v) in values.iter().enumerate() {
        m._set(
            &format!("{}{}", (b'A' + (i % 3) as u8) as char, i / 3 + 1),
            v,
        );
    }
    m
}

#[test]
fn choose_rows_and_columns_count_from_either_end() {
    let mut m = filled(&["1", "2", "3", "4", "5", "6", "7", "8", "9"]);
    m._set("E1", "=CHOOSEROWS(A1:C3, 3, -3)");
    m._set("I1", "=CHOOSECOLS(A1:C3, 1, -1)");
    m._set("K1", "=CHOOSEROWS(A1:C3, 4)");
    m._set("L1", "=CHOOSECOLS(A1:C3, 0)");
    m.evaluate();
    assert_eq!(
        grid_text(&m, "E1", 2, 3),
        [["7", "8", "9"], ["1", "2", "3"]]
    );
    assert_eq!(
        grid_text(&m, "I1", 3, 2),
        [["1", "3"], ["4", "6"], ["7", "9"]]
    );
    assert_eq!(m._get_text("K1"), "#VALUE!");
    assert_eq!(m._get_text("L1"), "#VALUE!");
}

#[test]
fn expand_pads_and_refuses_to_shrink() {
    let mut m = filled(&["1", "2", "3", "4", "5", "6", "7", "8", "9"]);
    m._set("E1", "=EXPAND(A1:B2, 3, 3, \"x\")");
    m._set("I1", "=EXPAND(A1:B2, 3)");
    m._set("L1", "=EXPAND(A1:C3, 2)");
    m.evaluate();
    assert_eq!(
        grid_text(&m, "E1", 3, 3),
        [["1", "2", "x"], ["4", "5", "x"], ["x", "x", "x"]]
    );
    assert_eq!(
        grid_text(&m, "I1", 3, 2),
        [["1", "2"], ["4", "5"], ["#N/A", "#N/A"]]
    );
    assert_eq!(m._get_text("L1"), "#VALUE!");
}

#[test]
fn vstack_and_hstack_pad_the_short_side_with_na() {
    let mut m = filled(&["1", "2", "3", "4", "5", "6", "7", "8", "9"]);
    m._set("E1", "=VSTACK(A1:B1, A2:C2)");
    m._set("I1", "=HSTACK(A1:A2, A1:A3)");
    m.evaluate();
    assert_eq!(
        grid_text(&m, "E1", 2, 3),
        [["1", "2", "#N/A"], ["4", "5", "6"]]
    );
    assert_eq!(
        grid_text(&m, "I1", 3, 2),
        [["1", "1"], ["4", "4"], ["#N/A", "7"]]
    );
}

#[test]
fn wraprows_and_wrapcols_cut_a_vector_into_lines() {
    let mut m = new_empty_model();
    m._set("E1", "=WRAPROWS(SEQUENCE(1,5), 2)");
    m._set("I1", "=WRAPCOLS(SEQUENCE(5), 2, 0)");
    m._set("M1", "=WRAPROWS(A1:B2, 2)");
    m.evaluate();
    assert_eq!(
        grid_text(&m, "E1", 3, 2),
        [["1", "2"], ["3", "4"], ["5", "#N/A"]]
    );
    assert_eq!(
        grid_text(&m, "I1", 2, 3),
        [["1", "3", "5"], ["2", "4", "0"]]
    );
    assert_eq!(m._get_text("M1"), "#VALUE!");
}

#[test]
fn take_and_drop_accept_a_missing_size() {
    let mut m = filled(&["1", "2", "3", "4", "5", "6", "7", "8", "9"]);
    m._set("E1", "=TAKE(A1:C3, , -1)");
    m._set("G1", "=DROP(A1:C3, 1, 1)");
    m.evaluate();
    assert_eq!(grid_text(&m, "E1", 3, 1), [["3"], ["6"], ["9"]]);
    assert_eq!(grid_text(&m, "G1", 2, 2), [["5", "6"], ["8", "9"]]);
}

#[test]
fn an_optional_parameter_and_its_xlsx_spelling() {
    let mut m = new_empty_model();
    m._set("A1", "=LAMBDA(a, [b], IF(ISOMITTED(b), a/2, a*b))(12)");
    m._set("A2", "=LAMBDA(a, [b], IF(ISOMITTED(b), a/2, a*b))(12, 3)");
    m._set("A3", "=LAMBDA(a, [b], a+b)(1, 2, 3)");
    m.evaluate();
    assert_eq!(
        [m._get_text("A1"), m._get_text("A2"), m._get_text("A3")],
        ["6", "36", "#VALUE!"]
    );
}
