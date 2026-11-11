mod pages_fixture;

use easyoffice_formats::pages_iwa::{
    decompress_iwa, snappy_decompress, split_records, MAX_CHUNK_BYTES,
};
use easyoffice_formats::pages_proto::{
    parse_archive_info, read_varint, zigzag_decode, Message, MAX_PROTO_DEPTH,
};
use easyoffice_formats::{
    DecodedContent, DocumentFormat, EasyOfficeError, FormatDetector, RichAlignment, RichBlock,
    RichListKind, RichScript, UniversalDocumentModel, UniversalFormatConverter, ZipArchiveCodec,
};
use pages_fixture::*;

fn sample_blocks() -> (Vec<RichBlock>, Vec<String>) {
    let doc = UniversalFormatConverter::decode_rich(&sample_document(), Some("a.pages")).unwrap();
    assert_eq!(doc.format, DocumentFormat::Pages);
    let DecodedContent::Document(d) = doc.content else {
        panic!("expected a document");
    };
    assert_eq!(d.title, "Title text");
    (d.blocks, doc.warnings)
}

#[test]
fn snappy_literal_and_copy_round_trip() {
    let mut data = Vec::new();
    for i in 0..4000u32 {
        data.extend_from_slice(format!("row {} abcabcabc ", i % 17).as_bytes());
    }
    for enc in [snappy_literal(&data), snappy_compress(&data)] {
        assert_eq!(snappy_decompress(&enc, MAX_CHUNK_BYTES).unwrap(), data);
    }
    assert!(
        snappy_compress(&data).len() < data.len() / 2,
        "copies must be used"
    );
}

#[test]
fn snappy_overlapping_copy_repeats_bytes() {
    // literal "ab" then copy len 6 offset 2 -> "abababab"
    let mut enc = vec![8u8, 1 << 2, b'a', b'b'];
    enc.push((((6 - 1) as u8) << 2) | 2);
    enc.extend_from_slice(&2u16.to_le_bytes());
    assert_eq!(snappy_decompress(&enc, 64).unwrap(), b"abababab");

    // literal "x" then copy len 5 offset 1 -> "xxxxxx"
    let rle = vec![6u8, 0, b'x', (((5 - 4) as u8) << 2) | 1, 1];
    assert_eq!(snappy_decompress(&rle, 64).unwrap(), b"xxxxxx");
}

