//! The defined names of a sheet payload reach the XLSX and ODS file, come back to the page as the same
//! `names` list, and a bad list refuses the whole save (PD-214).

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{read_sheet_styling, DocumentFormat};

fn json_str_field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

fn save(dst: &str, names: &str) -> Result<Vec<u8>, String> {
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"{dst}","title":"t","names":{names},"sheets":[{{"name":"시트","cells":[{{"ref":"A1","raw":"1"}},{{"ref":"B1","raw":"=세율*A1"}}]}},{{"name":"Data","cells":[{{"ref":"A1","raw":"2"}}]}}]}}"#
    );
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload)
        .map_err(|e| e.to_string())?;
    Ok(EasyOfficeIpcRouter::decode_base64(json_str_field(&json, "convertedBase64")).unwrap())
}

fn open(dst: &str, bytes: &[u8]) -> String {
    let payload = format!(
        r#"{{"action":"decode","fileName":"s.{dst}","sourceBase64":"{}"}}"#,
        EasyOfficeIpcRouter::encode_base64(bytes)
    );
    EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).unwrap()
}

const NAMES: &str = r#"[{"name":"세율","refersTo":"0.1","comment":"부가세"},{"name":"Prices","refersTo":"'시트'!$A$1:$B$1"},{"name":"Local","scope":"Data","refersTo":"Data!$A$1","hidden":true}]"#;

fn same_list(opened: &str) -> bool {
    let marker = "\"names\":";
    let at = opened.find(marker).unwrap() + marker.len();
    let list = &opened[at..opened.len() - 1];
    for name in ["세율", "Prices", "Local"] {
        assert!(list.contains(&format!("\"name\":\"{name}\"")), "{list}");
    }
    list.contains("\"comment\":\"부가세\"")
        && list.contains("\"scope\":\"Data\"")
        && list.contains("\"hidden\":true")
        && (list.contains("'시트'!$A$1:$B$1") || list.contains("시트!$A$1:$B$1"))
}

#[test]
fn the_names_reach_the_file_and_come_back_as_the_same_list() {
    for (dst, format) in [("xlsx", DocumentFormat::Xlsx), ("ods", DocumentFormat::Ods)] {
        let bytes = save(dst, NAMES).unwrap();
        let first = read_sheet_styling(&bytes, format).unwrap().remove(0).1;
        assert_eq!(first.defined_names.len(), 3, "{dst}");
        let opened = open(dst, &bytes);
        assert!(same_list(&opened), "{dst}: {opened}");
        let plain = open(dst, &save(dst, "[]").unwrap());
        assert!(
            !plain.contains("\"names\""),
            "{dst}: a workbook without names says nothing: {plain}"
        );
    }
}

#[test]
fn a_bad_list_refuses_the_save() {
    let many = format!(
        "[{}]",
        (0..easyoffice_formats::MAX_NAMES + 1)
            .map(|i| format!(r#"{{"name":"n_{i}","refersTo":"1"}}"#))
            .collect::<Vec<_>>()
            .join(",")
    );
    for names in [
        many.as_str(),
        r#"{"name":"a"}"#,
        r#"[1]"#,
        r#"[{"name":5}]"#,
        r#"[{"name":"a","refersTo":["x"]}]"#,
        r#"[{"name":"A1","refersTo":"1"}]"#,
        r#"[{"name":"_xlnm.Print_Area","refersTo":"Sheet1!$A$1"}]"#,
        r#"[{"name":"two words","refersTo":"1"}]"#,
        r#"[{"name":"Dup_","refersTo":"1"},{"name":"DUP_","refersTo":"2"}]"#,
    ] {
        for dst in ["xlsx", "ods"] {
            assert!(
                save(dst, names).is_err(),
                "{dst}: {}",
                &names[..names.len().min(40)]
            );
        }
    }
}
