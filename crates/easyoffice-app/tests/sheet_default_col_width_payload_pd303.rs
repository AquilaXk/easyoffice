//! The `defaultColWidth` of a sheet payload (`{"width": chars, "base": count}`) reaches the XLSX file, comes back to the page as
//! the same key and survives a second save, and a bad one refuses the whole save (PD-303).

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{
    read_sheet_styling, DefaultColWidth, DocumentFormat, ZipArchiveCodec, ZipSecurityPolicy,
};

fn json_str_field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

fn save(dst: &str, width: &str) -> Result<Vec<u8>, String> {
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"{dst}","title":"t","sheets":[{{"name":"시트","cells":[{{"ref":"A1","raw":"가"}}]{width}}}]}}"#
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

const WIDTH: &str = r#","defaultColWidth":{"width":20.7109375,"base":10}"#;

#[test]
fn the_width_reaches_the_file_and_comes_back_as_the_same_key() {
    let bytes = save("xlsx", WIDTH).unwrap();
    let styling = read_sheet_styling(&bytes, DocumentFormat::Xlsx)
        .unwrap()
        .remove(0)
        .1;
    assert_eq!(
        styling.default_col_width,
        Some(DefaultColWidth {
            width: Some(20.7109375),
            base: Some(10)
        })
    );
    let opened = open("xlsx", &bytes);
    assert!(opened.contains(WIDTH), "{opened}");
    // A second save of what the page got back writes the same attributes.
    let again = save("xlsx", WIDTH).unwrap();
    let sheet = ZipArchiveCodec::unpack(&again, &ZipSecurityPolicy::default())
        .unwrap()
        .into_iter()
        .find(|e| e.path == "xl/worksheets/sheet1.xml")
        .unwrap();
    let xml = String::from_utf8(sheet.data).unwrap();
    assert!(
        xml.contains(r#"baseColWidth="10" defaultColWidth="20.7109375""#),
        "{xml}"
    );
    // A sheet without the key says nothing, and either value alone is as it was written.
    assert!(!open("xlsx", &save("xlsx", "").unwrap()).contains("defaultColWidth"));
    for alone in [
        r#","defaultColWidth":{"width":9.140625}"#,
        r#","defaultColWidth":{"base":10}"#,
    ] {
        let opened = open("xlsx", &save("xlsx", alone).unwrap());
        assert!(opened.contains(alone), "{alone}: {opened}");
    }
}

/// The page draws the columns without a width at `defaultColPx`, which only the engine works out (PD-305): the width in
/// characters first, then the base with its padding rounded up to 8 px; a sheet that says nothing has none (64 px).
#[test]
fn the_page_gets_the_pixels_of_the_default_column() {
    for (key, px) in [
        (WIDTH, 145),
        (r#","defaultColWidth":{"width":9.140625}"#, 64),
        (r#","defaultColWidth":{"width":8.7109375}"#, 61),
        (r#","defaultColWidth":{"base":10}"#, 80),
        (r#","defaultColWidth":{"base":8}"#, 64),
        (r#","defaultColWidth":{"base":255}"#, 1_785),
    ] {
        let opened = open("xlsx", &save("xlsx", key).unwrap());
        assert!(
            opened.contains(&format!(r#","defaultColPx":{px}"#)),
            "{key}: {opened}"
        );
    }
    assert!(!open("xlsx", &save("xlsx", "").unwrap()).contains("defaultColPx"));
}

/// ODS has no such width: the same payload saves, and the file says nothing of it.
#[test]
fn an_ods_save_takes_the_key_and_writes_nothing() {
    let bytes = save("ods", WIDTH).unwrap();
    assert!(!open("ods", &bytes).contains("defaultColWidth"));
}

#[test]
fn a_bad_width_refuses_the_whole_save() {
    for width in [
        r#","defaultColWidth":{"width":0}"#,
        r#","defaultColWidth":{"width":-1}"#,
        r#","defaultColWidth":{"width":0.009}"#,
        r#","defaultColWidth":{"width":1e-300}"#,
        r#","defaultColWidth":{"width":255.5}"#,
        r#","defaultColWidth":{"width":1e999}"#,
        r#","defaultColWidth":{"width":1e-999}"#,
        r#","defaultColWidth":{"width":"20"}"#,
        r#","defaultColWidth":{"width":true}"#,
        r#","defaultColWidth":{"width":[20]}"#,
        r#","defaultColWidth":{"base":256}"#,
        r#","defaultColWidth":{"base":-1}"#,
        r#","defaultColWidth":{"base":8.5}"#,
        r#","defaultColWidth":{"base":"8"}"#,
        r#","defaultColWidth":{"base":4294967296}"#,
        r#","defaultColWidth":{"base":18446744073709551616}"#,
        r#","defaultColWidth":{}"#,
        r#","defaultColWidth":{"width":null,"base":null}"#,
        r#","defaultColWidth":{"size":20}"#,
        r#","defaultColWidth":{"width":20,"size":20}"#,
        r#","defaultColWidth":[20,10]"#,
        r#","defaultColWidth":20"#,
        r#","defaultColWidth":"20""#,
        r#","defaultColWidth":null"#,
        r#","defaultColWidth":{"width":20,"width":30}"#,
    ] {
        assert!(save("xlsx", width).is_err(), "{width}");
        assert!(save("ods", width).is_err(), "{width}");
    }
}
