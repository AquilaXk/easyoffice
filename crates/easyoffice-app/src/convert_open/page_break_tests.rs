use super::tests::{json_field, request};
use super::*;

#[test]
fn a_page_break_survives_docx_and_odt_save_and_reopen_and_a_non_boolean_rejects_the_save() {
    let payload = |format: &str| {
        format!(
            concat!(
                "{{\"targetFormat\":\"{}\",\"blocks\":[",
                "{{\"type\":\"paragraph\",\"runs\":[{{\"text\":\"첫 쪽\"}}]}},",
                "{{\"type\":\"paragraph\",\"pageBreakBefore\":true,\"runs\":[{{\"text\":\"둘째 쪽\"}}]}}]}}"
            ),
            format
        )
    };
    for (format, name) in [("docx", "쪽.docx"), ("odt", "쪽.odt")] {
        let saved =
            EasyOfficeIpcRouter::dispatch("convert_document_format", &payload(format)).unwrap();
        let bytes =
            EasyOfficeIpcRouter::decode_base64(json_field(&saved, "convertedBase64")).unwrap();
        let json = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &request(Some("decode"), name, &bytes, ""),
        )
        .unwrap();
        assert_eq!(
            json.matches("\"pageBreakBefore\":true").count(),
            1,
            "{format}: {json}"
        );
    }
    let bad = payload("docx").replace("true", "1");
    assert!(EasyOfficeIpcRouter::dispatch("convert_document_format", &bad).is_err());
}
