use crate::test::util::new_empty_model;

#[test]
fn test_inverted_range_sum() {
    let mut model = new_empty_model();
    model._set("B2", "1");
    model._set("B3", "2");
    model._set("B4", "3");

    model._set("A1", "=SUM(B2:B4)");
    model._set("A2", "=SUM(B4:B2)");

    model.evaluate();

    assert_eq!(model._get_text("A1"), "6");
    assert_eq!(model._get_text("A2"), "6");
}
#[test]
fn test_mixed_absolute_relative_range() {
    let mut model = new_empty_model();
    model._set("B2", "10");
    model._set("B3", "20");
    model._set("B4", "30");
    model._set("B5", "40");

    model._set("A5", "=SUM(B$2:B5)");
    model._set("C5", "=SUM(B5:B$2)");

    model.evaluate();

    assert_eq!(model._get_text("A5"), "100");
    assert_eq!(model._get_text("C5"), "100");
}
#[test]
fn test_inverted_range_absolute_column() {
    let mut model = new_empty_model();
    model._set("B2", "5");
    model._set("C2", "10");
    model._set("D2", "15");

    model._set("A1", "=SUM($B2:D2)");
    model._set("A2", "=SUM(D2:$B2)");

    model.evaluate();

    assert_eq!(model._get_text("A1"), "30");
    assert_eq!(model._get_text("A2"), "30");
}
#[test]
fn test_inverted_2d_range() {
    let mut model = new_empty_model();
    model._set("B2", "1");
    model._set("B3", "2");
    model._set("C2", "3");
    model._set("C3", "4");

    model._set("A1", "=SUM(B2:C3)");
    model._set("A2", "=SUM(C3:B2)");
    model._set("A3", "=SUM(B3:C2)");
    model._set("A4", "=SUM(C2:B3)");

    model.evaluate();

    assert_eq!(model._get_text("A1"), "10");
    assert_eq!(model._get_text("A2"), "10");
    assert_eq!(model._get_text("A3"), "10");
    assert_eq!(model._get_text("A4"), "10");
}
