//! PD-303: a pivot table the engine leaves out for its size is named in the answer of the session's `pivots` action, so the
//! page can say why a `GETPIVOTDATA` that points into it answers `#REF!`. A table that is left out for another reason (no
//! readable source) is not named, and a list of readable tables names nothing.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{MAX_PIVOT_FILTER_VALUES, MAX_PIVOT_TEXT_BYTES};

fn call(payload: &str) -> String {
    EasyOfficeIpcRouter::dispatch("evaluate_sheet", payload).unwrap()
}

fn session() -> String {
    let opened = call(r#"{"sessionAction":"open","sheets":["Data","Report"]}"#);
    let marker = "\"session\":\"";
    let start = opened.find(marker).unwrap() + marker.len();
    let end = opened[start..].find('"').unwrap() + start;
    opened[start..end].to_string()
}

fn pivot(location: &str, name: &str, source: &str, filter_items: &str) -> String {
    format!(
        r#"{{"name":"{name}","cacheId":1,"sourceSheet":"Data","sourceRange":"{source}","sourceTable":null,
        "hasHeader":true,"location":"{location}","fieldNames":["a","b","c"],"rowFields":[0],"colFields":[1],
        "filterFields":[],"valueFields":[{{"col":2,"agg":"sum","name":"sum"}}],
        "filters":[{{"col":0,"values":[{filter_items}],"blank":false}}],"slicers":[]}}"#
    )
}

fn tell(id: &str, pivots: &[String]) -> String {
    call(&format!(
        r#"{{"sessionAction":"pivots","session":"{id}","sheet":"Report","pivots":[{}]}}"#,
        pivots.join(",")
    ))
}

fn omitted(answer: &str) -> Option<&str> {
    let marker = "\"pivotsOmitted\":[";
    let start = answer.find(marker)? + marker.len();
    Some(&answer[start..start + answer[start..].find(']')?])
}

