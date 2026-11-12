use super::*;
use easyoffice_formats::{
    OoxmlCodec, RichDocument, RichImage, RichParagraph, RichRun, RichScript, ZipArchiveCodec,
    ZipSecurityPolicy,
};

fn request(action: Option<&str>, file: &str, bytes: &[u8], extra: &str) -> String {
    let action = action.map_or(String::new(), |a| format!("\"action\":\"{a}\","));
    format!(
        "{{{action}\"fileName\":\"{file}\",\"sourceBase64\":\"{}\"{extra}}}",
        EasyOfficeIpcRouter::encode_base64(bytes)
    )
}

fn docx_bytes() -> Vec<u8> {
    let doc = RichDocument {
        title: "t".into(),
        blocks: vec![
            RichBlock::Paragraph(RichParagraph {
                runs: vec![
                    RichRun::plain("앞 "),
                    RichRun {
                        text: "굵게".into(),
                        bold: true,
                        ..RichRun::default()
                    },
                ],
                alignment: Some(RichAlignment::Center),
                heading_level: 1,
                list: None,
                ..RichParagraph::default()
            }),
            RichBlock::Table(vec![vec!["가".into(), "나".into()]].into()),
        ],
        official_agency: None,
    };
    UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).unwrap()
}

fn json_field<'a>(json: &'a str, key: &str) -> &'a str {
    let start = json.find(&format!("\"{key}\":\"")).unwrap() + key.len() + 4;
    &json[start..start + json[start..].find('"').unwrap()]
}

const RUN_PAYLOAD: &str = concat!(
    "{\"targetFormat\":\"docx\",\"title\":\"런\",\"blocks\":[{\"type\":\"paragraph\",\"runs\":[",
    "{\"text\":\"앞 \"},",
    "{\"text\":\"강조\",\"bold\":true,\"sizeHalfPt\":28,\"color\":\"#ff0000\",\"font\":\"함초롬바탕\",\"highlight\":\"FFFF00\"},",
    "{\"text\":\" 뒤\"}]}]}"
);

#[test]
fn structured_runs_with_size_color_font_survive_docx_save_and_reopen() {
    let saved = EasyOfficeIpcRouter::dispatch("convert_document_format", RUN_PAYLOAD).unwrap();
    let bytes = EasyOfficeIpcRouter::decode_base64(json_field(&saved, "convertedBase64")).unwrap();
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let xml = String::from_utf8(
        entries
            .iter()
            .find(|e| e.path == "word/document.xml")
            .unwrap()
            .data
            .clone(),
    )
    .unwrap();
    assert!(
        xml.contains("<w:color w:val=\"FF0000\"/><w:sz w:val=\"28\"/>"),
        "{xml}"
    );
    let json = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &request(Some("decode"), "런.docx", &bytes, ""),
    )
    .unwrap();
    assert!(json.contains("{\"text\":\"앞 \"},"), "{json}");
    assert!(
        json.contains("{\"text\":\"강조\",\"bold\":true,\"sizeHalfPt\":28,\"font\":\"함초롬바탕\",\"color\":\"FF0000\",\"highlight\":\"FFFF00\"}"),
        "{json}"
    );
    assert!(json.contains("{\"text\":\" 뒤\"}"), "{json}");
}

#[test]
fn invalid_run_properties_are_rejected_and_old_payloads_still_work() {
    for bad in [
        "\"sizeHalfPt\":1",
        "\"sizeHalfPt\":99999",
        "\"sizeHalfPt\":\"큼\"",
        "\"color\":\"red\"",
        "\"highlight\":\"#12\"",
        "\"font\":\"a\\u0007b\"",
    ] {
        let payload = format!(
            "{{\"targetFormat\":\"docx\",\"blocks\":[{{\"type\":\"paragraph\",\"runs\":[{{\"text\":\"x\",{bad}}}]}}]}}"
        );
        assert!(
            EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).is_err(),
            "{bad}"
        );
    }
    let old = "{\"targetFormat\":\"docx\",\"blocks\":[{\"type\":\"paragraph\",\"runs\":[{\"text\":\"x\",\"bold\":true}]}]}";
    assert!(EasyOfficeIpcRouter::dispatch("convert_document_format", old).is_ok());
}

