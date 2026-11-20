use crate::test::util::{new_empty_model, new_empty_model_ref_1900};

#[test]
fn empty_text_returns_zero() {
    let mut model = new_empty_model();
    model._set("A1", "=DECIMAL(\"\", 2)");
    model._set("A2", "=DECIMAL(A3, 2)");
    model._set("A4", "=DECIMAL(\"\", 99)");

    model.evaluate();

    assert_eq!(model._get_text("A1"), *"0");
    assert_eq!(model._get_text("A2"), *"0");
    assert_eq!(model._get_text("A4"), *"#NUM!");
}
#[test]
fn base_min_length_max_255() {
    let mut model = new_empty_model();
    model._set("A1", "=LEN(BASE(5, 2, 255))");
    model._set("A2", "=BASE(5, 2, 255.9)");
    model._set("A3", "=BASE(5, 2, 256)");
    model._set("A4", "=BASE(5, 2, 1E6)");

    model.evaluate();

    assert_eq!(model._get_text("A1"), *"255");
    assert_eq!(model._get_text("A2"), "0".repeat(252) + "101");
    assert_eq!(model._get_text("A3"), *"#NUM!");
    assert_eq!(model._get_text("A4"), *"#NUM!");
}
#[test]
fn base_number_at_or_above_2_pow_53_is_num_error() {
    let mut model = new_empty_model();
    model._set("A1", "=BASE(2^53-1, 16)");
    model._set("A2", "=BASE(2^53, 2)");
    model._set("A3", "=BASE(2^53, 2, B1)");
    model._set("A4", "=BASE(1E+16, 16)");

    model.evaluate();

    assert_eq!(model._get_text("A1"), *"1FFFFFFFFFFFFF");
    assert_eq!(model._get_text("A2"), *"#NUM!");
    assert_eq!(model._get_text("A3"), *"#NUM!");
    assert_eq!(model._get_text("A4"), *"#NUM!");
}
#[test]
fn base_negative_fraction_is_num_error() {
    let mut model = new_empty_model();
    model._set("A1", "=BASE(-0.5, 2)");
    model._set("A2", "=BASE(-0.5, 2, )");
    model._set("A3", "=BASE(-9.9999999999999995E-8, 2)");
    model._set("A4", "=BASE(-1, 2)");
    model._set("A5", "=BASE(0.5, 2)");
    model._set("A6", "=BASE(-0, 2)");

    model.evaluate();

    assert_eq!(model._get_text("A1"), *"#NUM!");
    assert_eq!(model._get_text("A2"), *"#NUM!");
    assert_eq!(model._get_text("A3"), *"#NUM!");
    assert_eq!(model._get_text("A4"), *"#NUM!");
    assert_eq!(model._get_text("A5"), *"0");
    assert_eq!(model._get_text("A6"), *"0");
}
#[test]
fn decimal_text_max_255() {
    let mut model = new_empty_model();
    model._set("A1", "=DECIMAL(REPT(\"0\",254)&\"1\", 2)");
    model._set("A2", "=DECIMAL(REPT(\"0\",255)&\"1\", 2)");
    model._set("A3", "=DECIMAL(REPT(\"0\",300), 16)");

    model.evaluate();

    assert_eq!(model._get_text("A1"), *"1");
    assert_eq!(model._get_text("A2"), *"#VALUE!");
    assert_eq!(model._get_text("A3"), *"#VALUE!");
}
#[test]
fn leading_space_is_ignored() {
    let mut model = new_empty_model();
    model._set("A1", "=DECIMAL(\" 10\", 2)");
    model._set("A2", "=DECIMAL(\" FF\", 16)");

    model.evaluate();

    assert_eq!(model._get_text("A1"), *"2");
    assert_eq!(model._get_text("A2"), *"255");
}
