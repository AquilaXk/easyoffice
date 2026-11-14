//! The colour, date group, top and average conditions of an auto filter and the filter of a table
//! survive a save and reopen in XLSX and ODS, in each format and across them, with the rows each
//! filter hides kept apart from the user's (PD-226).

use easyoffice_app::EasyOfficeIpcRouter;

fn json_str_field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

fn save(sheet: &str, target: &str) -> Vec<u8> {
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"{target}","title":"필터","sheets":[{sheet}]}}"#
    );
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).unwrap();
    EasyOfficeIpcRouter::decode_base64(json_str_field(&json, "convertedBase64")).unwrap()
}

fn open(bytes: &[u8], file: &str) -> String {
    let payload = format!(
        r#"{{"action":"decode","fileName":"{file}","sourceBase64":"{}"}}"#,
        EasyOfficeIpcRouter::encode_base64(bytes)
    );
    EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).unwrap()
}

/// 30 data rows: dept, sales, a date serial (2023-03-15 plus the row), a text.
fn cells() -> String {
    let mut out = String::from(
        r#""cells":[{"ref":"A1","raw":"부서"},{"ref":"B1","raw":"매출"},{"ref":"C1","raw":"날짜"},{"ref":"D1","raw":"비고"}"#,
    );
    for r in 2..=31 {
        let dept = ["영업", "개발", "기획"][r % 3];
        out.push_str(&format!(
            r#",{{"ref":"A{r}","raw":"{dept}"}},{{"ref":"B{r}","raw":"{}"}},{{"ref":"C{r}","raw":"{}"}},{{"ref":"D{r}","raw":"n{r}"}}"#,
            r * 10,
            45_000 + r
        ));
    }
    out.push(']');
    out
}

const NUMFMT: &str = r#""styles":{"C2:C31":{"numFmt":"yyyy-mm-dd"}}"#;

/// Which rows the conditions of the sample hide: they are the file's own `hidden` rows when a reader
/// splits them back.
const SHEET_FILTER: &str = r#""filter":{"range":"A1:D31","columns":[
 {"col":1,"values":["영업"]},
 {"col":2,"top":{"count":20}},
 {"col":3,"dates":[{"unit":"year","year":2023}]}],"hidden":[[3,4],[6,7]]}"#;

fn sheet(extra: &str) -> String {
    format!(r#"{{"name":"S",{},{extra}}}"#, cells())
}

#[test]
fn top_and_date_group_conditions_survive_xlsx_and_ods_and_cross_over() {
    let source = sheet(SHEET_FILTER);
    for (target, file) in [("xlsx", "필터.xlsx"), ("ods", "필터.ods")] {
        let reopened = open(&save(&source, target), file);
        assert!(
            reopened.contains(r#""filter":{"range":"A1:D31""#),
            "{target}: {reopened}"
        );
        let at = reopened.find(r#""filter":"#).unwrap();
        let filter = &reopened[at..];
        assert!(
            filter.contains(r#"{"col":1,"values":["영업"]}"#),
            "{target}: {filter}"
        );
        assert!(
            filter.contains(r#"{"col":2,"top":{"count":20}}"#),
            "{target}: {filter}"
        );
        if target == "xlsx" {
            assert!(
                filter.contains(r#"{"col":3,"values":[],"dates":[{"unit":"year","year":2023}]}"#),
                "xlsx keeps the date group: {filter}"
            );
        } else {
            // The format has no date group: the column is left out and its rows stay hidden.
            assert!(!filter.contains(r#""col":3"#), "ods: {filter}");
        }
        // The rows were hidden in the file by the filter: they come back as the filter's.
        assert!(
            !reopened.contains("hiddenRows"),
            "{target}: the user hid nothing: {reopened}"
        );
    }
}

#[test]
fn every_kind_in_one_xlsx_and_the_table_beside_a_sheet_filter() {
    let table = r#""tables":[{"name":"Sales","range":"F12:G20","columns":["a","b"],"filter":[{"col":6,"average":{"above":true}}],"filterHidden":[[14,15]]}]"#;
    let filter = r#""filter":{"range":"A1:E9","columns":[
 {"col":1,"blank":true,"values":["x"],"dates":[{"unit":"month","year":2024,"month":3}]},
 {"col":2,"top":{"bottom":true,"percent":true,"count":10}},
 {"col":3,"average":{"above":false}},
 {"col":4,"color":{"rgb":"ffc7ce"}},
 {"col":5,"color":{"font":true}}],"hidden":[[4,5]]}"#;
    let reopened = open(
        &save(&sheet(&format!("{filter},{table}")), "xlsx"),
        "필터.xlsx",
    );
    for needle in [
        r#"{"col":1,"blank":true,"values":["x"],"dates":[{"unit":"month","year":2024,"month":3}]}"#,
        r#"{"col":2,"top":{"bottom":true,"percent":true,"count":10}}"#,
        r#"{"col":3,"average":{"above":false}}"#,
        r#"{"col":4,"color":{"rgb":"ffc7ce"}}"#,
        r#"{"col":5,"color":{"font":true}}"#,
        r#"{"col":6,"average":{"above":true}}"#,
    ] {
        assert!(reopened.contains(needle), "{needle} in {reopened}");
    }
    // The table's hidden rows are the table's own (the file's `hidden` flag, told apart by the table's
    // condition), the sheet filter's rows are its own, and nobody hid a row by hand.
    assert!(
        reopened.contains(r#""filterHidden":[[14,15]]"#),
        "{reopened}"
    );
    assert!(reopened.contains(r#""hidden":[[4,5]]"#), "{reopened}");
    assert!(!reopened.contains("hiddenRows"), "{reopened}");
}

#[test]
fn xls_drops_the_filter_without_failing_the_save() {
    let source = sheet(SHEET_FILTER);
    let reopened = open(&save(&source, "xls"), "필터.xls");
    assert!(!reopened.contains(r#""filter":"#), "{reopened}");
}

#[test]
fn a_number_format_run_is_only_a_sheet_style() {
    // The style key stays valid in the payload and does not break the filter.
    let reopened = open(
        &save(&sheet(&format!("{NUMFMT},{SHEET_FILTER}")), "xlsx"),
        "필터.xlsx",
    );
    assert!(
        reopened.contains(r#"{"col":2,"top":{"count":20}}"#),
        "{reopened}"
    );
}
