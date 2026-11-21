//! The `rowHeights` of a sheet payload reach the XLSX and ODS files, come back to the page as the same key, and a bad
//! height refuses the whole save (PD-278).

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{read_sheet_styling, DocumentFormat};

fn json_str_field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

fn save(dst: &str, heights: &str) -> Result<Vec<u8>, String> {
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"{dst}","title":"t","sheets":[{{"name":"시트","cells":[{{"ref":"A1","raw":"가"}}]{heights}}}]}}"#
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

const HEIGHTS: &str = r#","rowHeights":[[1,30],[3,14.4],[6,409]]"#;

#[test]
fn row_heights_reach_both_files_and_come_back_as_the_same_key() {
    for (dst, format) in [("xlsx", DocumentFormat::Xlsx), ("ods", DocumentFormat::Ods)] {
        let bytes = save(dst, HEIGHTS).unwrap();
        let styling = read_sheet_styling(&bytes, format).unwrap().remove(0).1;
        let rows: Vec<(u32, u32, bool)> = styling
            .row_heights
            .iter()
            .map(|(&r, h)| (r, h.centipt, h.custom))
            .collect();
        assert_eq!(
            rows,
            vec![(0, 3000, true), (2, 1440, true), (5, 40_900, true)],
            "{dst}"
        );
        let opened = open(dst, &bytes);
        assert!(opened.contains(HEIGHTS), "{dst}: {opened}");
        // A sheet without heights says nothing about them.
        let plain = open(dst, &save(dst, "").unwrap());
        assert!(!plain.contains("rowHeights"), "{dst}");
    }
}

#[test]
fn a_bad_height_refuses_the_whole_save() {
    for heights in [
        r#","rowHeights":[[1,0]]"#,
        r#","rowHeights":[[1,409.5]]"#,
        r#","rowHeights":[[1,-1]]"#,
        r#","rowHeights":[[0,20]]"#,
        r#","rowHeights":[[1048577,20]]"#,
        r#","rowHeights":[[1,"20"]]"#,
        r#","rowHeights":[[1,1e999]]"#,
        r#","rowHeights":"tall""#,
    ] {
        assert!(save("xlsx", heights).is_err(), "{heights}");
        assert!(save("ods", heights).is_err(), "{heights}");
    }
}
