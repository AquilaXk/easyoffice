//! Data validation against other sheets and the memo of a session (external review M-14).

use super::*;
use crate::recalc::RecalcSession;
use crate::{EvaluatedCellValue, PackedCellCoordinate, WorkbookEngine};
use easyoffice_formats::{parse_sqref, Cmp, DataValidation, DvKind, SheetRules};

fn at(a1: &str) -> PackedCellCoordinate {
    PackedCellCoordinate::parse_a1(a1).unwrap()
}

fn rule(kind: DvKind, a: &str) -> DataValidation {
    let mut v = DataValidation::new(parse_sqref("B2:B9"), kind);
    v.cmp = Cmp::Between;
    v.a = a.into();
    v.error = "no".into();
    v
}

fn two_sheets() -> WorkbookEngine {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    wb.add_sheet("Sheet2");
    wb.add_sheet("My Sheet");
    for (sheet, a1, raw) in [
        ("Sheet1", "D1", "local"),
        ("Sheet2", "A1", "red"),
        ("Sheet2", "A2", "green"),
        ("My Sheet", "A1", "it's"),
    ] {
        wb.set_cell(sheet, a1, raw).unwrap();
    }
    wb
}

fn session_with(wb: WorkbookEngine, rules: Vec<DataValidation>) -> RecalcSession {
    let mut session = RecalcSession::new(wb).unwrap();
    session.recalculate();
    let rules = SheetRules {
        dv: rules,
        ..SheetRules::default()
    };
    session.set_rules("Sheet1", rules).unwrap();
    session
}

fn accepts(session: &mut RecalcSession, cell: &str, input: &str) -> bool {
    session.validate("Sheet1", at(cell), input).unwrap() == Verdict::Valid
}

#[test]
fn a_list_range_on_another_sheet_reads_that_sheet() {
    for kind in [DvKind::List, DvKind::ListMultiple] {
        let mut s = session_with(two_sheets(), vec![rule(kind, "=Sheet2!$A$1:$A$3")]);
        assert!(accepts(&mut s, "B2", "red"), "{kind:?}");
        assert!(accepts(&mut s, "B2", "green"), "{kind:?}");
        assert!(
            !accepts(&mut s, "B2", "local"),
            "{kind:?}: the own sheet is not read"
        );
        assert!(!accepts(&mut s, "B2", "blue"), "{kind:?}");
    }
}

#[test]
fn a_list_range_names_its_sheet_with_quotes_and_any_case() {
    let mut s = session_with(two_sheets(), vec![rule(DvKind::List, "='My Sheet'!A1:A2")]);
    assert!(accepts(&mut s, "B2", "it's"));
    let mut s = session_with(two_sheets(), vec![rule(DvKind::List, "=SHEET2!A1:A2")]);
    assert!(accepts(&mut s, "B2", "green"));
    let mut s = session_with(two_sheets(), vec![rule(DvKind::List, "=Sheet1!$D$1:$D$2")]);
    assert!(accepts(&mut s, "B2", "local"));
}

#[test]
fn a_list_range_on_a_missing_sheet_offers_nothing() {
    let mut s = session_with(two_sheets(), vec![rule(DvKind::List, "=Gone!A1:A2")]);
    assert!(!accepts(&mut s, "B2", "red"));
    assert!(!accepts(&mut s, "B2", "#REF!"));
}

fn custom_session() -> RecalcSession {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    wb.set_cell("Sheet1", "C2", "=B2*2").unwrap();
    wb.set_cell("Sheet1", "D2", "=C2+1").unwrap();
    session_with(wb, vec![rule(DvKind::Custom, "D2>10")])
}

#[test]
fn a_custom_formula_sees_the_formulas_that_read_the_input() {
    let mut s = custom_session();
    assert!(accepts(&mut s, "B2", "6"), "6*2+1 is 13");
    assert!(!accepts(&mut s, "B2", "4"), "4*2+1 is 9");
}

#[test]
fn a_custom_check_leaves_the_session_memo_as_it_was() {
    let mut s = custom_session();
    let cells = |s: &RecalcSession| {
        (
            s.cell_value("Sheet1", at("C2")),
            s.cell_value("Sheet1", at("D2")),
        )
    };
    let before = cells(&s);
    assert_eq!(before.1, EvaluatedCellValue::Number(1.0));
    assert!(accepts(&mut s, "B2", "6"));
    assert!(!accepts(&mut s, "B2", "1"));
    assert_eq!(
        cells(&s),
        before,
        "a rejected or tried input leaves no value behind"
    );
}

#[test]
fn a_custom_check_that_runs_out_of_time_rejects_and_restores_the_cell() {
    let mut wb = WorkbookEngine::new();
    wb.add_sheet("Sheet1");
    wb.set_cell("Sheet1", "B2", "keep").unwrap();
    let rules = [rule(DvKind::Custom, "B2>1")];
    let sid = wb.sheets()[0].resolved_sheet_id();
    let (verdict, _) = validate::validate_within(
        &mut wb,
        0,
        &rules,
        at("B2"),
        "99",
        crate::MemoTable::new(sid),
        std::time::Duration::ZERO,
    );
    assert!(matches!(verdict, Verdict::Invalid { .. }));
    assert_eq!(wb.sheet("Sheet1").unwrap().get_raw_cell("B2"), Some("keep"));
}
