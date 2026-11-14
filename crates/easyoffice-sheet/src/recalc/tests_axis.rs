//! Row and column edits inside a session: the values after an edit equal a fresh session built
//! from the edited workbook.

use super::*;
use crate::axis::{Axis, AxisBudget, AxisEdit, AxisEditReport, EditKind, SheetChange};
use std::collections::HashMap;
use std::time::Duration;

fn at(a1: &str) -> PackedCellCoordinate {
    PackedCellCoordinate::parse_a1(a1).unwrap()
}

fn session(sheets: &[&str], cells: &[(&str, &str, &str)]) -> RecalcSession {
    let mut wb = WorkbookEngine::new();
    for name in sheets {
        wb.add_sheet(*name);
    }
    for &(sheet, a1, raw) in cells {
        wb.set_cell(sheet, a1, raw).unwrap();
    }
    let mut s = RecalcSession::new(wb).unwrap();
    s.recalculate();
    s
}

fn row_edit(kind: EditKind, row1: u32, count: u32) -> AxisEdit {
    AxisEdit::new(Axis::Row, kind, row1 - 1, count).unwrap()
}

fn raw(s: &RecalcSession, sheet: &str, a1: &str) -> Option<String> {
    s.workbook()
        .sheet(sheet)
        .unwrap()
        .raw_cells()
        .get(&at(a1))
        .cloned()
}

fn values(s: &RecalcSession) -> HashMap<(String, String), String> {
    let mut out = HashMap::new();
    for sheet in s.workbook().sheets() {
        for (&coord, text) in sheet.raw_cells() {
            if text.starts_with('=') {
                out.insert(
                    (sheet.name.clone(), coord.to_a1()),
                    s.cell_value(&sheet.name, coord).to_display_string(),
                );
            }
        }
    }
    out
}

/// A fresh session over the same cells: the oracle.
fn fresh_values(s: &RecalcSession) -> HashMap<(String, String), String> {
    let mut fresh = RecalcSession::new(s.workbook().clone()).unwrap();
    fresh.recalculate();
    values(&fresh)
}

fn edit_ok(s: &mut RecalcSession, sheet: &str, e: AxisEdit) -> (AxisEditReport, RecalcOutcome) {
    let out = s
        .apply_axis_edit(sheet, e, AxisBudget::within(Duration::from_secs(30)))
        .unwrap();
    assert_eq!(values(s), fresh_values(s), "{e:?}");
    out
}

#[test]
fn inserting_a_row_moves_cells_rewrites_formulas_and_keeps_values() {
    let mut s = session(
        &["S", "T"],
        &[
            ("S", "A1", "1"),
            ("S", "A2", "2"),
            ("S", "A3", "3"),
            ("S", "B1", "=SUM(A1:A3)"),
            ("S", "B3", "=A3*10"),
            ("T", "A1", "=S!A3+SUM(S!A1:A3)"),
        ],
    );
    let (report, out) = edit_ok(&mut s, "S", row_edit(EditKind::Insert, 2, 1));
    assert_eq!(raw(&s, "S", "A2"), None);
    assert_eq!(raw(&s, "S", "A3").as_deref(), Some("2"));
    assert_eq!(raw(&s, "S", "A4").as_deref(), Some("3"));
    assert_eq!(raw(&s, "S", "B1").as_deref(), Some("=SUM(A1:A4)"));
    assert_eq!(raw(&s, "S", "B4").as_deref(), Some("=A4*10"));
    assert_eq!(raw(&s, "T", "A1").as_deref(), Some("=S!A4+SUM(S!A1:A4)"));
    assert_eq!(report.rewritten.len(), 3);
    // Nothing a formula shows changed, so nothing is reported.
    assert!(out.changed.is_empty(), "{:?}", out.changed);
    assert_eq!(s.cell_value("S", at("B1")), EvaluatedCellValue::Number(6.0));
}

