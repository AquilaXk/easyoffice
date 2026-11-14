//! Arrays next to merged cells and next to formulas that compute their reference (PD-222): a merged range
//! blocks an area, and `OFFSET` and `INDIRECT` read the cells arrays fill in the same calculation.
#![allow(clippy::unwrap_used)]

mod dynamic_support;
use dynamic_support::new_empty_model;
use easyoffice_formats::CellBox;

fn merges(list: &[&str]) -> Vec<CellBox> {
    list.iter().map(|r| CellBox::parse(r).unwrap()).collect()
}

fn texts(m: &dynamic_support::Model, cells: &[&str]) -> Vec<String> {
    cells.iter().map(|c| m._get_text(c)).collect()
}

#[test]
fn a_merged_range_in_the_area_blocks_the_array() {
    let mut m = new_empty_model();
    m.session()
        .set_sheet_merges("Sheet1", merges(&["A2:B2"]))
        .unwrap();
    m._set("A1", "=SEQUENCE(3)");
    m._set("D1", "=SEQUENCE(3)");
    m.evaluate();
    // A2 is merged with B2: the area A1:A3 crosses it
    assert_eq!(
        texts(&m, &["A1", "A2", "D1", "D3"]),
        ["#SPILL!", "", "1", "3"]
    );
    assert!(m.last.as_ref().unwrap().spills.iter().any(|s| s.blocked));
    // unmerged: the array spills
    m.session().set_sheet_merges("Sheet1", Vec::new()).unwrap();
    m.evaluate();
    assert_eq!(texts(&m, &["A1", "A2", "A3"]), ["1", "2", "3"]);
}

#[test]
fn merging_over_a_placed_area_blocks_it_and_the_anchors_own_merge_counts() {
    let mut m = new_empty_model();
    m._set("A1", "=SEQUENCE(3)");
    m.evaluate();
    assert_eq!(m._get_text("A3"), "3");
    m.session()
        .set_sheet_merges("Sheet1", merges(&["A3:C3"]))
        .unwrap();
    m.evaluate();
    assert_eq!(texts(&m, &["A1", "A2", "A3"]), ["#SPILL!", "", ""]);
    // the merge of the anchor itself crosses the area too
    let mut m = new_empty_model();
    m.session()
        .set_sheet_merges("Sheet1", merges(&["A1:A2"]))
        .unwrap();
    m._set("A1", "=SEQUENCE(1,3)");
    m._set("A5", "=SEQUENCE(1,3)");
    m.evaluate();
    assert_eq!(
        texts(&m, &["A1", "B1", "A5", "C5"]),
        ["#SPILL!", "", "1", "3"]
    );
    // a merge beside the area does not
    m.session()
        .set_sheet_merges("Sheet1", merges(&["D5:E6"]))
        .unwrap();
    m.evaluate();
    assert_eq!(texts(&m, &["A5", "C5"]), ["1", "3"]);
}

#[test]
fn merges_of_another_sheet_do_not_block_and_a_one_cell_array_ignores_merges() {
    let mut m = new_empty_model();
    m.session()
        .set_sheet_merges("Sheet2", merges(&["A1:B3"]))
        .unwrap();
    m._set("A1", "=SEQUENCE(3)");
    m._set("C1", "=SUM(SEQUENCE(2))");
    m.session()
        .set_sheet_merges("Sheet1", merges(&["C1:C2"]))
        .unwrap();
    m.evaluate();
    assert_eq!(texts(&m, &["A1", "A3", "C1"]), ["1", "3", "3"]);
}

