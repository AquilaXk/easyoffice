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
