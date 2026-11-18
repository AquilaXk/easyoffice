//! PD-244: Structured IPC round-trip and Fail-Closed validation for sheet per-cell font families
//! (`sheets[].fonts`, `cell.style.fn`) and standard `[MS-XLSX]` table slicer parts.

use std::time::Instant;

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{base64_codec, ZipArchiveCodec, ZipSecurityPolicy};

fn json_str_field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

#[test]
fn structured_sheet_fonts_and_slicers_round_trip_through_ipc_xlsx_and_ods() {
    let save_req = r##"{
        "targetFormat": "xlsx",
        "title": "분기별 매출",
        "sheets": [{
            "name": "요약",
            "fonts": ["Malgun Gothic", "D2Coding"],
            "cells": [
                {"ref": "A1", "raw": "부서", "style": {"bold": true, "fn": 1}},
                {"ref": "B1", "raw": "코드", "style": {"bold": true, "fn": 1}},
                {"ref": "A2", "raw": "플랫폼", "style": {"fn": 1}},
                {"ref": "B2", "raw": "PLT-01", "style": {"fn": 2, "sizePt": 10.5}},
                {"ref": "C2", "raw": "", "style": {"fn": 2}}
            ],
            "tables": [{
                "name": "DeptTable",
                "range": "A1:B2",
                "header": true,
                "totals": false,
                "columns": ["부서", "코드"],
                "slicers": [{
                    "col": 0,
                    "caption": "부서 선택",
                    "left": 260,
                    "top": 24,
                    "width": 188,
                    "height": 212,
                    "multi": true
                }]
            }]
        }]
    }"##;

    let saved = EasyOfficeIpcRouter::dispatch("convert_document_format", save_req)
        .expect("xlsx structured save");
    let b64 = json_str_field(&saved, "convertedBase64");
    let xlsx_bytes = base64_codec::decode(b64).unwrap();

    // Confirm standard `[MS-XLSX]` parts exist in the IPC artifact
    let zip_entries = ZipArchiveCodec::unpack(&xlsx_bytes, &ZipSecurityPolicy::default()).unwrap();
    assert!(zip_entries
        .iter()
        .any(|e| e.path == "xl/slicerCaches/slicerCache1.xml"));
    assert!(zip_entries
        .iter()
        .any(|e| e.path == "xl/slicers/slicer1.xml"));

    // Decode back via IPC
    let decode_req =
        format!(r#"{{"action":"decode","fileName":"report.xlsx","sourceBase64":"{b64}"}}"#);
    let decoded =
        EasyOfficeIpcRouter::dispatch("convert_document_format", &decode_req).expect("xlsx decode");
    assert!(
        decoded.contains("\"fonts\":[\"Malgun Gothic\",\"D2Coding\"]"),
        "{decoded}"
    );
    assert!(
        decoded.contains("\"ref\":\"B2\",\"raw\":\"PLT-01\",\"style\":{\"sizePt\":10.5,\"fn\":2}"),
        "{decoded}"
    );
    assert!(
        decoded.contains("\"ref\":\"C2\",\"raw\":\"\",\"style\":{\"fn\":2}"),
        "{decoded}"
    );
    assert!(decoded.contains("\"caption\":\"부서 선택\""), "{decoded}");

    // Also round-trip per-cell fonts through ODS
    let ods_req = save_req.replace("\"targetFormat\": \"xlsx\"", "\"targetFormat\": \"ods\"");
    let ods_saved = EasyOfficeIpcRouter::dispatch("convert_document_format", &ods_req)
        .expect("ods structured save");
    let ods_b64 = json_str_field(&ods_saved, "convertedBase64");
    let ods_decoded = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(r#"{{"action":"decode","fileName":"report.ods","sourceBase64":"{ods_b64}"}}"#),
    )
    .expect("ods decode");
    assert!(
        ods_decoded.contains("\"fonts\":[\"Malgun Gothic\",\"D2Coding\"]"),
        "{ods_decoded}"
    );
    assert!(ods_decoded.contains("\"fn\":2"), "{ods_decoded}");
}

#[test]
fn structured_save_rejects_invalid_or_out_of_range_sheet_fonts() {
    let bad_fn = r#"{
        "targetFormat": "xlsx",
        "sheets": [{
            "name": "S1",
            "fonts": ["Malgun Gothic"],
            "cells": [{"ref": "A1", "raw": "x", "style": {"fn": 2}}]
        }]
    }"#;
    assert!(EasyOfficeIpcRouter::dispatch("convert_document_format", bad_fn).is_err());

    let bad_face = r#"{
        "targetFormat": "xlsx",
        "sheets": [{
            "name": "S1",
            "fonts": ["Bad;Font{}"],
            "cells": [{"ref": "A1", "raw": "x", "style": {"fn": 1}}]
        }]
    }"#;
    assert!(EasyOfficeIpcRouter::dispatch("convert_document_format", bad_face).is_err());
}

#[test]
#[ignore = "benchmark: cargo test --release -p easyoffice-app --test sheet_slicers_font_ipc_pd244 -- --ignored --nocapture"]
fn bench_pd244_sheet_slicers_font_release() {
    let mut cells_json = String::from(
        r#"{"ref":"A1","raw":"지역","style":{"bold":true,"fn":1}},{"ref":"B1","raw":"수치","style":{"bold":true,"fn":1}}"#,
    );
    for r in 2..=500 {
        let region = if r % 2 == 0 { "서울" } else { "부산" };
        let fn_id = if r % 3 == 0 { 2 } else { 1 };
        cells_json.push_str(&format!(
            r#", {0}"ref":"A{r}","raw":"{region}","style":{0}"fn":{fn_id}{1}{1}, {0}"ref":"B{r}","raw":"{r}0","style":{0}"fn":2{1}{1}"#,
            "{", "}"
        ));
    }
    let req = format!(
        r##"{{
            "targetFormat": "xlsx",
            "title": "BenchPD244",
            "sheets": [{{
                "name": "Data",
                "fonts": ["Malgun Gothic", "JetBrains Mono"],
                "cells": [{cells_json}],
                "tables": [{{
                    "name": "BenchTable",
                    "range": "A1:B500",
                    "header": true,
                    "totals": false,
                    "columns": ["지역", "수치"],
                    "slicers": [{{
                        "col": 0,
                        "caption": "지역",
                        "left": 240,
                        "top": 24,
                        "width": 184,
                        "height": 210,
                        "multi": false
                    }}]
                }}]
            }}]
        }}"##
    );

    let iters = 25u32;
    let t0 = Instant::now();
    let mut artifact_len = 0usize;
    for _ in 0..iters {
        let saved = EasyOfficeIpcRouter::dispatch("convert_document_format", &req).unwrap();
        let b64 = json_str_field(&saved, "convertedBase64");
        artifact_len = base64_codec::decode(b64).unwrap().len();
        let dec = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &format!(r#"{{"action":"decode","fileName":"bench.xlsx","sourceBase64":"{b64}"}}"#),
        )
        .unwrap();
        assert!(dec.contains("JetBrains Mono"));
    }
    let avg_us = t0.elapsed().as_secs_f64() * 1_000_000.0 / f64::from(iters);
    eprintln!(
        "PD-244 release bench (1,000 font-styled cells + native slicer cache/view XLSX save+decode): avg {avg_us:.1} µs ({artifact_len} B)"
    );
}
