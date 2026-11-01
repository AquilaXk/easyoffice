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
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=2026\u{20000}!B2"),
        EvaluatedCellValue::Error(FormulaError::Value)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=1e999분기"),
        EvaluatedCellValue::Error(FormulaError::Num)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=(Ⅰ분기!A1:A1)"),
        EvaluatedCellValue::Number(66.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=(Missing!A1:B2)"),
        EvaluatedCellValue::Error(FormulaError::Ref)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=ROW((B3))"),
        EvaluatedCellValue::Number(3.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=COLUMN((C4))"),
        EvaluatedCellValue::Number(3.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=ROWS((A1:B3))"),
        EvaluatedCellValue::Number(3.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=COLUMNS((A1:C2))"),
        EvaluatedCellValue::Number(3.0)
    );
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", "=SUM((Ⅰ분기!A1:A1))"),
        EvaluatedCellValue::Number(66.0)
    );
    let nested_add_33 = format!("=1+{}1{}", "(1+".repeat(33), ")".repeat(33));
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", &nested_add_33),
        EvaluatedCellValue::Number(35.0)
    );
    let nested_add_64 = format!("=1+{}1{}", "(1+".repeat(64), ")".repeat(64));
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", &nested_add_64),
        EvaluatedCellValue::Number(66.0)
    );
    let mixed_prec_13 = format!("={}1{}", "1=1&1+1*1^(".repeat(13), ")".repeat(13));
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", &mixed_prec_13),
        EvaluatedCellValue::Boolean(false)
    );
    let nested_parens_64 = format!("=1+{}1{}", "(".repeat(64), ")".repeat(64));
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", &nested_parens_64),
        EvaluatedCellValue::Number(2.0)
    );
    let nested_parens_65 = format!("=1+{}1{}", "(".repeat(65), ")".repeat(65));
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", &nested_parens_65),
        EvaluatedCellValue::Error(FormulaError::Value)
    );
    let nested_row_64 = format!("={}ROW(){}", "(".repeat(64), ")".repeat(64));
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", &nested_row_64),
        EvaluatedCellValue::Number(1.0)
    );
    let nested_row_65 = format!("={}ROW(){}", "(".repeat(65), ")".repeat(65));
    assert_eq!(
        wb.evaluate_formula_on_sheet("Main", &nested_row_65),
        EvaluatedCellValue::Error(FormulaError::Value)
    );
}

#[test]
fn test_packed_cell_coordinate_and_a1_roundtrip() {
    use easyoffice_sheet::{PackedCellCoordinate, PackedWorkbookCoord};
    let c = PackedCellCoordinate::parse_a1("B12").unwrap();
    assert_eq!(c.row(), 11);
    assert_eq!(c.col(), 1);
    assert_eq!(c.to_a1(), "B12");

    let xfd = PackedCellCoordinate::parse_a1("$XFD$1048576").unwrap();
    assert_eq!(xfd.row(), 1_048_575);
    assert_eq!(xfd.col(), 16_383);
    assert_eq!(xfd.to_a1(), "XFD1048576");

    let a1 = PackedWorkbookCoord::new(7, PackedCellCoordinate::parse_a1("A1").unwrap());
    let a2 = PackedWorkbookCoord::new(7, PackedCellCoordinate::parse_a1("A2").unwrap());
    let a70000 = PackedWorkbookCoord::new(7, PackedCellCoordinate::parse_a1("A70000").unwrap());
    assert_ne!(a1, a2);
    assert_ne!(a1, a70000);
    assert_ne!(a2, a70000);
    assert_eq!(a70000.sheet_id(), 7);
    assert_eq!(a70000.row(), 69_999);
    assert_eq!(a70000.col(), 0);

    assert!(PackedCellCoordinate::new(1_048_576, 0).is_err());
    assert!(PackedCellCoordinate::new(0, 16_384).is_err());
}

