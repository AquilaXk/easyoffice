use crate::test::util::new_empty_model;

#[test]
fn simple_colum() {
    let mut model = new_empty_model();
    model._set("A1", "1");
    model._set("A2", "2");
    model._set("A3", "3");

    model._set("C2", "=@A1:A3");

    model.evaluate();

    assert_eq!(model._get_text("C2"), "2".to_string());
}
#[test]
fn concat() {
    let mut model = new_empty_model();
    model._set("A1", "=CONCAT(@B1:B3)");
    model._set("A2", "=CONCAT(B1:B3)");
    model._set("B1", "Hello");
    model._set("B2", " ");
    model._set("B3", "world!");

    model.evaluate();

    assert_eq!(model._get_text("A1"), *"Hello");
    assert_eq!(model._get_text("A2"), *"Hello world!");
}
#[test]
fn scalar_context_unwraps_1x1_array_from_offset() {
    let mut model = new_empty_model();
    model._set("B1", "10");
    model._set("B2", "20");
    model._set("B3", "30");

    model._set("A1", "=2 * IF(TRUE, OFFSET(B1, 2, 0), 0)");

    model.evaluate();

    assert_eq!(model._get_text("A1"), "60".to_string());
}
#[test]
fn scalar_context_unwraps_1x1_array_from_offset_for_dependents() {
    let mut model = new_empty_model();
    model._set("B1", "10");
    model._set("B2", "20");
    model._set("B3", "30");

    model._set("A1", "=IF(TRUE, OFFSET(B1, 2, 0), 0)");
    model._set("C1", "=A1 + 1");
    model._set("D1", "=A1");

    model.evaluate();

    assert_eq!(model._get_text("A1"), "30".to_string());
    assert_eq!(model._get_text("C1"), "31".to_string());
    assert_eq!(model._get_text("D1"), "30".to_string());
}
#[test]
fn at_operator_on_cross_sheet_single_cell_dereferences() {
    let mut model = new_empty_model();
    model.new_sheet();
    model._set("Sheet2!D2", "=42");
    model._set("A1", "=@Sheet2!D2");
    model.evaluate();

    assert_eq!(model._get_text("A1"), *"42");
}
#[test]
fn at_operator_on_offset_to_other_sheet_dereferences() {
    let mut model = new_empty_model();
    model.new_sheet();
    model._set("Sheet2!C3", "=7");
    model._set("A1", "=@OFFSET(Sheet2!A1, 2, 2)");
    model.evaluate();

    assert_eq!(model._get_text("A1"), *"7");
}
#[test]
fn at_operator_on_offset_indirect_cross_sheet_editpnl_shape() {
    let mut model = new_empty_model();
    model.new_sheet();
    model._set("Sheet2!B1", "=100");
    model._set("A1", "A1");
    model._set(
        "B1",
        "=IFERROR(@OFFSET(INDIRECT(\"Sheet2!\"&A1), 0, 1)/1, -999)",
    );
    model.evaluate();

    assert_eq!(model._get_text("B1"), *"100");
}
#[test]
fn at_operator_on_same_sheet_single_cell_still_works() {
    let mut model = new_empty_model();
    model._set("D2", "=42");
    model._set("A1", "=@D2");
    model.evaluate();

    assert_eq!(model._get_text("A1"), *"42");
}
#[test]
fn at_operator_row_aligned_intersection_across_sheets() {
    let mut model = new_empty_model();
    model.new_sheet();
    model._set("Sheet2!D3", "=55");
    model._set("C3", "=@Sheet2!D1:D5");
    model.evaluate();

    assert_eq!(model._get_text("C3"), *"55");
}
#[test]
fn choose_arm_at_wrapped_scalar_if_flows_through() {
    let mut model = new_empty_model();
    model._set("A1", "10");
    model._set("A2", "20");
    model._set(
        "C1",
        "=IFERROR(CHOOSE(1, @IF(TRUE, 5, AVERAGE(A1:A2)), 99), -1)",
    );
    model.evaluate();

    assert_eq!(model._get_text("C1"), "5".to_string());
}
#[test]
fn choose_arm_at_wrapped_scalar_cellref_flows_through() {
    let mut model = new_empty_model();
    model._set("A1", "10");
    model._set("A2", "20");
    model._set(
        "C1",
        "=IFERROR(CHOOSE(1, @IF(TRUE, A1, AVERAGE(A1:A2)), 99), -777)",
    );
    model.evaluate();

    assert_eq!(model._get_text("C1"), "10".to_string());
}
#[test]
fn at_range_in_reference_context_intersects() {
    let mut model = new_empty_model();
    model._set("B1", "1");
    model._set("B2", "2");
    model._set("B3", "3");
    model._set("A2", "=SUM(CHOOSE(1, @B1:B3))");
    model.evaluate();

    assert_eq!(model._get_text("A2"), "2".to_string());
}
#[test]
fn formulatext_of_at_column_intersects() {
    let mut model = new_empty_model();
    model._set("A1", "=1+2");
    model._set("B1", "=FORMULATEXT(@A:A)");
    model.evaluate();

    assert_eq!(model._get_text("B1"), "=1+2".to_string());
}
#[test]
fn columns_of_at_range_intersects() {
    let mut model = new_empty_model();
    model._set("A1", "1");
    model._set("B1", "2");
    model._set("C1", "3");
    model._set("B3", "=COLUMNS(@A1:C1)");
    model.evaluate();

    assert_eq!(model._get_text("B3"), "1".to_string());
}
#[test]
fn array_literal_takes_top_left() {
    let mut model = new_empty_model();
    model._set("A1", "=@{1,2,3}");
    model._set("A2", "=@{\"a\",2;3,4}");
    model._set("A3", "=@{#N/A,1}");
    model._set("A4", "=@{1,2,3}+10");
    model._set("A5", "=SUM(@{5,2,3})");
    model.evaluate();

    assert_eq!(model._get_text("A1"), "1");
    assert_eq!(model._get_text("A2"), "a");
    assert_eq!(model._get_text("A3"), "#N/A");
    assert_eq!(model._get_text("A4"), "11");
    assert_eq!(model._get_text("A5"), "5");
    assert_eq!(model._get_text("B1"), "");
}
#[test]
fn computed_array_takes_top_left() {
    let mut model = new_empty_model();
    model._set("B1", "1");
    model._set("B2", "2");
    model._set("A1", "=@SEQUENCE(3)");
    model._set("A2", "=@({1,2,3}*2)");
    model._set("A3", "=LET(x,{7,8},@x)");
    model._set("A9", "=@(B1:B2*3)");
    model.evaluate();

    assert_eq!(model._get_text("A1"), "1");
    assert_eq!(model._get_text("A2"), "2");
    assert_eq!(model._get_text("A3"), "7");
    assert_eq!(model._get_text("A9"), "3");
}
