use easyoffice_formats::{
    DocumentFormat, EasyOfficeError, HwpxCodec, HwpxPayload, OdfCodec, OoxmlCodec,
    OoxmlXlsxPayload, PdfDocumentModel, PdfPageModel, PdfSerializer, StrictJsonValue,
    ZipArchiveCodec, ZipSecurityPolicy,
};

fn b64_encode_test(raw: &[u8]) -> String {
    const TBL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in raw.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        out.push(TBL[(b0 >> 2) as usize] as char);
        out.push(TBL[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TBL[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TBL[(b2 & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[test]
fn test_f5_ods_xlsx_hwpx_and_pdf_standard_compatibility() {
    let ods_bytes = OdfCodec::pack_ods(
        "예산표",
        &[
            (0, 0, "항목".to_string()),
            (2, 3, "450".to_string()),
            (4, 1, "=D3*2".to_string()),
        ],
    )
    .unwrap();
    let ods_entries = ZipArchiveCodec::unpack(&ods_bytes, &ZipSecurityPolicy::default()).unwrap();
    let ods_xml = std::str::from_utf8(
        &ods_entries
            .iter()
            .find(|e| e.path == "content.xml")
            .unwrap()
            .data,
    )
    .unwrap();
    assert!(ods_xml.matches("<table:table-row").count() >= 3);

    let std_ods_xml = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" office:version=\"1.2\">",
        "<office:body><office:spreadsheet><table:table table:name=\"표준시트\">",
        "<table:table-row>",
        "<table:table-cell office:value-type=\"string\"><text:p>A1</text:p></table:table-cell>",
        "<table:table-cell table:number-columns-repeated=\"2\"/>",
        "<table:table-cell office:value-type=\"float\" office:value=\"42\"><text:p>42</text:p></table:table-cell>",
        "</table:table-row>",
        "<table:table-row table:number-rows-repeated=\"2\"><table:table-cell/></table:table-row>",
        "<table:table-row>",
        "<table:table-cell table:number-columns-repeated=\"1\"/>",
        "<table:table-cell table:formula=\"of:=SUM(D1:D3)\" office:value-type=\"float\" office:value=\"42\"><text:p>42</text:p></table:table-cell>",
        "</table:table-row>",
        "</table:table></office:spreadsheet></office:body></office:document-content>"
    );
    let std_ods_zip = ZipArchiveCodec::pack(&[
        (
            "mimetype".to_string(),
            OdfCodec::MIMETYPE_ODS.as_bytes().to_vec(),
        ),
        ("content.xml".to_string(), std_ods_xml.as_bytes().to_vec()),
    ])
    .unwrap();
    let (sheet_name, unpacked_cells) = OdfCodec::unpack_ods(&std_ods_zip).unwrap();
    assert_eq!(sheet_name, "표준시트");
    assert!(unpacked_cells.contains(&(0, 0, "A1".to_string())));
    assert!(unpacked_cells.contains(&(0, 3, "42".to_string())));
    assert!(unpacked_cells.contains(&(3, 1, "=SUM(D1:D3)".to_string())));

    let xlsx_bytes = OoxmlCodec::pack_xlsx(&OoxmlXlsxPayload {
        sheet_name: "Sheet1".to_string(),
        cells: vec![(0, 0, "A1".to_string()), (2, 1, "B3".to_string())],
    })
    .unwrap();
    let xlsx_entries = ZipArchiveCodec::unpack(&xlsx_bytes, &ZipSecurityPolicy::default()).unwrap();
    let sheet_xml = std::str::from_utf8(
        &xlsx_entries
            .iter()
            .find(|e| e.path == "xl/worksheets/sheet1.xml")
            .unwrap()
            .data,
    )
    .unwrap();
    assert!(sheet_xml.contains("<row r=\"1\">"));
    assert!(sheet_xml.contains("<row r=\"3\">"));

    let hwpx_bytes = HwpxCodec::pack(&HwpxPayload {
        title: "기안문".to_string(),
        is_template: false,
        official_agency: Some("행정안전부".to_string()),
        approval_lines: vec!["기안".to_string()],
        paragraphs: vec!["본문".to_string()],
        tables: Vec::new(),
    })
    .unwrap();
    let hwpx_entries = ZipArchiveCodec::unpack(&hwpx_bytes, &ZipSecurityPolicy::default()).unwrap();
    assert!(hwpx_entries
        .iter()
        .any(|e| e.path == "Contents/content.hpf"));

    let pdf_with_literal_encrypt = PdfSerializer::serialize(&PdfDocumentModel {
        title: "Guide to /Encrypt in PDF".to_string(),
        pages: vec![PdfPageModel {
            page_number: 1,
            rotation_degrees: 0,
            body_text: "Section 1 discusses /Encrypt dictionary.".to_string(),
            approval_stamp: None,
        }],
        watermark_text: None,
    });
    assert!(PdfSerializer::parse(&pdf_with_literal_encrypt).is_ok());

    let mut pdf_with_b64_json = pdf_with_literal_encrypt.clone();
    let b64_trailer = "\n%EasyOfficePDFModelBase64:eyJ0aXRsZSI6Ikd1aWRlIHRvIC9FbmNyeXB0IGluIFBERiIsImJvZHlUZXh0IjoiU2VjdGlvbiAxIGRpc2N1c3NlcyAvRW5jcnlwdCBkaWN0aW9uYXJ5LiJ9\n";
    pdf_with_b64_json.extend_from_slice(b64_trailer.as_bytes());
    assert!(PdfSerializer::parse(&pdf_with_b64_json).is_ok());

    let mut pdf_with_spoofed_b64_json = pdf_with_literal_encrypt;
    let spoofed_trailer = "\n%EasyOfficePDFModelBase64:eyJ0aXRsZSI6IlNwb29mZWQiLCJib2R5VGV4dCI6IkNvbXBsZXRlbHkgZGlmZmVyZW50IGNvbnRlbnQifQ==\n";
    pdf_with_spoofed_b64_json.extend_from_slice(spoofed_trailer.as_bytes());
    assert!(matches!(
        PdfSerializer::parse(&pdf_with_spoofed_b64_json),
        Err(EasyOfficeError::SecurityViolation(_))
    ));

    let korean_pdf_model = PdfDocumentModel {
        title: "한글 보고서".to_string(),
        pages: vec![PdfPageModel {
            page_number: 1,
            rotation_degrees: 0,
            body_text: "첫째 줄 한글 본문\nASCII Line 2".to_string(),
            approval_stamp: Some("전결승인".to_string()),
        }],
        watermark_text: Some("대외비문서".to_string()),
    };
    let korean_pdf_bytes = PdfSerializer::serialize(&korean_pdf_model);
    let parsed_korean_pdf = PdfSerializer::parse(&korean_pdf_bytes).unwrap();
    assert_eq!(parsed_korean_pdf, korean_pdf_model);

    let leading_blank_pdf_model = PdfDocumentModel {
        title: "빈 줄 보존 테스트".to_string(),
        pages: vec![PdfPageModel {
            page_number: 1,
            rotation_degrees: 0,
            body_text: "\n둘째 줄 한글 본문".to_string(),
            approval_stamp: None,
        }],
        watermark_text: None,
    };
    let leading_blank_bytes = PdfSerializer::serialize(&leading_blank_pdf_model);
    assert_eq!(
        PdfSerializer::parse(&leading_blank_bytes).unwrap(),
        leading_blank_pdf_model
    );

    let newline_only_pdf_bytes = PdfSerializer::serialize(&PdfDocumentModel {
        title: "줄바꿈 전용 문서".to_string(),
        pages: vec![PdfPageModel {
            page_number: 1,
            rotation_degrees: 0,
            body_text: "\n".to_string(),
            approval_stamp: None,
        }],
        watermark_text: None,
    });
    let mut spoofed_newline_pdf = newline_only_pdf_bytes;
    spoofed_newline_pdf.extend_from_slice(spoofed_trailer.as_bytes());
    assert!(matches!(
        PdfSerializer::parse(&spoofed_newline_pdf),
        Err(EasyOfficeError::SecurityViolation(_))
    ));

    let external_pdf = concat!(
        "%PDF-1.7\n",
        "1 0 obj << /Type /Catalog /Pages 2 0 R /Title (External Spec) >> endobj\n",
        "2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n",
        "3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n",
        "4 0 obj << /Length 98 >>\nstream\n",
        "BT /F1 11 Tf 72 720 Td (External Line 1) Tj ET\n",
        "BT /F2 11 Tf 72 704 Td <FEFFAC00B098> Tj ET\n",
        "endstream\nendobj\n",
        "xref\n0 5\ntrailer << /Root 1 0 R /Size 5 >>\nstartxref\n0\n%%EOF\n"
    );
    let parsed_external = PdfSerializer::parse(external_pdf.as_bytes()).unwrap();
    assert_eq!(parsed_external.title, "External Spec");
    assert_eq!(parsed_external.pages[0].body_text, "External Line 1\n가나");

    let empty_title_whitespace_pdf = PdfSerializer::serialize(&PdfDocumentModel {
        title: String::new(),
        pages: vec![PdfPageModel {
            page_number: 1,
            rotation_degrees: 0,
            body_text: "   ".to_string(),
            approval_stamp: None,
        }],
        watermark_text: None,
    });
    for bad_json in [
        r#"{"title":"","title":"Spoofed","pages":[{"extractedText":"   "}]}"#,
        r#"{ @#$ "title":"","pages":[{"extractedText":"   "}] }"#,
        r#"{"title":"","pages":[{"extractedText":"   "}]} TRAILING"#,
    ] {
        let mut bad_pdf = empty_title_whitespace_pdf.clone();
        bad_pdf.extend_from_slice(
            format!(
                "\n%EasyOfficePDFModelBase64:{}\n",
                b64_encode_test(bad_json.as_bytes())
            )
            .as_bytes(),
        );
        assert!(matches!(
            PdfSerializer::parse(&bad_pdf),
            Err(EasyOfficeError::SecurityViolation(_))
        ));
    }

    let two_page_pdf = PdfSerializer::serialize(&PdfDocumentModel {
        title: "TwoPages".to_string(),
        pages: vec![
            PdfPageModel {
                page_number: 1,
                rotation_degrees: 0,
                body_text: "A".to_string(),
                approval_stamp: None,
            },
            PdfPageModel {
                page_number: 2,
                rotation_degrees: 0,
                body_text: "B".to_string(),
                approval_stamp: None,
            },
        ],
        watermark_text: None,
    });
    for spoof_two_page_json in [
        r#"{"title":"TwoPages","pages":[{"extractedText":"AB"},{"extractedText":""}]}"#,
        r#"{"title":"TwoPages","pages":[{"extractedText":"AB"}]}"#,
        r#"{"title":"TwoPages","bodyText":"AB"}"#,
        r#"{"title":"TwoPages","pages":[{"bodyText":"A","extractedText":"SPOOFED"},{"extractedText":"B"}]}"#,
    ] {
        let mut boundary_spoof_pdf = two_page_pdf.clone();
        boundary_spoof_pdf.extend_from_slice(
            format!(
                "\n%EasyOfficePDFModelBase64:{}\n",
                b64_encode_test(spoof_two_page_json.as_bytes())
            )
            .as_bytes(),
        );
        assert!(matches!(
            PdfSerializer::parse(&boundary_spoof_pdf),
            Err(EasyOfficeError::SecurityViolation(_))
        ));
    }

    let structural_body_model = PdfDocumentModel {
        title: "StreamFraming".to_string(),
        pages: vec![PdfPageModel {
            page_number: 1,
            rotation_degrees: 0,
            body_text: "Literal /Type /Metadata and /Type /Page /Rotate 90 inside stream"
                .to_string(),
            approval_stamp: None,
        }],
        watermark_text: None,
    };
    let structural_body_bytes = PdfSerializer::serialize(&structural_body_model);
    assert_eq!(
        PdfSerializer::parse(&structural_body_bytes).unwrap(),
        structural_body_model
    );

    let multiline_tj_comment_pdf = b"%PDF-1.7\n1 0 obj << /Type /Catalog /Title (StreamEdge) >> endobj\n2 0 obj << /Type /Page /PageNumber 1 >> endobj\n3 0 obj << /Length 260 >>\nstream\n/F2 11 Tf\nBT /F1 12 Tf 54 760 Td [(Hel) -15 (lo )\n(World)] TJ ET\nBT [3 2] 0 d /Span << /MCID 0 /Lang (ko-KR) >> BDC (\\101\\tLine\\\nCont) Tj EMC ET\nBT /F2 11 Tf ET\nBT <AC00> Tj ET\nendstream\nendobj\nxref\n0 4\ntrailer << /Size 4 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n";
    let parsed_edge = PdfSerializer::parse(multiline_tj_comment_pdf).unwrap();
    assert_eq!(
        parsed_edge.pages[0].body_text,
        "Hello World\nA\tLineCont\n가"
    );
    assert_eq!(PdfSerializer::decode_base64_bytes("YQ==YQ=="), None);
}

#[test]
fn test_pdf_single_line_stream_relative_td_ocr_3tr_and_swift_blank_line_pagination() {
    let single_line_pdf = concat!(
        "%PDF-1.7\n",
        "1 0 obj << /Title (Report (2026)) /Type /Catalog >> endobj\n",
        "2 0 obj << /Type /Metadata >> endobj\n",
        "3 0 obj << /Rotate 90 /PageNumber 1 /Type /Page >> endobj\n",
        "4 0 obj << /Length 120 >> stream BT 3 Tr /F1 11 Tf 100 200 Td (OCR Word1) Tj 0 0 Td (OCR Word2) Tj 286 404 Td ([STAMP] BodyLine) Tj ET endstream endobj\n",
        "xref\n0 5\ntrailer << /Root 1 0 R /Size 5 >>\nstartxref\n0\n%%EOF\n"
    );
    let parsed = PdfSerializer::parse(single_line_pdf.as_bytes()).unwrap();
    assert_eq!(parsed.title, "Report (2026)");
    assert_eq!(parsed.pages.len(), 1);
    assert_eq!(parsed.pages[0].rotation_degrees, 90);
    assert_eq!(
        parsed.pages[0].body_text,
        "OCR Word1\nOCR Word2\n[STAMP] BodyLine"
    );
    assert_eq!(parsed.pages[0].approval_stamp, None);

    let relative_stamp_pdf = concat!(
        "%PDF-1.7\n",
        "1 0 obj << /Type /Catalog /Title (RelativeTd) >> endobj\n",
        "2 0 obj << /Type /Page /PageNumber 1 >> endobj\n",
        "3 0 obj << /Length 96 >>\nstream\n",
        "BT /F1 10 Tf 386 620 Td (Header) Tj 0 -16 Td ([STAMP] 결재승인) Tj ET\n",
        "endstream\nendobj\n",
        "xref\n0 4\ntrailer << /Root 1 0 R /Size 4 >>\nstartxref\n0\n%%EOF\n"
    );
    let parsed_rel = PdfSerializer::parse(relative_stamp_pdf.as_bytes()).unwrap();
    assert_eq!(parsed_rel.pages[0].body_text, "Header");
    assert_eq!(
        parsed_rel.pages[0].approval_stamp.as_deref(),
        Some("결재승인")
    );

    let non_empty_lines: Vec<String> = (1..=45).map(|n| format!("문단_{n:02}")).collect();
    let raw_with_blanks = non_empty_lines.join("\n\n");
    let page1_chunk = non_empty_lines[..42].join("\n");
    let page2_chunk = non_empty_lines[42..].join("\n");
    let swift_style_pdf = format!(
        concat!(
            "%PDF-1.7\n",
            "1 0 obj\n<< /Type /Catalog /Pages 2 0 R /Title (Swift Line1 Line2) >>\nendobj\n",
            "2 0 obj\n<< /Type /Pages /Count 2 /Kids [3 0 R 5 0 R] >>\nendobj\n",
            "3 0 obj\n<< /Type /Page /Parent 2 0 R /PageNumber 1 /Rotate 0 /Contents 4 0 R >>\nendobj\n",
            "4 0 obj\n<< /Length 200 >>\nstream\n",
            "BT 3 Tr /F1 1 Tf 0 0 Td ({}) Tj ET\n",
            "BT 3 Tr /F1 1 Tf 0 2 Td ([FORM] 기안자: 홍길동) Tj ET\n",
            "1 1 1 rg BT 0 Tr /F1 13 Tf 54 775 Td (Banner1) Tj ET\n",
            "endstream\nendobj\n",
            "5 0 obj\n<< /Type /Page /Parent 2 0 R /PageNumber 2 /Rotate 0 /Contents 6 0 R >>\nendobj\n",
            "6 0 obj\n<< /Length 120 >>\nstream\n",
            "BT 3 Tr /F1 1 Tf 0 0 Td ({}) Tj ET\n",
            "1 1 1 rg BT 0 Tr /F1 13 Tf 54 775 Td (Banner2) Tj ET\n",
            "endstream\nendobj\n",
            "7 0 obj\n<< /Type /Metadata /Subtype /EasyOfficeStudio /Length 64 >>\nstream\n",
            "%EasyOfficePDFModelBase64:{}\n",
            "endstream\nendobj\n",
            "xref\n0 8\ntrailer << /Size 8 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n"
        ),
        PdfSerializer::escape_pdf_text(&page1_chunk),
        PdfSerializer::escape_pdf_text(&page2_chunk),
        b64_encode_test(
            format!(
                r#"{{"title":"Swift Line1\nLine2","watermarkText":"","pages":[{{"extractedText":{}}}]}}"#,
                StrictJsonValue::Str(raw_with_blanks).to_json_string()
            )
            .as_bytes()
        )
    );
    let parsed_swift_mp = PdfSerializer::parse(swift_style_pdf.as_bytes()).unwrap();
    assert_eq!(parsed_swift_mp.title, "Swift Line1\nLine2");
    assert_eq!(parsed_swift_mp.pages.len(), 2);
    assert_eq!(parsed_swift_mp.pages[0].body_text, page1_chunk);
    assert_eq!(parsed_swift_mp.pages[1].body_text, page2_chunk);
}

#[test]
fn test_pdf_stream_comment_paren_encrypt_fail_closed_and_non_page_streams() {
    let comment_paren_encrypted_pdf = concat!(
        "%PDF-1.7\n",
        "1 0 obj << /Type /Catalog /Title (EncryptedDoc) >> endobj\n",
        "2 0 obj << /Type /Page /PageNumber 1 >> endobj\n",
        "3 0 obj << /Length 64 >>\nstream\n",
        "% comment with unmatched ( and <\n",
        "BT /F1 12 Tf 72 720 Td (Body) Tj ET\n",
        "endstream\nendobj\n",
        "xref\n0 4\ntrailer << /Root 1 0 R /Encrypt 9 0 R /Size 4 >>\nstartxref\n0\n%%EOF\n"
    );
    assert!(matches!(
        PdfSerializer::parse(comment_paren_encrypted_pdf.as_bytes()),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let missing_endstream_pdf = concat!(
        "%PDF-1.7\n",
        "1 0 obj << /Type /Catalog /Title (Truncated) >> endobj\n",
        "2 0 obj << /Type /Page /PageNumber 1 >> endobj\n",
        "3 0 obj << /Length 64 >>\nstream\n",
        "BT /F1 12 Tf 72 720 Td (Body) Tj ET\n",
        "xref\n0 4\ntrailer << /Root 1 0 R /Size 4 >>\nstartxref\n0\n%%EOF\n"
    );
    assert!(matches!(
        PdfSerializer::parse(missing_endstream_pdf.as_bytes()),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    let spoofed_in_page_stream_pdf = concat!(
        "%PDF-1.7\n",
        "1 0 obj << /Type /Catalog /Title (PageSpoof) >> endobj\n",
        "2 0 obj << /Type /Page /PageNumber 1 >> endobj\n",
        "3 0 obj << /Length 96 >>\nstream\n",
        "%EasyOfficePDFModelBase64:eyJ0aXRsZSI6IlBhZ2VTcG9vZiIsImJvZHlUZXh0IjoiQm9keSJ9\n",
        "BT /F1 12 Tf 72 720 Td (Body) Tj ET\n",
        "endstream\nendobj\n",
        "xref\n0 4\ntrailer << /Root 1 0 R /Size 4 >>\nstartxref\n0\n%%EOF\n"
    );
    assert!(matches!(
        PdfSerializer::parse(spoofed_in_page_stream_pdf.as_bytes()),
        Err(EasyOfficeError::SecurityViolation(_))
    ));

    let non_page_streams_and_q_leading_pdf = concat!(
        "%PDF-1.7\n",
        "1 0 obj << /Type /Catalog /Title (NonPageAndLeading) >> endobj\n",
        "2 0 obj << /Type /Page /PageNumber 1 >> endobj\n",
        "3 0 obj << /Length 140 >>\nstream\n",
        "BT /F1 10 Tf 386 620 Td 16 TL q 99 TL (Header) Tj Q T* ([STAMP] 정위치스탬프) Tj ET\n",
        "endstream\nendobj\n",
        "4 0 obj << /Type /XObject /Subtype /Image /Width 10 /Height 10 /Length 12 >>\nstream\n",
        "(binary_img\n",
        "endstream\nendobj\n",
        "5 0 obj << /Length 12 /Length1 24 >>\nstream\n",
        "<font_bytes\n",
        "endstream\nendobj\n",
        "6 0 obj << /Type /XRef /Size 7 /Length 8 >>\nstream\n",
        "xrefbin\n",
        "endstream\nendobj\n",
        "xref\n0 7\ntrailer << /Root 1 0 R /Size 7 >>\nstartxref\n0\n%%EOF\n"
    );
    let parsed_np = PdfSerializer::parse(non_page_streams_and_q_leading_pdf.as_bytes()).unwrap();
    assert_eq!(parsed_np.pages.len(), 1);
    assert_eq!(parsed_np.pages[0].body_text, "Header");
    assert_eq!(
        parsed_np.pages[0].approval_stamp.as_deref(),
        Some("정위치스탬프")
    );

    let tagged_dict_spoof_pdf = concat!(
        "%PDF-1.7\n",
        "1 0 obj << /Type /Catalog /Title (TaggedSpoof) >> endobj\n",
        "2 0 obj << /Type /Page /PageNumber 1 >> endobj\n",
        "3 0 obj << /Length 94 >>\nstream\n",
        "BT /Span << /MCID 0\n%EasyOfficePDFModelBase64:eyJ0aXRsZSI6IlNwb29mIn0=\n>> BDC (Body) Tj EMC ET\n",
        "endstream\nendobj\n",
        "xref\n0 4\ntrailer << /Root 1 0 R /Size 4 >>\nstartxref\n0\n%%EOF\n"
    );
    assert!(matches!(
        PdfSerializer::parse(tagged_dict_spoof_pdf.as_bytes()),
        Err(EasyOfficeError::SecurityViolation(_))
    ));

    let tagged_dict_valid_pdf = concat!(
        "%PDF-1.7\n",
        "1 0 obj << /Type /Catalog /Title (About /Encrypt Guide) /OpenAction << /S /URI /Title (SubIgnore) >> >> endobj\n",
        "2 0 obj << /Type /Page /PageNumber 1 /Resources << /Font << /F1 << /Type /Font >> >> >> >> endobj\n",
        "3 0 obj << /Length 85 >>\nstream\n",
        "BT /F1 12 Tf 72 720 Td /Span << /Alt (a > b\nendstream\n) >> BDC (TaggedBody) Tj EMC ET\n",
        "endstream\nendobj\n",
        "xref\n0 4\ntrailer << /Root 1 0 R /Size 4 >>\nstartxref\n0\n%%EOF\n"
    );
    let parsed_tagged = PdfSerializer::parse(tagged_dict_valid_pdf.as_bytes()).unwrap();
    assert_eq!(parsed_tagged.title, "About /Encrypt Guide");
    assert_eq!(parsed_tagged.pages.len(), 1);
    assert_eq!(parsed_tagged.pages[0].body_text, "TaggedBody");

    let unencrypted_stream_with_encrypt_text = {
        let body = b"BT /F1 12 Tf 72 720 Td (a\nendstream\n<< /Encrypt ) Tj ET";
        let mut raw = Vec::new();
        raw.extend_from_slice(
            format!(
                "%PDF-1.7\n1 0 obj << /Type /Catalog /Title (LiteralEncrypt) >> endobj\n2 0 obj << /Type /Page /PageNumber 1 >> endobj\n3 0 obj << /Length {} >>\nstream\n",
                body.len()
            )
            .as_bytes(),
        );
        raw.extend_from_slice(body);
        raw.extend_from_slice(
            b"\nendstream\nendobj\nxref\n0 4\ntrailer << /Root 1 0 R /Size 4 >>\nstartxref\n0\n%%EOF\n",
        );
        raw
    };
    let parsed_lit_enc = PdfSerializer::parse(&unencrypted_stream_with_encrypt_text).unwrap();
    assert_eq!(parsed_lit_enc.pages.len(), 1);
    assert_eq!(
        parsed_lit_enc.pages[0].body_text,
        "a\nendstream\n<< /Encrypt "
    );

    let missing_endstream_before_xref_encrypt = concat!(
        "%PDF-1.7\n",
        "1 0 obj << /Type /Catalog /Title (MissingEndstreamEncrypt) >> endobj\n",
        "2 0 obj << /Type /Page /PageNumber 1 >> endobj\n",
        "3 0 obj << /Length 12 >>\nstream\n",
        "cipher_bytes\n",
        "endobj\n",
        "4 0 obj << /Type /XRef /Size 5 /Root 1 0 R /Encrypt 9 0 R /Length 4 >>\nstream\n",
        "0000\n",
        "endstream\nendobj\n",
        "startxref\n0\n%%EOF\n"
    );
    assert!(matches!(
        PdfSerializer::parse(missing_endstream_before_xref_encrypt.as_bytes()),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let unclosed_obj_paren_before_xref_encrypt = concat!(
        "%PDF-1.7\n",
        "1 0 obj << /Type /Catalog /Title (UnclosedCipher >> endobj\n",
        "2 0 obj << /Type /Page /PageNumber 1 >> endobj\n",
        "4 0 obj << /Type /XRef /Size 5 /Root 1 0 R /Encrypt 9 0 R /Length 4 >>\nstream\n",
        "0000\n",
        "endstream\nendobj\n",
        "startxref\n0\n%%EOF\n"
    );
    assert!(matches!(
        PdfSerializer::parse(unclosed_obj_paren_before_xref_encrypt.as_bytes()),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let forged_length_hiding_page2 = {
        let p1_part = b"BT /F1 12 Tf 72 720 Td (Page1) Tj ET\nendstream % comment\nendobj\n4 0 obj << /Type /Page /PageNumber 2 >>\nstream\nBT /F1 12 Tf 72 720 Td (HiddenPage2) Tj ET";
        let mut raw = Vec::new();
        raw.extend_from_slice(
            format!(
                "%PDF-1.7\n1 0 obj << /Type /Catalog /Title (ForgedLen) >> endobj\n2 0 obj << /Type /Page /PageNumber 1 >> endobj\n3 0 obj << /Length {} >>\nstream\n",
                p1_part.len()
            )
            .as_bytes(),
        );
        raw.extend_from_slice(p1_part);
        raw.extend_from_slice(
            b"\nendstream\nendobj\nxref\n0 5\ntrailer << /Root 1 0 R /Size 5 >>\nstartxref\n0\n%%EOF\n",
        );
        raw
    };
    assert!(matches!(
        PdfSerializer::parse(&forged_length_hiding_page2),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    let unclosed_nonstream_paren_after_page1 = concat!(
        "%PDF-1.7\n",
        "1 0 obj << /Type /Catalog /Title (UnclosedTail) >> endobj\n",
        "2 0 obj << /Type /Page /PageNumber 1 >> endobj\n",
        "3 0 obj << /Length 36 >>\nstream\n",
        "BT /F1 12 Tf 72 720 Td (Page1) Tj ET\n",
        "endstream\nendobj\n",
        "4 0 obj << /Note (unclosed_swallow >> endobj\n",
        "5 0 obj << /Type /Page /PageNumber 2 >>\nstream\n",
        "BT /F1 12 Tf 72 720 Td (Page2) Tj ET\n",
        "endstream\nendobj\n",
        "xref\n0 6\ntrailer << /Root 1 0 R /Size 6 >>\nstartxref\n0\n%%EOF\n"
    );
    assert!(matches!(
        PdfSerializer::parse(unclosed_nonstream_paren_after_page1.as_bytes()),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    let tounicode_without_type_cmap_pdf = concat!(
        "%PDF-1.7\n",
        "1 0 obj << /Type /Catalog /Title (ToUnicodeNoType) >> endobj\n",
        "2 0 obj << /Type /Page /PageNumber 1 >> endobj\n",
        "3 0 obj << /Length 35 >>\nstream\n",
        "BT /F1 12 Tf 72 720 Td (Body) Tj ET\n",
        "endstream\nendobj\n",
        "4 0 obj << >>\nstream\n",
        "/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n% ps(unclosed\nendcmap\n",
        "endstream\nendobj\n",
        "xref\n0 5\ntrailer << /Root 1 0 R /Size 5 >>\nstartxref\n0\n%%EOF\n"
    );
    let parsed_tu = PdfSerializer::parse(tounicode_without_type_cmap_pdf.as_bytes()).unwrap();
    assert_eq!(parsed_tu.pages.len(), 1);
    assert_eq!(parsed_tu.pages[0].body_text, "Body");

    let tounicode_spoof_trailer_pdf = concat!(
        "%PDF-1.7\n",
        "1 0 obj << /Type /Catalog /Title (ToUnicodeSpoof) >> endobj\n",
        "2 0 obj << /Type /Page /PageNumber 1 >> endobj\n",
        "3 0 obj << /Length 35 >>\nstream\n",
        "BT /F1 12 Tf 72 720 Td (Body) Tj ET\n",
        "endstream\nendobj\n",
        "4 0 obj << >>\nstream\n",
        "/CIDInit /ProcSet findresource begin begincmap\n% ps(unclosed\n%EasyOfficePDFModelBase64:eyJ0aXRsZSI6IlNwb29mIn0=\nendcmap\n",
        "endstream\nendobj\n",
        "xref\n0 5\ntrailer << /Root 1 0 R /Size 5 >>\nstartxref\n0\n%%EOF\n"
    );
    assert!(matches!(
        PdfSerializer::parse(tounicode_spoof_trailer_pdf.as_bytes()),
        Err(EasyOfficeError::SecurityViolation(_))
    ));

    let binary_high_bytes_and_stream_space_eol = {
        let mut raw = Vec::new();
        raw.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");
        raw.extend_from_slice(b"1 0 obj << /Type /Catalog /Title (HighBytes) >> endobj\n");
        let img_bytes: [u8; 8] = [0x89, 0xFE, 0xA0, b'(', 0xFF, b'<', 0x90, 0x81];
        raw.extend_from_slice(
            format!(
                "2 0 obj << /Type /XObject /Subtype /Image /Length {} >>\nstream \n",
                img_bytes.len()
            )
            .as_bytes(),
        );
        raw.extend_from_slice(&img_bytes);
        raw.extend_from_slice(b"\nendstream\nendobj\n");
        let page_stream = b"BT /F1 12 Tf 72 720 Td (ExactBinaryOffset) Tj ET";
        raw.extend_from_slice(b"3 0 obj << /Type /Page /PageNumber 1 >> endobj\n");
        raw.extend_from_slice(
            format!("4 0 obj << /Length {} >>\nstream \t\r\n", page_stream.len()).as_bytes(),
        );
        raw.extend_from_slice(page_stream);
        raw.extend_from_slice(
            b"\r\nendstream\nendobj\nxref\n0 5\ntrailer << /Root 1 0 R /Size 5 >>\nstartxref\n0\n%%EOF\n",
        );
        raw
    };
    let parsed_bin = PdfSerializer::parse(&binary_high_bytes_and_stream_space_eol).unwrap();
    assert_eq!(parsed_bin.pages.len(), 1);
    assert_eq!(parsed_bin.pages[0].body_text, "ExactBinaryOffset");

    let ciphertext_paren_trailer_encrypt_pdf = concat!(
        "%PDF-1.7\n",
        "1 0 obj << /Type /Catalog /Title (CipherDoc) >> endobj\n",
        "2 0 obj << /Type /Page /PageNumber 1 >> endobj\n",
        "3 0 obj << /Length 18 >>\nstream\n",
        "cipher(text<bytes!\n",
        "endstream\nendobj\n",
        "xref\n0 4\ntrailer << /Root 1 0 R /Encrypt 9 0 R /Size 4 >>\nstartxref\n0\n%%EOF\n"
    );
    assert!(matches!(
        PdfSerializer::parse(ciphertext_paren_trailer_encrypt_pdf.as_bytes()),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let ciphertext_xref_stream_encrypt_pdf = concat!(
        "%PDF-1.7\n",
        "1 0 obj << /Type /Catalog /Title (CipherXRef) >> endobj\n",
        "2 0 obj << /Type /Page /PageNumber 1 >> endobj\n",
        "3 0 obj << /Length 12 >>\nstream\n",
        "cipher(open!\n",
        "endstream\nendobj\n",
        "4 0 obj << /Type /XRef /Size 5 /Root 1 0 R /Encrypt 9 0 R /Length 6 >>\nstream\n",
        "xyz)00\n",
        "endstream\nendobj\n",
        "xref\nstartxref\n0\n%%EOF\n"
    );
    assert!(matches!(
        PdfSerializer::parse(ciphertext_xref_stream_encrypt_pdf.as_bytes()),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let cr_nonpage_spoof_pdf = concat!(
        "%PDF-1.7\n",
        "1 0 obj << /Type /Catalog /Title (CrSpoof) >> endobj\n",
        "2 0 obj << /Type /Page /PageNumber 1 >> endobj\n",
        "3 0 obj << /Length 36 >>\nstream\n",
        "BT /F1 12 Tf 72 720 Td (Body) Tj ET\n",
        "endstream\nendobj\n",
        "4 0 obj << /Type /CMap >>\nstream\r",
        "begincmap\r%EasyOfficePDFModelBase64:eyJ0aXRsZSI6IlNwb29mIn0=\rendcmap\r",
        "endstream\nendobj\n",
        "xref\n0 5\ntrailer << /Root 1 0 R /Size 5 >>\nstartxref\n0\n%%EOF\n"
    );
    assert!(matches!(
        PdfSerializer::parse(cr_nonpage_spoof_pdf.as_bytes()),
        Err(EasyOfficeError::SecurityViolation(_))
    ));
}