#[test]
fn test_formula_evaluation_operators_isblank_and_functions() {
    use easyoffice_sheet::PackedCellCoordinate;
    let mut sheet = SheetEngine::new("재무시트");
    sheet.set_cell("A1", "100").unwrap();
    sheet.set_cell("A2", "200").unwrap();
    sheet.set_cell("A3", "=+$A$1+A2*15%").unwrap();
    sheet.set_cell("B1", "=\"\"").unwrap();
    sheet.set_cell("B2", "=ISBLANK(C1)").unwrap();
    sheet.set_cell("B3", "=ISBLANK(B1)").unwrap();
    sheet.set_cell("D1", "=SUM(A1:A3)").unwrap();
    sheet.set_cell("D2", "=AVERAGE(A1:A2)").unwrap();
    sheet
        .set_cell("D3", "=IF(D1>200, UPPER(\"pass\"), \"fail\")")
        .unwrap();

    assert_eq!(sheet.evaluate_cell("A3"), EvaluatedCellValue::Number(130.0));
    let all = sheet.recalculate_all();
    assert_eq!(
        all.get(&PackedCellCoordinate::parse_a1("D1").unwrap()),
        Some(&EvaluatedCellValue::Number(430.0))
    );
    assert_eq!(sheet.evaluate_cell("B2"), EvaluatedCellValue::Boolean(true));
    assert_eq!(
        sheet.evaluate_cell("B3"),
        EvaluatedCellValue::Boolean(false)
    );
    assert_eq!(sheet.evaluate_cell("D1"), EvaluatedCellValue::Number(430.0));
    assert_eq!(sheet.evaluate_cell("D2"), EvaluatedCellValue::Number(150.0));
    assert_eq!(
        sheet.evaluate_cell("D3"),
        EvaluatedCellValue::Text("PASS".to_string())
    );
}

#[test]
fn test_cycle_detection_lookup_npv_and_pivot() {
    use easyoffice_sheet::{CellRange, PivotAggregation};
    let mut sheet = SheetEngine::new("분석시트");
    sheet.set_cell("A1", "=B1+1").unwrap();
    sheet.set_cell("B1", "=A1+1").unwrap();
    assert_eq!(
        sheet.evaluate_cell("A1"),
        EvaluatedCellValue::Error(FormulaError::Cycle)
    );

    sheet.set_cell("A1", "부서").unwrap();
    sheet.set_cell("B1", "매출").unwrap();
    sheet.set_cell("A2", "플랫폼").unwrap();
    sheet.set_cell("B2", "1200").unwrap();
    sheet.set_cell("A3", "코어엔진").unwrap();
    sheet.set_cell("B3", "1800").unwrap();
    sheet.set_cell("A4", "플랫폼").unwrap();
    sheet.set_cell("B4", "800").unwrap();

    sheet
        .set_cell("C1", "=VLOOKUP(\"코어엔진\", A2:B4, 2, FALSE)")
        .unwrap();
    sheet.set_cell("C2", "=INDEX(A2:B4, 2, 2)").unwrap();
    sheet
        .set_cell("C3", "=MATCH(\"코어엔진\", A2:A4, 0)")
        .unwrap();
    sheet.set_cell("C4", "=ROUND(NPV(0.1, B2:B3), 2)").unwrap();
    sheet
        .set_cell("C5", "=SQRT(16)+POWER(2,3)+MOD(10,3)")
        .unwrap();
    sheet.set_cell("C6", "=CONCAT(A2, \"-\", A3)").unwrap();
    sheet.set_cell("C7", "=MEDIAN(B2:B4)").unwrap();
    sheet.set_cell("C8", "=ROUND(STDEV(B2:B4), 2)").unwrap();
    sheet
        .set_cell("C9", "=VLOOKUP(\"코어엔진\", A2:B4, 1/0, FALSE)")
        .unwrap();

    assert_eq!(
        sheet.evaluate_cell("C1"),
        EvaluatedCellValue::Number(1800.0)
    );
    assert_eq!(
        sheet.evaluate_cell("C2"),
        EvaluatedCellValue::Number(1800.0)
    );
    assert_eq!(sheet.evaluate_cell("C3"), EvaluatedCellValue::Number(2.0));
    assert_eq!(
        sheet.evaluate_cell("C4"),
        EvaluatedCellValue::Number(2578.51)
    );
    assert_eq!(sheet.evaluate_cell("C5"), EvaluatedCellValue::Number(13.0));
    assert_eq!(
        sheet.evaluate_cell("C6"),
        EvaluatedCellValue::Text("플랫폼-코어엔진".to_string())
    );
    assert_eq!(
        sheet.evaluate_cell("C7"),
        EvaluatedCellValue::Number(1200.0)
    );
    assert_eq!(
        sheet.evaluate_cell("C8"),
        EvaluatedCellValue::Number(503.32)
    );
    assert_eq!(
        sheet.evaluate_cell("C9"),
        EvaluatedCellValue::Error(FormulaError::DivByZero)
    );

    let range = CellRange::parse("A1:B4").unwrap();
    let pivot = sheet.compute_pivot(&range, true, 0, 1, PivotAggregation::Sum);
    assert_eq!(
        pivot.row_keys,
        vec!["플랫폼".to_string(), "코어엔진".to_string()]
    );
    assert_eq!(pivot.row_totals, vec![2000.0, 1800.0]);
    assert_eq!(pivot.grand_total, 3800.0);
}

