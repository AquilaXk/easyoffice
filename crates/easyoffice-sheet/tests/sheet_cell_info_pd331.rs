//! Integration tests for `CELL` and `INFO` metadata evaluation, `CELL("width")` 1x2 array spill,
//! format descriptors, protection/alignment prefixes, cross-sheet addressing, `RecalcSession`
//! updates, and axis-edit shifting (`PD-331`).

use easyoffice_formats::{
    CellStyle, CellValueKind, DefaultColWidth, DefaultFont, HAlign, SheetStyling, CUSTOM_BASE,
};
use easyoffice_sheet::axis::{Axis, AxisBudget, AxisEdit, EditKind};
use easyoffice_sheet::{
    EvaluatedCellValue, PackedCellCoordinate, RecalcSession, SheetCellMeta, WorkbookEngine,
    WorkbookMeta,
};

fn a1(s: &str) -> PackedCellCoordinate {
    PackedCellCoordinate::parse_a1(s).expect("valid A1")
}

fn eval_text(wb: &WorkbookEngine, sheet: &str, cell: &str) -> String {
    wb.evaluate_cell(sheet, cell).to_display_string()
}

#[test]
fn cell_format_color_parentheses_cover_standard_and_custom_codes() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");

    let mut st = SheetStyling::default();
    let custom = [
        "0",                         // 164 -> F0
        "0.00",                      // 165 -> F2
        "#,##0",                     // 166 -> ,0
        "#,##0.00",                  // 167 -> ,2
        "$#,##0.00",                 // 168 -> C2
        "₩#,##0",                    // 169 -> C0
        "0%",                        // 170 -> P0
        "0.00%",                     // 171 -> P2
        "0.00E+00",                  // 172 -> S2
        "d-mmm-yy",                  // 173 -> D1
        "d-mmm",                     // 174 -> D2
        "mmm-yy",                    // 175 -> D3
        "yyyy-mm-dd",                // 176 -> D4
        "mm/dd",                     // 177 -> D5
        "h:mm:ss AM/PM",             // 178 -> D6
        "h:mm AM/PM",                // 179 -> D7
        "h:mm:ss",                   // 180 -> D8
        "h:mm",                      // 181 -> D9
        "#,##0;[Red]-#,##0",         // 182 -> ,0-
        "($#,##0.00)",               // 183 -> C2()
        "0.00_);[Red](0.00)",        // 184 -> F2- (accounting _( must NOT trigger parens!)
        "[Blue](#,##0);[Red]-#,##0", // 185 -> ,0-()
        "@",                         // 186 -> G
        "0,",                        // 187 -> F0 (trailing scaling comma != grouping)
        "[<0][Red]0;[>=0]0",         // 188 -> F0-
        "[Red]0",                    // 189 -> F0 (section 0 color is not negative-only)
    ];
    st.num_fmts = std::sync::Arc::new(custom.iter().map(|s| (*s).to_string()).collect());

    for idx in 0..custom.len() {
        st.cells.insert(
            (idx as u32, 0),
            CellStyle {
                num_fmt: CUSTOM_BASE + idx as u16,
                ..CellStyle::default()
            },
        );
    }
    // Row 26 (A27): built-in id 14 (mm-dd-yy -> D4), Row 27 (A28): plain General (G)
    st.cells.insert(
        (26, 0),
        CellStyle {
            num_fmt: 14,
            ..CellStyle::default()
        },
    );

    wb.set_sheet_cell_meta("Sheet1", Some(SheetCellMeta::from_styling(&st)))
        .unwrap();

    for i in 1..=28usize {
        wb.set_cell(
            "Sheet1",
            &format!("B{i}"),
            format!("=CELL(\"format\", A{i})"),
        )
        .unwrap();
        wb.set_cell(
            "Sheet1",
            &format!("C{i}"),
            format!("=CELL(\"color\", A{i})"),
        )
        .unwrap();
        wb.set_cell(
            "Sheet1",
            &format!("D{i}"),
            format!("=CELL(\"parentheses\", A{i})"),
        )
        .unwrap();
    }

    let expected_fmt = [
        "F0", "F2", ",0", ",2", "C2", "C0", "P0", "P2", "S2", "D1", "D2", "D3", "D4", "D5", "D6",
        "D7", "D8", "D9", ",0-", "C2()", "F2-", ",0-()", "G", "F0", "F0-", "F0", "D4", "G",
    ];
    for (idx, exp) in expected_fmt.iter().enumerate() {
        let row = idx + 1;
        assert_eq!(
            eval_text(&wb, "Sheet1", &format!("B{row}")),
            *exp,
            "row {row}"
        );
    }

    assert_eq!(eval_text(&wb, "Sheet1", "C19"), "1");
    assert_eq!(eval_text(&wb, "Sheet1", "C21"), "1");
    assert_eq!(eval_text(&wb, "Sheet1", "C22"), "1");
    assert_eq!(eval_text(&wb, "Sheet1", "C25"), "1");
    assert_eq!(eval_text(&wb, "Sheet1", "C26"), "0");
    assert_eq!(eval_text(&wb, "Sheet1", "C20"), "0");

    assert_eq!(eval_text(&wb, "Sheet1", "D20"), "1");
    assert_eq!(eval_text(&wb, "Sheet1", "D21"), "0");
    assert_eq!(eval_text(&wb, "Sheet1", "D22"), "1");
    assert_eq!(eval_text(&wb, "Sheet1", "D28"), "0");
}

