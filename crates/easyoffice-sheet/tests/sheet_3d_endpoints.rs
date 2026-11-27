//! A 3D span follows the sheets that are inserted, moved and deleted the way the reference
//! spreadsheet app does (its help page on 3D references across worksheets):
//! - inserting or copying a sheet between the ends adds its cells to the calculation;
//! - deleting a sheet between the ends removes its values;
//! - deleting an end moves that end to the neighbor inside the span, and only a span with no sheet
//!   left becomes `#REF!`.
#![allow(clippy::unwrap_used)]

use easyoffice_sheet::{
    axis::{rewrite_sheet_names, SheetChange},
    AxisBudget, EvaluatedCellValue, WorkbookEngine,
};

const TABS: [&str; 6] = ["Sheet1", "Sheet2", "Sheet3", "Sheet4", "Sheet5", "Sheet6"];

fn delete(formula: &str, name: &str, tab_order: &[&str]) -> Option<String> {
    rewrite_sheet_names(formula, SheetChange::Delete { name, tab_order })
}

/// A workbook with `tabs` in this order; sheet `SheetN` holds `N` in `A1`, `Calc` sums the span.
fn workbook(tabs: &[&str], formula: &str) -> WorkbookEngine {
    let mut wb = WorkbookEngine::new();
    for name in tabs {
        let digits = name.trim_start_matches("Sheet");
        wb.add_sheet(*name).set_cell("A1", digits).unwrap();
    }
    wb.add_sheet("Calc").set_cell("A1", formula).unwrap();
    wb
}

fn sum_of(tabs: &[&str], formula: &str) -> f64 {
    match workbook(tabs, formula).evaluate_cell("Calc", "A1") {
        EvaluatedCellValue::Number(n) => n,
        other => panic!("expected a number, got {other:?}"),
    }
}

#[test]
fn deleting_the_first_end_moves_it_to_the_next_sheet() {
    assert_eq!(
        delete("=SUM(Sheet2:Sheet6!A2:A5)", "Sheet2", &TABS).as_deref(),
        Some("=SUM(Sheet3:Sheet6!A2:A5)")
    );
}

#[test]
fn deleting_the_last_end_moves_it_to_the_previous_sheet() {
    assert_eq!(
        delete("=SUM(Sheet2:Sheet6!A2:A5)", "Sheet6", &TABS).as_deref(),
        Some("=SUM(Sheet2:Sheet5!A2:A5)")
    );
}

#[test]
fn a_span_that_shrinks_to_one_sheet_becomes_a_plain_reference() {
    assert_eq!(
        delete("=SUM(Sheet2:Sheet3!A1)", "Sheet2", &TABS).as_deref(),
        Some("=SUM(Sheet3!A1)")
    );
    assert_eq!(
        delete(
            "=SUM('Sheet 2':'Sheet 3'!A1:B2)",
            "Sheet 3",
            &["Sheet 2", "Sheet 3"]
        )
        .as_deref(),
        Some("=SUM('Sheet 2'!A1:B2)")
    );
}

#[test]
fn a_span_with_no_sheet_left_becomes_ref() {
    assert_eq!(
        delete("=SUM(Sheet2:Sheet2!A1)", "Sheet2", &TABS).as_deref(),
        Some("=SUM(#REF!)")
    );
    // A single sheet reference is unchanged in meaning: it is gone.
    assert_eq!(
        delete("=Sheet2!A1+1", "Sheet2", &TABS).as_deref(),
        Some("=#REF!+1")
    );
}

#[test]
fn deleting_an_interior_or_unrelated_sheet_leaves_the_span_alone() {
    assert_eq!(delete("=SUM(Sheet2:Sheet6!A1)", "Sheet4", &TABS), None);
    assert_eq!(delete("=SUM(Sheet2:Sheet6!A1)", "Sheet1", &TABS), None);
}

#[test]
fn the_span_is_followed_in_tab_order_not_in_name_order() {
    // The tab order is Sheet1 Sheet3 Sheet2: the sheet after Sheet1 is Sheet3.
    let tabs = ["Sheet1", "Sheet3", "Sheet2", "Sheet4"];
    assert_eq!(
        delete("=SUM(Sheet1:Sheet4!A1)", "Sheet1", &tabs).as_deref(),
        Some("=SUM(Sheet3:Sheet4!A1)")
    );
    assert_eq!(
        delete("=SUM(Sheet1:Sheet4!A1)", "Sheet4", &tabs).as_deref(),
        Some("=SUM(Sheet1:Sheet2!A1)")
    );
}

