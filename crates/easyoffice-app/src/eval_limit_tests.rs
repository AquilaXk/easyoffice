//! The time limit of an evaluation request (PD-215): the session answers `timedOut` and stays
//! usable, the stateless answer says so, and a save is refused instead of writing `#VALUE!`.

use crate::eval_limit::LimitGuard;
use crate::sheet_session::{Applied, Limits, Registry, LIMITS};
use crate::EasyOfficeIpcRouter;
use easyoffice_formats::{EasyOfficeError, ZipArchiveCodec, ZipSecurityPolicy};
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
    assert!(
        first.contains(",\"timedOut\":true,\"pending\":30"),
        "{first}"
    );
    assert!(!first.contains("#VALUE!"), "{first}");
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
    assert!(res.contains("\"value\":null,"), "{res}");
    assert!(res.contains("\"targetValues\":[null,\"7\"]"), "{res}");
    assert!(res.contains("\"values\":{\"Sheet1!B1\":\"7\"}"), "{res}");
    assert!(!res.contains("#VALUE!"), "{res}");
    assert!(res.ends_with(",\"timedOut\":true}"), "{res}");
    let one = EasyOfficeIpcRouter::dispatch(
        "evaluate_workbook_formula",
        r#"{"sheet":"Sheet1","expr":"=SUM(C1:C3)","cells":[{"cell":"C1","value":"2"}]}"#,
    )
    .unwrap();
    assert!(
        one.contains("\"timedOut\":true") && one.contains("\"value\":null"),
        "{one}"
    );
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

fn converted(dst: &str, payload: &str) -> Vec<u8> {
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", payload).unwrap();
    let marker = "\"convertedBase64\":\"";
    let start = json.find(marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    let _ = dst;
    EasyOfficeIpcRouter::decode_base64(&json[start..end]).unwrap()
}

fn part(package: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(package, &ZipSecurityPolicy::default()).unwrap();
    let entry = entries.iter().find(|e| e.path == path).expect(path);
    String::from_utf8(entry.data.clone()).unwrap()
}

#[test]
fn an_xlsx_saved_out_of_time_keeps_its_formulas_without_values_and_recalculates_on_open() {
    let _limit = LimitGuard::set(Duration::ZERO);
    let xlsx = converted("xlsx", &workbook_payload("xlsx"));
    let sheet = part(&xlsx, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("<f>A1*2</f></c>"), "{sheet}");
    assert!(!sheet.contains("<v>8</v>"), "{sheet}");
    assert!(part(&xlsx, "xl/workbook.xml").contains("fullCalcOnLoad=\"1\""));
    let body = r#"{"sourceFormat":"xlsx","targetFormat":"xlsx","title":"t","body":"[[SHEET_A1]]\tA1\t10\n[[SHEET_A1]]\tA2\t=A1+1"}"#;
    let legacy = converted("xlsx", body);
    assert!(part(&legacy, "xl/worksheets/sheet1.xml").contains("<f>A1+1</f></c>"));
}

#[test]
fn an_ods_saved_out_of_time_keeps_its_formulas_without_a_value() {
    let _limit = LimitGuard::set(Duration::ZERO);
    let ods = converted("ods", &workbook_payload("ods"));
    let content = part(&ods, "content.xml");
    assert!(
        content.contains("table:formula=\"of:=[.A1]*2\"/>"),
        "{content}"
    );
    assert!(!content.contains("<text:p>8</text:p>"), "{content}");
}

#[test]
fn a_save_that_needs_the_values_is_refused_when_it_runs_out_of_time() {
    for dst in ["xls", "csv", "pdf", "docx"] {
        let _limit = LimitGuard::set(Duration::ZERO);
        let err = EasyOfficeIpcRouter::dispatch("convert_document_format", &workbook_payload(dst))
            .unwrap_err();
        assert!(
            matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)),
            "{dst}: {err:?}"
        );
    }
}

#[test]
fn a_save_with_time_to_spare_still_writes_the_values() {
    for dst in ["xlsx", "xls", "csv"] {
        let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &workbook_payload(dst))
            .unwrap();
        assert!(json.contains("convertedBase64"), "{dst}: {json}");
    }
}
