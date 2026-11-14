//! Structured references (PD-225): `Table1[Q1]`, `[@Q1]`, the special items, names, edits.
#![allow(clippy::unwrap_used)]

mod dynamic_support;
use dynamic_support::new_empty_model;
use easyoffice_formats::{CellBox, SheetTable};

fn table(name: &str, range: &str, columns: &[&str], totals: bool) -> SheetTable {
    let mut t = SheetTable::new(name, CellBox::parse(range).unwrap());
    t.columns = columns.iter().map(|c| c.to_string()).collect();
    t.totals = totals;
    t
}

/// Sheet1 A1:C5 is `Table1` (Q1, Q2, Total); data rows 2-4; the totals row is 5.
fn model() -> dynamic_support::Model {
    let mut m = new_empty_model();
    for (cell, v) in [
        ("A1", "Q1"),
        ("B1", "Q2"),
        ("C1", "Total"),
        ("A2", "1"),
        ("B2", "10"),
        ("A3", "2"),
        ("B3", "20"),
        ("A4", "3"),
        ("B4", "30"),
    ] {
        m._set(cell, v);
    }
    m.session()
        .set_sheet_table_defs(
            "Sheet1",
            vec![table("Table1", "A1:C5", &["Q1", "Q2", "Total"], true)],
        )
        .unwrap();
    m
}

fn text(m: &mut dynamic_support::Model, formula: &str) -> String {
    m._set("F1", formula);
    m.evaluate();
    m._get_text("F1")
}

#[test]
fn a_column_a_range_and_the_special_items() {
    let mut m = model();
    assert_eq!(text(&mut m, "=SUM(Table1[Q1])"), "6");
    assert_eq!(text(&mut m, "=sum(table1[q2])"), "60");
    assert_eq!(text(&mut m, "=SUM(Table1[[Q1]:[Q2]])"), "66");
    assert_eq!(text(&mut m, "=SUM(Table1)"), "66");
    assert_eq!(text(&mut m, "=SUM(Table1[#Data])"), "66");
    assert_eq!(text(&mut m, "=COUNTA(Table1[#All])"), "9");
    assert_eq!(text(&mut m, "=INDEX(Table1[#Headers],1,2)"), "Q2");
    assert_eq!(text(&mut m, "=ROWS(Table1[[#Headers],[#Data]])"), "4");
    assert_eq!(text(&mut m, "=Table1[[#Headers],[Q2]]"), "Q2");
}

#[test]
fn this_row_and_the_calculated_column() {
    let mut m = model();
    for r in 2..=4 {
        m._set(&format!("C{r}"), "=[@Q1]*[@Q2]");
    }
    m._set("E2", "=Table1[[#This Row],[Q2]]+1");
    m._set("E6", "=Table1[[#This Row],[Q2]]");
    m.evaluate();
    assert_eq!(m._get_text("C2"), "10");
    assert_eq!(m._get_text("C4"), "90");
    assert_eq!(m._get_text("E2"), "11");
    // Row 6 is not a data row of the table.
    assert_eq!(m._get_text("E6"), "#VALUE!");
    assert_eq!(text(&mut m, "=SUM(Table1[Total])"), "140");
}

#[test]
fn missing_tables_columns_and_items() {
    let mut m = model();
    assert_eq!(text(&mut m, "=SUM(Nope[Q1])"), "#REF!");
    assert_eq!(text(&mut m, "=SUM(Table1[Q9])"), "#REF!");
    assert_eq!(text(&mut m, "=SUM(Table1[[#Totals],[Q1]])"), "0");
    assert_eq!(text(&mut m, "=Table1[#This Row]"), "#VALUE!");
    assert_eq!(
        text(&mut m, "=SUM(Table1[[#Totals],[#Headers]])"),
        "#VALUE!"
    );
    assert_eq!(text(&mut m, "=SUM([@Q1])"), "#REF!");
    m.session()
        .set_sheet_table_defs("Sheet1", Vec::new())
        .unwrap();
    m.evaluate();
    assert_eq!(text(&mut m, "=SUM(Table1[Q1])"), "#REF!");
}

use easyoffice_sheet::{Axis, AxisBudget, AxisEdit, EditKind};

fn edit(m: &mut dynamic_support::Model, axis: Axis, kind: EditKind, at1: u32, count: u32) {
    let e = AxisEdit::new(axis, kind, at1 - 1, count).unwrap();
    m.session()
        .apply_axis_edit("Sheet1", e, AxisBudget::default())
        .unwrap();
}

fn raw(m: &mut dynamic_support::Model, cell: &str) -> String {
    let sheet = m.session().workbook().sheet("Sheet1").unwrap();
    sheet.get_raw_cell(cell).unwrap_or_default().to_string()
}