#[test]
fn a_span_written_in_reverse_order_moves_the_end_on_its_own_side() {
    assert_eq!(
        delete("=SUM(Sheet6:Sheet2!A1)", "Sheet6", &TABS).as_deref(),
        Some("=SUM(Sheet5:Sheet2!A1)")
    );
    assert_eq!(
        delete("=SUM(Sheet6:Sheet2!A1)", "Sheet2", &TABS).as_deref(),
        Some("=SUM(Sheet6:Sheet3!A1)")
    );
}

#[test]
fn the_deleted_sheet_must_be_in_the_tab_order_or_the_span_fails_closed() {
    assert_eq!(
        delete("=SUM(Sheet2:Sheet6!A1)", "Sheet2", &[]).as_deref(),
        Some("=SUM(#REF!)")
    );
    assert_eq!(
        delete("=SUM(Sheet2:Sheet6!A1)", "Sheet2", &["Sheet2"]).as_deref(),
        Some("=SUM(#REF!)")
    );
}

#[test]
fn other_references_in_the_formula_keep_their_text() {
    assert_eq!(
        delete(
            "=SUM(Sheet2:Sheet6!A1:B2)+Sheet3!C1+A1+SUM(Sheet1:Sheet2!A1)",
            "Sheet2",
            &TABS
        )
        .as_deref(),
        Some("=SUM(Sheet3:Sheet6!A1:B2)+Sheet3!C1+A1+SUM(Sheet1!A1)")
    );
}

#[test]
fn the_workbook_plan_uses_its_own_tab_order() {
    let mut wb = workbook(&TABS, "=SUM(Sheet2:Sheet6!A1)");
    wb.set_cell("Calc", "B1", "=SUM(Sheet1:Sheet2!A1)").unwrap();
    // The change names no tab order: the workbook knows its own.
    let plan = wb
        .plan_sheet_rewrites(
            SheetChange::Delete {
                name: "Sheet2",
                tab_order: &[],
            },
            AxisBudget::default(),
        )
        .unwrap();
    let mut texts: Vec<(String, String)> = plan
        .iter()
        .map(|(_, coord, text)| (coord.to_a1(), text.clone()))
        .collect();
    texts.sort();
    assert_eq!(
        texts,
        vec![
            ("A1".to_string(), "=SUM(Sheet3:Sheet6!A1)".to_string()),
            ("B1".to_string(), "=SUM(Sheet1!A1)".to_string()),
        ]
    );
}

#[test]
fn inserting_between_the_ends_adds_the_sheet_and_outside_does_not() {
    let formula = "=SUM(Sheet2:Sheet4!A1)";
    assert_eq!(
        sum_of(&["Sheet1", "Sheet2", "Sheet3", "Sheet4"], formula),
        9.0
    );
    // A sheet inserted between Sheet2 and Sheet4 counts.
    assert_eq!(
        sum_of(&["Sheet1", "Sheet2", "Sheet6", "Sheet3", "Sheet4"], formula),
        15.0
    );
    // A sheet inserted before the first end or after the last end does not.
    assert_eq!(
        sum_of(
            &["Sheet6", "Sheet1", "Sheet2", "Sheet3", "Sheet4", "Sheet5"],
            formula
        ),
        9.0
    );
}

#[test]
fn moving_a_sheet_out_of_the_span_removes_it_and_moving_an_end_changes_the_range() {
    let formula = "=SUM(Sheet2:Sheet4!A1)";
    // Sheet3 moved out of the span, before its first end or after its last end.
    assert_eq!(
        sum_of(&["Sheet1", "Sheet3", "Sheet2", "Sheet4"], formula),
        6.0
    );
    assert_eq!(
        sum_of(&["Sheet1", "Sheet2", "Sheet4", "Sheet3"], formula),
        6.0
    );
    // Sheet5 moved between the ends.
    assert_eq!(
        sum_of(&["Sheet1", "Sheet2", "Sheet5", "Sheet3", "Sheet4"], formula),
        14.0
    );
}
