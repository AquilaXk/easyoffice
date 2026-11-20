use crate::test::util::{new_empty_model, new_empty_model_ref_1900};

#[test]
fn fn_round_approximation() {
    let mut model = new_empty_model();
    model._set("A1", "=ROUND(1.05*(0.0284+0.0046)-0.0284,4)");
    model._set("A2", "=ROUNDDOWN(1.05*(0.0284+0.0046)-0.0284,5)");

    model.evaluate();

    assert_eq!(model._get_text("A1"), *"0.0063");
    assert_eq!(model._get_text("A2"), *"0.00625");
}
#[test]
fn fn_round_digit_counts_beyond_f64_range() {
    // Values from the reference app. the reference app does not parse 1E+308 as a literal, so it is computed.
    let mut model = new_empty_model();
    model._set("A1", "=ROUND(1E+307*10,10)");
    model._set("A2", "=ROUNDUP(1E+307*10,5)");
    model._set("A3", "=ROUNDDOWN(1E+307*10,5)");
    model._set("A4", "=ROUND(123.456,-400)");
    model._set("A5", "=ROUNDDOWN(123.456,-400)");
    model._set("A6", "=TRUNC(123.456,-400)");
    model._set("A7", "=TRUNC(123.456,-16)");
    model._set("A8", "=TRUNC(-1E+300,20)");
    model.evaluate();

    assert_eq!(model._get_text("A1"), *"1E+308");
    assert_eq!(model._get_text("A2"), *"1E+308");
    assert_eq!(model._get_text("A3"), *"1E+308");
    assert_eq!(model._get_text("A4"), *"0");
    assert_eq!(model._get_text("A5"), *"0");
    assert_eq!(model._get_text("A6"), *"0");
    assert_eq!(model._get_text("A7"), *"0");
    assert_eq!(model._get_text("A8"), *"-1E+300");
}
