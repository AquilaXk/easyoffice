//! End-to-end JSON IPC (`decode` / `convert` / `inspect` readback) roundtrip and fail-closed
//! bounds for table column widths, cell shading/borders, cell inline images, and image display size.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{
    base64_codec, deflate_zlib, IeeeCrc32, StrictJsonValue, MAX_IMAGES_PER_DOCUMENT,
};

fn make_png_bytes(width: u32, height: u32) -> Vec<u8> {
    let chunk = |kind: &[u8; 4], data: &[u8]| {
        let mut out = (data.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = IeeeCrc32::update(IeeeCrc32::checksum(kind), data);
        out.extend_from_slice(&crc.to_be_bytes());
        out
    };
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    let raw = vec![0u8; height as usize * (1 + width as usize * 3)];
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    png.extend(chunk(b"IHDR", &ihdr));
    png.extend(chunk(b"IDAT", &deflate_zlib(&raw)));
    png.extend(chunk(b"IEND", &[]));
    png
}

fn obj(v: &StrictJsonValue) -> &[(String, StrictJsonValue)] {
    v.as_object().expect("JSON object")
}

fn field<'a>(v: &'a StrictJsonValue, key: &str) -> Option<&'a StrictJsonValue> {
    StrictJsonValue::get_field(obj(v), key)
}

fn save_to_bytes(payload: &str) -> Vec<u8> {
    let resp = EasyOfficeIpcRouter::dispatch("convert_document_format", payload)
        .unwrap_or_else(|e| panic!("save failed: {e}"));
    let parsed = StrictJsonValue::parse(&resp).expect("JSON");
    let b64 = field(&parsed, "convertedBase64")
        .and_then(StrictJsonValue::as_str)
        .expect("convertedBase64");
    base64_codec::decode(b64).expect("base64")
}

fn dispatch_json(payload: &str) -> StrictJsonValue {
    let resp = EasyOfficeIpcRouter::dispatch("convert_document_format", payload)
        .unwrap_or_else(|e| panic!("dispatch failed: {e}"));
    StrictJsonValue::parse(&resp).expect("valid JSON")
}

#[test]
fn ipc_decode_save_and_inspect_preserve_table_styles_widths_and_cell_images() {
    let png_b64 = base64_codec::encode(&make_png_bytes(24, 16));

    for target in ["docx", "odt", "hwpx"] {
        let payload = format!(
            r##"{{
              "sourceFormat":"docx",
              "targetFormat":"{target}",
              "title":"IPC 표 스타일 검증",
              "blocks":[
                {{
                  "type":"image",
                  "base64":"{png_b64}",
                  "widthEmu":2743200,
                  "heightEmu":1828800,
                  "alt":""
                }},
                {{
                  "type":"table",
                  "rows":[
                    [{{"text":"구분"}},{{"text":"값"}}],
                    [{{"text":"항목1"}},{{"text":""}}]
                  ],
                  "colWidthsTwips":[2160, 4320],
                  "cellStyles":[
                    {{
                      "row":0,"col":0,"fill":"#E8F1FC",
                      "borders":{{"top":{{"kind":"thick","color":"#1F4E79"}},"left":{{"kind":"none"}}}}
                    }},
                    {{
                      "row":1,"col":1,"fill":"FFF4CC",
                      "borders":{{"bottom":"double:D93025","right":"none"}}
                    }}
                  ],
                  "cellImages":[
                    {{
                      "row":1,"col":1,"base64":"{png_b64}",
                      "widthEmu":914400,"heightEmu":914400,"alt":"셀 그림"
                    }}
                  ]
                }}
              ]
            }}"##
        );
        let bytes = save_to_bytes(&payload);
        let b64 = base64_codec::encode(&bytes);

        let dec = dispatch_json(&format!(
            r#"{{"action":"decode","fileName":"out.{target}","sourceBase64":"{b64}"}}"#
        ));
        assert!(
            field(&dec, "warnings")
                .and_then(StrictJsonValue::as_array)
                .is_some_and(|w| w.is_empty()),
            "{target} unexpected warnings: {dec:?}"
        );
        let blocks = field(&dec, "blocks")
            .and_then(StrictJsonValue::as_array)
            .expect("blocks");
        assert_eq!(blocks.len(), 2);
        let top_w = field(&blocks[0], "widthEmu")
            .and_then(StrictJsonValue::as_u64)
            .unwrap();
        assert!(
            top_w.abs_diff(2_743_200) <= 640,
            "{target} top widthEmu={top_w}"
        );

        let tbl = &blocks[1];
        let widths: Vec<u64> = field(tbl, "colWidthsTwips")
            .and_then(StrictJsonValue::as_array)
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap())
            .collect();
        assert_eq!(widths, vec![2160, 4320], "{target} colWidthsTwips");

        let styles = field(tbl, "cellStyles")
            .and_then(StrictJsonValue::as_array)
            .unwrap();
        assert_eq!(styles.len(), 2, "{target} cellStyles");
        assert_eq!(
            field(&styles[0], "fill").and_then(StrictJsonValue::as_str),
            Some("E8F1FC")
        );
        let b0 = field(&styles[0], "borders").unwrap();
        assert_eq!(
            field(field(b0, "top").unwrap(), "kind").and_then(StrictJsonValue::as_str),
            Some("thick")
        );
        assert_eq!(
            field(field(b0, "left").unwrap(), "kind").and_then(StrictJsonValue::as_str),
            Some("none")
        );

        let cimgs = field(tbl, "cellImages")
            .and_then(StrictJsonValue::as_array)
            .unwrap();
        assert_eq!(cimgs.len(), 1, "{target} cellImages");
        assert_eq!(
            field(&cimgs[0], "row").and_then(StrictJsonValue::as_u64),
            Some(1)
        );
        assert_eq!(
            field(&cimgs[0], "col").and_then(StrictJsonValue::as_u64),
            Some(1)
        );
        assert_eq!(
            field(&cimgs[0], "format").and_then(StrictJsonValue::as_str),
            Some("png")
        );
        assert_eq!(
            field(&cimgs[0], "alt").and_then(StrictJsonValue::as_str),
            Some("셀 그림")
        );

        let insp = dispatch_json(&format!(
            r#"{{"action":"inspect","fileName":"out.{target}","sourceBase64":"{b64}"}}"#
        ));
        let stats = field(&insp, "statistics").unwrap();
        assert_eq!(
            field(stats, "imageCount").and_then(StrictJsonValue::as_u64),
            Some(2)
        );
        assert_eq!(
            field(stats, "missingAltImageCount").and_then(StrictJsonValue::as_u64),
            Some(1)
        );
        let images = field(&insp, "images")
            .and_then(StrictJsonValue::as_array)
            .unwrap();
        assert_eq!(images.len(), 2);
        assert_eq!(
            field(&images[1], "tableBlockIndex").and_then(StrictJsonValue::as_u64),
            Some(1)
        );
        let iblocks = field(&insp, "blocks")
            .and_then(StrictJsonValue::as_array)
            .unwrap();
        assert_eq!(
            field(&iblocks[1], "hasCustomColWidths").and_then(StrictJsonValue::as_bool),
            Some(true)
        );
        assert_eq!(
            field(&iblocks[1], "styledCellCount").and_then(StrictJsonValue::as_u64),
            Some(2)
        );
        assert_eq!(
            field(&iblocks[1], "cellImageCount").and_then(StrictJsonValue::as_u64),
            Some(1)
        );
    }
}

