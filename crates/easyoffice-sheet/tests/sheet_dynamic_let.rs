//! 참조 구현: IronCalc `base/src/test/test_fn_let.rs` (기준 커밋 6dc9e8bae4b8d066276f399de7b3e1cf991c220b)의
//! 자료와 기대값을 그대로 옮긴 시험(MIT OR Apache-2.0, Copyright 2023 EqualTo GmbH, 2023-present
//! IronCalc contributors, 2026-present IronCalc GmbH). 엔진 모델은 `dynamic_support`의 세션입니다.
//! 기대값이 스프레드시트 프로그램과 다른 곳은 `DIFFERS` 주석으로 표시하고 이 엔진의 값을 씁니다.
#![allow(clippy::unwrap_used)]

mod dynamic_support;
use dynamic_support::new_empty_model;

#[test]
fn test_let_wrong_arg_count() {
    let mut model = new_empty_model();
    // Too few args
    model._set("A1", "=LET(x,1)");
    model.evaluate();
    // DIFFERS: the reference answers a call with the wrong number of arguments with `#ERROR!`.
    assert_eq!(model._get_text("A1"), "#VALUE!");

    // Even number of args
    model._set("A2", "=LET(x,1,y,2)");
    model.evaluate();
    assert_eq!(model._get_text("A2"), "#VALUE!");
}

#[test]
fn test_let_two_variables() {
    let mut model = new_empty_model();
    // LET(x, 2, y, 4, x+y) → 6
    model._set("A1", "=LET(x,2,y,4,x+y)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "6");
}

#[test]
fn test_let_with_cell_reference() {
    let mut model = new_empty_model();
    model._set("A1", "5");
    // LET(x, A1, SUM(x*3, x)) → SUM(15, 5) = 20
    model._set("B1", "=LET(x,A1,SUM(x*3,x))");
    model.evaluate();
    assert_eq!(model._get_text("B1"), "20");
}

#[test]
fn test_let_nested() {
    let mut model = new_empty_model();
    // LET(x, 3, LET(x, 4, x*5)*x) → inner x=4: 4*5=20, outer x=3: 20*3=60
    model._set("A1", "=LET(x,3,LET(x,4,x*5)*x)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "60");
}

#[test]
fn test_let_inner_binding_references_outer_value() {
    let mut model = new_empty_model();
    // LET(x,1,LET(x,x+1,x)) → inner x is bound from outer x+1 = 2, result = 2
    model._set("A1", "=LET(x,1,LET(x,x+1,x))");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "2");
}
#[test]
fn test_let_rebind_same_name_within_single_let() {
    let mut model = new_empty_model();
    model._set("A1", "=LET(x,1,x,2,x)");
    model.evaluate();
    // DIFFERS: the reference refuses a name bound twice in one LET; here the later binding shadows the earlier.
    assert_eq!(model._get_text("A1"), "2");
}

#[test]
fn test_let_r_and_c_variables() {
    let mut model = new_empty_model();
    // r and c must work as LET variable names
    model._set("A1", "=LET(r,3,c,4,r+c)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "7");
}

#[test]
fn test_lambda_r_and_c_parameters() {
    let mut model = new_empty_model();
    // r and c must work as LAMBDA parameter names
    model._set("A1", "=LAMBDA(r,c,r+c)(3,4)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "7");
}
