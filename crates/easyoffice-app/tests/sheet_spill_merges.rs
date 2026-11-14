//! Arrays and merged ranges, `OFFSET` and `INDIRECT` through the engine's commands (PD-222): a session told
//! the merges answers `#SPILL!` for an array that would cross one, a request without a session takes them
//! in `merges`, a save places arrays around the merges of the sheet, and the readers of a cell an array fills
//! see it in the same answer.

use easyoffice_app::EasyOfficeIpcRouter;

fn call(payload: &str) -> String {
    EasyOfficeIpcRouter::dispatch("evaluate_sheet", payload).unwrap()
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
fn a_session_told_the_merges_blocks_the_array_that_crosses_one() {
    let opened = call(r#"{"sessionAction":"open","sheets":["S"]}"#);
    let id = json_str_field(&opened, "session").to_string();
    let apply = call(&format!(
        r#"{{"sessionAction":"apply","session":"{id}","recalc":true,"changes":[
          {{"sheet":"S","cell":"A1","value":"=SEQUENCE(3)"}},
          {{"sheet":"S","cell":"C1","value":"=SUM(INDIRECT(\"A1:A3\"))"}}]}}"#
    ));
    assert_eq!(changed_value(&apply, "C1").as_deref(), Some("6"), "{apply}");
    let merged = call(&format!(
        r#"{{"sessionAction":"merges","session":"{id}","sheet":"S","merges":["A2:B2"]}}"#
    ));
    assert_eq!(
        changed_value(&merged, "A1").as_deref(),
        Some("#SPILL!"),
        "{merged}"
    );
    // the reader sees the error of the anchor it reads through INDIRECT
    assert_eq!(
        changed_value(&merged, "C1").as_deref(),
        Some("#SPILL!"),
        "{merged}"
    );
    assert!(merged.contains("\"blocked\":true"), "{merged}");
    let unmerged = call(&format!(
        r#"{{"sessionAction":"merges","session":"{id}","sheet":"S","merges":[]}}"#
    ));
    assert_eq!(
        changed_value(&unmerged, "A1").as_deref(),
        Some("1"),
        "{unmerged}"
    );
    assert_eq!(
        changed_value(&unmerged, "C1").as_deref(),
        Some("6"),
        "{unmerged}"
    );
    // a merge list that is not ranges is refused
    let bad = EasyOfficeIpcRouter::dispatch(
        "evaluate_sheet",
        &format!(r#"{{"sessionAction":"merges","session":"{id}","sheet":"S","merges":["A1"]}}"#),
    );
    assert!(bad.is_err());
}

#[test]
fn a_request_without_a_session_takes_the_merges_and_reads_the_filled_cells() {
    let answer = call(
        r#"{"sheet":"S","sheets":["S"],"cells":[
            {"sheet":"S","cell":"A1","value":"=SEQUENCE(3)"},
            {"sheet":"S","cell":"C1","value":"=SUM(INDIRECT(\"A1:A3\"))"},
            {"sheet":"S","cell":"E1","value":"=SEQUENCE(2)"}],
          "merges":{"S":["E1:E2"]},
          "targets":[{"sheet":"S","cell":"C1"},{"sheet":"S","cell":"E1"},{"sheet":"S","cell":"A1"}]}"#,
    );
    assert!(answer.contains(r#""targetValues":["6","#), "{answer}");
    assert!(answer.contains("\"#SPILL!\""), "{answer}");
}

#[test]
fn a_save_places_arrays_around_the_merges_of_the_sheet() {
    let sheet = r#"{"name":"S","cells":[
        {"ref":"A1","raw":"=SEQUENCE(3)"},{"ref":"C1","raw":"=SEQUENCE(3)"}],
      "merges":["A2:B2"],"spills":["C1:C3"]}"#;
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"csv","title":"병합","sheets":[{sheet}]}}"#
    );
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).unwrap();
    let csv = String::from_utf8(
        EasyOfficeIpcRouter::decode_base64(json_str_field(&json, "convertedBase64")).unwrap(),
    )
    .unwrap();
    let lines: Vec<&str> = csv.trim_start_matches('\u{feff}').lines().collect();
    assert_eq!(lines, ["#SPILL!,,1", ",,2", ",,3"], "{csv}");
}