#[test]
fn deleting_rows_turns_lost_references_into_ref_errors() {
    let mut s = session(
        &["S"],
        &[
            ("S", "A1", "1"),
            ("S", "A2", "2"),
            ("S", "A3", "3"),
            ("S", "A4", "4"),
            ("S", "B1", "=A2*2"),
            ("S", "B4", "=SUM(A1:A4)+A3"),
        ],
    );
    let (report, out) = edit_ok(&mut s, "S", row_edit(EditKind::Delete, 2, 2));
    assert_eq!(report.removed_cells, 2);
    assert_eq!(raw(&s, "S", "B1").as_deref(), Some("=#REF!*2"));
    assert_eq!(raw(&s, "S", "B2").as_deref(), Some("=SUM(A1:A2)+#REF!"));
    assert_eq!(raw(&s, "S", "A2").as_deref(), Some("4"));
    assert!(out
        .changed
        .iter()
        .any(|c| c.coord == at("B1") && c.value == EvaluatedCellValue::Error(FormulaError::Ref)));
}

#[test]
fn column_edits_move_columns_and_whole_column_references() {
    let mut s = session(
        &["S"],
        &[
            ("S", "A1", "1"),
            ("S", "B1", "2"),
            ("S", "C1", "=SUM(A:B)"),
            ("S", "D1", "=B1+C1"),
        ],
    );
    let ins = AxisEdit::new(Axis::Col, EditKind::Insert, 1, 2).unwrap();
    edit_ok(&mut s, "S", ins);
    assert_eq!(raw(&s, "S", "E1").as_deref(), Some("=SUM(A:D)"));
    assert_eq!(raw(&s, "S", "F1").as_deref(), Some("=D1+E1"));
    let del = AxisEdit::new(Axis::Col, EditKind::Delete, 0, 1).unwrap();
    edit_ok(&mut s, "S", del);
    assert_eq!(raw(&s, "S", "D1").as_deref(), Some("=SUM(A:C)"));
}

#[test]
fn a_refused_edit_changes_nothing() {
    let mut s = session(&["S"], &[("S", "A1048576", "9"), ("S", "B1", "=A1048576")]);
    let before = values(&s);
    let err = s.apply_axis_edit(
        "S",
        row_edit(EditKind::Insert, 1, 1),
        AxisBudget::within(Duration::from_secs(5)),
    );
    assert!(err.is_err());
    assert_eq!(raw(&s, "S", "A1048576").as_deref(), Some("9"));
    assert_eq!(values(&s), before);
    assert!(s
        .apply_axis_edit(
            "Nope",
            row_edit(EditKind::Insert, 1, 1),
            AxisBudget::within(Duration::from_secs(5))
        )
        .is_err());

    let mut many = session(&["S"], &[]);
    for r in 1..=2_500 {
        many.set_cell("S", at(&format!("A{r}")), &format!("=B{r}"))
            .unwrap();
    }
    many.recalculate();
    let err = many.apply_axis_edit(
        "S",
        row_edit(EditKind::Insert, 1, 1),
        AxisBudget::within(Duration::ZERO),
    );
    assert!(matches!(err, Err(RecalcError::Invalid(_))));
    assert_eq!(raw(&many, "S", "A1").as_deref(), Some("=B1"));
}

