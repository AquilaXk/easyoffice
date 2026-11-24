//! Integration tests for `TRIMRANGE` pre-clipping over full column/row/sheet references,
//! dynamic reference propagation (`ISREF`, `ROW`, `ROWS`, `CELL`, `SUM`), spill/overlay handling,
//! `RecalcSession` invalidation, and fail-closed budget enforcement (`PD-329`).

mod dynamic_support;
use dynamic_support::{new_empty_model, DefinedNameEntry};
use easyoffice_formats::{parse_sqref, Cmp, DataValidation, DvKind, SheetRules};
use easyoffice_sheet::{EvaluatedCellValue, FormulaError, PackedCellCoordinate, Verdict};

#[test]
fn trimrange_full_columns_rows_and_full_sheet_pre_clip_to_used_bounds() {
    let mut m = new_empty_model();
    m._set("A500000", "10");
    m._set("A500002", "20");
    m._set("B3", "1");
    m._set("D5", "2");
    m._set("Sheet2!B900000", "7");
    m._set("Sheet2!D900001", "8");
    m._set("F1", "100");
    m._set("XFD1", "200");
    m._set("AA8", "55");
    m._set("AC8", "66");

    // Full single column with leading/trailing blanks and interior blank
    m._set("G1", "=TRIMRANGE(A:A)");
    // Multi-column full reference (3,145,728 cells unclipped > MAX_ARRAY_CELLS)
    m._set("G10", "=TRIMRANGE(B:D)");
    // Cross-sheet full-column reference near the bottom of the grid
    m._set("G20", "=TRIMRANGE(Sheet2!B:D)");
    // Full row reference (16,384 cells wide) spilling trimmed 1x3 row
    m._set("G30", "=TRIMRANGE(8:8)");
    // Interior blanks preserved between sparse extremities in full column
    m._set("G40", "=COUNTA(TRIMRANGE(A:A))");
    m._set("G41", "=ROWS(TRIMRANGE(A:A))");
    m._set("G42", "=COLUMNS(TRIMRANGE(1:1))");

    m.evaluate();

    assert_eq!(m._get_text("G1"), "10");
    assert_eq!(m._get_text("G2"), "0");
    assert_eq!(m._get_text("G3"), "20");
    assert_eq!(m._get_text("G4"), "");

    assert_eq!(m._get_text("G10"), "1");
    assert_eq!(m._get_text("I12"), "2");
    assert_eq!(m._get_text("J12"), "");

    assert_eq!(m._get_text("G20"), "7");
    assert_eq!(m._get_text("I21"), "8");

    assert_eq!(m._get_text("G30"), "55");
    assert_eq!(m._get_text("H30"), "0");
    assert_eq!(m._get_text("I30"), "66");
    assert_eq!(m._get_text("J30"), "");

    assert_eq!(m._get_text("G40"), "2");
    assert_eq!(m._get_text("G41"), "3");
    assert_eq!(m._get_text("G42"), "16379");

    // Full 16,384 x 1,048,576 sheet clipped via streaming reference inside SUM
    m._set("Sheet2!XFD1048576", "5");
    m._set("G43", "=SUM(TRIMRANGE(Sheet2!A1:XFD1048576))");
    m._set("H500000", "30");
    m._set("K500002", "40");
    m._set("G44", "=TRIMRANGE(500000:500002)");
    m.evaluate();
    assert_eq!(m._get_text("G43"), "20");
    assert_eq!(m._get_text("G44"), "10");
    assert_eq!(m._get_text("Q46"), "40");
}

