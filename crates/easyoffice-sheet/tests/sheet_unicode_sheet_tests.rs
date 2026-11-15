use easyoffice_sheet::{compute_sheet_id, EvaluatedCellValue, WorkbookEngine};

#[test]
fn test_unicode_case_folding_sheet_identity_and_cross_sheet_references() {
    assert_eq!(compute_sheet_id("Übersicht"), compute_sheet_id("übersicht"));
    assert_eq!(compute_sheet_id("Αθήνα"), compute_sheet_id("αθήνα"));
    assert_eq!(compute_sheet_id("매출"), compute_sheet_id("매출"));

    assert!(WorkbookEngine::is_same_sheet_identity(
        "Übersicht",
        "übersicht"
    ));
    assert!(WorkbookEngine::is_same_sheet_identity(
        "übersicht",
        "Übersicht"
    ));
    assert!(WorkbookEngine::is_same_sheet_identity("Αθήνα", "αθήνα"));
    assert!(WorkbookEngine::is_same_sheet_identity("매출", "매출"));
    assert!(!WorkbookEngine::is_same_sheet_identity("매출", "매입"));

    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Übersicht");
    wb.set_cell("Übersicht", "A1", "42").unwrap();

    let existing_sheet = wb.try_add_sheet("übersicht").unwrap();
    assert_eq!(existing_sheet.name, "Übersicht");
    assert_eq!(wb.sheet_count(), 1);

    wb.add_sheet("Report");
    wb.set_cell("Report", "A1", "=übersicht!A1 + 8").unwrap();
    wb.set_cell("Report", "A2", "='Übersicht'!A1 * 2").unwrap();

    assert_eq!(
        wb.evaluate_cell("Report", "A1"),
        EvaluatedCellValue::Number(50.0)
    );
    assert_eq!(
        wb.evaluate_cell("Report", "A2"),
        EvaluatedCellValue::Number(84.0)
    );

    wb.add_sheet("Αθήνα");
    wb.set_cell("Αθήνα", "B5", "100").unwrap();
    wb.set_cell("Report", "B1", "=αθήνα!B5 + 23").unwrap();
    assert_eq!(
        wb.evaluate_cell("Report", "B1"),
        EvaluatedCellValue::Number(123.0)
    );

    let dup_greek = wb.try_add_sheet("αθήνα").unwrap();
    assert_eq!(dup_greek.name, "Αθήνα");
    assert_eq!(wb.sheet_count(), 3);
}
