use super::tests::{json_field, request};
use super::*;

fn payload(format: &str, frame: &str) -> String {
    format!(
        "{{\"targetFormat\":\"{format}\",\"blocks\":[{{\"type\":\"paragraph\",\"runs\":[{{\"text\":\"본문\"}}]}}],{frame}}}"
    )
}

const FRAME: &str = concat!(
    "\"header\":[{\"type\":\"paragraph\",\"align\":\"center\",\"runs\":[{\"text\":\"머리말 글\",\"bold\":true}]}],",
    "\"footer\":[{\"type\":\"paragraph\",\"align\":\"right\",\"runs\":[{\"text\":\"쪽 \"},{\"text\":\"\",\"field\":\"page\"}]}]"
);

#[test]
fn a_header_and_footer_survive_the_three_document_saves_and_reopen() {
    for (format, name) in [("docx", "틀.docx"), ("odt", "틀.odt"), ("hwpx", "틀.hwpx")] {
        let saved =
            EasyOfficeIpcRouter::dispatch("convert_document_format", &payload(format, FRAME))
                .unwrap();
        let bytes =
            EasyOfficeIpcRouter::decode_base64(json_field(&saved, "convertedBase64")).unwrap();
        let json = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &request(Some("decode"), name, &bytes, ""),
        )
        .unwrap();
        assert!(
            json.contains("\"header\":[{\"type\":\"paragraph\""),
            "{format}: {json}"
        );
        assert!(
            json.contains("\"text\":\"머리말 글\",\"bold\":true"),
            "{format}: {json}"
        );
        assert!(json.contains("\"align\":\"center\""), "{format}: {json}");
        assert!(
            json.contains("\"footer\":[{\"type\":\"paragraph\""),
            "{format}: {json}"
        );
        assert!(
            json.contains("{\"text\":\"\",\"field\":\"page\"}"),
            "{format}: {json}"
        );
        assert_eq!(
            json.matches("\"text\":\"본문\"").count(),
            1,
            "{format}: {json}"
        );
    }
}

#[test]
fn a_document_without_a_frame_answers_without_header_and_footer_keys() {
    let saved =
        EasyOfficeIpcRouter::dispatch("convert_document_format", &payload("docx", "\"x\":1"))
            .unwrap();
    let bytes = EasyOfficeIpcRouter::decode_base64(json_field(&saved, "convertedBase64")).unwrap();
    let json = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &request(Some("decode"), "없음.docx", &bytes, ""),
    )
    .unwrap();
    assert!(!json.contains("\"header\""), "{json}");
    assert!(!json.contains("\"footer\""), "{json}");
}

#[test]
fn a_malformed_or_oversized_frame_rejects_the_save() {
    let many = (0..65)
        .map(|_| "{\"type\":\"paragraph\",\"runs\":[{\"text\":\"줄\"}]}")
        .collect::<Vec<_>>()
        .join(",");
    for frame in [
        format!("\"header\":[{many}]"),
        "\"header\":[{\"type\":\"table\",\"rows\":[[{\"text\":\"표\"}]]}]".to_string(),
        "\"footer\":[{\"type\":\"paragraph\",\"runs\":[{\"text\":\"\",\"field\":\"slide\"}]}]"
            .to_string(),
        "\"header\":\"문자열\"".to_string(),
        "\"header\":[{\"type\":\"paragraph\",\"runs\":[{\"text\":\"링크\",\"link\":\"javascript:1\"}]}]"
            .to_string(),
    ] {
        assert!(
            EasyOfficeIpcRouter::dispatch("convert_document_format", &payload("docx", &frame))
                .is_err(),
            "{frame}"
        );
    }
    let sixty_four = (0..64)
        .map(|_| "{\"type\":\"paragraph\",\"runs\":[{\"text\":\"줄\"}]}")
        .collect::<Vec<_>>()
        .join(",");
    assert!(EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &payload("docx", &format!("\"header\":[{sixty_four}]"))
    )
    .is_ok());
}
