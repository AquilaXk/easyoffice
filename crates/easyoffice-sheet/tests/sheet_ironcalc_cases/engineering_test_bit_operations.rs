use crate::test::util::{new_empty_model, new_empty_model_ref_1900};

// the reference app does not pass this test (g sheets does)
#[test]
fn fn_bitshift_overflow() {
    let mut model = new_empty_model();
    model._set("A1", "=BITRSHIFT(12, -53)");
    model._set("A2", "=BITLSHIFT(12, 53)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), *"#NUM!");
    assert_eq!(model._get_text("A2"), *"#NUM!");
}
