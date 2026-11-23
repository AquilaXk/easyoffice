//! Integration tests for the dynamic `:` range operator, remaining `@` compositions,
//! `INDEX`/`CHOOSE`/`IF` reference passing, `RecalcSession` dynamic reads, CF template shifting,
//! and fail-closed bounds.

mod dynamic_support;
use dynamic_support::{new_empty_model, DefinedNameEntry};
use easyoffice_formats::{parse_sqref, CellBorders, CfKind, CfRule, CfStyle, SheetRules};
use easyoffice_sheet::{EvaluatedCellValue, PackedCellCoordinate, WorkbookEngine};

#[test]
fn dynamic_colon_endpoints_resolve_all_ref_producers_and_spill_ranges() {
    let mut m = new_empty_model();
    for r in 1..=5 {
        m._set(&format!("A{r}"), &(r * 10).to_string());
        m._set(&format!("B{r}"), &(r * 100).to_string());
        m._set(&format!("C{r}"), &(r * 1000).to_string());
        m._set(&format!("Sheet2!A{r}"), &r.to_string());
    }

    m._set("E1", "=SUM(OFFSET(A1,0,1):OFFSET(A1,2,2))");
    m._set("E2", "=SUM(A1:INDEX(B1:C5,3,2))");
    m._set("E3", "=SUM(A1:CHOOSE(2,B2,B4))");
    m._set("E4", "=SUM(A1:IF(TRUE,B3,B5))");
    m._set("E5", "=SUM(A1:INDIRECT(\"B4\"))");
    m._set("E6", "=SUM(Sheet2!A1:OFFSET(Sheet2!A1,3,0))");
    m._set("E7", "=ROWS(B1:OFFSET(B1,3,0))");
    m._set("E8", "=COLUMNS(A1:OFFSET(A1,0,2))");
    m._set("E9", "=ISREF(A1:OFFSET(A1,2,1))");
    m._set("E10", "=MAXA(B1:OFFSET(B1,2,0))");
    m._set("E11", "=A1:OFFSET($A$1,2,0)");

    m.evaluate();

    // OFFSET(A1,0,1):OFFSET(A1,2,2) -> B1:C3 -> 600 + 6000 = 6600
    assert_eq!(m._get_text("E1"), "6600");
    // A1:INDEX(B1:C5,3,2) -> A1:C3 -> 60 + 600 + 6000 = 6660
    assert_eq!(m._get_text("E2"), "6660");
    // A1:CHOOSE(2,B2,B4) -> A1:B4 -> 100 + 1000 = 1100
    assert_eq!(m._get_text("E3"), "1100");
    // A1:IF(TRUE,B3,B5) -> A1:B3 -> 60 + 600 = 660
    assert_eq!(m._get_text("E4"), "660");
    // A1:INDIRECT("B4") -> A1:B4 -> 1100
    assert_eq!(m._get_text("E5"), "1100");
    // Sheet2!A1:Sheet2!A4 -> 1 + 2 + 3 + 4 = 10
    assert_eq!(m._get_text("E6"), "10");
    assert_eq!(m._get_text("E7"), "4");
    assert_eq!(m._get_text("E8"), "3");
    assert_eq!(m._get_text("E9"), "TRUE");
    assert_eq!(m._get_text("E10"), "300");
    assert_eq!(m._get_text("E11"), "10");
    assert_eq!(m._get_text("E12"), "20");
    assert_eq!(m._get_text("E13"), "30");
}

