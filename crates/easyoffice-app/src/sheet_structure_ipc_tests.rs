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
fn row_heights_follow_a_row_edit_and_a_bad_height_changes_nothing() {
    let id = open_with(&[("S!A1", "1")]);
    let edit = |op: &str, at: u32, count: u32, heights: &str| {
        format!(
            r#"{{"sessionAction":"structure","session":"{id}","sheet":"S","axis":"row","op":"{op}","at":{at},"count":{count},"rowHeights":{heights}}}"#
        )
    };
    let heights = "[[2,30],[5,40.5],[7,21,0]]";
    // Rows 3 and 4 open below row 2 and take its height; the rest move down by two.
    let inserted = call(&edit("insert", 3, 2, heights));
    assert_eq!(
        field(&inserted, "rowHeights").to_json_string(),
        "[[2,30],[3,30],[4,30],[7,40.5],[9,21,0]]"
    );
    let deleted = call(&edit("delete", 4, 2, heights));
    assert_eq!(
        field(&deleted, "rowHeights").to_json_string(),
        "[[2,30],[5,21,0]]"
    );
    let cols = call(&format!(
        r#"{{"sessionAction":"structure","session":"{id}","sheet":"S","axis":"col","op":"insert","at":1,"count":1,"rowHeights":{heights}}}"#
    ));
    assert_eq!(field(&cols, "rowHeights").to_json_string(), heights);
    for bad in ["[[2,0]]", "[[2,410]]", "[[0,20]]", "[[2,\"20\"]]", "{}"] {
        assert!(
            EasyOfficeIpcRouter::dispatch("evaluate_sheet", &edit("insert", 1, 1, bad)).is_err(),
            "{bad}"
        );
    }
}

