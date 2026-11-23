//! End-to-end IPC round-trip and budget tests for XLSX `<rPh>` furigana and `PHONETIC` (PD-319).

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{
    base64_codec, EasyOfficeError, OoxmlCodec, StrictJsonValue, ZipArchiveCodec,
    MAX_PHONETIC_RUNS_PER_CELL, MAX_PHONETIC_TEXT_BYTES,
};

fn build_rph_xlsx() -> Vec<u8> {
    let wb = r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheets><sheet name="Sheet1" sheetId="1"/></sheets></workbook>"#;
    let sst = concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="2" uniqueCount="2">"#,
        r#"<si><t>東京都</t><rPh sb="0" eb="3"><t>トウキョウト</t></rPh><phoneticPr fontId="0"/></si>"#,
        r#"<si><t>申し込む</t><rPh sb="0" eb="1"><t>もう</t></rPh><rPh sb="2" eb="3"><t>こ</t></rPh><phoneticPr fontId="0" type="Hiragana"/></si>"#,
        r#"</sst>"#,
    );
    let sheet = concat!(
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#,
        r#"<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c><c r="C1"><f>PHONETIC(A1)</f></c><c r="D1"><f>PHONETIC(B1)</f></c></row>"#,
        r#"</sheetData></worksheet>"#,
    );
    ZipArchiveCodec::pack(&[
        ("xl/workbook.xml".to_string(), wb.as_bytes().to_vec()),
        ("xl/sharedStrings.xml".to_string(), sst.as_bytes().to_vec()),
        (
            "xl/worksheets/sheet1.xml".to_string(),
            sheet.as_bytes().to_vec(),
        ),
    ])
    .unwrap()
}

fn json_str<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

