//! The frozen pane, tab colour and groups of a sheet payload reach the XLSX file, come back to the
//! page as the same keys, and a bad value refuses the whole save (PD-208).

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{read_sheet_styling, DocumentFormat, Freeze};

fn json_str_field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

fn save(dst: &str, view: &str) -> Result<Vec<u8>, String> {
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"{dst}","title":"t","sheets":[{{"name":"시트","cells":[{{"ref":"A1","raw":"가"}}]{view}}}]}}"#
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

const VIEW: &str = r#","freeze":{"rows":2,"cols":1},"tabColor":"3366cc","outline":{"rows":[[3,4,1],[5,6,2],[7,9,1]],"cols":[[2,3,1]],"foldedRows":[[5,6]],"foldedCols":[[2,3]],"summaryAbove":true}"#;

#[test]
fn view_keys_reach_the_xlsx_file_and_come_back_as_the_same_keys() {
    let bytes = save("xlsx", VIEW).unwrap();
    let styling = read_sheet_styling(&bytes, DocumentFormat::Xlsx)
        .unwrap()
        .remove(0)
        .1;
    assert_eq!(styling.freeze, Freeze::new(2, 1));
    assert_eq!(styling.tab_color, Some(0x3366CC));
    assert_eq!(styling.outline.collapsed_rows, vec![(4, 5)]);
    assert_eq!(styling.outline.collapsed_cols, vec![(1, 2)]);
    assert!(styling.hidden_rows.is_empty() && styling.hidden_cols.is_empty());
    let opened = open("xlsx", &bytes);
    assert!(opened.contains(VIEW), "{opened}");
    // A sheet without a view says nothing about it.
    let plain = open("xlsx", &save("xlsx", "").unwrap());
    assert!(!plain.contains("freeze") && !plain.contains("tabColor") && !plain.contains("outline"));
}

#[test]
fn a_bad_view_value_refuses_the_save() {
    for view in [
        r#","freeze":{"rows":100000}"#,
        r#","freeze":"top""#,
        r#","tabColor":"red""#,
        r##","tabColor":"#3366cc""##,
        r#","outline":{"rows":[[1,2,8]]}"#,
        r#","outline":{"rows":[[1,2,1]],"foldedRows":[[5,6]]}"#,
        r#","outline":{"rows":[[1,2,1],[2,3,1]]}"#,
    ] {
        assert!(save("xlsx", view).is_err(), "{view}");
    }
}
