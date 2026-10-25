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

    let mut zlib_wrapped = vec![0x78, 0x9C];
    zlib_wrapped.extend_from_slice(&compressed);
    let restored_zlib =
        inflate_auto(&zlib_wrapped, 1024 * 1024).expect("inflate_auto should strip zlib header");
    assert_eq!(restored_zlib, original.as_bytes());

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

    let mut zlib_bomb = vec![0x78, 0x9C];
    zlib_bomb.extend_from_slice(&compressed);
    let zlib_err = inflate_auto(&zlib_bomb, 1024)
        .expect_err("zlib-wrapped bomb must return ResourceBudgetExceeded");
    assert!(matches!(
        zlib_err,
        EasyOfficeError::ResourceBudgetExceeded(_)
    ));
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

    let detected_no_hint =
        FormatDetector::detect(&zip_bytes, None).expect("detect compressed hwtx without hint");
    assert_eq!(detected_no_hint, DocumentFormat::Hwtx);
    let detected = FormatDetector::detect(&zip_bytes, Some("official_form.hwtx"))
        .expect("detect compressed hwtx");
    assert_eq!(detected, DocumentFormat::Hwtx);

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
        FormatDetector::detect(&ppsx_zip, None).unwrap(),
        DocumentFormat::Ppsx
    );
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

    let mut fh = vec![0u8; 256];
    fh[..17].copy_from_slice(b"HWP Document File");
    fh[32..36].copy_from_slice(&[0, 3, 0, 5]);
    fh[36..40].copy_from_slice(&1u32.to_le_bytes());

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
    doc_props.push(1);
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
    p_hdr.push(0x01);
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
    tbl.push(1);
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
    doc_stream.push(0);
    doc_stream.extend_from_slice(&1u32.to_le_bytes());
    doc_stream.extend_from_slice(&encode_u16_prefixed("본문"));
    doc_stream.extend_from_slice(&encode_u16_prefixed("양쪽"));
    doc_stream.extend_from_slice(&encode_u16_prefixed("본고딕"));
    doc_stream.extend_from_slice(&[0u8; 9]);
    doc_stream.extend_from_slice(&encode_u16_prefixed("확장 트레일러 문단"));
    doc_stream.extend_from_slice(&0u32.to_le_bytes());

    let doc_container =
        Ole2ContainerCodec::pack(&[(["Wor", "dDocument"].concat(), doc_stream)]).unwrap();
    let unpacked_doc = LegacyOle2Codec::unpack_doc(&doc_container).expect("unpack Swift .doc");
    assert_eq!(unpacked_doc.title, "확장워드제목");
    assert_eq!(
        unpacked_doc.paragraphs,
        vec!["확장 트레일러 문단".to_string()]
    );

    let mut wb = Vec::new();
    wb.extend_from_slice(&LegacyOle2Codec::BIFF8_BOF.to_le_bytes());
    wb.extend_from_slice(&16u16.to_le_bytes());
    wb.extend_from_slice(&[0u8; 16]);

    let formula_utf16 = encode_u16_raw("=SUM(A1:A5)");
    let f_len = (16 + formula_utf16.len()) as u16;
    wb.extend_from_slice(&LegacyOle2Codec::BIFF8_FORMULA.to_le_bytes());
    wb.extend_from_slice(&f_len.to_le_bytes());
    wb.extend_from_slice(&2u16.to_le_bytes());
    wb.extend_from_slice(&1u16.to_le_bytes());
    wb.extend_from_slice(&0u16.to_le_bytes());
    wb.extend_from_slice(&42.0f64.to_bits().to_le_bytes());
    wb.extend_from_slice(&0u16.to_le_bytes());
    wb.extend_from_slice(&formula_utf16);

    wb.extend_from_slice(&LegacyOle2Codec::BIFF8_EOF.to_le_bytes());
    wb.extend_from_slice(&0u16.to_le_bytes());

    let xls_container = Ole2ContainerCodec::pack(&[("Workbook".to_string(), wb)]).unwrap();
    let unpacked_xls = LegacyOle2Codec::unpack_xls(&xls_container).expect("unpack Swift .xls");
    assert_eq!(unpacked_xls.cells, vec![(2, 1, "=SUM(A1:A5)".to_string())]);

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

    let mut bad_tbl_doc = vec![0u8; 64];
    bad_tbl_doc[0..2].copy_from_slice(&LegacyOle2Codec::DOC_FIB_MAGIC.to_le_bytes());
    bad_tbl_doc[2..4].copy_from_slice(&LegacyOle2Codec::DOC_FIB_VERSION.to_le_bytes());
    bad_tbl_doc[24..28].copy_from_slice(&64u32.to_le_bytes());
    bad_tbl_doc[28..32].copy_from_slice(&64u32.to_le_bytes());
    bad_tbl_doc.extend_from_slice(&LegacyOle2Codec::DOC_FIB_MARKER.to_le_bytes());
    bad_tbl_doc.extend_from_slice(&encode_u16_prefixed("제목"));
    bad_tbl_doc.extend_from_slice(&encode_u16_prefixed("A4"));
    bad_tbl_doc.extend_from_slice(&encode_u16_prefixed(""));
    bad_tbl_doc.extend_from_slice(&encode_u16_prefixed(""));
    bad_tbl_doc.push(0);
    bad_tbl_doc.extend_from_slice(&0u32.to_le_bytes());
    bad_tbl_doc.extend_from_slice(&1u32.to_le_bytes());
    bad_tbl_doc.extend_from_slice(&encode_u16_prefixed("표"));
    bad_tbl_doc.push(1);
    bad_tbl_doc.extend_from_slice(&0xFFFF_0001u32.to_le_bytes());
    bad_tbl_doc.extend_from_slice(&0u32.to_le_bytes());
    let bad_doc_stream =
        Ole2ContainerCodec::pack(&[(["Wor", "dDocument"].concat(), bad_tbl_doc)]).unwrap();
    assert!(matches!(
        LegacyOle2Codec::unpack_doc(&bad_doc_stream),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn test_xml_control_stripping_zip_duplicate_lfh_mismatch_and_multiline_pdf_hardening() {
    use easyoffice_formats::{
        PdfDocumentModel, PdfPageModel, PdfSerializer, UniversalFormatConverter, XmlSanitizer,
        ZipEntryDescriptor, ZipSecurityGuard,
    };

    let escaped = XmlSanitizer::escape("A\x00B\x08C\tD\nE\x1FF\u{FFFE}G&H");
    assert_eq!(escaped, "ABC\tD\nEFG&amp;H");
    assert_eq!(
        escaped,
        XmlSanitizer::escape_scalar_baseline("A\x00B\x08C\tD\nE\x1FF\u{FFFE}G&H")
    );
    for sample in ["\t\x08AAAAAA", "\n\x0BAAAAAA", "\r\x0CAAAAAA"] {
        assert_eq!(
            XmlSanitizer::escape(sample),
            XmlSanitizer::escape_scalar_baseline(sample)
        );
    }

    let policy = ZipSecurityPolicy::default();
    assert!(ZipSecurityGuard::validate_archive_entries(
        &[
            ZipEntryDescriptor::new("word/", 0, 0, false),
            ZipEntryDescriptor::new("word/document.xml", 128, 256, false),
        ],
        &policy
    )
    .is_ok());
    assert!(ZipSecurityGuard::validate_archive_entries(
        &[
            ZipEntryDescriptor::new("word/document.xml", 128, 256, false),
            ZipEntryDescriptor::new("word/document.xml", 128, 256, false),
        ],
        &policy
    )
    .is_err());

    let mut zip_bytes = ZipArchiveCodec::pack_deflate(&[ZipEntry::new(
        "word/document.xml",
        b"<w:document/>".to_vec(),
    )])
    .unwrap();
    zip_bytes[30] = b'X';
    assert!(ZipArchiveCodec::unpack(&zip_bytes, &policy).is_err());

    let pdf_model = PdfDocumentModel {
        title: "다중행 PDF".to_string(),
        pages: vec![PdfPageModel {
            page_number: 1,
            rotation_degrees: 90,
            body_text: "첫째 줄\n둘째 줄\n셋째 줄".to_string(),
            approval_stamp: Some("승인필".to_string()),
        }],
        watermark_text: Some("대외비".to_string()),
    };
    let pdf_bytes = PdfSerializer::serialize(&pdf_model);
    let parsed_pdf = PdfSerializer::parse(&pdf_bytes).unwrap();
    assert_eq!(parsed_pdf, pdf_model);

    let mut forged_pdf = pdf_bytes.clone();
    forged_pdf.extend_from_slice(b"\n%EasyOfficePDFModelBase64:!!!FORGED_BASE64!!!\n%%EOF\n");
    assert!(PdfSerializer::parse(&forged_pdf).is_err());

    let xlsx_zip = ZipArchiveCodec::pack_deflate(&[
        ZipEntry::new(
            "[Content_Types].xml",
            "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>"
                .as_bytes()
                .to_vec(),
        ),
        ZipEntry::new(
            "xl/workbook.xml",
            "<workbook><sheets><sheet name=\"예산시트\"/></sheets></workbook>"
                .as_bytes()
                .to_vec(),
        ),
        ZipEntry::new(
            "xl/sharedStrings.xml",
            "<sst><si><r><t>복합 </t></r><r><t>런 텍스트</t></r></si></sst>"
                .as_bytes()
                .to_vec(),
        ),
        ZipEntry::new(
            "xl/worksheets/sheet1.xml",
            "<worksheet><sheetData><row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c><c r=\"B1\" t=\"inlineStr\"><is><t>인라인</t></is></c></row></sheetData></worksheet>"
                .as_bytes()
                .to_vec(),
        ),
    ])
    .unwrap();
    let (_, unpacked_xlsx) = UniversalFormatConverter::decode(&xlsx_zip, Some("xlsx")).unwrap();
    assert_eq!(
        unpacked_xlsx.sheet_cells,
        vec![
            (0, 0, "복합 런 텍스트".to_string()),
            (0, 1, "인라인".to_string()),
        ]
    );

    let pptx_zip = ZipArchiveCodec::pack_deflate(&[
        ZipEntry::new(
            "[Content_Types].xml",
            "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>"
                .as_bytes()
                .to_vec(),
        ),
        ZipEntry::new(
            "ppt/presentation.xml",
            "<p:presentation name=\"덱\"/>".as_bytes().to_vec(),
        ),
        ZipEntry::new(
            "ppt/slides/slide10.xml",
            "<p:sld><a:t>슬라이드 10</a:t></p:sld>".as_bytes().to_vec(),
        ),
        ZipEntry::new(
            "ppt/slides/slide2.xml",
            "<p:sld><a:t>슬라이드 2</a:t></p:sld>".as_bytes().to_vec(),
        ),
        ZipEntry::new("ppt/media/image1.png", vec![0x89, b'P', b'N', b'G', 0x00]),
    ])
    .unwrap();
    let (_, unpacked_pptx) = UniversalFormatConverter::decode(&pptx_zip, Some("pptx")).unwrap();
    assert_eq!(unpacked_pptx.slides.len(), 2);
    assert_eq!(unpacked_pptx.slides[0].0, "슬라이드 2");
    assert_eq!(unpacked_pptx.slides[1].0, "슬라이드 10");
}

#[test]
fn test_mscfb_ministream_hierarchical_storage_and_hwp5_table_budget() {
    let small_a = b"MiniStream entry under 4096 bytes".to_vec();
    let small_b = vec![0x42u8; 1500];
    let large_c = vec![0x7Au8; 6000];
    let cfb = Ole2ContainerCodec::pack(&[
        ("FileHeader".to_string(), small_a.clone()),
        ("BodyText/Section0".to_string(), small_b.clone()),
        ("BodyText/Section1".to_string(), large_c.clone()),
    ])
    .expect("pack CFB with MiniStream and storage hierarchy");
    assert_eq!(&cfb[24..26], &0x003Eu16.to_le_bytes());
    assert_eq!(&cfb[26..28], &0x0003u16.to_le_bytes());
    assert_eq!(&cfb[56..60], &4096u32.to_le_bytes());

    let unpacked = Ole2ContainerCodec::unpack(&cfb).expect("unpack CFB with MiniStream");
    let map: std::collections::BTreeMap<String, Vec<u8>> =
        unpacked.into_iter().map(|e| (e.name, e.data)).collect();
    assert_eq!(map.get("FileHeader"), Some(&small_a));
    assert_eq!(map.get("BodyText/Section0"), Some(&small_b));
    assert_eq!(map.get("BodyText/Section1"), Some(&large_c));

    // HWP 5.0 table cell budget rejection (row_count * col_count > 65,536)
    let mut fh = vec![0u8; 256];
    fh[..17].copy_from_slice(b"HWP Document File");
    fh[32..36].copy_from_slice(&[0, 3, 0, 5]);
    fh[36..40].copy_from_slice(&0u32.to_le_bytes());
    let doc_info = Hwp5Codec::encode_record_header(Hwp5Codec::HWPTAG_DOCUMENT_PROPERTIES, 0, 0);

    let mut bad_sec = Vec::new();
    let mut bad_tbl = Vec::new();
    bad_tbl.extend_from_slice(&1000u16.to_le_bytes());
    bad_tbl.extend_from_slice(&1000u16.to_le_bytes());
    bad_sec.extend_from_slice(&Hwp5Codec::encode_record_header(
        Hwp5Codec::HWPTAG_TABLE,
        1,
        bad_tbl.len(),
    ));
    bad_sec.extend_from_slice(&bad_tbl);
    let bad_hwp = Ole2ContainerCodec::pack(&[
        ("FileHeader".to_string(), fh),
        ("DocInfo".to_string(), doc_info),
        ("BodyText/Section0".to_string(), bad_sec),
    ])
    .unwrap();
    assert!(matches!(
        Hwp5Codec::unpack(&bad_hwp),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn test_biff8_xls_multi_substream_sst_rk_mulrk_and_ppt_recursive_containers() {
    use easyoffice_formats::{LegacyXlsPayload, PdfDocumentModel, PdfPageModel, PdfSerializer};

    let xls_payload = LegacyXlsPayload {
        sheet_name: "재무실적".to_string(),
        cells: vec![
            (0, 0, "항목".to_string()),
            (0, 1, "금액".to_string()),
            (1, 0, "매출액".to_string()),
            (1, 1, "125000.5".to_string()),
        ],
    };
    let packed_xls = LegacyOle2Codec::pack_xls(&xls_payload).expect("pack standard BIFF8 XLS");
    let unpacked_xls = LegacyOle2Codec::unpack_xls(&packed_xls).expect("unpack standard BIFF8 XLS");
    assert_eq!(unpacked_xls, xls_payload);

    // Build a synthetic BIFF8 stream with RK, MULRK, and FORMULA + STRING records
    let mut wb = Vec::new();
    // Workbook Globals BOF (0x0809, dt=0x0005)
    let mut bof_globals = vec![0u8; 16];
    bof_globals[0..2].copy_from_slice(&0x0600u16.to_le_bytes());
    bof_globals[2..4].copy_from_slice(&0x0005u16.to_le_bytes());
    wb.extend_from_slice(&LegacyOle2Codec::BIFF8_BOF.to_le_bytes());
    wb.extend_from_slice(&16u16.to_le_bytes());
    wb.extend_from_slice(&bof_globals);
    // 1-byte cch BOUNDSHEET (0x0085)
    let mut bs = Vec::new();
    bs.extend_from_slice(&0u32.to_le_bytes());
    bs.push(0);
    bs.push(0);
    bs.push(5); // cch = 5
    bs.push(0); // grbit = 0 (compressed Latin-1)
    bs.extend_from_slice(b"Sheet");
    wb.extend_from_slice(&LegacyOle2Codec::BIFF8_BOUNDSHEET.to_le_bytes());
    wb.extend_from_slice(&(bs.len() as u16).to_le_bytes());
    wb.extend_from_slice(&bs);
    // Workbook Globals EOF
    wb.extend_from_slice(&LegacyOle2Codec::BIFF8_EOF.to_le_bytes());
    wb.extend_from_slice(&0u16.to_le_bytes());

    // Worksheet Substream BOF (dt=0x0010)
    let mut bof_sheet = vec![0u8; 16];
    bof_sheet[0..2].copy_from_slice(&0x0600u16.to_le_bytes());
    bof_sheet[2..4].copy_from_slice(&0x0010u16.to_le_bytes());
    wb.extend_from_slice(&LegacyOle2Codec::BIFF8_BOF.to_le_bytes());
    wb.extend_from_slice(&16u16.to_le_bytes());
    wb.extend_from_slice(&bof_sheet);

    // RK record (0x027E): row 0, col 0, integer 42 -> (42 << 2) | 0x02
    let mut rk = Vec::new();
    rk.extend_from_slice(&0u16.to_le_bytes());
    rk.extend_from_slice(&0u16.to_le_bytes());
    rk.extend_from_slice(&0u16.to_le_bytes());
    rk.extend_from_slice(&((42u32 << 2) | 0x02).to_le_bytes());
    wb.extend_from_slice(&0x027Eu16.to_le_bytes());
    wb.extend_from_slice(&(rk.len() as u16).to_le_bytes());
    wb.extend_from_slice(&rk);

    // MULRK record (0x00BD): row 0, colFirst 1..=2, values 10 and 20
    let mut mulrk = Vec::new();
    mulrk.extend_from_slice(&0u16.to_le_bytes());
    mulrk.extend_from_slice(&1u16.to_le_bytes());
    mulrk.extend_from_slice(&0u16.to_le_bytes());
    mulrk.extend_from_slice(&((10u32 << 2) | 0x02).to_le_bytes());
    mulrk.extend_from_slice(&0u16.to_le_bytes());
    mulrk.extend_from_slice(&((20u32 << 2) | 0x02).to_le_bytes());
    mulrk.extend_from_slice(&2u16.to_le_bytes()); // colLast = 2
    wb.extend_from_slice(&0x00BDu16.to_le_bytes());
    wb.extend_from_slice(&(mulrk.len() as u16).to_le_bytes());
    wb.extend_from_slice(&mulrk);

    // FORMULA record (0x0006) with cached string indicator (byte 6 == 0, bytes 12..14 == 0xFFFF) + STRING (0x0207)
    let mut f_rec = vec![0u8; 22];
    f_rec[0..2].copy_from_slice(&1u16.to_le_bytes()); // row 1
    f_rec[2..4].copy_from_slice(&0u16.to_le_bytes()); // col 0
    f_rec[6] = 0x00;
    f_rec[12] = 0xFF;
    f_rec[13] = 0xFF;
    wb.extend_from_slice(&LegacyOle2Codec::BIFF8_FORMULA.to_le_bytes());
    wb.extend_from_slice(&(f_rec.len() as u16).to_le_bytes());
    wb.extend_from_slice(&f_rec);

    let mut str_rec = Vec::new();
    str_rec.extend_from_slice(&6u16.to_le_bytes()); // cch = 6
    str_rec.push(0); // compressed Latin-1
    str_rec.extend_from_slice(b"Result");
    wb.extend_from_slice(&0x0207u16.to_le_bytes());
    wb.extend_from_slice(&(str_rec.len() as u16).to_le_bytes());
    wb.extend_from_slice(&str_rec);

    wb.extend_from_slice(&LegacyOle2Codec::BIFF8_EOF.to_le_bytes());
    wb.extend_from_slice(&0u16.to_le_bytes());

    let cfb_xls = Ole2ContainerCodec::pack(&[("Workbook".to_string(), wb)]).unwrap();
    let parsed_xls = LegacyOle2Codec::unpack_xls(&cfb_xls).unwrap();
    assert_eq!(parsed_xls.sheet_name, "Sheet");
    assert_eq!(
        parsed_xls.cells,
        vec![
            (0, 0, "42".to_string()),
            (0, 1, "10".to_string()),
            (0, 2, "20".to_string()),
            (1, 0, "Result".to_string()),
        ]
    );

    // Nested PPT container with RT_TextBytesAtom (0x0FA8) for Title (0) and Body (1)
    let mut nested_atoms = Vec::new();
    // RT_TextHeaderAtom (0x0F9F), textType = 0 (Title)
    nested_atoms.extend_from_slice(&0u16.to_le_bytes());
    nested_atoms.extend_from_slice(&0x0F9Fu16.to_le_bytes());
    nested_atoms.extend_from_slice(&4u32.to_le_bytes());
    nested_atoms.extend_from_slice(&0u32.to_le_bytes());
    // RT_TextBytesAtom (0x0FA8)
    let title_ascii = b"Nested PPT Title";
    nested_atoms.extend_from_slice(&0u16.to_le_bytes());
    nested_atoms.extend_from_slice(&0x0FA8u16.to_le_bytes());
    nested_atoms.extend_from_slice(&(title_ascii.len() as u32).to_le_bytes());
    nested_atoms.extend_from_slice(title_ascii);
    // RT_TextHeaderAtom (0x0F9F), textType = 1 (Body)
    nested_atoms.extend_from_slice(&0u16.to_le_bytes());
    nested_atoms.extend_from_slice(&0x0F9Fu16.to_le_bytes());
    nested_atoms.extend_from_slice(&4u32.to_le_bytes());
    nested_atoms.extend_from_slice(&1u32.to_le_bytes());
    // RT_TextBytesAtom (0x0FA8)
    let body_ascii = b"Bullet 1\rBullet 2";
    nested_atoms.extend_from_slice(&0u16.to_le_bytes());
    nested_atoms.extend_from_slice(&0x0FA8u16.to_le_bytes());
    nested_atoms.extend_from_slice(&(body_ascii.len() as u32).to_le_bytes());
    nested_atoms.extend_from_slice(body_ascii);

    // Wrap inside RT_Drawing container (ver_inst = 0x000F, rec_type = 0x040C)
    let mut slide_body = Vec::new();
    slide_body.extend_from_slice(&0x000Fu16.to_le_bytes());
    slide_body.extend_from_slice(&0x040Cu16.to_le_bytes());
    slide_body.extend_from_slice(&(nested_atoms.len() as u32).to_le_bytes());
    slide_body.extend_from_slice(&nested_atoms);

    // Wrap inside RT_SlideContainer (0x03EE)
    let mut ppt_stream = Vec::new();
    ppt_stream.extend_from_slice(&0x000Fu16.to_le_bytes());
    ppt_stream.extend_from_slice(&LegacyOle2Codec::RT_SLIDE.to_le_bytes());
    ppt_stream.extend_from_slice(&(slide_body.len() as u32).to_le_bytes());
    ppt_stream.extend_from_slice(&slide_body);

    let cfb_ppt =
        Ole2ContainerCodec::pack(&[("PowerPoint Document".to_string(), ppt_stream)]).unwrap();
    let parsed_ppt = LegacyOle2Codec::unpack_ppt(&cfb_ppt).unwrap();
    assert_eq!(parsed_ppt.slides.len(), 1);
    assert_eq!(parsed_ppt.slides[0].title, "Nested PPT Title");
    assert_eq!(
        parsed_ppt.slides[0].bullets,
        vec!["Bullet 1".to_string(), "Bullet 2".to_string()]
    );

    // Multi-page PDF pagination (60 lines -> 2 /Page objects) and [WM]/[STAMP] preservation in body
    let sixty_lines: String = (1..=60)
        .map(|i| {
            if i == 1 {
                "[WM] 본문 첫 줄".to_string()
            } else if i == 2 {
                "[STAMP] 본문 둘째 줄".to_string()
            } else {
                format!("Line {i}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let multi_pdf = PdfDocumentModel {
        title: "Pagination Test".to_string(),
        pages: vec![PdfPageModel {
            page_number: 1,
            rotation_degrees: 0,
            body_text: sixty_lines.clone(),
            approval_stamp: Some("전결".to_string()),
        }],
        watermark_text: Some("CONFIDENTIAL".to_string()),
    };
    let multi_pdf_bytes = PdfSerializer::serialize(&multi_pdf);
    let multi_pdf_text = String::from_utf8_lossy(&multi_pdf_bytes);
    assert!(
        multi_pdf_text.contains("/Count 2"),
        "60 lines must paginate into 2 PDF /Page objects"
    );
    assert!(
        !multi_pdf_text.contains(" -"),
        "No negative y coordinates allowed in PDF stream"
    );
    let reparsed_multi = PdfSerializer::parse(&multi_pdf_bytes).unwrap();
    assert_eq!(reparsed_multi.pages.len(), 2);
    let rejoined: String = reparsed_multi
        .pages
        .iter()
        .map(|p| p.body_text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(rejoined, sixty_lines);
}

#[test]
fn test_markdown_multi_table_escaped_pipe_empty_cell_and_dash_row_roundtrip() {
    use easyoffice_formats::{UniversalDocumentModel, UniversalFormatConverter};

    let expected_tables = vec![
        vec![
            vec!["".to_string(), "A\\|B".to_string(), "".to_string()],
            vec!["---".to_string(), "---".to_string(), "---".to_string()],
            vec!["-".to_string(), "-".to_string(), "-".to_string()],
        ],
        vec![
            vec!["구분".to_string(), "수치|단위".to_string()],
            vec!["-".to_string(), "-".to_string()],
        ],
    ];
    let model = UniversalDocumentModel {
        title: "표 라운드트립 문서".to_string(),
        official_agency: None,
        paragraphs: vec!["본문 요약".to_string()],
        tables: expected_tables.clone(),
        sheet_cells: Vec::new(),
        slides: Vec::new(),
    };

    let md_bytes = UniversalFormatConverter::encode(&model, DocumentFormat::Markdown).unwrap();
    let (fmt, decoded) = UniversalFormatConverter::decode(&md_bytes, Some("report.md")).unwrap();
    assert_eq!(fmt, DocumentFormat::Markdown);
    assert_eq!(decoded.tables.len(), 2);
    assert_eq!(decoded.tables, expected_tables);
}
