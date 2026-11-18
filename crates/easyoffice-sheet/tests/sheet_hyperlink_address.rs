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
        "=IF(FALSE,HYPERLINK(A1),\"대기\")",
        "=HYPERLINK(A1)&\"x\"",
        "=UPPER(HYPERLINK(A1,\"열기\"))",
        "=SUM(A4)",
    ] {
        assert_eq!(address(&wb, formula), None, "{formula}");
    }
    assert_eq!(
        wb.hyperlink_address_at("없는 시트", None, "=HYPERLINK(A1)"),
        None
    );
}

#[test]
fn wrapped_hyperlink_and_defined_names_keep_the_selected_address() {
    let mut wb = book();
    wb.set_names(&[
        easyoffice_formats::DefinedName {
            name: "GoLink".into(),
            scope: None,
            refers_to: "=HYPERLINK(S!A1,\"바로가기\")".into(),
            comment: String::new(),
            hidden: false,
        },
        easyoffice_formats::DefinedName {
            name: "WrapLink".into(),
            scope: None,
            refers_to: "=LAMBDA(u, HYPERLINK(u, \"람다\"))".into(),
            comment: String::new(),
            hidden: false,
        },
    ])
    .unwrap();
    for (formula, expected) in [
        ("=IF(TRUE,HYPERLINK(A1))", "https://example.com/a"),
        (
            "=IFERROR(1/0,HYPERLINK('두 번째'!A1,\"대체\"))",
            "https://other.example/x",
        ),
        (
            "=IFS(FALSE,\"대기\",A4>3,HYPERLINK(A1,\"열기\"))",
            "https://example.com/a",
        ),
        (
            "=SWITCH(A4,1,\"하나\",5,HYPERLINK(A1,\"다섯\"))",
            "https://example.com/a",
        ),
        (
            "=CHOOSE(2,\"대기\",HYPERLINK('두 번째'!A1,\"둘\"))",
            "https://other.example/x",
        ),
        (
            "=LET(u, A1, HYPERLINK(u, \"열기\"))",
            "https://example.com/a",
        ),
        ("=GoLink", "https://example.com/a"),
        ("=WrapLink('두 번째'!A1)", "https://other.example/x"),
        (
            "=INDEX(HYPERLINK({\"https://a.example/\";\"https://b.example/\"},{\"A\";\"B\"}),2)",
            "https://b.example/",
        ),
    ] {
        assert_eq!(address(&wb, formula), ok(expected), "{formula}");
    }
    assert_eq!(
        address(&wb, "=IFS(TRUE,\"일반\",TRUE,HYPERLINK(A1,\"열기\"))"),
        None
    );
    assert_eq!(address(&wb, "=GoLink&\"!\""), None);
}

#[test]
fn array_argument_hyperlink_spills_and_toggles_link_flags_per_cell() {
    use easyoffice_sheet::{CellChange, EvaluatedCellValue, PackedCellCoordinate, RecalcSession};

    let mut wb = WorkbookEngine::new();
    wb.add_sheet("S");
    wb.set_cell("S", "A1", "TRUE").unwrap();
    wb.set_cell(
        "S",
        "B1",
        "=IF(A1, HYPERLINK(\"https://example.com/one\", \"동일문구\"), \"동일문구\")",
    )
    .unwrap();
    wb.set_cell(
        "S",
        "D1",
        "=VSTACK(\"머리글\", HYPERLINK(\"https://example.com/item/\"&SEQUENCE(3), \"항목\"&SEQUENCE(3)))",
    )
    .unwrap();

    let mut session = RecalcSession::new(wb).unwrap();
    let out1 = session.recalculate();
    let b1 = PackedCellCoordinate::parse_a1("B1").unwrap();
    let d1 = PackedCellCoordinate::parse_a1("D1").unwrap();
    let d2 = PackedCellCoordinate::parse_a1("D2").unwrap();
    let d3 = PackedCellCoordinate::parse_a1("D3").unwrap();
    let d4 = PackedCellCoordinate::parse_a1("D4").unwrap();

    fn flag_of(
        out: &easyoffice_sheet::RecalcOutcome,
        coord: PackedCellCoordinate,
    ) -> Option<(EvaluatedCellValue, bool)> {
        out.changed
            .iter()
            .find(|c| c.coord == coord)
            .map(|c| (c.value.clone(), c.hyperlink))
    }
    assert_eq!(
        flag_of(&out1, b1),
        Some((EvaluatedCellValue::Text("동일문구".into()), true))
    );
    assert_eq!(
        flag_of(&out1, d1),
        Some((EvaluatedCellValue::Text("머리글".into()), false))
    );
    assert_eq!(
        flag_of(&out1, d2),
        Some((EvaluatedCellValue::Text("항목1".into()), true))
    );
    assert_eq!(
        flag_of(&out1, d4),
        Some((EvaluatedCellValue::Text("항목3".into()), true))
    );
    let spills = session.spill_areas();
    assert_eq!(spills.len(), 1);
    assert_eq!(spills[0].links, vec![false, true, true, true]);

    assert_eq!(session.hyperlink_address_for_cell("S", d1, None, ""), None);
    assert_eq!(
        session.hyperlink_address_for_cell("S", d2, None, ""),
        ok("https://example.com/item/1")
    );
    assert_eq!(
        session.hyperlink_address_for_cell("S", d3, None, ""),
        ok("https://example.com/item/2")
    );
    assert_eq!(
        session.hyperlink_address_for_cell("S", d4, None, ""),
        ok("https://example.com/item/3")
    );

    // Flipping A1 to FALSE keeps B1's text ("동일문구") identical, yet ChangedCell must report hyperlink: false!
    session
        .apply(&[CellChange {
            sheet: "S".into(),
            coord: PackedCellCoordinate::parse_a1("A1").unwrap(),
            raw: "FALSE".into(),
        }])
        .unwrap();
    let out2 = session.recalculate();
    assert_eq!(
        flag_of(&out2, b1),
        Some((EvaluatedCellValue::Text("동일문구".into()), false))
    );
    assert_eq!(session.hyperlink_address_for_cell("S", b1, None, ""), None);
}

#[test]
#[ignore = "measurement"]
fn bench_hyperlink_spill_10k() {
    use easyoffice_sheet::{PackedCellCoordinate, RecalcSession};
    use std::time::Instant;

    let mut recalc_us = Vec::new();
    let mut click_us = Vec::new();
    for _ in 0..7 {
        let mut wb = WorkbookEngine::new();
        wb.add_sheet("S");
        wb.set_cell(
            "S",
            "A1",
            "=HYPERLINK(\"https://example.com/item/\"&SEQUENCE(10000), \"항목\"&SEQUENCE(10000))",
        )
        .unwrap();
        let mut session = RecalcSession::new(wb).unwrap();
        let t0 = Instant::now();
        let out = session.recalculate();
        recalc_us.push(t0.elapsed().as_micros());
        assert_eq!(out.changed.len(), 10_000);
        assert!(out.changed.iter().all(|c| c.hyperlink));
        let target_cell = PackedCellCoordinate::parse_a1("A9999").unwrap();
        let t1 = Instant::now();
        let addr = session.hyperlink_address_for_cell("S", target_cell, None, "");
        click_us.push(t1.elapsed().as_micros());
        assert_eq!(addr, ok("https://example.com/item/9999"));
    }
    recalc_us.sort_unstable();
    click_us.sort_unstable();
    println!(
        "hyperlink_10k_recalc_p50_us={} click_p50_us={} changed_cell_bytes={}",
        recalc_us[3],
        click_us[3],
        std::mem::size_of::<easyoffice_sheet::ChangedCell>()
    );
}
