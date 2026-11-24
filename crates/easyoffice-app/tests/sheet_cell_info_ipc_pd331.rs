//! App IPC, stateless evaluation, session `cellMeta`, Fail-Closed payload validation, and export
//! formula-cache tests for `CELL` and `INFO` metadata (`PD-331`).

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::EasyOfficeError;

fn eval_sheet(payload: &str) -> Result<String, EasyOfficeError> {
    EasyOfficeIpcRouter::dispatch("evaluate_sheet", payload)
}

fn convert_doc(payload: &str) -> Vec<u8> {
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", payload).unwrap();
    let marker = "\"convertedBase64\":\"";
    let start = json.find(marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    EasyOfficeIpcRouter::decode_base64(&json[start..end]).unwrap()
}

fn json_str_field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

fn changed_value(answer: &str, cell: &str) -> Option<String> {
    let marker = format!("\"cell\":\"{cell}\",\"value\":\"");
    let start = answer.find(&marker)? + marker.len();
    let end = answer[start..].find('"')? + start;
    Some(answer[start..end].to_string())
}

#[test]
fn stateless_evaluate_sheet_applies_cell_meta_and_file_name() {
    let payload = r##"{
        "sheet":"Sheet1",
        "sheets":["Sheet1","Sheet2"],
        "fileName":"/Users/hidden/Budget.xlsx",
        "cellMeta":{
            "Sheet1":{
                "numFmts":["#,##0;[Red]-#,##0"],
                "cells":{"A1":{"nf":1,"ha":"c"}},
                "cols":[[1,165]],
                "lock":{"unlocked":[[1,1,1]],"locked":[],"rows":[],"cols":[]},
                "origin":[5,3]
            }
        },
        "cells":[
            {"sheet":"Sheet1","cell":"A1","value":"text"},
            {"sheet":"Sheet1","cell":"B1","value":"=CELL(\"format\",A1)"},
            {"sheet":"Sheet1","cell":"B2","value":"=CELL(\"color\",A1)"},
            {"sheet":"Sheet1","cell":"B3","value":"=CELL(\"prefix\",A1)"},
            {"sheet":"Sheet1","cell":"B4","value":"=CELL(\"protect\",A1)"},
            {"sheet":"Sheet1","cell":"B5","value":"=@CELL(\"width\",A1)"},
            {"sheet":"Sheet1","cell":"B6","value":"=CELL(\"filename\")"},
            {"sheet":"Sheet1","cell":"B7","value":"=CELL(\"address\",Sheet2!D4)"},
            {"sheet":"Sheet1","cell":"B8","value":"=INFO(\"origin\")"},
            {"sheet":"Sheet1","cell":"B9","value":"=INFO(\"directory\")"}
        ],
        "targets":[
            {"sheet":"Sheet1","cell":"B1"},
            {"sheet":"Sheet1","cell":"B2"},
            {"sheet":"Sheet1","cell":"B3"},
            {"sheet":"Sheet1","cell":"B4"},
            {"sheet":"Sheet1","cell":"B5"},
            {"sheet":"Sheet1","cell":"B6"},
            {"sheet":"Sheet1","cell":"B7"},
            {"sheet":"Sheet1","cell":"B8"},
            {"sheet":"Sheet1","cell":"B9"}
        ]
    }"##;

    let res = eval_sheet(payload).unwrap();
    assert!(
        res.contains(r#""targetValues":[",0-","1","^","0","20","[Budget.xlsx]Sheet1","[Budget.xlsx]Sheet2!$D$4","$A:$C$5",""]"#),
        "unexpected stateless targets response: {res}"
    );
    // Ensure directory segment from UI/fileName never leaked
    assert!(!res.contains("hidden"), "{res}");
}

#[test]
fn session_cell_meta_action_updates_formats_widths_spills_and_filenames() {
    let opened = eval_sheet(r#"{"sessionAction":"open","sheets":["Sheet1"]}"#).unwrap();
    let id = json_str_field(&opened, "session").to_string();

    let applied = eval_sheet(&format!(
        r#"{{"sessionAction":"apply","session":"{id}","recalc":true,"changes":[
            {{"sheet":"Sheet1","cell":"A1","value":"hi"}},
            {{"sheet":"Sheet1","cell":"B1","value":"=CELL(\"format\",A1)"}},
            {{"sheet":"Sheet1","cell":"B2","value":"=CELL(\"width\",A1)"}},
            {{"sheet":"Sheet1","cell":"B3","value":"=CELL(\"filename\")"}}
        ]}}"#
    ))
    .unwrap();
    assert_eq!(changed_value(&applied, "B1").as_deref(), Some("G"));
    assert_eq!(changed_value(&applied, "B2").as_deref(), Some("8"));
    assert_eq!(changed_value(&applied, "C2").as_deref(), Some("TRUE"));
    assert_eq!(changed_value(&applied, "B3").as_deref(), Some(""));

    let updated = eval_sheet(&format!(
        r#"{{"sessionAction":"cellMeta","session":"{id}","fileName":"Report.xlsx","cellMeta":{{"Sheet1":{{"numFmts":["0.00%"],"cells":{{"A1":{{"nf":1}}}},"cols":[[1,165]]}}}}}}"#
    ))
    .unwrap();
    assert_eq!(changed_value(&updated, "B1").as_deref(), Some("P2"));
    assert_eq!(changed_value(&updated, "B2").as_deref(), Some("20"));
    assert_eq!(changed_value(&updated, "C2").as_deref(), Some("FALSE"));
    assert_eq!(
        changed_value(&updated, "B3").as_deref(),
        Some("[Report.xlsx]Sheet1")
    );

    eval_sheet(&format!(r#"{{"sessionAction":"close","session":"{id}"}}"#)).unwrap();
}

#[test]
fn cell_meta_payload_fails_closed_on_malformed_or_oversized_inputs() {
    let cases = [
        // NUL byte in fileName
        r#"{"sheet":"S","sheets":["S"],"fileName":"bad\u0000name.xlsx","cells":[]}"#,
        // Invalid horizontal alignment tag
        r#"{"sheet":"S","sheets":["S"],"cellMeta":{"S":{"cells":{"A1":{"ha":"justify"}}}},"cells":[]}"#,
        // Out-of-bounds custom numFmt index
        r#"{"sheet":"S","sheets":["S"],"cellMeta":{"S":{"numFmts":[],"cells":{"A1":{"nf":1}}}},"cells":[]}"#,
        // Out-of-bounds col index (XFD is 16384)
        r#"{"sheet":"S","sheets":["S"],"cellMeta":{"S":{"cols":[[16385,100]]}},"cells":[]}"#,
        // Non-object cellMeta
        r#"{"sheet":"S","sheets":["S"],"cellMeta":[],"cells":[]}"#,
    ];
    for payload in cases {
        let err = eval_sheet(payload).expect_err(payload);
        assert!(
            matches!(
                err,
                EasyOfficeError::InvalidArgument(_) | EasyOfficeError::ResourceBudgetExceeded(_)
            ),
            "{payload}: {err:?}"
        );
    }
}

#[test]
fn export_workbook_embeds_title_and_styling_for_cell_and_info_formulas() {
    let csv = convert_doc(
        r##"{
            "sourceFormat":"xlsx",
            "targetFormat":"csv",
            "title":"월간예산",
            "sheets":[{
                "name":"Budget",
                "numFmts":["#,##0.00"],
                "cells":[
                    {"ref":"A1","raw":"1234.5","style":{"nf":1}},
                    {"ref":"A2","raw":"=CELL(\"format\",A1)"},
                    {"ref":"A3","raw":"=CELL(\"filename\")"},
                    {"ref":"A4","raw":"=INFO(\"release\")"}
                ]
            }]
        }"##,
    );
    let text = String::from_utf8(csv)
        .unwrap()
        .trim_start_matches('\u{feff}')
        .to_string();
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.strip_suffix(',').unwrap_or(l))
        .collect();
    assert_eq!(
        lines,
        ["\"1,234.50\"", "\",2\"", "[월간예산.csv]Budget", "16.0"],
        "{text}"
    );
}
