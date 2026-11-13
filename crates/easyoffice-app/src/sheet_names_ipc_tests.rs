//! Defined names over the IPC command (PD-214): the session takes the whole list, recomputes what uses a
//! changed name, moves the names with a row insertion or a sheet change, and hostile requests change
//! nothing.

use super::*;
use easyoffice_formats::StrictJsonValue;

fn call(body: &str) -> StrictJsonValue {
    let res = EasyOfficeIpcRouter::dispatch("evaluate_sheet", body).unwrap();
    StrictJsonValue::parse(&res).unwrap()
}

fn field<'a>(v: &'a StrictJsonValue, key: &str) -> &'a StrictJsonValue {
    StrictJsonValue::get_field(v.as_object().unwrap(), key).unwrap()
}

fn open(cells: &[(&str, &str)]) -> String {
    let res = call("{\"sessionAction\":\"open\",\"sheets\":[\"Sheet1\",\"Data\"]}");
    let id = field(&res, "session").as_str().unwrap().to_string();
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

fn values(res: &StrictJsonValue) -> Vec<(String, String)> {
    field(res, "changed")
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            let o = c.as_object().unwrap();
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

fn set_names(id: &str, names: &str) -> StrictJsonValue {
    call(&format!(
        "{{\"sessionAction\":\"names\",\"session\":\"{id}\",\"names\":{names}}}"
    ))
}

#[test]
fn the_list_is_taken_whole_and_the_formulas_that_use_a_name_are_computed() {
    let id = open(&[
        ("Sheet1!A1", "1"),
        ("Sheet1!A2", "2"),
        ("Sheet1!C1", "=SUM(Prices)*Fee"),
    ]);
    let res = set_names(
        &id,
        "[{\"name\":\"Prices\",\"refersTo\":\"Sheet1!$A$1:$A$2\"},{\"name\":\"Fee\",\"refersTo\":\"10\"}]",
    );
    assert_eq!(
        values(&res),
        vec![("Sheet1!C1".to_string(), "30".to_string())]
    );
    let res = set_names(&id, "[{\"name\":\"Prices\",\"refersTo\":\"Sheet1!$A$1\"},{\"name\":\"Fee\",\"refersTo\":\"10\"}]");
    assert_eq!(
        values(&res),
        vec![("Sheet1!C1".to_string(), "10".to_string())]
    );
    let res = set_names(&id, "[]");
    assert_eq!(
        values(&res),
        vec![("Sheet1!C1".to_string(), "#NAME?".to_string())]
    );
}

#[test]
fn a_name_that_cannot_be_used_is_listed_and_the_rest_still_work() {
    let id = open(&[("Sheet1!C1", "=Good")]);
    let res = set_names(
        &id,
        "[{\"name\":\"Good\",\"refersTo\":\"7\"},{\"name\":\"A1\",\"refersTo\":\"1\"},{\"name\":\"Loop_a\",\"refersTo\":\"Loop_b\"},{\"name\":\"Loop_b\",\"refersTo\":\"Loop_a\"}]",
    );
    assert_eq!(
        values(&res),
        vec![("Sheet1!C1".to_string(), "7".to_string())]
    );
    let issues = field(&res, "nameIssues").to_json_string();
    assert_eq!(
        issues,
        "[{\"index\":1,\"problem\":\"name\"},{\"index\":2,\"problem\":\"cycle\"},{\"index\":3,\"problem\":\"cycle\"}]"
    );
}

#[test]
fn a_hostile_list_is_refused_without_changing_the_session() {
    let id = open(&[("Sheet1!A1", "5"), ("Sheet1!C1", "=Keep")]);
    set_names(&id, "[{\"name\":\"Keep\",\"refersTo\":\"Sheet1!$A$1\"}]");
    let many = format!(
        "[{}]",
        (0..easyoffice_formats::MAX_NAMES + 1)
            .map(|i| format!("{{\"name\":\"n_{i}\",\"refersTo\":\"1\"}}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    for body in [
        format!("{{\"sessionAction\":\"names\",\"session\":\"{id}\",\"names\":{many}}}"),
        format!("{{\"sessionAction\":\"names\",\"session\":\"{id}\",\"names\":{{}}}}"),
        format!("{{\"sessionAction\":\"names\",\"session\":\"{id}\",\"names\":[1]}}"),
        format!(
            "{{\"sessionAction\":\"names\",\"session\":\"{id}\",\"names\":[{{\"name\":\"{}\"}}]}}",
            "x".repeat(300)
        ),
    ] {
        assert!(
            EasyOfficeIpcRouter::dispatch("evaluate_sheet", &body).is_err(),
            "{}",
            &body[..80]
        );
    }
    let res = call(&format!(
        "{{\"sessionAction\":\"apply\",\"session\":\"{id}\",\"changes\":[{{\"sheet\":\"Sheet1\",\"cell\":\"A1\",\"value\":\"6\"}}]}}"
    ));
    assert_eq!(
        values(&res),
        vec![("Sheet1!C1".to_string(), "6".to_string())]
    );
    let gone = call("{\"sessionAction\":\"names\",\"session\":\"nope\",\"names\":[]}");
    assert_eq!(gone.to_json_string(), "{\"expired\":true}");
}

#[test]
fn a_row_insertion_answers_the_moved_names() {
    let id = open(&[
        ("Sheet1!A1", "1"),
        ("Sheet1!A2", "2"),
        ("Sheet1!C1", "=SUM(Prices)"),
    ]);
    set_names(&id, "[{\"name\":\"Prices\",\"refersTo\":\"Sheet1!$A$1:$A$2\"},{\"name\":\"Data_only\",\"refersTo\":\"Data!$A$1\"}]");
    let res = call(&format!(
        "{{\"sessionAction\":\"structure\",\"session\":\"{id}\",\"sheet\":\"Sheet1\",\"axis\":\"row\",\"op\":\"insert\",\"at\":2,\"count\":1}}"
    ));
    assert_eq!(
        field(&res, "names").to_json_string(),
        "[{\"name\":\"Prices\",\"refersTo\":\"Sheet1!$A$1:$A$3\"},{\"name\":\"Data_only\",\"refersTo\":\"Data!$A$1\"}]"
    );
    let untouched = call(&format!(
        "{{\"sessionAction\":\"structure\",\"session\":\"{id}\",\"sheet\":\"Data\",\"axis\":\"row\",\"op\":\"insert\",\"at\":9,\"count\":1}}"
    ));
    assert!(StrictJsonValue::get_field(untouched.as_object().unwrap(), "names").is_none());
}

#[test]
fn a_sheet_rename_and_deletion_answer_the_names_they_change() {
    let id = open(&[("Sheet1!A1", "1")]);
    set_names(
        &id,
        "[{\"name\":\"Prices\",\"refersTo\":\"Data!$A$1:$A$3\"},{\"name\":\"Mine\",\"scope\":\"Data\",\"refersTo\":\"Data!$B$1\"},{\"name\":\"Other\",\"refersTo\":\"Sheet1!$A$1\"}]",
    );
    let renamed = call(&format!(
        "{{\"sessionAction\":\"sheetRefs\",\"session\":\"{id}\",\"op\":\"rename\",\"sheet\":\"Data\",\"newName\":\"My Data\"}}"
    ));
    assert_eq!(
        field(&renamed, "names").to_json_string(),
        "[{\"name\":\"Prices\",\"refersTo\":\"'My Data'!$A$1:$A$3\"},{\"name\":\"Mine\",\"scope\":\"My Data\",\"refersTo\":\"'My Data'!$B$1\"},{\"name\":\"Other\",\"refersTo\":\"Sheet1!$A$1\"}]"
    );
    let deleted = call(&format!(
        "{{\"sessionAction\":\"sheetRefs\",\"session\":\"{id}\",\"op\":\"delete\",\"sheet\":\"Data\"}}"
    ));
    assert_eq!(
        field(&deleted, "names").to_json_string(),
        "[{\"name\":\"Prices\",\"refersTo\":\"#REF!\"},{\"name\":\"Other\",\"refersTo\":\"Sheet1!$A$1\"}]"
    );
}

#[test]
fn the_stateless_command_reads_the_names_of_the_request() {
    let body = concat!(
        "{\"sheet\":\"Sheet1\",\"sheets\":[\"Sheet1\"],",
        "\"cells\":[{\"sheet\":\"Sheet1\",\"cell\":\"A1\",\"value\":\"4\"},{\"sheet\":\"Sheet1\",\"cell\":\"B1\",\"value\":\"=Base*Fee\"}],",
        "\"names\":[{\"name\":\"Base\",\"refersTo\":\"Sheet1!$A$1\"},{\"name\":\"Fee\",\"refersTo\":\"2.5\"}],",
        "\"targets\":[{\"sheet\":\"Sheet1\",\"cell\":\"B1\"}]}"
    );
    let res = call(body);
    assert_eq!(field(&res, "value").as_str().unwrap(), "10");
}

#[test]
fn the_name_check_names_the_first_rule_a_new_name_breaks() {
    let check = |candidate: &str, replace: &str| {
        call(&format!(
            "{{\"sessionAction\":\"nameCheck\",\"sheets\":[\"Sheet1\",\"Data\"],\"names\":[{{\"name\":\"Prices\",\"refersTo\":\"Sheet1!$A$1\"}},{{\"name\":\"Rate\",\"refersTo\":\"1\"}}],\"candidate\":{candidate}{replace}}}"
        ))
        .to_json_string()
    };
    let ok = "{\"name\":\"Fresh\",\"refersTo\":\"Sheet1!$A$1:$B$2\"}";
    assert_eq!(check(ok, ""), "{\"problem\":null}");
    assert_eq!(
        check("{\"name\":\"prices\",\"refersTo\":\"1\"}", ""),
        "{\"problem\":\"duplicate\"}"
    );
    assert_eq!(
        check(
            "{\"name\":\"prices\",\"refersTo\":\"2\"}",
            ",\"replace\":{\"name\":\"Prices\"}"
        ),
        "{\"problem\":null}"
    );
    assert_eq!(
        check("{\"name\":\"SUM\",\"refersTo\":\"1\"}", ""),
        "{\"problem\":\"reserved\"}"
    );
    assert_eq!(
        check("{\"name\":\"data\",\"refersTo\":\"1\"}", ""),
        "{\"problem\":\"reserved\"}"
    );
    assert_eq!(
        check("{\"name\":\"A1\",\"refersTo\":\"1\"}", ""),
        "{\"problem\":\"name\",\"detail\":\"cellAddress\"}"
    );
    assert_eq!(
        check("{\"name\":\"two words\",\"refersTo\":\"1\"}", ""),
        "{\"problem\":\"name\",\"detail\":\"badChar\"}"
    );
    assert_eq!(
        check("{\"name\":\"Bad\",\"refersTo\":\"SUM(\"}", ""),
        "{\"problem\":\"formula\"}"
    );
    assert_eq!(
        check("{\"name\":\"Self\",\"refersTo\":\"Self+1\"}", ""),
        "{\"problem\":\"cycle\"}"
    );
    assert_eq!(
        check(
            "{\"name\":\"Fresh\",\"scope\":\"Nowhere\",\"refersTo\":\"1\"}",
            ""
        ),
        "{\"problem\":\"scope\"}"
    );
}
