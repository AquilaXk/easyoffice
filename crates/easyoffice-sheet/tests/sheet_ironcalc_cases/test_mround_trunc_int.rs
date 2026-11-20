#![allow(clippy::unwrap_used)]

use crate::test::util::new_empty_model;

#[test]
fn arguments() {
    let mut model = new_empty_model();
    model._set("A1", "=MROUND()");
    model._set("A2", "=MROUND(10)");
    model._set("A3", "=MROUND(10, 3)");
    model._set("A4", "=MROUND(10, 3, 1)");

    model._set("A5", "=TRUNC()");
    model._set("A6", "=TRUNC(10)");
    model._set("A7", "=TRUNC(10.22, 1)");
    model._set("A8", "=TRUNC(10, 3, 1)");

    model._set("A9", "=INT()");
    model._set("A10", "=INT(10.22)");
    model._set("A11", "=INT(10.22, 1)");
    model._set("A12", "=INT(10.22, 1, 2)");

    model.evaluate();

    assert_eq!(model._get_text("A3"), *"9");

    assert_eq!(model._get_text("A6"), *"10");
    assert_eq!(model._get_text("A7"), *"10.2");

    assert_eq!(model._get_text("A10"), *"10");
}
