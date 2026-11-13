//! The time limit of an evaluation request (PD-215): the session answers `timedOut` and stays
//! usable, the stateless answer says so, and a save is refused instead of writing `#VALUE!`.

use crate::eval_limit::LimitGuard;
use crate::sheet_session::{Applied, Limits, Registry, LIMITS};
use crate::EasyOfficeIpcRouter;
use easyoffice_formats::EasyOfficeError;
use easyoffice_sheet::{CellChange, PackedCellCoordinate};
use std::time::{Duration, Instant};

/// A block of numbers and `heavy` formula cells that each read it many times.
fn heavy_changes(heavy: u32) -> Vec<CellChange> {
    let mut changes = Vec::new();
    for r in 1..=40u32 {
        for c in 0..10u32 {
            changes.push(CellChange {
                sheet: "S".into(),
                coord: PackedCellCoordinate::new(r - 1, c).unwrap(),
                raw: (r + c).to_string(),
            });
        }
    }
    let formula = format!("={}0", "SUM(A1:J40)+".repeat(200));
    for i in 0..heavy {
        changes.push(CellChange {
            sheet: "S".into(),
            coord: PackedCellCoordinate::new(i, 11).unwrap(),
            raw: formula.clone(),
        });
    }
    changes
}

#[test]
fn a_session_that_runs_out_of_time_answers_and_takes_the_next_edit() {
    let reg = Registry::new(Limits {
        max_recalc: Duration::ZERO,
        ..LIMITS
    });
    let now = Instant::now();
    let id = reg.open(&["S".to_string()], "w", now).unwrap();
    let Applied::Done(first) = reg.apply(&id, "w", &heavy_changes(30), true, now).unwrap() else {
        panic!("the session was refused");
    };
    assert!(first.contains(",\"timedOut\":true"), "{first}");
    assert!(first.contains("\"value\":\"#VALUE!\""), "{first}");
    assert!(!first.contains("\"refused\""));
    let edit = [CellChange {
        sheet: "S".into(),
        coord: PackedCellCoordinate::new(0, 0).unwrap(),
        raw: "5".into(),
    }];
    let Applied::Done(second) = reg.apply(&id, "w", &edit, true, now).unwrap() else {
        panic!("the session was dropped");
    };
    assert!(second.starts_with("{\"session\":"), "{second}");
    assert_eq!(reg.len(), 1);
}

#[test]
fn a_session_with_time_to_spare_does_not_say_timed_out() {
    let reg = Registry::new(LIMITS);
    let now = Instant::now();
    let id = reg.open(&["S".to_string()], "w", now).unwrap();
    let Applied::Done(answer) = reg.apply(&id, "w", &heavy_changes(3), true, now).unwrap() else {
        panic!("the session was refused");
    };
    assert!(!answer.contains("timedOut"), "{answer}");
    assert!(answer.contains("\"recomputed\":3"), "{answer}");
}

const STATELESS: &str = r#"{"sheet":"Sheet1","targets":[{"cell":"A1"},{"cell":"B1"}],"cells":[{"cell":"A1","value":"=SUM(C1:C3)"},{"cell":"B1","value":"7"},{"cell":"C1","value":"2"},{"cell":"C2","value":"3"}]}"#;

#[test]
fn the_stateless_answer_says_it_ran_out_of_time() {
    let _limit = LimitGuard::set(Duration::ZERO);
    let res = EasyOfficeIpcRouter::dispatch("evaluate_sheet", STATELESS).unwrap();
    assert!(
        res.contains("\"targetValues\":[\"#VALUE!\",\"7\"]"),
        "{res}"
    );
    assert!(res.ends_with(",\"timedOut\":true}"), "{res}");
    let one = EasyOfficeIpcRouter::dispatch(
        "evaluate_workbook_formula",
        r#"{"sheet":"Sheet1","expr":"=SUM(C1:C3)","cells":[{"cell":"C1","value":"2"}]}"#,
    )
    .unwrap();
    assert!(one.contains("\"timedOut\":true"), "{one}");
}

#[test]
fn the_stateless_answer_is_unchanged_with_time_to_spare() {
    let res = EasyOfficeIpcRouter::dispatch("evaluate_sheet", STATELESS).unwrap();
    assert!(res.contains("\"targetValues\":[\"5\",\"7\"]"), "{res}");
    assert!(!res.contains("timedOut"), "{res}");
}

fn workbook_payload(dst: &str) -> String {
    format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"{dst}","title":"t","sheets":[{{"name":"S0","cells":[{{"ref":"A1","raw":"4"}},{{"ref":"A2","raw":"=A1*2"}}]}},{{"name":"S1","cells":[{{"ref":"A1","raw":"=S0!A2+1"}}]}}]}}"#
    )
}

#[test]
fn a_save_that_runs_out_of_time_is_refused_in_every_format() {
    for dst in ["xlsx", "ods", "xls", "csv", "pdf", "docx"] {
        let _limit = LimitGuard::set(Duration::ZERO);
        let err = EasyOfficeIpcRouter::dispatch("convert_document_format", &workbook_payload(dst))
            .unwrap_err();
        assert!(
            matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)),
            "{dst}: {err:?}"
        );
    }
    let body = r#"{"sourceFormat":"xlsx","targetFormat":"xlsx","title":"t","body":"[[SHEET_A1]]\tA1\t10\n[[SHEET_A1]]\tA2\t=A1+1"}"#;
    let _limit = LimitGuard::set(Duration::ZERO);
    assert!(EasyOfficeIpcRouter::dispatch("convert_document_format", body).is_err());
}

#[test]
fn a_save_with_time_to_spare_still_writes_the_values() {
    for dst in ["xlsx", "xls", "csv"] {
        let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &workbook_payload(dst))
            .unwrap();
        assert!(json.contains("convertedBase64"), "{dst}: {json}");
    }
}
