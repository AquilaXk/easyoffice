use crate::test::util::{new_empty_model, new_empty_model_ref_1900};

// =============================================================================
// Error conditions
// =============================================================================

#[test]
fn fn_accrintm_error_issue_after_settlement() {
    let mut model = new_empty_model();
    model._set("A1", "=ACCRINTM(39614, 39539, 0.1, 1000, 3)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), *"#NUM!");
}
#[test]
fn fn_accrintm_error_issue_equals_settlement() {
    // Boundary: same-day is error per the vendor spec, not zero accrual
    let mut model = new_empty_model();
    model._set("A1", "=ACCRINTM(39539, 39539, 0.1, 1000, 3)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), *"#NUM!");
}
#[test]
fn fn_accrintm_error_zero_rate() {
    // rate <= 0 → #NUM! (covers both zero and negative)
    let mut model = new_empty_model();
    model._set("A1", "=ACCRINTM(39539, 39614, 0, 1000, 3)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), *"#NUM!");
}
#[test]
fn fn_accrintm_error_zero_par() {
    // par <= 0 → #NUM! (covers both zero and negative)
    let mut model = new_empty_model();
    model._set("A1", "=ACCRINTM(39539, 39614, 0.1, 0, 3)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), *"#NUM!");
}
#[test]
fn fn_accrintm_error_invalid_basis() {
    // basis outside 0-4 → #NUM! (delegated to fn_yearfrac)
    let mut model = new_empty_model();
    model._set("A1", "=ACCRINTM(39539, 39614, 0.1, 1000, 5)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), *"#NUM!");
}
