//! PD-243: Lossless DOC open/save roundtrip for multi-level lists (`listLevel` 0..8, `listNumber`),
//! multi-column layouts (`columns` / `docColumns`), first/even page header & footer variants,
//! table cell hyperlinks (`links`), and Markdown inline links/formatting, plus Fail-Closed bounds.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{base64_codec, RichListNumber, StrictJsonValue};

fn json_str_field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

fn save_to_bytes(payload: &str) -> Vec<u8> {
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", payload)
        .unwrap_or_else(|e| panic!("save failed: {e:?}"));
    base64_codec::decode(json_str_field(&json, "convertedBase64")).expect("base64")
}

fn decode_bytes(file_name: &str, bytes: &[u8]) -> StrictJsonValue {
    let req = format!(
        r#"{{"action":"decode","fileName":"{file_name}","sourceBase64":"{}"}}"#,
        base64_codec::encode(bytes)
    );
    let resp = EasyOfficeIpcRouter::dispatch("convert_document_format", &req)
        .unwrap_or_else(|e| panic!("decode {file_name} failed: {e:?}"));
    StrictJsonValue::parse(&resp).expect("valid decode JSON")
}

fn obj(v: &StrictJsonValue) -> &[(String, StrictJsonValue)] {
    v.as_object().expect("object")
}

fn field<'a>(v: &'a StrictJsonValue, key: &str) -> Option<&'a StrictJsonValue> {
    StrictJsonValue::get_field(obj(v), key)
}

