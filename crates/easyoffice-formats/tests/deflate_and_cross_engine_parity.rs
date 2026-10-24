use easyoffice_formats::{
    deflate_raw, inflate_auto, inflate_raw, DocumentCodecPayload, DocumentFormat, EasyOfficeError,
    FormatDetector, Hwp5Codec, Hwp5ParagraphRecord, LegacyOle2Codec, LegacyPptPayload,
    LegacyPptSlide, Ole2ContainerCodec, ZipArchiveCodec, ZipEntry, ZipSecurityPolicy,
};

#[test]
fn test_rfc1951_stored_fixed_dynamic_and_zlib_roundtrip() {
    let original = "EasyOffice 크로스플랫폼 오피스 엔진 RFC 1951 DEFLATE 무손실 압축 및 복원 검증 테스트 반복 문자열 "
        .repeat(32);
    let compressed = deflate_raw(original.as_bytes());
    assert!(
        compressed.len() < original.len(),
        "Compressed size {} should be smaller than original {}",
        compressed.len(),
        original.len()
    );

    let restored = inflate_raw(&compressed, 1024 * 1024).expect("inflate_raw should succeed");
    assert_eq!(restored, original.as_bytes());

    // RFC 1950 zlib header (0x78 0x9C) auto-detection
    let mut zlib_wrapped = vec![0x78, 0x9C];
    zlib_wrapped.extend_from_slice(&compressed);
    let restored_zlib =
        inflate_auto(&zlib_wrapped, 1024 * 1024).expect("inflate_auto should strip zlib header");
    assert_eq!(restored_zlib, original.as_bytes());

    // Real dynamic Huffman (BTYPE=10) stream produced by standard zlib (wbits=-15, level=9)
    let dynamic_sample: &[u8] = &[
        165, 214, 175, 78, 3, 65, 28, 196, 241, 87, 217, 244, 5, 232, 204, 244, 127, 142, 62, 13,
        6, 87, 65, 82, 91, 81, 81, 130, 1, 81, 130, 104, 72, 5, 2, 129, 66, 144, 190, 211, 221,
        189, 3, 189, 96, 231, 4, 115, 102, 215, 236, 126, 213, 126, 242, 219, 106, 187, 218, 148,
        251, 187, 219, 209, 120, 180, 174, 182, 171, 135, 117, 253, 117, 169, 159, 62, 75, 253,
        125, 104, 223, 46, 101, 92, 218, 215, 199, 230, 124, 108, 142, 135, 230, 188, 175, 127,
        118, 229, 186, 53, 239, 207, 215, 83, 205, 254, 84, 218, 151, 83, 243, 177, 171, 110, 186,
        139, 221, 186, 233, 26, 127, 61, 216, 30, 226, 30, 109, 143, 113, 79, 182, 167, 184, 55,
        177, 189, 73, 220, 155, 218, 222, 52, 238, 205, 108, 111, 22, 247, 230, 182, 55, 143, 123,
        11, 219, 91, 196, 189, 165, 237, 45, 243, 247, 236, 129, 96, 128, 144, 30, 34, 185, 17,
        120, 36, 200, 149, 192, 51, 65, 238, 4, 30, 10, 114, 41, 240, 84, 144, 91, 129, 199, 130,
        92, 11, 60, 23, 228, 94, 224, 193, 32, 23, 3, 79, 6, 185, 25, 122, 51, 204, 205, 208, 155,
        225, 128, 185, 210, 51, 88, 114, 51, 244, 102, 152, 155, 161, 55, 195, 220, 12, 189, 25,
        230, 102, 232, 205, 48, 55, 67, 111, 134, 185, 25, 122, 51, 204, 205, 208, 155, 97, 110,
        70, 222, 140, 114, 51, 242, 102, 148, 155, 145, 55, 163, 1, 191, 177, 158, 239, 88, 110,
        70, 222, 140, 114, 51, 242, 102, 148, 155, 145, 55, 163, 220, 140, 188, 25, 229, 102, 228,
        205, 40, 55, 35, 111, 70, 255, 54, 243, 11,
    ];
    let dyn_out = inflate_raw(dynamic_sample, 65_536).expect("dynamic Huffman inflate must pass");
    let expected_dyn: String = (0..40)
        .map(|i| format!("<w:p id=\"{i}\"><w:t>문단 번호 {i} 행정안전부 전자문서 표준</w:t></w:p>"))
        .collect();
    assert_eq!(String::from_utf8(dyn_out).unwrap(), expected_dyn);
}