/// A picture moves with its cells and its pixels are worked out on the rows after the edit: the default row height of
/// the sheet is 30 pt (40 px) and row 4 is 60 pt (80 px) before the edit, row 5 after it (PD-299).
#[test]
fn pictures_are_placed_on_the_rows_the_edit_leaves() {
    let id = open_with(&[("S!A1", "1")]);
    let picture = concat!(
        r#"[{"id":"p","name":"n","alt":"","mime":"image/png","imageId":"x","anchor":{"#,
        r#""fromCol":1,"fromRow":4,"colOffEmu":0,"rowOffEmu":0,"toCol":3,"toRow":6,"toColOffEmu":0,"toRowOffEmu":0,"#,
        r#""leftPx":80,"topPx":0,"widthPx":160,"heightPx":80}}]"#
    );
    let edit = format!(
        r#"{{"sessionAction":"structure","session":"{id}","sheet":"S","axis":"row","op":"insert","at":2,"count":1,"defaultRowHeight":[30],"rowHeights":[[4,60]],"images":{picture}}}"#
    );
    let res = call(&edit);
    assert_eq!(
        field(&res, "rowHeights").to_json_string(),
        "[[5,60]]",
        "the tall row moved down with the edit"
    );
    let images = field(&res, "images").to_json_string();
    // Zero-based rows 0..=3 are 40 px and row 4 is 80 px, so row 5 starts at 240 px; rows 5 and 6 are 40 px each.
    assert!(images.contains(r#""fromRow":5"#), "{images}");
    assert!(images.contains(r#""toRow":7"#), "{images}");
    assert!(images.contains(r#""topPx":240"#), "{images}");
    assert!(images.contains(r#""heightPx":80"#), "{images}");
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

#[test]
fn the_filter_follows_the_edit_and_a_deleted_condition_column_asks_for_a_reapply() {
    let id = open_with(&[("S!A1", "1")]);
    let filter = r#"{"range":"B3:E12","columns":[{"col":2,"values":["x"]},{"col":4,"conditions":[{"text":"y*"}]}],"hidden":[[5,6],[10,10]]}"#;
    let insert = call(&format!(
        r#"{{"sessionAction":"structure","session":"{id}","sheet":"S","axis":"row","op":"insert","at":1,"count":2,"filter":{filter}}}"#
    ));
    let moved = field(&insert, "filter").to_json_string();
    assert!(moved.starts_with(r#"{"range":"B5:E14","#), "{moved}");
    assert!(moved.ends_with(r#""hidden":[[7,8],[12,12]]}"#), "{moved}");
    assert!(StrictJsonValue::get_field(insert.as_object().unwrap(), "filterReapply").is_none());
    let delete = call(&format!(
        r#"{{"sessionAction":"structure","session":"{id}","sheet":"S","axis":"col","op":"delete","at":2,"count":1,"filter":{filter}}}"#
    ));
    let kept = field(&delete, "filter").to_json_string();
    assert!(
        kept.starts_with(r#"{"range":"B3:D12","columns":[{"col":3,"#),
        "{kept}"
    );
    assert_eq!(field(&delete, "filterReapply").to_json_string(), "true");
    // A sheet without a filter sends `null` and gets no filter field back.
    let none = call(&format!(
        r#"{{"sessionAction":"structure","session":"{id}","sheet":"S","axis":"row","op":"insert","at":1,"count":1,"filter":null}}"#
    ));
    assert!(StrictJsonValue::get_field(none.as_object().unwrap(), "filter").is_none());
    let gone = call(&format!(
        r#"{{"sessionAction":"structure","session":"{id}","sheet":"S","axis":"row","op":"delete","at":3,"count":1,"filter":{filter}}}"#
    ));
    assert_eq!(field(&gone, "filter").to_json_string(), "null");
}

#[test]
fn the_groups_follow_the_edit_and_a_bad_outline_changes_nothing() {
    let id = open_with(&[("S!A1", "1")]);
    let outline = r#"{"rows":[[3,3,1],[4,5,2],[6,6,1]],"foldedRows":[[4,5]],"cols":[[2,3,1]]}"#;
    let inside = call(&format!(
        r#"{{"sessionAction":"structure","session":"{id}","sheet":"S","axis":"row","op":"insert","at":5,"count":2,"outline":{outline}}}"#
    ));
    // Two rows inserted between rows 4 and 5, both level 2 (the smaller of 2 and 2).
    assert_eq!(
        field(&inside, "outline").to_json_string(),
        r#"{"rows":[[3,3,1],[4,7,2],[8,8,1]],"cols":[[2,3,1]],"foldedRows":[[4,7]]}"#
    );
    let delete = call(&format!(
        r#"{{"sessionAction":"structure","session":"{id}","sheet":"S","axis":"row","op":"delete","at":3,"count":4,"outline":{outline}}}"#
    ));
    assert_eq!(
        field(&delete, "outline").to_json_string(),
        r#"{"cols":[[2,3,1]]}"#
    );
    let cols = call(&format!(
        r#"{{"sessionAction":"structure","session":"{id}","sheet":"S","axis":"col","op":"insert","at":1,"count":1,"outline":{outline}}}"#
    ));
    assert_eq!(
        field(&cols, "outline").to_json_string(),
        r#"{"rows":[[3,3,1],[4,5,2],[6,6,1]],"cols":[[3,4,1]],"foldedRows":[[4,5]]}"#
    );
    for bad in [
        r#"{"rows":[[1,3,9]]}"#,
        r#"{"rows":[[3,4,1]],"foldedRows":[[1,9]]}"#,
        r#"{"rows":[[1,3,1],[2,5,1]]}"#,
    ] {
        let body = format!(
            r#"{{"sessionAction":"structure","session":"{id}","sheet":"S","axis":"row","op":"insert","at":1,"count":1,"outline":{bad}}}"#
        );
        assert!(
            EasyOfficeIpcRouter::dispatch("evaluate_sheet", &body).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn a_row_edit_answers_the_in_workbook_link_targets_it_moves() {
    let id = open_with(&[("S!A1", "1")]);
    let res = call(&format!(
        r##"{{"sessionAction":"structure","session":"{id}","sheet":"S","axis":"row","op":"insert","at":2,"count":3,"linkTargets":{{"T":["#S!B5","#S!B1","#T!B5","https://a.b/","#Total"],"S":["#B7","#S!A1:A9"]}}}}"##
    ));
    assert_eq!(
        field(&res, "linkTargets").to_json_string(),
        r##"{"T":[["#S!B5","#S!B8"]],"S":[["#B7","#B10"],["#S!A1:A9","#S!A1:A12"]]}"##
    );
    let plain = call(&format!(
        r#"{{"sessionAction":"structure","session":"{id}","sheet":"S","axis":"row","op":"insert","at":2,"count":1}}"#
    ));
    assert!(StrictJsonValue::get_field(plain.as_object().unwrap(), "linkTargets").is_none());
}

#[test]
fn hostile_link_targets_are_refused_before_the_session_changes() {
    let id = open_with(&[("S!A1", "1")]);
    for bad in [
        r#""linkTargets":[1]"#,
        r#""linkTargets":{"S":"x"}"#,
        r#""linkTargets":{"S":[1]}"#,
    ] {
        let body = format!(
            r#"{{"sessionAction":"structure","session":"{id}","sheet":"S","axis":"row","op":"insert","at":1,"count":1,{bad}}}"#
        );
        assert!(
            EasyOfficeIpcRouter::dispatch("evaluate_sheet", &body).is_err(),
            "{bad}"
        );
    }
}
