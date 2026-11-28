//! PD-402: the new chart kinds cross the structured save wire of a slide deck and come back from PPTX; an unknown kind refuses
//! the save.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::StrictJsonValue;

fn field<'a>(obj: &'a [(String, StrictJsonValue)], key: &str) -> &'a StrictJsonValue {
    StrictJsonValue::get_field(obj, key).unwrap_or_else(|| panic!("missing JSON field '{key}'"))
}

fn save_req(chart_type: &str, series: &str) -> String {
    format!(
        r#"{{"sourceFormat":"pptx","targetFormat":"pptx","title":"차트","aspect":"16:9","slides":[{{"title":"","bullets":[],"notes":"","layout":"blank","transition":"none","elements":[{{"id":"c1","kind":"chart","x":40,"y":60,"w":400,"h":260,"chartType":"{chart_type}","title":"차트","categories":["1","2","3"],"series":{series},"legend":"bottom"}}]}}]}}"#
    )
}

fn round_trip(chart_type: &str, series: &str) -> Vec<(String, StrictJsonValue)> {
    let save =
        EasyOfficeIpcRouter::dispatch("convert_document_format", &save_req(chart_type, series))
            .unwrap();
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
fn every_new_kind_comes_back_with_its_series() {
    let one = r#"[{"name":"가","values":[1,2,3]}]"#;
    let two = r#"[{"name":"가","values":[1,2,3]},{"name":"나","values":[4,5,6]}]"#;
    let four = r#"[{"name":"시가","values":[1,2,3]},{"name":"고가","values":[4,5,6]},{"name":"저가","values":[0,1,2]},{"name":"종가","values":[3,4,5]}]"#;
    for (kind, series, want) in [
        ("doughnut", one, 1),
        ("radar", two, 2),
        ("bubble", two, 2),
        ("stock", four, 4),
        ("combo", two, 2),
    ] {
        let el = round_trip(kind, series);
        assert_eq!(field(&el, "chartType").as_str(), Some(kind));
        assert_eq!(
            field(&el, "series").as_array().unwrap().len(),
            want,
            "{kind}"
        );
    }
}

#[test]
fn an_unknown_kind_refuses_the_save() {
    for kind in ["treemap", "Doughnut2", "", "pie;x"] {
        let resp = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &save_req(kind, r#"[{"name":"가","values":[1,2,3]}]"#),
        );
        assert!(resp.is_err(), "{kind:?} must be refused");
    }
}