#[test]
fn a_table_that_grows_or_moves_takes_the_references_with_it() {
    let mut m = model();
    m._set("E20", "=SUM(Table1[Q1])");
    m._set("E21", "=SUM(Table1[[#Totals],[Q1]])");
    m._set("E22", "=ROWS(Table1[#All])");
    m._set("A5", "=SUBTOTAL(109,Table1[Q1])");
    m.evaluate();
    assert_eq!(texts(&m, &["E20", "E21", "E22"]), ["6", "6", "5"]);
    // A row inserted inside the table: the totals row moves down and the new row is data.
    edit(&mut m, Axis::Row, EditKind::Insert, 3, 1);
    m._set("A3", "100");
    m.evaluate();
    assert_eq!(raw(&mut m, "E21"), "=SUM(Table1[Q1])");
    assert_eq!(
        texts(&m, &["E21", "E22", "E23", "A6"]),
        ["106", "106", "6", "106"]
    );
    // Two rows inserted above the table move it.
    edit(&mut m, Axis::Row, EditKind::Insert, 1, 2);
    assert_eq!(texts(&m, &["E23", "E24", "E25"]), ["106", "106", "6"]);
    // Two data rows deleted: the table shrinks.
    edit(&mut m, Axis::Row, EditKind::Delete, 5, 2);
    assert_eq!(
        texts(&m, &["E21", "E22", "E23", "A6"]),
        ["4", "4", "4", "4"]
    );
    // The column of Q1 is deleted: the reference to it is an error, the others read on.
    m._set("E30", "=SUM(Table1[Q2])");
    edit(&mut m, Axis::Col, EditKind::Delete, 1, 1);
    assert_eq!(raw(&mut m, "D21"), "=SUM(Table1[Q1])");
    assert_eq!(m._get_text("D21"), "#REF!");
    assert_eq!(m._get_text("D30"), "40");
}

#[test]
fn a_change_of_the_tables_compiles_the_formulas_that_name_one_again() {
    let mut m = model();
    m._set("Sheet2!A1", "=SUM(Table1[Q2])");
    m._set("Sheet2!A2", "=SUM(Other[x])");
    m.evaluate();
    assert_eq!(texts(&m, &["Sheet2!A1", "Sheet2!A2"]), ["60", "#REF!"]);
    // The table grows by a row.
    m._set("B5", "40");
    m._set("A5", "4");
    m.session()
        .set_sheet_table_defs(
            "Sheet1",
            vec![table("Table1", "A1:C5", &["Q1", "Q2", "Total"], false)],
        )
        .unwrap();
    m.evaluate();
    assert_eq!(m._get_text("Sheet2!A1"), "100");
    // A table on the other sheet appears: the formula that named it reads it.
    m._set("Sheet2!C1", "5");
    m._set("Sheet2!C2", "x");
    m.session()
        .set_sheet_table_defs("Sheet2", vec![table("Other", "C1:C2", &["x"], false)])
        .unwrap();
    m.evaluate();
    assert_eq!(m._get_text("Sheet2!A2"), "0");
    // A column of the table renamed: the old name is gone.
    m.session()
        .set_sheet_table_defs(
            "Sheet1",
            vec![table("Table1", "A1:C5", &["Q1", "Q2x", "Total"], false)],
        )
        .unwrap();
    m.evaluate();
    assert_eq!(m._get_text("Sheet2!A1"), "#REF!");
}

#[test]
fn defined_names_can_hold_a_structured_reference() {
    let mut m = model();
    m.new_defined_name(dynamic_support::DefinedNameEntry::new("Qs", "=Table1[Q1]"))
        .unwrap();
    m.new_defined_name(dynamic_support::DefinedNameEntry::new(
        "Hdr",
        "=Table1[#Headers]",
    ))
    .unwrap();
    assert_eq!(text(&mut m, "=SUM(Qs)"), "6");
    assert_eq!(text(&mut m, "=COUNTA(Hdr)"), "3");
    m._set("F2", "=SUM(Qs)");
    m._set("A5", "4");
    m.session()
        .set_sheet_table_defs(
            "Sheet1",
            vec![table("Table1", "A1:C5", &["Q1", "Q2", "Total"], false)],
        )
        .unwrap();
    m.evaluate();
    assert_eq!(m._get_text("F2"), "10");
}

fn texts(m: &dynamic_support::Model, cells: &[&str]) -> Vec<String> {
    cells.iter().map(|c| m._get_text(c)).collect()
}