#[test]
fn decode_action_returns_structured_blocks_module_and_format() {
    let json = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &request(Some("decode"), "보고서.docx", &docx_bytes(), ""),
    )
    .unwrap();
    assert!(json.contains("\"action\":\"decode\""), "{json}");
    assert!(json.contains("\"format\":\"docx\""), "{json}");
    assert!(json.contains("\"module\":\"DOC\""), "{json}");
    assert!(json.contains("\"kind\":\"document\""), "{json}");
    assert!(json.contains("{\"text\":\"굵게\",\"bold\":true}"), "{json}");
    assert!(
        json.contains("\"align\":\"center\",\"heading\":1"),
        "{json}"
    );
    assert!(
        json.contains("{\"type\":\"table\",\"rows\":[[{\"text\":\"가\"},{\"text\":\"나\"}]]}"),
        "{json}"
    );
}

#[test]
fn decode_action_routes_sheets_and_clips_to_the_editor_grid() {
    let sheets = vec![
        (
            "매출".to_string(),
            vec![
                (0, 0, "1".to_string()),
                (0, 9, "멀리".to_string()),
                (5, 1, "=SUM(A1:A1)".to_string()),
            ],
        ),
        ("요약".to_string(), vec![(0, 0, "x".to_string())]),
    ];
    let bytes = OoxmlCodec::pack_xlsx_multi(&sheets).unwrap();
    let json = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &request(
            Some("decode"),
            "a.xlsx",
            &bytes,
            ",\"maxRows\":3,\"maxCols\":8",
        ),
    )
    .unwrap();
    assert!(json.contains("\"module\":\"SHEET\""), "{json}");
    assert!(json.contains("\"kind\":\"workbook\""), "{json}");
    assert!(json.contains("\"name\":\"매출\""), "{json}");
    assert!(json.contains("\"name\":\"요약\""), "{json}");
    assert!(json.contains("{\"ref\":\"A1\",\"raw\":\"1\"}"), "{json}");
    assert!(
        !json.contains("멀리"),
        "column J is outside the grid: {json}"
    );
    assert!(
        !json.contains("SUM(A1:A1)"),
        "row 6 is outside the grid: {json}"
    );
    assert!(json.contains("셀 2개는 가져오지 않았습니다"), "{json}");
}

const STYLED_SHEETS: &str = concat!(
    "\"sheets\":[{\"name\":\"서식\",\"cells\":[",
    "{\"ref\":\"A1\",\"raw\":\"굵게\",\"style\":{\"bold\":true}},",
    "{\"ref\":\"B1\",\"raw\":\"=1+2\",\"style\":{\"italic\":true,\"underline\":true,\"strike\":true,\"sizePt\":11.25}},",
    "{\"ref\":\"C3\",\"raw\":\"\",\"style\":{\"bold\":true}},",
    "{\"ref\":\"D4\",\"raw\":\"평범\",\"style\":{}}],",
    "\"columns\":[{\"index\":1,\"widthPx\":150},{\"index\":3,\"widthPx\":60}]}]"
);

fn convert_sheets(target: &str, sheets: &str) -> Result<Vec<u8>, EasyOfficeError> {
    let payload = format!("{{\"targetFormat\":\"{target}\",\"title\":\"서식\",{sheets}}}");
    let saved = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload)?;
    Ok(EasyOfficeIpcRouter::decode_base64(json_field(&saved, "convertedBase64")).unwrap())
}

