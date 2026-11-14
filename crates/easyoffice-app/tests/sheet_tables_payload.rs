//! Tables through the engine's commands (PD-224): a file's tables come back in the open answer and go out
//! again in XLSX and ODS, XLS keeps values only, a session told the tables blocks the arrays that would be
//! placed in or cross one, a row or column edit moves them, and hostile payloads are refused.

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

fn convert(payload: &str) -> String {
    EasyOfficeIpcRouter::dispatch("convert_document_format", payload).unwrap()
}

fn bytes_of(json: &str) -> Vec<u8> {
    EasyOfficeIpcRouter::decode_base64(json_str_field(json, "convertedBase64")).unwrap()
}

fn decode(name: &str, bytes: &[u8]) -> String {
    convert(&format!(
        r#"{{"action":"decode","fileName":"{name}","sourceBase64":"{}"}}"#,
        EasyOfficeIpcRouter::encode_base64(bytes)
    ))
}

const SALES: &str = r#"{"name":"Sales","range":"A1:C4","header":true,"totals":false,"columns":["Region","Q1","Q2"],"style":"TableStyleMedium2","firstColumn":false,"lastColumn":false,"rowStripes":true,"columnStripes":false,"filter":[]}"#;

fn sheet_with_tables(tables: &str) -> String {
    format!(
        r#"{{"name":"S","cells":[{{"ref":"A1","raw":"Region"}},{{"ref":"B1","raw":"Q1"}},{{"ref":"C1","raw":"Q2"}},
          {{"ref":"A2","raw":"north"}},{{"ref":"B2","raw":"1"}},{{"ref":"C2","raw":"2"}}],"tables":{tables}}}"#
    )
}

fn save(format: &str, sheet: &str) -> Vec<u8> {
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"{format}","title":"표","sheets":[{sheet}]}}"#
    );
    bytes_of(&convert(&payload))
}

#[test]
fn a_saved_table_is_opened_again_in_xlsx_and_ods_and_xls_holds_values_only() {
    let sheet = sheet_with_tables(&format!("[{SALES}]"));
    for (format, name) in [("xlsx", "표.xlsx"), ("ods", "표.ods")] {
        let opened = decode(name, &save(format, &sheet));
        assert!(
            opened.contains(r#""tables":[{"name":"Sales","range":"A1:C4""#),
            "{format}: {opened}"
        );
        assert!(
            opened.contains(r#""style":"TableStyleMedium2""#),
            "{format}: {opened}"
        );
        assert!(opened.contains(r#""filter":[]"#), "{format}: {opened}");
        assert!(
            !opened.contains("가져오지 않았습니다"),
            "{format}: {opened}"
        );
    }
    // XLS: values only, no table and no warning about it.
    let xls = save("xls", &sheet);
    let opened = decode("표.xls", &xls);
    assert!(!opened.contains("\"tables\""), "{opened}");
    let saved = convert(&format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"xls","title":"표","sheets":[{sheet}]}}"#
    ));
    assert!(
        saved.contains(r#""warnings":[]"#),
        "no warning for the tables: {}",
        &saved[..200]
    );
    // XLSX to ODS to XLSX keeps the name and the range.
    let ods = save("ods", &sheet);
    let via = convert(&format!(
        r#"{{"sourceFormat":"ods","targetFormat":"xlsx","title":"표","sourceBase64":"{}"}}"#,
        EasyOfficeIpcRouter::encode_base64(&ods)
    ));
    let opened = decode("표.xlsx", &bytes_of(&via));
    assert!(
        opened.contains(r#""name":"Sales","range":"A1:C4""#),
        "{opened}"
    );
    assert!(
        opened.contains(r#""columns":["Region","Q1","Q2"]"#),
        "{opened}"
    );
}

#[test]
fn a_save_places_arrays_around_the_tables_of_the_sheet() {
    let sheet = r#"{"name":"S","cells":[
        {"ref":"A1","raw":"=SEQUENCE(3)"},{"ref":"E1","raw":"=SEQUENCE(3)"},{"ref":"G3","raw":"=SEQUENCE(2)"}],
      "tables":[{"name":"T","range":"G1:H4","columns":["a","b"]}],"spills":["E1:E3"]}"#;
    let csv = String::from_utf8(save("csv", sheet)).unwrap();
    let lines: Vec<&str> = csv.trim_start_matches('\u{feff}').lines().collect();
    // E1:E3 spills beside the table; the array anchored in the table is #SPILL!.
    assert_eq!(lines[0], "1,,,,1,,", "{csv}");
    assert!(lines[2].contains("#SPILL!"), "{csv}");
}

#[test]
fn a_session_told_the_tables_blocks_the_array_in_or_across_one() {
    let opened = call(r#"{"sessionAction":"open","sheets":["S"]}"#);
    let id = json_str_field(&opened, "session").to_string();
    let apply = call(&format!(
        r#"{{"sessionAction":"apply","session":"{id}","recalc":true,"changes":[
          {{"sheet":"S","cell":"A1","value":"=SEQUENCE(3)"}},
          {{"sheet":"S","cell":"C1","value":"=SEQUENCE(3)"}}]}}"#
    ));
    assert_eq!(changed_value(&apply, "A1").as_deref(), Some("1"), "{apply}");
    let tabled = call(&format!(
        r#"{{"sessionAction":"tables","session":"{id}","sheet":"S","tables":["A2:B5"]}}"#
    ));
    assert_eq!(
        changed_value(&tabled, "A1").as_deref(),
        Some("#SPILL!"),
        "{tabled}"
    );
    assert!(tabled.contains("\"blocked\":true"), "{tabled}");
    let free = call(&format!(
        r#"{{"sessionAction":"tables","session":"{id}","sheet":"S","tables":[]}}"#
    ));
    assert_eq!(changed_value(&free, "A1").as_deref(), Some("1"), "{free}");
}

