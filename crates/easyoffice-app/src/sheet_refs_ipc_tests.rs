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
fn deleting_the_end_of_a_3d_span_moves_it_to_the_neighbor_in_tab_order() {
    // The session holds the sheets in tab order: Jan Feb Mar Apr Sum.
    let id = open_with(
        &["Jan", "Feb", "Mar", "Apr", "Sum"],
        &[
            ("Sum!A1", "=SUM(Jan:Apr!A1)"),
            ("Sum!A2", "=SUM(Jan:Feb!A1)"),
            ("Sum!A3", "=SUM(Feb:Feb!A1)"),
        ],
    );
    let mut got = texts(&call(&refs(&id, "\"op\":\"delete\",\"sheet\":\"Jan\"")));
    got.sort();
    assert_eq!(
        got,
        vec![
            ("Sum!A1".to_string(), "=SUM(Feb:Apr!A1)".to_string()),
            ("Sum!A2".to_string(), "=SUM(Feb!A1)".to_string()),
        ]
    );
    let mut got = texts(&call(&refs(&id, "\"op\":\"delete\",\"sheet\":\"Apr\"")));
    got.sort();
    assert_eq!(
        got,
        vec![("Sum!A1".to_string(), "=SUM(Jan:Mar!A1)".to_string())]
    );
    let got = texts(&call(&refs(&id, "\"op\":\"delete\",\"sheet\":\"Feb\"")));
    assert!(got.contains(&("Sum!A3".to_string(), "=SUM(#REF!)".to_string())));
}

