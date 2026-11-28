//! PD-402: a custom slide size crosses the structured save wire (`aspect: "custom:<cx>x<cy>"`) and comes back from
//! PPTX, ODP and PDF; a size outside the range is refused before any file is written.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{PdfSerializer, StrictJsonValue};

fn field<'a>(obj: &'a [(String, StrictJsonValue)], key: &str) -> &'a StrictJsonValue {
    StrictJsonValue::get_field(obj, key).unwrap_or_else(|| panic!("missing JSON field '{key}'"))
}

fn save_req(target: &str, aspect: &str, orientation: &str) -> String {
    format!(
        r#"{{"sourceFormat":"pptx","targetFormat":"{target}","title":"크기","aspect":"{aspect}","orientation":"{orientation}","slides":[{{"title":"한 장","bullets":[],"notes":"","layout":"blank","transition":"none","elements":[{{"id":"t1","kind":"textbox","x":600,"y":300,"w":100,"h":40,"text":"오른쪽 아래","fontSizePt":18,"bold":false,"italic":false,"underline":false,"align":"left","color":"1F2937","fill":"","role":null}}]}}]}}"#
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

fn decode(ext: &str, bytes: &[u8]) -> StrictJsonValue {
    let req = format!(
        r#"{{"action":"decode","fileName":"s.{ext}","sourceBase64":"{}"}}"#,
        EasyOfficeIpcRouter::encode_base64(bytes)
    );
    StrictJsonValue::parse(&EasyOfficeIpcRouter::dispatch("convert_document_format", &req).unwrap())
        .unwrap()
}

#[test]
fn custom_size_and_orientation_survive_pptx_and_the_pdf_page_has_the_size() {
    let aspect = "custom:9144000x5143500";
    let pptx = converted(&save_req("pptx", aspect, "landscape"));
    let back = decode("pptx", &pptx);
    let obj = back.as_object().unwrap();
    assert_eq!(field(obj, "aspect").as_str(), Some(aspect));
    assert_eq!(field(obj, "orientation").as_str(), Some("landscape"));

    let pdf = converted(&save_req("pdf", aspect, "landscape"));
    let report = PdfSerializer::inspect_readback(&pdf).unwrap();
    assert!(
        (report.pages[0].width_pt - 720.0).abs() < 0.5
            && (report.pages[0].height_pt - 405.0).abs() < 0.5,
        "{}x{}",
        report.pages[0].width_pt,
        report.pages[0].height_pt
    );

    let portrait = converted(&save_req("pptx", aspect, "portrait"));
    let back = decode("pptx", &portrait);
    let obj = back.as_object().unwrap();
    assert_eq!(field(obj, "aspect").as_str(), Some(aspect));
    assert_eq!(field(obj, "orientation").as_str(), Some("portrait"));
}

#[test]
fn custom_size_survives_odp_within_the_unit_rounding() {
    let odp = converted(&save_req("odp", "custom:9144000x5143500", "landscape"));
    let back = decode("odp", &odp);
    let wire = field(back.as_object().unwrap(), "aspect")
        .as_str()
        .unwrap()
        .to_string();
    let (cx, cy) = wire
        .strip_prefix("custom:")
        .and_then(|s| s.split_once('x'))
        .unwrap_or_else(|| panic!("not custom: {wire}"));
    let (cx, cy): (i64, i64) = (cx.parse().unwrap(), cy.parse().unwrap());
    assert!(
        (cx - 9_144_000).abs() < 400 && (cy - 5_143_500).abs() < 400,
        "{wire}"
    );
}

#[test]
fn a_hostile_size_is_refused_by_the_save() {
    for aspect in [
        "custom:1x1",
        "custom:99999999999x914400",
        "custom:914400x",
        "custom:-9144000x5143500",
        "custom:51206401x914400",
        "custom:9144000x5143500x1",
        "huge",
    ] {
        let resp = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &save_req("pptx", aspect, "landscape"),
        );
        assert!(resp.is_err(), "{aspect} must be refused");
    }
}