#[test]
fn a_column_spills_and_feeds_the_array_functions() {
    let mut m = model();
    m._set("H1", "=Table1[Q1]");
    m._set("J1", "=SORT(Table1[Q2],,-1)");
    m._set("L1", "=SUM(FILTER(Table1[Q1],Table1[Q2]>10))");
    m._set("L2", "=XLOOKUP(2,Table1[Q1],Table1[Q2])");
    m._set("L3", "=SUMPRODUCT(Table1[Q1],Table1[Q2])");
    m.evaluate();
    assert_eq!(texts(&m, &["H1", "H2", "H3", "H4"]), ["1", "2", "3", ""]);
    assert_eq!(texts(&m, &["J1", "J2", "J3"]), ["30", "20", "10"]);
    assert_eq!(texts(&m, &["L1", "L2", "L3"]), ["5", "20", "140"]);
    // An array that would run into another table is blocked, as with a plain range.
    m._set("H6", "x");
    m._set("H7", "1");
    m.session()
        .set_sheet_table_defs(
            "Sheet1",
            vec![
                table("Table1", "A1:C5", &["Q1", "Q2", "Total"], true),
                table("Z", "H2:H7", &["x"], false),
            ],
        )
        .unwrap();
    m.evaluate();
    assert_eq!(m._get_text("H1"), "#SPILL!");
}

#[test]
fn names_with_special_characters_and_other_alphabets() {
    let mut m = new_empty_model();
    let names = [
        "Q1 $",
        "it's",
        "a[1]",
        "#tag",
        "@at",
        "매출 합계",
        "x,y",
        "a:b",
    ];
    for (i, n) in names.iter().enumerate() {
        m._set(&format!("{}1", (b'A' + i as u8) as char), n);
        m._set(
            &format!("{}2", (b'A' + i as u8) as char),
            &format!("{}", i + 1),
        );
    }
    m.session()
        .set_sheet_table_defs("Sheet1", vec![table("T", "A1:H2", &names, false)])
        .unwrap();
    for (name, shown) in [
        ("Q1 $", "[[Q1 $]]"),
        ("it's", "[[it''s]]"),
        ("a[1]", "[[a'[1']]]"),
        ("#tag", "[['#tag]]"),
        ("@at", "[['@at]]"),
        ("매출 합계", "[매출 합계]"),
        ("x,y", "[[x,y]]"),
        ("a:b", "[[a:b]]"),
    ] {
        let i = names.iter().position(|n| *n == name).unwrap();
        let formula = format!("=SUM(T{shown})");
        assert_eq!(text(&mut m, &formula), format!("{}", i + 1), "{formula}");
    }
    assert_eq!(text(&mut m, "=SUM(T[[Q1 $]:[it''s]])"), "3");
    assert_eq!(text(&mut m, "=SUM(T[매출 합계])"), "6");
    assert_eq!(
        text(&mut m, "=SUM(t[Q1 $])"),
        "1",
        "a reference reads names without case"
    );
}

#[test]
fn a_table_on_another_sheet_and_this_row_across_sheets() {
    let mut m = model();
    m._set("Sheet2!A1", "=SUM(Table1[Q1])");
    m._set("Sheet2!A2", "=Table1[[#This Row],[Q1]]");
    m.evaluate();
    // `#This Row` is the formula's row on the table's own sheet only.
    assert_eq!(texts(&m, &["Sheet2!A1", "Sheet2!A2"]), ["6", "#VALUE!"]);
    // A table of one data row and one column: the reference is one cell.
    m._set("Sheet1!E1", "5");
    m._set("Sheet1!E2", "6");
    m.session()
        .set_sheet_table_defs(
            "Sheet1",
            vec![
                table("Table1", "A1:C5", &["Q1", "Q2", "Total"], true),
                table("One", "E1:E2", &["h"], false),
            ],
        )
        .unwrap();
    m._set("Sheet2!B1", "=One[h]+1");
    m._set("Sheet2!B2", "=ROWS(One[h])");
    m.evaluate();
    assert_eq!(texts(&m, &["Sheet2!B1", "Sheet2!B2"]), ["7", "1"]);
}

#[test]
fn a_table_without_a_header_or_totals_row() {
    let mut m = new_empty_model();
    for r in 1..=3 {
        m._set(&format!("A{r}"), &format!("{r}"));
    }
    let mut t = table("Plain", "A1:A3", &["Column1"], false);
    t.header = false;
    m.session().set_sheet_table_defs("Sheet1", vec![t]).unwrap();
    assert_eq!(text(&mut m, "=SUM(Plain[Column1])"), "6");
    assert_eq!(text(&mut m, "=SUM(Plain[#Headers])"), "#REF!");
    assert_eq!(text(&mut m, "=SUM(Plain[#Totals])"), "#REF!");
    assert_eq!(text(&mut m, "=ROWS(Plain[#All])"), "3");
}

#[test]
fn a_sort_moves_a_relative_cell_but_not_a_structured_reference() {
    let moved =
        easyoffice_sheet::sorted_formula("=[@A5]*A5+Sales[[#This Row],[A5]]", "S", 0, 9, 2, 6);
    assert_eq!(moved, "=[@A5]*A9+Sales[[#This Row],[A5]]");
}
