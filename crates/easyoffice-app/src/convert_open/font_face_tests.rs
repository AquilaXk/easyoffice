//! The font variant of a run (`fontFace`, PD-368) through the save and open requests.

use super::tests::{blocks_array, json_field, request};
use super::*;

/// `fontFace` (the variant's PostScript name) is saved in the `.pages` font field in place of the family, opens back
/// under `font`, and a name that is too long or has control characters is read as no face.
#[test]
fn editor_json_font_face_reaches_the_pages_font_field_and_hostile_names_are_dropped() {
    let pages_font = |run: &str| {
        let payload = format!(
            "{{\"targetFormat\":\"pages\",\"blocks\":[{{\"type\":\"paragraph\",\"runs\":[{{\"text\":\"x\",{run}}}]}}]}}"
        );
        let saved = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).unwrap();
        let bytes =
            EasyOfficeIpcRouter::decode_base64(json_field(&saved, "convertedBase64")).unwrap();
        let opened = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &request(Some("decode"), "f.pages", &bytes, ""),
        )
        .unwrap();
        let font = blocks_array(&opened).split("\"font\":\"").nth(1);
        font.map(|f| f[..f.find('"').unwrap()].to_string())
    };
    assert_eq!(
        pages_font("\"font\":\"Apple SD Gothic Neo\",\"fontFace\":\"AppleSDGothicNeo-Light\""),
        Some("AppleSDGothicNeo-Light".to_string())
    );
    let too_long = "a".repeat(129);
    for bad in [too_long.as_str(), "a\\u0000b", "   "] {
        assert_eq!(
            pages_font(&format!(
                "\"font\":\"Apple SD Gothic Neo\",\"fontFace\":\"{bad}\""
            )),
            Some("Apple SD Gothic Neo".to_string()),
            "{bad:?}"
        );
    }
    let payload = "{\"targetFormat\":\"docx\",\"blocks\":[{\"type\":\"paragraph\",\"runs\":[{\"text\":\"x\",\"fontFace\":7}]}]}";
    assert!(
        EasyOfficeIpcRouter::dispatch("convert_document_format", payload).is_err(),
        "a face that is not a string is malformed"
    );
}