#[test]
fn trimrange_modes_and_fail_closed_budgets_on_full_columns() {
    let mut m = new_empty_model();
    m._set("A1048575", "11");
    m._set("B1", "22");
    m._set("B2", "33");
    // Bare "=" evaluates to Empty; must be trimmed off edges of C:C
    m._set("C1", "=");
    m._set("C5", "99");
    m._set("C10", "=");
    m._set("D1", "1");
    m._set("D900000", "2");

    // trim_rows = 1 (leading only) trims 1,048,574 leading blank rows above A1048575 -> 2 rows
    m._set("G1", "=TRIMRANGE(A:A, 1, 0)");
    // trim_rows = 2 (trailing only) trims 1,048,574 trailing blank rows below B2 -> 2 rows
    m._set("G10", "=TRIMRANGE(B:B, 2, 0)");
    // Bare "=" edge cells in C:C must not bloat trimmed height
    m._set("G20", "=TRIMRANGE(C:C)");
    // Omitted mode arg defaults to 3; empty cell ref coerces to 0 (no trim on cols)
    m._set("G25", "=TRIMRANGE(B:B, , Z10)");
    // All-blank column/row/sheet returns #REF! in O(log N)
    m._set("G30", "=TRIMRANGE(E:E)");
    m._set("G31", "=TRIMRANGE(500:500)");
    m._set("G32", "=TRIMRANGE(A100:C200, 1, 0)");
    // Invalid trim mode rejects immediately with #VALUE!
    m._set("G33", "=TRIMRANGE(A:A, 4)");
    m._set("G34", "=TRIMRANGE(A:A, 0, -1)");
    // Unclipped oversized array (A:C = 3,145,728 > MAX_ARRAY_CELLS) stays #SPILL!
    m._set("G35", "=TRIMRANGE(A:C, 0, 0)");
    // Actual used span A1:D900000 (4 * 900,000 = 3,600,000 > MAX_ARRAY_CELLS) stays #SPILL!
    m._set("G36", "=TRIMRANGE(A:D)");
    // Error in edge cell is NOT blank and must propagate
    m._set("F100", "=1/0");
    m._set("G37", "=TRIMRANGE(F:F)");

    m.evaluate();

    assert_eq!(m._get_text("G1"), "11");
    assert_eq!(m._get_text("G2"), "0");
    assert_eq!(m._get_text("G3"), "");

    assert_eq!(m._get_text("G10"), "22");
    assert_eq!(m._get_text("G11"), "33");
    assert_eq!(m._get_text("G12"), "");

    assert_eq!(m._get_text("G20"), "99");
    assert_eq!(m._get_text("G21"), "");

    assert_eq!(m._get_text("G25"), "22");
    assert_eq!(m._get_text("G26"), "33");

    assert_eq!(m._get_text("G30"), "#REF!");
    assert_eq!(m._get_text("G31"), "#REF!");
    assert_eq!(m._get_text("G32"), "#REF!");
    assert_eq!(m._get_text("G33"), "#VALUE!");
    assert_eq!(m._get_text("G34"), "#VALUE!");
    assert_eq!(
        m.value("G35"),
        EvaluatedCellValue::Error(FormulaError::Spill)
    );
    assert_eq!(
        m.value("G36"),
        EvaluatedCellValue::Error(FormulaError::Spill)
    );
    assert_eq!(m._get_text("G37"), "#DIV/0!");
}

#[test]
fn trimrange_propagates_trimmed_references_and_composes_with_dynamic_operators() {
    let mut m = new_empty_model();
    m._set("A1", "=");
    m._set("A5", "10");
    m._set("A6", "20");
    m._set("B4", "3");
    m._set("B5", "4");
    m._set("C20", "=555");
    m._set("D1", "100");
    m.new_defined_name(DefinedNameEntry::new("FullA", "=Sheet1!$A:$A"))
        .unwrap();

    // Reference introspection and streaming functions
    m._set("F1", "=ISREF(TRIMRANGE(A:A))");
    m._set("F2", "=ISREF(TRIMRANGE(E:E))");
    m._set("F3", "=ISREF(TRIMRANGE(TRANSPOSE(A:A)))");
    m._set("F4", "=ISREF(TRIMRANGE({0,1,0}))");
    m._set("F5", "=ROW(TRIMRANGE(A:A))");
    m._set("F6", "=ROWS(TRIMRANGE(A:A))");
    m._set("F7", "=CELL(\"address\", TRIMRANGE(B:B))");
    m._set("F8", "=FORMULATEXT(TRIMRANGE(C:C))");
    m._set("F9", "=SUM(A1:TRIMRANGE(B:B))");
    m._set("F10", "=SUM(OFFSET(TRIMRANGE(A:A), 1, 0, 1, 1))");
    m._set("F11", "=TRIMRANGE(FullA)");
    m._set("F14", "=LET(c, A:A, TRIMRANGE(c))");
    m._set("F17", "=LET(c, A:A, SUM(TRIMRANGE(c)))");
    m._set("F18", "=LAMBDA(c, SUM(TRIMRANGE(c)))(A:A)");
    m._set("F19", "=TRIMRANGE(INDIRECT(\"B:B\"))");
    m._set("F22", "=TRIMRANGE(TRANSPOSE(B:B))");
    // `@TRIMRANGE(A:A)` on row 5 hits A5=10, row 10 is out of trimmed A5:A6 -> #VALUE!
    m._set("H5", "=@TRIMRANGE(A:A)");
    m._set("H10", "=@TRIMRANGE(A:A)");

    m.evaluate();

    assert_eq!(m._get_text("F1"), "TRUE");
    assert_eq!(m._get_text("F2"), "FALSE");
    assert_eq!(m._get_text("F3"), "FALSE");
    assert_eq!(m._get_text("F4"), "FALSE");
    assert_eq!(m._get_text("F5"), "5");
    assert_eq!(m._get_text("F6"), "2");
    assert_eq!(m._get_text("F7"), "$B$4");
    assert_eq!(m._get_text("F8"), "=555");
    // A1:TRIMRANGE(B:B) -> A1:(B4:B5) = A1:B5 -> A5(10)+B4(3)+B5(4) = 17
    assert_eq!(m._get_text("F9"), "17");
    assert_eq!(m._get_text("F10"), "20");
    assert_eq!(m._get_text("F11"), "10");
    assert_eq!(m._get_text("F12"), "20");
    assert_eq!(m._get_text("F14"), "10");
    assert_eq!(m._get_text("F15"), "20");
    assert_eq!(m._get_text("F17"), "30");
    assert_eq!(m._get_text("F18"), "30");
    assert_eq!(m._get_text("F19"), "3");
    assert_eq!(m._get_text("F20"), "4");
    assert_eq!(m._get_text("F22"), "3");
    assert_eq!(m._get_text("G22"), "4");
    assert_eq!(m._get_text("H5"), "10");
    assert_eq!(m._get_text("H10"), "#VALUE!");

    // Streaming SUM over 300,000 populated rows succeeds via trimmed reference without hitting array cell cap
    m._set("L1", "=SUM(TRIMRANGE(M:M))");
    for r in 1..=300_000 {
        m._set(&format!("M{r}"), "1");
    }
    m.evaluate();
    assert_eq!(m._get_text("L1"), "300000");
}

