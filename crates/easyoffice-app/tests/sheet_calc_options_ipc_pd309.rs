//! PD-309: IPC encode/decode parity, manual `calcOnSave=false` cache skip, `evaluate_sheet`
//! `INFO("RECALC")` + iterative circular evaluation, and fail-closed `calcOptions` rejection.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{base64_codec, StrictJsonValue, ZipArchiveCodec, ZipSecurityPolicy};

fn str_field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

fn zip_part(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let entry = entries.into_iter().find(|e| e.path == path).unwrap();
    String::from_utf8(entry.data).unwrap()
}

#[test]
fn convert_round_trips_calc_options_and_honors_calc_on_save_flag() {
    let payload = r#"{
        "sourceFormat":"xlsx",
        "targetFormat":"xlsx",
        "title":"calc",
        "calcOptions":{"mode":"manual","calcOnSave":false,"fullCalcOnLoad":false,"iterate":true,"iterateCount":50,"iterateDelta":0.0001},
        "sheets":[{
            "name":"Sheet1",
            "cells":[{"ref":"A1","raw":"10"},{"ref":"B1","raw":"=A1*2"}]
        }]
    }"#;

    for target in ["xlsx", "ods", "xls"] {
        let req = payload.replacen(
            r#""targetFormat":"xlsx""#,
            &format!(r#""targetFormat":"{target}""#),
            1,
        );
        let res = EasyOfficeIpcRouter::dispatch("convert_document_format", &req).unwrap();
        let b64 = str_field(&res, "convertedBase64");
        let bytes = base64_codec::decode(b64).unwrap();
        if target == "xlsx" {
            let s1 = zip_part(&bytes, "xl/worksheets/sheet1.xml");
            // Manual + calcOnSave=false skips evaluating cached `<v>` on formulas.
            assert!(!s1.contains("<v>20</v>"), "{s1}");
        } else if target == "ods" {
            let content = zip_part(&bytes, "content.xml");
            assert!(!content.contains(r#"office:value="20""#), "{content}");
        }
        let decoded = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &format!(r#"{{"action":"decode","fileName":"out.{target}","sourceBase64":"{b64}"}}"#),
        )
        .unwrap();
        assert!(
            decoded.contains(
                r#""calcOptions":{"mode":"manual","calcOnSave":false,"fullCalcOnLoad":false,"iterate":true,"iterateCount":50,"iterateDelta":0.0001}"#
            ),
            "{target}: {decoded}"
        );
    }

    // Auto mode writes evaluated `<v>20</v>` in XLSX.
    let auto_payload = payload.replace(
        r#"{"mode":"manual","calcOnSave":false,"fullCalcOnLoad":false,"iterate":true,"iterateCount":50,"iterateDelta":0.0001}"#,
        r#"{"mode":"auto","calcOnSave":true}"#,
    );
    let auto_res = EasyOfficeIpcRouter::dispatch("convert_document_format", &auto_payload).unwrap();
    let auto_bytes = base64_codec::decode(str_field(&auto_res, "convertedBase64")).unwrap();
    let auto_sheet = zip_part(&auto_bytes, "xl/worksheets/sheet1.xml");
    assert!(auto_sheet.contains("<v>20</v>"), "{auto_sheet}");
}

#[test]
fn evaluate_sheet_and_session_honor_calc_options_and_reject_hostile_payloads() {
    // Stateless evaluate_sheet with manual mode + iterative circular convergence.
    let eval_req = r#"{
        "sheet":"Sheet1",
        "calcOptions":{"mode":"manual","iterate":true,"iterateCount":100,"iterateDelta":0.0001},
        "cells":[
            {"sheet":"Sheet1","cell":"A1","value":"=B1+1"},
            {"sheet":"Sheet1","cell":"B1","value":"=A1/2"},
            {"sheet":"Sheet1","cell":"C1","value":"=INFO(\"RECALC\")"}
        ],
        "targets":[
            {"sheet":"Sheet1","cell":"A1"},
            {"sheet":"Sheet1","cell":"B1"},
            {"sheet":"Sheet1","cell":"C1"}
        ]
    }"#;
    let res = EasyOfficeIpcRouter::dispatch("evaluate_sheet", eval_req).unwrap();
    let parsed = StrictJsonValue::parse(&res).unwrap();
    let root = parsed.as_object().unwrap();
    let vals = StrictJsonValue::get_field(root, "values")
        .unwrap()
        .as_object()
        .unwrap();
    let a1: f64 = StrictJsonValue::get_str(vals, "Sheet1!A1")
        .unwrap()
        .parse()
        .unwrap();
    let b1: f64 = StrictJsonValue::get_str(vals, "Sheet1!B1")
        .unwrap()
        .parse()
        .unwrap();
    assert!((a1 - 2.0).abs() < 1e-3, "{res}");
    assert!((b1 - 1.0).abs() < 1e-3, "{res}");
    assert_eq!(StrictJsonValue::get_str(vals, "Sheet1!C1"), Some("Manual"));
    assert!(res.contains(r#""circular":false"#), "{res}");

    // Stateful session apply honors calcOptions too.
    let open = EasyOfficeIpcRouter::dispatch_for_owner(
        "evaluate_sheet",
        r#"{"sessionAction":"open","sheets":["Sheet1"]}"#,
        "w-calc-pd309",
    )
    .unwrap();
    let sid = str_field(&open, "session");
    let apply = EasyOfficeIpcRouter::dispatch_for_owner(
        "evaluate_sheet",
        &format!(
            r#"{{"sessionAction":"apply","session":"{sid}","calcOptions":{{"mode":"manual","iterate":true,"iterateCount":100,"iterateDelta":0.0001}},"changes":[{{"sheet":"Sheet1","cell":"A1","value":"=B1+1"}},{{"sheet":"Sheet1","cell":"B1","value":"=A1/2"}},{{"sheet":"Sheet1","cell":"C1","value":"=INFO(\"RECALC\")"}}]}}"#
        ),
        "w-calc-pd309",
    )
    .unwrap();
    assert!(apply.contains(r#""circular":false"#), "{apply}");
    assert!(apply.contains("Manual"), "{apply}");

    // Hostile calcOptions payloads are rejected fail-closed.
    for bad in [
        r#"{"iterateCount":0}"#,
        r#"{"iterateCount":40000}"#,
        r#"{"iterateDelta":-1}"#,
        r#"{"mode":"bogus"}"#,
        r#"{"extra":1}"#,
    ] {
        let bad_eval = format!(r#"{{"sheet":"Sheet1","calcOptions":{bad},"expr":"1+1"}}"#);
        assert!(
            EasyOfficeIpcRouter::dispatch("evaluate_sheet", &bad_eval).is_err(),
            "{bad}"
        );
        let bad_save = format!(
            r#"{{"sourceFormat":"xlsx","targetFormat":"xlsx","title":"x","calcOptions":{bad},"sheets":[{{"name":"Sheet1","cells":[]}}]}}"#
        );
        assert!(
            EasyOfficeIpcRouter::dispatch("convert_document_format", &bad_save).is_err(),
            "{bad}"
        );
    }
}
