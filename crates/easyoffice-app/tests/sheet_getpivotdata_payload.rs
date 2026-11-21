//! `GETPIVOTDATA` through the engine's commands (PD-292): the pivot tables of a request without a session and of a
//! session's `pivots` action reach the engine, the answers follow the spreadsheet program (the grand total, field and item pairs,
//! any cell of the table, `#REF!` for a reference outside a pivot table or an item that is not there), and hostile
//! payloads are refused before anything is registered.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{
    EasyOfficeError, StrictJsonValue, MAX_PIVOTS_PER_SHEET, MAX_PIVOT_AXIS_FIELDS,
    MAX_PIVOT_FILTER_VALUES, MAX_PIVOT_TEXT_BYTES, MAX_PIVOT_VALUE_FIELDS,
};
use easyoffice_sheet::MAX_REGISTERED_PIVOTS;

fn call(payload: &str) -> Result<String, EasyOfficeError> {
    EasyOfficeIpcRouter::dispatch("evaluate_sheet", payload)
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

/// The source rows: region, quarter, sales (the second `east` is in lower case on purpose).
const DATA: &str = r#"{"sheet":"Data","cell":"A1","value":"지역"},{"sheet":"Data","cell":"B1","value":"분기"},{"sheet":"Data","cell":"C1","value":"매출"},
    {"sheet":"Data","cell":"A2","value":"East"},{"sheet":"Data","cell":"B2","value":"Q1"},{"sheet":"Data","cell":"C2","value":"10"},
    {"sheet":"Data","cell":"A3","value":"east"},{"sheet":"Data","cell":"B3","value":"Q2"},{"sheet":"Data","cell":"C3","value":"20"},
    {"sheet":"Data","cell":"A4","value":"West"},{"sheet":"Data","cell":"B4","value":"Q1"},{"sheet":"Data","cell":"C4","value":"5"},
    {"sheet":"Data","cell":"A5","value":"West"},{"sheet":"Data","cell":"B5","value":"Q1"},{"sheet":"Data","cell":"C5","value":"7"},
    {"sheet":"Data","cell":"A6","value":"West"},{"sheet":"Data","cell":"B6","value":"Q2"},{"sheet":"Data","cell":"C6","value":"1"},
    {"sheet":"Data","cell":"A7","value":"North"},{"sheet":"Data","cell":"B7","value":"Q1"},{"sheet":"Data","cell":"C7","value":"100"}"#;

/// The pivot table of the sheet `Report`: A3:D6, region by quarter, the sum of the sales, the North left out.
const PIVOT: &str = r#"{"name":"피벗 테이블1","cacheId":1,"sourceSheet":"Data","sourceRange":"A1:C7","sourceTable":null,
    "hasHeader":true,"location":"A3:D6","fieldNames":["지역","분기","매출"],"rowFields":[0],"colFields":[1],
    "filterFields":[],"valueFields":[{"col":2,"agg":"sum","name":"합계: 매출"}],
    "filters":[{"col":0,"values":["East","West"],"blank":false}],"slicers":[]}"#;

