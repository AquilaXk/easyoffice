//! PD-310 IPC defaults for slide decks: omitted `aspect` and `transition` default to `16:9`
//! and `none`, and generated PPTX packages emit `맑은 고딕` theme fonts without `Calibri`.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{StrictJsonValue, ZipArchiveCodec, ZipSecurityPolicy};

fn field<'a>(obj: &'a [(String, StrictJsonValue)], key: &str) -> &'a StrictJsonValue {
    StrictJsonValue::get_field(obj, key).unwrap_or_else(|| panic!("missing JSON field '{key}'"))
}

#[test]
fn ipc_slide_omitted_aspect_and_transition_default_to_16_9_and_none() {
    for ext in ["pptx", "odp"] {
        let req = format!(
            r#"{{
                "sourceFormat": "pptx",
                "targetFormat": "{ext}",
                "title": "한국어 기본 덱",
                "slides": [
                    {{"title": "표지", "bullets": ["부제목"], "layout": "titleSlide"}},
                    {{"title": "본문", "bullets": ["내용"]}}
                ]
            }}"#
        );
        let save_resp = EasyOfficeIpcRouter::dispatch("convert_document_format", &req).unwrap();
        let b64 = field(
            StrictJsonValue::parse(&save_resp)
                .unwrap()
                .as_object()
                .unwrap(),
            "convertedBase64",
        )
        .as_str()
        .unwrap()
        .to_string();

        if ext == "pptx" {
            let bytes = EasyOfficeIpcRouter::decode_base64(&b64).unwrap();
            let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
            let theme = entries
                .iter()
                .find(|e| e.path == "ppt/theme/theme1.xml")
                .expect("theme1.xml");
            let theme_xml = std::str::from_utf8(&theme.data).unwrap();
            assert!(theme_xml.contains("typeface=\"맑은 고딕\""));
            assert!(!theme_xml.contains("Calibri"));
        }

        let dec_req =
            format!(r#"{{"action":"decode","fileName":"default.{ext}","sourceBase64":"{b64}"}}"#);
        let dec_resp = EasyOfficeIpcRouter::dispatch("convert_document_format", &dec_req).unwrap();
        let dec_val = StrictJsonValue::parse(&dec_resp).unwrap();
        let dec_obj = dec_val.as_object().unwrap();

        assert_eq!(field(dec_obj, "aspect").as_str(), Some("16:9"), "{ext}");
        let slides = field(dec_obj, "slides").as_array().unwrap();
        assert_eq!(slides.len(), 2, "{ext}");
        let s0 = slides[0].as_object().unwrap();
        assert_eq!(field(s0, "layout").as_str(), Some("titleSlide"), "{ext}");
        assert_eq!(field(s0, "transition").as_str(), Some("none"), "{ext}");

        let s1 = slides[1].as_object().unwrap();
        assert_eq!(
            field(s1, "layout").as_str(),
            Some("titleAndContent"),
            "{ext}"
        );
        assert_eq!(field(s1, "transition").as_str(), Some("none"), "{ext}");
    }
}
