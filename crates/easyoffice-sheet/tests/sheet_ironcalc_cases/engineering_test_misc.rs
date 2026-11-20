use crate::test::util::{new_empty_model, new_empty_model_ref_1900};

#[test]
fn fn_delta_misc() {
    let mut model = new_empty_model();
    model._set("A1", "=3+1e-16");
    model._set("A2", "=3");
    model._set("A3", "=3+1e-15");
    model._set("B1", "=DELTA(A1, A2)");
    model._set("B2", "=DELTA(A1, A3)");

    model._set("B1", "1");
    model._set("B2", "0");

    model.evaluate();

    assert_eq!(model._get_text("B1"), *"1");
}
