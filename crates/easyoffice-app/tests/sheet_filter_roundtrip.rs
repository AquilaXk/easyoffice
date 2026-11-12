//! A sheet with an auto filter saves to XLSX and reopens with the filter, the rows it hides and
//! the rows the user hid kept apart (PD-198).

use easyoffice_app::EasyOfficeIpcRouter;

fn json_str_field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

fn save(sheet: &str) -> Vec<u8> {
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"xlsx","title":"필터","sheets":[{sheet}]}}"#
    );
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).unwrap();
    EasyOfficeIpcRouter::decode_base64(json_str_field(&json, "convertedBase64")).unwrap()
}

fn open(bytes: &[u8]) -> String {
    let payload = format!(
        r#"{{"action":"decode","fileName":"필터.xlsx","sourceBase64":"{}"}}"#,
        EasyOfficeIpcRouter::encode_base64(bytes)
    );
    EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).unwrap()
}

const CELLS: &str = r#""cells":[
 {"ref":"A1","raw":"부서"},{"ref":"B1","raw":"매출"},
 {"ref":"A2","raw":"영업"},{"ref":"B2","raw":"5"},
 {"ref":"A3","raw":"개발"},{"ref":"B3","raw":"20"},
 {"ref":"A4","raw":"영업"},{"ref":"B4","raw":"30"},
 {"ref":"A5","raw":"영업"},{"ref":"B5","raw":"40"},
 {"ref":"A6","raw":"기획"},{"ref":"B6","raw":"50"}]"#;

const FILTER: &str = r#""filter":{"range":"A1:B6","columns":[
 {"col":1,"values":["영업"]},
 {"col":2,"conditions":[{"op":"greaterThan","text":"10"}]}],"hidden":[[2,3],[6,6]]}"#;

#[test]
fn the_filter_and_both_kinds_of_hidden_rows_survive_save_and_reopen() {
    // Rows 2 (5 is not above 10), 3 (개발) and 6 (기획) are the filter's; row 5 is the user's.
    let sheet = format!(r#"{{"name":"S",{CELLS},"hiddenRows":[[5,5]],{FILTER}}}"#);
    let reopened = open(&save(&sheet));
    assert!(
        reopened.contains(r#""hiddenRows":[[5,5]],"#),
        "the user's row stays the user's: {reopened}"
    );
    let marker = r#""filter":{"range":"A1:B6","columns":["#;
    let at = reopened
        .find(marker)
        .unwrap_or_else(|| panic!("{reopened}"));
    let filter = &reopened[at..];
    assert!(
        filter.contains(r#"{"col":1,"values":["영업"]}"#),
        "{filter}"
    );
    assert!(
        filter.contains(r#"{"col":2,"conditions":[{"op":"greaterThan","text":"10"}]}"#),
        "{filter}"
    );
    assert!(filter.contains(r#""hidden":[[2,3],[6,6]]}"#), "{filter}");
}

#[test]
fn a_filter_without_hidden_rows_reopens_without_them() {
    let sheet = format!(r#"{{"name":"S",{CELLS},"filter":{{"range":"A1:B6","columns":[]}}}}"#);
    let reopened = open(&save(&sheet));
    assert!(
        reopened.contains(r#""filter":{"range":"A1:B6","columns":[]}"#),
        "{reopened}"
    );
    assert!(!reopened.contains("hiddenRows"), "{reopened}");
}

#[test]
fn a_filter_over_the_bounds_is_refused_on_save() {
    let sheet = format!(r#"{{"name":"S",{CELLS},"filter":{{"range":"A1:B900000","columns":[]}}}}"#);
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"xlsx","title":"필터","sheets":[{sheet}]}}"#
    );
    assert!(EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).is_err());
}