#[test]
fn cell_width_spills_1x2_handles_custom_hidden_fonts_and_scalar_implicit_intersection() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");

    // Default Malgun Gothic 11pt (digit_px=8, default_col_px=72) -> (72 - 5) / 8 = 8.375 -> 8
    let mut st = SheetStyling {
        columns: [(1, 165), (2, 100)].into_iter().collect(), // col B: (165-5)/8 = 20, col C: hidden -> 0
        hidden_cols: vec![(2, 2)],
        default_font: DefaultFont::new("Malgun Gothic", 11.0),
        ..SheetStyling::default()
    };
    st.default_col_width = Some(DefaultColWidth {
        width: None,
        base: None,
    });
    wb.set_sheet_cell_meta("Sheet1", Some(SheetCellMeta::from_styling(&st)))
        .unwrap();

    // Verify 1x2 spill via RecalcSession
    let mut sess = RecalcSession::new(wb).unwrap();
    sess.set_cell("Sheet1", a1("A1"), "=CELL(\"width\", A1)")
        .unwrap();
    sess.set_cell("Sheet1", a1("A2"), "=CELL(\"width\", B1)")
        .unwrap();
    sess.set_cell("Sheet1", a1("A3"), "=CELL(\"width\", C1)")
        .unwrap();
    sess.set_cell("Sheet1", a1("D1"), "=@CELL(\"width\", B1)")
        .unwrap();
    // Block spill neighbor for A4 -> #SPILL!
    sess.set_cell("Sheet1", a1("B4"), "occupied").unwrap();
    sess.set_cell("Sheet1", a1("A4"), "=CELL(\"width\", A1)")
        .unwrap();
    // Dynamic first arg -> picks width vs non-width dynamically
    sess.set_cell("Sheet1", a1("D2"), "width").unwrap();
    sess.set_cell("Sheet1", a1("A5"), "=CELL(D2, B1)").unwrap();
    sess.set_cell("Sheet1", a1("D3"), "col").unwrap();
    sess.set_cell("Sheet1", a1("A6"), "=CELL(D3, B1)").unwrap();
    // Self-referential omitted reference in E1 (=CELL("width")) -> col E (hidden? no, default) -> {8, TRUE}
    sess.set_cell("Sheet1", a1("E1"), "=CELL(\"width\")")
        .unwrap();
    sess.recalculate();

    assert_eq!(
        sess.cell_value("Sheet1", a1("A1")),
        EvaluatedCellValue::Number(8.0)
    );
    assert_eq!(
        sess.cell_value("Sheet1", a1("B1")),
        EvaluatedCellValue::Boolean(true)
    );
    assert_eq!(
        sess.cell_value("Sheet1", a1("A2")),
        EvaluatedCellValue::Number(20.0)
    );
    assert_eq!(
        sess.cell_value("Sheet1", a1("B2")),
        EvaluatedCellValue::Boolean(false)
    );
    assert_eq!(
        sess.cell_value("Sheet1", a1("A3")),
        EvaluatedCellValue::Number(0.0)
    );
    assert_eq!(
        sess.cell_value("Sheet1", a1("B3")),
        EvaluatedCellValue::Boolean(false)
    );
    assert_eq!(
        sess.cell_value("Sheet1", a1("D1")),
        EvaluatedCellValue::Number(20.0)
    );
    assert_eq!(
        sess.cell_value("Sheet1", a1("A4")).to_display_string(),
        "#SPILL!"
    );
    assert_eq!(
        sess.cell_value("Sheet1", a1("A5")),
        EvaluatedCellValue::Number(20.0)
    );
    assert_eq!(
        sess.cell_value("Sheet1", a1("B5")),
        EvaluatedCellValue::Boolean(false)
    );
    assert_eq!(
        sess.cell_value("Sheet1", a1("A6")),
        EvaluatedCellValue::Number(2.0)
    );
    assert_eq!(
        sess.cell_value("Sheet1", a1("E1")),
        EvaluatedCellValue::Number(8.0)
    );
    assert_eq!(
        sess.cell_value("Sheet1", a1("F1")),
        EvaluatedCellValue::Boolean(true)
    );

    // Calibri 11pt (digit_px = 7, default 64px) -> (64 - 5)/7 = 8.43 -> 8
    let calibri_st = SheetStyling {
        default_font: DefaultFont::new("Calibri", 11.0),
        ..SheetStyling::default()
    };
    sess.set_sheet_cell_meta("Sheet1", Some(SheetCellMeta::from_styling(&calibri_st)))
        .unwrap();
    sess.recalculate();
    assert_eq!(
        sess.cell_value("Sheet1", a1("A1")),
        EvaluatedCellValue::Number(8.0)
    );
    assert_eq!(
        sess.cell_value("Sheet1", a1("B1")),
        EvaluatedCellValue::Boolean(true)
    );
}