#[test]
fn sheet_styles_and_column_widths_survive_save_and_reopen_in_xlsx_and_ods() {
    for (target, file) in [("xlsx", "서식.xlsx"), ("ods", "서식.ods")] {
        let bytes = convert_sheets(target, STYLED_SHEETS).unwrap();
        let json = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &request(Some("decode"), file, &bytes, ""),
        )
        .unwrap();
        assert!(
            json.contains("{\"ref\":\"A1\",\"raw\":\"굵게\",\"style\":{\"bold\":true}}"),
            "{target}: {json}"
        );
        assert!(
            json.contains(
                "\"style\":{\"italic\":true,\"underline\":true,\"strike\":true,\"sizePt\":11.25}"
            ),
            "{target}: {json}"
        );
        assert!(
            json.contains("{\"ref\":\"C3\",\"raw\":\"\",\"style\":{\"bold\":true}}"),
            "{target}: {json}"
        );
        assert!(
            json.contains("{\"ref\":\"D4\",\"raw\":\"평범\"}"),
            "plain style is dropped: {json}"
        );
        assert!(
            json.contains(
                "\"columns\":[{\"index\":1,\"widthPx\":150},{\"index\":3,\"widthPx\":60}]"
            ),
            "{target}: {json}"
        );
        // A reopened file saved again keeps its formatting (open -> save -> reopen).
        let again = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &request(
                None,
                file,
                &bytes,
                &format!(",\"targetFormat\":\"{target}\""),
            ),
        )
        .unwrap();
        let again_bytes =
            EasyOfficeIpcRouter::decode_base64(json_field(&again, "convertedBase64")).unwrap();
        let json2 = EasyOfficeIpcRouter::dispatch(
            "convert_document_format",
            &request(Some("decode"), file, &again_bytes, ""),
        )
        .unwrap();
        assert!(json2.contains("\"widthPx\":150"), "{target}: {json2}");
        assert!(
            json2.contains("\"style\":{\"bold\":true}"),
            "{target}: {json2}"
        );
    }
}

#[test]
fn decode_clips_styles_and_widths_to_the_editor_grid() {
    let bytes = convert_sheets("xlsx", STYLED_SHEETS).unwrap();
    let json = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &request(
            Some("decode"),
            "a.xlsx",
            &bytes,
            ",\"maxRows\":2,\"maxCols\":2",
        ),
    )
    .unwrap();
    assert!(json.contains("\"ref\":\"A1\""), "{json}");
    assert!(!json.contains("\"ref\":\"C3\""), "{json}");
    assert!(
        json.contains("\"columns\":[{\"index\":1,\"widthPx\":150}]"),
        "{json}"
    );
}

#[test]
fn invalid_sheet_styles_are_rejected_and_old_payloads_still_work() {
    for bad in [
        "\"style\":{\"bold\":\"yes\"}",
        "\"style\":{\"sizePt\":0}",
        "\"style\":{\"sizePt\":9999}",
        "\"style\":{\"sizePt\":\"크게\"}",
        "\"style\":[1]",
    ] {
        let sheets = format!(
            "\"sheets\":[{{\"name\":\"a\",\"cells\":[{{\"ref\":\"A1\",\"raw\":\"x\",{bad}}}]}}]"
        );
        assert!(convert_sheets("xlsx", &sheets).is_err(), "{bad}");
    }
    for bad in [
        "{\"index\":0,\"widthPx\":100}",
        "{\"index\":16385,\"widthPx\":100}",
        "{\"index\":1,\"widthPx\":1}",
        "{\"index\":1,\"widthPx\":99999}",
        "{\"index\":1}",
        "{\"widthPx\":100}",
        "5",
    ] {
        let sheets = format!("\"sheets\":[{{\"name\":\"a\",\"columns\":[{bad}]}}]");
        assert!(convert_sheets("xlsx", &sheets).is_err(), "{bad}");
    }
    let old = "\"sheets\":[{\"name\":\"a\",\"cells\":[{\"ref\":\"A1\",\"raw\":\"x\"}]}]";
    assert!(convert_sheets("xlsx", old).is_ok());
    assert!(convert_sheets("ods", old).is_ok());
}