#[test]
fn indirect_and_offset_read_the_cells_an_array_fills_in_the_same_calculation() {
    let mut m = new_empty_model();
    m._set("A1", "=SEQUENCE(3)");
    m._set("C1", "=SUM(INDIRECT(\"A1:A3\"))");
    m._set("C2", "=OFFSET(A1,2,0)");
    m._set("C3", "=INDIRECT(\"A\"&(1+1))");
    m._set("C4", "=SUM(OFFSET(A1,0,0,3,1))");
    m.evaluate();
    assert_eq!(texts(&m, &["C1", "C2", "C3", "C4"]), ["6", "3", "2", "6"]);
    let changed = &m.last.as_ref().unwrap().changed;
    let c1 = changed
        .iter()
        .find(|c| c.coord.to_a1() == "C1")
        .expect("C1 is reported");
    assert_eq!(c1.value.to_display_string(), "6");
    // a bigger array: the readers follow without a second evaluate
    m._set("A1", "=SEQUENCE(4)");
    m.evaluate();
    assert_eq!(texts(&m, &["C1", "C2", "C3", "C4"]), ["6", "3", "2", "6"]);
    m._set("A1", "=SEQUENCE(3,1,10)");
    m.evaluate();
    assert_eq!(
        texts(&m, &["C1", "C2", "C3", "C4"]),
        ["33", "12", "11", "33"]
    );
}

#[test]
fn an_array_that_reads_its_own_area_through_indirect_settles_or_is_circular() {
    let mut m = new_empty_model();
    // reads the cell it fills: the area stops changing after one round
    m._set("B1", "=SEQUENCE(2+0*SUM(INDIRECT(\"B2:B5\")))");
    m.evaluate();
    assert_eq!(texts(&m, &["B1", "B2"]), ["1", "2"]);
    // each size it makes changes what it reads (3 cells fill what it sums, then it makes 1): it never settles,
    // so it is the circular error
    let mut m = new_empty_model();
    m._set("B1", "=SEQUENCE(IF(SUM(INDIRECT(\"B2:B3\"))=0,3,1))");
    m.evaluate();
    let outcome = m.last.as_ref().unwrap();
    assert!(outcome.circular);
    assert_eq!(m._get_text("B2"), "");
    // two arrays that fill what the other one's size reads
    let mut m = new_empty_model();
    m._set("A1", "=SEQUENCE(IF(SUM(INDIRECT(\"C2:C3\"))=0,3,1))");
    m._set("C1", "=SEQUENCE(IF(SUM(INDIRECT(\"A2:A3\"))=0,1,3))");
    m.evaluate();
    assert!(m.last.as_ref().unwrap().circular);
}

#[test]
fn volatile_readers_of_an_array_do_not_keep_the_calculation_going() {
    let mut m = new_empty_model();
    m._set("A1", "=SEQUENCE(3)");
    m._set("B1", "=SUM(INDIRECT(\"A1:A3\"))+0*RAND()");
    m._set("B2", "=NOW()+SUM(INDIRECT(\"A1:A3\"))*0");
    m.evaluate();
    assert!(!m.last.as_ref().unwrap().circular);
    assert_eq!(m._get_text("B1"), "6");
    m.evaluate();
    assert!(!m.last.as_ref().unwrap().circular);
    assert_eq!(m._get_text("B1"), "6");
}

#[test]
fn a_reader_that_stops_reading_the_area_is_not_recalculated_for_it() {
    let mut m = new_empty_model();
    m._set("A1", "=SEQUENCE(3)");
    m._set("D1", "=IF(E1=1,SUM(INDIRECT(\"A1:A3\")),0)");
    m._set("E1", "1");
    m.evaluate();
    assert_eq!(m._get_text("D1"), "6");
    m._set("E1", "0");
    m.evaluate();
    m._set("A1", "=SEQUENCE(4)");
    m.evaluate();
    assert_eq!(m._get_text("D1"), "0");
}

#[test]
fn a_reader_of_a_huge_range_stays_cheap() {
    let mut m = new_empty_model();
    m._set("A1", "=SEQUENCE(1000)");
    for i in 0..40 {
        m._set(
            &format!("C{}", i + 1),
            &format!("=SUM(INDIRECT(\"A1:A{}\"))", 10 + i * 20),
        );
    }
    m._set("D1", "=COUNT(INDIRECT(\"A:A\"))");
    m.evaluate();
    assert_eq!(m._get_text("D1"), "1000");
    assert_eq!(m._get_text("C1"), "55");
}