#[test]
fn docx_odt_and_hwpx_roundtrip_nested_lists_columns_frame_variants_and_table_links() {
    // "안내 페이지 바로가기": "바로가기" starts at UTF-8 byte 13 ("안내 페이지 " = 3+3+1+3+3+3+1 = 17 bytes)
    let cell_text = "안내 페이지 바로가기";
    let link_start = cell_text.find("바로가기").unwrap();
    let link_end = link_start + "바로가기".len();

    let save_body = |target: &str| {
        format!(
            r#"{{
              "sourceFormat":"docx",
              "targetFormat":"{target}",
              "title":"왕복 검증",
              "firstPageDifferent":true,
              "evenOddDifferent":true,
              "header":[{{"type":"paragraph","runs":[{{"text":"홀수 머리글"}}]}}],
              "footer":[{{"type":"paragraph","runs":[{{"text":"홀수 꼬리글"}}]}}],
              "firstHeader":[{{"type":"paragraph","runs":[{{"text":"첫 쪽 머리글"}}]}}],
              "firstFooter":[{{"type":"paragraph","runs":[{{"text":"첫 쪽 꼬리글"}}]}}],
              "evenHeader":[{{"type":"paragraph","runs":[{{"text":"짝수 머리글"}}]}}],
              "evenFooter":[{{"type":"paragraph","runs":[{{"text":"짝수 꼬리글"}}]}}],
              "blocks":[
                {{
                  "type":"paragraph",
                  "list":"ordered",
                  "listNumber":"upperRoman:rparen",
                  "columns":{{"count":2,"equal":true,"gapTwips":720}},
                  "runs":[{{"text":"대분류 항목"}}]
                }},
                {{
                  "type":"paragraph",
                  "list":"ordered",
                  "listLevel":1,
                  "listNumber":"lowerLatin:paren",
                  "columns":{{"count":2,"equal":true,"gapTwips":720}},
                  "runs":[{{"text":"중분류 항목"}}]
                }},
                {{
                  "type":"paragraph",
                  "list":"bullet",
                  "listLevel":2,
                  "columns":{{"count":2,"equal":true,"gapTwips":720}},
                  "runs":[{{"text":"세부 글머리"}}]
                }},
                {{
                  "type":"table",
                  "rows":[[{{"text":"구분"}},{{"text":"{cell_text}"}}]],
                  "links":[{{"row":0,"col":1,"start":{link_start},"end":{link_end},"url":"https://example.org/guide"}}]
                }}
              ]
            }}"#
        )
    };

    for (ext, check_first_text) in [("docx", true), ("odt", true), ("hwpx", false)] {
        let bytes = save_to_bytes(&save_body(ext));
        let decoded = decode_bytes(&format!("roundtrip.{ext}"), &bytes);
        let warnings = field(&decoded, "warnings").unwrap().as_array().unwrap();
        assert!(
            warnings.is_empty(),
            "{ext} unexpected warnings: {warnings:?}"
        );

        assert_eq!(
            field(&decoded, "evenOddDifferent").and_then(StrictJsonValue::as_bool),
            Some(true),
            "{ext} evenOddDifferent"
        );
        let hdr = field(&decoded, "header").unwrap().as_array().unwrap();
        let even_hdr = field(&decoded, "evenHeader").unwrap().as_array().unwrap();
        assert_eq!(
            field(&hdr[0], "runs").unwrap().as_array().unwrap()[0]
                .as_object()
                .and_then(|o| StrictJsonValue::get_field(o, "text"))
                .and_then(StrictJsonValue::as_str),
            Some("홀수 머리글"),
            "{ext} odd header"
        );
        assert_eq!(
            field(&even_hdr[0], "runs").unwrap().as_array().unwrap()[0]
                .as_object()
                .and_then(|o| StrictJsonValue::get_field(o, "text"))
                .and_then(StrictJsonValue::as_str),
            Some("짝수 머리글"),
            "{ext} even header"
        );

        if check_first_text {
            assert_eq!(
                field(&decoded, "firstPageDifferent").and_then(StrictJsonValue::as_bool),
                Some(true),
                "{ext} firstPageDifferent"
            );
            let first_hdr = field(&decoded, "firstHeader").unwrap().as_array().unwrap();
            assert_eq!(
                field(&first_hdr[0], "runs").unwrap().as_array().unwrap()[0]
                    .as_object()
                    .and_then(|o| StrictJsonValue::get_field(o, "text"))
                    .and_then(StrictJsonValue::as_str),
                Some("첫 쪽 머리글"),
                "{ext} first header"
            );
        }

        let blocks = field(&decoded, "blocks").unwrap().as_array().unwrap();
        assert_eq!(blocks.len(), 4, "{ext} block count");

        // Paragraph 0: level 0, upperRoman:rparen, 2 columns
        assert_eq!(
            field(&blocks[0], "listNumber").and_then(StrictJsonValue::as_str),
            Some("upperRoman:rparen"),
            "{ext} level 0 listNumber"
        );
        assert!(
            field(&blocks[0], "listLevel").is_none(),
            "{ext} level 0 omitted"
        );
        let cols = field(&blocks[0], "columns").unwrap();
        assert_eq!(
            field(cols, "count").and_then(StrictJsonValue::as_u64),
            Some(2),
            "{ext} columns.count"
        );

        // Paragraph 1: level 1, lowerLatin:paren
        assert_eq!(
            field(&blocks[1], "listLevel").and_then(StrictJsonValue::as_u64),
            Some(1),
            "{ext} level 1"
        );
        assert_eq!(
            field(&blocks[1], "listNumber").and_then(StrictJsonValue::as_str),
            Some("lowerLatin:paren"),
            "{ext} level 1 listNumber"
        );

        // Paragraph 2: bullet level 2
        assert_eq!(
            field(&blocks[2], "list").and_then(StrictJsonValue::as_str),
            Some("bullet"),
            "{ext} bullet"
        );
        assert_eq!(
            field(&blocks[2], "listLevel").and_then(StrictJsonValue::as_u64),
            Some(2),
            "{ext} bullet level 2"
        );

        // Block 3: table cell UTF-8 hyperlink
        let links = field(&blocks[3], "links").unwrap().as_array().unwrap();
        assert_eq!(links.len(), 1, "{ext} table link count");
        assert_eq!(
            field(&links[0], "row").and_then(StrictJsonValue::as_u64),
            Some(0)
        );
        assert_eq!(
            field(&links[0], "col").and_then(StrictJsonValue::as_u64),
            Some(1)
        );
        assert_eq!(
            field(&links[0], "start").and_then(StrictJsonValue::as_u64),
            Some(link_start as u64)
        );
        assert_eq!(
            field(&links[0], "end").and_then(StrictJsonValue::as_u64),
            Some(link_end as u64)
        );
        assert_eq!(
            field(&links[0], "url").and_then(StrictJsonValue::as_str),
            Some("https://example.org/guide")
        );
    }
}