#[test]
fn test_shared_coercion_parity_fixture_vectors() {
    use easyoffice_formats::StrictJsonValue;

    let raw_json = include_str!("../../../fixtures/sheet-coercion-parity.json");
    let parsed = StrictJsonValue::parse(raw_json).expect("valid sheet-coercion-parity.json");
    let StrictJsonValue::Array(items) = parsed else {
        panic!("expected top-level array in sheet-coercion-parity.json");
    };
    assert!(items.len() >= 23);

    for item in &items {
        let StrictJsonValue::Object(fields) = item else {
            panic!("expected object entry");
        };
        let id = match StrictJsonValue::get_field(fields, "id") {
            Some(StrictJsonValue::Str(s)) => s.as_str(),
            _ => panic!("missing id"),
        };
        let target = match StrictJsonValue::get_field(fields, "target") {
            Some(StrictJsonValue::Str(s)) => s.as_str(),
            _ => panic!("missing target for {id}"),
        };
        let expected_type = match StrictJsonValue::get_field(fields, "expectedType") {
            Some(StrictJsonValue::Str(s)) => s.as_str(),
            _ => panic!("missing expectedType for {id}"),
        };
        let expected_display = match StrictJsonValue::get_field(fields, "expectedDisplay") {
            Some(StrictJsonValue::Str(s)) => s.as_str(),
            _ => panic!("missing expectedDisplay for {id}"),
        };
        let Some(StrictJsonValue::Object(cell_entries)) =
            StrictJsonValue::get_field(fields, "cells")
        else {
            panic!("missing cells object for {id}");
        };

        let mut sheet = SheetEngine::new("Parity");
        for (addr, raw_val) in cell_entries {
            let StrictJsonValue::Str(s) = raw_val else {
                panic!("cell value must be string for {id}");
            };
            sheet.set_cell(addr, s).unwrap();
        }
        let val = sheet.evaluate_cell(target);
        let actual_type = match &val {
            EvaluatedCellValue::Empty => "empty",
            EvaluatedCellValue::Number(_) => "number",
            EvaluatedCellValue::Text(_) => "text",
            EvaluatedCellValue::Boolean(_) => "boolean",
            EvaluatedCellValue::Error(_) => "error",
        };
        assert_eq!(actual_type, expected_type, "type mismatch on fixture {id}");
        assert_eq!(
            val.to_display_string(),
            expected_display,
            "display mismatch on fixture {id}"
        );
    }
}

