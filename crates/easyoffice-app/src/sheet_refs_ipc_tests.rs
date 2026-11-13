//! Sheet rename and deletion over the IPC command (PD-209): the session answers the formulas the
//! change rewrites and leaves itself alone; hostile requests change nothing.

use super::*;
use easyoffice_formats::StrictJsonValue;

fn call(body: &str) -> StrictJsonValue {
    let res = EasyOfficeIpcRouter::dispatch("evaluate_sheet", body).unwrap();
    StrictJsonValue::parse(&res).unwrap()
}

fn open_with(sheets: &[&str], cells: &[(&str, &str)]) -> String {
    let names: Vec<String> = sheets.iter().map(|s| format!("\"{s}\"")).collect();
    let res = call(&format!(
        "{{\"sessionAction\":\"open\",\"sheets\":[{}]}}",
        names.join(",")
    ));
    let id = res.as_object().unwrap()[0].1.as_str().unwrap().to_string();
    let changes: Vec<String> = cells
        .iter()
        .map(|(a1, v)| {
            let (sheet, cell) = a1.split_once('!').unwrap();
            format!("{{\"sheet\":\"{sheet}\",\"cell\":\"{cell}\",\"value\":\"{v}\"}}")
        })
        .collect();
    call(&format!(
        "{{\"sessionAction\":\"apply\",\"session\":\"{id}\",\"changes\":[{}]}}",
        changes.join(",")
    ));
    id
}

fn field<'a>(v: &'a StrictJsonValue, key: &str) -> &'a StrictJsonValue {
    StrictJsonValue::get_field(v.as_object().unwrap(), key).unwrap()
}

fn texts(res: &StrictJsonValue) -> Vec<(String, String)> {
    field(res, "formulas")
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            let o = f.as_object().unwrap();
            (
                format!(
                    "{}!{}",
                    StrictJsonValue::get_str(o, "sheet").unwrap(),
                    StrictJsonValue::get_str(o, "cell").unwrap()
                ),
                StrictJsonValue::get_str(o, "value").unwrap().to_string(),
            )
        })
        .collect()
}

fn refs(id: &str, rest: &str) -> String {
    format!("{{\"sessionAction\":\"sheetRefs\",\"session\":\"{id}\",{rest}}}")
}

#[test]
fn a_rename_answers_the_formulas_and_rules_that_name_the_sheet() {
    let id = open_with(
        &["S", "T"],
        &[("S!A1", "1"), ("T!A1", "=S!A1+1"), ("T!A2", "=A1")],
    );
    let res = call(&refs(
        &id,
        concat!(
            "\"op\":\"rename\",\"sheet\":\"S\",\"newName\":\"매출 합계\",",
            "\"rules\":{\"T\":{\"cf\":[{\"r\":\"A1:A9\",\"p\":1,\"t\":\"cellIs\",\"op\":\"greaterThan\",\"a\":\"=S!$B$1\"}],\"dv\":[]}}"
        ),
    ));
    assert_eq!(
        texts(&res),
        vec![("T!A1".to_string(), "='매출 합계'!A1+1".to_string())]
    );
    let rules = field(&res, "rules").to_json_string();
    assert!(
        rules.contains("'매출 합계'!$B$1") && rules.starts_with("{\"T\":"),
        "{rules}"
    );
    // The session still holds the old name: the same request answers the same.
    assert_eq!(
        texts(&call(&refs(
            &id,
            "\"op\":\"rename\",\"sheet\":\"S\",\"newName\":\"X\""
        )))
        .len(),
        1
    );
}

#[test]
fn a_deletion_answers_ref_errors() {
    let id = open_with(&["S", "T"], &[("T!A1", "=SUM(S!A1:A9)"), ("S!A1", "=T!A1")]);
    let res = call(&refs(&id, "\"op\":\"delete\",\"sheet\":\"S\""));
    assert_eq!(
        texts(&res),
        vec![("T!A1".to_string(), "=SUM(#REF!)".to_string())]
    );
}

#[test]
fn hostile_requests_change_nothing() {
    let id = open_with(&["S", "T"], &[("T!A1", "=S!A1")]);
    let long = "가".repeat(32);
    let bad_names = [
        "",
        "   ",
        long.as_str(),
        "a/b",
        "a\\\\b",
        "a?b",
        "a*b",
        "[a]",
        "a:b",
        "'a",
        "a'",
        "History",
        "t",
        "T",
        "a\\u0000b",
    ];
    for name in bad_names {
        let body = refs(
            &id,
            &format!("\"op\":\"rename\",\"sheet\":\"S\",\"newName\":\"{name}\""),
        );
        assert!(
            EasyOfficeIpcRouter::dispatch("evaluate_sheet", &body).is_err(),
            "{name:?}"
        );
    }
    for body in [
        refs(
            &id,
            "\"op\":\"rename\",\"sheet\":\"Nope\",\"newName\":\"X\"",
        ),
        refs(&id, "\"op\":\"delete\",\"sheet\":\"Nope\""),
        refs(&id, "\"op\":\"rename\",\"sheet\":\"S\""),
        refs(&id, "\"op\":\"spin\",\"sheet\":\"S\""),
        refs(&id, "\"op\":\"delete\""),
        refs(&id, "\"op\":\"delete\",\"sheet\":\"S\",\"rules\":[]"),
    ] {
        assert!(
            EasyOfficeIpcRouter::dispatch("evaluate_sheet", &body).is_err(),
            "{body}"
        );
    }
    // A name that only differs by case from the sheet's own is a plain rename.
    let res = call(&refs(
        &id,
        "\"op\":\"rename\",\"sheet\":\"S\",\"newName\":\"s\"",
    ));
    assert_eq!(texts(&res).len(), 1);
    let res = call(&refs("ssnone", "\"op\":\"delete\",\"sheet\":\"S\""));
    assert_eq!(res.to_json_string(), "{\"expired\":true}");
}

#[test]
fn the_last_sheet_cannot_be_deleted() {
    let id = open_with(&["Only"], &[("Only!A1", "1")]);
    let body = refs(&id, "\"op\":\"delete\",\"sheet\":\"Only\"");
    assert!(EasyOfficeIpcRouter::dispatch("evaluate_sheet", &body).is_err());
}

#[test]
fn a_session_of_many_sheets_renames_within_its_limits() {
    let names: Vec<String> = (1..=1_000).map(|i| format!("Sheet{i}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let id = open_with(&names, &[("Sheet2!A1", "=Sheet1000!A1+Sheet1!A1")]);
    let res = call(&refs(
        &id,
        "\"op\":\"rename\",\"sheet\":\"Sheet1000\",\"newName\":\"끝\"",
    ));
    assert_eq!(
        texts(&res),
        vec![("Sheet2!A1".to_string(), "='끝'!A1+Sheet1!A1".to_string())]
    );
    // Past the session's sheet limit the session is not even opened.
    let many: Vec<String> = (0..=1_024).map(|i| format!("\"S{i}\"")).collect();
    let body = format!(
        "{{\"sessionAction\":\"open\",\"sheets\":[{}]}}",
        many.join(",")
    );
    assert!(EasyOfficeIpcRouter::dispatch("evaluate_sheet", &body).is_err());
}
