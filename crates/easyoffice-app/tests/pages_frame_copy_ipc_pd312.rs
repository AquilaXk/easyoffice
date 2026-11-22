//! Structured JSON IPC roundtrip and fail-closed validation for multi-section Pages
//! `frameCopy` snapshots (`PD-312`).

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{
    base64_codec, DecodedContent, EasyOfficeError, RichBlock, StrictJsonValue,
    UniversalFormatConverter,
};

fn field<'a>(v: &'a StrictJsonValue, key: &str) -> Option<&'a StrictJsonValue> {
    v.as_object()
        .and_then(|obj| StrictJsonValue::get_field(obj, key))
}

fn dispatch(payload: &str) -> Result<String, EasyOfficeError> {
    EasyOfficeIpcRouter::dispatch("convert_document_format", payload)
}

#[test]
fn structured_ipc_frame_copy_accepts_string_and_bool_and_fails_closed_on_invalid_or_oversized() {
    let encoded_pages = dispatch(
        r#"{
          "sourceFormat": "docx",
          "targetFormat": "pages",
          "title": "IPC 프레임 복사",
          "header": [{"type":"paragraph","runs":[{"text":"1장 머리글"}]}],
          "blocks": [
            {"type":"paragraph","runs":[{"text":"본문 글"}]},
            {"type":"paragraph","heading":2,"frameCopy":"머리글","runs":[{"text":"머리글"}]},
            {"type":"paragraph","frameCopy":"2장 부록 머리글","runs":[{"text":"2장 부록 머리글"}]},
            {"type":"paragraph","frameCopy":true,"runs":[{"text":"1장 머리글"}]}
          ]
        }"#,
    )
    .expect("pages convert should succeed");
    let pages_json = StrictJsonValue::parse(&encoded_pages).expect("valid pages json");
    let pages_b64 = field(&pages_json, "convertedBase64")
        .and_then(StrictJsonValue::as_str)
        .expect("convertedBase64");
    let pages_bytes = base64_codec::decode(pages_b64).expect("base64 decode");

    let DecodedContent::Document(reopened) =
        UniversalFormatConverter::decode_rich(&pages_bytes, Some("out.pages"))
            .expect("decode written pages")
            .content
    else {
        panic!("expected document");
    };
    let body_texts: Vec<String> = reopened
        .body_blocks()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p.plain_text()),
            _ => None,
        })
        .collect();
    assert_eq!(reopened.page_frame.header[0].plain_text(), "1장 머리글");
    assert_eq!(body_texts, vec!["본문 글"]);

    let edited_pages = dispatch(
        r#"{
          "sourceFormat": "pages",
          "targetFormat": "pages",
          "title": "수정된 프레임 복사",
          "header": [{"type":"paragraph","runs":[{"text":"1장 머리글"}]}],
          "blocks": [
            {"type":"paragraph","runs":[{"text":"본문 글"}]},
            {"type":"paragraph","heading":2,"frameCopy":"머리글","runs":[{"text":"머리글"}]},
            {"type":"paragraph","frameCopy":"2장 부록 머리글","runs":[{"text":"사용자가 수정한 2장 글"}]},
            {"type":"paragraph","frameCopy":"2장 바닥글","runs":[{"text":"2장 바닥글"}]}
          ]
        }"#,
    )
    .expect("edited pages convert should succeed");
    let edited_json = StrictJsonValue::parse(&edited_pages).expect("valid edited json");
    let edited_b64 = field(&edited_json, "convertedBase64")
        .and_then(StrictJsonValue::as_str)
        .expect("convertedBase64");
    let edited_bytes = base64_codec::decode(edited_b64).expect("base64 decode");
    let DecodedContent::Document(reopened2) =
        UniversalFormatConverter::decode_rich(&edited_bytes, Some("edited.pages"))
            .expect("decode edited pages")
            .content
    else {
        panic!("expected edited document");
    };
    let edited_body_texts: Vec<String> = reopened2
        .body_blocks()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p.plain_text()),
            _ => None,
        })
        .collect();
    assert_eq!(edited_body_texts, vec!["본문 글", "사용자가 수정한 2장 글"]);

    let decode_req =
        format!(r#"{{"action":"decode","fileName":"out.pages","sourceBase64":"{pages_b64}"}}"#);
    let decoded_raw = dispatch(&decode_req).expect("decode action should succeed");
    let decoded_json = StrictJsonValue::parse(&decoded_raw).expect("valid decode json");
    let frame_copies: Vec<&str> = field(&decoded_json, "blocks")
        .and_then(StrictJsonValue::as_array)
        .expect("blocks array")
        .iter()
        .filter_map(|b| field(b, "frameCopy").and_then(StrictJsonValue::as_str))
        .collect();
    assert!(
        frame_copies.contains(&"머리글") && frame_copies.contains(&"1장 머리글"),
        "expected string frameCopy entries in decoded JSON: {frame_copies:?}"
    );

    for bad_val in ["123", "[]", "{}"] {
        let req = format!(
            r#"{{"sourceFormat":"pages","targetFormat":"docx","blocks":[{{"type":"paragraph","frameCopy":{bad_val},"runs":[{{"text":"x"}}]}}]}}"#
        );
        assert!(
            matches!(dispatch(&req), Err(EasyOfficeError::InvalidArgument(_))),
            "expected InvalidArgument for frameCopy={bad_val}"
        );
    }

    let oversized = "가".repeat(25_000);
    let too_long_req = format!(
        r#"{{"sourceFormat":"pages","targetFormat":"docx","blocks":[{{"type":"paragraph","frameCopy":"{oversized}","runs":[{{"text":"x"}}]}}]}}"#
    );
    assert!(
        matches!(
            dispatch(&too_long_req),
            Err(EasyOfficeError::ResourceBudgetExceeded(_))
        ),
        "expected ResourceBudgetExceeded for >64KiB frameCopy"
    );
}
