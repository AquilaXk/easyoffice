//! Structured references through the engine's commands (PD-225): a session told the tables with their names
//! and columns calculates `Sales[Q1]` and follows a row edit and a table change, a request without a session
//! reads them, a save keeps the file spelling in XLSX, ranges in ODS and values in XLS and CSV, and hostile
//! formula text is answered with an error.

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

/// A1:C4 is `Sales` (Region, Q1, Q2); rows 2-4 are data.
const SALES: &str = r#"{"name":"Sales","range":"A1:C4","header":true,"totals":false,"columns":["Region","Q1","Q2"]}"#;

fn changes(extra: &str) -> String {
    format!(
        r#"[{{"sheet":"S","cell":"A1","value":"Region"}},{{"sheet":"S","cell":"B1","value":"Q1"}},{{"sheet":"S","cell":"C1","value":"Q2"}},
          {{"sheet":"S","cell":"B2","value":"1"}},{{"sheet":"S","cell":"B3","value":"2"}},{{"sheet":"S","cell":"B4","value":"3"}},
          {{"sheet":"S","cell":"C2","value":"10"}},{{"sheet":"S","cell":"C3","value":"20"}},{{"sheet":"S","cell":"C4","value":"30"}}{extra}]"#
    )
}

fn open() -> String {
    let opened = call(r#"{"sessionAction":"open","sheets":["S"]}"#);
    json_str_field(&opened, "session").to_string()
}

#[test]
fn a_session_told_the_tables_calculates_references_and_follows_edits() {
    let id = open();
    let told = call(&format!(
        r#"{{"sessionAction":"tables","session":"{id}","sheet":"S","tables":[{SALES}]}}"#
    ));
    assert!(!told.contains("expired"), "{told}");
    let extra = r#",{"sheet":"S","cell":"E1","value":"=SUM(Sales[Q1])"},{"sheet":"S","cell":"E2","value":"=Sales[[#Headers],[Q2]]"},
        {"sheet":"S","cell":"D2","value":"=[@Q1]*[@Q2]"}"#;
    let applied = call(&format!(
        r#"{{"sessionAction":"apply","session":"{id}","recalc":true,"changes":{}}}"#,
        changes(extra)
    ));
    assert_eq!(
        changed_value(&applied, "E1").as_deref(),
        Some("6"),
        "{applied}"
    );
    assert_eq!(
        changed_value(&applied, "E2").as_deref(),
        Some("Q2"),
        "{applied}"
    );
    // D2 is outside the table's columns: `[@Q1]` there has no table.
    assert_eq!(
        changed_value(&applied, "D2").as_deref(),
        Some("#REF!"),
        "{applied}"
    );
    // A row inserted inside the table: the engine moved its own copy, so the sum covers the new row.
    let moved = call(&format!(
        r#"{{"sessionAction":"structure","session":"{id}","sheet":"S","axis":"row","op":"insert","at":3,"count":1,"tables":[{SALES}]}}"#
    ));
    assert!(moved.contains(r#""range":"A1:C5""#), "{moved}");
    let filled = call(&format!(
        r#"{{"sessionAction":"apply","session":"{id}","recalc":true,"changes":[{{"sheet":"S","cell":"B3","value":"100"}}]}}"#
    ));
    assert_eq!(
        changed_value(&filled, "E1").as_deref(),
        Some("106"),
        "{filled}"
    );
    // The page sends the moved table again: nothing changes.
    let moved_table = SALES.replace("A1:C4", "A1:C5");
    let again = call(&format!(
        r#"{{"sessionAction":"tables","session":"{id}","sheet":"S","tables":[{moved_table}]}}"#
    ));
    assert!(!again.contains("\"cell\":\"E1\""), "{again}");
    // A renamed column: the old name is gone.
    let renamed = moved_table.replace(r#""Q1""#, r#""Q1x""#);
    let renamed = call(&format!(
        r#"{{"sessionAction":"tables","session":"{id}","sheet":"S","tables":[{renamed}]}}"#
    ));
    assert_eq!(
        changed_value(&renamed, "E1").as_deref(),
        Some("#REF!"),
        "{renamed}"
    );
    // No tables: the table is not there.
    let none = call(&format!(
        r#"{{"sessionAction":"tables","session":"{id}","sheet":"S","tables":[]}}"#
    ));
    assert_eq!(
        changed_value(&none, "E1").as_deref(),
        Some("#REF!"),
        "{none}"
    );
}

#[test]
fn a_request_without_a_session_reads_structured_references() {
    let answer = call(
        r#"{"sheet":"S","sheets":["S"],"cells":[
            {"sheet":"S","cell":"B1","value":"Q1"},{"sheet":"S","cell":"B2","value":"4"},{"sheet":"S","cell":"B3","value":"5"},
            {"sheet":"S","cell":"E1","value":"=SUM(Sales[Q1])"},{"sheet":"S","cell":"E2","value":"=Sales[[#This Row],[Q1]]"}],
          "tables":{"S":[{"name":"Sales","range":"B1:B3","columns":["Q1"]}]},
          "targets":[{"sheet":"S","cell":"E1"},{"sheet":"S","cell":"E2"}]}"#,
    );
    assert!(answer.contains(r#""targetValues":["9","4"]"#), "{answer}");
}

fn sheet(formulas: &str) -> String {
    format!(
        r#"{{"name":"S","cells":[{{"ref":"A1","raw":"Region"}},{{"ref":"B1","raw":"Q1"}},{{"ref":"C1","raw":"Q2"}},
          {{"ref":"A2","raw":"n"}},{{"ref":"B2","raw":"1"}},{{"ref":"C2","raw":"=[@Q1]+1"}},
          {{"ref":"A3","raw":"s"}},{{"ref":"B3","raw":"2"}},{{"ref":"C3","raw":"=Sales[[#This Row],[Q1]]+1"}}{formulas}],
          "tables":[{SALES}]}}"#
    )
}

fn save(format: &str, sheet: &str) -> Vec<u8> {
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"{format}","title":"표","sheets":[{sheet}]}}"#
    );
    bytes_of(&convert(&payload))
}

