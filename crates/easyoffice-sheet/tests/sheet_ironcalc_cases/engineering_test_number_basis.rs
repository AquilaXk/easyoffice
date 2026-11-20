use crate::test::util::{new_empty_model, new_empty_model_ref_1900};

#[test]
fn fn_bin2hex_misc() {
    let mut model = new_empty_model();
    model._set("A1", "=BIN2HEX(1100011011, -2)");
    model._set("A2", "=BIN2HEX(1100011011, 11)");

    model.evaluate();

    assert_eq!(model._get_text("A1"), *"#NUM!");
    assert_eq!(model._get_text("A2"), *"#NUM!");
}
#[test]
fn fn_bin2oct_misc() {
    let mut model = new_empty_model();
    model._set("A1", "=BIN2OCT(1100011011, -2)");
    model._set("A2", "=BIN2OCT(1100011011, 11)");

    model.evaluate();

    assert_eq!(model._get_text("A1"), *"#NUM!");
    assert_eq!(model._get_text("A2"), *"#NUM!");
}
#[test]
fn fn_dec2oct_misc() {
    let mut model = new_empty_model();
    model._set("A1", "=DEC2OCT(-1213, 1)");

    model.evaluate();

    assert_eq!(model._get_text("A1"), *"7777775503");
}
#[test]
fn fn_dec2bin_misc() {
    let mut model = new_empty_model();
    model._set("A1", "=DEC2BIN(-511, 4)");
    model._set("A2", "=DEC2BIN(TRUE, -1)");
    model._set("A3", "=DEC2OCT(TRUE, -1)");
    model._set("A4", "=DEC2HEX(TRUE, -1)");

    model.evaluate();

    assert_eq!(model._get_text("A1"), *"1000000001");
    // Note the reference app here return #NUM! instead
    assert_eq!(model._get_text("A2"), *"#VALUE!");
    assert_eq!(model._get_text("A3"), *"#VALUE!");
    assert_eq!(model._get_text("A4"), *"#VALUE!");
}
#[test]
fn fn_oct2dec_misc() {
    let mut model = new_empty_model();
    model._set("A1", r#"=OCT2DEC(777)"#);
    model._set("A2", r#"=OCT2DEC("777")"#);
    model._set("A3", r#"=OCT2DEC("-1")"#);
    model._set("A4", r#"=OCT2BIN("-1")"#);
    model._set("A5", r#"=OCT2HEX("-1")"#);
    model._set("A6", r#"=OCT2DEC(4000000000)"#);

    model.evaluate();

    assert_eq!(model._get_text("A1"), *"511");
    assert_eq!(model._get_text("A1"), *"511");

    assert_eq!(model._get_text("A3"), *"#NUM!");
    assert_eq!(model._get_text("A4"), *"#NUM!");
    assert_eq!(model._get_text("A5"), *"#NUM!");
    assert_eq!(model._get_text("A6"), *"-536870912");
}
