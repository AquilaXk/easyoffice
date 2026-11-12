//! The alignment, wrapping and border keys of a sheet payload reach the XLSX and ODS files, come
//! back to the page as the same keys, and a bad value refuses the whole save (PD-207).

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{
    read_sheet_styling, BorderEdge, BorderLine, DocumentFormat, HAlign, VAlign,
};

fn json_str_field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

fn save(dst: &str, style: &str) -> Result<Vec<u8>, String> {
    let payload = format!(
        r#"{{"sourceFormat":"xlsx","targetFormat":"{dst}","title":"t","sheets":[{{"name":"시트","cells":[{{"ref":"A1","raw":"가","style":{style}}}]}}]}}"#
    );
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload)
        .map_err(|e| e.to_string())?;
    Ok(EasyOfficeIpcRouter::decode_base64(json_str_field(&json, "convertedBase64")).unwrap())
}

const LOOK: &str = r#"{"ha":"center","va":"middle","wrap":true,"bd":{"t":"thin:112233","b":"double:000000","l":"dashed:ff0000"}}"#;

#[test]
fn look_keys_reach_both_files_and_come_back_as_the_same_keys() {
    for (dst, format) in [("xlsx", DocumentFormat::Xlsx), ("ods", DocumentFormat::Ods)] {
        let bytes = save(dst, LOOK).unwrap();
        let style = read_sheet_styling(&bytes, format)
            .unwrap()
            .remove(0)
            .1
            .cells[&(0, 0)];
        assert_eq!(style.h_align, HAlign::Center, "{dst}");
        assert_eq!(style.v_align, VAlign::Center, "{dst}");
        assert!(style.wrap, "{dst}");
        assert_eq!(
            style.borders.top,
            BorderEdge::new(BorderLine::Thin, 0x112233)
        );
        assert_eq!(style.borders.bottom, BorderEdge::new(BorderLine::Double, 0));
        assert_eq!(
            style.borders.left,
            BorderEdge::new(BorderLine::Dashed, 0xFF0000)
        );
        assert_eq!(style.borders.right, BorderEdge::NONE);

        let payload = format!(
            r#"{{"action":"decode","fileName":"s.{dst}","sourceBase64":"{}"}}"#,
            EasyOfficeIpcRouter::encode_base64(&bytes)
        );
        let opened = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).unwrap();
        assert!(
            opened.contains(
                r#""style":{"ha":"center","va":"middle","wrap":true,"bd":{"t":"thin:112233","b":"double:000000","l":"dashed:ff0000"}}"#
            ),
            "{dst}: {opened}"
        );
    }
}

#[test]
fn a_bad_look_value_refuses_the_save() {
    for style in [
        r#"{"ha":"justify"}"#,
        r#"{"ha":1}"#,
        r#"{"va":"sideways"}"#,
        r#"{"wrap":"yes"}"#,
        r#"{"bd":"thin"}"#,
        r#"{"bd":{"t":"thin"}}"#,
        r#"{"bd":{"t":"thin:zzzzzz"}}"#,
        r#"{"bd":{"t":"thin:12345"}}"#,
        r#"{"bd":{"t":"wavy:112233"}}"#,
        r#"{"bd":{"x":"thin:112233"}}"#,
    ] {
        assert!(save("xlsx", style).is_err(), "{style}");
    }
}