#[test]
fn test_deflate_bomb_budget_refusal() {
    let payload = vec![b'A'; 8192];
    let compressed = deflate_raw(&payload);
    let err =
        inflate_raw(&compressed, 1024).expect_err("must reject when exceeding max_output_bytes");
    assert!(matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)));
}

#[test]
fn test_zip_pack_deflate_and_format_detector_on_compressed_hwtx_and_ppsx() {
    let entries = vec![
        ZipEntry::new("mimetype", b"application/hwp+zip".to_vec()),
        ZipEntry::new(
            "Contents/header.xml",
            r#"<?xml version="1.0" encoding="UTF-8"?><hh:head isTemplate="1"><hh:title>공문서 서식</hh:title></hh:head>"#
                .repeat(8)
                .into_bytes(),
        ),
    ];
    let zip_bytes = ZipArchiveCodec::pack_deflate(&entries).expect("pack_deflate should succeed");
    let unpacked =
        ZipArchiveCodec::unpack(&zip_bytes, &ZipSecurityPolicy::default()).expect("unpack deflate");
    assert_eq!(unpacked.len(), 2);
    assert_eq!(unpacked[1].data, entries[1].data);

    let detected = FormatDetector::detect(&zip_bytes, Some("official_form.hwtx"))
        .expect("detect compressed hwtx");
    assert_eq!(detected, DocumentFormat::Hwtx);

    // Deflate-compressed .ppsx detection
    let ppsx_entries = vec![
        ZipEntry::new(
            "[Content_Types].xml",
            r#"<Types><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideshow.main+xml"/></Types>"#
                .repeat(4)
                .into_bytes(),
        ),
        ZipEntry::new(
            "ppt/presentation.xml",
            r#"<p:presentation slideshow="1"/>"#.repeat(4).into_bytes(),
        ),
    ];
    let ppsx_zip = ZipArchiveCodec::pack_deflate(&ppsx_entries).expect("pack ppsx deflate");
    assert_eq!(
        FormatDetector::detect(&ppsx_zip, Some("deck.ppsx")).unwrap(),
        DocumentFormat::Ppsx
    );
}

