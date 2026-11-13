//! Arrays that spill save and reopen (PD-217): XLSX and ODS keep the formula and its area, the other
//! formats hold the values, and a conversion from one format to another keeps the cells the array filled.

use easyoffice_app::EasyOfficeIpcRouter;

fn json_str_field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

const SHEET: &str = r#"{"name":"S","cells":[
 {"ref":"A1","raw":"b"},{"ref":"A2","raw":"a"},{"ref":"A3","raw":"b"},{"ref":"A4","raw":"c"},
 {"ref":"C1","raw":"=SORT(UNIQUE(A1:A4))"},{"ref":"C2","raw":"b"},{"ref":"C3","raw":"c"},
 {"ref":"E1","raw":"=COUNTA(C1#)"}],"spills":["C1:C3"]}"#;

fn save(format: &str) -> Vec<u8> {
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"{format}","title":"스필","sheets":[{SHEET}]}}"#
    );
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).unwrap();
    EasyOfficeIpcRouter::decode_base64(json_str_field(&json, "convertedBase64")).unwrap()
}

fn open(bytes: &[u8], name: &str) -> String {
    let payload = format!(
        r#"{{"action":"decode","fileName":"{name}","sourceBase64":"{}"}}"#,
        EasyOfficeIpcRouter::encode_base64(bytes)
    );
    EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).unwrap()
}

#[test]
fn xlsx_and_ods_reopen_with_the_formula_and_its_area_and_without_the_filled_cells() {
    for (format, name) in [("xlsx", "스필.xlsx"), ("ods", "스필.ods")] {
        let reopened = open(&save(format), name);
        assert!(
            reopened.contains(r#""spills":["C1:C3"]"#),
            "{format}: {reopened}"
        );
        assert!(
            reopened.contains("=SORT(UNIQUE(A1:A4))"),
            "{format}: {reopened}"
        );
        assert!(reopened.contains("=COUNTA(C1#)"), "{format}: {reopened}");
        assert!(!reopened.contains(r#"{"ref":"C2""#), "{format}: {reopened}");
        assert!(!reopened.contains(r#"{"ref":"C3""#), "{format}: {reopened}");
    }
}

#[test]
fn values_only_formats_hold_the_array_as_values() {
    let csv = String::from_utf8(save("csv")).unwrap();
    let lines: Vec<&str> = csv.trim_start_matches('\u{feff}').lines().collect();
    // row 1: A1 = b, C1 = a (the array's first value), E1 = 3 (COUNTA of the whole area)
    assert_eq!(lines, ["b,,a,,3", "a,,b,,", "b,,c,,", "c,,,,"], "{csv}");
}

#[test]
fn a_conversion_between_formats_keeps_the_cells_the_array_filled() {
    let xlsx = save("xlsx");
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"csv","title":"스필","sourceBase64":"{}"}}"#,
        EasyOfficeIpcRouter::encode_base64(&xlsx)
    );
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).unwrap();
    let csv = String::from_utf8(
        EasyOfficeIpcRouter::decode_base64(json_str_field(&json, "convertedBase64")).unwrap(),
    )
    .unwrap();
    let lines: Vec<&str> = csv.trim_start_matches('\u{feff}').lines().collect();
    assert_eq!(lines, ["b,,a,,3", "a,,b,,", "b,,c,,", "c,,,,"], "{csv}");
}
