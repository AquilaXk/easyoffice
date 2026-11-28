//! PD-402: the `crop` of a slide picture crosses the structured save wire and comes back from PPTX; a crop past the
//! limits refuses the save, and a text box ignores one.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::StrictJsonValue;

const PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

fn field<'a>(obj: &'a [(String, StrictJsonValue)], key: &str) -> &'a StrictJsonValue {
    StrictJsonValue::get_field(obj, key).unwrap_or_else(|| panic!("missing JSON field '{key}'"))
}

fn save_req(kind_fields: &str) -> String {
    format!(
        r#"{{"sourceFormat":"pptx","targetFormat":"pptx","title":"자르기","aspect":"16:9","slides":[{{"title":"","bullets":[],"notes":"","layout":"blank","transition":"none","elements":[{{"id":"p1","x":40,"y":60,"w":200,"h":100,{kind_fields}}}]}}]}}"#
    )
}

fn image(crop: &str) -> String {
    save_req(&format!(
        r#""kind":"image","mime":"image/png","data":"{PNG_B64}","alt":"그림"{crop}"#
    ))
}

fn element_after_round_trip(req: &str) -> Vec<(String, StrictJsonValue)> {
    let save = EasyOfficeIpcRouter::dispatch("convert_document_format", req).unwrap();
    let parsed = StrictJsonValue::parse(&save).unwrap();
    let b64 = field(parsed.as_object().unwrap(), "convertedBase64")
        .as_str()
        .unwrap()
        .to_string();
    let dec = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(r#"{{"action":"decode","fileName":"c.pptx","sourceBase64":"{b64}"}}"#),
    )
    .unwrap();
    let value = StrictJsonValue::parse(&dec).unwrap();
    let slides = field(value.as_object().unwrap(), "slides")
        .as_array()
        .unwrap();
    let els = field(slides[0].as_object().unwrap(), "elements")
        .as_array()
        .unwrap();
    els[0].as_object().unwrap().to_vec()
}

#[test]
fn crop_round_trips_through_pptx() {
    let el = element_after_round_trip(&image(
        r#","crop":{"left":10000,"top":0,"right":20000,"bottom":5000}"#,
    ));
    let crop = field(&el, "crop").as_object().unwrap();
    for (key, want) in [
        ("left", "10000"),
        ("top", "0"),
        ("right", "20000"),
        ("bottom", "5000"),
    ] {
        assert!(
            matches!(field(crop, key), StrictJsonValue::Number(n) if n == want),
            "{key}"
        );
    }
    let plain = element_after_round_trip(&image(""));
    assert!(StrictJsonValue::get_field(&plain, "crop").is_none());
}

#[test]
fn a_crop_past_the_limits_refuses_the_save() {
    for crop in [
        r#","crop":{"left":60000,"right":60000}"#,
        r#","crop":{"left":-1}"#,
        r#","crop":{"left":99001}"#,
        r#","crop":{"left":1.5}"#,
        r#","crop":{"top":50000,"bottom":50000}"#,
        r#","crop":"all""#,
        r#","crop":{"left":"5"}"#,
    ] {
        let resp = EasyOfficeIpcRouter::dispatch("convert_document_format", &image(crop));
        assert!(resp.is_err(), "{crop} must be refused");
    }
}

#[test]
fn a_text_box_keeps_no_crop() {
    let el = element_after_round_trip(&save_req(
        r#""kind":"textbox","text":"글","fontSizePt":18,"bold":false,"italic":false,"underline":false,"align":"left","color":"1F2937","fill":"","role":null,"crop":{"left":1000}"#,
    ));
    assert!(StrictJsonValue::get_field(&el, "crop").is_none());
}