#[test]
fn styled_sheets_still_convert_to_non_sheet_formats() {
    for target in ["csv", "docx", "xls"] {
        assert!(convert_sheets(target, STYLED_SHEETS).is_ok(), "{target}");
    }
}
#[test]
fn convert_uses_the_source_file_bytes_and_ignores_editor_text() {
    let bytes = docx_bytes();
    let json = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &request(
            None,
            "보고서.docx",
            &bytes,
            ",\"sourceFormat\":\"hwpx\",\"targetFormat\":\"md\",\"title\":\"변환\",\"body\":\"에디터 텍스트\"",
        ),
    )
    .unwrap();
    assert!(json.contains("\"sourceFormat\":\"docx\""), "{json}");
    assert!(json.contains("\"targetFormat\":\"md\""), "{json}");
    assert!(
        json.contains(&format!("\"sourceBytes\":{}", bytes.len())),
        "{json}"
    );
    let key = "\"convertedBase64\":\"";
    let start = json.find(key).unwrap() + key.len();
    let end = json[start..].find('"').unwrap() + start;
    let out = EasyOfficeIpcRouter::decode_base64(&json[start..end]).unwrap();
    let md = String::from_utf8(out).unwrap();
    assert!(md.contains("굵게"), "{md}");
    assert!(md.contains("| 가 | 나 |"), "{md}");
    assert!(!md.contains("에디터 텍스트"), "{md}");
}

#[test]
fn source_file_convert_evaluates_table_formulas_and_preserves_metadata_title() {
    let doc = RichDocument {
        title: "원본 메타 제목".to_string(),
        blocks: vec![RichBlock::Table(
            vec![
                vec!["항목".to_string(), "금액".to_string()],
                vec!["A".to_string(), "15".to_string()],
                vec!["B".to_string(), "25".to_string()],
                vec!["합계".to_string(), "=SUM(ABOVE)".to_string()],
            ]
            .into(),
        )],
        official_agency: None,
    };
    let src_bytes =
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).unwrap();
    let json = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &request(None, "원본.docx", &src_bytes, ",\"targetFormat\":\"docx\""),
    )
    .unwrap();
    let key = "\"convertedBase64\":\"";
    let start = json.find(key).unwrap() + key.len();
    let end = json[start..].find('"').unwrap() + start;
    let out_bytes = EasyOfficeIpcRouter::decode_base64(&json[start..end]).unwrap();
    let decoded = UniversalFormatConverter::decode_rich(&out_bytes, Some("out.docx")).unwrap();
    assert_eq!(decoded.title, "원본 메타 제목");
    let DecodedContent::Document(out_doc) = decoded.content else {
        panic!("document expected");
    };
    let RichBlock::Table(tbl) = &out_doc.blocks[0] else {
        panic!("table expected");
    };
    assert_eq!(tbl.rows[3][1], "40");
}

#[test]
fn source_limit_fits_the_http_ipc_body_with_room_for_the_other_fields() {
    let base64_len = MAX_SOURCE_BYTES.div_ceil(3) * 4;
    let envelope_room = 64 * 1024;
    assert!(
        base64_len + envelope_room < crate::http_server::MAX_IPC_HTTP_BODY_BYTES,
        "a source at the limit must still fit one HTTP IPC request"
    );
}

#[test]
fn source_one_byte_over_the_limit_is_refused_before_decoding() {
    let over = vec![b'a'; MAX_SOURCE_BYTES + 1];
    let err = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &request(Some("decode"), "a.txt", &over, ""),
    )
    .unwrap_err();
    assert!(
        matches!(err, EasyOfficeError::PayloadTooLarge { max, .. } if max == MAX_SOURCE_BYTES),
        "{err:?}"
    );
    let at_limit = vec![b'a'; MAX_SOURCE_BYTES];
    let ok = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &request(Some("decode"), "a.txt", &at_limit, ""),
    );
    assert!(ok.is_ok(), "{ok:?}");
}