#[test]
fn cell_prefix_and_protect_reflect_value_kind_alignment_and_lock_state() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");

    // Note: protect is None (sheet protection OFF), yet CELL("protect") reads cell lock bit
    let mut st = SheetStyling {
        unlocked_rows: vec![(3, 3)], // row 4 unlocked (0-based 3)
        unlocked_cols: vec![(2, 2)], // col C unlocked (0-based 2)
        ..SheetStyling::default()
    };
    st.cells.insert(
        (0, 0),
        CellStyle {
            h_align: HAlign::Center,
            unlocked: true,
            ..CellStyle::default()
        },
    ); // A1: center text, unlocked
    st.cells.insert(
        (0, 1),
        CellStyle {
            h_align: HAlign::Right,
            ..CellStyle::default()
        },
    ); // B1: right text, locked (default)
    st.cells.insert(
        (3, 1),
        CellStyle {
            unlocked: false,
            force_locked: true,
            ..CellStyle::default()
        },
    ); // B4: force-locked inside unlocked row 4

    wb.set_sheet_cell_meta("Sheet1", Some(SheetCellMeta::from_styling(&st)))
        .unwrap();

    wb.set_cell("Sheet1", "A1", "hello").unwrap();
    wb.set_cell("Sheet1", "B1", "world").unwrap();
    wb.set_cell("Sheet1", "C1", "=1+1").unwrap(); // numeric formula, left/right number -> prefix ""
    wb.set_cell("Sheet1", "D1", "=\"hi\"").unwrap(); // text formula -> prefix "'"
    wb.set_cell("Sheet1", "E1", "TRUE").unwrap(); // boolean -> prefix ""
    wb.set_cell("Sheet1", "F1", "").unwrap(); // empty -> prefix ""
    wb.sheet_mut("Sheet1")
        .unwrap()
        .set_cell_typed("A4", "00123", Some(CellValueKind::Text))
        .unwrap(); // explicit text cell on unlocked row

    wb.set_cell("Sheet1", "G1", "=CELL(\"prefix\", A1)")
        .unwrap();
    wb.set_cell("Sheet1", "G2", "=CELL(\"prefix\", B1)")
        .unwrap();
    wb.set_cell("Sheet1", "G3", "=CELL(\"prefix\", C1)")
        .unwrap();
    wb.set_cell("Sheet1", "G4", "=CELL(\"prefix\", D1)")
        .unwrap();
    wb.set_cell("Sheet1", "G5", "=CELL(\"prefix\", E1)")
        .unwrap();
    wb.set_cell("Sheet1", "G6", "=CELL(\"prefix\", F1)")
        .unwrap();
    wb.set_cell("Sheet1", "G7", "=CELL(\"prefix\", A4)")
        .unwrap();

    wb.set_cell("Sheet1", "H1", "=CELL(\"protect\", A1)")
        .unwrap(); // unlocked cell -> 0
    wb.set_cell("Sheet1", "H2", "=CELL(\"protect\", B1)")
        .unwrap(); // locked cell -> 1
    wb.set_cell("Sheet1", "H3", "=CELL(\"protect\", C1)")
        .unwrap(); // unlocked col -> 0
    wb.set_cell("Sheet1", "H4", "=CELL(\"protect\", A4)")
        .unwrap(); // unlocked row -> 0
    wb.set_cell("Sheet1", "H5", "=CELL(\"protect\", B4)")
        .unwrap(); // force_locked in row 4 -> 1

    assert_eq!(eval_text(&wb, "Sheet1", "G1"), "^");
    assert_eq!(eval_text(&wb, "Sheet1", "G2"), "\"");
    assert_eq!(eval_text(&wb, "Sheet1", "G3"), "");
    assert_eq!(eval_text(&wb, "Sheet1", "G4"), "'");
    assert_eq!(eval_text(&wb, "Sheet1", "G5"), "");
    assert_eq!(eval_text(&wb, "Sheet1", "G6"), "");
    assert_eq!(eval_text(&wb, "Sheet1", "G7"), "'");

    assert_eq!(eval_text(&wb, "Sheet1", "H1"), "0");
    assert_eq!(eval_text(&wb, "Sheet1", "H2"), "1");
    assert_eq!(eval_text(&wb, "Sheet1", "H3"), "0");
    assert_eq!(eval_text(&wb, "Sheet1", "H4"), "0");
    assert_eq!(eval_text(&wb, "Sheet1", "H5"), "1");
}