#[test]
fn merges_arrays_and_readers_survive_xlsx_and_a_conversion_to_values_and_ods_holds_no_merge() {
    // A1 would cross the merge A2:B2 (blocked); C1 spills over C1:C3; E1 and E2 read C1:C3 through INDIRECT and OFFSET.
    let sheet = r#"{"name":"S","cells":[
        {"ref":"A1","raw":"=SEQUENCE(3)"},{"ref":"C1","raw":"=SEQUENCE(3)"},
        {"ref":"E1","raw":"=SUM(INDIRECT(\"C1:C3\"))"},{"ref":"E2","raw":"=SUM(OFFSET(C1,0,0,3,1))"}],
      "merges":["A2:B2"],"spills":["C1:C3"]}"#;
    // ODS has no merged ranges (PD-194): the array that crossed the merge fills its cells once the file is opened again
    for (format, name, merged, expected) in [
        (
            "xlsx",
            "병합.xlsx",
            true,
            ["#SPILL!,,1,,6", ",,2,,6", ",,3,,"],
        ),
        ("ods", "병합.ods", false, ["1,,1,,6", "2,,2,,6", "3,,3,,"]),
    ] {
        let payload = format!(
            r#"{{"sourceFormat":"xlsx","targetFormat":"{format}","title":"병합","sheets":[{sheet}]}}"#
        );
        let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).unwrap();
        let bytes =
            EasyOfficeIpcRouter::decode_base64(json_str_field(&json, "convertedBase64")).unwrap();
        let opened = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &format!(
                r#"{{"action":"decode","fileName":"{name}","sourceBase64":"{}"}}"#,
                EasyOfficeIpcRouter::encode_base64(&bytes)
            ),
        )
        .unwrap();
        assert_eq!(
            opened.contains(r#""merges":["A2:B2"]"#),
            merged,
            "{format}: {opened}"
        );
        assert!(
            opened.contains(r#""spills":["C1:C3"]"#),
            "{format}: {opened}"
        );
        // another format: the values the engine calculates with the merge and the area read back
        let csv_payload = format!(
            r#"{{"sourceFormat":"{format}","targetFormat":"csv","title":"병합","sourceBase64":"{}"}}"#,
            EasyOfficeIpcRouter::encode_base64(&bytes)
        );
        let csv_json =
            EasyOfficeIpcRouter::dispatch("convert_document_format", &csv_payload).unwrap();
        let csv = String::from_utf8(
            EasyOfficeIpcRouter::decode_base64(json_str_field(&csv_json, "convertedBase64"))
                .unwrap(),
        )
        .unwrap();
        let lines: Vec<&str> = csv.trim_start_matches('\u{feff}').lines().collect();
        assert_eq!(lines, expected, "{format}: {csv}");
    }
}

#[test]
fn an_xls_holds_the_values_of_the_arrays_placed_around_the_merges() {
    let sheet = r#"{"name":"S","cells":[
        {"ref":"A1","raw":"=SEQUENCE(3)"},{"ref":"C1","raw":"=SEQUENCE(3)"},
        {"ref":"E1","raw":"=SUM(INDIRECT(\"C1:C3\"))"}],
      "merges":["A2:B2"],"spills":["C1:C3"]}"#;
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"xls","title":"병합","sheets":[{sheet}]}}"#
    );
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).unwrap();
    let bytes =
        EasyOfficeIpcRouter::decode_base64(json_str_field(&json, "convertedBase64")).unwrap();
    let opened = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(
            r#"{{"action":"decode","fileName":"병합.xls","sourceBase64":"{}"}}"#,
            EasyOfficeIpcRouter::encode_base64(&bytes)
        ),
    )
    .unwrap();
    for cell in [
        r##"{"ref":"A1","raw":"#SPILL!"}"##,
        r##"{"ref":"C3","raw":"3"}"##,
        r##"{"ref":"E1","raw":"6"}"##,
    ] {
        assert!(opened.contains(cell), "{cell}: {opened}");
    }
}

#[test]
fn a_merge_list_over_the_limit_or_with_a_wrong_entry_is_refused() {
    let opened = call(r#"{"sessionAction":"open","sheets":["S"]}"#);
    let id = json_str_field(&opened, "session").to_string();
    let many = vec![r#""A1:B2""#; 100_001].join(",");
    for bad in [
        format!("[{many}]"),
        r#"["A:B"]"#.to_string(),
        r#"[1]"#.to_string(),
        r#"{"A1":"B2"}"#.to_string(),
    ] {
        let answer = EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            &format!(r#"{{"sessionAction":"merges","session":"{id}","sheet":"S","merges":{bad}}}"#),
        );
        assert!(answer.is_err(), "{}", &bad[..bad.len().min(40)]);
    }
    // a request without a session refuses a merge list that is not an object of lists
    let stateless = EasyOfficeIpcRouter::dispatch(
        "evaluate_sheet",
        r#"{"sheet":"S","cells":[{"cell":"A1","value":"=1"}],"merges":["A1:B2"],"targets":[{"sheet":"S","cell":"A1"}]}"#,
    );
    assert!(stateless.is_err());
    // a merge of a sheet that is not there is ignored
    let ignored = call(
        r#"{"sheet":"S","cells":[{"cell":"A1","value":"=1"}],"merges":{"Z":["A1:B2"]},"targets":[{"sheet":"S","cell":"A1"}]}"#,
    );
    assert!(ignored.contains(r#""targetValues":["1"]"#), "{ignored}");
}
