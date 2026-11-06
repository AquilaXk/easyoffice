use easyoffice_sheet::{
    EvaluatedCellValue, FormulaError, SheetEngine, WorkbookEngine, WorkbookEvalTarget,
};

fn text(s: &str) -> EvaluatedCellValue {
    EvaluatedCellValue::Text(s.to_string())
}

#[test]
fn test_issue_107_text_format_code_table() {
    let sheet = SheetEngine::new("Sheet1");
    let cases = [
        ("=TEXT(1234567,\"#,##0원\")", "1,234,567원"),
        ("=TEXT(3,\"00\")", "03"),
        ("=TEXT(-1234,\"#,##0;(#,##0)\")", "(1,234)"),
        ("=TEXT(45000,\"ddd\")", "Wed"),
        ("=TEXT(45000,\"mmm d, yyyy\")", "Mar 15, 2023"),
        ("=TEXT(45000.75,\"yyyy-mm-dd hh:mm\")", "2023-03-15 18:00"),
        ("=TEXT(0.75,\"hh:mm\")", "18:00"),
    ];
    for (formula, expected) in cases {
        assert_eq!(
            sheet.evaluate_formula_str(formula),
            text(expected),
            "formula={formula}"
        );
    }
}

#[test]
fn test_text_format_codes_beyond_issue_table() {
    let sheet = SheetEngine::new("Sheet1");
    let cases = [
        ("=TEXT(1234.5,\"#,##0.00\")", "1,234.50"),
        ("=TEXT(12.345,\"0.00\")", "12.35"),
        ("=TEXT(2.5,\"0\")", "3"),
        ("=TEXT(0.256,\"0.0%\")", "25.6%"),
        ("=TEXT(-1234.5,\"$#,##0.00\")", "-$1,234.50"),
        ("=TEXT(1234567,\"#,##0,\")", "1,235"),
        ("=TEXT(1234567,\"0.0,,\\M\")", "1.2M"),
        ("=TEXT(1.5,\"0.0#\")", "1.5"),
        ("=TEXT(1.256,\"0.0#\")", "1.26"),
        ("=TEXT(0.5,\"#.00\")", ".50"),
        ("=TEXT(5,\"0,000\")", "0,005"),
        ("=TEXT(1234,\"\\₩#,##0\")", "₩1,234"),
        ("=TEXT(1234,\"₩#,##0\")", "₩1,234"),
        ("=TEXT(1234,\"[$₩-412]#,##0\")", "₩1,234"),
        ("=TEXT(42,\"\"\"합계: \"\"0\"\" 건\"\"\")", "합계: 42 건"),
        ("=TEXT(0,\"0;(0);\"\"zero\"\"\")", "zero"),
        ("=TEXT(-5,\"0;(0);\"\"zero\"\"\")", "(5)"),
        ("=TEXT(12345,\"0.00E+00\")", "1.23E+04"),
        ("=TEXT(0.000123,\"0.0E+0\")", "1.2E-4"),
        ("=TEXT(1234567,\"000-0000\")", "123-4567"),
        ("=TEXT(1234.5678,\"General\")", "1234.5678"),
        ("=TEXT(45000,\"dddd, mmmm d\")", "Wednesday, March 15"),
        ("=TEXT(45000,\"yy/m/d\")", "23/3/15"),
        ("=TEXT(45000,\"mmmmm\")", "M"),
        ("=TEXT(45000,\"yyyy-mm-dd\")", "2023-03-15"),
        ("=TEXT(0.75,\"h:mm AM/PM\")", "6:00 PM"),
        ("=TEXT(0.25,\"h:mm am/pm\")", "6:00 am"),
        ("=TEXT(0.5,\"hh:mm:ss A/P\")", "12:00:00 P"),
        ("=TEXT(1.5,\"[h]:mm\")", "36:00"),
        ("=TEXT(0.0625,\"[mm]:ss\")", "90:00"),
        ("=TEXT(0.000011574,\"[s]\")", "1"),
        ("=TEXT(45000.5,\"m/d/yyyy h:mm:ss\")", "3/15/2023 12:00:00"),
        ("=TEXT(0.123456,\"hh:mm:ss.00\")", "02:57:46.60"),
        ("=TEXT(1,\"mm:ss\")", "00:00"),
        ("=TEXT(\"abc\",\"0;-0;0;\"\"[\"\"@\"\"]\"\"\")", "[abc]"),
        ("=TEXT(\"abc\",\"0.00\")", "abc"),
        ("=TEXT(7,\"@\")", "7"),
        ("=TEXT(1234567,\"#,##0 원\")", "1,234,567 원"),
        ("=TEXT(3,\"#\")", "3"),
        ("=TEXT(0,\"#\")", ""),
        ("=TEXT(0,\"0.00\")", "0.00"),
    ];
    for (formula, expected) in cases {
        assert_eq!(
            sheet.evaluate_formula_str(formula),
            text(expected),
            "formula={formula}"
        );
    }
    assert_eq!(
        sheet.evaluate_formula_str("=TEXT(-1,\"yyyy-mm-dd\")"),
        EvaluatedCellValue::Error(FormulaError::Value)
    );
}

