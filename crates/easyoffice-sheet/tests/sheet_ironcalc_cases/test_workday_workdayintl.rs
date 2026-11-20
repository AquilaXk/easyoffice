use crate::test::util::{new_empty_model, new_empty_model_ref_1900};

#[test]
fn workday_intl_empty_weekend_argument() {
    let mut model = new_empty_model();
    model._set("A1", "=WORKDAY.INTL(DATE(2025,1,10),10,B1)");
    model._set("A2", "=WORKDAY.INTL(DATE(2025,1,10),10,)");
    model._set("A3", "=WORKDAY.INTL(DATE(2025,1,10),10)");

    model.evaluate();

    assert_eq!(model._get_text("A1"), *"#NUM!");
    assert_eq!(model._get_text("A2"), *"45681");
    assert_eq!(model._get_text("A3"), *"45681");
}