#[test]
fn cell_filename_cross_sheet_address_and_info_metadata() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    wb.add_sheet("Sheet2");
    wb.add_sheet("My Sheet");

    wb.set_cell("Sheet1", "A1", "=CELL(\"filename\")").unwrap();
    wb.set_cell("Sheet1", "A2", "=CELL(\"filename\", Sheet2!B3)")
        .unwrap();
    wb.set_cell("Sheet1", "A3", "=CELL(\"address\", Sheet2!B3)")
        .unwrap();
    wb.set_cell("Sheet1", "A4", "=CELL(\"address\", 'My Sheet'!C4)")
        .unwrap();
    wb.set_cell("Sheet1", "A5", "=CELL(\"address\", Sheet1!D5)")
        .unwrap();
    wb.set_cell("Sheet1", "A6", "=CELL(\"address\", NoSuchSheet!A1)")
        .unwrap();

    wb.set_cell("Sheet1", "B1", "=INFO(\"release\")").unwrap();
    wb.set_cell("Sheet1", "B2", "=INFO(\"osversion\")").unwrap();
    wb.set_cell("Sheet1", "B3", "=INFO(\"directory\")").unwrap();
    wb.set_cell("Sheet1", "B4", "=INFO(\"origin\")").unwrap();
    wb.set_cell("Sheet1", "B5", "=INFO(\"timezone\")").unwrap();

    // Unsaved workbook defaults
    assert_eq!(eval_text(&wb, "Sheet1", "A1"), "");
    assert_eq!(eval_text(&wb, "Sheet1", "A2"), "");
    assert_eq!(eval_text(&wb, "Sheet1", "A3"), "Sheet2!$B$3");
    assert_eq!(eval_text(&wb, "Sheet1", "A4"), "'My Sheet'!$C$4");
    assert_eq!(eval_text(&wb, "Sheet1", "A5"), "$D$5");
    assert_eq!(eval_text(&wb, "Sheet1", "A6"), "#REF!");
    assert_eq!(eval_text(&wb, "Sheet1", "B1"), "16.0");
    assert!(!eval_text(&wb, "Sheet1", "B2").is_empty());
    assert_eq!(eval_text(&wb, "Sheet1", "B3"), "");
    assert_eq!(eval_text(&wb, "Sheet1", "B4"), "$A:$A$1");
    assert_eq!(eval_text(&wb, "Sheet1", "B5"), "#N/A");

    // Configured workbook & sheet metadata (basename + explicit directory/origin/release/osversion)
    wb.set_meta(WorkbookMeta {
        file_name: "Book1.xlsx".to_string(),
        directory: Some("/docs/reports".to_string()),
        release: Some("16.5".to_string()),
        osversion: Some("macOS 15.0".to_string()),
    });
    let mut st = SheetStyling::default();
    st.view.top_left = (4, 2); // row 5, col C -> "$A:$C$5"
    wb.set_sheet_cell_meta("Sheet1", Some(SheetCellMeta::from_styling(&st)))
        .unwrap();

    assert_eq!(
        eval_text(&wb, "Sheet1", "A1"),
        "/docs/reports/[Book1.xlsx]Sheet1"
    );
    assert_eq!(
        eval_text(&wb, "Sheet1", "A2"),
        "/docs/reports/[Book1.xlsx]Sheet2"
    );
    assert_eq!(eval_text(&wb, "Sheet1", "A3"), "[Book1.xlsx]Sheet2!$B$3");
    assert_eq!(
        eval_text(&wb, "Sheet1", "A4"),
        "'[Book1.xlsx]My Sheet'!$C$4"
    );
    assert_eq!(eval_text(&wb, "Sheet1", "A5"), "$D$5");
    assert_eq!(eval_text(&wb, "Sheet1", "B1"), "16.5");
    assert_eq!(eval_text(&wb, "Sheet1", "B2"), "macOS 15.0");
    assert_eq!(eval_text(&wb, "Sheet1", "B3"), "/docs/reports/");
    assert_eq!(eval_text(&wb, "Sheet1", "B4"), "$A:$C$5");

    // Standalone evaluate_text without anchor requires ref for omitted CELL("address")
    let mut sess = RecalcSession::new(wb).unwrap();
    assert_eq!(
        sess.evaluate_text("Sheet1", "=CELL(\"address\")", None)
            .to_display_string(),
        "#VALUE!"
    );
}