#[test]
fn hostile_ipc_table_styles_and_excess_cell_images_fail_closed() {
    let png_b64 = base64_codec::encode(&make_png_bytes(8, 8));

    // 1. Invalid fill hex rejects save Fail-Closed.
    let bad_fill = r#"{"sourceFormat":"docx","targetFormat":"docx","blocks":[{"type":"table","rows":[[{"text":"a"}]],"cellStyles":[{"row":0,"col":0,"fill":"not-hex"}]}]}"#;
    assert!(EasyOfficeIpcRouter::dispatch("convert_document_format", bad_fill).is_err());

    // 2. Invalid border kind rejects save Fail-Closed.
    let bad_border = r#"{"sourceFormat":"docx","targetFormat":"docx","blocks":[{"type":"table","rows":[[{"text":"a"}]],"cellStyles":[{"row":0,"col":0,"borders":{"top":{"kind":"zigzag"}}}]}]}"#;
    assert!(EasyOfficeIpcRouter::dispatch("convert_document_format", bad_border).is_err());

    // 3. > MAX_IMAGES_PER_DOCUMENT cell images rejects save Fail-Closed.
    let mut cell_imgs = String::new();
    for i in 0..=MAX_IMAGES_PER_DOCUMENT {
        if i > 0 {
            cell_imgs.push(',');
        }
        cell_imgs.push_str(&format!(
            r#"{{"row":0,"col":0,"base64":"{png_b64}","alt":"x"}}"#
        ));
    }
    let too_many = format!(
        r#"{{"sourceFormat":"docx","targetFormat":"docx","blocks":[{{"type":"table","rows":[[{{"text":"a"}}]],"cellImages":[{cell_imgs}]}}]}}"#
    );
    let err = EasyOfficeIpcRouter::dispatch("convert_document_format", &too_many).unwrap_err();
    assert!(err.to_string().contains("한도"), "unexpected error: {err}");

    // 4. Out-of-bounds cellImages are dropped cleanly with a warning and not serialized.
    let oob = format!(
        r#"{{"sourceFormat":"docx","targetFormat":"docx","blocks":[{{"type":"table","rows":[[{{"text":"a"}}]],"cellImages":[{{"row":9,"col":9,"base64":"{png_b64}","alt":"oob"}}]}}]}}"#
    );
    let resp = EasyOfficeIpcRouter::dispatch("convert_document_format", &oob).unwrap();
    let parsed = StrictJsonValue::parse(&resp).unwrap();
    assert!(!field(&parsed, "warnings")
        .and_then(StrictJsonValue::as_array)
        .unwrap()
        .is_empty());
    let bytes = base64_codec::decode(
        field(&parsed, "convertedBase64")
            .and_then(StrictJsonValue::as_str)
            .unwrap(),
    )
    .unwrap();
    let dec = dispatch_json(&format!(
        r#"{{"action":"decode","fileName":"oob.docx","sourceBase64":"{}"}}"#,
        base64_codec::encode(&bytes)
    ));
    let blocks = field(&dec, "blocks")
        .and_then(StrictJsonValue::as_array)
        .unwrap();
    assert!(field(&blocks[0], "cellImages").is_none());
}
