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

const MARGINS: &str = concat!(
    "\"pageSetup\":{\"paper\":{\"width\":16838,\"height\":11906},",
    "\"margins\":{\"top\":1985,\"bottom\":1590,\"header\":1134,\"footer\":680,\"left\":1700,\"right\":1500}},"
);
const WIDE_FRAME: &str = concat!(
    "\"header\":[{\"type\":\"paragraph\",\"runs\":[{\"text\":\"머리\"}]}],",
    "\"footer\":[{\"type\":\"paragraph\",\"runs\":[{\"text\":\"\",\"field\":\"filename\"},{\"text\":\" \"},",
    "{\"text\":\"\",\"field\":\"page\"},{\"text\":\"/\"},{\"text\":\"\",\"field\":\"pages\"}]}]"
);

#[test]
fn page_margins_and_the_page_count_and_file_name_fields_survive_docx_and_odt_save_and_reopen() {
    for (format, name) in [
        ("docx", "여백.docx"),
        ("odt", "여백.odt"),
        ("hwpx", "여백.hwpx"),
    ] {
        let frame = format!("{MARGINS}{WIDE_FRAME}");
        let saved =
            EasyOfficeIpcRouter::dispatch("convert_document_format", &payload(format, &frame))
                .unwrap();
        let bytes =
            EasyOfficeIpcRouter::decode_base64(json_field(&saved, "convertedBase64")).unwrap();
        let json = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &request(Some("decode"), name, &bytes, ""),
        )
        .unwrap();
        assert!(
            json.contains(
                "\"pageSetup\":{\"paper\":{\"width\":16838,\"height\":11906},\"margins\":{\"top\":1985,\"bottom\":1590,\"header\":1134,\"footer\":680,\"left\":1700,\"right\":1500}}"
            ),
            "{format}: {json}"
        );
        assert!(
            json.contains("{\"text\":\"\",\"field\":\"pages\"}"),
            "{format}: {json}"
        );
        assert_eq!(
            json.contains("\"field\":\"filename\""),
            format != "hwpx",
            "{format}: the file name is a field except in HWPX"
        );
    }
}

#[test]
fn bad_page_setups_reject_the_save() {
    const OK_MARGINS: &str =
        "\"margins\":{\"top\":1985,\"bottom\":1590,\"header\":1134,\"footer\":680,\"left\":1700,\"right\":1500}";
    for setup in [
        // a missing distance, a value over the bound, a negative, the wrong types
        "\"pageSetup\":{\"margins\":{\"top\":1,\"bottom\":1,\"header\":1,\"footer\":1,\"left\":1}},".to_string(),
        "\"pageSetup\":{\"margins\":{\"top\":99999,\"bottom\":1,\"header\":1,\"footer\":1,\"left\":1,\"right\":1}},".to_string(),
        "\"pageSetup\":{\"margins\":{\"top\":-1,\"bottom\":1,\"header\":1,\"footer\":1,\"left\":1,\"right\":1}},".to_string(),
        "\"pageSetup\":[1,2,3,4],".to_string(),
        "\"pageSetup\":{\"margins\":[1,2,3,4,5,6]},".to_string(),
        "\"pageSetup\":{\"margins\":{\"top\":\"1\",\"bottom\":1,\"header\":1,\"footer\":1,\"left\":1,\"right\":1}},".to_string(),
        // a header or footer distance past its body distance
        "\"pageSetup\":{\"margins\":{\"top\":100,\"bottom\":1000,\"header\":101,\"footer\":10,\"left\":1000,\"right\":1000}},".to_string(),
        "\"pageSetup\":{\"margins\":{\"top\":1000,\"bottom\":100,\"header\":10,\"footer\":101,\"left\":1000,\"right\":1000}},".to_string(),
        // paper: no side, a side of zero, a side over the bound
        "\"pageSetup\":{\"paper\":{\"width\":11906}},".to_string(),
        "\"pageSetup\":{\"paper\":{\"width\":0,\"height\":16838}},".to_string(),
        "\"pageSetup\":{\"paper\":{\"width\":31681,\"height\":16838}},".to_string(),
        "\"pageSetup\":{\"paper\":{\"width\":1e3,\"height\":16838}},".to_string(),
        // margins that leave no room on the paper
        format!("\"pageSetup\":{{\"paper\":{{\"width\":3000,\"height\":16838}},{OK_MARGINS}}},"),
        "\"pageSetup\":{\"paper\":{\"width\":11906,\"height\":2000},\"margins\":{\"top\":1000,\"bottom\":1000,\"header\":10,\"footer\":10,\"left\":1000,\"right\":1000}},".to_string(),
    ] {
        let frame = format!("{setup}{WIDE_FRAME}");
        assert!(
            EasyOfficeIpcRouter::dispatch("convert_document_format", &payload("docx", &frame))
                .is_err(),
            "{setup}"
        );
    }
}

#[test]
fn a_page_setup_without_a_header_or_footer_survives_the_saves() {
    for (format, name) in [("docx", "쪽.docx"), ("hwpx", "쪽.hwpx"), ("odt", "쪽.odt")] {
        let saved = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &payload(format, &format!("{MARGINS}\"x\":1")),
        )
        .unwrap();
        let bytes =
            EasyOfficeIpcRouter::decode_base64(json_field(&saved, "convertedBase64")).unwrap();
        let json = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &request(Some("decode"), name, &bytes, ""),
        )
        .unwrap();
        assert!(
            json.contains("\"paper\":{\"width\":16838,\"height\":11906}"),
            "{format}: {json}"
        );
        assert!(
            json.contains("\"left\":1700,\"right\":1500"),
            "{format}: {json}"
        );
        assert!(!json.contains("\"header\":["), "{format}: {json}");
    }
}