#[test]
fn a_deletion_answers_the_rules_that_name_a_span_end() {
    let id = open_with(&["Jan", "Feb", "Mar", "T"], &[]);
    let res = call(&refs(
        &id,
        concat!(
            "\"op\":\"delete\",\"sheet\":\"Mar\",",
            "\"rules\":{\"T\":{\"cf\":[{\"r\":\"A1:A9\",\"p\":1,\"t\":\"cellIs\",\"op\":\"greaterThan\",\"a\":\"=SUM(Jan:Mar!$B$1)\"}],\"dv\":[]}}"
        ),
    ));
    let rules = field(&res, "rules").to_json_string();
    assert!(rules.contains("=SUM(Jan:Feb!$B$1)"), "{rules}");
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

#[test]
fn a_rename_and_a_deletion_answer_the_link_targets_that_name_the_sheet() {
    let id = open_with(&["S", "T"], &[("S!A1", "1")]);
    let targets =
        r##""linkTargets":{"T":["#S!B5","#'S'!A1:A3","#T!B5","#Total","https://a.b/S!A1"]}"##;
    let rename = call(&refs(
        &id,
        &format!("\"op\":\"rename\",\"sheet\":\"S\",\"newName\":\"매출 합계\",{targets}"),
    ));
    assert_eq!(
        field(&rename, "linkTargets").to_json_string(),
        r##"{"T":[["#S!B5","#'매출 합계'!B5"],["#'S'!A1:A3","#'매출 합계'!A1:A3"]]}"##
    );
    let delete = call(&refs(
        &id,
        &format!("\"op\":\"delete\",\"sheet\":\"S\",{targets}"),
    ));
    assert_eq!(
        field(&delete, "linkTargets").to_json_string(),
        r###"{"T":[["#S!B5","##REF!"],["#'S'!A1:A3","##REF!"]]}"###
    );
}

fn moved(id: &str, sheets: &str, order: &str) -> String {
    refs(
        id,
        &format!("\"op\":\"move\",\"sheets\":[{sheets}],\"order\":[{order}]"),
    )
}

#[test]
fn moving_an_end_of_a_3d_span_past_the_other_end_takes_the_neighbor_in_tab_order() {
    // The session holds the sheets in tab order: Jan Feb Mar Apr Sum.
    let id = open_with(
        &["Jan", "Feb", "Mar", "Apr", "Sum"],
        &[
            ("Sum!A1", "=SUM(Jan:Apr!A1)"),
            ("Sum!A2", "=SUM(Feb:Mar!A1)"),
            ("Sum!A3", "=SUM(Jan:Feb!A1)"),
        ],
    );
    // Jan goes after Apr.
    let first = texts(&call(&moved(
        &id,
        r#""Jan""#,
        r#""Feb","Mar","Apr","Jan","Sum""#,
    )));
    assert_eq!(
        first
            .iter()
            .filter(|(c, _)| c == "Sum!A1")
            .collect::<Vec<_>>(),
        vec![&("Sum!A1".to_string(), "=SUM(Feb:Apr!A1)".to_string())]
    );
    // Jan:Feb with Jan after Feb is the one sheet that stays; Feb:Mar is untouched.
    assert!(first.contains(&("Sum!A3".to_string(), "=SUM(Feb!A1)".to_string())));
    assert_eq!(first.len(), 2);
    // Apr goes before Jan (the session did not change, so the same sheets answer).
    let second = texts(&call(&moved(
        &id,
        r#""Apr""#,
        r#""Apr","Jan","Feb","Mar","Sum""#,
    )));
    assert_eq!(
        second,
        vec![("Sum!A1".to_string(), "=SUM(Jan:Mar!A1)".to_string())]
    );
    // A move that keeps the ends in order rewrites nothing, whatever it passes over.
    let inside = texts(&call(&moved(
        &id,
        r#""Mar""#,
        r#""Jan","Feb","Apr","Mar","Sum""#,
    )));
    assert!(inside.is_empty(), "{inside:?}");
}

#[test]
fn a_move_answers_the_rules_that_name_a_span_end() {
    let id = open_with(&["Jan", "Feb", "Mar", "T"], &[]);
    let res = call(&refs(
        &id,
        concat!(
            "\"op\":\"move\",\"sheets\":[\"Jan\"],\"order\":[\"Feb\",\"Mar\",\"Jan\",\"T\"],",
            "\"rules\":{\"T\":{\"cf\":[{\"r\":\"A1:A9\",\"p\":1,\"t\":\"cellIs\",\"op\":\"greaterThan\",\"a\":\"=SUM(Jan:Mar!$B$1)\"}],\"dv\":[]}}"
        ),
    ));
    let rules = field(&res, "rules").to_json_string();
    assert!(rules.contains("=SUM(Feb:Mar!$B$1)"), "{rules}");
}

fn refusal(body: &str) -> String {
    EasyOfficeIpcRouter::dispatch("evaluate_sheet", body)
        .unwrap_err()
        .to_string()
}

#[test]
fn a_move_order_that_is_not_the_sessions_sheets_is_refused_before_anything_is_rewritten() {
    let tabs = [
        "Sheet1", "Sheet2", "Sheet3", "Sheet4", "Sheet5", "Sheet6", "Sum",
    ];
    let id = open_with(&tabs, &[("Sum!A1", "=SUM(Sheet2:Sheet6!A1)")]);
    let names = |list: &[&str]| {
        list.iter()
            .map(|n| format!("\"{n}\""))
            .collect::<Vec<_>>()
            .join(",")
    };
    let huge: Vec<String> = (0..5_000).map(|i| format!("\"S{i}\"")).collect();
    let orders = [
        // A name twice (and so one of the session's names missing): Sheet2:Sheet6 must not become Sheet3:Sheet6.
        names(&[
            "Sheet1", "Sheet6", "Sheet6", "Sheet4", "Sheet5", "Sheet6", "Sheet2",
        ]),
        // A name the session does not have.
        names(&[
            "Sheet1",
            "Sheet3",
            "Sheet4",
            "Sheet5",
            "Sheet6",
            "Elsewhere",
            "Sum",
        ]),
        // The same name in another case twice.
        names(&[
            "Sheet1", "Sheet3", "Sheet4", "Sheet5", "Sheet6", "SHEET6", "Sum",
        ]),
        // Too short, too long, empty.
        names(&["Sheet1", "Sheet3", "Sheet4", "Sheet5", "Sheet6", "Sum"]),
        names(&[
            "Sheet1", "Sheet3", "Sheet4", "Sheet5", "Sheet6", "Sheet2", "Sum", "Sum",
        ]),
        String::new(),
        huge.join(","),
    ];
    // The rules and link targets are malformed on purpose: the order check answers first.
    let malformed = ",\"rules\":[],\"linkTargets\":7";
    for order in &orders {
        let plain = moved(&id, r#""Sheet2""#, order);
        let message = refusal(&plain);
        assert!(message.contains("탭 순서"), "{order}: {message}");
        let with_rules = refs(
            &id,
            &format!("\"op\":\"move\",\"sheets\":[\"Sheet2\"],\"order\":[{order}]{malformed}"),
        );
        let message = refusal(&with_rules);
        assert!(message.contains("탭 순서"), "{order}: {message}");
    }
    // The session's own names in another case are a fine order.
    let fine = names(&[
        "sheet1", "SHEET3", "Sheet4", "Sheet5", "Sheet6", "sheet2", "SUM",
    ]);
    let got = texts(&call(&moved(&id, r#""Sheet2""#, &fine)));
    assert_eq!(
        got,
        vec![("Sum!A1".to_string(), "=SUM(Sheet3:Sheet6!A1)".to_string())]
    );
}

/// `fmt` saved from the structured `sheets` (a JSON array) and decoded again: the JSON the file opens as.
fn saved_and_reopened(fmt: &str, sheets: &str) -> String {
    let save = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(
            r#"{{"sourceFormat":"xlsx","targetFormat":"{fmt}","forSave":true,"sheets":{sheets}}}"#
        ),
    )
    .unwrap();
    let marker = "\"convertedBase64\":\"";
    let start = save.find(marker).unwrap() + marker.len();
    let b64 = &save[start..start + save[start..].find('"').unwrap()];
    EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(r#"{{"action":"decode","fileName":"book.{fmt}","sourceBase64":"{b64}"}}"#),
    )
    .unwrap()
}

#[test]
fn a_moved_span_end_survives_a_save_and_a_reopen_in_the_new_tab_order() {
    // Sheet2:Sheet6 with Sheet2 moved after Sheet6 (the reference app's help page example).
    let tabs = [
        "Sheet1", "Sheet2", "Sheet3", "Sheet4", "Sheet5", "Sheet6", "Sum",
    ];
    let id = open_with(&tabs, &[("Sum!A1", "=SUM(Sheet2:Sheet6!A1)")]);
    let after = [
        "Sheet1", "Sheet3", "Sheet4", "Sheet5", "Sheet6", "Sheet2", "Sum",
    ];
    let order: Vec<String> = after.iter().map(|n| format!("\"{n}\"")).collect();
    let rewritten = texts(&call(&moved(&id, r#""Sheet2""#, &order.join(","))));
    assert_eq!(
        rewritten,
        vec![("Sum!A1".to_string(), "=SUM(Sheet3:Sheet6!A1)".to_string())]
    );
    // The page saves the sheets in the new order with the rewritten formula.
    let sheets: Vec<String> = after
        .iter()
        .map(|name| match *name {
            "Sum" => format!(
                r#"{{"name":"Sum","cells":[{{"ref":"A1","raw":"{}"}}]}}"#,
                rewritten[0].1
            ),
            n => format!(
                r#"{{"name":"{n}","cells":[{{"ref":"A1","raw":"{}"}}]}}"#,
                n.trim_start_matches("Sheet")
            ),
        })
        .collect();
    for fmt in ["xlsx", "ods", "xls"] {
        let reopened = saved_and_reopened(fmt, &format!("[{}]", sheets.join(",")));
        let at: Vec<usize> = after
            .iter()
            .map(|n| {
                reopened
                    .find(&format!("\"name\":\"{n}\""))
                    .unwrap_or(usize::MAX)
            })
            .collect();
        assert!(
            at.windows(2).all(|w| w[0] < w[1]),
            "{fmt} order {at:?}: {reopened}"
        );
        if fmt == "xls" {
            // The binary format opens with the result the formula had when it was saved: 3 + 4 + 5 + 6.
            assert!(
                reopened.contains(r#"{"ref":"A1","raw":"18"}"#),
                "{fmt}: {reopened}"
            );
        } else {
            assert!(
                reopened.contains("=SUM(Sheet3:Sheet6!A1)"),
                "{fmt}: {reopened}"
            );
        }
    }
    // Opened in that order, the span sums Sheet3 to Sheet6 and leaves Sheet2 out: Sheet3 set to 10 gives 10 + 4 + 5 + 6.
    let session = open_with(
        &after,
        &[
            ("Sheet3!A1", "3"),
            ("Sheet4!A1", "4"),
            ("Sheet5!A1", "5"),
            ("Sheet6!A1", "6"),
            ("Sheet2!A1", "2"),
            ("Sum!A1", "=SUM(Sheet3:Sheet6!A1)"),
        ],
    );
    let sum = call(&format!(
        "{{\"sessionAction\":\"apply\",\"session\":\"{session}\",\"changes\":[{{\"sheet\":\"Sheet3\",\"cell\":\"A1\",\"value\":\"10\"}}]}}"
    ));
    let json = sum.to_json_string();
    assert!(json.contains("\"value\":\"25\""), "{json}");
}

#[test]
fn hostile_move_requests_change_nothing() {
    let id = open_with(&["S", "T"], &[("T!A1", "=SUM(S:T!A1)")]);
    let long: Vec<String> = (0..64).map(|i| format!("\"S{i}\"")).collect();
    let long = long.join(",");
    for body in [
        moved(&id, r#""Nope""#, r#""T","S""#),
        moved(&id, "", r#""T","S""#),
        moved(&id, r#""S","T","S""#, r#""T","S""#),
        moved(&id, r#""S""#, &long),
        moved(&id, "1", r#""T","S""#),
        moved(&id, r#""S""#, "2"),
        refs(&id, "\"op\":\"move\",\"order\":[\"T\",\"S\"]"),
        refs(&id, "\"op\":\"move\",\"sheets\":[\"S\"]"),
        refs(
            &id,
            "\"op\":\"move\",\"sheets\":\"S\",\"order\":[\"T\",\"S\"]",
        ),
        refs(
            &id,
            "\"op\":\"move\",\"sheet\":\"S\",\"order\":[\"T\",\"S\"]",
        ),
    ] {
        assert!(
            EasyOfficeIpcRouter::dispatch("evaluate_sheet", &body).is_err(),
            "{body}"
        );
    }
    assert_eq!(
        texts(&call(&moved(&id, r#""S""#, r#""T","S""#))),
        vec![("T!A1".to_string(), "=SUM(T!A1)".to_string())]
    );
    let res = call(&moved("ssnone", r#""S""#, r#""T","S""#));
    assert_eq!(res.to_json_string(), "{\"expired\":true}");
}
