use crate::test::util::{new_empty_model, new_empty_model_ref_1900};

// These tests are grouped because in many cases XOR and OR have similar behaviour.

// Test specific to xor
#[test]
fn fn_xor() {
    let mut model = new_empty_model();

    model._set("A1", "=XOR(1, 1, 1, 0, 0)");
    model._set("A2", "=XOR(1, 1, 0, 0, 0)");
    model._set("A3", "=XOR(TRUE, TRUE, TRUE, FALSE, FALSE)");
    model._set("A4", "=XOR(TRUE, TRUE, FALSE, FALSE, FALSE)");
    model._set("A5", "=XOR(FALSE, FALSE, FALSE, FALSE, FALSE)");
    model._set("A6", "=XOR(TRUE, TRUE)");
    model._set("A7", "=XOR(0,0,0)");
    model._set("A8", "=XOR(0,0,1)");
    model._set("A9", "=XOR(0,1,0)");
    model._set("A10", "=XOR(0,1,1)");
    model._set("A11", "=XOR(1,0,0)");
    model._set("A12", "=XOR(1,0,1)");
    model._set("A13", "=XOR(1,1,0)");
    model._set("A14", "=XOR(1,1,1)");

    model.evaluate();

    assert_eq!(model._get_text("A1"), *"TRUE");
    assert_eq!(model._get_text("A2"), *"FALSE");
    assert_eq!(model._get_text("A3"), *"TRUE");
    assert_eq!(model._get_text("A4"), *"FALSE");
    assert_eq!(model._get_text("A5"), *"FALSE");
    assert_eq!(model._get_text("A6"), *"FALSE");
    assert_eq!(model._get_text("A7"), *"FALSE");
    assert_eq!(model._get_text("A8"), *"TRUE");
    assert_eq!(model._get_text("A9"), *"TRUE");
    assert_eq!(model._get_text("A10"), *"FALSE");
    assert_eq!(model._get_text("A11"), *"TRUE");
    assert_eq!(model._get_text("A12"), *"FALSE");
    assert_eq!(model._get_text("A13"), *"FALSE");
    assert_eq!(model._get_text("A14"), *"TRUE");
}
#[test]
fn fn_or() {
    let mut model = new_empty_model();

    model._set("A1", "=OR(1, 1, 1, 0, 0)");
    model._set("A2", "=OR(1, 1, 0, 0, 0)");
    model._set("A3", "=OR(TRUE, TRUE, TRUE, FALSE, FALSE)");
    model._set("A4", "=OR(TRUE, TRUE, FALSE, FALSE, FALSE)");
    model._set("A5", "=OR(FALSE, FALSE, FALSE, FALSE, FALSE)");
    model._set("A6", "=OR(TRUE, TRUE)");

    model.evaluate();

    assert_eq!(model._get_text("A1"), *"TRUE");
    assert_eq!(model._get_text("A2"), *"TRUE");
    assert_eq!(model._get_text("A3"), *"TRUE");
    assert_eq!(model._get_text("A4"), *"TRUE");
    assert_eq!(model._get_text("A5"), *"FALSE");
    assert_eq!(model._get_text("A6"), *"TRUE");
}
#[test]
fn fn_or_xor_missing_arguments() {
    inner("or");
    inner("xor");

    fn inner(func: &str) {
        println!("Testing function: {func}");

        let mut model = new_empty_model();
        model._set("A1", &format!("={func}(,)"));
        model._set("A2", &format!("={func}(,1)"));
        model._set("A3", &format!("={func}(1,)"));
        model._set("A4", &format!("={func}(,B1)"));
        model._set("A5", &format!("={func}(,B1:B4)"));
        model.evaluate();
        assert_eq!(model._get_text("A1"), *"FALSE");
        assert_eq!(model._get_text("A2"), *"TRUE");
        assert_eq!(model._get_text("A3"), *"TRUE");
        assert_eq!(model._get_text("A4"), *"FALSE");
        assert_eq!(model._get_text("A5"), *"FALSE");
    }
}
#[test]
fn fn_and_or_evaluate_every_argument() {
    // the reference app has no short circuit: an error after the deciding argument is the result.
    // The shape of the vendor's Loan Amortization template (cell E18, saved by the reference app as #VALUE!
    // with its inputs empty): every input is "" or empty, so B1+C1 is #VALUE!.
    let mut model = new_empty_model();
    model._set("A1", r#"="""#);
    model._set("B1", r#"="""#);
    model._set("D1", r#"="""#);
    model._set(
        "E1",
        r#"=IF(AND(A1<>"",B1+C1<D1),C1,IF(AND(A1<>"",D1-B1>0),D1-B1,IF(A1<>"",0,"")))"#,
    );
    model._set("E2", r#"=AND(A1<>"",B1+C1<D1)"#);
    model._set("E3", r#"=OR(A1="",B1+C1<D1)"#);
    model._set("E4", r#"=IFERROR(AND(A1<>"",B1+C1<D1),"error")"#);
    // Without an error the result is the same as before.
    model._set("E5", r#"=AND(A1<>"",TRUE)"#);
    model._set("E6", r#"=OR(A1="",FALSE)"#);

    model.evaluate();

    assert_eq!(model._get_text("E1"), *"#VALUE!");
    assert_eq!(model._get_text("E2"), *"#VALUE!");
    assert_eq!(model._get_text("E3"), *"#VALUE!");
    assert_eq!(model._get_text("E4"), *"error");
    assert_eq!(model._get_text("E5"), *"FALSE");
    assert_eq!(model._get_text("E6"), *"TRUE");
}