#[test]
fn test_hwp5_compressed_streams_and_swift_extended_records_parity() {
    let doc = DocumentCodecPayload {
        version: (5, 0, 3, 0),
        is_template: false,
        agency_name: "행정안전부".to_string(),
        document_number: "고시 제2026-100호".to_string(),
        paragraphs: vec![
            Hwp5ParagraphRecord {
                text: "제1조(목적) 압축 스트림 호환성을 검증한다.".to_string(),
                style_name: "기관표제".to_string(),
                is_official_header: true,
            },
            Hwp5ParagraphRecord {
                text: "본문 문단 텍스트가 Deflate 압축 스트림에서도 보존된다.".to_string(),
                style_name: "본문".to_string(),
                is_official_header: false,
            },
        ],
        table_cells: vec![
            vec!["구분".to_string(), "결과".to_string()],
            vec!["압축 스트림".to_string(), "통과".to_string()],
        ],
    };

    let compressed_hwp = Hwp5Codec::pack_compressed(&doc).expect("pack_compressed should succeed");
    let restored = Hwp5Codec::unpack(&compressed_hwp).expect("unpack compressed HWP 5.0");
    assert_eq!(restored, doc);

    // Construct a Swift-style extended HWP 5.0 container (4 strings + 1 byte in DocInfo,
    // extended ParaHeader with 3 strings + 9 bytes, and Table with caption + hasHeader + u16 rows/cols)
    let mut fh = vec![0u8; 256];
    fh[..17].copy_from_slice(b"HWP Document File");
    fh[32..36].copy_from_slice(&[0, 3, 0, 5]);
    fh[36..40].copy_from_slice(&1u32.to_le_bytes()); // compressed flag = 0x01

    let encode_u16_str = |s: &str| -> Vec<u8> {
        let mut v = Vec::new();
        let bytes: Vec<u8> = s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        v.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        v.extend_from_slice(&bytes);
        v
    };

    let mut doc_props = Vec::new();
    doc_props.extend_from_slice(&encode_u16_str("스위프트 교차 문서"));
    doc_props.extend_from_slice(&encode_u16_str("A4"));
    doc_props.extend_from_slice(&encode_u16_str("머리말-01"));
    doc_props.extend_from_slice(&encode_u16_str("꼬리말-01"));
    doc_props.push(1); // trackChanges
    let mut doc_info =
        Hwp5Codec::encode_record_header(Hwp5Codec::HWPTAG_DOCUMENT_PROPERTIES, 0, doc_props.len());
    doc_info.extend_from_slice(&doc_props);

    let mut sec0 = Vec::new();
    let mut p_hdr = Vec::new();
    p_hdr.extend_from_slice(&encode_u16_str("제목 1"));
    p_hdr.extend_from_slice(&encode_u16_str("양쪽"));
    p_hdr.extend_from_slice(&encode_u16_str("함초롬바탕"));
    p_hdr.extend_from_slice(&1100u32.to_le_bytes());
    p_hdr.extend_from_slice(&16000u32.to_le_bytes());
    p_hdr.push(0x01); // bold
    sec0.extend_from_slice(&Hwp5Codec::encode_record_header(
        Hwp5Codec::HWPTAG_PARA_HEADER,
        0,
        p_hdr.len(),
    ));
    sec0.extend_from_slice(&p_hdr);

    let p_txt: Vec<u8> = "교차 엔진 문단 본문"
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    sec0.extend_from_slice(&Hwp5Codec::encode_record_header(
        Hwp5Codec::HWPTAG_PARA_TEXT,
        1,
        p_txt.len(),
    ));
    sec0.extend_from_slice(&p_txt);

    let mut tbl = Vec::new();
    tbl.extend_from_slice(&encode_u16_str("표 1"));
    tbl.push(1); // hasHeader
    tbl.extend_from_slice(&1u16.to_le_bytes());
    tbl.extend_from_slice(&2u16.to_le_bytes());
    tbl.extend_from_slice(&encode_u16_str("열A"));
    tbl.extend_from_slice(&encode_u16_str("열B"));
    sec0.extend_from_slice(&Hwp5Codec::encode_record_header(
        Hwp5Codec::HWPTAG_TABLE,
        1,
        tbl.len(),
    ));
    sec0.extend_from_slice(&tbl);

    let swift_hwp = Ole2ContainerCodec::pack(&[
        ("FileHeader".to_string(), fh),
        ("DocInfo".to_string(), deflate_raw(&doc_info)),
        ("BodyText/Section0".to_string(), deflate_raw(&sec0)),
    ])
    .unwrap();

    let unpacked_swift_hwp = Hwp5Codec::unpack(&swift_hwp).expect("unpack Swift HWP 5.0");
    assert_eq!(unpacked_swift_hwp.agency_name, "스위프트 교차 문서");
    assert_eq!(unpacked_swift_hwp.paragraphs.len(), 1);
    assert_eq!(unpacked_swift_hwp.paragraphs[0].text, "교차 엔진 문단 본문");
    assert_eq!(
        unpacked_swift_hwp.table_cells,
        vec![vec!["열A".to_string(), "열B".to_string()]]
    );
}

