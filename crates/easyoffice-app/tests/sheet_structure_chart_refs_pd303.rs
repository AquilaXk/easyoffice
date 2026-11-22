//! PD-303: a row or column edit of sheet `S` also moves the data ranges and series references that the charts of the other
//! sheets point at `S` with (`S!$B$2:$B$5`). The page sends `chartRefs` `{sheet: [{id, dataRange, series: [...]}]}` of the
//! other sheets; the answer names the sheets whose charts changed. The rule is the one a formula follows, a reference the
//! edit deletes becomes `#REF!`, and a bad request is refused before the session changes.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{StrictJsonValue, MAX_CHARTS_PER_SHEET, MAX_CHART_TEXT_BYTES};

fn dispatch(body: &str) -> Result<StrictJsonValue, easyoffice_formats::EasyOfficeError> {
    EasyOfficeIpcRouter::dispatch("evaluate_sheet", body)
        .map(|res| StrictJsonValue::parse(&res).unwrap())
}

fn open() -> String {
    let res = dispatch(r#"{"sessionAction":"open","sheets":["S","T","My Sheet"]}"#).unwrap();
    res.as_object().unwrap()[0].1.as_str().unwrap().to_string()
}

fn edit(id: &str, axis: &str, op: &str, at: u32, count: u32, extra: &str) -> String {
    format!(
        r#"{{"sessionAction":"structure","session":"{id}","sheet":"S","axis":"{axis}","op":"{op}","at":{at},"count":{count},{extra}}}"#
    )
}

fn series(name_ref: &str, categories_ref: &str, values_ref: &str) -> String {
    format!(
        r#"{{"nameRef":{name_ref},"categoriesRef":{categories_ref},"valuesRef":"{values_ref}"}}"#
    )
}

fn chart(id: &str, data_range: &str, series: &[String]) -> String {
    format!(
        r#"{{"id":"{id}","dataRange":"{data_range}","series":[{}]}}"#,
        series.join(",")
    )
}

fn refs_of(res: &StrictJsonValue, sheet: &str) -> Option<String> {
    let all = StrictJsonValue::get_field(res.as_object().unwrap(), "chartRefs")?;
    StrictJsonValue::get_field(all.as_object()?, sheet).map(StrictJsonValue::to_json_string)
}

#[test]
fn a_row_insert_moves_the_ranges_that_point_at_the_edited_sheet() {
    let id = open();
    let t = chart(
        "c1",
        "S!$A$1:$B$5",
        &[series(r#""S!$B$1""#, r#""S!$A$2:$A$5""#, "S!$B$2:$B$5")],
    );
    let res = dispatch(&edit(
        &id,
        "row",
        "insert",
        3,
        2,
        &format!(r#""chartRefs":{{"T":[{t}]}}"#),
    ))
    .unwrap();
    let moved = refs_of(&res, "T").expect("T changed");
    // Row 3 is inserted: the rows from 3 on move down two, the range that starts above grows.
    assert!(moved.contains(r#""dataRange":"S!$A$1:$B$7""#), "{moved}");
    assert!(moved.contains(r#""nameRef":"S!$B$1""#), "{moved}");
    assert!(
        moved.contains(r#""categoriesRef":"S!$A$2:$A$7""#),
        "{moved}"
    );
    assert!(moved.contains(r#""valuesRef":"S!$B$2:$B$7""#), "{moved}");
    assert!(moved.contains(r#""id":"c1""#), "{moved}");
}

#[test]
fn a_column_edit_moves_them_too() {
    let id = open();
    let t = chart("c1", "S!A1:C5", &[series("null", "null", "S!$C$2:$C$5")]);
    let res = dispatch(&edit(
        &id,
        "col",
        "delete",
        1,
        1,
        &format!(r#""chartRefs":{{"T":[{t}]}}"#),
    ))
    .unwrap();
    let moved = refs_of(&res, "T").unwrap();
    assert!(moved.contains(r#""dataRange":"S!A1:B5""#), "{moved}");
    assert!(moved.contains(r#""valuesRef":"S!$B$2:$B$5""#), "{moved}");
    assert!(moved.contains(r#""nameRef":null"#), "{moved}");
}

#[test]
fn a_reference_the_edit_deletes_becomes_a_ref_error() {
    let id = open();
    let t = chart("c1", "S!$B$2:$B$5", &[series("null", "null", "S!$B$3")]);
    let res = dispatch(&edit(
        &id,
        "row",
        "delete",
        3,
        1,
        &format!(r#""chartRefs":{{"T":[{t}]}}"#),
    ))
    .unwrap();
    let moved = refs_of(&res, "T").unwrap();
    assert!(moved.contains(r#""dataRange":"S!$B$2:$B$4""#), "{moved}");
    assert!(moved.contains(r##""valuesRef":"#REF!""##), "{moved}");
}

#[test]
fn a_quoted_sheet_name_and_the_case_of_the_name_are_read_as_a_formula_reads_them() {
    let id = open();
    let t = chart(
        "c1",
        "s!$A$1:$A$3",
        &[series("null", "null", "'S'!$A$2:$A$3")],
    );
    let res = dispatch(&edit(
        &id,
        "row",
        "insert",
        1,
        1,
        &format!(r#""chartRefs":{{"T":[{t}]}}"#),
    ))
    .unwrap();
    let moved = refs_of(&res, "T").unwrap();
    assert!(moved.contains(r#""dataRange":"s!$A$2:$A$4""#), "{moved}");
    assert!(moved.contains(r#""valuesRef":"'S'!$A$3:$A$4""#), "{moved}");
}

/// A reference without a sheet name points at the sheet that holds the chart, and a reference to another sheet is not
/// the edited sheet's: neither moves. Only the sheets with a changed reference are in the answer.
#[test]
fn references_to_other_sheets_stay_and_unchanged_sheets_are_not_named() {
    let id = open();
    let own = chart("a", "A1:B5", &[series("null", "null", "B2:B5")]);
    let other = chart(
        "b",
        "'My Sheet'!$A$1:$B$5",
        &[series("null", "null", "T!$B$2:$B$5")],
    );
    let moved = chart("c", "S!$A$1:$A$5", &[series("null", "null", "S!$A$2:$A$5")]);
    let body = edit(
        &id,
        "row",
        "insert",
        1,
        1,
        &format!(r#""chartRefs":{{"T":[{own},{other}],"My Sheet":[{moved}]}}"#),
    );
    let res = dispatch(&body).unwrap();
    assert_eq!(refs_of(&res, "T"), None, "{}", res.to_json_string());
    assert!(refs_of(&res, "My Sheet")
        .unwrap()
        .contains(r#""dataRange":"S!$A$2:$A$6""#));
    // A request that carries the field and nothing changes answers an empty object.
    let id = open();
    let none = dispatch(&edit(
        &id,
        "row",
        "insert",
        1,
        1,
        &format!(r#""chartRefs":{{"T":[{own}]}}"#),
    ))
    .unwrap();
    assert_eq!(
        StrictJsonValue::get_field(none.as_object().unwrap(), "chartRefs")
            .unwrap()
            .to_json_string(),
        "{}"
    );
}

/// The charts of the edited sheet are still `charts`; a request that names the edited sheet in `chartRefs` would move them twice.
#[test]
fn the_edited_sheet_is_not_a_member_of_chart_refs() {
    let id = open();
    let c = chart("c", "S!$A$1:$A$5", &[]);
    for name in ["S", "s"] {
        let body = edit(
            &id,
            "row",
            "insert",
            1,
            1,
            &format!(r#""chartRefs":{{"{name}":[{c}]}}"#),
        );
        assert!(dispatch(&body).is_err(), "{name}");
    }
}

#[test]
fn a_hostile_chart_refs_is_refused_before_the_session_changes() {
    let id = open();
    let ok = chart(
        "ok",
        "S!$A$1:$A$5",
        &[series("null", "null", "S!$A$2:$A$5")],
    );
    let long = "x".repeat(MAX_CHART_TEXT_BYTES + 1);
    let many_charts = vec![ok.clone(); MAX_CHARTS_PER_SHEET + 1].join(",");
    let many_series = vec![series("null", "null", "S!$A$2"); 33].join(",");
    let many_sheets = (0..257)
        .map(|i| format!(r#""sheet{i}":[{ok}]"#))
        .collect::<Vec<_>>()
        .join(",");
    // 17 sheets of 64 charts are 1,088 charts in all, over the 1,024 a workbook may send.
    let many_in_all = (0..17)
        .map(|i| {
            format!(
                r#""sheet{i}":[{}]"#,
                vec![ok.clone(); MAX_CHARTS_PER_SHEET].join(",")
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let bad = [
        r#"[]"#.to_string(),
        r#""T""#.to_string(),
        r#"{"T":{"id":"x"}}"#.to_string(),
        r#"{"T":[7]}"#.to_string(),
        r#"{"T":[{"id":"x","dataRange":7}]}"#.to_string(),
        r#"{"T":[{"id":"x","series":{}}]}"#.to_string(),
        r#"{"T":[{"id":"x","series":[7]}]}"#.to_string(),
        r#"{"T":[{"id":"x","series":[{"valuesRef":7}]}]}"#.to_string(),
        r#"{"T":[{"id":"","dataRange":""}]}"#.to_string(),
        format!(r#"{{"T":[{{"id":"x","dataRange":"{long}"}}]}}"#),
        format!(r#"{{"T":[{{"id":"x","series":[{{"valuesRef":"{long}"}}]}}]}}"#),
        format!(r#"{{"T":[{many_charts}]}}"#),
        format!(r#"{{"T":[{{"id":"x","series":[{many_series}]}}]}}"#),
        format!("{{{many_sheets}}}"),
        format!("{{{many_in_all}}}"),
    ];
    for field in bad {
        let body = edit(
            &id,
            "row",
            "insert",
            1,
            1,
            &format!(r#""chartRefs":{field}"#),
        );
        assert!(dispatch(&body).is_err(), "{:.100}", field);
    }
    // The session answers still: no refused edit changed it.
    let res = dispatch(&edit(
        &id,
        "row",
        "insert",
        1,
        1,
        &format!(r#""chartRefs":{{"T":[{ok}]}}"#),
    ))
    .unwrap();
    assert!(refs_of(&res, "T")
        .unwrap()
        .contains(r#""dataRange":"S!$A$2:$A$6""#));
}

/// The ranges that a request names are bounded in points as the charts of a sheet are (a range of the whole sheet is no
/// smaller a request than it is for `charts`).
#[test]
fn a_range_over_the_point_bound_is_refused() {
    let id = open();
    let big = chart(
        "big",
        "S!$A$1:$A$5",
        &[series("null", "null", "S!$A$1:$A$1000000")],
    );
    let body = edit(
        &id,
        "row",
        "insert",
        1,
        1,
        &format!(r#""chartRefs":{{"T":[{big}]}}"#),
    );
    assert!(dispatch(&body).is_err());
}

/// An edit with no `chartRefs` answers as before.
#[test]
fn a_request_without_chart_refs_has_no_answer_for_them() {
    let id = open();
    let res = dispatch(&edit(&id, "row", "insert", 1, 1, r#""charts":[]"#)).unwrap();
    assert_eq!(
        StrictJsonValue::get_field(res.as_object().unwrap(), "chartRefs"),
        None
    );
}