#[test]
fn at_colon_names_let_lambda_and_single_cell_index_compose_as_references() {
    let mut m = new_empty_model();
    for r in 1..=5 {
        m._set(&format!("A{r}"), &(r * 2).to_string());
        m._set(&format!("B{r}"), &(r * 5).to_string());
    }
    m._set("C1", "=999");
    m.new_defined_name(DefinedNameEntry::new("MyEnd", "=Sheet1!$B$4"))
        .unwrap();
    m.new_defined_name(DefinedNameEntry::new("MyCol", "=Sheet1!$A$1:$A$3"))
        .unwrap();

    // `@` on either side of `:`, plus `@INDEX`, `@x` in LET, single-cell INDEX as ref
    m._set("D2", "=SUM(A1:@B1:B5)");
    m._set("D3", "=SUM((@B1:B5):A5)");
    m._set("D4", "=SUM(A1:MyEnd)");
    m._set("D5", "=SUM(MyCol:B4)");
    m._set("D6", "=LET(x, B3, SUM(A1:x))");
    m._set("D7", "=LET(n, 3, SUM(A1:OFFSET(A1, n, 0)))");
    m._set("D8", "=LET(x, B1:B5, OFFSET(@x, 1, 0))");
    m._set("D9", "=LAMBDA(r, SUM(A1:r))(B3)");
    m._set("F1", "=ISREF(INDEX(A1:C5, 1, 3))");
    m._set("F2", "=CELL(\"address\", INDEX(A1:C5, 1, 3))");
    m._set("F3", "=FORMULATEXT(INDEX(A1:C5, 1, 3))");
    m._set("F4", "=SUM((A1:OFFSET(A1, 2, 0))*10)");

    m.evaluate();

    // D2: @B1:B5 = B2, A1:B2 = (2+4) + (5+10) = 21
    assert_eq!(m._get_text("D2"), "21");
    // D3: @B1:B5 = B3, B3:A5 -> A3:B5 = (6+8+10) + (15+20+25) = 84
    assert_eq!(m._get_text("D3"), "84");
    // A1:B4 = (2+4+6+8) + (5+10+15+20) = 70
    assert_eq!(m._get_text("D4"), "70");
    // MyCol(A1:A3):B4 -> A1:B4 = 70
    assert_eq!(m._get_text("D5"), "70");
    // A1:B3 = (2+4+6) + (5+10+15) = 42
    assert_eq!(m._get_text("D6"), "42");
    // A1:A4 = 2+4+6+8 = 20
    assert_eq!(m._get_text("D7"), "20");
    // OFFSET(B8? No, @x in D8 is B8 -> wait: B1:B5 on row 8 is out of range! Check row alignment!
    // NOTE: D8 is row 8, outside B1:B5 -> #VALUE!; put valid row-aligned test on D5/G2 below
    assert_eq!(m._get_text("D8"), "#VALUE!");
    m._set("G2", "=LET(x, B1:B5, OFFSET(@x, 1, 0))");
    m.evaluate();
    assert_eq!(m._get_text("G2"), "15");
    assert_eq!(m._get_text("D9"), "42");
    assert_eq!(m._get_text("F1"), "TRUE");
    assert_eq!(m._get_text("F2"), "$C$1");
    assert_eq!(m._get_text("F3"), "=999");
    assert_eq!(m._get_text("F4"), "120");
}

#[test]
fn recalc_session_tracks_spill_endpoints_and_interior_dynamic_range_edits() {
    let mut m = new_empty_model();
    for r in 1..=5 {
        m._set(&format!("B{r}"), "10");
        m._set(&format!("C{r}"), "100");
        m._set(&format!("D{r}"), "1");
    }
    m._set("E1", "=2");
    // Spill endpoint A1:B1# grows when F1 changes
    m._set("F1", "=SEQUENCE(2)");
    m._set("G1", "=SUM(E1:F1#)");
    // Non-volatile INDEX interior range B1:INDEX(D1:D5, E1) spans C1:C2 not listed statically
    m._set("G2", "=SUM(B1:INDEX(D1:D5, E1))");
    m.evaluate();

    // E1:F2 -> E1(2) + E2(0) + F1(1) + F2(2) = 5
    assert_eq!(m._get_text("G1"), "5");
    // B1:D2 -> B1+B2(20) + C1+C2(200) + D1+D2(2) = 222
    assert_eq!(m._get_text("G2"), "222");

    // Edit interior cell C2 inside B1:INDEX(D1:D5, E1)
    m._set("C2", "500");
    m.evaluate();
    assert_eq!(m._get_text("G2"), "622");

    // Grow spill at F1 -> SEQUENCE(4) -> SUM(E1:F1#) expands to E1:F4
    m._set("F1", "=SEQUENCE(4)");
    m.evaluate();
    assert_eq!(m._get_text("G1"), "12");
}