#[test]
fn trimrange_tracks_spills_recalc_session_updates_and_cell_overlay() {
    let mut m = new_empty_model();
    m._set("A2", "=SEQUENCE(3)");
    m._set("C1", "=TRIMRANGE(A:A)");
    m._set("D1", "=SUM(TRIMRANGE(INDIRECT(\"B:B\")))");
    m.evaluate();

    assert_eq!(m._get_text("C1"), "1");
    assert_eq!(m._get_text("C2"), "2");
    assert_eq!(m._get_text("C3"), "3");
    assert_eq!(m._get_text("D1"), "#REF!");

    // Populate B500 later: RecalcSession must invalidate INDIRECT("B:B") consumer in D1
    m._set("B500", "42");
    // Expand A2 spill: C1 must reflect all 4 spilled elements
    m._set("A2", "=SEQUENCE(4)");
    m.evaluate();
    assert_eq!(m._get_text("C4"), "4");
    assert_eq!(m._get_text("D1"), "42");

    // Verify zero-mutation cell_overlay in data validation sees pending candidate cell inside TRIMRANGE(A:A)
    let mut overlay_m = new_empty_model();
    overlay_m._set("A5", "6");
    overlay_m.evaluate();
    let mut dv_sum = DataValidation::new(parse_sqref("A1:A1"), DvKind::Custom);
    dv_sum.cmp = Cmp::Equal;
    dv_sum.a = "=SUM(TRIMRANGE(A:A))=106".to_string();
    overlay_m
        .session()
        .set_rules(
            "Sheet1",
            SheetRules {
                dv: vec![dv_sum],
                ..SheetRules::default()
            },
        )
        .unwrap();
    let verdict_ok = overlay_m
        .session()
        .validate(
            "Sheet1",
            PackedCellCoordinate::parse_a1("A1").unwrap(),
            "100",
        )
        .unwrap();
    assert!(matches!(verdict_ok, Verdict::Valid));

    let mut dv_arr = DataValidation::new(parse_sqref("A1:A1"), DvKind::Custom);
    dv_arr.cmp = Cmp::Equal;
    dv_arr.a = "=INDEX(TRIMRANGE(A:A),1,1)=100".to_string();
    overlay_m
        .session()
        .set_rules(
            "Sheet1",
            SheetRules {
                dv: vec![dv_arr],
                ..SheetRules::default()
            },
        )
        .unwrap();
    let verdict_arr = overlay_m
        .session()
        .validate(
            "Sheet1",
            PackedCellCoordinate::parse_a1("A1").unwrap(),
            "100",
        )
        .unwrap();
    assert!(matches!(verdict_arr, Verdict::Valid));
}
