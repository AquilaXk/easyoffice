//! The frozen pane, tab colour and groups of a sheet payload reach the XLSX and ODS files, come back to the
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
fn view_keys_reach_both_files_and_come_back_as_the_same_keys() {
    for (dst, format) in [("xlsx", DocumentFormat::Xlsx), ("ods", DocumentFormat::Ods)] {
        let bytes = save(dst, VIEW).unwrap();
        let styling = read_sheet_styling(&bytes, format).unwrap().remove(0).1;
        assert_eq!(styling.freeze, Freeze::new(2, 1), "{dst}");
        assert_eq!(styling.tab_color, Some(0x3366CC), "{dst}");
        assert_eq!(styling.outline.collapsed_rows, vec![(4, 5)], "{dst}");
        assert_eq!(styling.outline.collapsed_cols, vec![(1, 2)], "{dst}");
        assert!(
            styling.hidden_rows.is_empty() && styling.hidden_cols.is_empty(),
            "{dst}"
        );
        let opened = open(dst, &bytes);
        // A summary above the group is an XLSX setting; ODS keeps the groups and their folds.
        let want = if dst == "xlsx" {
            VIEW.to_string()
        } else {
            VIEW.replace(r#","summaryAbove":true"#, "")
        };
        assert!(opened.contains(&want), "{dst}: {opened}");
        // A sheet without a view says nothing about it.
        let plain = open(dst, &save(dst, "").unwrap());
        assert!(
            !plain.contains("freeze") && !plain.contains("tabColor") && !plain.contains("outline"),
            "{dst}"
        );
    }
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

#[test]
fn the_selection_zoom_and_scroll_reach_both_files_and_come_back() {
    let view = r#","view":{"active":[12,6],"range":[12,6,14,7],"zoom":150,"top":[10,4]}"#;
    let frozen = r#","freeze":{"rows":2,"cols":1},"view":{"active":[12,6],"range":[12,6,14,7],"zoom":80,"top":[30,5]}"#;
    for dst in ["xlsx", "ods"] {
        for (extra, want) in [(view, view), (frozen, frozen)] {
            let bytes = save(dst, extra).unwrap();
            let opened = open(dst, &bytes);
            // ODS keeps the cursor cell, not the range.
            let want = if dst == "ods" {
                want.replace(r#","range":[12,6,14,7]"#, "")
            } else {
                want.to_string()
            };
            assert!(opened.contains(&want), "{dst}: {opened}");
        }
        let plain = open(dst, &save(dst, "").unwrap());
        assert!(!plain.contains("\"view\""), "{dst}");
        // A view that only repeats the defaults says nothing in the file either.
        let default = open(
            dst,
            &save(dst, r#","view":{"zoom":100,"active":[1,1],"top":[1,1]}"#).unwrap(),
        );
        assert!(!default.contains("\"view\""), "{dst}");
    }
}

#[test]
fn a_bad_selection_zoom_or_scroll_refuses_the_save() {
    for view in [
        r#","view":{"zoom":401}"#,
        r#","view":{"zoom":9}"#,
        r#","view":{"active":[0,0]}"#,
        r#","view":{"active":[1048577,1]}"#,
        r#","view":{"top":[1,16385]}"#,
        r#","view":{"range":[1,1,2]}"#,
        r#","view":"A1""#,
    ] {
        assert!(save("xlsx", view).is_err(), "{view}");
    }
}
