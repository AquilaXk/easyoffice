use crate::test::util::{new_empty_model, new_empty_model_ref_1900};

#[test]
fn sum_arrays() {
    let mut model = new_empty_model();
    model._set("A1", "=SUM({1,2,3}+{3,4,5})");

    model.evaluate();

    assert_eq!(model._get_text("A1"), *"18");
}
// Concatenation (`&`) broadcasts element-wise over ranges/arrays.
#[test]
fn concatenate_broadcasts_over_range() {
    let mut model = new_empty_model();
    model._set("A1", "a");
    model._set("A2", "b");
    // Spills the per-cell concatenation.
    model._set("C1", "=A1:A2&\"!\"");
    // And it can be consumed in an array context.
    model._set("E1", "=COUNTA(A1:A2&\"!\")");
    model.evaluate();
    assert_eq!(model._get_text("C1"), "a!");
    assert_eq!(model._get_text("C2"), "b!");
    assert_eq!(model._get_text("E1"), "2");
}
// UPPER/LOWER broadcast element-wise over ranges/arrays.
#[test]
fn upper_lower_broadcast_over_range() {
    let mut model = new_empty_model();
    model._set("A1", "abc");
    model._set("A2", "DeF");
    model._set("C1", "=UPPER(A1:A2)");
    model._set("D1", "=LOWER(A1:A2)");
    model.evaluate();
    assert_eq!(model._get_text("C1"), "ABC");
    assert_eq!(model._get_text("C2"), "DEF");
    assert_eq!(model._get_text("D1"), "abc");
    assert_eq!(model._get_text("D2"), "def");
}
// TEXT broadcasts element-wise over a range/array (spilling), and DAY/MONTH/YEAR
// do the same (these previously panicked / failed when handed a range).
#[test]
fn text_and_date_parts_broadcast_over_range() {
    let mut model = new_empty_model();
    model._set("A1", "1.5");
    model._set("A2", "2.4");
    model._set("C1", "=TEXT(A1:A2, \"0.0\")");
    model.evaluate();
    assert_eq!(model._get_text("C1"), "1.5");
    assert_eq!(model._get_text("C2"), "2.4");

    // 2024-01-01 (serial 45292) and 2024-02-15 (serial 45337)
    let mut model = new_empty_model();
    model._set("A1", "45292");
    model._set("A2", "45337");
    model._set("C1", "=DAY(A1:A2)");
    model._set("D1", "=MONTH(A1:A2)");
    model._set("E1", "=YEAR(A1:A2)");
    model.evaluate();
    assert_eq!(model._get_text("C1"), "1");
    assert_eq!(model._get_text("C2"), "15");
    assert_eq!(model._get_text("D1"), "1");
    assert_eq!(model._get_text("D2"), "2");
    assert_eq!(model._get_text("E1"), "2024");
    assert_eq!(model._get_text("E2"), "2024");
}
// Regression: the reference app's size-1 broadcasting for element-wise operators when the
// two operands have different (but compatible) shapes. A 2×1 array combined
// with a 1×2 array must produce a 2×2 result, with the singleton row/column
// repeating — not #VALUE! holes from out-of-range indexing.

#[test]
fn arithmetic_size_one_broadcast() {
    let mut model = new_empty_model();
    // {1;2} + {10,20} -> 2x2
    model._set("A1", "={1;2}+{10,20}");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "11");
    assert_eq!(model._get_text("B1"), "21");
    assert_eq!(model._get_text("A2"), "12");
    assert_eq!(model._get_text("B2"), "22");
}
#[test]
fn concatenate_size_one_broadcast() {
    let mut model = new_empty_model();
    // {"a";"b"} & {"x","y"} -> 2x2
    model._set("A1", "={\"a\";\"b\"}&{\"x\",\"y\"}");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "ax");
    assert_eq!(model._get_text("B1"), "ay");
    assert_eq!(model._get_text("A2"), "bx");
    assert_eq!(model._get_text("B2"), "by");
}
#[test]
fn comparison_size_one_broadcast() {
    let mut model = new_empty_model();
    // {1;2} = {1,2} -> 2x2
    model._set("A1", "={1;2}={1,2}");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "TRUE");
    assert_eq!(model._get_text("B1"), "FALSE");
    assert_eq!(model._get_text("A2"), "FALSE");
    assert_eq!(model._get_text("B2"), "TRUE");
}