#[test]
fn convert_rejects_corrupt_bomb_and_missing_source() {
    let corrupt = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &request(
            Some("decode"),
            "a.docx",
            b"PK\x03\x04junk-not-a-zip-at-all-0123456789",
            "",
        ),
    );
    assert!(corrupt.is_err());

    let zeros = vec![b' '; 24 * 1024 * 1024];
    let bomb = ZipArchiveCodec::pack_deflate(&[("word/document.xml", zeros)]).unwrap();
    let err = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &request(Some("decode"), "bomb.docx", &bomb, ""),
    )
    .unwrap_err();
    assert!(
        !matches!(err, EasyOfficeError::InvalidArgument(_)),
        "{err:?}"
    );

    assert!(EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        "{\"action\":\"decode\",\"fileName\":\"a.docx\"}",
    )
    .is_err());
    assert!(EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        "{\"sourceBase64\":\"!!!\",\"fileName\":\"a.docx\"}",
    )
    .is_err());
}

fn pb_varint(mut v: u64, out: &mut Vec<u8>) {
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

fn pb_field(num: u64, wire: u64, body: &[u8], out: &mut Vec<u8>) {
    pb_varint((num << 3) | wire, out);
    if wire == 2 {
        pb_varint(body.len() as u64, out);
    }
    out.extend_from_slice(body);
}

fn record(id: u64, type_id: u64, payload: &[u8]) -> Vec<u8> {
    let mut mi = Vec::new();
    pb_field(
        1,
        0,
        &{
            let mut v = Vec::new();
            pb_varint(type_id, &mut v);
            v
        },
        &mut mi,
    );
    pb_field(
        3,
        0,
        &{
            let mut v = Vec::new();
            pb_varint(payload.len() as u64, &mut v);
            v
        },
        &mut mi,
    );
    let mut info = Vec::new();
    pb_field(
        1,
        0,
        &{
            let mut v = Vec::new();
            pb_varint(id, &mut v);
            v
        },
        &mut info,
    );
    pb_field(2, 2, &mi, &mut info);
    let mut rec = Vec::new();
    pb_varint(info.len() as u64, &mut rec);
    rec.extend(info);
    rec.extend_from_slice(payload);
    rec
}

fn tiny_pages() -> Vec<u8> {
    let mut storage = Vec::new();
    pb_field(3, 2, "첫 줄\n둘째 줄".as_bytes(), &mut storage);
    let mut reference = Vec::new();
    pb_field(1, 0, &[2], &mut reference);
    let mut root = Vec::new();
    pb_field(4, 2, &reference, &mut root);
    let mut stream = record(1, 10000, &root);
    stream.extend(record(2, 2001, &storage));
    // one literal-only Snappy block in one IWA chunk
    let mut block = Vec::new();
    pb_varint(stream.len() as u64, &mut block);
    let n = stream.len() - 1;
    block.push(61 << 2);
    block.extend_from_slice(&(n as u16).to_le_bytes());
    block.extend_from_slice(&stream);
    let mut iwa = vec![0u8];
    iwa.extend_from_slice(&(block.len() as u32).to_le_bytes()[..3]);
    iwa.extend(block);
    easyoffice_formats::ZipArchiveCodec::pack(&[("Index/Document.iwa".to_string(), iwa)]).unwrap()
}

#[test]
fn pages_opens_through_the_decode_action_and_is_an_experimental_target() {
    let json = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &request(Some("decode"), "메모.pages", &tiny_pages(), ""),
    )
    .unwrap();
    assert!(json.contains("\"module\":\"DOC\""), "{json}");
    assert!(json.contains("\"format\":\"pages\""), "{json}");
    assert!(json.contains("첫 줄") && json.contains("둘째 줄"), "{json}");
    assert!(json.contains("warnings"), "{json}");
    let to_pages = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &request(None, "a.docx", &docx_bytes(), ",\"targetFormat\":\"pages\""),
    );
    let to_pages = to_pages.expect("a document converts to Pages (experimental)");
    assert!(
        to_pages.contains("\"detectedTarget\":\"pages\""),
        "{to_pages}"
    );
    let legacy = easyoffice_formats::ZipArchiveCodec::pack(&[(
        "index.xml".to_string(),
        b"<sl:document/>".to_vec(),
    )])
    .unwrap();
    let err = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &request(Some("decode"), "old.pages", &legacy, ""),
    )
    .unwrap_err();
    assert!(err.to_string().contains("index.xml"), "{err}");
}

fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc =
        easyoffice_formats::IeeeCrc32::update(easyoffice_formats::IeeeCrc32::checksum(kind), data);
    out.extend_from_slice(&crc.to_be_bytes());
    out
}

fn tiny_png() -> Vec<u8> {
    let mut ihdr = 3u32.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&2u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    let raw: Vec<u8> = (0..2)
        .flat_map(|_| std::iter::once(0).chain(0..9))
        .collect();
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    png.extend(png_chunk(b"IHDR", &ihdr));
    png.extend(png_chunk(b"IDAT", &easyoffice_formats::deflate_zlib(&raw)));
    png.extend(png_chunk(b"IEND", &[]));
    png
}

/// The `[...]` array of the `"blocks"` field of a decode response.
fn blocks_array(json: &str) -> &str {
    let start = json.find("\"blocks\":").unwrap() + "\"blocks\":".len();
    let (mut depth, mut in_str, mut esc) = (0usize, false, false);
    for (i, c) in json[start..].char_indices() {
        match (in_str, c) {
            (true, _) if esc => esc = false,
            (true, '\\') => esc = true,
            (true, '"') => in_str = false,
            (true, _) => {}
            (false, '"') => in_str = true,
            (false, '[' | '{') => depth += 1,
            (false, ']' | '}') => {
                depth -= 1;
                if depth == 0 {
                    return &json[start..start + i + 1];
                }
            }
            _ => {}
        }
    }
    panic!("unterminated blocks array");
}

fn rich_sample(png: &[u8]) -> RichDocument {
    RichDocument {
        title: "t".into(),
        blocks: vec![
            RichBlock::Paragraph(RichParagraph {
                runs: vec![
                    RichRun::plain("X"),
                    RichRun {
                        text: "2".into(),
                        script: RichScript::Superscript,
                        ..RichRun::default()
                    },
                ],
                left_indent_twips: Some(720),
                first_line_indent_twips: Some(-360),
                line_spacing_percent: Some(200),
                ..RichParagraph::default()
            }),
            RichBlock::Image(RichImage::new(png.to_vec(), "작은 \"그림\"").unwrap()),
        ],
        official_agency: None,
    }
}

#[test]
fn editor_json_keeps_pictures_scripts_indents_and_spacing_through_open_and_save() {
    let png = tiny_png();
    let doc = rich_sample(&png);
    let docx = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).unwrap();
    let json = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &request(Some("decode"), "a.docx", &docx, ""),
    )
    .unwrap();
    let blocks = blocks_array(&json);
    assert!(blocks.contains("\"script\":\"superscript\""), "{blocks}");
    assert!(blocks.contains("\"leftIndentTwips\":720"), "{blocks}");
    assert!(blocks.contains("\"firstLineIndentTwips\":-360"), "{blocks}");
    assert!(blocks.contains("\"lineSpacingPercent\":200"), "{blocks}");
    assert!(
        blocks.contains("\"type\":\"image\",\"format\":\"png\""),
        "{blocks}"
    );
    assert!(blocks.contains("\"widthPx\":3,\"heightPx\":2"), "{blocks}");

    let payload = format!("{{\"targetFormat\":\"docx\",\"blocks\":{blocks}}}");
    let saved = EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).unwrap();
    let bytes = EasyOfficeIpcRouter::decode_base64(json_field(&saved, "convertedBase64")).unwrap();
    let back = match UniversalFormatConverter::decode_rich(&bytes, Some("b.docx"))
        .unwrap()
        .content
    {
        DecodedContent::Document(d) => d,
        other => panic!("document expected: {other:?}"),
    };
    assert_eq!(back.blocks, doc.blocks);
    let RichBlock::Image(img) = &back.blocks[1] else {
        panic!("image block");
    };
    assert_eq!(img.bytes(), png.as_slice());
}

