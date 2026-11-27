//! `INDEX` with an array for `row_num` or `column_num` returns one element per array item, as in
//! the reference spreadsheet app (365 edition), where `=SUM(INDEX(A1:A10,{1,3}))` adds the first and third cell. It does not read
//! only the first item of the array.
#![allow(clippy::unwrap_used)]

mod dynamic_support;
use dynamic_support::new_empty_model;

fn model() -> dynamic_support::Model {
    let mut m = new_empty_model();
    for r in 1..=10 {
        m._set(&format!("A{r}"), &r.to_string());
    }
    // A 3x3 block in D1:F3 holding 1..9 by rows.
    for (i, cell) in ["D1", "E1", "F1", "D2", "E2", "F2", "D3", "E3", "F3"]
        .iter()
        .enumerate()
    {
        m._set(cell, &(i + 1).to_string());
    }
    m
}

#[test]
fn an_array_of_row_numbers_picks_every_row() {
    let mut m = model();
    m._set("H1", "=SUM(INDEX(A1:A10,{1,3}))");
    m._set("H2", "=SUM(INDEX(A1:A10,{1;3;5}))");
    m._set("H3", "=SUM(INDEX(A1:A10,{2}))");
    m._set("H4", "=SUM(INDEX(A1:A10,2))");
    m.evaluate();
    assert_eq!(m._get_text("H1"), "4");
    assert_eq!(m._get_text("H2"), "9");
    assert_eq!(m._get_text("H3"), "2");
    assert_eq!(m._get_text("H4"), "2");
}

#[test]
fn the_picked_items_spill_in_the_shape_of_the_index_array() {
    let mut m = model();
    m._set("H1", "=INDEX(A1:A10,{1,3})");
    m._set("J1", "=INDEX(A1:A10,{2;4})");
    m.evaluate();
    assert_eq!(
        [
            m._get_text("H1"),
            m._get_text("I1"),
            m._get_text("J1"),
            m._get_text("J2")
        ],
        ["1", "3", "2", "4"]
    );
}

#[test]
fn row_and_column_arrays_pair_up_item_by_item() {
    let mut m = model();
    // (1,2) and (2,3) of D1:F3 are 2 and 6.
    m._set("H1", "=SUM(INDEX(D1:F3,{1,2},{2,3}))");
    // A row array with a scalar column.
    m._set("H2", "=SUM(INDEX(D1:F3,{1,3},1))");
    // A single row range takes the array as columns.
    m._set("H3", "=SUM(INDEX(D1:F1,{1,3}))");
    m.evaluate();
    assert_eq!(m._get_text("H1"), "8");
    assert_eq!(m._get_text("H2"), "8");
    assert_eq!(m._get_text("H3"), "4");
}

#[test]
fn an_item_past_the_range_is_a_ref_error_and_a_negative_one_a_value_error() {
    let mut m = model();
    m._set("H1", "=SUM(INDEX(A1:A10,{1,20}))");
    m._set("H2", "=SUM(INDEX(A1:A10,{1,-1}))");
    m._set("H3", "=INDEX(A1:A10,{1,20})");
    m.evaluate();
    assert_eq!(m._get_text("H1"), "#REF!");
    assert_eq!(m._get_text("H2"), "#VALUE!");
    assert_eq!(m._get_text("H3"), "1");
    assert_eq!(m._get_text("I3"), "#REF!");
}

#[test]
fn an_array_held_in_a_name_or_a_range_of_numbers_works_too() {
    let mut m = model();
    m._set("B1", "1");
    m._set("C1", "3");
    m._set("H1", "=SUM(INDEX(A1:A10,B1:C1))");
    m._set("H2", "=LET(r,{2,4},SUM(INDEX(A1:A10,r)))");
    // A LET variable that holds one number still takes the scalar path.
    m._set("H3", "=LET(r,5,INDEX(A1:A10,r))");
    m.evaluate();
    assert_eq!(m._get_text("H1"), "4");
    assert_eq!(m._get_text("H2"), "6");
    assert_eq!(m._get_text("H3"), "5");
}

#[test]
fn a_whole_line_of_a_two_dimensional_range_cannot_be_an_array_item() {
    let mut m = model();
    // Row 0 of an item would be a whole column: an array cannot hold arrays.
    m._set("H1", "=SUM(INDEX(D1:F3,{0,1},2))");
    m.evaluate();
    assert_eq!(m._get_text("H1"), "#VALUE!");
}