#[test]
fn a_request_without_a_session_takes_the_tables() {
    let answer = call(
        r#"{"sheet":"S","sheets":["S"],"cells":[
            {"sheet":"S","cell":"A1","value":"=SEQUENCE(3)"},
            {"sheet":"S","cell":"E1","value":"=SEQUENCE(2)"}],
          "tables":{"S":["E1:F4"]},
          "targets":[{"sheet":"S","cell":"A1"},{"sheet":"S","cell":"E1"}]}"#,
    );
    assert!(answer.contains(r#""targetValues":["1","#), "{answer}");
    assert!(answer.contains("\"#SPILL!\""), "{answer}");
}

fn structure(id: &str, axis: &str, op: &str, at: u32, count: u32, tables: &str) -> String {
    call(&format!(
        r#"{{"sessionAction":"structure","session":"{id}","sheet":"S","axis":"{axis}","op":"{op}","at":{at},"count":{count},"tables":{tables}}}"#
    ))
}

#[test]
fn row_and_column_edits_move_the_tables_the_page_sent() {
    let opened = call(r#"{"sessionAction":"open","sheets":["S"]}"#);
    let id = json_str_field(&opened, "session").to_string();
    let tables = format!("[{SALES}]");
    let rows = structure(&id, "row", "insert", 3, 2, &tables);
    assert!(
        rows.contains(r#""tables":[{"name":"Sales","range":"A1:C6""#),
        "{rows}"
    );
    let before = structure(&id, "row", "insert", 1, 1, &tables);
    assert!(before.contains(r#""range":"A2:C5""#), "{before}");
    let cols = structure(&id, "col", "insert", 2, 1, &tables);
    assert!(cols.contains(r#""range":"A1:D4""#), "{cols}");
    assert!(
        cols.contains(r#""columns":["Region","Column1","Q1","Q2"]"#),
        "{cols}"
    );
    let gone = structure(&id, "row", "delete", 1, 1, &tables);
    assert!(gone.contains(r#""tables":[]"#), "{gone}");
    let dropped = structure(&id, "col", "delete", 2, 1, &tables);
    assert!(
        dropped.contains(r#""columns":["Region","Q2"]"#),
        "{dropped}"
    );
    // A protected sheet refuses the edit and nothing moves.
    let refused = call(&format!(
        r#"{{"sessionAction":"structure","session":"{id}","sheet":"S","axis":"row","op":"insert","at":2,"count":1,
          "protect":{{"on":true,"deny":["insertRows"]}},"tables":{tables}}}"#
    ));
    assert!(refused.contains("\"refused\":\"protected\""), "{refused}");
    assert!(!refused.contains("\"tables\""), "{refused}");
}

#[test]
fn hostile_tables_are_refused_in_every_entry_point() {
    let opened = call(r#"{"sessionAction":"open","sheets":["S"]}"#);
    let id = json_str_field(&opened, "session").to_string();
    let item = |n: usize| {
        format!(
            r#"{{"name":"T{n}x","range":"A{}:A{}","columns":["a"]}}"#,
            2 * n + 1,
            2 * n + 2
        )
    };
    let flood = format!("[{}]", (0..100_000).map(item).collect::<Vec<_>>().join(","));
    let overlap = r#"[{"name":"A","range":"A1:B3","columns":["a","b"]},{"name":"B","range":"B3:C5","columns":["a","b"]}]"#;
    let injected = r#"[{"name":"a\"<>&","range":"A1:A2","columns":["a"]}]"#;
    let wide = r#"[{"name":"W","range":"A1:XFD2","columns":["a"]}]"#;
    for tables in [flood.as_str(), overlap, injected, wide] {
        let sheet = sheet_with_tables(tables);
        let payload = format!(
            r#"{{"sourceFormat":"xlsx","targetFormat":"xlsx","title":"표","sheets":[{sheet}]}}"#
        );
        assert!(
            EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).is_err(),
            "{}",
            &tables[..tables.len().min(60)]
        );
        let edit = format!(
            r#"{{"sessionAction":"structure","session":"{id}","sheet":"S","axis":"row","op":"insert","at":1,"count":1,"tables":{tables}}}"#
        );
        assert!(EasyOfficeIpcRouter::dispatch("evaluate_sheet", &edit).is_err());
    }
    for bad in [
        flood.replace("\"A", "\"B").as_str(),
        r#"["A1"]"#,
        r#"["A:B"]"#,
        r#"[1]"#,
        r#"{"A1":"B2"}"#,
    ] {
        let answer = EasyOfficeIpcRouter::dispatch(
            "evaluate_sheet",
            &format!(r#"{{"sessionAction":"tables","session":"{id}","sheet":"S","tables":{bad}}}"#),
        );
        assert!(answer.is_err(), "{}", &bad[..bad.len().min(40)]);
    }
}

#[test]
fn a_table_name_with_markup_characters_never_reaches_a_package() {
    // A name the syntax allows but that looks like markup once escaped stays one name.
    let sheet = sheet_with_tables(
        r#"[{"name":"Tab.1_x","range":"A1:C2","columns":["a\"<>&","b'","c"],"style":"S&amp;\"<"}]"#,
    );
    let opened = decode("표.xlsx", &save("xlsx", &sheet));
    assert!(opened.contains(r#""style":"S&amp;\"<""#), "{opened}");
    assert!(opened.contains(r#""name":"Tab.1_x""#), "{opened}");
}
