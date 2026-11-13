//! 참조 구현: IronCalc `base/src/test/test_fn_randarray.rs` (기준 커밋 6dc9e8bae4b8d066276f399de7b3e1cf991c220b)의
//! 자료와 기대값을 그대로 옮긴 시험(MIT OR Apache-2.0, Copyright 2023 EqualTo GmbH, 2023-present
//! IronCalc contributors, 2026-present IronCalc GmbH). 엔진 모델은 `dynamic_support`의 세션입니다.
//! 기대값이 스프레드시트 프로그램과 다른 곳은 `DIFFERS` 주석으로 표시하고 이 엔진의 값을 씁니다.
#![allow(clippy::unwrap_used)]

mod dynamic_support;
use dynamic_support::new_empty_model;

#[test]
fn test_randarray_default() {
    let mut model = new_empty_model();
    model._set("A1", "=RANDARRAY()");
    model.evaluate();
    let val: f64 = model._get_text("A1").parse().unwrap();
    assert!((0.0..1.0).contains(&val));
}

#[test]
fn test_randarray_shape() {
    let mut model = new_empty_model();
    model._set("A1", "=RANDARRAY(2,3)");
    model.evaluate();
    for row in ["A", "B"] {
        for col in ["1", "2", "3"] {
            let cell = format!(
                "{}{}",
                ["A", "B", "C"][col.parse::<usize>().unwrap() - 1],
                ["1", "2"][["A", "B"].iter().position(|&r| r == row).unwrap()]
            );
            let val: f64 = model._get_text(&cell).parse().unwrap();
            assert!((0.0..=1.0).contains(&val));
        }
    }
}

#[test]
fn test_randarray_range() {
    let mut model = new_empty_model();
    model._set("A1", "=RANDARRAY(1,1,5,10)");
    model.evaluate();
    let val: f64 = model._get_text("A1").parse().unwrap();
    assert!((5.0..=10.0).contains(&val));
}

#[test]
fn test_randarray_whole_number() {
    let mut model = new_empty_model();
    model._set("A1", "=RANDARRAY(1,1,1,6,TRUE)");
    model.evaluate();
    let val: f64 = model._get_text("A1").parse().unwrap();
    assert_eq!(val, val.floor());
    assert!((1.0..=6.0).contains(&val));
}

