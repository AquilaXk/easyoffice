//! PD-308: IPC encode/decode parity for XLS, ODS, and XLSX default font, default col width,
//! default row height, explicit columns, and row heights.

use easyoffice_app::EasyOfficeIpcRouter;

fn str_field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

#[test]
fn xls_and_ods_round_trip_sheet_defaults_and_layout_via_ipc() {
    let payload = r#"{
        "sourceFormat":"xlsx",
        "targetFormat":"xls",
        "title":"layout",
        "defaultFont":{"name":"Calibri","size":11},
        "sheets":[{
            "name":"S1",
            "cells":[{"ref":"A1","raw":"x"}],
            "defaultColWidth":{"width":13.5,"base":10},
            "defaultRowHeight":[22.5],
            "columns":[{"index":1,"widthPx":140}],
            "rowHeights":[[1,30],[3,25]],
            "hiddenRows":[[2,3]]
        }]
    }"#;
    let xls_res = EasyOfficeIpcRouter::dispatch("convert_document_format", payload).unwrap();
    let xls_b64 = str_field(&xls_res, "convertedBase64");
    let dec_xls = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(r#"{{"action":"decode","fileName":"out.xls","sourceBase64":"{xls_b64}"}}"#),
    )
    .unwrap();
    assert!(
        dec_xls.contains(r#""defaultFont":{"name":"Calibri","size":11}"#),
        "{dec_xls}"
    );
    assert!(
        dec_xls.contains(r#""defaultColWidth":{"width":13.5,"base":10}"#),
        "{dec_xls}"
    );
    assert!(
        dec_xls.contains(r#""defaultRowHeight":[22.5]"#),
        "{dec_xls}"
    );
    assert!(
        dec_xls.contains(r#""columns":[{"index":1,"widthPx":140}]"#),
        "{dec_xls}"
    );
    assert!(
        dec_xls.contains(r#""rowHeights":[[1,30],[3,25]]"#),
        "{dec_xls}"
    );
    assert!(dec_xls.contains(r#""hiddenRows":[[2,3]]"#), "{dec_xls}");

    // Same payload round-trips cleanly through ODS too.
    let ods_payload = payload.replace(r#""targetFormat":"xls""#, r#""targetFormat":"ods""#);
    let ods_res = EasyOfficeIpcRouter::dispatch("convert_document_format", &ods_payload).unwrap();
    let ods_b64 = str_field(&ods_res, "convertedBase64");
    let dec_ods = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(r#"{{"action":"decode","fileName":"out.ods","sourceBase64":"{ods_b64}"}}"#),
    )
    .unwrap();
    assert!(
        dec_ods.contains(r#""defaultFont":{"name":"Calibri","size":11}"#),
        "{dec_ods}"
    );
    assert!(
        dec_ods.contains(r#""defaultColWidth":{"width":13.5,"base":10}"#),
        "{dec_ods}"
    );
    assert!(
        dec_ods.contains(r#""columns":[{"index":1,"widthPx":140}]"#),
        "{dec_ods}"
    );
}

#[test]
fn ods_ipc_round_trips_automatic_font_default_row_height_with_zero_flag() {
    let payload = r#"{
        "sourceFormat":"xlsx",
        "targetFormat":"ods",
        "title":"auto_row_height",
        "defaultFont":{"name":"Calibri","size":11},
        "sheets":[{
            "name":"S1",
            "cells":[{"ref":"A1","raw":"x"}],
            "defaultRowHeight":[14.4,0]
        },{
            "name":"S2",
            "cells":[{"ref":"A1","raw":"y"}],
            "defaultRowHeight":[24,1]
        }]
    }"#;
    let ods_res = EasyOfficeIpcRouter::dispatch("convert_document_format", payload).unwrap();
    let ods_b64 = str_field(&ods_res, "convertedBase64");
    let dec_ods = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(r#"{{"action":"decode","fileName":"out.ods","sourceBase64":"{ods_b64}"}}"#),
    )
    .unwrap();
    assert!(
        dec_ods.contains(r#""defaultRowHeight":[14.4,0]"#),
        "{dec_ods}"
    );
    assert!(dec_ods.contains(r#""defaultRowHeight":[24]"#), "{dec_ods}");
    assert!(
        dec_ods.contains(r#""defaultFont":{"name":"Calibri","size":11}"#),
        "{dec_ods}"
    );

    for bad in ["[-1,0]", "[410,0]", "[0,0]"] {
        let bad_payload = payload.replace("[14.4,0]", bad);
        assert!(
            EasyOfficeIpcRouter::dispatch("convert_document_format", &bad_payload).is_err(),
            "{bad} must fail closed"
        );
    }
}
