//! PD-302: a row or column edit carries the charts of the sheet. A two-cell chart moves and sizes with its cells, a one-cell
//! chart moves and keeps its size, an absolute chart stays; the pixels are worked out on the columns the page draws (64 px
//! for a sheet that says nothing, the width of the request's `defaultColWidth` otherwise, PD-305).
//! A bad chart in the request is refused before the session changes.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{StrictJsonValue, MAX_CHARTS_PER_SHEET};

const EMU_PER_PX: u64 = 9_525;

fn call(body: &str) -> StrictJsonValue {
    let res = EasyOfficeIpcRouter::dispatch("evaluate_sheet", body).unwrap();
    StrictJsonValue::parse(&res).unwrap()
}

fn open() -> String {
    let res = call(r#"{"sessionAction":"open","sheets":["S"]}"#);
    res.as_object().unwrap()[0].1.as_str().unwrap().to_string()
}

fn num(v: &StrictJsonValue, key: &str) -> u64 {
    StrictJsonValue::get_field(v.as_object().unwrap(), key)
        .and_then(StrictJsonValue::as_u64)
        .unwrap_or_else(|| panic!("no number {key} in {}", v.to_json_string()))
}

fn anchors(res: &StrictJsonValue) -> Vec<StrictJsonValue> {
    let charts = StrictJsonValue::get_field(res.as_object().unwrap(), "charts").unwrap();
    charts
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            StrictJsonValue::get_field(c.as_object().unwrap(), "anchor")
                .unwrap()
                .clone()
        })
        .collect()
}

/// A chart over C3:E5 (zero-based columns 2..4, rows 2..4) of the kind `edit_as`, 128 x 72 px on the default grid.
fn chart(id: &str, edit_as: &str, from: (u32, u32), to: (u32, u32)) -> String {
    let (from_col, from_row) = from;
    let (to_col, to_row) = to;
    format!(
        r#"{{"id":"{id}","chartType":"column","title":"t","dataRange":"A1:B3","series":[{{"name":"s","valuesRef":"S!$B$2:$B$3"}}],"anchor":{{"fromCol":{from_col},"fromRow":{from_row},"toCol":{to_col},"toRow":{to_row},"width":128,"height":72,"editAs":"{edit_as}"}}}}"#
    )
}

fn structure(id: &str, axis: &str, op: &str, at: u32, count: u32, extra: &str) -> String {
    format!(
        r#"{{"sessionAction":"structure","session":"{id}","sheet":"S","axis":"{axis}","op":"{op}","at":{at},"count":{count},{extra}}}"#
    )
}

