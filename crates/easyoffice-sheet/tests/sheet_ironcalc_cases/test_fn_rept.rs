use crate::test::util::{new_empty_model, new_empty_model_ref_1900};

#[test]
fn simple_cases() {
    let mut model = new_empty_model();
    model._set("A1", "Well");

    model._set("B1", "=REPT(A1, 3)");

    model.evaluate();

    assert_eq!(model._get_text("B1"), *"WellWellWell");
}