#[test]
fn test_randarray_invalid_range() {
    let mut model = new_empty_model();
    model._set("A1", "=RANDARRAY(1,1,10,5)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "#VALUE!");
}

#[test]
fn test_randarray_omit_rows() {
    // =RANDARRAY(,3) — rows omitted, defaults to 1
    let mut model = new_empty_model();
    model._set("A1", "=RANDARRAY(,3)");
    model.evaluate();
    // Should produce a 1×3 array: A1, B1, C1 have values; A2 is empty
    let a1: f64 = model._get_text("A1").parse().unwrap();
    let b1: f64 = model._get_text("B1").parse().unwrap();
    let c1: f64 = model._get_text("C1").parse().unwrap();
    assert!((0.0..=1.0).contains(&a1));
    assert!((0.0..=1.0).contains(&b1));
    assert!((0.0..=1.0).contains(&c1));
    assert_eq!(model._get_text("A2"), "");
}

#[test]
fn test_randarray_omit_cols() {
    // =RANDARRAY(3,) — cols omitted, defaults to 1
    let mut model = new_empty_model();
    model._set("A1", "=RANDARRAY(3,)");
    model.evaluate();
    // Should produce a 3×1 array: A1, A2, A3 have values; B1 is empty
    let a1: f64 = model._get_text("A1").parse().unwrap();
    let a2: f64 = model._get_text("A2").parse().unwrap();
    let a3: f64 = model._get_text("A3").parse().unwrap();
    assert!((0.0..=1.0).contains(&a1));
    assert!((0.0..=1.0).contains(&a2));
    assert!((0.0..=1.0).contains(&a3));
    assert_eq!(model._get_text("B1"), "");
}

#[test]
fn test_randarray_omit_rows_size() {
    // =RANDARRAY(,3) must produce exactly 1 row × 3 columns.
    let mut model = new_empty_model();
    model._set("A1", "=RANDARRAY(,3)");
    model.evaluate();
    // All three columns in row 1 must be present.
    assert!(model._get_text("A1").parse::<f64>().is_ok());
    assert!(model._get_text("B1").parse::<f64>().is_ok());
    assert!(model._get_text("C1").parse::<f64>().is_ok());
    // One column beyond the array must be empty.
    assert_eq!(model._get_text("D1"), "");
    // One row beyond the array must be empty.
    assert_eq!(model._get_text("A2"), "");
}

#[test]
fn test_randarray_omit_cols_size() {
    // =RANDARRAY(5,) must produce exactly 5 rows × 1 column.
    let mut model = new_empty_model();
    model._set("A1", "=RANDARRAY(5,)");
    model.evaluate();
    // All five rows in column A must be present.
    for row in ["A1", "A2", "A3", "A4", "A5"] {
        assert!(
            model._get_text(row).parse::<f64>().is_ok(),
            "{row} should be a number"
        );
    }
    // One row beyond the array must be empty.
    assert_eq!(model._get_text("A6"), "");
    // One column beyond the array must be empty.
    assert_eq!(model._get_text("B1"), "");
}

#[test]
fn test_randarray_omit_max() {
    // =RANDARRAY(1,1,2,) — max omitted, defaults to 1, but min=2 > max=1 → #VALUE!
    // Actually with max defaulting to 1 and min=2, min > max triggers error
    let mut model = new_empty_model();
    model._set("A1", "=RANDARRAY(1,1,2,)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "#VALUE!");
}

#[test]
fn test_randarray_omit_min() {
    // =RANDARRAY(1,1,,5) — min omitted (defaults to 0), max=5
    let mut model = new_empty_model();
    model._set("A1", "=RANDARRAY(1,1,,5)");
    model.evaluate();
    let val: f64 = model._get_text("A1").parse().unwrap();
    assert!((0.0..=5.0).contains(&val));
}

#[test]
fn test_randarray_omit_whole_number() {
    // =RANDARRAY(1,1,1,10,) — whole_number omitted (defaults to FALSE)
    let mut model = new_empty_model();
    model._set("A1", "=RANDARRAY(1,1,1,10,)");
    model.evaluate();
    let val: f64 = model._get_text("A1").parse().unwrap();
    assert!((1.0..=10.0).contains(&val));
    // decimal result expected; we can't assert it's not an integer since it might be by chance,
    // but we can at least confirm it doesn't error
}

#[test]
fn test_randarray_rows_exceeds_sheet_limit() {
    // LAST_ROW is 1_048_576; one beyond that must return #VALUE!
    let mut model = new_empty_model();
    model._set("A1", "=RANDARRAY(1048577, 1)");
    model.evaluate();
    // DIFFERS: an array past the sheet edge is `#SPILL!` in the spreadsheet program (the reference says `#VALUE!`).
    assert_eq!(model._get_text("A1"), "#SPILL!");
}

#[test]
fn test_randarray_cols_exceeds_sheet_limit() {
    // LAST_COLUMN is 16_384; one beyond that must return #VALUE!
    let mut model = new_empty_model();
    model._set("A1", "=RANDARRAY(1,16385)");
    model.evaluate();
    // DIFFERS: an array past the sheet edge is `#SPILL!` in the spreadsheet program (the reference says `#VALUE!`).
    assert_eq!(model._get_text("A1"), "#SPILL!");
}

#[test]
fn test_randarray_rows_cols_exceeds_max_size() {
    // If rows * columns exceeds MAX_SIZE (1_000_000), must return #ERROR!
    let mut model = new_empty_model();
    model._set("A1", "=RANDARRAY(1000,1001)");
    model.evaluate();
    // DIFFERS: the reference stops at 1,000,000 cells with `#ERROR!`; here the limit is the array budget (2,097,152 cells).
    assert!(model._get_text("A1").parse::<f64>().is_ok());
}

#[test]
fn test_randarray_zero_rows_returns_calc() {
    // rows=0 is a zero-size array, consistent with SEQUENCE(0) → #CALC!
    let mut model = new_empty_model();
    model._set("A1", "=RANDARRAY(0)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "#CALC!");
}

#[test]
fn test_randarray_zero_cols_returns_calc() {
    // cols=0 is a zero-size array, consistent with SEQUENCE(1,0) → #CALC!
    let mut model = new_empty_model();
    model._set("A1", "=RANDARRAY(1,0)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "#CALC!");
}

#[test]
fn test_randarray_negative_rows_returns_value() {
    // Negative row count is an invalid argument → #VALUE! (not #CALC!)
    let mut model = new_empty_model();
    model._set("A1", "=RANDARRAY(-1)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "#VALUE!");
}

#[test]
fn test_randarray_negative_cols_returns_value() {
    // Negative column count is an invalid argument → #VALUE! (not #CALC!)
    let mut model = new_empty_model();
    model._set("A1", "=RANDARRAY(1,-1)");
    model.evaluate();
    assert_eq!(model._get_text("A1"), "#VALUE!");
}