#[test]
fn editor_json_rejects_damaged_oversized_and_malformed_pictures() {
    let png = tiny_png();
    let good = EasyOfficeIpcRouter::encode_base64(&png);
    let mut flipped = png.clone();
    flipped[30] ^= 0x40;
    let big = "A".repeat(48 * 1_048_576);
    for (label, image) in [
        (
            "flipped",
            format!(
                "\"base64\":\"{}\"",
                EasyOfficeIpcRouter::encode_base64(&flipped)
            ),
        ),
        ("not base64", "\"base64\":\"!!!\"".to_string()),
        ("missing", "\"alt\":\"x\"".to_string()),
        ("oversized", format!("\"base64\":\"{big}\"")),
        (
            "bad size",
            format!("\"base64\":\"{good}\",\"widthEmu\":0,\"heightEmu\":5"),
        ),
    ] {
        let payload =
            format!("{{\"targetFormat\":\"docx\",\"blocks\":[{{\"type\":\"image\",{image}}}]}}");
        assert!(
            EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).is_err(),
            "{label}"
        );
    }
    for bad in ["\"script\":\"above\"", "\"script\":1"] {
        let payload = format!(
            "{{\"targetFormat\":\"docx\",\"blocks\":[{{\"type\":\"paragraph\",\"runs\":[{{\"text\":\"x\",{bad}}}]}}]}}"
        );
        assert!(EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).is_err());
    }
    for bad in [
        "\"leftIndentTwips\":-1",
        "\"lineSpacingPercent\":0",
        "\"lineSpacingPercent\":99999",
        "\"firstLineIndentTwips\":\"큼\"",
    ] {
        let payload = format!(
            "{{\"targetFormat\":\"docx\",\"blocks\":[{{\"type\":\"paragraph\",{bad},\"runs\":[{{\"text\":\"x\"}}]}}]}}"
        );
        assert!(EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).is_err());
    }
}