#[test]
fn a_row_edit_moves_the_merges_the_array_may_not_cross() {
    use easyoffice_sheet::{Axis, AxisBudget, AxisEdit, EditKind};
    let mut m = new_empty_model();
    m.session()
        .set_sheet_merges("Sheet1", merges(&["A6:A7"]))
        .unwrap();
    m._set("A1", "=SEQUENCE(4)");
    m.evaluate();
    assert_eq!(m._get_text("A4"), "4");
    // two rows between the area and the merge go: the merge now starts inside the area
    let edit = AxisEdit::new(Axis::Row, EditKind::Delete, 3, 2).unwrap();
    m.session()
        .apply_axis_edit("Sheet1", edit, AxisBudget::default())
        .unwrap();
    assert_eq!(texts(&m, &["A1", "A2"]), ["#SPILL!", ""]);
    // the page sends the same merge at its new place: nothing changes
    m.session()
        .set_sheet_merges("Sheet1", merges(&["A4:A5"]))
        .unwrap();
    m.evaluate();
    assert_eq!(m._get_text("A1"), "#SPILL!");
}

#[test]
fn a_workbook_without_a_session_places_arrays_around_merges_and_reads_them_through_indirect() {
    use easyoffice_sheet::{WorkbookEngine, WorkbookEvalTarget};
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("S");
    for (cell, v) in [
        ("A1", "=SEQUENCE(3)"),
        ("C1", "=SUM(INDIRECT(\"A1:A3\"))"),
        ("E1", "=SEQUENCE(2)"),
    ] {
        wb.set_cell("S", cell, v.to_string()).unwrap();
    }
    wb.sheet_mut("S")
        .unwrap()
        .set_spill_merges(merges(&["E2:F2"]));
    let target = |cell| WorkbookEvalTarget {
        sheet: "S",
        expr: cell,
        anchor: None,
    };
    let batch = wb.evaluate_targets(&[target("C1"), target("A3"), target("E1")]);
    let shown: Vec<String> = batch.values.iter().map(|v| v.to_display_string()).collect();
    assert_eq!(shown, ["6", "3", "#SPILL!"]);
}

#[test]
fn a_formula_with_more_references_than_the_list_holds_is_still_calculated_after_the_array() {
    let mut m = new_empty_model();
    m._set("A1", "=SEQUENCE(40)");
    let reads: Vec<String> = (1..=40).map(|i| format!("INDIRECT(\"A{i}\")")).collect();
    m._set("C1", &format!("={}", reads.join("+")));
    m.evaluate();
    assert_eq!(m._get_text("C1"), "820");
    m._set("A1", "=SEQUENCE(40,1,2)");
    m.evaluate();
    assert_eq!(m._get_text("C1"), "860");
}

#[test]
fn a_formula_that_reads_itself_through_indirect_or_offset_is_a_circular_reference() {
    let mut m = new_empty_model();
    m._set("A1", "=SEQUENCE(3)");
    m._set("C1", "=SUM(INDIRECT(\"C1:C3\"))");
    m._set("C2", "=OFFSET(C2,0,0)+1");
    m._set("E1", "=SUM(INDIRECT(\"A1:A3\"))");
    m.evaluate();
    assert!(m.last.as_ref().unwrap().circular);
    assert_eq!(m._get_text("E1"), "6");
    // and the next calculation ends too
    m.evaluate();
    assert_eq!(m._get_text("E1"), "6");
}

#[test]
fn readers_of_a_filled_range_follow_edits_of_the_input_the_array_is_made_from() {
    let mut m = new_empty_model();
    m._set("A1", "3");
    m._set("C1", "=SEQUENCE(A1)");
    m._set("E1", "=SUM(INDIRECT(\"C1:C10\"))");
    m._set("E2", "=COUNT(OFFSET(C1,0,0,10,1))");
    m.evaluate();
    assert_eq!(texts(&m, &["E1", "E2"]), ["6", "3"]);
    m._set("A1", "5");
    m.evaluate();
    assert_eq!(texts(&m, &["E1", "E2"]), ["15", "5"]);
    m._set("A1", "1");
    m.evaluate();
    assert_eq!(texts(&m, &["E1", "E2"]), ["1", "1"]);
}
