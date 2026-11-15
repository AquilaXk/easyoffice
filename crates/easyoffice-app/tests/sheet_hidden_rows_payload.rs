//! The rows a sheet hides, through the engine's commands (PD-232): a session told them calculates only the `SUBTOTAL` and
//! `AGGREGATE` formulas again, a request without a session and a save take them, and hostile payloads are refused.

use easyoffice_app::EasyOfficeIpcRouter;

fn call(payload: &str) -> Result<String, easyoffice_formats::EasyOfficeError> {
    EasyOfficeIpcRouter::dispatch("evaluate_sheet", payload)
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

const CELLS: &str = r#"{"sheet":"S","cell":"A1","value":"440"},{"sheet":"S","cell":"A2","value":"285"},
    {"sheet":"S","cell":"A3","value":"215"},{"sheet":"S","cell":"A4","value":"310"},
    {"sheet":"S","cell":"B1","value":"=SUBTOTAL(109,A1:A4)"},{"sheet":"S","cell":"B2","value":"=SUBTOTAL(9,A1:A4)"},
    {"sheet":"S","cell":"B3","value":"=SUM(A1:A4)"}"#;

#[test]
fn a_session_told_the_hidden_rows_calculates_only_the_subtotals_again() {
    let opened = call(r#"{"sessionAction":"open","sheets":["S"]}"#).unwrap();
    let id = json_str_field(&opened, "session").to_string();
    let apply = call(&format!(
        r#"{{"sessionAction":"apply","session":"{id}","recalc":true,"changes":[{CELLS}]}}"#
    ))
    .unwrap();
    assert_eq!(
        changed_value(&apply, "B1").as_deref(),
        Some("1250"),
        "{apply}"
    );
    // Row 2 hidden by a filter, row 3 by the user (1-based runs).
    let hidden = call(&format!(
        r#"{{"sessionAction":"hiddenRows","session":"{id}","sheet":"S","hiddenRows":{{"hidden":[[3,3]],"filtered":[[2,2]]}}}}"#
    ))
    .unwrap();
    assert_eq!(
        changed_value(&hidden, "B1").as_deref(),
        Some("750"),
        "109: both kinds skipped, {hidden}"
    );
    assert_eq!(
        changed_value(&hidden, "B2").as_deref(),
        Some("965"),
        "9: the filtered row skipped, {hidden}"
    );
    assert_eq!(
        changed_value(&hidden, "B3"),
        None,
        "SUM is not calculated again: {hidden}"
    );
    let shown = call(&format!(
        r#"{{"sessionAction":"hiddenRows","session":"{id}","sheet":"S","hiddenRows":null}}"#
    ))
    .unwrap();
    assert_eq!(
        changed_value(&shown, "B1").as_deref(),
        Some("1250"),
        "{shown}"
    );
    assert_eq!(
        changed_value(&shown, "B2").as_deref(),
        Some("1250"),
        "{shown}"
    );
}

#[test]
fn a_request_without_a_session_takes_the_hidden_rows() {
    let answer = call(&format!(
        r#"{{"sheet":"S","sheets":["S"],"cells":[{CELLS}],
          "hiddenRows":{{"S":{{"hidden":[],"filtered":[[2,3]]}}}},
          "targets":[{{"sheet":"S","cell":"B1"}},{{"sheet":"S","cell":"B2"}},{{"sheet":"S","cell":"B3"}}]}}"#
    ))
    .unwrap();
    assert!(
        answer.contains(r#""targetValues":["750","750","1250"]"#),
        "{answer}"
    );
}

#[test]
fn a_save_calculates_the_subtotals_without_the_hidden_rows() {
    // The sheet's layout says row 2 is hidden by the filter; the cached value of the formula in the file is the filtered sum.
    let payload = r#"{"sourceFormat":"xlsx","targetFormat":"xlsx","title":"표","sheets":[{"name":"S",
      "cells":[{"ref":"A1","raw":"440"},{"ref":"A2","raw":"285"},{"ref":"A3","raw":"215"},{"ref":"B1","raw":"=SUBTOTAL(109,A1:A3)"},{"ref":"B2","raw":"=LAMBDA(r,SUBTOTAL(109,r))(A1:A3)"}],
      "hiddenRows":[[2,2]]}]}"#;
    let saved = EasyOfficeIpcRouter::dispatch("convert_document_format", payload).unwrap();
    let bytes =
        EasyOfficeIpcRouter::decode_base64(json_str_field(&saved, "convertedBase64")).unwrap();
    let entries = easyoffice_formats::ZipArchiveCodec::unpack(
        &bytes,
        &easyoffice_formats::ZipSecurityPolicy::default(),
    )
    .unwrap();
    let sheet = entries
        .iter()
        .find(|e| e.path == "xl/worksheets/sheet1.xml")
        .unwrap();
    let xml = String::from_utf8(sheet.data.clone()).unwrap();
    assert!(
        xml.contains("<f>SUBTOTAL(109,A1:A3)</f><v>655</v>"),
        "the user hid row 2: 109 leaves it out, {xml}"
    );
    assert!(
        xml.contains("<v>655</v>") && xml.matches("<v>655</v>").count() == 2,
        "a lambda around the subtotal too, {xml}"
    );
}

#[test]
fn hostile_hidden_rows_are_refused() {
    let opened = call(r#"{"sessionAction":"open","sheets":["S"]}"#).unwrap();
    let id = json_str_field(&opened, "session").to_string();
    let flood = format!(
        "[{}]",
        (1..=200_000u32)
            .map(|n| format!("[{},{}]", n * 2, n * 2))
            .collect::<Vec<_>>()
            .join(",")
    );
    for bad in [
        r#"[]"#.to_string(),
        r#""x""#.to_string(),
        r#"{"hidden":[[0,1]]}"#.to_string(),
        r#"{"hidden":[[3,2]]}"#.to_string(),
        r#"{"filtered":[[1,2000000]]}"#.to_string(),
        r#"{"filtered":"x"}"#.to_string(),
        format!(r#"{{"hidden":{flood}}}"#),
    ] {
        let sent = call(&format!(
            r#"{{"sessionAction":"hiddenRows","session":"{id}","sheet":"S","hiddenRows":{bad}}}"#
        ));
        // The flood is within the row bound (it is a list of single rows), so it is taken; every other one is refused.
        if bad.starts_with(r#"{"hidden":["#) && bad.len() > 1000 {
            assert!(sent.is_ok());
        } else {
            assert!(sent.is_err(), "{}", &bad[..bad.len().min(60)]);
        }
    }
    let unknown = call(&format!(
        r#"{{"sessionAction":"hiddenRows","session":"{id}","sheet":"Nope","hiddenRows":{{"hidden":[[1,1]]}}}}"#
    ));
    assert!(
        unknown.is_err(),
        "a sheet the session does not have is refused"
    );
}
