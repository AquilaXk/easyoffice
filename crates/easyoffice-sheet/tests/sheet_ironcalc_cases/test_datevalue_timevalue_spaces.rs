use crate::test::util::{new_empty_model, new_empty_model_ref_1900};

// the reference app ignores spaces around the time separator (#1402).
#[test]
fn timevalue_spaces_around_separators() {
    let mut model = new_empty_model();
    model._set("A1", "=TIMEVALUE(\"4 : 35\")");
    model._set("A2", "=TIMEVALUE(\"4:35\")");
    model._set("A3", "=TIMEVALUE(\"12 : 00 : 00\")");
    model._set("A4", "=TIMEVALUE(\"3 : 30 PM\")");
    model._set("A5", "=TIMEVALUE(\"3:30 PM\")");
    model.evaluate();

    assert_ne!(model._get_text("A1"), *"#VALUE!");
    assert_eq!(model._get_text("A1"), model._get_text("A2"));
    assert_eq!(model._get_text("A3"), *"0.5");
    assert_eq!(model._get_text("A4"), model._get_text("A5"));
}
