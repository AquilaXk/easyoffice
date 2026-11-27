//! Moving the end sheet of a 3D span follows the reference spreadsheet app (its help page on 3D
//! references across worksheets, section "Move an endpoint"):
//! - an end moved to another place that keeps the order of the two ends leaves the text alone, the
//!   span then holds the sheets that are between the ends now;
//! - an end moved past the other end is replaced by its neighbor inside the old span
//!   (`Sheet2:Sheet6` with Sheet2 after Sheet6 is `Sheet3:Sheet6`, with Sheet6 before Sheet2 it is
//!   `Sheet2:Sheet5`);
//! - a sheet between the ends moved out of the span leaves the calculation and one moved in joins
//!   it, which the positions of the ends already give.
#![allow(clippy::unwrap_used)]

use easyoffice_sheet::{
    axis::{rewrite_sheet_names, SheetChange},
    AxisBudget, EvaluatedCellValue, WorkbookEngine,
};

const TABS: [&str; 7] = [
    "Sheet1", "Sheet2", "Sheet3", "Sheet4", "Sheet5", "Sheet6", "Sheet7",
];

/// `names` with `moved` taken out and put in again so that it is at `to`.
fn reordered(names: &[&str], moved: &str, to: usize) -> Vec<String> {
    let mut order: Vec<String> = names
        .iter()
        .filter(|n| **n != moved)
        .map(ToString::to_string)
        .collect();
    order.insert(to, moved.to_string());
    order
}

/// `formula` after `moved` goes to tab position `to` in `before`.
fn move_to(formula: &str, before: &[&str], moved: &str, to: usize) -> Option<String> {
    let after = reordered(before, moved, to);
    rewrite_sheet_names(
        formula,
        SheetChange::Move {
            moved: &[moved.to_string()],
            tab_order: before,
            new_order: &after,
        },
    )
}

const SPAN: &str = "=SUM(Sheet2:Sheet6!A2:A5)";

#[test]
fn the_first_end_moved_after_the_last_end_becomes_its_next_sheet() {
    assert_eq!(
        move_to(SPAN, &TABS, "Sheet2", 5).as_deref(),
        Some("=SUM(Sheet3:Sheet6!A2:A5)")
    );
    // After the sheet that follows the last end too.
    assert_eq!(
        move_to(SPAN, &TABS, "Sheet2", 6).as_deref(),
        Some("=SUM(Sheet3:Sheet6!A2:A5)")
    );
}

#[test]
fn the_last_end_moved_before_the_first_end_becomes_its_previous_sheet() {
    assert_eq!(
        move_to(SPAN, &TABS, "Sheet6", 1).as_deref(),
        Some("=SUM(Sheet2:Sheet5!A2:A5)")
    );
    // Before the sheet that precedes the first end too.
    assert_eq!(
        move_to(SPAN, &TABS, "Sheet6", 0).as_deref(),
        Some("=SUM(Sheet2:Sheet5!A2:A5)")
    );
}

#[test]
fn an_end_moved_inside_the_span_or_away_on_its_own_side_keeps_its_text() {
    // Between Sheet4 and Sheet5; before Sheet1; after Sheet7 for the last end.
    assert_eq!(move_to(SPAN, &TABS, "Sheet2", 3), None);
    assert_eq!(move_to(SPAN, &TABS, "Sheet2", 0), None);
    assert_eq!(move_to(SPAN, &TABS, "Sheet6", 6), None);
    assert_eq!(move_to(SPAN, &TABS, "Sheet6", 2), None);
}

#[test]
fn a_sheet_between_the_ends_or_outside_them_never_changes_the_text() {
    for (sheet, to) in [("Sheet4", 0), ("Sheet4", 6), ("Sheet1", 3), ("Sheet7", 3)] {
        assert_eq!(move_to(SPAN, &TABS, sheet, to), None, "{sheet} to {to}");
    }
}

#[test]
fn adjacent_ends_collapse_to_the_one_that_stays() {
    let span = "=SUM(Sheet2:Sheet3!A1)";
    assert_eq!(
        move_to(span, &TABS, "Sheet2", 2).as_deref(),
        Some("=SUM(Sheet3!A1)")
    );
    assert_eq!(
        move_to(span, &TABS, "Sheet3", 1).as_deref(),
        Some("=SUM(Sheet2!A1)")
    );
}