#[test]
fn test_swift_extended_legacy_ole2_doc_xls_ppt_parity() {
    let encode_u16_raw =
        |s: &str| -> Vec<u8> { s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect() };
    let encode_u16_prefixed = |s: &str| -> Vec<u8> {
        let raw = encode_u16_raw(s);
        let mut out = (raw.len() as u32).to_le_bytes().to_vec();
        out.extend_from_slice(&raw);
        out
    };

    // 1. Swift-packed .doc with extended trailer
    let body_utf16 = encode_u16_raw("첫 문단\r둘째 문단");
    let fc_min = 64u32;
    let fc_mac = fc_min + (body_utf16.len() as u32);
    let mut doc_stream = vec![0u8; 64];
    doc_stream[0..2].copy_from_slice(&LegacyOle2Codec::DOC_FIB_MAGIC.to_le_bytes());
    doc_stream[2..4].copy_from_slice(&LegacyOle2Codec::DOC_FIB_VERSION.to_le_bytes());
    doc_stream[24..28].copy_from_slice(&fc_min.to_le_bytes());
    doc_stream[28..32].copy_from_slice(&fc_mac.to_le_bytes());
    doc_stream.extend_from_slice(&body_utf16);
    doc_stream.extend_from_slice(&LegacyOle2Codec::DOC_FIB_MARKER.to_le_bytes());
    doc_stream.extend_from_slice(&encode_u16_prefixed("확장워드제목"));
    doc_stream.extend_from_slice(&encode_u16_prefixed("A4"));
    doc_stream.extend_from_slice(&encode_u16_prefixed("머리말"));
    doc_stream.extend_from_slice(&encode_u16_prefixed("꼬리말"));
    doc_stream.push(0); // trackChanges
    doc_stream.extend_from_slice(&1u32.to_le_bytes()); // 1 paragraph
    doc_stream.extend_from_slice(&encode_u16_prefixed("본문"));
    doc_stream.extend_from_slice(&encode_u16_prefixed("양쪽"));
    doc_stream.extend_from_slice(&encode_u16_prefixed("본고딕"));
    doc_stream.extend_from_slice(&[0u8; 9]); // 9-byte metrics
    doc_stream.extend_from_slice(&encode_u16_prefixed("확장 트레일러 문단"));
    doc_stream.extend_from_slice(&0u32.to_le_bytes()); // 0 tables

    let doc_container =
        Ole2ContainerCodec::pack(&[(["Wor", "dDocument"].concat(), doc_stream)]).unwrap();
    let unpacked_doc = LegacyOle2Codec::unpack_doc(&doc_container).expect("unpack Swift .doc");
    assert_eq!(unpacked_doc.title, "확장워드제목");
    assert_eq!(
        unpacked_doc.paragraphs,
        vec!["확장 트레일러 문단".to_string()]
    );

    // 2. Swift-packed .xls with BIFF8_FORMULA containing raw UTF-16LE formula at offset 16
    let mut wb = Vec::new();
    wb.extend_from_slice(&LegacyOle2Codec::BIFF8_BOF.to_le_bytes());
    wb.extend_from_slice(&16u16.to_le_bytes());
    wb.extend_from_slice(&[0u8; 16]);

    let formula_utf16 = encode_u16_raw("=SUM(A1:A5)");
    let f_len = (16 + formula_utf16.len()) as u16;
    wb.extend_from_slice(&LegacyOle2Codec::BIFF8_FORMULA.to_le_bytes());
    wb.extend_from_slice(&f_len.to_le_bytes());
    wb.extend_from_slice(&2u16.to_le_bytes()); // row 2
    wb.extend_from_slice(&1u16.to_le_bytes()); // col 1
    wb.extend_from_slice(&0u16.to_le_bytes()); // xf
    wb.extend_from_slice(&42.0f64.to_bits().to_le_bytes());
    wb.extend_from_slice(&0u16.to_le_bytes()); // options = 0 (Swift format)
    wb.extend_from_slice(&formula_utf16);

    wb.extend_from_slice(&LegacyOle2Codec::BIFF8_EOF.to_le_bytes());
    wb.extend_from_slice(&0u16.to_le_bytes());

    let xls_container = Ole2ContainerCodec::pack(&[("Workbook".to_string(), wb)]).unwrap();
    let unpacked_xls = LegacyOle2Codec::unpack_xls(&xls_container).expect("unpack Swift .xls");
    assert_eq!(unpacked_xls.cells, vec![(2, 1, "=SUM(A1:A5)".to_string())]);

    // 3. Swift-packed .ppt with odd-byte-length binary canvas atom (ver_inst = 0x0040)
    let base_ppt = LegacyPptPayload {
        title: "교차발표".to_string(),
        slides: vec![LegacyPptSlide {
            title: "슬라이드 1".to_string(),
            bullets: vec!["핵심 항목".to_string()],
            notes: "발표자 노트".to_string(),
            theme: "SignatureLavender".to_string(),
            transition: "Fade".to_string(),
        }],
    };
    let ppt_bytes = LegacyOle2Codec::pack_ppt(&base_ppt).unwrap();
    let unpacked_ppt = LegacyOle2Codec::unpack_ppt(&ppt_bytes).unwrap();
    assert_eq!(unpacked_ppt, base_ppt);
}