#[test]
fn hwpx_first_page_header_suppression_and_doc_columns_fallback_roundtrip() {
    let payload = r#"{
      "sourceFormat":"hwpx",
      "targetFormat":"hwpx",
      "firstPageDifferent":true,
      "docColumns":3,
      "header":[{"type":"paragraph","runs":[{"text":"본문 머리글"}]}],
      "blocks":[
        {"type":"paragraph","runs":[{"text":"1단락"}]},
        {"type":"paragraph","runs":[{"text":"2단락"}]}
      ]
    }"#;
    let bytes = save_to_bytes(payload);
    let decoded = decode_bytes("hidden-first.hwpx", &bytes);
    assert_eq!(
        field(&decoded, "firstPageDifferent").and_then(StrictJsonValue::as_bool),
        Some(true)
    );
    assert!(field(&decoded, "firstHeader").is_none());
    let blocks = field(&decoded, "blocks").unwrap().as_array().unwrap();
    for b in blocks {
        let cols = field(b, "columns").expect("docColumns fallback applied");
        assert_eq!(
            field(cols, "count").and_then(StrictJsonValue::as_u64),
            Some(3)
        );
    }
}

#[test]
fn markdown_roundtrips_inline_links_nested_lists_highlight_script_and_table_links() {
    let payload = r##"{
      "sourceFormat":"md",
      "targetFormat":"md",
      "blocks":[
        {"type":"paragraph","runs":[
          {"text":"참조 "},
          {"text":"공식 문서","bold":true,"link":"https://example.org/docs"},
          {"text":" 및 "},
          {"text":"형광","highlight":"#FFFF00"},
          {"text":" H"},
          {"text":"2","script":"subscript"},
          {"text":"O"}
        ]},
        {"type":"paragraph","list":"ordered","listNumber":"decimal:rparen","runs":[{"text":"상위"}]},
        {"type":"paragraph","list":"bullet","listLevel":1,"runs":[{"text":"하위"}]},
        {"type":"table","rows":[[{"text":"항목"},{"text":"링크 바로가기"}]],
         "links":[{"row":0,"col":1,"start":7,"end":19,"url":"https://example.org/table"}]}
      ]
    }"##;
    let md_bytes = save_to_bytes(payload);
    let md_text = std::str::from_utf8(&md_bytes).unwrap();
    assert!(
        md_text.contains("[**공식 문서**](https://example.org/docs)"),
        "{md_text}"
    );
    assert!(md_text.contains("==형광=="), "{md_text}");
    assert!(md_text.contains("<sub>2</sub>"), "{md_text}");
    assert!(md_text.contains("1) 상위\n  - 하위"), "{md_text}");
    assert!(
        md_text.contains("[바로가기](https://example.org/table)"),
        "{md_text}"
    );

    let decoded = decode_bytes("doc.md", &md_bytes);
    let blocks = field(&decoded, "blocks").unwrap().as_array().unwrap();
    // blocks[0] is the `# 변환_문서` title heading written when the body has no heading 1.
    let runs = field(&blocks[1], "runs").unwrap().as_array().unwrap();
    assert!(
        runs.iter().any(|r| {
            field(r, "text").and_then(StrictJsonValue::as_str) == Some("공식 문서")
                && field(r, "link").and_then(StrictJsonValue::as_str)
                    == Some("https://example.org/docs")
                && field(r, "bold").and_then(StrictJsonValue::as_bool) == Some(true)
        }),
        "markdown inline link + bold roundtripped: {runs:?}"
    );
    assert_eq!(
        field(&blocks[2], "listNumber").and_then(StrictJsonValue::as_str),
        Some("decimal:rparen")
    );
    assert_eq!(
        field(&blocks[3], "listLevel").and_then(StrictJsonValue::as_u64),
        Some(1)
    );
    let tbl_links = field(&blocks[4], "links").unwrap().as_array().unwrap();
    assert_eq!(tbl_links.len(), 1);
    assert_eq!(
        field(&tbl_links[0], "url").and_then(StrictJsonValue::as_str),
        Some("https://example.org/table")
    );
}