#[test]
fn a_row_insert_moves_charts_by_their_edit_kind() {
    let id = open();
    let charts = format!(
        "[{},{},{}]",
        chart("two", "twoCell", (2, 2), (4, 5)),
        chart("one", "oneCell", (2, 2), (4, 5)),
        chart("abs", "absolute", (2, 2), (4, 5)),
    );
    let res = call(&structure(
        &id,
        "row",
        "insert",
        2,
        3,
        &format!(r#""charts":{charts}"#),
    ));
    let moved = anchors(&res);
    // Two cells: both ends move down three rows and the chart is as tall as before (3 rows of 22 px).
    assert_eq!((num(&moved[0], "fromRow"), num(&moved[0], "toRow")), (5, 8));
    assert_eq!((num(&moved[0], "top"), num(&moved[0], "height")), (110, 66));
    // One cell: the start moves, the size stays (72 px across 22 px rows).
    assert_eq!((num(&moved[1], "fromRow"), num(&moved[1], "toRow")), (5, 8));
    assert_eq!((num(&moved[1], "top"), num(&moved[1], "height")), (110, 72));
    // Absolute: the pixels stay (the top is 44 px, row 2) and the cells are worked out again.
    assert_eq!((num(&moved[2], "fromRow"), num(&moved[2], "top")), (2, 44));
}

/// The workbook's default font as the request says it when it is not the standard one: Calibri 11 pt, a 7 px digit and
/// 64 px default columns (PD-306).
const CALIBRI: &str = r#""defaultFont":{"name":"Calibri","size":11}"#;

#[test]
fn the_pixels_of_a_column_edit_are_worked_out_on_64_px_columns() {
    let id = open();
    // Column 2 (B) is 200 px wide. The chart spans columns 1..3 (B:C): 200 + 64 px.
    let charts = format!("[{}]", chart("c", "twoCell", (1, 0), (3, 4)));
    let res = call(&structure(
        &id,
        "col",
        "insert",
        1,
        2,
        &format!(r#"{CALIBRI},"widths":[[2,200]],"charts":{charts}"#),
    ));
    let moved = anchors(&res);
    assert_eq!((num(&moved[0], "fromCol"), num(&moved[0], "toCol")), (3, 5));
    // Three default columns come before it, and the wide column moved along with it.
    assert_eq!(
        (num(&moved[0], "left"), num(&moved[0], "width")),
        (192, 264)
    );
}

/// The standard default font (Malgun Gothic 11 pt) has an 8 px digit: the default columns are 72 px (PD-306).
#[test]
fn the_pixels_of_a_column_edit_are_worked_out_on_72_px_columns_of_the_standard_font() {
    let id = open();
    let charts = format!("[{}]", chart("c", "twoCell", (1, 0), (3, 4)));
    let res = call(&structure(
        &id,
        "col",
        "insert",
        1,
        2,
        &format!(r#""widths":[[2,200]],"charts":{charts}"#),
    ));
    let moved = anchors(&res);
    assert_eq!((num(&moved[0], "fromCol"), num(&moved[0], "toCol")), (3, 5));
    assert_eq!(
        (num(&moved[0], "left"), num(&moved[0], "width")),
        (216, 272)
    );
    // The same edit in a Calibri workbook and in a request that says the standard font itself.
    let explicit = r#""defaultFont":{"name":"맑은 고딕","size":11}"#;
    let again = call(&structure(
        &id,
        "col",
        "insert",
        1,
        2,
        &format!(r#"{explicit},"widths":[[2,200]],"charts":{charts}"#),
    ));
    assert_eq!(
        (
            num(&anchors(&again)[0], "left"),
            num(&anchors(&again)[0], "width")
        ),
        (216, 272)
    );
}

/// A font the request names must be a bounded font: the whole request is refused otherwise.
#[test]
fn a_hostile_default_font_refuses_the_request() {
    let id = open();
    let long = "F".repeat(65);
    for font in [
        format!(r#""defaultFont":{{"name":"{long}","size":11}}"#),
        r#""defaultFont":{"name":"Arial","size":410}"#.to_string(),
        r#""defaultFont":{"name":"Arial","size":0}"#.to_string(),
        r#""defaultFont":{"name":"Bad;Font","size":11}"#.to_string(),
        r#""defaultFont":"Arial""#.to_string(),
        r#""defaultFont":{"name":"Arial"}"#.to_string(),
    ] {
        let body = structure(&id, "col", "insert", 1, 1, &font);
        assert!(
            EasyOfficeIpcRouter::dispatch("evaluate_sheet", &body).is_err(),
            "{font}"
        );
    }
}

/// The sheet's own default width (the request's `defaultColWidth`, 20.7109375 characters: 145 px) is the width of the columns
/// without one of their own, before and after the edit (PD-305).
#[test]
fn the_pixels_of_a_column_edit_follow_the_default_width_of_the_sheet() {
    let id = open();
    let charts = format!("[{}]", chart("c", "twoCell", (1, 0), (3, 4)));
    let res = call(&structure(
        &id,
        "col",
        "insert",
        1,
        2,
        &format!(r#"{CALIBRI},"defaultColWidth":{{"width":20.7109375}},"charts":{charts}"#),
    ));
    let moved = anchors(&res);
    assert_eq!((num(&moved[0], "fromCol"), num(&moved[0], "toCol")), (3, 5));
    assert_eq!(
        (num(&moved[0], "left"), num(&moved[0], "width")),
        (3 * 145, 2 * 145)
    );
    // `null` and a missing value are a sheet that says nothing; a value that is not a width is refused.
    for (field, ok) in [
        (r#""defaultColWidth":null"#, true),
        (r#""defaultColWidth":{"width":"x"}"#, false),
        (r#""defaultColWidth":{"width":1e308}"#, false),
        (r#""defaultColWidth":{"base":256}"#, false),
        (r#""defaultColWidth":7"#, false),
    ] {
        let id = open();
        let charts = format!("[{}]", chart("c", "twoCell", (1, 0), (3, 4)));
        let body = structure(
            &id,
            "col",
            "insert",
            1,
            2,
            &format!(r#"{field},"charts":{charts}"#),
        );
        let result = EasyOfficeIpcRouter::dispatch("evaluate_sheet", &body);
        assert_eq!(result.is_ok(), ok, "{field}");
    }
}

#[test]
fn a_one_cell_chart_keeps_its_size_when_columns_are_inserted_before_it() {
    let id = open();
    let charts = format!("[{}]", chart("c", "oneCell", (2, 0), (4, 3)));
    let res = call(&structure(
        &id,
        "col",
        "insert",
        1,
        1,
        &format!(r#"{CALIBRI},"charts":{charts}"#),
    ));
    let moved = anchors(&res);
    // 128 px is two columns of 64 px.
    assert_eq!((num(&moved[0], "fromCol"), num(&moved[0], "toCol")), (3, 5));
    assert_eq!(
        (num(&moved[0], "left"), num(&moved[0], "width")),
        (192, 128)
    );
}

/// A chart whose rows are deleted keeps its place at the edit and keeps its size when every row it covered is gone, and
/// shrinks by the rows that went when only some did (the rule of the pictures).
#[test]
fn deleting_the_rows_of_a_chart_shrinks_it_or_leaves_it_in_place() {
    let id = open();
    let charts = format!(
        "[{},{}]",
        chart("part", "twoCell", (2, 2), (4, 8)),
        chart("all", "twoCell", (2, 4), (4, 6)),
    );
    // Zero-based rows 4..=5 go.
    let res = call(&structure(
        &id,
        "row",
        "delete",
        5,
        2,
        &format!(r#""charts":{charts}"#),
    ));
    let moved = anchors(&res);
    assert_eq!((num(&moved[0], "fromRow"), num(&moved[0], "toRow")), (2, 6));
    assert_eq!(num(&moved[0], "height"), 4 * 22);
    assert_eq!((num(&moved[1], "fromRow"), num(&moved[1], "toRow")), (4, 4));
    assert_eq!(num(&moved[1], "height"), 72, "the size it had stays");
    assert_eq!(num(&moved[1], "rowOffEmu"), 0);
}

#[test]
fn the_offsets_inside_a_cell_survive_the_edit() {
    let id = open();
    let anchor = r#"{"id":"c","chartType":"line","title":"","dataRange":"A1:B3","anchor":{"fromCol":2,"fromRow":2,"colOffEmu":190500,"rowOffEmu":95250,"toCol":4,"toRow":5,"toColOffEmu":95250,"toRowOffEmu":47625,"width":182,"height":79}}"#;
    let res = call(&structure(
        &id,
        "row",
        "insert",
        1,
        1,
        &format!(r#""charts":[{anchor}]"#),
    ));
    let moved = anchors(&res);
    assert_eq!(
        (num(&moved[0], "colOffEmu"), num(&moved[0], "rowOffEmu")),
        (190_500, 95_250)
    );
    assert_eq!(
        (num(&moved[0], "toColOffEmu"), num(&moved[0], "toRowOffEmu")),
        (95_250, 47_625)
    );
    assert_eq!(num(&moved[0], "fromRow"), 3);
    assert_eq!(num(&moved[0], "top"), 3 * 22 + 95_250 / EMU_PER_PX);
}

#[test]
fn the_data_range_and_series_follow_the_edit() {
    let id = open();
    let charts = format!("[{}]", chart("c", "twoCell", (6, 0), (9, 5)));
    let res = call(&structure(
        &id,
        "row",
        "insert",
        1,
        2,
        &format!(r#""charts":{charts}"#),
    ));
    let json = res.to_json_string();
    assert!(json.contains(r#""dataRange":"A3:B5""#), "{json}");
    assert!(json.contains(r#""valuesRef":"S!$B$4:$B$5""#), "{json}");
}

#[test]
fn a_hostile_chart_is_refused_before_the_session_changes() {
    let id = open();
    let ok = chart("ok", "twoCell", (2, 2), (4, 4));
    let too_many = format!("[{}]", vec![ok.clone(); MAX_CHARTS_PER_SHEET + 1].join(","));
    let bad_charts = [
        too_many,
        r#"[{"id":"x","chartType":"radar","anchor":{}}]"#.to_string(),
        r#"[{"id":"x","chartType":"column","anchor":{"fromRow":2147483648}}]"#.to_string(),
        r#"[{"id":"x","chartType":"column","anchor":{"fromRow":1048576}}]"#.to_string(),
        r#"[{"id":"x","chartType":"column","anchor":{"width":0}}]"#.to_string(),
        r#"[{"id":"x","chartType":"column","anchor":{"colOffEmu":4294967296}}]"#.to_string(),
        r#"[{"id":"x","chartType":"column","anchor":"near"}]"#.to_string(),
        r#"{"id":"x"}"#.to_string(),
        r#"[7]"#.to_string(),
    ];
    for bad in bad_charts {
        let body = structure(&id, "row", "insert", 1, 1, &format!(r#""charts":{bad}"#));
        assert!(
            EasyOfficeIpcRouter::dispatch("evaluate_sheet", &body).is_err(),
            "{}",
            &bad[..bad.len().min(80)]
        );
    }
    // The session still answers: no refused edit changed it.
    let res = call(&structure(
        &id,
        "row",
        "insert",
        1,
        1,
        &format!(r#""charts":[{ok}]"#),
    ));
    assert_eq!(anchors(&res).len(), 1);
}