#[test]
fn axis_edit_shifts_cell_meta_formats_alignments_widths_and_locks() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");

    let mut st = SheetStyling {
        columns: [(1, 165)].into_iter().collect(), // col B -> width 20
        ..SheetStyling::default()
    };
    st.num_fmts = std::sync::Arc::new(vec!["0.00".to_string()]);
    st.cells.insert(
        (0, 1),
        CellStyle {
            num_fmt: CUSTOM_BASE,
            h_align: HAlign::Center,
            unlocked: true,
            ..CellStyle::default()
        },
    ); // B1
    wb.set_sheet_cell_meta("Sheet1", Some(SheetCellMeta::from_styling(&st)))
        .unwrap();

    let mut sess = RecalcSession::new(wb).unwrap();
    sess.set_cell("Sheet1", a1("B1"), "abc").unwrap();
    sess.set_cell("Sheet1", a1("E1"), "=CELL(\"format\", B1)")
        .unwrap();
    sess.set_cell("Sheet1", a1("E2"), "=CELL(\"prefix\", B1)")
        .unwrap();
    sess.set_cell("Sheet1", a1("E3"), "=CELL(\"protect\", B1)")
        .unwrap();
    sess.set_cell("Sheet1", a1("E4"), "=@CELL(\"width\", B1)")
        .unwrap();
    sess.recalculate();

    assert_eq!(
        sess.cell_value("Sheet1", a1("E1")).to_display_string(),
        "F2"
    );
    assert_eq!(sess.cell_value("Sheet1", a1("E2")).to_display_string(), "^");
    assert_eq!(
        sess.cell_value("Sheet1", a1("E3")),
        EvaluatedCellValue::Number(0.0)
    );
    assert_eq!(
        sess.cell_value("Sheet1", a1("E4")),
        EvaluatedCellValue::Number(20.0)
    );

    // Insert 1 column before column B (at index 1): B1 moves to C1, formulas rewrite B1 -> C1, B becomes default width 8
    sess.apply_axis_edit(
        "Sheet1",
        AxisEdit::new(Axis::Col, EditKind::Insert, 1, 1).unwrap(),
        AxisBudget::default(),
    )
    .unwrap();
    sess.set_cell("Sheet1", a1("A1"), "=@CELL(\"width\", B1)")
        .unwrap();
    sess.recalculate();

    assert_eq!(
        sess.cell_value("Sheet1", a1("F1")).to_display_string(),
        "F2"
    );
    assert_eq!(sess.cell_value("Sheet1", a1("F2")).to_display_string(), "^");
    assert_eq!(
        sess.cell_value("Sheet1", a1("F3")),
        EvaluatedCellValue::Number(0.0)
    );
    assert_eq!(
        sess.cell_value("Sheet1", a1("F4")),
        EvaluatedCellValue::Number(20.0)
    );
    assert_eq!(
        sess.cell_value("Sheet1", a1("A1")),
        EvaluatedCellValue::Number(8.0)
    );

    // Live workbook meta update recalculates INFO/CELL cleanly
    sess.set_workbook_meta(WorkbookMeta {
        file_name: "Updated.xlsx".to_string(),
        ..WorkbookMeta::default()
    });
    sess.set_cell("Sheet1", a1("A2"), "=CELL(\"filename\")")
        .unwrap();
    sess.recalculate();
    assert_eq!(
        sess.cell_value("Sheet1", a1("A2")).to_display_string(),
        "[Updated.xlsx]Sheet1"
    );
}