#[test]
fn a_single_sheet_reference_is_never_rewritten_by_a_move() {
    assert_eq!(move_to("=Sheet2!A1+1", &TABS, "Sheet2", 5), None);
    assert_eq!(move_to("=SUM(Sheet2:Sheet2!A1)", &TABS, "Sheet2", 5), None);
    assert_eq!(move_to("=A1+B1", &TABS, "Sheet2", 5), None);
}

#[test]
fn a_span_written_in_reverse_order_replaces_the_end_on_its_own_side() {
    let span = "=SUM(Sheet6:Sheet2!A1)";
    assert_eq!(
        move_to(span, &TABS, "Sheet2", 5).as_deref(),
        Some("=SUM(Sheet6:Sheet3!A1)")
    );
    assert_eq!(
        move_to(span, &TABS, "Sheet6", 1).as_deref(),
        Some("=SUM(Sheet5:Sheet2!A1)")
    );
}

#[test]
fn the_sheets_are_followed_in_tab_order_and_names_ignore_case_and_quotes() {
    let tabs = ["Sheet1", "Sheet3", "Sheet2", "Sheet4"];
    // The sheet after Sheet1 in tab order is Sheet3.
    assert_eq!(
        move_to("=SUM(Sheet1:Sheet4!A1)", &tabs, "Sheet1", 3).as_deref(),
        Some("=SUM(Sheet3:Sheet4!A1)")
    );
    let quoted = ["1 월", "2 월", "3 월", "4 월"];
    assert_eq!(
        move_to("=SUM('2 월':'4 월'!A1)", &quoted, "2 월", 3).as_deref(),
        Some("=SUM('3 월:4 월'!A1)")
    );
    assert_eq!(
        move_to("=SUM(sheet2:SHEET6!A1)", &TABS, "Sheet2", 5).as_deref(),
        Some("=SUM(Sheet3:SHEET6!A1)")
    );
}

#[test]
fn a_group_moved_together_replaces_the_end_by_the_first_sheet_that_stays() {
    let group = |formula: &str, moved: &[&str], after: &[&str]| {
        let moved: Vec<String> = moved.iter().map(ToString::to_string).collect();
        let after: Vec<String> = after.iter().map(ToString::to_string).collect();
        rewrite_sheet_names(
            formula,
            SheetChange::Move {
                moved: &moved,
                tab_order: &TABS,
                new_order: &after,
            },
        )
    };
    // Sheet2 and Sheet3 go after Sheet6: the first end's neighbor Sheet3 left too, so Sheet4.
    let order = [
        "Sheet1", "Sheet4", "Sheet5", "Sheet6", "Sheet2", "Sheet3", "Sheet7",
    ];
    assert_eq!(
        group(SPAN, &["Sheet2", "Sheet3"], &order).as_deref(),
        Some("=SUM(Sheet4:Sheet6!A2:A5)")
    );
    // Both ends are in the group and keep their order: nothing to rewrite.
    let order = [
        "Sheet1", "Sheet7", "Sheet2", "Sheet3", "Sheet4", "Sheet5", "Sheet6",
    ];
    assert_eq!(
        group(
            SPAN,
            &["Sheet2", "Sheet3", "Sheet4", "Sheet5", "Sheet6"],
            &order
        ),
        None
    );
}

