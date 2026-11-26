//! Slice 5 (PD-357): IPC roundtrip & Fail-Closed bounds for `DOC` spacing (`spaceBeforeTwips`/`spaceAfterTwips`),
//! rich `cellParagraphs` (DOCX/ODT), and multi-page PDF `pdfPages` extraction on `decode`.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{
    base64_codec, DocumentFormat, StrictJsonValue, UniversalDocumentModel, UniversalFormatConverter,
};

fn json_obj(resp: &str) -> Vec<(String, StrictJsonValue)> {
    StrictJsonValue::parse(resp)
        .expect("valid json")
        .as_object()
        .expect("json object")
        .to_vec()
}

fn json_str<'a>(obj: &'a [(String, StrictJsonValue)], key: &str) -> &'a str {
    StrictJsonValue::get_field(obj, key)
        .and_then(StrictJsonValue::as_str)
        .unwrap_or("")
}

#[test]
fn ipc_roundtrips_doc_spacing_and_table_cell_paragraphs_across_docx_and_odt() {
    for (target, file_name) in [("docx", "table_rich.docx"), ("odt", "table_rich.odt")] {
        let save_req = format!(
            r##"{{
              "targetFormat":"{target}",
              "title":"표셀서식",
              "blocks":[
                {{
                  "type":"paragraph",
                  "spaceBeforeTwips":240,
                  "spaceAfterTwips":480,
                  "runs":[
                    {{"text":"E=mc"}},
                    {{"text":"2","script":"superscript"}},
                    {{"text":" · H"}},
                    {{"text":"2","script":"subscript"}}
                  ]
                }},
                {{
                  "type":"table",
                  "rows":[
                    [{{"text":"굵은 빨강 H2"}},{{"text":"10"}},{{"text":"=SUM(LEFT)"}}]
                  ],
                  "cellParagraphs":[
                    {{
                      "row":0,
                      "col":0,
                      "paragraphs":[
                        {{
                          "align":"center",
                          "lineSpacingPercent":150,
                          "runs":[
                            {{"text":"굵은 빨강 ","bold":true,"italic":true,"underline":true,"strike":true,"color":"DC2626","font":"맑은 고딕","sizeHalfPt":28}},
                            {{"text":"H"}},
                            {{"text":"2","script":"subscript"}}
                          ]
                        }}
                      ]
                    }},
                    {{
                      "row":0,
                      "col":2,
                      "paragraphs":[{{"align":"right","runs":[{{"text":"=SUM(LEFT)","bold":true}}]}}]
                    }}
                  ]
                }}
              ]
            }}"##
        );
        let save_resp = EasyOfficeIpcRouter::dispatch("convert_document_format", &save_req)
            .unwrap_or_else(|e| panic!("{target} save failed: {e}"));
        let save_obj = json_obj(&save_resp);
        let warnings = StrictJsonValue::get_field(&save_obj, "warnings")
            .and_then(StrictJsonValue::as_array)
            .expect("warnings array");
        assert!(
            warnings
                .iter()
                .any(|w| w.as_str().is_some_and(|s| s.contains("수식 셀"))),
            "{target}: formula cell should warn and be skipped: {warnings:?}"
        );
        let bytes = base64_codec::decode(json_str(&save_obj, "convertedBase64")).unwrap();

        let decode_req = format!(
            r#"{{"action":"decode","fileName":"{file_name}","sourceBase64":"{}"}}"#,
            base64_codec::encode(&bytes)
        );
        let dec_resp =
            EasyOfficeIpcRouter::dispatch("convert_document_format", &decode_req).unwrap();
        assert!(
            dec_resp.contains("\"spaceBeforeTwips\":240"),
            "{target}: {dec_resp}"
        );
        assert!(
            dec_resp.contains("\"spaceAfterTwips\":480"),
            "{target}: {dec_resp}"
        );
        assert!(
            dec_resp.contains("\"script\":\"superscript\""),
            "{target}: {dec_resp}"
        );
        assert!(
            dec_resp.contains("\"script\":\"subscript\""),
            "{target}: {dec_resp}"
        );
        assert!(
            dec_resp.contains("\"cellParagraphs\":["),
            "{target}: {dec_resp}"
        );
        assert!(
            dec_resp.contains("\"align\":\"center\""),
            "{target}: {dec_resp}"
        );
        assert!(
            dec_resp.contains("\"lineSpacingPercent\":150"),
            "{target}: {dec_resp}"
        );
        assert!(
            dec_resp.contains("\"color\":\"DC2626\""),
            "{target}: {dec_resp}"
        );
        assert!(
            dec_resp.contains("\"sizeHalfPt\":28"),
            "{target}: {dec_resp}"
        );
        assert!(dec_resp.contains("\"strike\":true"), "{target}: {dec_resp}");
    }
}

