//! PD-402: the drawing guides (`guides`, `showGuides`) cross the structured save wire and come back from PPTX as
//! `p:guideLst` of `ppt/viewProps.xml`; guides past the limits refuse the save.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{StrictJsonValue, ZipArchiveCodec, ZipSecurityPolicy};

fn field<'a>(obj: &'a [(String, StrictJsonValue)], key: &str) -> &'a StrictJsonValue {
    StrictJsonValue::get_field(obj, key).unwrap_or_else(|| panic!("missing JSON field '{key}'"))
}

fn save_req(deck_fields: &str) -> String {
    format!(
        r#"{{"sourceFormat":"pptx","targetFormat":"pptx","title":"안내선","aspect":"16:9",{deck_fields}"slides":[{{"title":"","bullets":[],"notes":"","layout":"blank","transition":"none","elements":[]}}]}}"#
    )
}

fn converted(req: &str) -> Vec<u8> {
    let save = EasyOfficeIpcRouter::dispatch("convert_document_format", req).unwrap();
    let parsed = StrictJsonValue::parse(&save).unwrap();
    let b64 = field(parsed.as_object().unwrap(), "convertedBase64")
        .as_str()
        .unwrap()
        .to_string();
    EasyOfficeIpcRouter::decode_base64(&b64).unwrap()
}

#[test]
fn guides_round_trip_through_the_view_part() {
    let pptx = converted(&save_req(
        r#""showGuides":true,"guides":[{"orient":"vert","posPt":480},{"orient":"horz","posPt":270.125}],"#,
    ));
    let entries = ZipArchiveCodec::unpack(&pptx, &ZipSecurityPolicy::default()).unwrap();
    let view = entries
        .iter()
        .find(|e| e.path == "ppt/viewProps.xml")
        .unwrap();
    let xml = String::from_utf8(view.data.clone()).unwrap();
    assert!(
        xml.contains("<p:guide pos=\"3840\"/><p:guide orient=\"horz\" pos=\"2161\"/>"),
        "{xml}"
    );
    assert!(xml.contains("showGuides=\"1\""));

    let dec = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(
            r#"{{"action":"decode","fileName":"g.pptx","sourceBase64":"{}"}}"#,
            EasyOfficeIpcRouter::encode_base64(&pptx)
        ),
    )
    .unwrap();
    let value = StrictJsonValue::parse(&dec).unwrap();
    let obj = value.as_object().unwrap();
    assert!(matches!(
        field(obj, "showGuides"),
        StrictJsonValue::Bool(true)
    ));
    let guides = field(obj, "guides").as_array().unwrap();
    assert_eq!(guides.len(), 2);
    let second = guides[1].as_object().unwrap();
    assert_eq!(field(second, "orient").as_str(), Some("horz"));
    assert!(matches!(field(second, "posPt"), StrictJsonValue::Number(n) if n == "270.125"));
}

#[test]
fn a_deck_without_guides_writes_no_view_part() {
    let pptx = converted(&save_req(""));
    let entries = ZipArchiveCodec::unpack(&pptx, &ZipSecurityPolicy::default()).unwrap();
    assert!(entries.iter().all(|e| e.path != "ppt/viewProps.xml"));
}

const FILE_VIEW: &str = r#"<?xml version="1.0"?><p:viewPr xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:slideViewPr><p:cSldViewPr showGuides="1"><p:guideLst><p:guide pos="10"/></p:guideLst></p:cSldViewPr></p:slideViewPr><p:notesViewPr><p:cSldViewPr><p:guideLst><p:guide orient="horz" pos="99"/></p:guideLst></p:cSldViewPr></p:notesViewPr><p:gridSpacing cx="72008" cy="72008"/></p:viewPr>"#;

fn view_part(pptx: &[u8]) -> String {
    let entries = ZipArchiveCodec::unpack(pptx, &ZipSecurityPolicy::default()).unwrap();
    let e = entries
        .iter()
        .find(|e| e.path == "ppt/viewProps.xml")
        .unwrap();
    String::from_utf8(e.data.clone()).unwrap()
}

#[test]
fn a_file_view_part_keeps_its_other_views_through_a_save() {
    let first = converted(&save_req(
        r#""showGuides":true,"guides":[{"orient":"vert","posPt":1}],"#,
    ));
    let entries = ZipArchiveCodec::unpack(&first, &ZipSecurityPolicy::default()).unwrap();
    let edited: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            if e.path == "ppt/viewProps.xml" {
                (e.path, FILE_VIEW.as_bytes().to_vec())
            } else {
                (e.path, e.data)
            }
        })
        .collect();
    let file = ZipArchiveCodec::pack(&edited).unwrap();
    let dec = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(
            r#"{{"action":"decode","fileName":"g.pptx","sourceBase64":"{}"}}"#,
            EasyOfficeIpcRouter::encode_base64(&file)
        ),
    )
    .unwrap();
    let value = StrictJsonValue::parse(&dec).unwrap();
    let obj = value.as_object().unwrap();
    assert_eq!(
        field(obj, "guides").as_array().unwrap().len(),
        1,
        "the notes view's guide is not read"
    );
    let raw = field(obj, "viewProps").as_str().unwrap().to_string();
    assert_eq!(raw, FILE_VIEW);
    // Save again with another guide: the part is the file's, patched.
    let escaped = raw.replace('"', "\\\"");
    let saved = converted(&save_req(&format!(
        r#""showGuides":true,"viewProps":"{escaped}","guides":[{{"orient":"horz","posPt":20}}],"#
    )));
    let xml = view_part(&saved);
    assert!(
        xml.contains(r#"<p:guideLst><p:guide orient="horz" pos="160"/></p:guideLst>"#),
        "{xml}"
    );
    assert!(
        xml.contains(
            r#"<p:notesViewPr><p:cSldViewPr><p:guideLst><p:guide orient="horz" pos="99"/>"#
        ),
        "{xml}"
    );
    assert!(xml.contains(r#"<p:gridSpacing cx="72008" cy="72008"/>"#));
    assert!(!xml.contains(r#"pos="10""#));
    // A view part that is not safe XML or not a view part refuses the save.
    for bad in [
        "<!DOCTYPE x [<!ENTITY a \\\"b\\\">]><p:viewPr/>",
        "not xml",
        &"<p:viewPr/>".repeat(7000),
    ] {
        let resp = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &save_req(&format!(r#""viewProps":"{bad}","#)),
        );
        assert!(resp.is_err(), "{}", &bad[..bad.len().min(30)]);
    }
}

#[test]
fn hostile_guides_refuse_the_save() {
    let many = (0..65)
        .map(|i| format!(r#"{{"orient":"vert","posPt":{i}}}"#))
        .collect::<Vec<_>>()
        .join(",");
    for fields in [
        r#""guides":[{"orient":"diag","posPt":1}],"#.to_string(),
        r#""guides":[{"orient":"vert","posPt":4001}],"#.to_string(),
        r#""guides":[{"orient":"vert","posPt":-4001}],"#.to_string(),
        r#""guides":[{"orient":"vert"}],"#.to_string(),
        r#""guides":[{"orient":"vert","posPt":"7"}],"#.to_string(),
        r#""guides":"all","#.to_string(),
        format!(r#""guides":[{many}],"#),
    ] {
        let resp = EasyOfficeIpcRouter::dispatch("convert_document_format", &save_req(&fields));
        assert!(resp.is_err(), "{fields} must be refused");
    }
}
