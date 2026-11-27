//! `IF` and `CHOOSE` that may pass a reference on (`SUM(IF(c, A1:A3, B1:B3))`) look at their branches
//! before they evaluate the condition. A call whose branches hold plain values is evaluated as a value,
//! and a condition that was evaluated while looking for a reference must not be evaluated again: nested
//! in each other the cost doubled at every level.
#![allow(clippy::unwrap_used)]

mod dynamic_support;
use dynamic_support::new_empty_model;
use std::time::{Duration, Instant};

fn chain(wrap: &str, depth: usize) -> String {
    let mut f = "1".to_string();
    for _ in 0..depth {
        f = wrap.replace("{}", &f);
    }
    format!("={f}")
}

fn timed(formula: &str) -> (String, Duration) {
    let mut m = new_empty_model();
    for r in 1..=3 {
        m._set(&format!("A{r}"), &r.to_string());
    }
    m._set("C1", formula);
    let started = Instant::now();
    m.evaluate();
    (m._get_text("C1"), started.elapsed())
}

#[test]
fn nested_if_and_choose_conditions_are_evaluated_once() {
    for wrap in ["SUM(IF({}=1,1,2))", "SUM(CHOOSE({},1,2))", "SUM(IF({}=1,1))"] {
        // 28 levels of two calls stay under the 64 levels a formula may nest; 2^28 evaluations would not finish.
        let (value, took) = timed(&chain(wrap, 28));
        assert_eq!(value, "1", "{wrap}");
        assert!(took < Duration::from_secs(2), "{wrap} took {took:?}");
    }
}

#[test]
fn a_branch_that_is_a_reference_still_passes_on_as_one() {
    let mut m = new_empty_model();
    for r in 1..=3 {
        m._set(&format!("A{r}"), &r.to_string());
        m._set(&format!("B{r}"), &(r * 10).to_string());
    }
    m._set("D1", "=SUM(IF(TRUE,A1:A3,B1:B3))");
    m._set("D2", "=SUM(IF(FALSE,A1:A3,B1:B3))");
    m._set("D3", "=SUM(CHOOSE(2,A1:A3,B1:B3))");
    m._set("D4", "=SUM(IF(TRUE,5,B1:B3))");
    m._set("D5", "=SUM(CHOOSE(1,A1:A3,5))");
    m._set("D6", "=ROWS(IF(TRUE,A1:A3,5))");
    m._set("D7", "=SUM(IF(1/0,1,A1:A3))");
    m.evaluate();
    assert_eq!(m._get_text("D1"), "6");
    assert_eq!(m._get_text("D2"), "60");
    assert_eq!(m._get_text("D3"), "60");
    assert_eq!(m._get_text("D4"), "5");
    assert_eq!(m._get_text("D5"), "6");
    assert_eq!(m._get_text("D6"), "3");
    assert_eq!(m._get_text("D7"), "#DIV/0!");
}
