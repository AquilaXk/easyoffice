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

#[test]
fn multi_column_structured_range_and_indirect_structured_refs_resolve_and_follow_table_growth() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    wb.add_sheet("Sheet2");
    let mut table = SheetTable::new("Table1", CellBox::parse("B2:E4").unwrap());
    table.columns = vec!["Q1".into(), "Q2".into(), "Q3".into(), "Calc".into()];
    wb.set_tables("Sheet1", vec![table]).unwrap();
    for (col, h) in [("B", "Q1"), ("C", "Q2"), ("D", "Q3"), ("E", "Calc")] {
        wb.set_cell("Sheet1", &format!("{col}2"), h.to_string())
            .unwrap();
    }
    for (cell, v) in [
        ("B3", "10"),
        ("C3", "20"),
        ("D3", "30"),
        ("B4", "1"),
        ("C4", "2"),
        ("D4", "3"),
    ] {
        wb.set_cell("Sheet1", cell, v.to_string()).unwrap();
    }
    wb.set_cell("Sheet1", "F1", "=SUM(Table1[Q1]:Table1[Q3])".to_string())
        .unwrap();
    wb.set_cell("Sheet1", "F2", "=SUM(Table1[Q3]:Table1[Q1])".to_string())
        .unwrap();
    wb.set_cell("Sheet1", "E3", "=SUM([@Q1]:[@Q3])".to_string())
        .unwrap();
    wb.set_cell("Sheet1", "G3", "=SUM(Table1[@Q1]:Table1[@Q3])".to_string())
        .unwrap();
    wb.set_cell("Sheet1", "G4", "=SUM([@Q1]:[@Q3])".to_string())
        .unwrap();
    wb.set_cell("Sheet1", "F3", "=SUM(INDIRECT(\"Table1[Q1]\"))".to_string())
        .unwrap();
    wb.set_cell(
        "Sheet1",
        "F4",
        "=SUM(INDIRECT(\"Table1[Q1]:Table1[Q2]\"))".to_string(),
    )
    .unwrap();
    wb.set_cell("Sheet1", "F5", "=SUM(INDIRECT(\"Table1\"))".to_string())
        .unwrap();
    wb.set_cell("Sheet1", "E4", "=INDIRECT(\"[@Q2]\")".to_string())
        .unwrap();
    wb.set_cell("Sheet1", "F6", "=INDIRECT(\"Table1[Q1]+1\")".to_string())
        .unwrap();
    wb.set_cell("Sheet1", "F7", "=INDIRECT(\"NoSuchTable[Q1]\")".to_string())
        .unwrap();
    wb.set_cell(
        "Sheet1",
        "H1",
        "=TOCOL(INDIRECT(\"Table1[Q1]\"))".to_string(),
    )
    .unwrap();
    wb.set_cell("Sheet2", "A1", "=SUM(Table1[Q1]:Table1[Q2])".to_string())
        .unwrap();
    wb.set_cell(
        "Sheet2",
        "A2",
        "=SUM(INDIRECT(\"Table1[Q2]:Table1[Q3]\"))".to_string(),
    )
    .unwrap();

    let mut session = RecalcSession::new(wb).unwrap();
    assert!(!session.recalculate().timed_out);
    assert_eq!(
        session.cell_value("Sheet1", at("F1")).to_display_string(),
        "66"
    );
    assert_eq!(
        session.cell_value("Sheet1", at("F2")).to_display_string(),
        "66"
    );
    assert_eq!(
        session.cell_value("Sheet1", at("E3")).to_display_string(),
        "60"
    );
    assert_eq!(
        session.cell_value("Sheet1", at("G3")).to_display_string(),
        "60"
    );
    assert_eq!(
        session.cell_value("Sheet1", at("G4")).to_display_string(),
        "#REF!"
    );
    assert_eq!(
        session.cell_value("Sheet1", at("F3")).to_display_string(),
        "11"
    );
    assert_eq!(
        session.cell_value("Sheet1", at("F4")).to_display_string(),
        "33"
    );
    assert_eq!(
        session.cell_value("Sheet1", at("F5")).to_display_string(),
        "128"
    );
    assert_eq!(
        session.cell_value("Sheet1", at("E4")).to_display_string(),
        "2"
    );
    assert_eq!(
        session.cell_value("Sheet1", at("F6")).to_display_string(),
        "#REF!"
    );
    assert_eq!(
        session.cell_value("Sheet1", at("F7")).to_display_string(),
        "#REF!"
    );
    assert_eq!(
        session.cell_value("Sheet1", at("H1")).to_display_string(),
        "10"
    );
    assert_eq!(
        session.cell_value("Sheet1", at("H2")).to_display_string(),
        "1"
    );
    assert_eq!(
        session.cell_value("Sheet2", at("A1")).to_display_string(),
        "33"
    );
    assert_eq!(
        session.cell_value("Sheet2", at("A2")).to_display_string(),
        "55"
    );

    session.set_cell("Sheet1", at("B5"), "100").unwrap();
    session.set_cell("Sheet1", at("C5"), "200").unwrap();
    session.set_cell("Sheet1", at("D5"), "300").unwrap();
    let mut grown = SheetTable::new("Table1", CellBox::parse("B2:E5").unwrap());
    grown.columns = vec!["Q1".into(), "Q2".into(), "Q3".into(), "Calc".into()];
    session.set_sheet_table_defs("Sheet1", vec![grown]).unwrap();
    assert!(!session.recalculate().timed_out);
    assert_eq!(
        session.cell_value("Sheet1", at("F1")).to_display_string(),
        "666"
    );
    assert_eq!(
        session.cell_value("Sheet1", at("F3")).to_display_string(),
        "111"
    );
    assert_eq!(
        session.cell_value("Sheet1", at("F4")).to_display_string(),
        "333"
    );
    assert_eq!(
        session.cell_value("Sheet1", at("H3")).to_display_string(),
        "100"
    );
    assert_eq!(
        session.cell_value("Sheet2", at("A1")).to_display_string(),
        "333"
    );
    assert_eq!(
        session.cell_value("Sheet2", at("A2")).to_display_string(),
        "555"
    );
}
