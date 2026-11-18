use super::*;
use easyoffice_formats::CellBox;

fn call(body: &str) -> Result<String, EasyOfficeError> {
    EasyOfficeIpcRouter::dispatch("evaluate_sheet", body)
}

const FILTER: &str = r#"{"range":"A1:B7","columns":[{"col":1,"values":["영업","개발"]},{"col":2,"conditions":[{"op":"greaterThan","text":"10"}]}]}"#;
const DATA: &str = r#"[{"col":1,"values":["영업","영업","개발","","기획","영업"]},{"col":2,"values":["5","20","30","40","50","abc"]}]"#;

#[test]
fn apply_answers_the_hidden_rows_one_based() {
    let json = call(&format!(
        r#"{{"sessionAction":"filter","op":"apply","filter":{FILTER},"data":{DATA}}}"#
    ))
    .unwrap();
    // Data rows 1..6 are sheet rows 2..7: row 2 (5 <= 10), 5 (blank), 6 (기획), 7 ("abc").
    assert_eq!(json, r#"{"hidden":[[2,2],[5,7]],"count":4}"#);
}

#[test]
fn values_leave_out_the_rows_other_columns_hide() {
    let json = call(&format!(
        r#"{{"sessionAction":"filter","op":"values","col":1,"filter":{FILTER},"data":{DATA}}}"#
    ))
    .unwrap();
    // Column B hides rows 2 (5) and 7 ("abc"): the rest are 영업, 개발, "", 기획.
    assert_eq!(
        json,
        r#"{"items":[["개발",1],["기획",1],["영업",1]],"blank":1,"more":false}"#
    );
    let searched = call(&format!(
        r#"{{"sessionAction":"filter","op":"values","col":1,"query":"영","filter":{FILTER},"data":{DATA}}}"#
    ))
    .unwrap();
    assert_eq!(searched, r#"{"items":[["영업",1]],"blank":0,"more":false}"#);
}

#[test]
fn the_displayed_text_is_judged_not_the_stored_one() {
    let filter = r#"{"range":"A1:A3","columns":[{"col":1,"values":["2023-03-15"]}]}"#;
    let data = r#"[{"col":1,"values":["45000","45001"],"fmt":[[0,1,"yyyy-mm-dd"]]}]"#;
    let json = call(&format!(
        r#"{{"sessionAction":"filter","op":"apply","filter":{filter},"data":{data}}}"#
    ))
    .unwrap();
    assert_eq!(json, r#"{"hidden":[[3,3]],"count":1}"#);
}

#[test]
fn opening_tells_the_filter_s_rows_from_the_user_s() {
    let mut filter = SheetFilter::new(CellBox::parse("A1:A5").unwrap());
    filter.columns = vec![FilterColumn {
        col: 0,
        criteria: FilterCriteria::Values {
            blank: false,
            values: vec!["영업".into()],
            dates: Vec::new(),
        },
    }];
    let styling = SheetStyling {
        filter: Some(filter),
        hidden_rows: vec![(2, 3), (7, 7)],
        ..SheetStyling::default()
    };
    let cells: Vec<(u32, u32, String)> = ["부서", "영업", "개발", "영업", "개발"]
        .iter()
        .enumerate()
        .map(|(i, t)| (i as u32, 0, t.to_string()))
        .collect();
    let split = split_for_open(&cells, &styling).unwrap();
    // Row 3 (index 2) is 개발 and hidden by the filter; row 4 is 영업 and the user's.
    assert_eq!(split.filter.unwrap().hidden, vec![(2, 2)]);
    assert_eq!(split.hidden, vec![(3, 3), (7, 7)]);
    assert!(split_for_open(&cells, &SheetStyling::default()).is_none());
}

#[test]
fn opening_judges_a_colour_condition_against_the_colours_of_the_cells() {
    use easyoffice_formats::CellStyle;
    let mut filter = SheetFilter::new(CellBox::parse("A1:A6").unwrap());
    filter.columns = vec![FilterColumn {
        col: 0,
        criteria: FilterCriteria::Color {
            font: false,
            color: Some(0xFFC7CE),
        },
    }];
    let pink = CellStyle {
        fill: Some(0xFFC7CE),
        ..CellStyle::default()
    };
    let green = CellStyle {
        fill: Some(0x00FF00),
        ..CellStyle::default()
    };
    let mut styling = SheetStyling {
        filter: Some(filter),
        hidden_rows: vec![(2, 3), (5, 5)],
        ..SheetStyling::default()
    };
    styling.cells.insert((1, 0), pink);
    styling.cells.insert((2, 0), green);
    styling.cells.insert((4, 0), pink);
    styling.cells.insert((5, 0), green);
    let cells: Vec<(u32, u32, String)> = ["부서", "a", "b", "c"]
        .iter()
        .enumerate()
        .map(|(i, t)| (i as u32, 0, t.to_string()))
        .collect();
    let split = split_for_open(&cells, &styling).unwrap();
    assert_eq!(split.filter.unwrap().hidden, vec![(2, 3), (5, 5)]);
    assert!(split.hidden.is_empty());
    styling.cells.clear();
    let split = split_for_open(&cells, &styling).unwrap();
    assert_eq!(split.filter.unwrap().hidden, vec![(2, 3), (5, 5)]);
}

/// 100,000 rows of 3 columns, a sheet filter over all of them and 300 tables of 100 rows each, every
/// table with a condition: the time `split_for_open` takes (`--ignored --nocapture`, release).
#[test]
#[ignore = "measurement"]
fn opening_a_sheet_with_a_filter_and_many_filtered_tables() {
    let rows = 100_000u32;
    let cells: Vec<(u32, u32, String)> = (0..rows)
        .flat_map(|r| (0..3).map(move |c| (r, c, format!("v{}", r % 7))))
        .collect();
    let on = |col| FilterColumn {
        col,
        criteria: FilterCriteria::Values {
            blank: false,
            values: vec!["v1".into()],
            dates: Vec::new(),
        },
    };
    let mut filter = SheetFilter::new(CellBox::parse(&format!("A1:C{rows}")).unwrap());
    filter.columns = vec![on(0)];
    let tables: Vec<SheetTable> = (0..300u32)
        .map(|i| {
            let first = i * 100 + 1;
            let mut t = SheetTable::new(
                format!("T{i}"),
                CellBox::parse(&format!("H{first}:J{}", first + 99)).unwrap(),
            );
            t.filter = Some(vec![on(7)]);
            t
        })
        .collect();
    let styling = SheetStyling {
        filter: Some(filter),
        tables,
        hidden_rows: vec![(2, 50_000)],
        ..SheetStyling::default()
    };
    let start = std::time::Instant::now();
    let split = split_for_open(&cells, &styling).unwrap();
    println!("split_for_open: {:?}", start.elapsed());
    assert!(split.filter.is_some());
}

#[test]
fn rows_a_file_marks_as_the_filter_s_are_kept_whatever_the_conditions_say() {
    let mut filter = SheetFilter::new(CellBox::parse("A1:A5").unwrap());
    filter.columns = vec![FilterColumn {
        col: 0,
        criteria: FilterCriteria::Values {
            blank: false,
            values: vec!["영업".into()],
            dates: Vec::new(),
        },
    }];
    filter.hidden = vec![(4, 4)];
    let mut table = SheetTable::new("T", CellBox::parse("C1:C5").unwrap());
    table.filter = filter.columns.clone().into();
    table.filter_hidden = vec![(3, 3)];
    let styling = SheetStyling {
        filter: Some(filter),
        tables: vec![table],
        hidden_rows: vec![(2, 2)],
        ..SheetStyling::default()
    };
    let cells: Vec<(u32, u32, String)> = ["부서", "영업", "개발", "영업", "개발"]
        .iter()
        .enumerate()
        .map(|(i, t)| (i as u32, 0, t.to_string()))
        .collect();
    let split = split_for_open(&cells, &styling).unwrap();
    assert_eq!(split.filter.unwrap().hidden, vec![(4, 4)]);
    assert_eq!(split.tables[0].filter_hidden, vec![(3, 3)]);
    assert_eq!(split.hidden, vec![(2, 2)]);
}

fn apply(filter: &str, data: &str) -> Result<String, EasyOfficeError> {
    call(&format!(
        r#"{{"sessionAction":"filter","op":"apply","filter":{filter},"data":{data}}}"#
    ))
}

#[test]
fn top_and_average_judge_the_numbers_of_the_visible_rows() {
    let data = r#"[{"col":1,"values":["a","a","b","a","a"]},{"col":2,"values":["10","99","30","20","x"]}]"#;
    let top =
        r#"{"range":"A1:B6","columns":[{"col":1,"values":["a"]},{"col":2,"top":{"count":2}}]}"#;
    assert_eq!(
        apply(top, data).unwrap(),
        r#"{"hidden":[[2,2],[4,4],[6,6]],"count":3}"#
    );
    let below = r#"{"range":"A1:B6","columns":[{"col":2,"average":{"above":false}}]}"#;
    assert_eq!(
        apply(below, data).unwrap(),
        r#"{"hidden":[[3,3],[6,6]],"count":2}"#
    );
    let bottom = r#"{"range":"A1:B6","columns":[{"col":2,"top":{"bottom":true,"percent":true,"count":50}}]}"#;
    assert_eq!(
        apply(bottom, data).unwrap(),
        r#"{"hidden":[[3,4],[6,6]],"count":3}"#
    );
}

#[test]
fn a_colour_criterion_reads_the_colour_runs_and_a_date_group_the_format_runs() {
    let fill = r#"[{"col":1,"values":["a","b","c","d"],"fill":[[0,0,"ffc7ce"],[2,3,"ffc7ce"]]}]"#;
    let pink = r#"{"range":"A1:A5","columns":[{"col":1,"color":{"rgb":"ffc7ce"}}]}"#;
    assert_eq!(
        apply(pink, fill).unwrap(),
        r#"{"hidden":[[3,3]],"count":1}"#
    );
    let none = r#"{"range":"A1:A5","columns":[{"col":1,"color":{}}]}"#;
    assert_eq!(
        apply(none, fill).unwrap(),
        r#"{"hidden":[[2,2],[4,5]],"count":3}"#
    );
    let font = r#"{"range":"A1:A5","columns":[{"col":1,"color":{"font":true,"rgb":"ff0000"}}]}"#;
    assert_eq!(apply(font, fill).unwrap(), r#"{"hidden":[],"count":0}"#);
    let plain = r#"[{"col":1,"values":["a","b","c","d"]}]"#;
    assert_eq!(apply(pink, plain).unwrap(), r#"{"hidden":[],"count":0}"#);
    assert_eq!(apply(none, plain).unwrap(), r#"{"hidden":[],"count":0}"#);
    let dates =
        r#"[{"col":1,"values":["45000","45400","45000","hello"],"fmt":[[0,1,"yyyy-mm-dd"]]}]"#;
    let year = r#"{"range":"A1:A5","columns":[{"col":1,"dates":[{"unit":"year","year":2023}]}]}"#;
    assert_eq!(
        apply(year, dates).unwrap(),
        r#"{"hidden":[[3,5]],"count":3}"#
    );
    let day = r#"{"range":"A1:A5","columns":[{"col":1,"dates":[{"unit":"day","year":2024,"month":4,"day":18}]}]}"#;
    assert_eq!(
        apply(day, dates).unwrap(),
        r#"{"hidden":[[2,2],[4,5]],"count":3}"#
    );
}

#[test]
fn dynamic_date_criteria_judge_date_formatted_cells_against_today_and_periods() {
    // 45000 = 2023-03-15 (Q1, M3), 45001 = 2023-03-16, 45017 = 2023-04-01 (Q2, M4),
    // 45382 = 2024-03-31 (Q1, M3, leap year), 45000 without numFmt (General, NOT a date), "" (blank).
    let data = r#"[{"col":1,"values":["45000","45001","45017","45382","45000",""],"fmt":[[0,3,"yyyy-mm-dd"]]}]"#;
    let apply_at = |dyn_kind: &str, today: i64| {
        call(&format!(
            r#"{{"sessionAction":"filter","op":"apply","today":{today},"filter":{{"range":"A1:A7","columns":[{{"col":1,"dynamic":"{dyn_kind}"}}]}},"data":{data}}}"#
        ))
        .unwrap()
    };
    // Today = 45000 (2023-03-15): only row 2 matches; row 6 (General 45000) and row 7 (blank) hide.
    assert_eq!(apply_at("today", 45_000), r#"{"hidden":[[3,7]],"count":5}"#);
    // Tomorrow = 45001 (2023-03-16): only row 3 matches.
    assert_eq!(
        apply_at("tomorrow", 45_000),
        r#"{"hidden":[[2,2],[4,7]],"count":5}"#
    );
    // ThisMonth (March 2023): rows 2 and 3 match; March 2024 (row 5) hides.
    assert_eq!(
        apply_at("thisMonth", 45_000),
        r#"{"hidden":[[4,7]],"count":4}"#
    );
    // NextQuarter from Q1 2023 -> Q2 2023: row 4 (2023-04-01) matches.
    assert_eq!(
        apply_at("nextQuarter", 45_000),
        r#"{"hidden":[[2,3],[5,7]],"count":5}"#
    );
    // Q1 across all years: rows 2, 3 (2023 Q1) and 5 (2024 Q1) match!
    assert_eq!(
        apply_at("Q1", 45_000),
        r#"{"hidden":[[4,4],[6,7]],"count":3}"#
    );
    // M4 across all years: row 4 (April) matches.
    assert_eq!(
        apply_at("M4", 45_000),
        r#"{"hidden":[[2,3],[5,7]],"count":5}"#
    );
    // YearToDate on 2023-03-15: row 2 matches, row 3 (2023-03-16) hides.
    assert_eq!(
        apply_at("yearToDate", 45_000),
        r#"{"hidden":[[3,7]],"count":5}"#
    );
    // `values` on another column excludes rows hidden by a dynamic date column using `today`.
    let two = r#"[{"col":1,"values":["45000","45001"],"fmt":[[0,1,"yyyy-mm-dd"]]},{"col":2,"values":["영업","개발"]}]"#;
    let vals = call(&format!(
        r#"{{"sessionAction":"filter","op":"values","col":2,"today":45000,"filter":{{"range":"A1:B3","columns":[{{"col":1,"dynamic":"today"}}]}},"data":{two}}}"#
    ))
    .unwrap();
    assert_eq!(vals, r#"{"items":[["영업",1]],"blank":0,"more":false}"#);

    // Out-of-bounds or malformed `today` is refused Fail-Closed.
    for bad_today in ["0", "-5", "2958466", "45000.5", "\"2023-03-15\"", "true"] {
        let body = format!(
            r#"{{"sessionAction":"filter","op":"apply","today":{bad_today},"filter":{{"range":"A1:A7","columns":[{{"col":1,"dynamic":"today"}}]}},"data":{data}}}"#
        );
        assert!(call(&body).is_err(), "{bad_today}");
    }
}

#[test]
fn the_value_list_of_a_column_with_dates_and_colours_carries_the_tree_and_the_colours() {
    let filter = r#"{"range":"A1:A6","columns":[]}"#;
    let data = r#"[{"col":1,"values":["45000","45001","45400","","t"],"fmt":[[0,2,"yyyy-mm-dd"]],"fill":[[0,1,"ff0000"]]}]"#;
    let json = call(&format!(
        r#"{{"sessionAction":"filter","op":"values","col":1,"filter":{filter},"data":{data}}}"#
    ))
    .unwrap();
    assert!(
        json.contains(r#""dates":{"years":[[2023,2,[[3,2,[[15,1],[16,1]]]]],[2024,1,[[4,1,[[18,1]]]]]],"more":false}"#),
        "{json}"
    );
    assert!(
        json.contains(r#""colors":{"fill":[["ff0000",2]],"noFill":3,"font":[],"noFont":5}"#),
        "{json}"
    );
    assert!(
        json.starts_with(r#"{"items":[["t",1]],"blank":1,"more":false,"dates""#),
        "{json}"
    );
    let searched = call(&format!(
        r#"{{"sessionAction":"filter","op":"values","col":1,"query":"2023","filter":{filter},"data":{data}}}"#
    ))
    .unwrap();
    assert!(
        searched.starts_with(r#"{"items":[["2023-03-15",1],["2023-03-16",1]],"#)
            && !searched.contains(r#""dates""#),
        "{searched}"
    );
}

#[test]
fn new_criteria_over_a_bound_or_malformed_are_refused() {
    let data = r#"[{"col":1,"values":["1","2"]}]"#;
    for column in [
        r#"{"col":1,"top":{"count":0}}"#,
        r#"{"col":1,"top":{"count":101,"percent":true}}"#,
        r#"{"col":1,"top":{"count":1000001}}"#,
        r#"{"col":1,"top":{}}"#,
        r#"{"col":1,"average":{}}"#,
        r#"{"col":1,"average":"above"}"#,
        r#"{"col":1,"dynamic":"null"}"#,
        r#"{"col":1,"dynamic":123}"#,
        r#"{"col":1,"color":{"rgb":"red"}}"#,
        r#"{"col":1,"color":{"rgb":"ff00001"}}"#,
        r#"{"col":1,"dates":[{"unit":"month","year":2024}]}"#,
        r#"{"col":1,"dates":[{"unit":"day","year":2023,"month":2,"day":29}]}"#,
        r#"{"col":1,"dates":[{"unit":"week","year":2023}]}"#,
        r#"{"col":1,"dates":[{"unit":"year","year":1700}]}"#,
        r#"{"col":1,"dates":{}}"#,
    ] {
        let filter = format!(r#"{{"range":"A1:A3","columns":[{column}]}}"#);
        assert!(apply(&filter, data).is_err(), "{column}");
    }
    let filter = r#"{"range":"A1:A3","columns":[{"col":1,"color":{"rgb":"ff0000"}}]}"#;
    for bad in [
        r#"[{"col":1,"values":["1","2"],"fill":[[0,2,"ff0000"]]}]"#,
        r#"[{"col":1,"values":["1","2"],"fill":[[1,0,"ff0000"]]}]"#,
        r#"[{"col":1,"values":["1","2"],"fill":[[0,0,"zzzzzz"]]}]"#,
        r#"[{"col":1,"values":["1","2"],"fill":[[0,0]]}]"#,
        r#"[{"col":1,"values":["1","2"],"fill":"red"}]"#,
    ] {
        assert!(apply(filter, bad).is_err(), "{bad}");
    }
}

#[test]
fn a_hostile_list_of_many_colours_and_days_stays_bounded() {
    let rows = 20_000;
    let values: Vec<String> = (0..rows).map(|i| (40_000 + i).to_string()).collect();
    let values = format!(
        "[{}]",
        values
            .iter()
            .map(|v| format!("\"{v}\""))
            .collect::<Vec<_>>()
            .join(",")
    );
    let fill = (0..rows)
        .map(|i| format!("[{i},{i},\"{:06x}\"]", i % 5000))
        .collect::<Vec<_>>()
        .join(",");
    let data = format!(
        r#"[{{"col":1,"values":{values},"fmt":[[0,{last},"yyyy-mm-dd"]],"fill":[{fill}]}}]"#,
        last = rows - 1
    );
    let filter = format!(r#"{{"range":"A1:A{}","columns":[]}}"#, rows + 1);
    let json = call(&format!(
        r#"{{"sessionAction":"filter","op":"values","col":1,"filter":{filter},"data":{data}}}"#
    ))
    .unwrap();
    assert!(json.contains(r#""more":true}"#), "the day tree is cut");
    assert!(json.matches(r#"["#).count() > 1_000);
}

#[test]
fn bad_requests_are_refused() {
    let short = r#"[{"col":1,"values":["a"]},{"col":2,"values":["1"]}]"#;
    for (op, data) in [
        ("apply", short),
        ("apply", r#"[{"col":1,"values":["a","b","c","d","e","f"]}]"#),
        ("nope", DATA),
        ("apply", r#"[{"col":1,"values":[1,2,3,4,5,6]}]"#),
        ("apply", r#"[{"col":0,"values":["a","b","c","d","e","f"]}]"#),
    ] {
        let body =
            format!(r#"{{"sessionAction":"filter","op":"{op}","filter":{FILTER},"data":{data}}}"#);
        assert!(call(&body).is_err(), "{op} {data}");
    }
    let too_many = format!(
        r#"{{"sessionAction":"filter","op":"values","col":9,"filter":{FILTER},"data":{DATA}}}"#
    );
    assert!(call(&too_many).is_err());
}