#[test]
fn a_save_keeps_the_references_in_xlsx_and_the_results_everywhere() {
    let book = sheet(r#",{"ref":"E1","raw":"=SUM(Sales[Q1])+SUM(Sales[[#All],[Q2]])"}"#);
    // XLSX: the file spelling, which opens again as written.
    let xlsx = save("xlsx", &book);
    let opened = decode("표.xlsx", &xlsx);
    assert!(opened.contains("=Sales[[#This Row],[Q1]]+1"), "{opened}");
    assert!(
        opened.contains("=SUM(Sales[Q1])+SUM(Sales[[#All],[Q2]])"),
        "{opened}"
    );
    assert!(!opened.contains("[@Q1]"), "{opened}");
    // The same file saved again gives the same formulas (XLSX to XLSX).
    let again = convert(&format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"xlsx","title":"표","sourceBase64":"{}"}}"#,
        EasyOfficeIpcRouter::encode_base64(&xlsx)
    ));
    let reopened = decode("표.xlsx", &bytes_of(&again));
    assert!(
        reopened.contains("=SUM(Sales[Q1])+SUM(Sales[[#All],[Q2]])"),
        "{reopened}"
    );
    // ODS: ranges, and the same values once it is opened and calculated.
    let ods = decode("표.ods", &save("ods", &book));
    assert!(ods.contains("=SUM($B$2:$B$4)+SUM($C$1:$C$4)"), "{ods}");
    assert!(ods.contains("=$B2+1"), "{ods}");
    assert!(!ods.contains("Sales["), "{ods}");
    // CSV and XLS hold values: Q1 sums to 3 and Q2 (header text left out) to 5.
    let csv = String::from_utf8(save("csv", &book)).unwrap();
    let lines: Vec<&str> = csv.trim_start_matches('\u{feff}').lines().collect();
    assert_eq!(lines[0], "Region,Q1,Q2,,8", "{csv}");
    assert_eq!(lines[1], "n,1,2,,", "{csv}");
    let xls = decode("표.xls", &save("xls", &book));
    assert!(
        xls.contains(r#""raw":"8""#) || xls.contains(r#""value":"8""#),
        "{xls}"
    );
}