#[test]
fn convert_open_evaluate_readback_and_structured_save_roundtrip_phonetics() {
    let input_bytes = build_rph_xlsx();
    let input_b64 = base64_codec::encode(&input_bytes);

    // 1. Decode via convert_document_format -> emits `"ph":{"r":...,"p":...,"k":...}`.
    let decoded_json = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(r#"{{"action":"decode","fileName":"rph.xlsx","sourceBase64":"{input_b64}"}}"#),
    )
    .unwrap();
    let parsed = StrictJsonValue::parse(&decoded_json).unwrap();
    let sheets = StrictJsonValue::get_field(parsed.as_object().unwrap(), "sheets")
        .unwrap()
        .as_array()
        .unwrap();
    let cells = StrictJsonValue::get_field(sheets[0].as_object().unwrap(), "cells")
        .unwrap()
        .as_array()
        .unwrap();
    let a1_ph = StrictJsonValue::get_field(cells[0].as_object().unwrap(), "ph")
        .expect("A1 carries ph")
        .as_object()
        .unwrap();
    assert_eq!(StrictJsonValue::get_str(a1_ph, "r"), Some("トウキョウト"));
    assert_eq!(StrictJsonValue::get_str(a1_ph, "k"), Some("fk"));
    let b1_ph = StrictJsonValue::get_field(cells[1].as_object().unwrap(), "ph")
        .expect("B1 carries ph")
        .as_object()
        .unwrap();
    assert_eq!(StrictJsonValue::get_str(b1_ph, "r"), Some("もうしこむ"));
    assert_eq!(StrictJsonValue::get_str(b1_ph, "k"), Some("h"));

    // 2. Stateless evaluate_sheet with `"ph"` evaluates PHONETIC accurately.
    let stateless_req = r#"{
        "sheet":"Sheet1",
        "cells":[
            {"sheet":"Sheet1","cell":"A1","value":"東京都","ph":"トウキョウト"},
            {"sheet":"Sheet1","cell":"B1","value":"申し込む","ph":"もうしこむ"},
            {"sheet":"Sheet1","cell":"C1","value":"=PHONETIC(A1:B1)"},
            {"sheet":"Sheet1","cell":"D1","value":"=PHONETIC(B1)"}
        ],
        "targets":[{"sheet":"Sheet1","cell":"C1"},{"sheet":"Sheet1","cell":"D1"}]
    }"#;
    let eval_res = EasyOfficeIpcRouter::dispatch("evaluate_sheet", stateless_req).unwrap();
    assert!(eval_res.contains("トウキョウト"), "{eval_res}");
    assert!(eval_res.contains("もうしこむ"), "{eval_res}");

    // 3. Stateful session apply updates PHONETIC incrementally when text/phonetic change.
    let open = EasyOfficeIpcRouter::dispatch_for_owner(
        "evaluate_sheet",
        r#"{"sessionAction":"open","sheets":["Sheet1"]}"#,
        "w-ph-pd319",
    )
    .unwrap();
    let sid = json_str(&open, "session");
    let apply1 = EasyOfficeIpcRouter::dispatch_for_owner(
        "evaluate_sheet",
        &format!(
            r#"{{"sessionAction":"apply","session":"{sid}","changes":[{{"sheet":"Sheet1","cell":"A1","value":"東京都","ph":"トウキョウト"}},{{"sheet":"Sheet1","cell":"C1","value":"=PHONETIC(A1)"}}]}}"#
        ),
        "w-ph-pd319",
    )
    .unwrap();
    assert!(apply1.contains("トウキョウト"), "{apply1}");

    let apply2 = EasyOfficeIpcRouter::dispatch_for_owner(
        "evaluate_sheet",
        &format!(
            r#"{{"sessionAction":"apply","session":"{sid}","changes":[{{"sheet":"Sheet1","cell":"A1","value":"大阪府"}}]}}"#
        ),
        "w-ph-pd319",
    )
    .unwrap();
    assert!(apply2.contains("大阪府"), "{apply2}");

    // 4. Structured XLSX save preserves `<rPh>` and evaluates formula cache with PHONETIC.
    let save_req = r#"{
        "sourceFormat":"xlsx",
        "targetFormat":"xlsx",
        "title":"phonetic-save",
        "sheets":[{
            "name":"Sheet1",
            "cells":[
                {"ref":"A1","raw":"東京都","ph":{"r":"トウキョウト","p":[[0,3,"トウキョウト"]],"k":"fk"}},
                {"ref":"B1","raw":"=PHONETIC(A1)"}
            ]
        }]
    }"#;
    let saved = EasyOfficeIpcRouter::dispatch("convert_document_format", save_req).unwrap();
    let saved_bytes = base64_codec::decode(json_str(&saved, "convertedBase64")).unwrap();
    let (reopened_sheets, reopened_stylings) =
        OoxmlCodec::unpack_xlsx_with_styling(&saved_bytes).unwrap();
    let b1 = reopened_sheets[0]
        .1
        .iter()
        .find(|&&(r, c, _)| r == 0 && c == 1)
        .unwrap();
    assert_eq!(b1.2, "=PHONETIC(A1)");
    assert_eq!(
        reopened_stylings[0]
            .1
            .cell_phonetic(0, 0, "東京都")
            .unwrap()
            .reading(),
        "トウキョウト"
    );

    // 5. Readback inspect evaluates PHONETIC on both saved XLSX binary and structured JSON.
    let saved_b64 = base64_codec::encode(&saved_bytes);
    let readback_bin = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(r#"{{"action":"inspect","fileName":"out.xlsx","sourceBase64":"{saved_b64}"}}"#),
    )
    .unwrap();
    assert!(readback_bin.contains("トウキョウト"), "{readback_bin}");

    let inspect_struct = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        r#"{"action":"inspect","sourceFormat":"xlsx","sheets":[{
            "name":"Sheet1",
            "cells":[
                {"ref":"A1","raw":"東京都","ph":{"r":"トウキョウト","p":[[0,3,"トウキョウト"]],"k":"fk"}},
                {"ref":"B1","raw":"=PHONETIC(A1)"}
            ]
        }]}"#,
    )
    .unwrap();
    assert!(inspect_struct.contains("トウキョウト"), "{inspect_struct}");
}

#[test]
fn hostile_phonetic_payloads_are_rejected_fail_closed() {
    let over_bytes = "ア".repeat(MAX_PHONETIC_TEXT_BYTES + 1);
    let eval_too_big = format!(
        r#"{{"sheet":"Sheet1","cells":[{{"sheet":"Sheet1","cell":"A1","value":"漢","ph":"{over_bytes}"}}],"expr":"=PHONETIC(A1)"}}"#
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("evaluate_sheet", &eval_too_big),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    let many_runs: String = (0..=MAX_PHONETIC_RUNS_PER_CELL)
        .map(|i| format!("[{i}, {}, \"x\"]", i + 1))
        .collect::<Vec<_>>()
        .join(",");
    let save_too_many = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"xlsx","title":"hostile","sheets":[{{"name":"Sheet1","cells":[{{"ref":"A1","raw":"{}","ph":{{"r":"x","p":[{many_runs}]}}}}]}}]}}"#,
        "漢".repeat(MAX_PHONETIC_RUNS_PER_CELL + 2)
    );
    assert!(matches!(
        EasyOfficeIpcRouter::dispatch("convert_document_format", &save_too_many),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}
