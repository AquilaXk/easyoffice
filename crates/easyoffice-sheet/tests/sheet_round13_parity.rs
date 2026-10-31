use easyoffice_sheet::{
    compute_sheet_id, EvaluatedCellValue, FormulaError, SheetEngine, WorkbookEngine,
};

#[test]
fn test_sheet_engine_preserves_edge_whitespace_and_sheet_id() {
    let mut sheet = SheetEngine::new(" 요약 ");
    assert_eq!(sheet.name, " 요약 ");
    sheet.set_cell("A1", "77").unwrap();
    assert_eq!(
        sheet.evaluate_formula_str("=' 요약 '!A1"),
        EvaluatedCellValue::Number(77.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=요약!A1"),
        EvaluatedCellValue::Error(FormulaError::Ref)
    );

    sheet.set_name("  2026 결산  ");
    assert_eq!(sheet.name, "  2026 결산  ");
    assert_ne!(compute_sheet_id(&sheet.name), compute_sheet_id("2026 결산"));
    assert_eq!(
        sheet.evaluate_formula_str("='  2026 결산  '!A1"),
        EvaluatedCellValue::Number(77.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=' 요약 '!A1"),
        EvaluatedCellValue::Error(FormulaError::Ref)
    );

    let blank1 = SheetEngine::new("");
    let blank2 = SheetEngine::new("   \t\n  ");
    assert_eq!(blank1.name, "Sheet1");
    assert_eq!(blank2.name, "Sheet1");

    sheet.set_name("   ");
    assert_eq!(sheet.name, "Sheet1");

    let mut wb = WorkbookEngine::new();
    wb.add_sheet(" 요약 ").set_cell("A1", "125").unwrap();
    wb.add_sheet("요약").set_cell("A1", "250").unwrap();
    assert_eq!(wb.sheet_names(), vec![" 요약 ", "요약"]);
    assert_eq!(
        wb.evaluate_formula_on_sheet("요약", "=' 요약 '!A1 + 요약!A1"),
        EvaluatedCellValue::Number(375.0)
    );
}

#[test]
fn test_indirect_second_arg_evaluation_and_error_propagation() {
    let mut sheet = SheetEngine::new("Sheet1");
    sheet.set_cell("A1", "15").unwrap();
    sheet.set_cell("A2", "25").unwrap();

    assert_eq!(
        sheet.evaluate_formula_str("=INDIRECT(\"A1\", TRUE)"),
        EvaluatedCellValue::Number(15.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=INDIRECT(\"A1\", 1)"),
        EvaluatedCellValue::Number(15.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=INDIRECT(\"A1\", \"TRUE\")"),
        EvaluatedCellValue::Number(15.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=SUM(INDIRECT(\"A1:A2\", TRUE))"),
        EvaluatedCellValue::Number(40.0)
    );

    assert_eq!(
        sheet.evaluate_formula_str("=INDIRECT(\"A1\", 1/0)"),
        EvaluatedCellValue::Error(FormulaError::DivByZero)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=INDIRECT(\"A1\", #N/A)"),
        EvaluatedCellValue::Error(FormulaError::Na)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=SUM(INDIRECT(\"A1:A2\", 1/0))"),
        EvaluatedCellValue::Error(FormulaError::DivByZero)
    );

    assert_eq!(
        sheet.evaluate_formula_str("=INDIRECT(\"A1\", FALSE)"),
        EvaluatedCellValue::Error(FormulaError::Value)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=INDIRECT(\"A1\", 0)"),
        EvaluatedCellValue::Error(FormulaError::Value)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=INDIRECT(\"A1\", \"FALSE\")"),
        EvaluatedCellValue::Error(FormulaError::Value)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=INDIRECT(\"A1\", \"invalid\")"),
        EvaluatedCellValue::Error(FormulaError::Value)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=SUM(INDIRECT(\"A1:A2\", FALSE))"),
        EvaluatedCellValue::Error(FormulaError::Value)
    );
}

#[test]
fn test_indirect_sheet_repeated_ranges() {
    let mut wb = WorkbookEngine::new();
    {
        let s1 = wb.add_sheet("Sheet1");
        s1.set_cell("A1", "10").unwrap();
        s1.set_cell("A2", "20").unwrap();
    }
    {
        let sum_sheet = wb.add_sheet("Summary");
        sum_sheet.set_cell("A1", "12").unwrap();
        sum_sheet.set_cell("A2", "18").unwrap();
    }
    {
        let colon_sheet = wb.add_sheet("Q1:Q2");
        colon_sheet.set_cell("A1", "30").unwrap();
        colon_sheet.set_cell("A2", "40").unwrap();
    }
    {
        let quoted_sheet = wb.add_sheet("'Summary'");
        quoted_sheet.set_cell("A1", "40").unwrap();
        quoted_sheet.set_cell("A2", "50").unwrap();
    }

    assert_eq!(
        wb.evaluate_formula_on_sheet("Sheet1", "=SUM(INDIRECT(\"'Summary'!A1:'Summary'!A2\"))"),
        EvaluatedCellValue::Number(30.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Summary", "=SUM(INDIRECT(\"Sheet1!A1:Sheet1!A2\"))"),
        EvaluatedCellValue::Number(30.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Sheet1", "=SUM(INDIRECT(\"'summary'!A1:SUMMARY!A2\"))"),
        EvaluatedCellValue::Number(30.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Sheet1", "=SUM(INDIRECT(\"'Q1:Q2'!A1:'Q1:Q2'!A2\"))"),
        EvaluatedCellValue::Number(70.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet(
            "Sheet1",
            "=SUM(INDIRECT(\"'''Summary'''!A1:'''Summary'''!A2\"))"
        ),
        EvaluatedCellValue::Number(90.0)
    );

    assert_eq!(
        wb.evaluate_formula_on_sheet("Sheet1", "=SUM(INDIRECT(\"Summary!A1:'''Summary'''!A2\"))"),
        EvaluatedCellValue::Error(FormulaError::Ref)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Sheet1", "=SUM(INDIRECT(\"Sheet1!A1:Summary!A2\"))"),
        EvaluatedCellValue::Error(FormulaError::Ref)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Sheet1", "=SUM(INDIRECT(\"A1:Sheet1!A2\"))"),
        EvaluatedCellValue::Error(FormulaError::Ref)
    );
}

#[test]
fn test_openformula_bracketed_sheet_quoting_parity() {
    let mut wb = WorkbookEngine::new();
    {
        let s = wb.add_sheet("2026Q1");
        s.set_cell("A1", "11").unwrap();
        s.set_cell("A2", "22").unwrap();
    }
    {
        let s = wb.add_sheet("1분기");
        s.set_cell("A1", "33").unwrap();
        s.set_cell("A2", "44").unwrap();
    }
    {
        let s = wb.add_sheet("TRUE");
        s.set_cell("A1", "55").unwrap();
        s.set_cell("A2", "66").unwrap();
    }
    {
        let s = wb.add_sheet("FALSE");
        s.set_cell("A1", "77").unwrap();
    }
    {
        let s = wb.add_sheet("A1");
        s.set_cell("B1", "88").unwrap();
        s.set_cell("B2", "99").unwrap();
    }
    wb.add_sheet("Main").set_cell("A1", "1").unwrap();

    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "of:=[2026Q1.A1]"),
        EvaluatedCellValue::Number(11.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "of:=[$2026Q1.$A$1]"),
        EvaluatedCellValue::Number(11.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "of:=[1분기.A1]"),
        EvaluatedCellValue::Number(33.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "of:=[TRUE.A1]"),
        EvaluatedCellValue::Number(55.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "of:=[FALSE.A1]"),
        EvaluatedCellValue::Number(77.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "of:=[A1.B2]"),
        EvaluatedCellValue::Number(99.0)
    );

    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "of:=SUM([2026Q1.A1:.A2])"),
        EvaluatedCellValue::Number(33.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "of:=SUM([1분기.A1:1분기.A2])"),
        EvaluatedCellValue::Number(77.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "of:=SUM([TRUE.A1:.A2])"),
        EvaluatedCellValue::Number(121.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "of:=SUM([A1.B1:A1.B2])"),
        EvaluatedCellValue::Number(187.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=INDIRECT(\"2026Q1!A1\")"),
        EvaluatedCellValue::Error(FormulaError::Ref)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=INDIRECT(\"'2026Q1'!A1\")"),
        EvaluatedCellValue::Number(11.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=INDIRECT(\"1분기!A1\")"),
        EvaluatedCellValue::Error(FormulaError::Ref)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=INDIRECT(\"'1분기'!A1\")"),
        EvaluatedCellValue::Number(33.0)
    );
    let c5 = easyoffice_sheet::PackedCellCoordinate::parse_a1("C5").unwrap();
    assert_eq!(
        wb.evaluate_formula_on_sheet_at("Main", Some(c5), "=ROW()"),
        EvaluatedCellValue::Number(5.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet_at("Main", Some(c5), "=COLUMN()"),
        EvaluatedCellValue::Number(3.0)
    );

    assert!(easyoffice_sheet::PackedCellCoordinate::parse_a1("$C$5").is_ok());
    assert!(easyoffice_sheet::PackedCellCoordinate::parse_a1("$$A1").is_err());
    assert!(easyoffice_sheet::PackedCellCoordinate::parse_a1("A$B1").is_err());
    assert!(easyoffice_sheet::PackedCellCoordinate::parse_a1("A1$").is_err());
    assert!(easyoffice_sheet::PackedCellCoordinate::parse_a1("C1$2").is_err());
    assert!(easyoffice_sheet::PackedCellCoordinate::parse_a1("A01").is_err());

    {
        let fw = wb.add_sheet("１분기");
        fw.set_cell("A1", "44").unwrap();
    }
    {
        let rn = wb.add_sheet("Ⅰ분기");
        rn.set_cell("A1", "66").unwrap();
    }
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "of:=[１분기.A1]"),
        EvaluatedCellValue::Number(44.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=INDIRECT(\"１분기!A1\")"),
        EvaluatedCellValue::Error(FormulaError::Ref)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=INDIRECT(\"'１분기'!A1\")"),
        EvaluatedCellValue::Number(44.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=Ⅰ분기!A1"),
        EvaluatedCellValue::Number(66.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=INDIRECT(\"Ⅰ분기!A1\")"),
        EvaluatedCellValue::Number(66.0)
    );

    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=A01"),
        EvaluatedCellValue::Error(FormulaError::Ref)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=XFE1"),
        EvaluatedCellValue::Error(FormulaError::Ref)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=A01:B2"),
        EvaluatedCellValue::Error(FormulaError::Ref)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=IF(TRUE, 42, A01)"),
        EvaluatedCellValue::Number(42.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=IFERROR(A01, 99)"),
        EvaluatedCellValue::Number(99.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=ROW(A01)"),
        EvaluatedCellValue::Error(FormulaError::Ref)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=ROW(FOOBAR)"),
        EvaluatedCellValue::Error(FormulaError::Name)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=OFFSET(A01, 0, 0)"),
        EvaluatedCellValue::Error(FormulaError::Ref)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=COUNTIF(A01:B2, \"<>0\")"),
        EvaluatedCellValue::Error(FormulaError::Ref)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=COUNTIF(A1:XFD1000000, \"<>0\")"),
        EvaluatedCellValue::Error(FormulaError::Ref)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=IF(TRUE, 42, Main!A1:Ⅰ분기!B2)"),
        EvaluatedCellValue::Number(42.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=IFERROR(Main!A1:'2026Q1'!B2, 99)"),
        EvaluatedCellValue::Number(99.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=Main!A1:Ⅰ분기!B2"),
        EvaluatedCellValue::Error(FormulaError::Ref)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=Missing!A1:B2"),
        EvaluatedCellValue::Error(FormulaError::Ref)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=Missing!A1:Missing!B2"),
        EvaluatedCellValue::Error(FormulaError::Ref)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=Missing!A1:A1"),
        EvaluatedCellValue::Error(FormulaError::Ref)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=Ⅰ분기!A1:A1"),
        EvaluatedCellValue::Number(66.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Ⅰ분기", "=A1:A1"),
        EvaluatedCellValue::Number(66.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=2026분기!B2"),
        EvaluatedCellValue::Error(FormulaError::Value)
    );
}
