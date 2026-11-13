//! 참조 구현: IronCalc `base/src/test/test_fn_isomitted.rs` (기준 커밋 01b39c1ad26252f7b6c810440e8433a932daee07)의
//! 자료와 기대값을 옮긴 시험(MIT OR Apache-2.0, Copyright 2023 EqualTo GmbH, 2023-present IronCalc
//! contributors, 2026-present IronCalc GmbH). 다른 곳은 `DIFFERS` 주석으로 표시합니다.
#![allow(clippy::unwrap_used)]

mod dynamic_support;
use dynamic_support::new_empty_model;

#[test]
fn test_isomitted_omitted_optional_arg_returns_true() {
    let mut model = new_empty_model();
    // [b] is optional; calling with only one arg leaves b as EmptyArg → TRUE
    model._set("A1", "=LAMBDA(a, [b], ISOMITTED(b))(12)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), *"TRUE");
}

#[test]
fn test_isomitted_provided_arg_returns_false() {
    let mut model = new_empty_model();
    model._set("A1", "=LAMBDA(a, [b], ISOMITTED(b))(12, 3)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), *"FALSE");
}

#[test]
fn test_isomitted_empty_string_is_not_omitted() {
    let mut model = new_empty_model();
    model._set("A1", "=LAMBDA(a, [b], ISOMITTED(b))(12, \"\")");
    model.evaluate();
    assert_eq!(model._get_text("A1"), *"FALSE");
}

#[test]
fn test_isomitted_zero_is_not_omitted() {
    let mut model = new_empty_model();
    model._set("A1", "=LAMBDA(a, [b], ISOMITTED(b))(12, 0)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), *"FALSE");
}

#[test]
fn test_isomitted_wrong_arg_count() {
    let mut model = new_empty_model();
    model._set("A1", "=ISOMITTED()");
    model._set("A2", "=ISOMITTED(1, 2)");
    model.evaluate();
    // DIFFERS: the reference answers a call with the wrong number of arguments with `#ERROR!`.
    assert_eq!(model._get_text("A1"), *"#VALUE!");
    assert_eq!(model._get_text("A2"), *"#VALUE!");
}

#[test]
fn test_isomitted_optional_arg_default_pattern() {
    // Classic ISOMITTED usage: provide a default when the optional arg is missing.
    // LAMBDA(a, [b], IF(ISOMITTED(b), a/2, a*b))
    let mut model = new_empty_model();
    model._set("A1", "=LAMBDA(a, [b], IF(ISOMITTED(b), a/2, a*b))(12)");
    model._set("A2", "=LAMBDA(a, [b], IF(ISOMITTED(b), a/2, a*b))(12, 3)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), *"6");
    assert_eq!(model._get_text("A2"), *"36");
}
