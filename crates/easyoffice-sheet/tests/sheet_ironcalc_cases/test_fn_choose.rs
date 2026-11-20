use crate::test::util::{new_empty_model, new_empty_model_ref_1900};

#[test]
fn test_fn_choose_incorrect_index() {
    let mut model = new_empty_model();
    model._set("A1", "=CHOOSE(-1, 42)");
    model._set("A2", "=CHOOSE(0, 42)");
    model._set("A3", "=CHOOSE(1, 42)");
    model._set("A4", "=CHOOSE(2, 42)");
    model._set("B1", "TEST");
    model._set("A5", "=CHOOSE(B1, 42)");
    model.evaluate();

    assert_eq!(model._get_text("A1"), *"#VALUE!");
    assert_eq!(model._get_text("A2"), *"#VALUE!");
    assert_eq!(model._get_text("A3"), *"42");
    assert_eq!(model._get_text("A4"), *"#VALUE!");
    assert_eq!(model._get_text("A5"), *"#VALUE!");
}
// CHOOSE broadcasts when the index argument is an array/range: each index element
// selects the matching value argument, and the result spills with the index shape.
#[test]
fn test_fn_choose_array_index() {
    let mut model = new_empty_model();
    model._set("A1", "10");
    model._set("A2", "20");
    model._set("A3", "30");

    // Inline array index reorders the scalar values.
    model._set("C1", "=CHOOSE({1;3;2}, 100, 200, 300)");
    // Array index selecting cell references (collapsed to scalars).
    model._set("E1", "=CHOOSE({1;2;3}, A1, A2, A3)");
    // An out-of-range index errors only in its own cell.
    model._set("G1", "=CHOOSE({1;5}, 7, 8)");

    model.evaluate();

    assert_eq!(model._get_text("C1"), "100");
    assert_eq!(model._get_text("C2"), "300");
    assert_eq!(model._get_text("C3"), "200");
    assert_eq!(model._get_text("C4"), "");

    assert_eq!(model._get_text("E1"), "10");
    assert_eq!(model._get_text("E2"), "20");
    assert_eq!(model._get_text("E3"), "30");

    assert_eq!(model._get_text("G1"), "7");
    assert_eq!(model._get_text("G2"), "#VALUE!");
}
