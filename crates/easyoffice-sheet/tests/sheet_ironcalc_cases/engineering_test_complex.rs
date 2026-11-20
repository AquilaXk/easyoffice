use crate::test::util::{new_empty_model, new_empty_model_ref_1900};

#[test]
fn fn_imabs() {
    let mut model = new_empty_model();
    model._set("A1", r#"=IMABS("3+4i")"#);

    model.evaluate();

    assert_eq!(model._get_text("A1"), "5");
}
#[test]
fn fn_imaginary() {
    let mut model = new_empty_model();
    model._set("A1", r#"=IMAGINARY("3+4i")"#);

    model.evaluate();

    assert_eq!(model._get_text("A1"), "4");
}
#[test]
fn fn_imreal() {
    let mut model = new_empty_model();
    model._set("A1", r#"=IMREAL("3+4i")"#);

    model.evaluate();

    assert_eq!(model._get_text("A1"), "3");
}
#[test]
fn fn_imconjugate() {
    let mut model = new_empty_model();
    model._set("A1", r#"=IMCONJUGATE("3+4i")"#);
    model._set("A2", r#"=IMCONJUGATE("12.7-32j")"#);

    model.evaluate();

    assert_eq!(model._get_text("A1"), "3-4i");
    assert_eq!(model._get_text("A2"), "12.7+32j");
}
#[test]
fn fn_imcos() {
    let mut model = new_empty_model();
    model._set("A1", r#"=IMCOS("4+3i")"#);
    // In macos non intel this is "-6.58066304055116+7.58155274274655i"
    model._set("A2", r#"=COMPLEX(-6.58066304055116, 7.58155274274654)"#);
    model._set("A3", r#"=IMABS(IMSUB(A1, A2)) < G1"#);

    // small number
    model._set("G1", "0.0000001");

    model.evaluate();

    assert_eq!(model._get_text("A3"), "TRUE");
}
#[test]
fn fn_imsin() {
    let mut model = new_empty_model();
    model._set("A1", r#"=IMSIN("4+3i")"#);

    model.evaluate();

    assert_eq!(model._get_text("A1"), "-7.61923172032141-6.548120040911i");
}
#[test]
fn fn_imaginary_misc() {
    let mut model = new_empty_model();
    model._set("A1", r#"=IMAGINARY("3.4i")"#);
    model._set("A2", r#"=IMAGINARY("-3.4")"#);

    model.evaluate();

    assert_eq!(model._get_text("A1"), "3.4");
    assert_eq!(model._get_text("A2"), "0");
}
#[test]
fn fn_imcosh() {
    let mut model = new_empty_model();
    model._set("A1", r#"=IMCOSH("4+3i")"#);

    model.evaluate();

    assert_eq!(model._get_text("A1"), "-27.0349456030742+3.85115333481178i");
}
#[test]
fn fn_power() {
    let mut model = new_empty_model();
    model._set("A2", r#"=IMPOWER("4+3i", 3)"#);
    model._set("A3", r#"=IMABS(IMSUB(IMPOWER("-i", -3), "-1"))<G1"#);
    model._set("A3", r#"=IMABS(IMSUB(IMPOWER("-1", 0.5), "i"))<G1"#);

    model._set("A1", r#"=IMABS(IMSUB(B1, "-1"))<G1"#);
    model._set("B1", r#"=IMPOWER("i", 2)"#);

    // small number
    model._set("G1", "0.0000001");

    model.evaluate();

    assert_eq!(model._get_text("A1"), "TRUE");
    assert_eq!(model._get_text("A2"), "-44+117i");
    assert_eq!(model._get_text("A3"), "TRUE");
    assert_eq!(model._get_text("A3"), "TRUE");
}