#[test]
fn test_issue_88_dag_chains_99999_budget_whole_col_row_and_max_limits() {
    use easyoffice_sheet::PackedCellCoordinate;

    assert_eq!(PackedCellCoordinate::MAX_ROWS, 1_048_576);
    assert_eq!(PackedCellCoordinate::MAX_COLS, 16_384);
    let max_addr = PackedCellCoordinate::parse_a1("XFD1048576").expect("XFD1048576 must parse");
    assert_eq!(max_addr.col(), 16_383);
    assert_eq!(max_addr.row(), 1_048_575);
    assert_eq!(max_addr.to_a1(), "XFD1048576");
    assert!(PackedCellCoordinate::parse_a1("XFD1048577").is_err());

    let mut fwd = SheetEngine::new("Forward2000");
    fwd.set_cell("A1", "1").unwrap();
    for r in 2..=2000 {
        fwd.set_cell(&format!("A{r}"), format!("=A{}+1", r - 1))
            .unwrap();
    }
    assert_eq!(
        fwd.evaluate_cell("A2000"),
        EvaluatedCellValue::Number(2000.0)
    );
    let fwd_all = fwd.recalculate_all();
    assert_eq!(
        fwd_all.get(&PackedCellCoordinate::parse_a1("A2000").unwrap()),
        Some(&EvaluatedCellValue::Number(2000.0))
    );

    let mut bwd = SheetEngine::new("Backward2000");
    bwd.set_cell("A2000", "1").unwrap();
    for r in (1..2000).rev() {
        bwd.set_cell(&format!("A{r}"), format!("=A{}+1", r + 1))
            .unwrap();
    }
    assert_eq!(bwd.evaluate_cell("A1"), EvaluatedCellValue::Number(2000.0));
    let bwd_all = bwd.recalculate_all();
    assert_eq!(
        bwd_all.get(&PackedCellCoordinate::parse_a1("A1").unwrap()),
        Some(&EvaluatedCellValue::Number(2000.0))
    );

    let mut wb = WorkbookEngine::new();
    {
        let s = wb.add_sheet("Chain");
        s.set_cell("A2000", "1").unwrap();
        for r in (1..2000).rev() {
            s.set_cell(&format!("A{r}"), format!("=A{}+1", r + 1))
                .unwrap();
        }
    }
    assert_eq!(
        wb.evaluate_cell("Chain", "A1"),
        EvaluatedCellValue::Number(2000.0)
    );

    let mut large = SheetEngine::new("LargeRange");
    for r in 1..=99_999u32 {
        large.set_cell(&format!("A{r}"), "1").unwrap();
    }
    large.set_cell("B1", "=SUM(A1:A99999)").unwrap();
    large.set_cell("B2", "=COUNTIF(A1:A99999, \">0\")").unwrap();
    large.set_cell("B3", "=SUMIF(A1:A99999, \">0\")").unwrap();
    let b1_eval = large.evaluate_cell("B1");
    let b2_eval = large.evaluate_cell("B2");
    let b3_eval = large.evaluate_cell("B3");
    assert_eq!(b1_eval, EvaluatedCellValue::Number(99_999.0));
    assert_eq!(b2_eval, EvaluatedCellValue::Number(99_999.0));
    assert_eq!(b3_eval, EvaluatedCellValue::Number(99_999.0));
    let large_all = large.recalculate_all();
    assert_eq!(
        large_all.get(&PackedCellCoordinate::parse_a1("B1").unwrap()),
        Some(&b1_eval)
    );
    assert_eq!(
        large_all.get(&PackedCellCoordinate::parse_a1("B2").unwrap()),
        Some(&b2_eval)
    );
    assert_eq!(
        large_all.get(&PackedCellCoordinate::parse_a1("B3").unwrap()),
        Some(&b3_eval)
    );

    let mut whole = SheetEngine::new("WholeAxis");
    whole.set_cell("A1", "10").unwrap();
    whole.set_cell("A1048576", "90").unwrap();
    whole.set_cell("B2", "25").unwrap();
    whole.set_cell("C3", "75").unwrap();
    whole.set_cell("D5", "=SUM(A:A)").unwrap();
    whole.set_cell("D6", "=SUM(B:C)").unwrap();
    whole.set_cell("D7", "=SUM(1:1)").unwrap();
    whole.set_cell("D8", "=SUM(2:3)").unwrap();
    assert_eq!(whole.evaluate_cell("D5"), EvaluatedCellValue::Number(100.0));
    assert_eq!(whole.evaluate_cell("D6"), EvaluatedCellValue::Number(100.0));
    assert_eq!(whole.evaluate_cell("D7"), EvaluatedCellValue::Number(10.0));
    assert_eq!(whole.evaluate_cell("D8"), EvaluatedCellValue::Number(100.0));

    let terms = vec!["1"; 1025].join("+");
    let formula_1025 = format!("={terms}");
    assert_eq!(
        whole.evaluate_formula_str(&formula_1025),
        EvaluatedCellValue::Number(1025.0)
    );
}