#[test]
fn ipc_rejects_malformed_spacing_and_cell_paragraphs_fail_closed() {
    for bad_req in [
        r#"{"targetFormat":"docx","title":"bad","blocks":[{"type":"paragraph","spaceBeforeTwips":99999,"runs":[{"text":"x"}]}]}"#,
        r#"{"targetFormat":"docx","title":"bad","blocks":[{"type":"paragraph","spaceAfterTwips":-1,"runs":[{"text":"x"}]}]}"#,
        r#"{"targetFormat":"docx","title":"bad","blocks":[{"type":"table","rows":[[{"text":"a"}]],"cellParagraphs":[{"row":0,"col":0,"paragraphs":[{"runs":[{"text":"a","script":"unknown"}]}]}]}]}"#,
        r#"{"targetFormat":"docx","title":"bad","blocks":[{"type":"table","rows":[[{"text":"a"}]],"cellParagraphs":[{"row":0,"col":0}]}]}"#,
        r#"{"targetFormat":"docx","title":"bad","blocks":[{"type":"table","rows":[[{"text":"a"}]],"cellParagraphs":[{"row":0,"col":0,"paragraphs":[{"lineSpacingPercent":9999,"runs":[{"text":"a"}]}]}]}]}"#,
    ] {
        assert!(
            EasyOfficeIpcRouter::dispatch("convert_document_format", bad_req).is_err(),
            "expected failure for {bad_req}"
        );
    }
}

#[test]
fn pdf_decode_exposes_per_page_text_for_cross_module_search() {
    let paragraphs = (0..85)
        .map(|i| {
            if i == 80 {
                "두번째쪽_찾기표적".to_string()
            } else {
                format!("문단 줄 번호 {i} 본문 내용입니다.")
            }
        })
        .collect();
    let model = UniversalDocumentModel {
        title: "다중쪽".to_string(),
        paragraphs,
        ..Default::default()
    };
    let pdf_bytes = UniversalFormatConverter::encode(&model, DocumentFormat::Pdf).unwrap();
    let decode_req = format!(
        r#"{{"action":"decode","fileName":"multi.pdf","sourceBase64":"{}"}}"#,
        base64_codec::encode(&pdf_bytes)
    );
    let resp = EasyOfficeIpcRouter::dispatch("convert_document_format", &decode_req).unwrap();
    let parsed = json_obj(&resp);
    let pages = StrictJsonValue::get_field(&parsed, "pdfPages")
        .and_then(StrictJsonValue::as_array)
        .expect("pdfPages array");
    assert!(
        pages.len() >= 2,
        "expected multi-page PDF, got {}",
        pages.len()
    );
    let has_target_page = pages.iter().any(|p| {
        let obj = p.as_object().unwrap();
        let num = StrictJsonValue::get_field(obj, "pageNumber")
            .and_then(StrictJsonValue::as_u64)
            .unwrap_or(0);
        let txt = json_str(obj, "text");
        num >= 2 && txt.contains("두번째쪽_찾기표적")
    });
    assert!(has_target_page, "expected target on page >= 2 in {resp}");
}
