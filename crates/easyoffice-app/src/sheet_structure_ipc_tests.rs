//! Row and column edits over the IPC command (PD-194): the session rewrites formulas and moves the
//! page's layout and rules; a bad request or an unknown session changes nothing.

use super::*;
use easyoffice_formats::StrictJsonValue;

fn call(body: &str) -> StrictJsonValue {
    let res = EasyOfficeIpcRouter::dispatch("evaluate_sheet", body).unwrap();
    StrictJsonValue::parse(&res).unwrap()
}

fn open_with(cells: &[(&str, &str)]) -> String {
    let res = call("{\"sessionAction\":\"open\",\"sheets\":[\"S\",\"T\"]}");
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

#[test]
fn inserting_a_row_rewrites_formulas_and_moves_the_page_layout() {
    let id = open_with(&[
        ("S!A1", "1"),
        ("S!A2", "2"),
        ("S!A3", "=SUM(A1:A2)"),
        ("T!A1", "=S!A3+S!A1"),
    ]);
    let res = call(&format!(
        concat!(
            "{{\"sessionAction\":\"structure\",\"session\":\"{}\",\"sheet\":\"S\",\"axis\":\"row\",",
            "\"op\":\"insert\",\"at\":2,\"count\":1,",
            "\"merges\":[\"A1:B3\",\"D5:E6\"],\"hiddenRows\":[[2,3]],\"hiddenCols\":[[4,4]],",
            "\"widths\":[[1,120]],",
            "\"rules\":{{\"S\":{{\"cf\":[{{\"r\":\"A1:A9\",\"p\":1,\"t\":\"cellIs\",\"op\":\"greaterThan\",\"a\":\"=$B$3\"}}],\"dv\":[]}}}}}}"
        ),
        id
    ));
    let formulas = field(&res, "formulas").as_array().unwrap();
    let texts: Vec<(String, String)> = formulas
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
        .collect();
    assert!(
        texts.contains(&("S!A4".into(), "=SUM(A1:A3)".into())),
        "{texts:?}"
    );
    assert!(
        texts.contains(&("T!A1".into(), "=S!A4+S!A1".into())),
        "{texts:?}"
    );
    assert_eq!(
        field(&res, "merges").to_json_string(),
        "[\"A1:B4\",\"D6:E7\"]"
    );
    assert_eq!(field(&res, "hiddenRows").to_json_string(), "[[3,4]]");
    assert_eq!(field(&res, "hiddenCols").to_json_string(), "[[4,4]]");
    assert!(field(&res, "rules").to_json_string().contains("=$B$4"));
    // Nothing a formula shows changed.
    assert!(field(&res, "changed").as_array().unwrap().is_empty());
}

#[test]
fn bad_requests_and_missing_sessions_are_refused() {
    let id = open_with(&[("S!A1", "1")]);
    for body in [
        format!("{{\"sessionAction\":\"structure\",\"session\":\"{id}\",\"sheet\":\"S\",\"axis\":\"diag\",\"op\":\"insert\",\"at\":1,\"count\":1}}"),
        format!("{{\"sessionAction\":\"structure\",\"session\":\"{id}\",\"sheet\":\"S\",\"axis\":\"row\",\"op\":\"insert\",\"at\":0,\"count\":1}}"),
        format!("{{\"sessionAction\":\"structure\",\"session\":\"{id}\",\"sheet\":\"S\",\"axis\":\"row\",\"op\":\"insert\",\"at\":1,\"count\":100000}}"),
        format!("{{\"sessionAction\":\"structure\",\"session\":\"{id}\",\"sheet\":\"Nope\",\"axis\":\"row\",\"op\":\"insert\",\"at\":1,\"count\":1}}"),
        format!("{{\"sessionAction\":\"structure\",\"session\":\"{id}\",\"sheet\":\"S\",\"axis\":\"row\",\"op\":\"insert\",\"at\":1,\"count\":1,\"merges\":[\"A1\"]}}"),
    ] {
        assert!(EasyOfficeIpcRouter::dispatch("evaluate_sheet", &body).is_err(), "{body}");
    }
    let res = call("{\"sessionAction\":\"structure\",\"session\":\"ssnone\",\"sheet\":\"S\",\"axis\":\"row\",\"op\":\"insert\",\"at\":1,\"count\":1}");
    assert_eq!(res.to_json_string(), "{\"expired\":true}");
}
