//! Structured references (PD-225) under the wall-clock deadline of a recalculation (PD-215): a formula the pass
//! does not reach keeps the value it showed and is calculated by the next pass, as a formula with plain references.

use super::*;
use crate::WorkbookEngine;
use easyoffice_formats::{CellBox, SheetTable};
use std::time::Duration;

fn at(a1: &str) -> PackedCellCoordinate {
    PackedCellCoordinate::parse_a1(a1).unwrap()
}

#[test]
fn a_cut_off_pass_keeps_the_shown_value_and_the_next_one_finishes() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    let mut table = SheetTable::new("T", CellBox::parse("A1:B201").unwrap());
    table.columns = vec!["a".into(), "b".into()];
    wb.set_tables("Sheet1", vec![table]).unwrap();
    wb.set_cell("Sheet1", "A1", "a".to_string()).unwrap();
    wb.set_cell("Sheet1", "B1", "b".to_string()).unwrap();
    for r in 2..=201u32 {
        wb.set_cell("Sheet1", &format!("A{r}"), r.to_string())
            .unwrap();
        wb.set_cell("Sheet1", &format!("B{r}"), "=[@a]*2".to_string())
            .unwrap();
    }
    wb.set_cell("Sheet1", "D1", "=SUM(T[b])".to_string())
        .unwrap();
    let mut session = RecalcSession::new(wb).unwrap();
    assert!(!session.recalculate().timed_out);
    assert_eq!(
        session.cell_value("Sheet1", at("D1")).to_display_string(),
        "40600"
    );
    // The input changes and the next pass is cut off before it starts: the old value stays shown.
    session.set_cell("Sheet1", at("A2"), "1000").unwrap();
    session.budget.max_time = Duration::ZERO;
    let cut = session.recalculate();
    assert!(cut.timed_out);
    assert_eq!(
        session.cell_value("Sheet1", at("D1")).to_display_string(),
        "40600"
    );
    assert!(session.is_unreached("Sheet1", at("D1")));
    // With time again the cells that were left are calculated: 2 * 1000 instead of 2 * 2.
    session.budget.max_time = Budget::default().max_time;
    let done = session.recalculate();
    assert!(!done.timed_out);
    assert_eq!(
        session.cell_value("Sheet1", at("B2")).to_display_string(),
        "2000"
    );
    assert_eq!(
        session.cell_value("Sheet1", at("D1")).to_display_string(),
        "42596"
    );
}

#[test]
fn a_change_of_the_tables_of_one_sheet_leaves_the_unnamed_references_of_the_others() {
    let mut wb = WorkbookEngine::new();
    let tables = |name: &str, rows: u32| {
        let mut t = SheetTable::new(name, CellBox::parse(&format!("A1:B{}", rows + 1)).unwrap());
        t.columns = vec!["a".into(), "b".into()];
        vec![t]
    };
    for (sheet, table) in [("S1", "Tab_1"), ("S2", "Tab_2")] {
        wb.add_sheet(sheet);
        wb.set_tables(sheet, tables(table, 5)).unwrap();
        wb.set_cell(sheet, "A1", "a".to_string()).unwrap();
        for r in 2..=6u32 {
            wb.set_cell(sheet, &format!("A{r}"), r.to_string()).unwrap();
            wb.set_cell(sheet, &format!("B{r}"), "=[@a]*2".to_string())
                .unwrap();
        }
    }
    let mut session = RecalcSession::new(wb).unwrap();
    session.recalculate();
    // Sheet 2's table grows by a row: its five formulas are compiled again and calculated, sheet 1's are not.
    session
        .set_sheet_table_defs("S2", tables("Tab_2", 6))
        .unwrap();
    assert_eq!(session.recalculate().recomputed, 5);
    session
        .set_sheet_table_defs("S2", tables("Tab_2", 6))
        .unwrap();
    assert_eq!(
        session.recalculate().recomputed,
        0,
        "the same tables again: nothing"
    );
}
