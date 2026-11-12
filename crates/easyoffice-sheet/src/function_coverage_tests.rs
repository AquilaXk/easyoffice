//! Every canonical function has to be called somewhere in the function tests.

use crate::SheetEngine;

/// The sources that call the canonical functions: the evaluation test of the workbook tests and the
/// integration tests of the function bundles (PD-195, PD-201).
fn calling_sources() -> String {
    let workbook_tests = include_str!("workbook_tests.rs");
    let start = workbook_tests
        .find("fn test_all_canonical_functions_evaluated()")
        .unwrap();
    let end = workbook_tests[start..]
        .find("fn test_workbook_cross_sheet_formulas_and_sort_goal_seek_chart()")
        .unwrap()
        + start;
    [
        &workbook_tests[start..end],
        include_str!("../tests/sheet_univer_functions_a.rs"),
        include_str!("../tests/sheet_univer_functions_b_fin.rs"),
        include_str!("../tests/sheet_univer_functions_b_eng.rs"),
        include_str!("../tests/sheet_univer_functions_b_stat.rs"),
        include_str!("../tests/sheet_univer_functions_b_text_info.rs"),
    ]
    .concat()
}

#[test]
fn test_f12_canonical_coverage_completeness() {
    let body = calling_sources();
    let missing: Vec<&str> = SheetEngine::supported_function_names()
        .iter()
        .copied()
        .filter(|name| !body.contains(&format!("{name}(")))
        .collect();
    assert!(
        missing.is_empty(),
        "Missing canonical functions in the function tests: {missing:?}"
    );
}
