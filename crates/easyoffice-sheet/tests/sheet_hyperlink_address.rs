//! The address a `HYPERLINK` formula opens (PD-223): the first argument evaluated on its sheet under the rules of
//! the function, and nothing for a formula that is not such a call.
#![allow(clippy::unwrap_used)]

use easyoffice_sheet::{FormulaError, WorkbookEngine};

fn book() -> WorkbookEngine {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("S");
    wb.add_sheet("두 번째");
    for (sheet, cell, raw) in [
        ("S", "A1", "https://example.com/a"),
        ("S", "A2", "문서 ID"),
        ("S", "A3", "javascript:alert(1)"),
        ("S", "A4", "5"),
        ("두 번째", "A1", "https://other.example/x"),
    ] {
        wb.set_cell(sheet, cell, raw).unwrap();
    }
    wb
}

fn address(wb: &WorkbookEngine, formula: &str) -> Option<Result<String, FormulaError>> {
    wb.hyperlink_address_at("S", None, formula)
}

fn ok(text: &str) -> Option<Result<String, FormulaError>> {
    Some(Ok(text.to_string()))
}

#[test]
fn the_address_is_the_first_argument_as_the_function_reads_it() {
    let wb = book();
    assert_eq!(
        address(&wb, "=HYPERLINK(\"https://a.b/c\",\"열기\")"),
        ok("https://a.b/c")
    );
    assert_eq!(address(&wb, "=hyperlink(A1)"), ok("https://example.com/a"));
    assert_eq!(
        address(&wb, " =HYPERLINK(A1, A2) "),
        ok("https://example.com/a")
    );
    assert_eq!(
        address(&wb, "=HYPERLINK(\"https://x.y/?n=\"&A4,\"go\")"),
        ok("https://x.y/?n=5")
    );
    assert_eq!(
        address(&wb, "=HYPERLINK('두 번째'!A1,\"다른 시트의 칸\")"),
        ok("https://other.example/x")
    );
    assert_eq!(
        address(&wb, "=HYPERLINK(\"#'두 번째'!B2\")"),
        ok("#'두 번째'!B2")
    );
    assert_eq!(
        address(&wb, "=HYPERLINK(\"mailto:me@example.com\")"),
        ok("mailto:me@example.com")
    );
    assert_eq!(
        address(
            &wb,
            "=HYPERLINK(IF(A4>1,\"https://big.example/\",\"https://small.example/\"),\"x\")"
        ),
        ok("https://big.example/")
    );
}

#[test]
fn an_address_the_policy_refuses_or_an_error_gives_the_functions_error() {
    let wb = book();
    for formula in [
        "=HYPERLINK(\"javascript:alert(1)\",\"x\")",
        "=HYPERLINK(A3)",
        "=HYPERLINK(\"file:///etc/passwd\")",
        "=HYPERLINK(\"data:text/html,x\")",
        "=HYPERLINK(\"\",\"x\")",
        "=HYPERLINK(\"ftp://a.b/\")",
        "=HYPERLINK(\"https://x.y/?q=\"&A2,\"go\")",
        "=HYPERLINK(\"#\")",
    ] {
        assert_eq!(
            address(&wb, formula),
            Some(Err(FormulaError::Value)),
            "{formula}"
        );
    }
    assert_eq!(
        address(&wb, "=HYPERLINK(1/0,\"x\")"),
        Some(Err(FormulaError::DivByZero))
    );
    assert_eq!(
        address(&wb, "=HYPERLINK(NOPE(1),\"x\")"),
        Some(Err(FormulaError::Name))
    );
}

#[test]
fn a_formula_that_is_not_a_hyperlink_call_has_no_address() {
    let wb = book();
    for formula in [
        "=1+1",
        "=A1",
        "text",
        "",
        "=HYPERLINK()",
        "=HYPERLINK(A1,A2,A3)",
        "=IF(TRUE,HYPERLINK(A1))",
        "=HYPERLINK(A1)&\"x\"",
        "=SUM(A4)",
    ] {
        assert_eq!(address(&wb, formula), None, "{formula}");
    }
    assert_eq!(
        wb.hyperlink_address_at("없는 시트", None, "=HYPERLINK(A1)"),
        None
    );
}
