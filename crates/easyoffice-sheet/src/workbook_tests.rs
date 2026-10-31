use super::*;

#[test]
fn test_all_122_canonical_functions_evaluated() {
    let names = SheetEngine::supported_function_names();
    assert_eq!(names.len(), 122);

    let mut s = SheetEngine::new("함수검증");
    s.set_cell("A1", "10").unwrap();
    s.set_cell("A2", "20").unwrap();
    s.set_cell("A3", "20").unwrap();
    s.set_cell("A4", "40").unwrap();
    s.set_cell("B1", "1").unwrap();
    s.set_cell("B2", "2").unwrap();
    s.set_cell("B3", "3").unwrap();
    s.set_cell("B4", "4").unwrap();
    s.set_cell("C1", "-1000").unwrap();
    s.set_cell("C2", "400").unwrap();
    s.set_cell("C3", "500").unwrap();
    s.set_cell("C4", "600").unwrap();

    assert_eq!(
        s.evaluate_formula_str("=PRODUCT(A1:A2)"),
        EvaluatedCellValue::Number(200.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=SUMPRODUCT(A1:A4, B1:B4)"),
        EvaluatedCellValue::Number(270.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=SUMIF(A1:A4, \">=20\", B1:B4)"),
        EvaluatedCellValue::Number(9.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=SUMIFS(B1:B4, A1:A4, \">=20\", B1:B4, \">2\")"),
        EvaluatedCellValue::Number(7.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=SIGN(-5)+ROUNDUP(1.21,1)+ROUNDDOWN(1.29,1)+INT(3.9)+TRUNC(4.8)"),
        EvaluatedCellValue::Number(8.5)
    );
    assert_eq!(
        s.evaluate_formula_str("=CEILING(4.2, 2)+FLOOR(4.9, 2)+QUOTIENT(17, 5)"),
        EvaluatedCellValue::Number(13.0)
    );
    assert_eq!(
        s.evaluate_formula_str(
            "=ROUND(LN(EXP(2))+LOG10(100)+LOG(8,2)+SIN(0)+COS(0)+TAN(0)+RAND(), 2)"
        ),
        EvaluatedCellValue::Number(8.5)
    );
    assert!(
        (s.evaluate_formula_str("=PI()").as_number().unwrap() - std::f64::consts::PI).abs() < 1e-9
    );

    assert_eq!(
        s.evaluate_formula_str("=AVERAGEA(A1:A4)"),
        EvaluatedCellValue::Number(22.5)
    );
    assert_eq!(
        s.evaluate_formula_str("=AVERAGEIF(A1:A4, \"20\", B1:B4)"),
        EvaluatedCellValue::Number(2.5)
    );
    assert_eq!(
        s.evaluate_formula_str("=AVERAGEIFS(B1:B4, A1:A4, \">=20\")"),
        EvaluatedCellValue::Number(3.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=MODE(A1:A4)"),
        EvaluatedCellValue::Number(20.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=SMALL(A1:A4, 2)+LARGE(A1:A4, 1)"),
        EvaluatedCellValue::Number(60.0)
    );
    assert_eq!(
        s.evaluate_formula_str(
            "=COUNTBLANK(D1:D3)+COUNTIF(A1:A4, 20)+COUNTIFS(A1:A4, \">=20\", B1:B4, \">=3\")"
        ),
        EvaluatedCellValue::Number(7.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=ROUND(STDEVP(A1:A4)+VAR(A1:A4)+VARP(A1:A4), 2)"),
        EvaluatedCellValue::Number(287.98)
    );
    assert_eq!(
        s.evaluate_formula_str("=RANK(20, A1:A4)+PERCENTILE(A1:A4, 0.5)+QUARTILE(A1:A4, 2)"),
        EvaluatedCellValue::Number(42.0)
    );

    assert_eq!(
        s.evaluate_formula_str("=IFS(A1>50, \"A\", A2=20, \"B\")"),
        EvaluatedCellValue::Text("B".to_string())
    );
    assert_eq!(
        s.evaluate_formula_str("=XOR(TRUE(), FALSE())"),
        EvaluatedCellValue::Boolean(true)
    );
    assert_eq!(
        s.evaluate_formula_str("=IFNA(MATCH(999, A1:A4, 0), 42)"),
        EvaluatedCellValue::Number(42.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=SWITCH(2, 1, \"일\", 2, \"이\", \"기타\")"),
        EvaluatedCellValue::Text("이".to_string())
    );

    assert_eq!(
        s.evaluate_formula_str("=HLOOKUP(10, A1:B2, 2, FALSE)"),
        EvaluatedCellValue::Number(20.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=XLOOKUP(40, A1:A4, B1:B4, 0)"),
        EvaluatedCellValue::Number(4.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=CHOOSE(2, \"가\", \"나\", \"다\")"),
        EvaluatedCellValue::Text("나".to_string())
    );
    assert_eq!(s.evaluate_formula_str("=OFFSET(A1, 2, 1)+ROW(B3)+COLUMN(C1)+ROWS(A1:B4)+COLUMNS(A1:C2)+ROWS(TRANSPOSE(A1:C2))"), EvaluatedCellValue::Number(19.0));

    assert_eq!(
        s.evaluate_formula_str("=TEXTJOIN(\"-\", TRUE, \"Easy\", \"\", \"Office\")"),
        EvaluatedCellValue::Text("Easy-Office".to_string())
    );
    assert_eq!(s.evaluate_formula_str("=CONCATENATE(LEFT(\"EasyOffice\", 4), MID(\"EasyOffice\", 5, 3), RIGHT(\"EasyOffice\", 3))"), EvaluatedCellValue::Text("EasyOffice".to_string()));
    assert_eq!(
        s.evaluate_formula_str("=PROPER(\"easy office suite\")"),
        EvaluatedCellValue::Text("Easy Office Suite".to_string())
    );
    assert_eq!(
        s.evaluate_formula_str("=SUBSTITUTE(REPLACE(\"2025-Q1\", 1, 4, \"2026\"), \"Q1\", \"Q2\")"),
        EvaluatedCellValue::Text("2026-Q2".to_string())
    );
    assert_eq!(
        s.evaluate_formula_str(
            "=FIND(\"Office\", \"EasyOffice\")+SEARCH(\"office\", \"EasyOffice\")"
        ),
        EvaluatedCellValue::Number(10.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=EXACT(\"AB\", \"AB\")"),
        EvaluatedCellValue::Boolean(true)
    );
    assert_eq!(
        s.evaluate_formula_str("=REPT(\"A\", 3)&TEXT(12.345, \"0.00\")&CHAR(CODE(\"Z\"))"),
        EvaluatedCellValue::Text("AAA12.35Z".to_string())
    );
    assert_eq!(
        s.evaluate_formula_str("=VALUE(\"1,250.5\")"),
        EvaluatedCellValue::Number(1250.5)
    );

    assert_eq!(
        s.evaluate_formula_str(
            "=YEAR(DATE(2026, 10, 24))+MONTH(DATE(2026, 10, 24))+DAY(DATE(2026, 10, 24))"
        ),
        EvaluatedCellValue::Number(2060.0)
    );
    assert_eq!(
        s.evaluate_formula_str(
            "=HOUR(TIME(14, 30, 45))+MINUTE(TIME(14, 30, 45))+SECOND(TIME(14, 30, 45))"
        ),
        EvaluatedCellValue::Number(89.0)
    );
    assert_eq!(
        s.evaluate_formula_str(
            "=DAYS(DATE(2026, 1, 11), DATE(2026, 1, 1))+MONTH(EDATE(DATE(2026, 1, 15), 2))"
        ),
        EvaluatedCellValue::Number(13.0)
    );

    assert!(
        s.evaluate_formula_str("=PMT(0.05, 10, 1000)")
            .as_number()
            .unwrap()
            < -129.0
    );
    assert!(
        s.evaluate_formula_str("=IPMT(0.05, 1, 10, 1000)+PPMT(0.05, 1, 10, 1000)")
            .as_number()
            .unwrap()
            < -129.0
    );
    assert!(
        s.evaluate_formula_str("=FV(0.05, 10, -100, -1000)")
            .as_number()
            .unwrap()
            > 2800.0
    );
    assert!(
        s.evaluate_formula_str("=PV(0.05, 10, -100)")
            .as_number()
            .unwrap()
            > 770.0
    );
    assert!(s.evaluate_formula_str("=IRR(C1:C4)").as_number().unwrap() > 0.2);
    assert!(
        s.evaluate_formula_str("=RATE(10, -129.5046, 1000)")
            .as_number()
            .unwrap()
            > 0.049
    );
    assert_eq!(
        s.evaluate_formula_str("=ROUND(NPER(0.05, -129.504575, 1000), 1)"),
        EvaluatedCellValue::Number(10.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=SLN(10000, 1000, 9)+DDB(10000, 1000, 5, 1)"),
        EvaluatedCellValue::Number(5000.0)
    );
    assert!(
        s.evaluate_formula_str("=DB(10000, 1000, 5, 1)")
            .as_number()
            .unwrap()
            > 3600.0
    );
    assert!(
        s.evaluate_formula_str("=NPV(0.08, C2:C4)")
            .as_number()
            .unwrap()
            > 1200.0
    );

    assert_eq!(
        s.evaluate_formula_str("=SUM(A1:A4)+ABS(-5)+MOD(17, 5)+SQRT(16)+POWER(2, 3)"),
        EvaluatedCellValue::Number(109.0)
    );
    assert_eq!(
        s.evaluate_formula_str(
            "=AVERAGE(A1:A4)+MEDIAN(A1:A4)+MIN(A1:A4)+MAX(A1:A4)+COUNT(A1:A4)+COUNTA(A1:A4)+ROUND(STDEV(A1:A4), 2)"
        ),
        EvaluatedCellValue::Number(113.08)
    );
    assert_eq!(
        s.evaluate_formula_str("=IF(OR(NOT(FALSE()), FALSE()), IFERROR(1/0, 7), 0)"),
        EvaluatedCellValue::Number(7.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=VLOOKUP(20, A1:B4, 2, FALSE)+INDEX(B1:B4, 4, 1)"),
        EvaluatedCellValue::Number(6.0)
    );
    assert_eq!(
        s.evaluate_formula_str(
            "=CONCAT(TRIM(\"  Easy  \"), UPPER(\"o\"), LOWER(\"FFICE\"), LEN(\"ABC\"))"
        ),
        EvaluatedCellValue::Text("EasyOffice3".to_string())
    );

    assert_eq!(s.evaluate_formula_str("=AND(ISBLANK(Z99), ISNUMBER(1), ISTEXT(\"a\"), ISLOGICAL(TRUE()), ISERR(1/0), ISERROR(#N/A), ISNA(MATCH(99, A1:A4, 0)), ISEVEN(4), ISODD(5))"), EvaluatedCellValue::Boolean(true));
    assert_eq!(
        s.evaluate_formula_str("=TYPE(42)+TYPE(\"abc\")+TYPE(TRUE())+TYPE(1/0)"),
        EvaluatedCellValue::Number(23.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=INDIRECT(\"A1\")"),
        EvaluatedCellValue::Number(10.0)
    );
}

#[test]
fn test_workbook_cross_sheet_formulas_and_sort_goal_seek_chart() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    wb.add_sheet("Sheet2");
    wb.set_cell("Sheet2", "A1", "50").unwrap();
    wb.set_cell("Sheet2", "A2", "70").unwrap();
    wb.set_cell("Sheet2", "A3", "80").unwrap();
    wb.set_cell("Sheet1", "B1", "=Sheet2!A1 * 2").unwrap();
    wb.set_cell("Sheet1", "B2", "=SUM(Sheet2!A1:A3)").unwrap();

    assert_eq!(
        wb.evaluate_cell("Sheet1", "B1"),
        EvaluatedCellValue::Number(100.0)
    );
    assert_eq!(
        wb.evaluate_cell("Sheet1", "B2"),
        EvaluatedCellValue::Number(200.0)
    );

    wb.set_cell("Sheet1", "C1", "=Sheet2!C1 + 1").unwrap();
    wb.set_cell("Sheet2", "C1", "=Sheet1!C1 + 1").unwrap();
    assert_eq!(
        wb.evaluate_cell("Sheet1", "C1"),
        EvaluatedCellValue::Error(FormulaError::Cycle)
    );

    let sheet = wb.sheet_mut("Sheet2").unwrap();
    sheet.set_cell("A1", "30").unwrap();
    sheet.set_cell("B1", "나").unwrap();
    sheet.set_cell("A2", "10").unwrap();
    sheet.set_cell("B2", "가").unwrap();
    sheet.set_cell("A3", "20").unwrap();
    sheet.set_cell("B3", "다").unwrap();
    sheet.sort_rows(0, 2, 0, true, Some(1), true).unwrap();
    assert_eq!(sheet.evaluate_cell("A1"), EvaluatedCellValue::Number(10.0));
    assert_eq!(sheet.evaluate_cell("A3"), EvaluatedCellValue::Number(30.0));

    sheet.set_cell("D1", "5").unwrap();
    sheet.set_cell("D2", "=D1 * 12 + 40").unwrap();
    let solved = sheet.goal_seek("D2", 160.0, "D1").unwrap();
    assert!((solved - 10.0).abs() < 1e-5);
    assert_eq!(sheet.evaluate_cell("D2"), EvaluatedCellValue::Number(160.0));

    let chart = ChartSeriesBuilder::build(
        sheet,
        ChartType::Bar,
        CellRange::parse("B1:B3").unwrap(),
        CellRange::parse("A1:A3").unwrap(),
    )
    .unwrap();
    assert_eq!(chart.points.len(), 3);
    assert!((chart.points[2].normalized_y - 1.0).abs() < 1e-6);
}

#[test]
fn test_workbook_sheet_id_collision_resolution_without_false_cycle() {
    let mut seen: HashMap<u16, String> = HashMap::new();
    let mut pair: Option<(String, String)> = None;
    for idx in 0..2000u32 {
        let candidate = format!("Sheet_{idx}");
        let h = crate::compute_sheet_id(&candidate);
        if let Some(prev) = seen.insert(h, candidate.clone()) {
            pair = Some((prev, candidate));
            break;
        }
    }
    let (sheet_a, sheet_b) = pair.expect("16-bit FNV-1a collision pair found");
    assert_eq!(
        crate::compute_sheet_id(&sheet_a),
        crate::compute_sheet_id(&sheet_b)
    );

    let mut wb = WorkbookEngine::new();
    wb.add_sheet(&sheet_a);
    wb.add_sheet(&sheet_b);
    assert_ne!(
        wb.sheet(&sheet_a).unwrap().resolved_sheet_id(),
        wb.sheet(&sheet_b).unwrap().resolved_sheet_id()
    );

    wb.set_cell(&sheet_b, "A1", "42").unwrap();
    wb.set_cell(&sheet_a, "A1", format!("={sheet_b}!A1 + 8"))
        .unwrap();
    assert_eq!(
        wb.evaluate_cell(&sheet_a, "A1"),
        EvaluatedCellValue::Number(50.0)
    );
}

#[test]
fn test_zero_alloc_streaming_range_aggregations_and_budget_guard() {
    let mut sheet = SheetEngine::new("Sheet1");
    for r in 1..=10u32 {
        for c in 0..6u32 {
            if r == 5 && c == 2 {
                continue;
            }
            let coord = PackedCellCoordinate::new(r - 1, c).unwrap().to_a1();
            sheet
                .set_cell(&coord, ((r as i64) * 10 + (c as i64)).to_string())
                .unwrap();
        }
    }
    assert_eq!(
        sheet.evaluate_formula_str("=SUM(A1:F10, 100)"),
        EvaluatedCellValue::Number(3498.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=COUNT(A1:F10)"),
        EvaluatedCellValue::Number(59.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=COUNTA(A1:F10)"),
        EvaluatedCellValue::Number(59.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=COUNTBLANK(A1:F10)"),
        EvaluatedCellValue::Number(1.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=MIN(A1:F10)"),
        EvaluatedCellValue::Number(10.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=MAX(A1:F10)"),
        EvaluatedCellValue::Number(105.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=ROUND(AVERAGE(A1:F10), 2)"),
        EvaluatedCellValue::Number(57.59)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=PRODUCT(A1:E1)"),
        EvaluatedCellValue::Number(10.0 * 11.0 * 12.0 * 13.0 * 14.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=SUMPRODUCT(A1:C2, D1:F2)"),
        EvaluatedCellValue::Number(
            10.0 * 13.0 + 11.0 * 14.0 + 12.0 * 15.0 + 20.0 * 23.0 + 21.0 * 24.0 + 22.0 * 25.0
        )
    );
    let sumif_2arg = sheet.evaluate_formula_str("=SUMIF(A1:F10, \">=60\")");
    let sumif_3arg = sheet.evaluate_formula_str("=SUMIF(A1:F10, \">=60\", A1:F10)");
    assert_eq!(sumif_2arg, sumif_3arg);
    assert_eq!(sumif_2arg, EvaluatedCellValue::Number(2475.0));

    let avgif_2arg = sheet.evaluate_formula_str("=AVERAGEIF(A1:F10, \">=60\")");
    let avgif_3arg = sheet.evaluate_formula_str("=AVERAGEIF(A1:F10, \">=60\", A1:F10)");
    assert_eq!(avgif_2arg, avgif_3arg);
    assert_eq!(avgif_2arg, EvaluatedCellValue::Number(82.5));

    assert_eq!(
        sheet.evaluate_formula_str("=COUNTIF(A1:F10, \">=60\")"),
        EvaluatedCellValue::Number(30.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=COUNTIFS(A1:F10, \">=60\", A1:F10, \"<80\")"),
        EvaluatedCellValue::Number(12.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=SUMIFS(A1:F10, A1:F10, \">=60\", A1:F10, \"<80\")"),
        EvaluatedCellValue::Number(810.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=AVERAGEIFS(A1:F10, A1:F10, \">=60\", A1:F10, \"<80\")"),
        EvaluatedCellValue::Number(67.5)
    );

    let huge_range = CellRange {
        start: PackedCellCoordinate::new(1, 1).unwrap(),
        end: PackedCellCoordinate::new(1000, 1000).unwrap(),
    };
    assert!(huge_range.cell_count() > CellRange::MAX_EXPANDED_CELLS);
    assert!(huge_range.expand().is_err());
}

#[test]
fn test_zero_alloc_xlookup_match_welford_stdev_rank_averagea_and_wildcard_escapes() {
    use crate::functions_date_fin::{wildcard_match, wildcard_match_alloc_baseline};

    let cases = [
        ("Report_2026_Final.xlsx", "report*final.xlsx", true),
        ("Report_2026_Draft.xlsx", "report*final.xlsx", false),
        ("A*B", "a~*b", true),
        ("AXB", "a~*b", false),
        ("A?B", "a~?b", true),
        ("AXB", "a~?b", false),
        ("A~B", "a~~b", true),
        ("Item-01", "item-??", true),
        ("Item-001", "item-??", false),
    ];
    for (text, pat, expected) in cases {
        assert_eq!(wildcard_match(text, pat), expected, "text={text} pat={pat}");
    }
    assert_eq!(
        wildcard_match("Report_2026_Final.xlsx", "report*final.xlsx"),
        wildcard_match_alloc_baseline("Report_2026_Final.xlsx", "report*final.xlsx")
    );

    let mut sheet = SheetEngine::new("ZeroAllocStats");
    let dataset = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
    for (idx, val) in dataset.iter().enumerate() {
        let r = idx + 1;
        sheet.set_cell(&format!("A{r}"), val.to_string()).unwrap();
        sheet
            .set_cell(&format!("B{r}"), ((idx as f64 + 1.0) * 100.0).to_string())
            .unwrap();
    }
    sheet.set_cell("B8", "=1/0").unwrap();

    assert_eq!(
        sheet.evaluate_formula_str("=XLOOKUP(5, A1:A8, B1:B8, -1)"),
        EvaluatedCellValue::Number(500.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=XLOOKUP(999, A1:A7, B1:B7, -1)"),
        EvaluatedCellValue::Number(-1.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=MATCH(7, A1:A8, 0)"),
        EvaluatedCellValue::Number(7.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=VLOOKUP(6, A1:B7, 2)"),
        EvaluatedCellValue::Number(600.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=RANK(7, A1:A8)"),
        EvaluatedCellValue::Number(2.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=RANK(7, A1:A8, 1)"),
        EvaluatedCellValue::Number(7.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=VARP(A1:A8)"),
        EvaluatedCellValue::Number(4.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=STDEVP(A1:A8)"),
        EvaluatedCellValue::Number(2.0)
    );
    if let EvaluatedCellValue::Number(v) = sheet.evaluate_formula_str("=VAR(A1:A8)") {
        assert!((v - 32.0 / 7.0).abs() < 1e-12);
    } else {
        panic!("Expected numeric VAR");
    }
    if let EvaluatedCellValue::Number(s) = sheet.evaluate_formula_str("=STDEV(A1:A8)") {
        assert!((s - (32.0_f64 / 7.0).sqrt()).abs() < 1e-12);
    } else {
        panic!("Expected numeric STDEV");
    }

    sheet.set_cell("C1", "10").unwrap();
    sheet.set_cell("C2", "TRUE").unwrap();
    sheet.set_cell("C3", "FALSE").unwrap();
    sheet.set_cell("C4", "label").unwrap();
    assert_eq!(
        sheet.evaluate_formula_str("=AVERAGEA(C1:C4)"),
        EvaluatedCellValue::Number(11.0 / 4.0)
    );

    sheet.set_cell("D1", "Apple*Pie").unwrap();
    sheet.set_cell("D2", "AppleSauce").unwrap();
    sheet.set_cell("D3", "Banana").unwrap();
    assert_eq!(
        sheet.evaluate_formula_str("=COUNTIF(D1:D3, \"apple~*pie\")"),
        EvaluatedCellValue::Number(1.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=COUNTIF(D1:D3, \"apple*\")"),
        EvaluatedCellValue::Number(2.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=COUNTIF(D1:D3, \"<>banana\")"),
        EvaluatedCellValue::Number(2.0)
    );

    assert_eq!(
        sheet.evaluate_formula_str("=DATE(2024, 1, 1)"),
        EvaluatedCellValue::Number(45292.0)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=ROUND(2.675, 2)"),
        EvaluatedCellValue::Number(2.68)
    );
    assert_eq!(
        sheet.evaluate_formula_str("=\"1\"=1"),
        EvaluatedCellValue::Boolean(false)
    );
    let deep_sum = format!("={}", vec!["1"; 250].join("+"));
    assert_eq!(
        sheet.evaluate_formula_str(&deep_sum),
        EvaluatedCellValue::Number(250.0)
    );
    let over_nested = format!("={}1{}", "(".repeat(70), ")".repeat(70));
    assert_eq!(
        sheet.evaluate_formula_str(&over_nested),
        EvaluatedCellValue::Error(FormulaError::Value)
    );
}

#[test]
fn test_adversarial_sheet_remediations() {
    let mut chain_sheet = SheetEngine::new("Chain500");
    chain_sheet.set_cell("A1", "1").unwrap();
    for r in 2..=500 {
        let prev = r - 1;
        chain_sheet
            .set_cell(&format!("A{r}"), format!("=A{prev}+1"))
            .unwrap();
    }
    assert_eq!(
        chain_sheet.evaluate_cell("A500"),
        EvaluatedCellValue::Number(500.0)
    );

    let mut s = SheetEngine::new("SpecRemediation");
    assert_eq!(
        s.evaluate_formula_str("=1E5"),
        EvaluatedCellValue::Number(100_000.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=1.5e-3"),
        EvaluatedCellValue::Number(0.0015)
    );
    assert_eq!(
        s.evaluate_formula_str("=1E+10"),
        EvaluatedCellValue::Number(1e10)
    );

    assert_eq!(
        s.evaluate_formula_str("=ROUND(0.4999999999995, 0)"),
        EvaluatedCellValue::Number(0.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=ROUND(10000.005, 2)"),
        EvaluatedCellValue::Number(10000.01)
    );
    if let EvaluatedCellValue::Number(z) = s.evaluate_formula_str("=ROUND(-0.0001, 0)") {
        assert_eq!(z, 0.0);
        assert!(!z.is_sign_negative());
    } else {
        panic!("Expected numeric zero");
    }

    assert_eq!(
        s.evaluate_formula_str("=1E-13>0"),
        EvaluatedCellValue::Boolean(true)
    );
    assert_eq!(
        s.evaluate_formula_str("=1E-13=0"),
        EvaluatedCellValue::Boolean(false)
    );

    assert_eq!(
        s.evaluate_formula_str("=DATE(1900, 1, 1)"),
        EvaluatedCellValue::Number(1.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=DATE(1900, 2, 28)"),
        EvaluatedCellValue::Number(59.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=DATE(1900, 2, 29)"),
        EvaluatedCellValue::Number(60.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=DATE(1900, 3, 1)"),
        EvaluatedCellValue::Number(61.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=YEAR(60)"),
        EvaluatedCellValue::Number(1900.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=MONTH(60)"),
        EvaluatedCellValue::Number(2.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=DAY(60)"),
        EvaluatedCellValue::Number(29.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=DATE(1E20, 1, 1)"),
        EvaluatedCellValue::Error(FormulaError::Num)
    );
    assert_eq!(
        s.evaluate_formula_str("=YEAR(1E20)"),
        EvaluatedCellValue::Error(FormulaError::Num)
    );
    assert_eq!(
        s.evaluate_formula_str("=ISEVEN(1E20)"),
        EvaluatedCellValue::Error(FormulaError::Num)
    );

    if let EvaluatedCellValue::Number(rate) =
        s.evaluate_formula_str("=RATE(48, -250, 10000, 0, 0, 0.9)")
    {
        assert!(rate > 0.0 && rate < 0.05, "rate={rate}");
    } else {
        panic!("Expected numeric RATE");
    }

    assert_eq!(
        s.evaluate_formula_str("=SWITCH(\"a*b\", \"a1b\", \"wrong\", \"a*b\", \"exact\")"),
        EvaluatedCellValue::Text("exact".to_string())
    );

    s.set_cell("A1", "a1b").unwrap();
    s.set_cell("A2", "a*b").unwrap();
    s.set_cell("A3", "100").unwrap();
    s.set_cell("A4", "TRUE").unwrap();
    s.set_cell("B1", "10").unwrap();
    s.set_cell("B2", "20").unwrap();
    s.set_cell("B3", "30").unwrap();
    s.set_cell("B4", "40").unwrap();
    assert_eq!(
        s.evaluate_formula_str("=XLOOKUP(\"a*b\", A1:A4, B1:B4, -1, 0)"),
        EvaluatedCellValue::Number(20.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=XLOOKUP(\"a*b\", A1:A4, B1:B4, -1, 2)"),
        EvaluatedCellValue::Number(10.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=COUNTIF(A1:A4, \"*\")"),
        EvaluatedCellValue::Number(2.0)
    );

    s.set_cell("C1", "10").unwrap();
    s.set_cell("C2", "20").unwrap();
    s.set_cell("C3", "TRUE").unwrap();
    s.set_cell("C4", "not-a-number").unwrap();
    assert_eq!(
        s.evaluate_formula_str("=AVERAGE(C1:C4)"),
        EvaluatedCellValue::Number(15.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=MEDIAN(C1:C4)"),
        EvaluatedCellValue::Number(15.0)
    );
    assert_eq!(
        s.evaluate_formula_str("=AVERAGE(10, TRUE)"),
        EvaluatedCellValue::Number(5.5)
    );

    let mut sort_sheet = SheetEngine::new("SortRel");
    sort_sheet.set_cell("A1", "10").unwrap();
    sort_sheet.set_cell("B1", "=A1*2").unwrap();
    sort_sheet.set_cell("C1", "=$A$1*2").unwrap();
    sort_sheet.set_cell("A2", "30").unwrap();
    sort_sheet.set_cell("B2", "=A2*2").unwrap();
    sort_sheet.set_cell("C2", "=$A$1*2").unwrap();
    sort_sheet.sort_rows(0, 1, 0, false, None, true).unwrap();
    let raw_at = |s: &SheetEngine, r: &str| -> Option<String> {
        s.raw_cells()
            .get(&PackedCellCoordinate::parse_a1(r).unwrap())
            .cloned()
    };
    assert_eq!(raw_at(&sort_sheet, "A1").as_deref(), Some("30"));
    assert_eq!(raw_at(&sort_sheet, "B1").as_deref(), Some("=A1*2"));
    assert_eq!(raw_at(&sort_sheet, "C1").as_deref(), Some("=$A$1*2"));
    assert_eq!(
        sort_sheet.evaluate_cell("B1"),
        EvaluatedCellValue::Number(60.0)
    );
    assert_eq!(raw_at(&sort_sheet, "A2").as_deref(), Some("10"));
    assert_eq!(raw_at(&sort_sheet, "B2").as_deref(), Some("=A2*2"));
    assert_eq!(
        sort_sheet.evaluate_cell("B2"),
        EvaluatedCellValue::Number(20.0)
    );
}

#[test]
fn test_f1_string_amplification_bomb_caps_at_32767_chars() {
    let mut sheet = SheetEngine::new("StringBombGuard");
    sheet.set_cell("A1", "=REPT(\"A\", 20000)").unwrap();
    sheet.set_cell("A2", "=A1 & A1").unwrap();
    assert_eq!(
        sheet.evaluate_cell("A2"),
        EvaluatedCellValue::Error(FormulaError::Value)
    );

    sheet.set_cell("A3", "=CONCAT(A1, A1)").unwrap();
    assert_eq!(
        sheet.evaluate_cell("A3"),
        EvaluatedCellValue::Error(FormulaError::Value)
    );

    sheet.set_cell("A4", "=CONCATENATE(A1, A1)").unwrap();
    assert_eq!(
        sheet.evaluate_cell("A4"),
        EvaluatedCellValue::Error(FormulaError::Value)
    );

    sheet
        .set_cell("A5", "=TEXTJOIN(\"-\", TRUE, A1, A1)")
        .unwrap();
    assert_eq!(
        sheet.evaluate_cell("A5"),
        EvaluatedCellValue::Error(FormulaError::Value)
    );

    sheet.set_cell("B1", "=REPT(\"A\", 2000)").unwrap();
    sheet
        .set_cell("B2", "=SUBSTITUTE(B1, \"A\", \"BBBBBBBBBBBBBBBBBBBB\")")
        .unwrap();
    assert_eq!(
        sheet.evaluate_cell("B2"),
        EvaluatedCellValue::Error(FormulaError::Value)
    );

    sheet
        .set_cell("B3", "=SUBSTITUTE(A1, \"A\", A1, 1)")
        .unwrap();
    assert_eq!(
        sheet.evaluate_cell("B3"),
        EvaluatedCellValue::Error(FormulaError::Value)
    );

    sheet.set_cell("B4", "=REPLACE(A1, 1, 1, A1)").unwrap();
    assert_eq!(
        sheet.evaluate_cell("B4"),
        EvaluatedCellValue::Error(FormulaError::Value)
    );

    let fmt_zeros = format!("0.{}", "0".repeat(500));
    sheet
        .set_cell("B5", format!("=TEXT(1.25, \"{fmt_zeros}\")"))
        .unwrap();
    if let EvaluatedCellValue::Text(formatted) = sheet.evaluate_cell("B5") {
        assert_eq!(formatted, "1.2500000000");
    } else {
        panic!("Expected bounded TEXT output");
    }

    let raw_over = "가".repeat(32_768);
    sheet.set_cell("C1", raw_over).unwrap();
    assert_eq!(
        sheet.evaluate_cell("C1"),
        EvaluatedCellValue::Error(FormulaError::Value)
    );
}

#[test]
fn test_f2_cycle_memoization_and_eval_step_budget() {
    let mut sheet = SheetEngine::new("CycleMemo");
    sheet.set_cell("A1", "=A2").unwrap();
    sheet.set_cell("A2", "=A1").unwrap();
    for idx in 1..=25u32 {
        let prev = if idx == 1 {
            "A1".to_string()
        } else {
            format!("B{}", idx - 1)
        };
        sheet
            .set_cell(
                &format!("B{idx}"),
                format!("=IFERROR({prev}, 1) + IFERROR({prev}, 1)"),
            )
            .unwrap();
    }

    let coord_a1 = PackedCellCoordinate::parse_a1("A1").unwrap();
    let mut ctx = crate::EvalContext::standalone_id(sheet.resolved_sheet_id());
    let a1_val = sheet.eval_coord_ctx(coord_a1, &mut ctx, 0);
    assert_eq!(a1_val, EvaluatedCellValue::Error(FormulaError::Cycle));
    let key_a1 = crate::PackedWorkbookCoord::new(sheet.resolved_sheet_id(), coord_a1);
    assert_eq!(
        ctx.memo.get(&key_a1),
        Some(&EvaluatedCellValue::Error(FormulaError::Cycle))
    );

    assert_eq!(
        sheet.evaluate_cell("B25"),
        EvaluatedCellValue::Number(33_554_432.0)
    );
}

#[test]
fn test_f12_canonical_122_coverage_completeness() {
    let test_src = include_str!("workbook_tests.rs");
    let fn_start = test_src
        .find("fn test_all_122_canonical_functions_evaluated()")
        .unwrap();
    let fn_end = test_src[fn_start..]
        .find("fn test_workbook_cross_sheet_formulas_and_sort_goal_seek_chart()")
        .unwrap()
        + fn_start;
    let body = &test_src[fn_start..fn_end];
    let mut missing = Vec::new();
    for &func_name in SheetEngine::supported_function_names() {
        let pat = format!("{func_name}(");
        if !body.contains(&pat) {
            missing.push(func_name);
        }
    }
    assert!(
        missing.is_empty(),
        "Missing canonical functions in test_all_122_canonical_functions_evaluated: {missing:?}"
    );
}

#[test]
fn test_sheet_whitespace_trim_and_apostrophe_cross_sheet_lookup() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet(" MySheet ");
    wb.set_cell(" MySheet ", "A1", "125").unwrap();
    assert_eq!(
        wb.evaluate_cell("MySheet", "A1"),
        EvaluatedCellValue::Number(125.0)
    );
    assert_eq!(
        wb.evaluate_cell(" MySheet ", "A1"),
        EvaluatedCellValue::Number(125.0)
    );

    wb.add_sheet("'특수시트'");
    wb.set_cell("'특수시트'", "A1", "250").unwrap();
    wb.add_sheet("Sheet1");
    wb.set_cell("Sheet1", "B1", "='특수시트'!A1+50").unwrap();
    wb.set_cell("Sheet1", "B2", "='''특수시트'''!A1+50")
        .unwrap();
    assert_eq!(
        wb.evaluate_cell("Sheet1", "B1"),
        EvaluatedCellValue::Number(300.0)
    );
    assert_eq!(
        wb.evaluate_cell("Sheet1", "B2"),
        EvaluatedCellValue::Number(300.0)
    );
    assert_eq!(
        wb.evaluate_cell("없는시트", "A1"),
        EvaluatedCellValue::Error(FormulaError::Ref)
    );
    assert!(WorkbookEngine::is_same_sheet_identity(
        "특수시트",
        "'특수시트'"
    ));
    assert!(WorkbookEngine::is_same_sheet_identity(
        "'특수시트'",
        "'''특수시트'''"
    ));
    assert!(WorkbookEngine::is_same_sheet_identity(
        "특수시트",
        "'''특수시트'''"
    ));
    assert!(!WorkbookEngine::is_same_sheet_identity("ß", "SS"));

    let mut wb_rev = WorkbookEngine::new();
    wb_rev.add_sheet("특수시트");
    wb_rev.add_sheet("'특수시트'");
    wb_rev.set_cell("특수시트", "A1", "250").unwrap();
    wb_rev.add_sheet("Sheet1");
    wb_rev
        .set_cell("Sheet1", "B2", "='''특수시트'''!A1+50")
        .unwrap();
    assert_eq!(
        wb_rev.evaluate_cell("Sheet1", "B2"),
        EvaluatedCellValue::Number(300.0)
    );

    wb_rev.set_cell("특수시트", "A2", "75").unwrap();
    wb_rev
        .set_cell("Sheet1", "B3", "=SUM('특수시트'!A1:'''특수시트'''!A2)")
        .unwrap();
    assert_eq!(
        wb_rev.evaluate_cell("Sheet1", "B3"),
        EvaluatedCellValue::Number(325.0)
    );

    assert!(WorkbookEngine::is_same_sheet_identity("A'B", "'A''B'"));
    assert!(WorkbookEngine::is_same_sheet_identity(
        "'A'B'",
        "'''A''B'''"
    ));
    assert!(!WorkbookEngine::is_same_sheet_identity("A'B", "'A'B'"));
    assert!(WorkbookEngine::is_same_sheet_identity(
        "' Sheet1 '",
        "Sheet1"
    ));
    assert!(!WorkbookEngine::is_same_sheet_identity("'   '", "''"));

    let mut wb_distinct = WorkbookEngine::new();
    wb_distinct.add_sheet("A'B");
    wb_distinct.add_sheet("'A'B'");
    assert_eq!(wb_distinct.sheet_count(), 2);
    wb_distinct.set_cell("A'B", "A1", "11").unwrap();
    wb_distinct.set_cell("'A'B'", "A1", "29").unwrap();
    wb_distinct.add_sheet("Main");
    wb_distinct.set_cell("Main", "A1", "='A''B'!A1").unwrap();
    wb_distinct
        .set_cell("Main", "A2", "='''A''B'''!A1")
        .unwrap();
    wb_distinct.set_cell("Main", "A3", "=' Main '!A1").unwrap();
    assert_eq!(
        wb_distinct.evaluate_cell("Main", "A1"),
        EvaluatedCellValue::Number(11.0)
    );
    assert_eq!(
        wb_distinct.evaluate_cell("Main", "A2"),
        EvaluatedCellValue::Number(29.0)
    );
    assert_eq!(
        wb_distinct.evaluate_cell("Main", "A3"),
        EvaluatedCellValue::Number(11.0)
    );
}