#[test]
fn hostile_save_payloads_fail_closed_or_drop_invalid_utf8_table_link_offsets() {
    for bad in [
        r#"{"sourceFormat":"docx","targetFormat":"docx","blocks":[{"type":"paragraph","list":"bullet","listLevel":9,"runs":[{"text":"x"}]}]}"#,
        r#"{"sourceFormat":"docx","targetFormat":"docx","blocks":[{"type":"paragraph","list":"bullet","listLevel":-1,"runs":[{"text":"x"}]}]}"#,
        r#"{"sourceFormat":"docx","targetFormat":"docx","blocks":[{"type":"paragraph","list":"ordered","listNumber":"bogus:style","runs":[{"text":"x"}]}]}"#,
        r#"{"sourceFormat":"docx","targetFormat":"docx","blocks":[{"type":"paragraph","columns":{"count":99},"runs":[{"text":"x"}]}]}"#,
        r#"{"sourceFormat":"docx","targetFormat":"docx","blocks":[{"type":"paragraph","columns":{"count":0},"runs":[{"text":"x"}]}]}"#,
        r#"{"sourceFormat":"docx","targetFormat":"docx","docColumns":20,"blocks":[{"type":"paragraph","runs":[{"text":"x"}]}]}"#,
        r#"{"sourceFormat":"docx","targetFormat":"docx","firstPageDifferent":"yes","blocks":[{"type":"paragraph","runs":[{"text":"x"}]}]}"#,
        r#"{"sourceFormat":"docx","targetFormat":"docx","blocks":[{"type":"table","rows":[[{"text":"안전"}]],"links":[{"row":0,"col":0,"start":0,"end":6,"url":"javascript:alert(1)"}]}]}"#,
    ] {
        let err = EasyOfficeIpcRouter::dispatch("convert_document_format", bad)
            .expect_err(&format!("must reject hostile payload: {bad}"));
        assert!(
            matches!(err, easyoffice_formats::EasyOfficeError::InvalidArgument(_)),
            "expected InvalidArgument for {bad}, got {err:?}"
        );
    }

    // Mid-codepoint UTF-8 byte offsets ("한글" = 6 bytes; 0..1 splits the first 3-byte char) must never panic
    // and must be dropped with a clear warning.
    let mid_char = r#"{
      "sourceFormat":"docx",
      "targetFormat":"docx",
      "blocks":[{
        "type":"table",
        "rows":[[{"text":"한글"}]],
        "links":[{"row":0,"col":0,"start":0,"end":1,"url":"https://example.org"}]
      }]
    }"#;
    let resp = EasyOfficeIpcRouter::dispatch("convert_document_format", mid_char).unwrap();
    assert!(resp.contains("셀 링크 1곳"), "{resp}");

    // Every 18 RichListNumber combinations roundtrip through static wire codes without heap allocation.
    for code in [
        "decimal:dot",
        "decimal:paren",
        "decimal:rparen",
        "lowerLatin:dot",
        "lowerLatin:paren",
        "lowerLatin:rparen",
        "upperLatin:dot",
        "upperLatin:paren",
        "upperLatin:rparen",
        "lowerRoman:dot",
        "lowerRoman:paren",
        "lowerRoman:rparen",
        "upperRoman:dot",
        "upperRoman:paren",
        "upperRoman:rparen",
        "hangul:dot",
        "hangul:paren",
        "hangul:rparen",
    ] {
        let parsed = RichListNumber::from_wire_code(code).expect(code);
        assert_eq!(parsed.to_wire_code(), code);
    }
}