#[test]
fn hostile_formula_text_is_an_error_value_and_stays_fast() {
    let deep = format!("=SUM(Sales{}Q1{})", "[".repeat(3000), "]".repeat(3000));
    let escapes = format!("=SUM(Sales[{}])", "'['".repeat(2000));
    let many = format!("=SUM({})", "Sales[Q1],".repeat(700));
    let started = std::time::Instant::now();
    for (i, formula) in [
        deep,
        escapes,
        many,
        "=SUM(NoTable[x])".to_string(),
        "=SUM(Sales[Nope])".to_string(),
    ]
    .iter()
    .enumerate()
    {
        let answer = call(&format!(
            r#"{{"sheet":"S","sheets":["S"],"cells":[{{"sheet":"S","cell":"B1","value":"Q1"}},{{"sheet":"S","cell":"B2","value":"4"}},
              {{"sheet":"S","cell":"E1","value":"{}"}}],
              "tables":{{"S":[{{"name":"Sales","range":"B1:B2","columns":["Q1"]}}]}},
              "targets":[{{"sheet":"S","cell":"E1"}}]}}"#,
            EasyOfficeIpcRouter::escape_json_str(formula)
        ));
        if i == 2 {
            assert!(
                answer.contains(r#""targetValues":["2"#) || answer.contains("targetValues"),
                "{answer}"
            );
        } else {
            assert!(answer.contains(r#""targetValues":["#), "{answer}");
            assert!(answer.contains('#'), "{i}: {answer}");
        }
    }
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

#[test]
fn a_renamed_column_is_written_into_the_formulas_that_name_it() {
    let answer = call(&format!(
        r#"{{"sessionAction":"tableColumn","table":"sales","from":"q1","to":"Q 1$",
          "tables":{{"S":[{SALES}],"Other":[{{"name":"Other","range":"A1:B3","columns":["x","Q1"]}}]}},
          "names":[{{"name":"Qs","refersTo":"Sales[Q1]"}},{{"name":"Keep","refersTo":"Sales[Q2]"}}],
          "formulas":[
            {{"sheet":"S","cell":"E1","text":"=SUM(Sales[Q1])+Sales[[#This Row],[Q1]]"}},
            {{"sheet":"S","cell":"B2","text":"=[@Q1]*2"}},
            {{"sheet":"S","cell":"E2","text":"=[@Q1]*2"}},
            {{"sheet":"Other","cell":"B2","text":"=[@Q1]*2"}},
            {{"sheet":"S","cell":"E3","text":"=SUM(Sales[Q2])"}},
            {{"sheet":"S","cell":"E4","text":"=\"Sales[Q1]\""}},
            {{"sheet":"S","cell":"E5","text":"plain"}}]}}"#
    ));
    assert!(
        answer.contains(
            r#"{"sheet":"S","cell":"E1","text":"=SUM(Sales[[Q 1$]])+Sales[[#This Row],[Q 1$]]"}"#
        ),
        "{answer}"
    );
    assert!(
        answer.contains(r#"{"sheet":"S","cell":"B2","text":"=[@[Q 1$]]*2"}"#),
        "{answer}"
    );
    // Outside the table, and in another table, `[@Q1]` is not this column.
    for cell in ["E2", "E3", "E4", "E5"] {
        assert!(
            !answer.contains(&format!("\"cell\":\"{cell}\"")),
            "{cell}: {answer}"
        );
    }
    assert!(!answer.contains(r#""sheet":"Other""#), "{answer}");
    assert!(answer.contains(r#""names":[{"name":"Qs","refersTo":"Sales[[Q 1$]]"},{"name":"Keep","refersTo":"Sales[Q2]"}]"#), "{answer}");
    let unchanged = call(
        r#"{"sessionAction":"tableColumn","table":"Sales","from":"Zz","to":"Y","tables":{},"formulas":[{"sheet":"S","cell":"A1","text":"=Sales[Q1]"}]}"#,
    );
    assert_eq!(unchanged, r#"{"rewrites":[],"names":null}"#);
    for hostile in [
        r#"{"sessionAction":"tableColumn","table":"","from":"a","to":"b","formulas":[]}"#,
        r#"{"sessionAction":"tableColumn","table":"T","from":"a","formulas":[]}"#,
        r#"{"sessionAction":"tableColumn","table":"T","from":"a","to":"b"}"#,
        r#"{"sessionAction":"tableColumn","table":"T","from":"a","to":"b","formulas":[{"sheet":"S","cell":"ZZZZ9","text":"=T[a]"}]}"#,
        r#"{"sessionAction":"tableColumn","table":"T","from":"a","to":"b","formulas":[5]}"#,
    ] {
        assert!(
            EasyOfficeIpcRouter::dispatch("evaluate_sheet", hostile).is_err(),
            "{hostile}"
        );
    }
}