#[test]
fn test_issue_89_and_90_chart_svg_dates_wildcards_error_literals_pivot_and_functions() {
    use easyoffice_sheet::{
        compute_sheet_id, CellRange, ChartSeriesBuilder, ChartType, PivotAggregation,
    };

    let mut chart_sheet = SheetEngine::new("ChartGuard");
    chart_sheet.set_cell("A1", "Q1").unwrap();
    chart_sheet.set_cell("A2", "Q2").unwrap();
    chart_sheet.set_cell("A3", "Q3").unwrap();
    chart_sheet.set_cell("B1", "=1E308").unwrap();
    chart_sheet.set_cell("B2", "=-1E308").unwrap();
    chart_sheet.set_cell("B3", "=1/0").unwrap();
    let chart = ChartSeriesBuilder::build(
        &chart_sheet,
        ChartType::Line,
        CellRange::parse("A1:A3").unwrap(),
        CellRange::parse("B1:B3").unwrap(),
    )
    .unwrap();
    assert!(!chart.svg_path.contains("NaN"));
    assert!(!chart.svg_path.contains("inf"));
    assert!(!chart.svg_path.contains("Inf"));
    for pt in &chart.points {
        assert!(pt.normalized_x.is_finite());
        assert!(pt.normalized_y.is_finite());
        assert!(pt.pie_share.is_finite());
    }

    let mut sheet = SheetEngine::new("Sheet1");
    sheet.set_cell("A1", "=DATE(2024, 1, 15)").unwrap();
    assert_eq!(sheet.formatted_cell("A1"), "2024-01-15");

    assert_ne!(compute_sheet_id("A-B"), compute_sheet_id("A_B"));

    assert_eq!(
        sheet.evaluate_formula_str("=IFERROR(#N/A, 42)"),
        EvaluatedCellValue::Number(42.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=IFNA(#N/A, 99)"),
        EvaluatedCellValue::Number(99.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=IFNA(#DIV/0!, 99)"),
        EvaluatedCellValue::Error(FormulaError::DivByZero)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=ISNA(#N/A)"),
        EvaluatedCellValue::Boolean(true)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=ISERR(#N/A)"),
        EvaluatedCellValue::Boolean(false)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=ISERR(#VALUE!)"),
        EvaluatedCellValue::Boolean(true)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=ISERROR(#CYCLE!)"),
        EvaluatedCellValue::Boolean(true)
    );

    assert_eq!(
        sheet.evaluate_formula_str("=IPMT(0.05/12, 0, 12, -1000)"),
        EvaluatedCellValue::Error(FormulaError::Num)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=PPMT(0.05/12, 13, 12, -1000)"),
        EvaluatedCellValue::Error(FormulaError::Num)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=ROUND(PMT(0.06/12, 12, -1200) - (IPMT(0.06/12, 3, 12, -1200) + PPMT(0.06/12, 3, 12, -1200)), 8)"),
        EvaluatedCellValue::Number(0.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=RATE(10, -100, -1000)"),
        EvaluatedCellValue::Error(FormulaError::Num)
    );

    sheet.set_cell("W1", "ab*cd").unwrap();
    sheet.set_cell("W2", "abXcd").unwrap();
    sheet.set_cell("W3", "ab?cd").unwrap();
    assert_eq!(
        sheet.evaluate_formula_str("=COUNTIF(W1:W3, \"ab~*cd\")"),
        EvaluatedCellValue::Number(1.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=COUNTIF(W1:W3, \"ab*cd\")"),
        EvaluatedCellValue::Number(3.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=MATCH(\"ab~?cd\", W1:W3, 0)"),
        EvaluatedCellValue::Number(3.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=SEARCH(\"~*\", \"ab*cd\")"),
        EvaluatedCellValue::Number(3.0)
    );

    sheet.set_cell("P1", "Key").unwrap();
    sheet.set_cell("Q1", "Col").unwrap();
    sheet.set_cell("R1", "Val").unwrap();
    sheet.set_cell("P2", "1.5").unwrap();
    sheet.set_cell("Q2", "East").unwrap();
    sheet.set_cell("R2", "10").unwrap();
    sheet.set_cell("P3", "=TEXT(1.5, \"0.00\")").unwrap();
    sheet.set_cell("Q3", "West").unwrap();
    sheet.set_cell("R3", "20").unwrap();
    let p_range = CellRange::parse("P1:R3").unwrap();
    let pivot = sheet.compute_pivot_grid(&p_range, true, 0, Some(1), 2, PivotAggregation::Sum);
    assert_eq!(pivot.row_keys, vec!["1.5".to_string()]);
    assert_eq!(
        pivot.column_keys,
        vec!["East".to_string(), "West".to_string()]
    );
    assert_eq!(pivot.matrix, vec![vec![10.0, 20.0]]);
    assert_eq!(pivot.grand_total, 30.0);

    assert_eq!(
        sheet.evaluate_formula_str("=SUMIFS(R2:R3, P2:P3, 1.5, Q2:Q3, \"West\")"),
        EvaluatedCellValue::Number(20.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=CHOOSE(2, \"A\", \"B\", \"C\")"),
        EvaluatedCellValue::Text("B".to_string())
    );
    assert_eq!(
        sheet.evaluate_formula_str("=MROUND(10, 3)"),
        EvaluatedCellValue::Number(9.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=QUOTIENT(10, 3)"),
        EvaluatedCellValue::Number(3.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=SIGN(-5)"),
        EvaluatedCellValue::Number(-1.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=FACT(5)"),
        EvaluatedCellValue::Number(120.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=SLN(1000, 100, 5)"),
        EvaluatedCellValue::Number(180.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=FLOOR(-2.5, 2)"),
        EvaluatedCellValue::Number(-4.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=DATE(1900, 1, 0)"),
        EvaluatedCellValue::Number(0.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=SUMIF(P2:P3, 1.5, R2)"),
        EvaluatedCellValue::Number(30.0)
    );
}