#[test]
fn readable_pivots_name_nothing() {
    let id = session();
    let answer = tell(&id, &[pivot("A3:D6", "p", "A1:C7", r#""East""#)]);
    assert_eq!(omitted(&answer), None, "{answer}");
}

#[test]
fn a_pivot_over_a_text_bound_is_named_by_its_location() {
    let id = session();
    let long_name = "가".repeat(171);
    let answer = tell(
        &id,
        &[
            pivot("A3:D6", "ok", "A1:C7", r#""East""#),
            pivot("H3:I4", &long_name, "A1:C7", r#""East""#),
        ],
    );
    assert_eq!(omitted(&answer), Some("\"H3:I4\""), "{answer}");
}

/// A filter item copied from a long cell (300 letters are 900 bytes), the way the page reaches the engine.
#[test]
fn a_pivot_with_a_long_filter_item_is_named_by_its_location() {
    let id = session();
    let items = format!(r#""East","West","{}""#, "가".repeat(300));
    let answer = tell(
        &id,
        &[
            pivot("A3:D6", "ok", "A1:C7", r#""East""#),
            pivot("A20:D23", "long", "A1:C7", &items),
        ],
    );
    assert_eq!(omitted(&answer), Some("\"A20:D23\""), "{answer}");
}

#[test]
fn a_pivot_over_a_list_bound_is_named_by_its_location() {
    let id = session();
    let items = vec!["\"v\""; MAX_PIVOT_FILTER_VALUES + 1].join(",");
    let answer = tell(&id, &[pivot("H3:I4", "many", "A1:C7", &items)]);
    assert_eq!(omitted(&answer), Some("\"H3:I4\""), "{answer}");
}

/// Every bound is kept and the table is still over its share of the bytes (128 KB): the engine leaves it out as well.
#[test]
fn a_pivot_over_its_share_of_the_bytes_is_named_by_its_location() {
    let id = session();
    let item = format!("\"{}\"", "x".repeat(MAX_PIVOT_TEXT_BYTES));
    let filters = vec![
        format!(
            r#"{{"col":0,"values":[{}],"blank":false}}"#,
            vec![item; 200].join(",")
        );
        64
    ]
    .join(",");
    let big = pivot("H3:I4", "big", "A1:C7", r#""East""#).replacen(
        r#""filters":[{"col":0,"values":["East"],"blank":false}]"#,
        &format!(r#""filters":[{filters}]"#),
        1,
    );
    let answer = tell(&id, &[pivot("A3:D6", "ok", "A1:C7", r#""East""#), big]);
    assert_eq!(omitted(&answer), Some("\"H3:I4\""), "{answer}");
}

/// A table with no readable source is not a size problem: the page has nothing to say about it.
#[test]
fn a_pivot_without_a_readable_source_is_not_named() {
    let id = session();
    let table = pivot("H3:I4", "table", "A1:C7", r#""East""#).replacen(
        r#""sourceRange":"A1:C7","sourceTable":null"#,
        r#""sourceRange":"","sourceTable":"표1""#,
        1,
    );
    let answer = tell(&id, &[table]);
    assert_eq!(omitted(&answer), None, "{answer}");
}

/// The names of two tables over a bound come back in the order of the list.
#[test]
fn every_omitted_pivot_is_named_in_order() {
    let id = session();
    let long_name = "가".repeat(171);
    let answer = tell(
        &id,
        &[
            pivot("A3:D6", &long_name, "A1:C7", r#""East""#),
            pivot("F3:G4", "ok", "A1:C7", r#""East""#),
            pivot("H3:I4", &long_name, "A1:C7", r#""East""#),
        ],
    );
    assert_eq!(omitted(&answer), Some("\"A3:D6\",\"H3:I4\""), "{answer}");
}

fn stateless_base_cells() -> &'static str {
    concat!(
        r#"[{"sheet":"Data","cell":"A1","value":"a"},{"sheet":"Data","cell":"B1","value":"b"},{"sheet":"Data","cell":"C1","value":"c"},"#,
        r#"{"sheet":"Data","cell":"A2","value":"East"},{"sheet":"Data","cell":"B2","value":"Q1"},{"sheet":"Data","cell":"C2","value":10},"#,
        r#"{"sheet":"Data","cell":"A3","value":"East"},{"sheet":"Data","cell":"B3","value":"Q1"},{"sheet":"Data","cell":"C3","value":20},"#,
        r#"{"sheet":"Report","cell":"H9","value":"=GETPIVOTDATA(\"sum\",$F$3)"},"#,
        r#"{"sheet":"Report","cell":"H10","value":"=GETPIVOTDATA(\"sum\",피벗이름)"},"#,
        r#"{"sheet":"Report","cell":"H11","value":"=GETPIVOTDATA(\"sum\",$A$3)"}]"#,
    )
}

#[test]
fn stateless_requests_report_omitted_pivots_and_evaluate_direct_and_named_refs() {
    let long_name = "가".repeat(171);
    let ok = pivot("A3:D6", "ok", "A1:C3", r#""East""#);
    let big = pivot("F3:I6", &long_name, "A1:C3", r#""East""#);
    let cells = stateless_base_cells();
    let targets_payload = format!(
        r#"{{"sheet":"Report","sheets":["Data","Report"],"targets":[{{"sheet":"Report","cell":"H9"}},{{"sheet":"Report","cell":"H10"}},{{"sheet":"Report","cell":"H11"}}],"cells":{cells},"names":[{{"name":"피벗이름","refersTo":"Report!$F$3:$I$6"}}],"pivots":{{"Report":[{ok},{big}]}}}}"#
    );
    let answer = call(&targets_payload);
    assert!(
        answer.contains(r#""targetValues":["#)
            && answer.contains(r#""pivotsOmitted":{"Report":["F3:I6"]}"#),
        "{answer}"
    );
    assert!(
        answer.contains(r##""targetValues":["#REF!","#REF!","30"]"##),
        "{answer}"
    );

    let expr_payload = format!(
        r#"{{"sheet":"Report","sheets":["Data","Report"],"expr":"H10","cells":{cells},"names":[{{"name":"피벗이름","refersTo":"Report!$F$3:$I$6"}}],"pivots":{{"Report":[{ok},{big}]}}}}"#
    );
    let single = call(&expr_payload);
    assert!(single.contains(r##""value":"#REF!""##), "{single}");
    assert!(
        single.contains(r#""pivotsOmitted":{"Report":["F3:I6"]}"#),
        "{single}"
    );
}

#[test]
fn stateless_request_without_size_omissions_omits_pivots_omitted_field() {
    let ok = pivot("A3:D6", "ok", "A1:C3", r#""East""#);
    let no_source = pivot("F3:I6", "table", "A1:C3", r#""East""#).replacen(
        r#""sourceRange":"A1:C3","sourceTable":null"#,
        r#""sourceRange":"","sourceTable":"표1""#,
        1,
    );
    let cells = stateless_base_cells();
    let payload = format!(
        r#"{{"sheet":"Report","sheets":["Data","Report"],"targets":[{{"sheet":"Report","cell":"H11"}}],"cells":{cells},"pivots":{{"Report":[{ok},{no_source}]}}}}"#
    );
    let answer = call(&payload);
    assert!(!answer.contains("pivotsOmitted"), "{answer}");
    assert!(answer.contains(r#""targetValues":["30"]"#), "{answer}");
}

#[test]
fn stateless_requests_canonicalize_pivot_sheet_name_case() {
    let long_name = "가".repeat(171);
    let big = pivot("H3:I4", &long_name, "A1:C3", r#""East""#);
    let cells = stateless_base_cells();
    let payload = format!(
        r#"{{"sheet":"Report","sheets":["Data","Report"],"targets":[{{"sheet":"Report","cell":"H9"}}],"cells":{cells},"pivots":{{"report":[{big}]}}}}"#
    );
    let answer = call(&payload);
    assert!(
        answer.contains(r#""pivotsOmitted":{"Report":["H3:I4"]}"#),
        "{answer}"
    );
}
