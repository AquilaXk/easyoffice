use crate::test::util::{new_empty_model, new_empty_model_ref_1900};

#[test]
fn test_trigonometric_identity() {
    let mut model = new_empty_model();
    model._set("A1", "=COTH(1)*CSCH(1)");
    model._set("A2", "=COSH(1)/(SINH(1))^2");
    model._set("A3", "=A1=A2");
    model.evaluate();

    assert_eq!(model._get_text("A3"), *"TRUE");
}
