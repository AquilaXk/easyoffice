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
#[test]
fn fn_round_binary_noise_after_scaling() {
    // 1.005 * 100 is 100.49999999999999 in binary, and -(0.1 + 0.7) is -0.7999999999999999.
    // ROUNDDOWN((0.1+0.7)*10,0) is 7 in the reference app (measured by QA on 2026-11-20);
    // the reference engine A gives 8 because it takes the value at 15 significant digits first.
    let mut model = new_empty_model();
    model._set("A1", "=ROUND(1.005,2)");
    model._set("A2", "=ROUNDDOWN((0.1+0.7)*10,0)");
    model._set("A3", "=TRUNC(-(0.1+0.7),1)");
    model.evaluate();

    assert_eq!(model._get_text("A1"), *"1.01");
    assert_eq!(model._get_text("A2"), *"7");
    assert_eq!(model._get_text("A3"), *"-0.8");
}