#[test]
fn snappy_rejects_malformed_blocks() {
    // offset 0, offset beyond output, declared length mismatch, over-cap declaration
    assert!(snappy_decompress(&[4, 0, b'a', (3 << 2) | 2, 0, 0], 64).is_err());
    assert!(snappy_decompress(&[4, 0, b'a', (3 << 2) | 2, 5, 0], 64).is_err());
    assert!(snappy_decompress(&[5, 0, b'a'], 64).is_err());
    assert!(snappy_decompress(&[1, 0, b'a', 0, b'b'], 64).is_err());
    let mut huge = Vec::new();
    varint(u64::MAX, &mut huge);
    assert!(snappy_decompress(&huge, MAX_CHUNK_BYTES).is_err());
    let mut over = Vec::new();
    varint(MAX_CHUNK_BYTES as u64 + 1, &mut over);
    assert!(matches!(
        snappy_decompress(&over, MAX_CHUNK_BYTES),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn iwa_framing_and_records_round_trip_across_chunks() {
    let payload_a = vec![7u8; 100_000];
    let payload_b = f_str(1, "hello");
    let mut stream = record(5, 2001, &payload_a);
    stream.extend(record(6, 2022, &payload_b));
    let framed = iwa_frame(&stream, true);
    let mut budget = 1 << 30;
    let back = decompress_iwa(&framed, &mut budget).unwrap();
    assert_eq!(back, stream);
    assert_eq!(budget, (1 << 30) - stream.len());
    let recs = split_records(&back).unwrap();
    assert_eq!(recs.len(), 2);
    assert_eq!(recs[0].info.identifier, 5);
    assert_eq!(recs[0].info.messages[0].type_id, 2001);
    assert_eq!(recs[0].payloads[0], payload_a);
    assert_eq!(recs[1].payloads[0], payload_b);
}

#[test]
fn iwa_budgets_and_bad_headers_are_rejected() {
    let stream = record(1, 1, &vec![1u8; 4096]);
    let framed = iwa_frame(&stream, false);
    let mut small = 100usize;
    assert!(matches!(
        decompress_iwa(&framed, &mut small),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
    let mut bad_type = framed.clone();
    bad_type[0] = 1;
    assert!(decompress_iwa(&bad_type, &mut 1_000_000).is_err());
    assert!(decompress_iwa(&framed[..framed.len() - 3], &mut 1_000_000).is_err());
    assert!(decompress_iwa(&framed[..3], &mut 1_000_000).is_err());
    // payload length beyond the stream
    let mut truncated = record(1, 1, b"abcdef");
    truncated.truncate(truncated.len() - 2);
    assert!(split_records(&truncated).is_err());
}

#[test]
fn protobuf_wire_decoding() {
    let mut v = Vec::new();
    varint(300, &mut v);
    let mut pos = 0;
    assert_eq!(read_varint(&v, &mut pos).unwrap(), 300);
    assert_eq!(zigzag_decode(0), 0);
    assert_eq!(zigzag_decode(1), -1);
    assert_eq!(zigzag_decode(2), 1);
    assert_eq!(zigzag_decode(u64::MAX), i64::MIN);
    let mut over = vec![0xFFu8; 9];
    over.push(0x02);
    assert!(read_varint(&over, &mut 0).is_err());

    let msg = cat(&[
        f_varint(1, 42),
        f_str(2, "hi"),
        f_f32(3, 1.5),
        {
            let mut o = Vec::new();
            varint((4 << 3) | 1, &mut o);
            o.extend_from_slice(&9u64.to_le_bytes());
            o
        },
        f_bytes(5, &f_varint(1, 7)),
    ]);
    let m = Message::parse(&msg).unwrap();
    assert_eq!(m.varint(1), Some(42));
    assert_eq!(m.string(2).unwrap().as_deref(), Some("hi"));
    assert_eq!(m.float(3), Some(1.5));
    assert_eq!(m.sub(5).unwrap().unwrap().varint(1), Some(7));

    for wire in [3u64, 4, 6, 7] {
        let mut g = Vec::new();
        varint((1 << 3) | wire, &mut g);
        g.push(0);
        assert!(
            Message::parse(&g).is_err(),
            "wire type {wire} must be rejected"
        );
    }
    assert!(Message::parse(&[0x00, 0x01]).is_err(), "field number 0");
    assert!(
        Message::parse(&[0x0A, 0x05, 1]).is_err(),
        "length beyond buffer"
    );
    let mut packed = Vec::new();
    for x in [1u64, 300, 5] {
        varint(x, &mut packed);
    }
    let packed_msg = f_bytes(5, &packed);
    let pm = Message::parse(&packed_msg).unwrap();
    assert_eq!(pm.repeated_varints(5).unwrap(), vec![1, 300, 5]);
}

#[test]
fn protobuf_depth_is_capped() {
    let mut payload = f_varint(1, 1);
    for _ in 0..(MAX_PROTO_DEPTH as usize + 8) {
        payload = f_bytes(1, &payload);
    }
    let mut m = Message::parse(&payload).unwrap();
    let mut failed = false;
    for _ in 0..(MAX_PROTO_DEPTH as usize + 8) {
        match m.sub(1) {
            Ok(Some(next)) => m = next,
            Ok(None) => break,
            Err(_) => {
                failed = true;
                break;
            }
        }
    }
    assert!(failed, "nesting beyond the cap must be rejected");
}

#[test]
fn archive_info_parses_references_and_caps_messages() {
    let mi = cat(&[
        f_varint(1, 2001),
        f_varint(3, 10),
        f_bytes(5, &{
            let mut p = Vec::new();
            varint(9, &mut p);
            varint(10, &mut p);
            p
        }),
    ]);
    let info = cat(&[f_varint(1, 77), f_bytes(2, &mi)]);
    let a = parse_archive_info(&info).unwrap();
    assert_eq!(a.identifier, 77);
    assert_eq!(a.messages[0].object_references, vec![9, 10]);
    let many: Vec<Vec<u8>> = (0..9).map(|_| f_bytes(2, &mi)).collect();
    let too_many = cat(&[f_varint(1, 1), cat(&many)]);
    assert!(parse_archive_info(&too_many).is_err());
    assert!(
        parse_archive_info(&f_bytes(2, &mi)).is_err(),
        "identifier is required"
    );
}

#[test]
fn synthetic_document_maps_to_rich_blocks() {
    let (blocks, warnings) = sample_blocks();
    let RichBlock::Paragraph(h) = &blocks[0] else {
        panic!("heading")
    };
    assert_eq!(h.plain_text(), "Title text");
    assert_eq!(h.heading_level, 1);
    assert_eq!(h.alignment, Some(RichAlignment::Left));
    assert!(h.runs[0].bold);
    assert_eq!(h.runs[0].size_half_pt, Some(34));

    let RichBlock::Paragraph(p) = &blocks[1] else {
        panic!("body")
    };
    assert_eq!(p.heading_level, 0);
    let runs: Vec<_> = p.runs.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(runs, ["Plain ", "bold ", "ital"]);
    assert!(!p.runs[0].bold && !p.runs[0].underline);
    assert!(p.runs[1].bold && p.runs[1].underline && !p.runs[1].italic);
    assert!(p.runs[2].italic && p.runs[2].strike && !p.runs[2].bold);
    assert_eq!(p.runs[2].color.as_deref(), Some("FF0000"));

    let RichBlock::Paragraph(item) = &blocks[2] else {
        panic!("list")
    };
    assert_eq!(item.list, Some(RichListKind::Bullet));
    let RichBlock::Paragraph(centered) = &blocks[3] else {
        panic!("centered")
    };
    assert_eq!(centered.alignment, Some(RichAlignment::Center));
    assert_eq!(centered.list, None);

    let RichBlock::Table(table) = &blocks[4] else {
        panic!("table")
    };
    assert_eq!(
        &table.rows,
        &vec![
            vec!["A1".to_string(), "B1".into()],
            vec!["A2".into(), "B2".into()]
        ]
    );
    let RichBlock::Paragraph(last) = blocks.last().unwrap() else {
        panic!("last")
    };
    assert_eq!(last.plain_text(), "Last");
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("그림 데이터 참조가 없는 그림") && w.contains("1개")),
        "skipped image must be reported: {warnings:?}"
    );
}

#[test]
fn detection_covers_pages_legacy_and_encrypted() {
    let ok = sample_document();
    assert_eq!(
        FormatDetector::detect(&ok, None).unwrap(),
        DocumentFormat::Pages
    );
    assert_eq!(
        FormatDetector::detect(&ok, Some("x.pages")).unwrap(),
        DocumentFormat::Pages
    );
    assert!(FormatDetector::detect(b"not a zip", Some("x.pages")).is_err());
    assert_eq!(
        DocumentFormat::from_extension("PAGES"),
        Some(DocumentFormat::Pages)
    );
    assert!(DocumentFormat::Pages.is_experimental_write());
    assert_eq!(DocumentFormat::Pages.editor_module(), "DOC");
    assert!(!DocumentFormat::ALL.contains(&DocumentFormat::Pages));

    let legacy = ZipArchiveCodec::pack(&[
        ("index.xml".to_string(), b"<sl:document/>".to_vec()),
        ("QuickLook/Thumbnail.jpg".to_string(), vec![1, 2, 3]),
    ])
    .unwrap();
    assert_eq!(
        FormatDetector::detect(&legacy, None).unwrap(),
        DocumentFormat::Pages
    );
    let err = UniversalFormatConverter::decode_rich(&legacy, Some("old.pages")).unwrap_err();
    assert!(err.to_string().contains("index.xml"), "{err}");
    let err = UniversalFormatConverter::decode_rich(&legacy, None).unwrap_err();
    assert!(err.to_string().contains("2009"), "{err}");

    let encrypted = ZipArchiveCodec::pack(&[
        ("Index/Document.iwa".to_string(), vec![0, 1, 2]),
        ("Index/PasswordHint.iwph".to_string(), vec![1]),
    ])
    .unwrap();
    assert_eq!(
        UniversalFormatConverter::decode_rich(&encrypted, Some("e.pages")).unwrap_err(),
        EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Pages)
    );
}

#[test]
fn package_directory_zip_is_supported() {
    let bytes = sample_document();
    let entries =
        ZipArchiveCodec::unpack(&bytes, &easyoffice_formats::ZipSecurityPolicy::default()).unwrap();
    let wrapped: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| (format!("My Doc.pages/{}", e.path), e.data))
        .collect();
    let zip = ZipArchiveCodec::pack(&wrapped).unwrap();
    let doc = UniversalFormatConverter::decode_rich(&zip, Some("My Doc.pages")).unwrap();
    assert!(matches!(doc.content, DecodedContent::Document(d) if d.blocks.len() > 4));
}

#[test]
fn pages_is_a_source_but_never_a_target() {
    let bytes = sample_document();
    let docx =
        UniversalFormatConverter::convert(&bytes, Some("a.pages"), DocumentFormat::Docx).unwrap();
    let (fmt, model) = UniversalFormatConverter::decode(&docx, Some("docx")).unwrap();
    assert_eq!(fmt, DocumentFormat::Docx);
    assert!(model.paragraphs.iter().any(|p| p == "Title text"));
    assert!(model.tables.iter().any(|t| t[1][1] == "B2"));
    let err =
        UniversalFormatConverter::encode(&UniversalDocumentModel::default(), DocumentFormat::Pages)
            .unwrap_err();
    assert!(matches!(err, EasyOfficeError::InvalidArgument(_)));
}

#[test]
fn full_style_properties_and_title_extraction() {
    let mut b = DocBuilder::default();
    let color =
        |r: f32, g: f32, bl: f32| cat(&[f_f32(3, r), f_f32(4, g), f_f32(5, bl), f_f32(6, 1.0)]);
    let spacing = |amount: f32| cat(&[f_varint(1, 0), f_f32(2, amount)]);

    // Full paragraph style (PARA_PROPS = 12) with first_line_indent (13), left_indent (16),
    // line_spacing (19), plus default char props (CHAR_PROPS = 11) with highlight (19) and subscript (10).
    let full_para = b.add(
        2022,
        &cat(&[
            f_bytes(1, &f_str(2, "x-paragraphstyle-Body")),
            f_bytes(
                11,
                &cat(&[
                    f_varint(10, 2),
                    f_str(16, "Menlo"),
                    f_bytes(19, &color(1.0, 1.0, 0.0)),
                ]),
            ),
            f_bytes(
                12,
                &cat(&[f_f32(13, 18.0), f_f32(16, 36.0), f_bytes(19, &spacing(1.5))]),
            ),
        ]),
    );

    // Style with both COMPACT_PARA_PROPS (3) and PARA_PROPS (12): compact takes precedence for
    // left_indent, while full supplies first_line_indent and line_spacing.
    let mixed_para = b.add(
        2022,
        &cat(&[
            f_bytes(1, &f_str(2, "x-paragraphstyle-Body")),
            f_bytes(3, &f_f32(7, 24.0)),
            f_bytes(
                12,
                &cat(&[f_f32(13, 12.0), f_f32(16, 36.0), f_bytes(19, &spacing(2.0))]),
            ),
        ]),
    );

    // Named character style (CHAR_PROPS = 11) with superscript (10), font (16), highlight (19).
    let full_char = b.add(
        2021,
        &f_bytes(
            11,
            &cat(&[
                f_varint(10, 1),
                f_str(16, "Courier New"),
                f_bytes(19, &color(0.0, 1.0, 0.0)),
            ]),
        ),
    );

    let text = "Base Styled\nMixed";
    let para_tbl = [(0, Some(full_para)), (12, Some(mixed_para))];
    let char_tbl = [(5, Some(full_char)), (12, None)];
    let st = b.add(
        2001,
        &DocBuilder::storage_payload(text, &para_tbl, &char_tbl, &[], &[]),
    );
    let zip = pages_zip(b.finish(st));

    let doc = UniversalFormatConverter::decode_rich(&zip, Some("no-title.pages")).unwrap();
    let DecodedContent::Document(d) = doc.content else {
        panic!("expected a document");
    };
    assert_eq!(d.title, "");

    let RichBlock::Paragraph(p0) = &d.blocks[0] else {
        panic!("p0");
    };
    assert_eq!(p0.first_line_indent_twips, Some(360));
    assert_eq!(p0.left_indent_twips, Some(720));
    assert_eq!(p0.line_spacing_percent, Some(150));
    // Run 0 inherits paragraph default char props (subscript + yellow highlight, no template font).
    assert_eq!(p0.runs[0].text, "Base ");
    assert_eq!(p0.runs[0].script, RichScript::Subscript);
    assert_eq!(p0.runs[0].highlight.as_deref(), Some("FFFF00"));
    assert_eq!(p0.runs[0].font, None);
    // Run 1 overrides with full character style (superscript + Courier New + green highlight).
    assert_eq!(p0.runs[1].text, "Styled");
    assert_eq!(p0.runs[1].script, RichScript::Superscript);
    assert_eq!(p0.runs[1].font.as_deref(), Some("Courier New"));
    assert_eq!(p0.runs[1].highlight.as_deref(), Some("00FF00"));

    let RichBlock::Paragraph(p1) = &d.blocks[1] else {
        panic!("p1");
    };
    assert_eq!(p1.left_indent_twips, Some(480));
    assert_eq!(p1.first_line_indent_twips, Some(240));
    assert_eq!(p1.line_spacing_percent, Some(200));
}