#[test]
fn random_edit_sequences_match_a_fresh_session() {
    for seed in 1..=12u64 {
        let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let mut rnd = |n: u32| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % u64::from(n)) as u32
        };
        let mut cells: Vec<(String, String, String)> = Vec::new();
        for r in 1..=12u32 {
            for (c, sheet) in [("A", "S"), ("A", "T")] {
                cells.push((sheet.into(), format!("{c}{r}"), (rnd(50) + 1).to_string()));
            }
            let other = ["S", "T"][rnd(2) as usize];
            let (r1, r2) = (rnd(12) + 1, rnd(12) + 1);
            let formula = match rnd(5) {
                0 => format!("=A{r1}+A{r2}"),
                1 => format!("=SUM(A{}:A{})", r1.min(r2), r1.max(r2)),
                2 => format!("=SUM({other}!A{}:A{})", r1.min(r2), r1.max(r2)),
                3 => format!("=MATCH(A{r1},A:A,0)+ROW()"),
                _ => format!("={other}!A{r1}*2+$A${r2}"),
            };
            cells.push(("S".into(), format!("B{r}"), formula));
            cells.push(("T".into(), format!("B{r}"), format!("=S!B{r1}+COUNT(A:B)")));
        }
        let borrowed: Vec<(&str, &str, &str)> = cells
            .iter()
            .map(|(a, b, c)| (a.as_str(), b.as_str(), c.as_str()))
            .collect();
        let mut s = session(&["S", "T"], &borrowed);
        for _ in 0..8 {
            let kind = if rnd(2) == 0 {
                EditKind::Insert
            } else {
                EditKind::Delete
            };
            let axis = if rnd(3) == 0 { Axis::Col } else { Axis::Row };
            let sheet = ["S", "T"][rnd(2) as usize];
            let e = AxisEdit::new(axis, kind, rnd(8), rnd(3) + 1).unwrap();
            edit_ok(&mut s, sheet, e);
        }
    }
}

#[test]
fn insertions_never_change_what_a_formula_shows() {
    // Oracle independent of the rewriter: every formula still computes the value it had, at the
    // position the edit moved it to.
    for seed in 1..=20u64 {
        let mut state = seed.wrapping_mul(0xD6E8_FEB8_6659_FD93) | 1;
        let mut rnd = |n: u32| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % u64::from(n)) as u32
        };
        let mut cells: Vec<(String, String, String)> = Vec::new();
        for r in 1..=10u32 {
            for sheet in ["S", "T"] {
                cells.push((sheet.into(), format!("A{r}"), (rnd(40) + 1).to_string()));
                cells.push((sheet.into(), format!("B{r}"), (rnd(40) + 1).to_string()));
            }
            let other = ["S", "T"][rnd(2) as usize];
            let (r1, r2) = (rnd(10) + 1, rnd(10) + 1);
            let (lo, hi) = (r1.min(r2), r1.max(r2));
            let formula = match rnd(6) {
                0 => format!("=A{r1}+B{r2}"),
                1 => format!("=SUM(A{lo}:B{hi})"),
                2 => format!("=SUM({other}!A{lo}:A{hi})*2"),
                3 => "=SUM(A:A)+COUNT(B:B)".to_string(),
                4 => format!("=SUM({lo}:{hi})"),
                _ => format!("={other}!B{r1}*2+$A${r2}"),
            };
            cells.push(("S".into(), format!("C{r}"), formula.clone()));
            cells.push(("T".into(), format!("C{r}"), formula.replace("S!", "T!")));
        }
        let borrowed: Vec<(&str, &str, &str)> = cells
            .iter()
            .map(|(a, b, c)| (a.as_str(), b.as_str(), c.as_str()))
            .collect();
        let mut s = session(&["S", "T"], &borrowed);
        for _ in 0..6 {
            let before = values(&s);
            let axis = if rnd(3) == 0 { Axis::Col } else { Axis::Row };
            let sheet = ["S", "T"][rnd(2) as usize];
            let e = AxisEdit::new(axis, EditKind::Insert, rnd(8), rnd(3) + 1).unwrap();
            s.apply_axis_edit(sheet, e, AxisBudget::within(Duration::from_secs(30)))
                .unwrap();
            let after = values(&s);
            assert_eq!(before.len(), after.len());
            for ((name, a1), want) in &before {
                let coord = at(a1);
                let moved = if name == sheet {
                    e.map_coord(coord).unwrap()
                } else {
                    coord
                };
                assert_eq!(
                    &after[&(name.clone(), moved.to_a1())],
                    want,
                    "{name}!{a1} under {e:?}"
                );
            }
        }
    }
}

