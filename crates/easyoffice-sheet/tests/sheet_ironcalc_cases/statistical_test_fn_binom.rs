use crate::test::util::{new_empty_model, new_empty_model_ref_1900};

#[test]
fn test_fn_binom_inv_small_trials_and_probabilities() {
    // These used to panic inside statrs' `inverse_cdf`.
    // Expected values: smallest k with CDF(k) >= alpha (scipy.stats.binom.ppf).
    let cases = [
        ("=BINOM.INV(1, 0.001, 0.1)", "0"),
        ("=BINOM.INV(1, 0.001, 0.9999)", "1"),
        ("=BINOM.INV(1, 0.5, 0.6)", "1"),
        ("=BINOM.INV(10, 0.01, 0.5)", "0"),
        ("=BINOM.INV(10, 0.01, 0.999)", "2"),
        ("=BINOM.INV(50, 0.001, 0.999)", "2"),
        ("=BINOM.INV(20, 0.3, 0.05)", "3"),
        ("=BINOM.INV(100, 0.99, 0.01)", "96"),
        ("=BINOM.INV(1000, 0.5, 0.5)", "500"),
        ("=BINOM.INV(1000000, 0.3, 0.9)", "300587"),
    ];
    let mut model = new_empty_model();
    for (row, (formula, _)) in cases.iter().enumerate() {
        model._set(&format!("A{}", row + 1), formula);
    }
    model.evaluate();
    for (row, (formula, expected)) in cases.iter().enumerate() {
        assert_eq!(
            model._get_text(&format!("A{}", row + 1)),
            *expected,
            "{formula}"
        );
    }
}
#[test]
fn test_booleans() {
    let mut model = new_empty_model();
    model._set("A1", "=BINOM.DIST(1,TRUE,0.7,TRUE)");
    model._set("A2", "=BINOM.DIST.RANGE(TRUE,0.3,0,1)");
    model._set("A3", "=BINOM.INV(TRUE,0.5,0.8)");
    model._set("A4", "=NEGBINOM.DIST(0,TRUE,0.7,TRUE)");

    model.evaluate();

    assert_eq!(model._get_text("A1"), *"1");
    assert_eq!(model._get_text("A2"), *"1");
    assert_eq!(model._get_text("A3"), *"1");
    assert_eq!(model._get_text("A4"), *"0.7");
}