#[test]
fn test_wide_range_aggregates_stream_populated_cells_only() {
    let mut sheet = SheetEngine::new("Sheet1");
    sheet.set_cell("A1", "10").unwrap();
    sheet.set_cell("Z10000", "20").unwrap();
    sheet.set_cell("M5000", "가나다").unwrap();
    sheet.set_cell("C3", "=A1*3").unwrap();
    sheet.set_cell("XFD1048576", "5").unwrap();

    let expect = |formula: &str, expected: EvaluatedCellValue| {
        assert_eq!(
            sheet.evaluate_formula_str(formula),
            expected,
            "formula={formula}"
        );
    };
    let num = EvaluatedCellValue::Number;
    expect("=SUM(A1:Z10000)", num(60.0));
    expect("=COUNT(A1:Z10000)", num(3.0));
    expect("=COUNTA(A1:Z10000)", num(4.0));
    expect("=AVERAGE(A1:Z10000)", num(20.0));
    expect("=MIN(A1:Z10000)", num(10.0));
    expect("=MAX(A1:Z10000)", num(30.0));
    expect("=SUMIF(A1:Z10000,\">15\")", num(50.0));
    expect("=COUNTIF(A1:Z10000,\">15\")", num(2.0));
    expect("=AVERAGEIF(A1:Z10000,\">15\")", num(25.0));
    expect("=SUMIFS(A1:Z10000,A1:Z10000,\">5\")", num(60.0));
    expect("=COUNTIFS(A1:Z10000,\"<>\")", num(4.0));
    expect("=COUNTBLANK(A1:Z10000)", num(260_000.0 - 4.0));
    expect("=SUM(A1:XFD1048576)", num(65.0));
    expect("=COUNT(A:XFD)", num(4.0));
    expect("=MAX(A1:XFD1048576)", num(30.0));
    expect("=COUNTIF(A1:XFD1048576,\"\")", num(17_179_869_184.0 - 5.0));
    expect("=SUM(Sheet1!A1:Z10000)", num(60.0));
}

#[test]
fn test_wide_range_dense_only_functions_keep_budget_guard() {
    let mut sheet = SheetEngine::new("Sheet1");
    sheet.set_cell("A1", "1").unwrap();
    let started = std::time::Instant::now();
    let res = sheet.evaluate_formula_str("=SUMPRODUCT(A1:XFD1048576*2)");
    assert!(matches!(res, EvaluatedCellValue::Error(_)), "res={res:?}");
    let res_match = sheet.evaluate_formula_str("=MATCH(99,A1:XFD1048576,0)");
    assert!(matches!(res_match, EvaluatedCellValue::Error(_)));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "dense range budget guard must bound evaluation time"
    );
}

#[test]
fn test_workbook_batch_targets_share_memo_and_flag_circular_references() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Data");
    for r in 1..=2000u32 {
        wb.set_cell("Data", &format!("A{r}"), r.to_string())
            .unwrap();
        wb.set_cell("Data", &format!("B{r}"), format!("=SUM($A$1:A{r})"))
            .unwrap();
    }
    let addrs: Vec<String> = (1..=2000u32).map(|r| format!("B{r}")).collect();
    let targets: Vec<WorkbookEvalTarget<'_>> = addrs
        .iter()
        .map(|a| WorkbookEvalTarget {
            sheet: "Data",
            expr: a.as_str(),
            anchor: None,
        })
        .collect();
    let batch = wb.evaluate_targets(&targets);
    assert_eq!(batch.values.len(), 2000);
    assert_eq!(
        batch.values[1999],
        EvaluatedCellValue::Number(2000.0 * 2001.0 / 2.0)
    );
    assert!(!batch.circular);

    wb.set_cell("Data", "C1", "=C2+1").unwrap();
    wb.set_cell("Data", "C2", "=C1+1").unwrap();
    wb.set_cell("Data", "C3", "=IFERROR(C1,0)").unwrap();
    let cyc = wb.evaluate_targets(&[
        WorkbookEvalTarget {
            sheet: "Data",
            expr: "C1",
            anchor: None,
        },
        WorkbookEvalTarget {
            sheet: "Data",
            expr: "C3",
            anchor: None,
        },
    ]);
    assert!(cyc.circular);
    assert_eq!(cyc.values[0].to_display_string(), "#NUM!");
    assert_eq!(cyc.values[1], EvaluatedCellValue::Number(0.0));
}

#[test]
fn test_circular_reference_displays_standard_num_error() {
    let mut sheet = SheetEngine::new("Sheet1");
    sheet.set_cell("A1", "=B1+1").unwrap();
    sheet.set_cell("B1", "=A1+1").unwrap();
    let val = sheet.evaluate_cell("A1");
    assert_eq!(val, EvaluatedCellValue::Error(FormulaError::Cycle));
    assert_eq!(val.to_display_string(), "#NUM!");
    assert_eq!(FormulaError::Cycle.code(), "#NUM!");
    assert_eq!(
        sheet.evaluate_formula_str("=ISERROR(A1)"),
        EvaluatedCellValue::Boolean(true)
    );
}
