//! The fill and font colour keys of a sheet payload reach XLSX, ODS and XLS files, come back to the page as
//! the same keys, and a bad value refuses the whole save (PD-227).

use easyoffice_app::EasyOfficeIpcRouter;

fn json_str_field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

fn save(dst: &str, style: &str) -> Result<(Vec<u8>, String), String> {
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"{dst}","title":"t","sheets":[{{"name":"시트","cells":[{{"ref":"A1","raw":"가","style":{style}}},{{"ref":"C3","raw":"","style":{style}}}]}}]}}"#
    );
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload)
        .map_err(|e| e.to_string())?;
    let bytes =
        EasyOfficeIpcRouter::decode_base64(json_str_field(&json, "convertedBase64")).unwrap();
    Ok((bytes, json))
}

fn open(dst: &str, bytes: &[u8]) -> String {
    let payload = format!(
        r#"{{"action":"decode","fileName":"s.{dst}","sourceBase64":"{}"}}"#,
        EasyOfficeIpcRouter::encode_base64(bytes)
    );
    EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).unwrap()
}

#[test]
fn colour_keys_reach_every_sheet_format_and_come_back_as_the_same_keys() {
    for dst in ["xlsx", "ods", "xls"] {
        let (bytes, _) = save(dst, r#"{"fill":"ffeb9c","fc":"9c5700","bold":true}"#).unwrap();
        let opened = open(dst, &bytes);
        assert!(
            opened.contains(r#""fill":"ffeb9c","fc":"9c5700""#),
            "{dst}: {opened}"
        );
        // The blank coloured cell comes back too, whatever the format.
        assert_eq!(opened.matches("ffeb9c").count(), 2, "{dst}: {opened}");
        assert_eq!(
            opened.contains(r#""bold":true"#),
            dst != "xls",
            "{dst}: bold is not carried by an XLS"
        );
    }
}

#[test]
fn a_cell_with_only_a_colour_is_a_styled_cell() {
    let (bytes, _) = save("xlsx", r#"{"fill":"00b050"}"#).unwrap();
    let opened = open("xlsx", &bytes);
    assert!(opened.contains(r#""style":{"fill":"00b050"}"#), "{opened}");
}

#[test]
fn a_bad_colour_refuses_the_save() {
    for style in [
        r#"{"fill":"red"}"#,
        r#"{"fill":"fff"}"#,
        r##"{"fill":"#ffffff"}"##,
        r#"{"fill":"ggggggg"}"#,
        r#"{"fill":255}"#,
        r#"{"fc":true}"#,
        r#"{"fc":"1234567"}"#,
        r#"{"fc":"éééé"}"#,
    ] {
        assert!(save("xlsx", style).is_err(), "{style}");
    }
}