/// The formulas and what the spreadsheet program answers for them (the North is filtered out of the table, so it has no data).
const FORMULAS: [(&str, &str); 11] = [
    (r#"=GETPIVOTDATA("합계: 매출",$A$3)"#, "43"),
    (r#"=GETPIVOTDATA("매출",$A$3)"#, "43"),
    (r#"=GETPIVOTDATA("합계: 매출",$C$5)"#, "43"),
    (r#"=GETPIVOTDATA("합계: 매출",$A$3,"지역","west")"#, "13"),
    (
        r#"=GETPIVOTDATA("합계: 매출",$A$3,"지역","East","분기","Q2")"#,
        "20",
    ),
    (r#"=GETPIVOTDATA("합계: 매출",$A$3,"분기","Q1")"#, "22"),
    (
        r#"=GETPIVOTDATA("합계: 매출",$A$3,"지역","North")"#,
        "#REF!",
    ),
    (r#"=GETPIVOTDATA("합계: 매출",$A$3,"지역","Mars")"#, "#REF!"),
    (r#"=GETPIVOTDATA("합계: 매출",$A$3,"매출","10")"#, "#REF!"),
    (r#"=GETPIVOTDATA("없는 필드",$A$3)"#, "#REF!"),
    (r#"=GETPIVOTDATA("합계: 매출",$H$20)"#, "#REF!"),
];

fn formula_cells() -> String {
    FORMULAS
        .iter()
        .enumerate()
        .map(|(i, (formula, _))| {
            format!(
                r#"{{"sheet":"Report","cell":"F{}","value":"{}"}}"#,
                i + 1,
                formula.replace('"', "\\\"")
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn targets() -> String {
    (1..=FORMULAS.len())
        .map(|i| format!(r#"{{"sheet":"Report","cell":"F{i}"}}"#))
        .collect::<Vec<_>>()
        .join(",")
}

fn values(answer: &str) -> Vec<String> {
    let marker = "\"targetValues\":[";
    let start = answer.find(marker).unwrap() + marker.len();
    let end = answer[start..].find(']').unwrap() + start;
    answer[start..end]
        .split(',')
        .map(|v| v.trim_matches('"').to_string())
        .collect()
}

fn expected() -> Vec<String> {
    FORMULAS.iter().map(|(_, v)| (*v).to_string()).collect()
}

fn stateless(pivots: &str) -> Result<String, EasyOfficeError> {
    call(&format!(
        r#"{{"sheet":"Report","sheets":["Data","Report"],"cells":[{DATA},{}],{pivots}"targets":[{}]}}"#,
        formula_cells(),
        targets()
    ))
}

#[test]
fn a_request_with_pivots_answers_the_way_the_documentation_says() {
    let answer = stateless(&format!(r#""pivots":{{"Report":[{PIVOT}]}},"#)).unwrap();
    assert_eq!(values(&answer), expected(), "{answer}");
}

#[test]
fn a_request_without_pivots_answers_ref_error() {
    for field in ["", r#""pivots":{},"#, r#""pivots":{"Report":[]},"#] {
        let answer = stateless(field).unwrap();
        assert!(
            values(&answer).iter().all(|v| v == "#REF!"),
            "{field}: {answer}"
        );
    }
}

#[test]
fn a_pivot_of_a_sheet_the_request_does_not_hold_is_left_out() {
    let answer = stateless(&format!(r#""pivots":{{"Gone":[{PIVOT}]}},"#)).unwrap();
    assert!(values(&answer).iter().all(|v| v == "#REF!"), "{answer}");
}

#[test]
fn a_pivot_without_a_readable_source_answers_ref_error() {
    // A table whose range the file did not keep, and a sheet that is not there.
    let table = PIVOT.replace(
        r#""sourceRange":"A1:C7","sourceTable":null"#,
        r#""sourceRange":"","sourceTable":"표1""#,
    );
    let other = PIVOT.replace(r#""sourceSheet":"Data""#, r#""sourceSheet":"Gone""#);
    for bad in [table, other] {
        let answer = stateless(&format!(r#""pivots":{{"Report":[{bad}]}},"#)).unwrap();
        assert!(values(&answer).iter().all(|v| v == "#REF!"), "{answer}");
    }
    // No source at all is not a pivot table.
    let none = PIVOT.replace(r#""sourceRange":"A1:C7""#, r#""sourceRange":"""#);
    let err = stateless(&format!(r#""pivots":{{"Report":[{none}]}},"#)).unwrap_err();
    assert!(matches!(err, EasyOfficeError::InvalidArgument(_)), "{err}");
}

#[test]
fn a_source_named_with_its_sheet_is_read_from_that_sheet() {
    let prefixed = PIVOT
        .replace(r#""sourceSheet":"Data""#, r#""sourceSheet":"Report""#)
        .replace(
            r#""sourceRange":"A1:C7""#,
            r#""sourceRange":"Data!$A$1:$C$7""#,
        );
    let answer = stateless(&format!(r#""pivots":{{"Report":[{prefixed}]}},"#)).unwrap();
    assert_eq!(values(&answer), expected(), "{answer}");
}

#[test]
fn a_pivot_reads_the_source_of_its_own_sheet_when_it_names_none() {
    let own = PIVOT
        .replace(r#""sourceSheet":"Data""#, r#""sourceSheet":"""#)
        .replace("A3:D6", "H3:K6");
    let cells = r#"{"sheet":"Report","cell":"A1","value":"지역"},{"sheet":"Report","cell":"B1","value":"분기"},{"sheet":"Report","cell":"C1","value":"매출"},
        {"sheet":"Report","cell":"A2","value":"East"},{"sheet":"Report","cell":"B2","value":"Q1"},{"sheet":"Report","cell":"C2","value":"4"},
        {"sheet":"Report","cell":"A3","value":"West"},{"sheet":"Report","cell":"B3","value":"Q1"},{"sheet":"Report","cell":"C3","value":"6"},
        {"sheet":"Report","cell":"F1","value":"=GETPIVOTDATA(\"합계: 매출\",$H$3)"}"#;
    let answer = call(&format!(
        r#"{{"sheet":"Report","sheets":["Report"],"cells":[{cells}],"pivots":{{"Report":[{own}]}},"targets":[{{"sheet":"Report","cell":"F1"}}]}}"#
    ))
    .unwrap();
    assert_eq!(values(&answer), ["10"], "{answer}");
}

#[test]
fn a_session_told_the_pivots_calculates_the_formulas_and_follows_the_source() {
    let opened = call(r#"{"sessionAction":"open","sheets":["Data","Report"]}"#).unwrap();
    let id = json_str_field(&opened, "session").to_string();
    let apply = call(&format!(
        r#"{{"sessionAction":"apply","session":"{id}","recalc":true,"changes":[{DATA},{}]}}"#,
        formula_cells()
    ))
    .unwrap();
    assert_eq!(
        changed_value(&apply, "F1").as_deref(),
        Some("#REF!"),
        "no pivot table is registered yet: {apply}"
    );
    let told = call(&format!(
        r#"{{"sessionAction":"pivots","session":"{id}","sheet":"Report","pivots":[{PIVOT}]}}"#
    ))
    .unwrap();
    for (i, (_, want)) in FORMULAS.iter().enumerate() {
        let cell = format!("F{}", i + 1);
        // A cell that stays `#REF!` is not reported again.
        if *want == "#REF!" {
            assert_eq!(changed_value(&told, &cell), None, "{cell}: {told}");
        } else {
            assert_eq!(
                changed_value(&told, &cell).as_deref(),
                Some(*want),
                "{cell}: {told}"
            );
        }
    }
    // A source cell changed: the pivot table is not worked out again by the page, the formulas follow it.
    let edited = call(&format!(
        r#"{{"sessionAction":"apply","session":"{id}","changes":[{{"sheet":"Data","cell":"C2","value":"110"}}]}}"#
    ))
    .unwrap();
    assert_eq!(
        changed_value(&edited, "F1").as_deref(),
        Some("143"),
        "{edited}"
    );
    assert_eq!(
        changed_value(&edited, "F6").as_deref(),
        Some("122"),
        "the first quarter: {edited}"
    );
    assert_eq!(
        changed_value(&edited, "F5"),
        None,
        "East in Q2 did not change: {edited}"
    );
    // The pivot table is deleted: the formulas answer `#REF!` again.
    let gone = call(&format!(
        r#"{{"sessionAction":"pivots","session":"{id}","sheet":"Report","pivots":[]}}"#
    ))
    .unwrap();
    assert_eq!(
        changed_value(&gone, "F1").as_deref(),
        Some("#REF!"),
        "{gone}"
    );
    let none = call(&format!(
        r#"{{"sessionAction":"pivots","session":"{id}","sheet":"Report","pivots":null}}"#
    ))
    .unwrap();
    assert_eq!(changed_value(&none, "F1"), None, "{none}");
    call(&format!(r#"{{"sessionAction":"close","session":"{id}"}}"#)).unwrap();
}

#[test]
fn a_session_refuses_pivots_of_a_sheet_it_does_not_hold() {
    let opened = call(r#"{"sessionAction":"open","sheets":["Data"]}"#).unwrap();
    let id = json_str_field(&opened, "session").to_string();
    let err = call(&format!(
        r#"{{"sessionAction":"pivots","session":"{id}","sheet":"Report","pivots":[{PIVOT}]}}"#
    ))
    .unwrap_err();
    assert!(matches!(err, EasyOfficeError::InvalidArgument(_)), "{err}");
    let gone =
        call(r#"{"sessionAction":"pivots","session":"nope","sheet":"Data","pivots":[]}"#).unwrap();
    assert!(gone.contains("expired"), "{gone}");
}

/// A pivot table a workbook file holds (the `pivotTable` part, decoded to the `pivots` of a sheet) is the pivot table
/// the page sends: it needs nothing more to answer.
#[test]
fn the_pivot_tables_of_a_decoded_workbook_answer() {
    let save = r#"{"targetFormat":"xlsx","title":"피벗","sheets":[
        {"name":"데이터","cells":[
            {"ref":"A1","raw":"부서"},{"ref":"B1","raw":"분기"},{"ref":"C1","raw":"매출"},
            {"ref":"A2","raw":"영업"},{"ref":"B2","raw":"1Q"},{"ref":"C2","raw":"500"},
            {"ref":"A3","raw":"영업"},{"ref":"B3","raw":"2Q"},{"ref":"C3","raw":"300"},
            {"ref":"A4","raw":"지원"},{"ref":"B4","raw":"1Q"},{"ref":"C4","raw":"70"}]},
        {"name":"요약","cells":[{"ref":"A1","raw":"요약"}],"pivots":[{
            "name":"피벗 테이블1","cacheId":1,"sourceSheet":"데이터","sourceRange":"A1:C4","sourceTable":null,
            "hasHeader":true,"location":"B2:D6","fieldNames":["부서","분기","매출"],"rowFields":[0],"colFields":[1],
            "filterFields":[],"valueFields":[{"col":2,"agg":"sum","name":"합계: 매출"}],
            "filters":[{"col":0,"values":["영업"],"blank":false}],"slicers":[]}]}]}"#;
    let saved = call_convert(save);
    let b64 = json_str_field(&saved, "convertedBase64").to_string();
    let decoded = call_convert(&format!(
        r#"{{"action":"decode","fileName":"pivot.xlsx","sourceBase64":"{b64}"}}"#
    ));
    let root = StrictJsonValue::parse(&decoded).unwrap();
    let sheets = StrictJsonValue::get_field(root.as_object().unwrap(), "sheets")
        .and_then(StrictJsonValue::as_array)
        .unwrap();
    let pivots = StrictJsonValue::get_field(sheets[1].as_object().unwrap(), "pivots")
        .unwrap()
        .to_json_string();
    let answer = call(&format!(
        r#"{{"sheet":"요약","sheets":["데이터","요약"],"cells":[
            {{"sheet":"데이터","cell":"A1","value":"부서"}},{{"sheet":"데이터","cell":"B1","value":"분기"}},{{"sheet":"데이터","cell":"C1","value":"매출"}},
            {{"sheet":"데이터","cell":"A2","value":"영업"}},{{"sheet":"데이터","cell":"B2","value":"1Q"}},{{"sheet":"데이터","cell":"C2","value":"500"}},
            {{"sheet":"데이터","cell":"A3","value":"영업"}},{{"sheet":"데이터","cell":"B3","value":"2Q"}},{{"sheet":"데이터","cell":"C3","value":"300"}},
            {{"sheet":"데이터","cell":"A4","value":"지원"}},{{"sheet":"데이터","cell":"B4","value":"1Q"}},{{"sheet":"데이터","cell":"C4","value":"70"}},
            {{"sheet":"요약","cell":"F1","value":"=GETPIVOTDATA(\"매출\",$B$2)"}},
            {{"sheet":"요약","cell":"F2","value":"=GETPIVOTDATA(\"합계: 매출\",$C$4,\"분기\",\"1Q\")"}},
            {{"sheet":"요약","cell":"F3","value":"=GETPIVOTDATA(\"매출\",$B$2,\"부서\",\"지원\")"}}],
          "pivots":{{"요약":{pivots}}},
          "targets":[{{"sheet":"요약","cell":"F1"}},{{"sheet":"요약","cell":"F2"}},{{"sheet":"요약","cell":"F3"}}]}}"#
    ))
    .unwrap();
    assert_eq!(values(&answer), ["800", "500", "#REF!"], "{answer}");
}

fn call_convert(payload: &str) -> String {
    EasyOfficeIpcRouter::dispatch("convert_document_format", payload).unwrap()
}

// ---- hostile payloads ------------------------------------------------------------------------------------------

fn over_budget(pivots: &str) {
    let err = stateless(&format!(r#""pivots":{pivots},"#)).unwrap_err();
    assert!(
        matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)),
        "{pivots:.200}: {err}"
    );
    let opened = call(r#"{"sessionAction":"open","sheets":["Data","Report"]}"#).unwrap();
    let id = json_str_field(&opened, "session").to_string();
    let list = &pivots[pivots.find('[').unwrap()..pivots.rfind(']').unwrap() + 1];
    let err = call(&format!(
        r#"{{"sessionAction":"pivots","session":"{id}","sheet":"Report","pivots":{list}}}"#
    ))
    .unwrap_err();
    assert!(
        matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)),
        "session: {err}"
    );
}

fn with(key: &str, from: &str, to: &str) -> String {
    let at = PIVOT.find(key).unwrap();
    let (head, tail) = PIVOT.split_at(at);
    format!("{head}{}", tail.replacen(from, to, 1))
}

fn numbers(count: usize) -> String {
    (0..count)
        .map(|i| (i % 3).to_string())
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn too_many_pivots_on_a_sheet_are_refused() {
    let list = vec![PIVOT; MAX_PIVOTS_PER_SHEET + 1].join(",");
    over_budget(&format!(r#"{{"Report":[{list}]}}"#));
}

#[test]
fn too_many_pivots_in_all_sheets_are_refused_before_any_is_registered() {
    // 32 on each of five sheets: each sheet is within its bound, the workbook is not.
    let per_sheet = vec![PIVOT; MAX_PIVOTS_PER_SHEET].join(",");
    let sheets = (0..=MAX_REGISTERED_PIVOTS / MAX_PIVOTS_PER_SHEET)
        .map(|i| format!(r#""S{i}":[{per_sheet}]"#))
        .collect::<Vec<_>>()
        .join(",");
    let err = stateless(&format!(r#""pivots":{{{sheets}}},"#)).unwrap_err();
    assert!(
        matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)),
        "{err}"
    );
}

#[test]
fn a_session_refuses_more_pivots_than_a_workbook_may_hold() {
    let names: Vec<String> = (0..=MAX_REGISTERED_PIVOTS / MAX_PIVOTS_PER_SHEET)
        .map(|i| format!("S{i}"))
        .collect();
    let list = names
        .iter()
        .map(|n| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join(",");
    let opened = call(&format!(r#"{{"sessionAction":"open","sheets":[{list}]}}"#)).unwrap();
    let id = json_str_field(&opened, "session").to_string();
    let per_sheet = vec![PIVOT; MAX_PIVOTS_PER_SHEET].join(",");
    let mut last = Ok(String::new());
    for name in &names {
        last = call(&format!(
            r#"{{"sessionAction":"pivots","session":"{id}","sheet":"{name}","pivots":[{per_sheet}]}}"#
        ));
    }
    assert!(
        matches!(last, Err(EasyOfficeError::ResourceBudgetExceeded(_))),
        "{last:?}"
    );
}

#[test]
fn a_pivot_over_a_bound_is_left_out_and_cannot_stop_the_other_formulas() {
    let value = r#"{"col":2,"agg":"sum","name":"x"}"#;
    let long_item = "가".repeat(MAX_PIVOT_TEXT_BYTES / 3 + 1);
    let big_filters = vec![
        format!(
            r#"{{"col":0,"values":[{}],"blank":false}}"#,
            vec![format!("\"{}\"", "x".repeat(MAX_PIVOT_TEXT_BYTES)); 200].join(",")
        );
        64
    ]
    .join(",");
    let many_items = vec!["\"v\""; MAX_PIVOT_FILTER_VALUES + 1].join(",");
    let bad = [
        // a name a page lets a user type to 255 characters: 171 Korean letters are over 512 bytes
        with("\"name\"", "피벗 테이블1", &"가".repeat(171)),
        // a filter item copied from a long cell
        with("\"filters\"", "\"East\"", &format!("\"{long_item}\"")),
        with("\"valueFields\"", "합계: 매출", &long_item),
        with("\"sourceSheet\"", "Data", &long_item),
        // lists over their bounds
        with(
            "\"rowFields\":[0]",
            "[0]",
            &format!("[{}]", numbers(MAX_PIVOT_AXIS_FIELDS + 1)),
        ),
        with(
            "\"valueFields\"",
            r#"[{"col":2,"agg":"sum","name":"합계: 매출"}]"#,
            &format!("[{}]", vec![value; MAX_PIVOT_VALUE_FIELDS + 1].join(",")),
        ),
        with(
            "\"filters\"",
            r#"["East","West"]"#,
            &format!("[{many_items}]"),
        ),
        // every bound kept and the table still over its share of the bytes
        with(
            "\"filters\"",
            r#"[{"col":0,"values":["East","West"],"blank":false}]"#,
            &format!("[{big_filters}]"),
        ),
    ];
    let extra = r#"{"sheet":"Report","cell":"F20","value":"=GETPIVOTDATA(\"합계: 매출\",$H$3)"},{"sheet":"Report","cell":"F21","value":"=1+2"}"#;
    let mut want = expected();
    want.extend(["#REF!".to_string(), "3".to_string()]);
    for variant in bad {
        let bad = variant.replace("A3:D6", "H3:I4");
        let answer = call(&format!(
            r#"{{"sheet":"Report","sheets":["Data","Report"],"cells":[{DATA},{},{extra}],
              "pivots":{{"Report":[{PIVOT},{bad}]}},
              "targets":[{},{{"sheet":"Report","cell":"F20"}},{{"sheet":"Report","cell":"F21"}}]}}"#,
            formula_cells(),
            targets()
        ))
        .unwrap();
        assert_eq!(values(&answer), want, "{variant:.120}");
        // a session takes the same list and answers for the other table
        let opened = call(r#"{"sessionAction":"open","sheets":["Data","Report"]}"#).unwrap();
        let id = json_str_field(&opened, "session").to_string();
        call(&format!(
            r#"{{"sessionAction":"apply","session":"{id}","recalc":true,"changes":[{DATA},{},{extra}]}}"#,
            formula_cells()
        ))
        .unwrap();
        let told = call(&format!(
            r#"{{"sessionAction":"pivots","session":"{id}","sheet":"Report","pivots":[{PIVOT},{bad}]}}"#
        ))
        .unwrap();
        assert_eq!(changed_value(&told, "F1").as_deref(), Some("43"), "{told}");
        assert_eq!(changed_value(&told, "F20"), None, "{variant:.120}: {told}");
    }
}

#[test]
fn malformed_pivots_are_refused() {
    for body in [
        r#"{"Report":{"name":"x"}}"#,
        r#"{"Report":["x"]}"#,
        r#"["x"]"#,
        r#"{"Report":[{"name":"x","location":"zz","sourceRange":"A1:C7"}]}"#,
        r#"{"Report":[{"name":"x","location":"A3:D6","rowFields":[-1]}]}"#,
        r#"{"Report":[{"name":"x","location":"A3:D6","valueFields":[{"col":2,"agg":"median"}]}]}"#,
    ] {
        let err = stateless(&format!(r#""pivots":{body},"#)).unwrap_err();
        assert!(
            matches!(
                err,
                EasyOfficeError::InvalidArgument(_) | EasyOfficeError::ResourceBudgetExceeded(_)
            ),
            "{body}: {err}"
        );
    }
}

#[test]
fn a_source_that_covers_the_whole_grid_is_bounded_by_the_evaluation_budget() {
    let huge = PIVOT.replace("A1:C7", "A1:XFD1048576");
    let started = std::time::Instant::now();
    let answer = stateless(&format!(r#""pivots":{{"Report":[{huge}]}},"#)).unwrap();
    assert!(
        started.elapsed() < std::time::Duration::from_secs(30),
        "the call took {:?}",
        started.elapsed()
    );
    assert!(
        values(&answer).iter().all(|v| v.starts_with('#')),
        "a source the engine will not read is an error value, not a number: {answer}"
    );
}

#[test]
fn field_numbers_beyond_the_source_are_refused() {
    for (key, from, to) in [
        (r#""rowFields""#, "[0]", "[16383]"),
        (r#""colFields""#, "[1]", "[3]"),
        (r#""valueFields""#, r#""col":2"#, r#""col":3"#),
        (r#""filters""#, r#""col":0"#, r#""col":9"#),
    ] {
        let wide = with(key, from, to);
        let err = stateless(&format!(r#""pivots":{{"Report":[{wide}]}},"#)).unwrap_err();
        assert!(
            matches!(err, EasyOfficeError::InvalidArgument(_)),
            "{key}: {err}"
        );
    }
}