#[test]
fn conditional_formatting_relative_shift_works_with_dynamic_colon_ranges() {
    let mut wb = WorkbookEngine::new();
    let _ = wb.add_sheet("Sheet1");
    wb.set_cell("Sheet1", "B1", "10").unwrap();
    wb.set_cell("Sheet1", "B2", "5").unwrap();
    wb.set_cell("Sheet1", "B3", "1").unwrap();
    wb.set_cell("Sheet1", "A1", "1").unwrap();
    wb.set_cell("Sheet1", "A2", "2").unwrap();
    wb.set_cell("Sheet1", "A3", "3").unwrap();
    let rules = SheetRules {
        cf: vec![CfRule {
            ranges: parse_sqref("C1:C3"),
            priority: 1,
            stop_if_true: false,
            kind: CfKind::Expression("SUM(B1:OFFSET($B$1,0,0))>10".to_string()),
            style: CfStyle {
                bg: Some(0xFF_00_00),
                fg: None,
                flags: 0,
                border: CellBorders::NONE,
            },
        }],
        ..SheetRules::default()
    };
    let visuals = wb.evaluate_rules("Sheet1", rules);
    // C1: SUM(B1:B1)=10 (false), C2: SUM(B2:B1)=15 (true), C3: SUM(B3:B1)=16 (true)
    assert_eq!(visuals.len(), 2);
    let coords: Vec<_> = visuals.iter().map(|v| v.coord).collect();
    assert!(!coords.contains(&PackedCellCoordinate::parse_a1("C1").unwrap()));
    assert!(coords.contains(&PackedCellCoordinate::parse_a1("C2").unwrap()));
    assert!(coords.contains(&PackedCellCoordinate::parse_a1("C3").unwrap()));
}

#[test]
fn hostile_and_invalid_dynamic_colon_ranges_fail_closed() {
    let mut m = new_empty_model();
    m._set("A1", "10");

    // Cross-sheet endpoints -> #REF!
    m._set("B1", "=SUM(Sheet1!A1:OFFSET(Sheet2!A1,1,0))");
    // Out-of-grid offset -> #REF!
    m._set("B2", "=SUM(A1:OFFSET(A1,-5,0))");
    // Error endpoint propagates
    m._set("B3", "=SUM(A1:(1/0))");
    // Non-reference endpoints -> #VALUE!
    m._set("B4", "=SUM(A1:5)");
    m._set("B5", "=SUM(A1:SEQUENCE(3))");
    m._set("B6", "=ROW(FILTER(A1:A3,A1:A3>0))");
    m._set("B7", "=ROW(CHOOSE(1,SEQUENCE(3),99))");
    m._set("B8", "=CELL(\"address\",SEQUENCE(3))");
    m._set("B9", "=FORMULATEXT(SEQUENCE(3))");
    m._set("B10", "=ISFORMULA(SEQUENCE(3))");
    // Excessive dynamic array spill -> #SPILL!
    m._set("D1", "=A1:OFFSET(A1,999999,100)");
    // Deep colon chain > MAX_PARSE_DEPTH -> #VALUE!
    let deep_static = format!("=SUM(A1{})", ":A1".repeat(160));
    let deep_dyn = format!("=SUM(A1{})", ":OFFSET(A1,0,0)".repeat(70));
    m._set("B11", &deep_static);
    m._set("B12", &deep_dyn);

    m.evaluate();

    assert_eq!(m._get_text("B1"), "#REF!");
    assert_eq!(m._get_text("B2"), "#REF!");
    assert_eq!(m._get_text("B3"), "#DIV/0!");
    assert_eq!(m._get_text("B4"), "#VALUE!");
    assert_eq!(m._get_text("B5"), "#VALUE!");
    assert_eq!(m._get_text("B6"), "#VALUE!");
    assert_eq!(m._get_text("B7"), "#VALUE!");
    assert_eq!(m._get_text("B8"), "#VALUE!");
    assert_eq!(m._get_text("B9"), "#N/A");
    assert_eq!(m._get_text("B10"), "#N/A");
    assert_eq!(m._get_text("B11"), "#VALUE!");
    assert_eq!(m._get_text("B12"), "#VALUE!");
    assert_eq!(
        m.value("D1"),
        EvaluatedCellValue::Error(easyoffice_sheet::FormulaError::Spill)
    );
}