/// The workbook after a rename: the sheet carries its new name, the planned texts replace the old.
fn renamed_session(s: &RecalcSession, from: &str, to: &str) -> RecalcSession {
    let plan = s
        .plan_sheet_rewrites(SheetChange::Rename { from, to }, AxisBudget::default())
        .unwrap();
    let mut wb = WorkbookEngine::new();
    for sheet in s.workbook().sheets() {
        wb.add_sheet(if sheet.name == from { to } else { &sheet.name });
    }
    for (si, sheet) in s.workbook().sheets().iter().enumerate() {
        let name = wb.sheets()[si].name.clone();
        for (&coord, text) in sheet.raw_cells() {
            let new = plan
                .iter()
                .find(|(i, c, _)| *i == si && *c == coord)
                .map_or(text.as_str(), |(_, _, t)| t.as_str());
            wb.set_cell(&name, &coord.to_a1(), new).unwrap();
        }
    }
    let mut next = RecalcSession::new(wb).unwrap();
    next.recalculate();
    next
}

#[test]
fn a_rename_changes_no_value_and_leaves_the_session_alone() {
    let s = session(
        &["Data", "Sum"],
        &[
            ("Data", "A1", "4"),
            ("Data", "A2", "5"),
            ("Data", "B1", "=A1*2"),
            ("Sum", "A1", "=SUM(Data!A1:A2)+Data!B1"),
            ("Sum", "A2", "=SUM(Data!A1:Data!A2)"),
            ("Sum", "A3", "=\"Data!A1\"&Data!A1"),
            ("Sum", "A4", "=Sum!A1+A2"),
        ],
    );
    let before = values(&s);
    let next = renamed_session(&s, "Data", "월별 합계");
    assert_eq!(
        raw(&s, "Sum", "A1").as_deref(),
        Some("=SUM(Data!A1:A2)+Data!B1")
    );
    assert_eq!(
        raw(&next, "Sum", "A1").as_deref(),
        Some("=SUM('월별 합계'!A1:A2)+'월별 합계'!B1")
    );
    assert_eq!(
        raw(&next, "Sum", "A3").as_deref(),
        Some("=\"Data!A1\"&'월별 합계'!A1")
    );
    assert_eq!(raw(&next, "Sum", "A4").as_deref(), Some("=Sum!A1+A2"));
    let after = values(&next);
    for ((sheet, cell), value) in &before {
        let sheet = if sheet == "Data" {
            "월별 합계"
        } else {
            sheet
        };
        assert_eq!(
            after[&(sheet.to_string(), cell.clone())],
            *value,
            "{sheet}!{cell}"
        );
    }
}

#[test]
fn deleting_a_sheet_plans_ref_errors_and_skips_its_own_formulas() {
    let s = session(
        &["Gone", "Keep"],
        &[
            ("Gone", "A1", "=Keep!A1+Gone!A2"),
            ("Keep", "A1", "3"),
            ("Keep", "B1", "=SUM(Gone!A1:A5)+A1"),
            ("Keep", "C1", "=A1"),
        ],
    );
    let plan = s
        .plan_sheet_rewrites(SheetChange::Delete { name: "Gone" }, AxisBudget::default())
        .unwrap();
    assert_eq!(plan.len(), 1);
    let (si, coord, text) = &plan[0];
    assert_eq!(
        (s.sheet_name(*si), coord.to_a1(), text.as_str()),
        (Some("Keep"), "B1".to_string(), "=SUM(#REF!)+A1")
    );
}

#[test]
fn a_plan_over_budget_is_refused() {
    let mut many = session(&["S", "T"], &[]);
    for r in 1..=3_000 {
        many.set_cell("S", at(&format!("A{r}")), &format!("=T!B{r}"))
            .unwrap();
    }
    let err = many.plan_sheet_rewrites(
        SheetChange::Rename { from: "T", to: "U" },
        AxisBudget::within(Duration::ZERO),
    );
    assert!(matches!(err, Err(RecalcError::Invalid(_))));
    let few = many.plan_sheet_rewrites(
        SheetChange::Rename { from: "T", to: "U" },
        AxisBudget {
            time: Duration::from_secs(5),
            max_rewrites: 10,
        },
    );
    assert!(matches!(few, Err(RecalcError::Invalid(_))));
}