/// The whole `decode` answer of a workbook, document and slide set, byte for byte: escapes,
/// styled cells without a value, clipping to the grid, the warnings of what was dropped.
#[test]
fn decode_answer_is_byte_exact_for_documents_workbooks_and_slides() {
    use easyoffice_formats::{CellStyle, RichTable};

    let decoded = |content| DecodedDocument {
        format: DocumentFormat::Xlsx,
        title: "제목 \"1\"".to_string(),
        content,
        warnings: vec!["앞 경고".to_string()],
    };
    let bold = CellStyle {
        bold: true,
        ..CellStyle::default()
    };
    let sized = CellStyle {
        italic: true,
        size_pt: Some(11.25),
        ..CellStyle::default()
    };
    let mut styling = SheetStyling::default();
    styling.cells.insert((0, 0), bold);
    styling.cells.insert((1, 1), sized);
    styling.cells.insert((2, 0), bold);
    styling.cells.insert((0, 5), CellStyle::default());
    styling.columns.insert(0, 150);
    styling.columns.insert(4, 60);
    styling.rules.unsupported.push("timePeriod".to_string());
    let sheets = vec![
        (
            "시트\n1".to_string(),
            vec![
                (0, 0, "a\"b".to_string()),
                (0, 1, "=A1".to_string()),
                (1, 1, "탭\t".to_string()),
                (3, 0, "아래".to_string()),
                (0, 6, "오른쪽".to_string()),
            ],
        ),
        ("빈".to_string(), Vec::new()),
    ];
    let styles = vec![("시트\n1".to_string(), styling)];
    let json = decode_response(
        &decoded(DecodedContent::Workbook(sheets.clone())),
        &styles,
        "a.xlsx",
        1234,
        Some(3),
        Some(5),
    )
    .unwrap();
    let spec = DocumentFormat::Xlsx.standard_specification();
    assert_eq!(
        json,
        format!(
            concat!(
                "{{\"status\":\"OK\",\"action\":\"decode\",\"fileName\":\"a.xlsx\",\"format\":\"xlsx\",",
                "\"module\":\"SHEET\",\"kind\":\"workbook\",\"title\":\"제목 \\\"1\\\"\",\"sourceBytes\":1234,",
                "\"specification\":\"{spec}\",\"itemCount\":2,",
                "\"warnings\":[\"앞 경고\",\"편집기 격자(최대 3행 × 5열) 밖의 셀 2개는 가져오지 않았습니다.\",",
                "\"시트\\n1 시트의 timePeriod 규칙은 지원하지 않아 가져오지 않았습니다.\"],",
                "\"sheets\":[{{\"name\":\"시트\\n1\",\"cells\":[",
                "{{\"ref\":\"A1\",\"raw\":\"a\\\"b\",\"style\":{{\"bold\":true}}}},",
                "{{\"ref\":\"B1\",\"raw\":\"=A1\"}},",
                "{{\"ref\":\"B2\",\"raw\":\"탭\\t\",\"style\":{{\"italic\":true,\"sizePt\":11.25}}}},",
                "{{\"ref\":\"A3\",\"raw\":\"\",\"style\":{{\"bold\":true}}}}",
                "],\"columns\":[{{\"index\":1,\"widthPx\":150}},{{\"index\":5,\"widthPx\":60}}]}},",
                "{{\"name\":\"빈\",\"cells\":[],\"columns\":[]}}]}}"
            ),
            spec = spec
        )
    );

    let table = RichTable::with_merges(
        vec![vec!["가".to_string(), "나".to_string()]],
        vec![easyoffice_formats::CellMerge {
            row: 0,
            col: 0,
            row_span: 1,
            col_span: 2,
        }],
    )
    .0;
    let document = RichDocument {
        title: "t".to_string(),
        blocks: vec![
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("본문\\")],
                heading_level: 2,
                ..RichParagraph::default()
            }),
            RichBlock::Table(table),
        ],
        official_agency: None,
    };
    let json = decode_response(
        &decoded(DecodedContent::Document(document)),
        &[],
        "",
        7,
        None,
        None,
    )
    .unwrap();
    assert!(
        json.ends_with(concat!(
            "\"itemCount\":2,\"warnings\":[\"앞 경고\"],\"blocks\":[",
            "{\"type\":\"paragraph\",\"runs\":[{\"text\":\"본문\\\\\"}],\"heading\":2},",
            "{\"type\":\"table\",\"rows\":[[{\"text\":\"가\"},{\"text\":\"나\"}]],",
            "\"merges\":[{\"row\":0,\"col\":0,\"rowSpan\":1,\"colSpan\":2}]}]}"
        )),
        "{json}"
    );

    let json = decode_response(
        &decoded(DecodedContent::Slides(vec![(
            "제목".to_string(),
            vec!["하나".to_string(), "둘\"".to_string()],
            "메모".to_string(),
        )])),
        &[],
        "s.pptx",
        9,
        None,
        None,
    )
    .unwrap();
    assert!(
        json.ends_with(concat!(
            "\"itemCount\":1,\"warnings\":[\"앞 경고\"],\"slides\":[",
            "{\"title\":\"제목\",\"bullets\":[\"하나\",\"둘\\\"\"],\"notes\":\"메모\"}]}"
        )),
        "{json}"
    );
}