#[test]
#[ignore]
fn bench_pd243_doc_roundtrip_release() {
    let payload = r#"{
      "sourceFormat":"docx",
      "targetFormat":"docx",
      "firstPageDifferent":true,
      "evenOddDifferent":true,
      "header":[{"type":"paragraph","runs":[{"text":"홀수 머리글"}]}],
      "firstHeader":[{"type":"paragraph","runs":[{"text":"첫 쪽 머리글"}]}],
      "evenHeader":[{"type":"paragraph","runs":[{"text":"짝수 머리글"}]}],
      "blocks":[
        {"type":"paragraph","list":"ordered","listNumber":"upperRoman:rparen","columns":{"count":2,"equal":true,"gapTwips":720},"runs":[{"text":"1단계"}]},
        {"type":"paragraph","list":"ordered","listLevel":1,"listNumber":"lowerLatin:paren","columns":{"count":2,"equal":true,"gapTwips":720},"runs":[{"text":"2단계"}]},
        {"type":"table","rows":[[{"text":"부서"},{"text":"안내 바로가기"}]],"links":[{"row":0,"col":1,"start":7,"end":19,"url":"https://example.org/doc"}]}
      ]
    }"#;
    let bytes = save_to_bytes(payload);
    let decode_req = format!(
        r#"{{"action":"decode","fileName":"bench.docx","sourceBase64":"{}"}}"#,
        base64_codec::encode(&bytes)
    );
    let iterations = 200u32;
    let start = std::time::Instant::now();
    for _ in 0..iterations {
        let out = EasyOfficeIpcRouter::dispatch("convert_document_format", payload).unwrap();
        std::hint::black_box(&out);
        let dec = EasyOfficeIpcRouter::dispatch("convert_document_format", &decode_req).unwrap();
        std::hint::black_box(&dec);
    }
    let avg_us = start.elapsed().as_secs_f64() * 1_000_000.0 / f64::from(iterations);
    println!(
        "PD-243 DOCX save+decode roundtrip avg: {avg_us:.1} us ({bytes_len} B)",
        bytes_len = bytes.len()
    );
    assert!(avg_us < 50_000.0);
}

#[test]
fn docx_odt_and_hwpx_ipc_roundtrip_right_indent_twips() {
    for (fmt, file_name) in [
        ("docx", "indent.docx"),
        ("odt", "indent.odt"),
        ("hwpx", "indent.hwpx"),
        ("rtf", "indent.rtf"),
    ] {
        let payload = format!(
            r#"{{
              "sourceFormat":"docx",
              "targetFormat":"{fmt}",
              "blocks":[{{"type":"paragraph","leftIndentTwips":240,"rightIndentTwips":540,"runs":[{{"text":"오른쪽 들여쓰기 테스트"}}]}}]
            }}"#
        );
        let bytes = save_to_bytes(&payload);
        let decoded = decode_bytes(file_name, &bytes);
        let blocks = field(&decoded, "blocks").unwrap().as_array().unwrap();
        let para = &blocks[0];
        assert_eq!(
            field(para, "rightIndentTwips").and_then(StrictJsonValue::as_u64),
            Some(540),
            "{file_name}"
        );
        assert_eq!(
            field(para, "leftIndentTwips").and_then(StrictJsonValue::as_u64),
            Some(240),
            "{file_name}"
        );
    }
}