#[test]
fn a_row_edit_moves_the_cells_a_protected_sheet_lets_an_array_cover() {
    use crate::SheetLock;
    let mut s = session(&["S"], &[("S", "A1", "=SEQUENCE(2)")]);
    // Protected: the anchor and the cell under it may be changed.
    s.set_sheet_lock(
        "S",
        Some(SheetLock {
            unlocked_cells: vec![(0, 0, 0), (1, 0, 0)],
            ..SheetLock::default()
        }),
    )
    .unwrap();
    s.recalculate();
    assert_eq!(s.cell_value("S", at("A2")).to_display_string(), "2");
    // The new row 2 has no unlocked mark: the array may not cover it any more.
    edit_ok(&mut s, "S", row_edit(EditKind::Insert, 2, 1));
    assert_eq!(s.cell_value("S", at("A1")).to_display_string(), "#SPILL!");
}

#[test]
fn the_lock_of_columns_follows_a_column_edit_and_a_lock_of_another_sheet_stays() {
    use crate::SheetLock;
    let mut s = session(&["S", "T"], &[]);
    let lock = SheetLock {
        unlocked_cells: vec![(0, 1, 3), (2, 0, 0)],
        unlocked_cols: vec![(1, 1), (3, 5)],
        unlocked_rows: vec![(0, 1)],
        ..SheetLock::default()
    };
    let col_insert = AxisEdit::new(Axis::Col, EditKind::Insert, 2, 2).unwrap();
    let moved = lock.shifted(&col_insert);
    assert_eq!(moved.unlocked_cells, vec![(0, 1, 1), (0, 4, 5), (2, 0, 0)]);
    assert_eq!(moved.unlocked_cols, vec![(1, 1), (5, 7)]);
    assert_eq!(moved.unlocked_rows, vec![(0, 1)]);
    let row_delete = row_edit(EditKind::Delete, 1, 1);
    let moved = lock.shifted(&row_delete);
    assert_eq!(moved.unlocked_cells, vec![(1, 0, 0)]);
    assert_eq!(moved.unlocked_rows, vec![(0, 0)]);
    assert_eq!(moved.unlocked_cols, lock.unlocked_cols);
    s.set_sheet_lock("T", Some(lock.clone())).unwrap();
    edit_ok(&mut s, "S", row_edit(EditKind::Insert, 1, 1));
    assert_eq!(
        s.workbook().sheet("T").unwrap().spill_lock.as_deref(),
        Some(&lock)
    );
}

#[test]
fn data_validations_survive_an_edit_and_follow_the_cells() {
    use crate::rules::Verdict;
    use easyoffice_formats::{CellBox, Cmp, DataValidation, DvKind, SheetRules};
    let mut s = session(&["S"], &[("S", "A1", "1")]);
    let mut dv = DataValidation::new(
        vec![CellBox {
            row0: 4,
            col0: 0,
            row1: 4,
            col1: 0,
        }],
        DvKind::Whole,
    );
    dv.cmp = Cmp::Between;
    dv.a = "1".to_string();
    dv.b = "10".to_string();
    s.set_rules(
        "S",
        SheetRules {
            dv: vec![dv],
            ..SheetRules::default()
        },
    )
    .unwrap();
    assert!(matches!(
        s.validate("S", at("A5"), "99").unwrap(),
        Verdict::Invalid { .. }
    ));
    edit_ok(&mut s, "S", row_edit(EditKind::Insert, 1, 2));
    assert!(matches!(
        s.validate("S", at("A7"), "99").unwrap(),
        Verdict::Invalid { .. }
    ));
    assert_eq!(s.validate("S", at("A7"), "5").unwrap(), Verdict::Valid);
    assert_eq!(s.validate("S", at("A5"), "99").unwrap(), Verdict::Valid);
}
