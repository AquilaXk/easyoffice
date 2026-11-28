//! PD-402: table merges (`merges`) cross the structured save wire and come back from PPTX and ODP; a merge outside the
//! grid or over another merge refuses the save.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::StrictJsonValue;

fn field<'a>(obj: &'a [(String, StrictJsonValue)], key: &str) -> &'a StrictJsonValue {
    StrictJsonValue::get_field(obj, key).unwrap_or_else(|| panic!("missing JSON field '{key}'"))
}

fn save_req(target: &str, merges: &str) -> String {
    format!(
        r#"{{"sourceFormat":"pptx","targetFormat":"{target}","title":"표","aspect":"16:9","slides":[{{"title":"","bullets":[],"notes":"","layout":"blank","transition":"none","elements":[{{"id":"t1","kind":"table","x":40,"y":60,"w":400,"h":150,"headerRow":false,"fill":"DBEAFE","rows":[["가","나","다"],["라","마","바"]],"merges":{merges}}}]}}]}}"#
    )
}

fn round_trip(target: &str, merges: &str) -> Vec<(String, StrictJsonValue)> {
    let save = EasyOfficeIpcRouter::dispatch("convert_document_format", &save_req(target, merges))
        .unwrap();
    let parsed = StrictJsonValue::parse(&save).unwrap();
    let b64 = field(parsed.as_object().unwrap(), "convertedBase64")
        .as_str()
        .unwrap()
        .to_string();
    let dec = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &format!(r#"{{"action":"decode","fileName":"t.{target}","sourceBase64":"{b64}"}}"#),
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
fn merges_round_trip_through_pptx_and_odp() {
    let merges =
        r#"[{"row":0,"col":0,"rowSpan":2,"colSpan":1},{"row":0,"col":1,"rowSpan":1,"colSpan":2}]"#;
    for target in ["pptx", "odp"] {
        let el = round_trip(target, merges);
        let list = field(&el, "merges").as_array().unwrap();
        assert_eq!(list.len(), 2, "{target}");
        let first = list[0].as_object().unwrap();
        assert!(
            matches!(field(first, "rowSpan"), StrictJsonValue::Number(n) if n == "2"),
            "{target}"
        );
        let second = list[1].as_object().unwrap();
        assert!(
            matches!(field(second, "colSpan"), StrictJsonValue::Number(n) if n == "2"),
            "{target}"
        );
    }
}

#[test]
fn hostile_merges_refuse_the_save() {
    for merges in [
        r#"[{"row":0,"col":0,"rowSpan":3,"colSpan":1}]"#,
        r#"[{"row":0,"col":2,"rowSpan":1,"colSpan":2}]"#,
        r#"[{"row":0,"col":0,"rowSpan":2,"colSpan":2},{"row":1,"col":1,"rowSpan":1,"colSpan":2}]"#,
        r#"[{"row":0,"col":0,"rowSpan":1,"colSpan":1}]"#,
        r#"[{"row":-1,"col":0,"rowSpan":2,"colSpan":1}]"#,
        r#"[{"row":0.5,"col":0,"rowSpan":2,"colSpan":1}]"#,
        r#"[{"row":0,"col":0,"rowSpan":70000,"colSpan":1}]"#,
        r#"[{"row":0,"col":0,"rowSpan":2}]"#,
        r#""all""#,
    ] {
        let resp =
            EasyOfficeIpcRouter::dispatch("convert_document_format", &save_req("pptx", merges));
        assert!(resp.is_err(), "{merges} must be refused");
    }
}