#[test]
fn a_tab_order_that_does_not_fit_the_move_closes_only_the_span_of_the_moved_end() {
    let formula = "=SUM(Sheet2:Sheet6!A1)+SUM(Sheet3:Sheet5!A1)+Sheet2!A1";
    let right = reordered(&TABS, "Sheet2", 5);
    let right: Vec<&str> = right.iter().map(String::as_str).collect();
    let moved = ["Sheet2".to_string()];
    let change = |before: &[&str], after: &[&str], moved: &[String]| {
        let after: Vec<String> = after.iter().map(ToString::to_string).collect();
        rewrite_sheet_names(
            formula,
            SheetChange::Move {
                moved,
                tab_order: before,
                new_order: &after,
            },
        )
    };
    let closed = Some("=SUM(#REF!)+SUM(Sheet3:Sheet5!A1)+Sheet2!A1".to_string());
    // The tab order lacks the other end, lacks the moved end, is empty.
    assert_eq!(change(&["Sheet2", "Sheet3"], &right, &moved), closed);
    assert_eq!(change(&["Sheet6", "Sheet3"], &right, &moved), closed);
    assert_eq!(change(&[], &right, &moved), closed);
    // The new order lacks an end, has another length, is empty.
    assert_eq!(change(&TABS, &right[..5], &moved), closed);
    assert_eq!(change(&TABS, &right[1..], &moved), closed);
    assert_eq!(change(&TABS, &[], &moved), closed);
    // The moved sheet is not in the old order at all.
    assert_eq!(change(&TABS, &right, &["Gone".to_string()]), None);
    // A move that names no sheet moves nothing.
    assert_eq!(change(&TABS, &right, &[]), None);
}

fn workbook(tabs: &[String], formula: &str) -> WorkbookEngine {
    let mut wb = WorkbookEngine::new();
    for name in tabs {
        let digits = name.trim_start_matches("Sheet");
        wb.add_sheet(name).set_cell("A1", digits).unwrap();
    }
    wb.add_sheet("Calc").set_cell("A1", formula).unwrap();
    wb
}

fn sum_in(tabs: &[String], formula: &str) -> f64 {
    match workbook(tabs, formula).evaluate_cell("Calc", "A1") {
        EvaluatedCellValue::Number(n) => n,
        other => panic!("expected a number, got {other:?}"),
    }
}

#[test]
fn the_rewritten_span_sums_the_sheets_the_help_page_describes() {
    let formula = "=SUM(Sheet2:Sheet6!A1)";
    let sum_after = |moved: &str, to: usize| {
        let after = reordered(&TABS, moved, to);
        let text = move_to(formula, &TABS, moved, to).unwrap_or_else(|| formula.to_string());
        sum_in(&after, &text)
    };
    // Sheet2 after Sheet6: Sheet3..Sheet6 only (3+4+5+6).
    assert_eq!(sum_after("Sheet2", 5), 18.0);
    // Sheet6 before Sheet2: Sheet2..Sheet5 (2+3+4+5).
    assert_eq!(sum_after("Sheet6", 1), 14.0);
    // Sheet2 moved between Sheet4 and Sheet5 keeps its name: the ends now hold 2, 5, 6.
    assert_eq!(sum_after("Sheet2", 3), 13.0);
    // Sheet4 moved out of the span and Sheet7 moved into it.
    assert_eq!(sum_after("Sheet4", 0), 16.0);
    assert_eq!(sum_after("Sheet7", 3), 27.0);
}

#[test]
fn the_workbook_plan_uses_its_own_tab_order_for_a_move() {
    let tabs: Vec<String> = TABS.iter().map(ToString::to_string).collect();
    let mut wb = workbook(&tabs, SPAN);
    wb.set_cell("Calc", "B1", "=SUM(Sheet1:Sheet2!A1)").unwrap();
    // The workbook holds `Calc` after the seven sheets; the new order names it too.
    let mut with_calc = TABS.to_vec();
    with_calc.push("Calc");
    let after: Vec<String> = reordered(&with_calc, "Sheet2", 5);
    let plan = wb
        .plan_sheet_rewrites(
            SheetChange::Move {
                moved: &["Sheet2".to_string()],
                tab_order: &[],
                new_order: &after,
            },
            AxisBudget::default(),
        )
        .unwrap();
    let mut texts: Vec<(String, String)> = plan
        .iter()
        .map(|(_, coord, text)| (coord.to_a1(), text.clone()))
        .collect();
    texts.sort();
    // Sheet1:Sheet2 keeps both names and now holds Sheet1 and the moved Sheet2 further on.
    assert_eq!(
        texts,
        vec![("A1".to_string(), "=SUM(Sheet3:Sheet6!A2:A5)".to_string())]
    );
}
