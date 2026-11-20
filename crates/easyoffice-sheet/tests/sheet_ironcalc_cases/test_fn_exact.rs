use crate::test::util::{new_empty_model, new_empty_model_ref_1900};

#[test]
fn fn_exact() {
    let mut model = new_empty_model();

    model._set("A1", "=EXACT(2.3, 2.3)");
    model._set("A2", r#"=EXACT(2.3, "2.3")"#);
    model._set("A3", r#"=EXACT("Hello", "hello")"#);

    model.evaluate();

    assert_eq!(model._get_text("A1"), *"TRUE");
    assert_eq!(model._get_text("A2"), *"TRUE");
    assert_eq!(model._get_text("A3"), *"FALSE");
}
#[test]
fn single_cells() {
    let mut model = new_empty_model();
    model._set("A1", "Hello");
    model._set("A2", "hello");
    model._set("A3", "Hello");
    model._set("B1", "=EXACT(A1, A2)");
    model._set("B2", "=EXACT(A1, A3)");
    // An empty cell is the empty string
    model._set("B3", "=EXACT(A4, \"\")");
    model._set("B4", "=EXACT(TRUE, \"TRUE\")");
    model._set("B5", "=EXACT(1/0, \"a\")");
    model.evaluate();

    assert_eq!(model._get_text("B1"), *"FALSE");
    assert_eq!(model._get_text("B2"), *"TRUE");
    assert_eq!(model._get_text("B3"), *"TRUE");
    assert_eq!(model._get_text("B4"), *"TRUE");
    assert_eq!(model._get_text("B5"), *"#DIV/0!");
}
#[test]
fn range_and_text() {
    let mut model = new_empty_model();
    model._set("A1", "he");
    model._set("A2", "He");
    model._set("A3", "HE");
    model._set("C1", "=EXACT(A1:A4, \"He\")");
    model._set("D1", "=EXACT(\"he\", A1:A4)");
    model.evaluate();

    assert_eq!(model._get_text("C1"), *"FALSE");
    assert_eq!(model._get_text("C2"), *"TRUE");
    assert_eq!(model._get_text("C3"), *"FALSE");
    assert_eq!(model._get_text("C4"), *"FALSE");
    assert_eq!(model._get_text("C5"), *"");

    assert_eq!(model._get_text("D1"), *"TRUE");
    assert_eq!(model._get_text("D2"), *"FALSE");
    assert_eq!(model._get_text("D3"), *"FALSE");
    assert_eq!(model._get_text("D4"), *"FALSE");
}
#[test]
fn two_ranges() {
    let mut model = new_empty_model();
    model._set("A1", "a");
    model._set("A2", "b");
    model._set("A3", "c");
    model._set("B1", "a");
    model._set("B2", "B");
    model._set("B3", "c");
    model._set("D1", "=EXACT(A1:A3, B1:B3)");
    // A column and a row make a table
    model._set("F1", "=EXACT(A1:A2, {\"a\",\"b\",\"c\"})");
    model.evaluate();

    assert_eq!(model._get_text("D1"), *"TRUE");
    assert_eq!(model._get_text("D2"), *"FALSE");
    assert_eq!(model._get_text("D3"), *"TRUE");

    assert_eq!(model._get_text("F1"), *"TRUE");
    assert_eq!(model._get_text("G1"), *"FALSE");
    assert_eq!(model._get_text("H1"), *"FALSE");
    assert_eq!(model._get_text("F2"), *"FALSE");
    assert_eq!(model._get_text("G2"), *"TRUE");
    assert_eq!(model._get_text("H2"), *"FALSE");
}
#[test]
fn ranges_of_different_sizes() {
    let mut model = new_empty_model();
    model._set("A1", "a");
    model._set("A2", "b");
    model._set("A3", "c");
    model._set("B1", "a");
    model._set("B2", "b");
    model._set("D1", "=EXACT(A1:A3, B1:B2)");
    model.evaluate();

    assert_eq!(model._get_text("D1"), *"TRUE");
    assert_eq!(model._get_text("D2"), *"TRUE");
    assert_eq!(model._get_text("D3"), *"#N/A");
}
