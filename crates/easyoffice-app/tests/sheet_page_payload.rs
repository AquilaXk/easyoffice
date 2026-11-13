//! The page setup of a sheet payload reaches the XLSX file, comes back to the page as the same key, and a bad
//! value refuses the whole save (PD-213).

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{read_sheet_styling, DocumentFormat, PageScale};

fn json_str_field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

fn save(dst: &str, extra: &str) -> Result<Vec<u8>, String> {
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"{dst}","title":"t","sheets":[{{"name":"시트","cells":[{{"ref":"A1","raw":"가"}}]{extra}}}]}}"#
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

const PAGE: &str = r#","page":{"landscape":true,"centerH":true,"grid":true,"headings":true,"paper":8,"margins":{"left":500,"right":600,"top":800,"bottom":900,"header":250,"footer":350},"scale":{"percent":85},"printArea":["A1:D10","F15"],"titleRows":[1,2],"titleCols":[1,1]}"#;

#[test]
fn the_page_setup_reaches_the_file_and_comes_back_as_the_same_key() {
    for (dst, format) in [("xlsx", DocumentFormat::Xlsx), ("ods", DocumentFormat::Ods)] {
        let bytes = save(dst, PAGE).unwrap();
        let page = read_sheet_styling(&bytes, format).unwrap().remove(0).1.page;
        assert!(
            page.landscape && page.center_horizontal && page.grid_lines && page.headings,
            "{dst}"
        );
        assert_eq!(page.paper, Some(8), "{dst}");
        assert_eq!(page.scale, PageScale::Percent(85), "{dst}");
        assert_eq!(page.print_area.len(), 2, "{dst}");
        assert_eq!(
            (page.title_rows, page.title_cols),
            (Some((0, 1)), Some((0, 0))),
            "{dst}"
        );
        let opened = open(dst, &bytes);
        assert!(opened.contains(&PAGE[1..]), "{dst}: {opened}");
        let plain = open(dst, &save(dst, "").unwrap());
        assert!(
            !plain.contains("\"page\""),
            "{dst}: a sheet without a setup says nothing"
        );
    }
}

#[test]
fn a_bad_page_value_refuses_the_save() {
    for page in [
        r#","page":{"scale":{"percent":500}}"#,
        r#","page":{"scale":{"fitWidth":5000,"fitHeight":1}}"#,
        r#","page":{"margins":{"left":-1,"right":1,"top":1,"bottom":1,"header":1,"footer":1}}"#,
        r#","page":{"printArea":["A1:XFE1048577"]}"#,
        r#","page":{"titleRows":[1,5000]}"#,
        r#","page":{"titleCols":[1,500]}"#,
        r#","page":"landscape""#,
    ] {
        for dst in ["xlsx", "ods"] {
            assert!(save(dst, page).is_err(), "{dst}: {page}");
        }
    }
}
