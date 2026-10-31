use easyoffice_formats::{
    adler32, deflate_zlib, DocumentFormat, EasyOfficeError, PdfDocumentModel, PdfPageModel,
    PdfSerializer,
};

fn build_stored_zlib(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + (payload.len() / 65_535 + 2) * 5 + 6);
    out.extend_from_slice(&[0x78, 0x01]);
    if payload.is_empty() {
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
    } else {
        let chunks: Vec<&[u8]> = payload.chunks(65_535).collect();
        for (idx, chunk) in chunks.iter().enumerate() {
            let is_last = idx + 1 == chunks.len();
            out.push(u8::from(is_last));
            let len = chunk.len() as u16;
            let nlen = !len;
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&nlen.to_le_bytes());
            out.extend_from_slice(chunk);
        }
    }
    out.extend_from_slice(&adler32(payload).to_be_bytes());
    out
}

#[test]
fn test_pdf_serializer_roundtrip_closed_boundary_and_encrypt_literals() {
    let doc = PdfDocumentModel {
        title: "Note: endobj trailer << /Encrypt 9 0 R >>".to_string(),
        pages: vec![
            PdfPageModel {
                page_number: 1,
                rotation_degrees: 0,
                body_text: [
                    "Note: endobj xref table",
                    "Example: endobj 2 0 obj",
                    "Note: endobj trailer << /Encrypt 9 0 R >>",
                    "한글 문장 endobj xref 확인",
                ]
                .join("\n"),
                approval_stamp: Some("승인완료".to_string()),
            },
            PdfPageModel {
                page_number: 2,
                rotation_degrees: 90,
                body_text: "endstream\nendobj\n2 0 obj << /Encrypt 1 0 R >>".to_string(),
                approval_stamp: None,
            },
        ],
        watermark_text: Some("대외비".to_string()),
    };

    let bytes = PdfSerializer::serialize(&doc);
    let parsed = PdfSerializer::parse(&bytes).expect("valid serialized PDF must round-trip");
    assert_eq!(parsed, doc);
}

#[test]
fn test_truncated_two_page_unencrypted_pdf_with_closed_encrypt_strings_returns_corrupted() {
    let p1_stream = b"BT /F1 12 Tf 72 720 Td (endobj 2 0 obj trailer << /Encrypt 1 0 R >>) Tj ET";
    let mut raw = Vec::new();
    raw.extend_from_slice(
        b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R /Title (endobj 2 0 obj trailer << /Encrypt 1 0 R >>) >> endobj\n",
    );
    raw.extend_from_slice(b"2 0 obj << /Type /Pages /Count 2 /Kids [3 0 R 5 0 R] >> endobj\n");
    raw.extend_from_slice(b"3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n");
    raw.extend_from_slice(
        format!("4 0 obj << /Length {} >>\nstream\n", p1_stream.len()).as_bytes(),
    );
    raw.extend_from_slice(p1_stream);
    raw.extend_from_slice(b"\nendstream\nendobj\n");
    raw.extend_from_slice(b"5 0 obj << /Type /Page /Parent 2 0 R /Contents 6 0 R >> endobj\n");
    raw.extend_from_slice(
        b"6 0 obj << /Length 120 >>\nstream\nBT /F1 12 Tf (truncated_p2) Tj ET\nendobj\n",
    );
    raw.extend_from_slice(b"xref\n0 7\ntrailer << /Size 7 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n");

    assert!(matches!(
        PdfSerializer::parse(&raw),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    let full_two_page = PdfSerializer::serialize(&PdfDocumentModel {
        title: "endobj 2 0 obj trailer << /Encrypt 1 0 R >>".to_string(),
        pages: vec![
            PdfPageModel {
                page_number: 1,
                rotation_degrees: 0,
                body_text: "endobj 2 0 obj trailer << /Encrypt 1 0 R >>".to_string(),
                approval_stamp: None,
            },
            PdfPageModel {
                page_number: 2,
                rotation_degrees: 0,
                body_text: "Page two intact text".to_string(),
                approval_stamp: None,
            },
        ],
        watermark_text: None,
    });
    let chopped = &full_two_page[..full_two_page.len() - 60];
    assert!(matches!(
        PdfSerializer::parse(chopped),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    let unclosed_into_xref_stream = concat!(
        "%PDF-1.7\n",
        "1 0 obj << /Type /Catalog /Title (UnclosedCipher >> endobj\n",
        "2 0 obj << /Type /Page /PageNumber 1 >> endobj\n",
        "4 0 obj << /Type /XRef /Size 5 /Root 1 0 R /Encrypt 9 0 R /Length 6 >>\nstream\n",
        "xyz)00\n",
        "endstream\nendobj\n",
        "startxref\n0\n%%EOF\n"
    );
    assert!(matches!(
        PdfSerializer::parse(unclosed_into_xref_stream.as_bytes()),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));
}

#[test]
fn test_direct_length_binary_stream_allows_boundary_bytes_and_forward_filter_resolves() {
    let binary_payload = b"\x89PNG\r\n(unclosed\nendstream\nendobj\n99 0 obj << /Encrypt 1 0 R >>";
    let page_flate = deflate_zlib(b"BT /F1 12 Tf 72 720 Td (ForwardFlateBody) Tj ET");
    let mut pdf = Vec::new();
    pdf.extend_from_slice(
        b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R /Title (DirectLenAndForwardFilter) >> endobj\n",
    );
    pdf.extend_from_slice(b"2 0 obj << /Type /Pages /Count 1 /Kids [3 0 R] >> endobj\n");
    pdf.extend_from_slice(
        b"3 0 obj << /Type /Page /Parent 2 0 R /Thumb 6 0 R /Contents 4 0 R >> endobj\n",
    );
    pdf.extend_from_slice(
        format!(
            "4 0 obj << /Length {} /Filter 9 0 R >>\nstream\n",
            page_flate.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(&page_flate);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");
    pdf.extend_from_slice(
        format!(
            "6 0 obj << /Subtype /Image /Length {} >>\nstream\n",
            binary_payload.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(binary_payload);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");
    pdf.extend_from_slice(b"9 0 obj [/FlateDecode] endobj\n");
    pdf.extend_from_slice(b"xref\n0 10\ntrailer << /Size 10 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n");

    let parsed =
        PdfSerializer::parse(&pdf).expect("direct /Length binary + forward /Filter must pass");
    assert_eq!(parsed.title, "DirectLenAndForwardFilter");
    assert_eq!(parsed.pages.len(), 1);
    assert_eq!(parsed.pages[0].body_text, "ForwardFlateBody");
}

#[test]
fn test_fail_closed_filter_and_non_page_or_inline_flate_b64_rejection() {
    let page_raw = b"BT /F1 12 Tf 72 720 Td (CleanBody) Tj ET";

    let mut missing_filter_pdf = Vec::new();
    missing_filter_pdf.extend_from_slice(
        b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Count 1 /Kids [3 0 R] >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n",
    );
    missing_filter_pdf.extend_from_slice(
        format!("4 0 obj << /Length {} >>\nstream\n", page_raw.len()).as_bytes(),
    );
    missing_filter_pdf.extend_from_slice(page_raw);
    missing_filter_pdf.extend_from_slice(b"\nendstream\nendobj\n");
    missing_filter_pdf.extend_from_slice(
        b"7 0 obj << /Length 4 /Filter 99 0 R >>\nstream\nABCD\nendstream\nendobj\nxref\n0 8\ntrailer << /Size 8 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n",
    );
    assert!(matches!(
        PdfSerializer::parse(&missing_filter_pdf),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    let mut corrupt_flate_pdf = Vec::new();
    corrupt_flate_pdf.extend_from_slice(
        b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Count 1 /Kids [3 0 R] >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n",
    );
    corrupt_flate_pdf.extend_from_slice(
        format!("4 0 obj << /Length {} >>\nstream\n", page_raw.len()).as_bytes(),
    );
    corrupt_flate_pdf.extend_from_slice(page_raw);
    corrupt_flate_pdf.extend_from_slice(b"\nendstream\nendobj\n");
    corrupt_flate_pdf.extend_from_slice(
        b"7 0 obj << /Length 6 /Filter /FlateDecode >>\nstream\nBADZLB\nendstream\nendobj\nxref\n0 8\ntrailer << /Size 8 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n",
    );
    assert!(matches!(
        PdfSerializer::parse(&corrupt_flate_pdf),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    let mut large_spoof = vec![b'A'; 5 * 1024 * 1024];
    large_spoof.extend_from_slice(b"\n%EasyOfficePDFModelBase64:eyJ0aXRsZSI6IlNwb29mIn0=\n");
    let compressed_spoof = build_stored_zlib(&large_spoof);
    let mut forward_spoof_pdf = Vec::new();
    forward_spoof_pdf.extend_from_slice(
        b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Count 1 /Kids [3 0 R] >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n",
    );
    forward_spoof_pdf.extend_from_slice(
        format!("4 0 obj << /Length {} >>\nstream\n", page_raw.len()).as_bytes(),
    );
    forward_spoof_pdf.extend_from_slice(page_raw);
    forward_spoof_pdf.extend_from_slice(b"\nendstream\nendobj\n");
    forward_spoof_pdf.extend_from_slice(
        format!(
            "7 0 obj << /Length {} /Filter 9 0 R >>\nstream\n",
            compressed_spoof.len()
        )
        .as_bytes(),
    );
    forward_spoof_pdf.extend_from_slice(&compressed_spoof);
    forward_spoof_pdf.extend_from_slice(b"\nendstream\nendobj\n9 0 obj /FlateDecode endobj\nxref\n0 10\ntrailer << /Size 10 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n");
    assert!(matches!(
        PdfSerializer::parse(&forward_spoof_pdf),
        Err(EasyOfficeError::SecurityViolation(_))
    ));

    let inline_spoof = deflate_zlib(
        b"BT /F1 12 Tf (Inline) Tj ET\n%EasyOfficePDFModelBase64:eyJ0aXRsZSI6IlNwb29mIn0=\n",
    );
    let mut inline_page_pdf = Vec::new();
    inline_page_pdf.extend_from_slice(
        b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Count 1 /Kids [3 0 R] >> endobj\n",
    );
    inline_page_pdf.extend_from_slice(
        format!(
            "3 0 obj << /Type /Page /Parent 2 0 R /Length {} /Filter /FlateDecode >>\nstream\n",
            inline_spoof.len()
        )
        .as_bytes(),
    );
    inline_page_pdf.extend_from_slice(&inline_spoof);
    inline_page_pdf.extend_from_slice(
        b"\nendstream\nendobj\nxref\n0 4\ntrailer << /Size 4 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n",
    );
    assert!(matches!(
        PdfSerializer::parse(&inline_page_pdf),
        Err(EasyOfficeError::SecurityViolation(_))
    ));
}

#[test]
fn test_cumulative_32mib_budget_shared_contents_and_page_tree_depth_caps() {
    let mut chunk_12m = vec![b' '; 12 * 1024 * 1024 - 32];
    chunk_12m.extend_from_slice(b"BT /F1 12 Tf 72 720 Td (Ok) Tj ET");
    let flate_12m = build_stored_zlib(&chunk_12m);

    let mut two_12m_pdf = Vec::new();
    two_12m_pdf.extend_from_slice(
        b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Count 2 /Kids [3 0 R 5 0 R] >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n",
    );
    two_12m_pdf.extend_from_slice(
        format!(
            "4 0 obj << /Length {} /Filter /FlateDecode >>\nstream\n",
            flate_12m.len()
        )
        .as_bytes(),
    );
    two_12m_pdf.extend_from_slice(&flate_12m);
    two_12m_pdf.extend_from_slice(
        b"\nendstream\nendobj\n5 0 obj << /Type /Page /Parent 2 0 R /Contents 6 0 R >> endobj\n",
    );
    two_12m_pdf.extend_from_slice(
        format!(
            "6 0 obj << /Length {} /Filter /FlateDecode >>\nstream\n",
            flate_12m.len()
        )
        .as_bytes(),
    );
    two_12m_pdf.extend_from_slice(&flate_12m);
    two_12m_pdf.extend_from_slice(
        b"\nendstream\nendobj\nxref\n0 7\ntrailer << /Size 7 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n",
    );
    let ok_24m = PdfSerializer::parse(&two_12m_pdf).expect("24MiB cumulative Flate must succeed");
    assert_eq!(ok_24m.pages.len(), 2);

    let mut three_12m_pdf = Vec::new();
    three_12m_pdf.extend_from_slice(
        b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Count 1 /Kids [3 0 R] >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n",
    );
    for obj_id in [4usize, 5, 6] {
        three_12m_pdf.extend_from_slice(
            format!(
                "{obj_id} 0 obj << /Length {} /Filter /FlateDecode >>\nstream\n",
                flate_12m.len()
            )
            .as_bytes(),
        );
        three_12m_pdf.extend_from_slice(&flate_12m);
        three_12m_pdf.extend_from_slice(b"\nendstream\nendobj\n");
    }
    three_12m_pdf
        .extend_from_slice(b"xref\n0 7\ntrailer << /Size 7 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n");
    assert!(matches!(
        PdfSerializer::parse(&three_12m_pdf),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    let mut shared_200k = vec![b' '; 200 * 1024 - 34];
    shared_200k.extend_from_slice(b"BT /F1 12 Tf 72 720 Td (Amp) Tj ET");
    let repeated_200 = std::iter::repeat_n("4 0 R", 200)
        .collect::<Vec<_>>()
        .join(" ");
    let mut shared_amp_pdf = Vec::new();
    shared_amp_pdf.extend_from_slice(
        format!(
            "%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Count 1 /Kids [3 0 R] >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R /Contents [{repeated_200}] >> endobj\n4 0 obj << /Length {} >>\nstream\n",
            shared_200k.len()
        )
        .as_bytes(),
    );
    shared_amp_pdf.extend_from_slice(&shared_200k);
    shared_amp_pdf.extend_from_slice(
        b"\nendstream\nendobj\nxref\n0 5\ntrailer << /Size 5 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n",
    );
    assert!(matches!(
        PdfSerializer::parse(&shared_amp_pdf),
        Err(EasyOfficeError::SecurityViolation(_))
    ));

    let repeated_257 = std::iter::repeat_n("4 0 R", 257)
        .collect::<Vec<_>>()
        .join(" ");
    let over_contents_cap = format!(
        "%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Count 1 /Kids [3 0 R] >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R /Contents [{repeated_257}] >> endobj\n4 0 obj << /Length 31 >>\nstream\nBT /F1 12 Tf 72 720 Td (X) Tj ET\nendstream\nendobj\nxref\n0 5\ntrailer << /Size 5 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n"
    );
    assert!(matches!(
        PdfSerializer::parse(over_contents_cap.as_bytes()),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    let build_deep_pages_pdf = |chain_len: usize| -> Vec<u8> {
        let mut s = String::from("%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n");
        for i in 0..chain_len {
            let cur = 2 + i;
            let next = cur + 1;
            s.push_str(&format!(
                "{cur} 0 obj << /Type /Pages /Count 1 /Kids [{next} 0 R] >> endobj\n"
            ));
        }
        let leaf_page = 2 + chain_len;
        let content_id = leaf_page + 1;
        let body = "BT /F1 12 Tf 72 720 Td (DeepLeaf) Tj ET";
        s.push_str(&format!(
            "{leaf_page} 0 obj << /Type /Page /Contents {content_id} 0 R >> endobj\n"
        ));
        s.push_str(&format!(
            "{content_id} 0 obj << /Length {} >>\nstream\n{body}\nendstream\nendobj\n",
            body.len()
        ));
        let total = content_id + 1;
        s.push_str(&format!(
            "xref\n0 {total}\ntrailer << /Size {total} /Root 1 0 R >>\nstartxref\n0\n%%EOF\n"
        ));
        s.into_bytes()
    };

    let ok_depth_64 = PdfSerializer::parse(&build_deep_pages_pdf(64)).unwrap();
    assert_eq!(ok_depth_64.pages.len(), 1);
    assert_eq!(ok_depth_64.pages[0].body_text, "DeepLeaf");

    assert!(matches!(
        PdfSerializer::parse(&build_deep_pages_pdf(65)),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));
}

#[test]
fn test_classic_trailer_cipher_xref_encrypt_and_linear_unclosed_paren_bounds() {
    let classic_cipher_pdf = b"%PDF-1.4\n\
1 0 obj << /Type /Catalog /Title (UnclosedCipher >> endobj\n\
2 0 obj << /Type /Page >> endobj\n\
xref\n0 3\n\
trailer << /Size 3 /Root 1 0 R /Encrypt << /Filter /Standard /O (ab)cd) >> >>\n\
startxref\n0\n%%EOF\n";
    assert!(matches!(
        PdfSerializer::parse(classic_cipher_pdf),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let xref_stream_cipher_pdf = b"%PDF-1.5\n\
1 0 obj << /Type /Catalog /Title (Cipher >> endobj\n\
4 0 obj << /Type /XRef /Size 5 /Root 1 0 R /Encrypt 9 0 R /ID [(ab)cd) (ef)] /Length 4 >>\n\
stream\nABCD\nendstream\nendobj\n\
startxref\n0\n%%EOF\n";
    assert!(matches!(
        PdfSerializer::parse(xref_stream_cipher_pdf),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let long_closed_note = format!(
        "{} Note: endobj 2 0 obj << >> stream x endstream endobj trailer << /Encrypt 9 0 R >>",
        "A".repeat(40_000)
    );
    let truncated_unencrypted = format!(
        "%PDF-1.7\n\
1 0 obj << /Type /Catalog /Pages 2 0 R /Title ({long_closed_note}) >> endobj\n\
2 0 obj << /Type /Pages /Count 2 /Kids [3 0 R 5 0 R] >> endobj\n\
3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n\
4 0 obj << /Length 31 >>\nstream\nBT /F1 12 Tf 72 720 Td (P1) Tj ET\nendstream\nendobj\n\
5 0 obj << /Type /Page /Broken [\nxref\n0 6\n%%EOF\n"
    );
    assert!(matches!(
        PdfSerializer::parse(truncated_unencrypted.as_bytes()),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    let stream1 = b"BT /F1 12 Tf 72 720 Td (endstream fake) Tj ET";
    let stream2 = b"BT /F1 12 Tf 72 720 Td (\n5 0 obj 999 endobj\n) Tj ET";
    let forward_len_pdf = format!(
        "%PDF-1.7\n\
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
2 0 obj << /Type /Pages /Count 2 /Kids [3 0 R 6 0 R] >> endobj\n\
3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n\
4 0 obj << /Length 5 0 R >>\nstream\n{}\nendstream\nendobj\n\
5 0 obj {} endobj\n\
6 0 obj << /Type /Page /Parent 2 0 R /Contents 7 0 R >> endobj\n\
7 0 obj << /Length {} >>\nstream\n{}\nendstream\nendobj\n\
xref\n0 8\ntrailer << /Size 8 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n",
        std::str::from_utf8(stream1).unwrap(),
        stream1.len(),
        stream2.len(),
        std::str::from_utf8(stream2).unwrap(),
    );
    let parsed_fwd = PdfSerializer::parse(forward_len_pdf.as_bytes()).unwrap();
    assert_eq!(parsed_fwd.pages.len(), 2);
    assert_eq!(parsed_fwd.pages[0].body_text, "endstream fake");

    let mut unclosed_flood = Vec::from(&b"%PDF-1.7\n1 0 obj "[..]);
    unclosed_flood.extend(std::iter::repeat_n(b'(', 262_144));
    unclosed_flood.extend_from_slice(b"\nxref\n%%EOF\n");
    let start = std::time::Instant::now();
    assert!(matches!(
        PdfSerializer::parse(&unclosed_flood),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));
    assert!(start.elapsed().as_secs_f64() < 1.0);

    let mut stream_paren_flood = Vec::from(&b"%PDF-1.7\n1 0 obj << >>\nstream\n"[..]);
    stream_paren_flood.extend(std::iter::repeat_n(b'(', 262_144));
    stream_paren_flood.extend_from_slice(b"\nendstream\nendobj\nxref\n%%EOF\n");
    let stream_start_t = std::time::Instant::now();
    assert!(matches!(
        PdfSerializer::parse(&stream_paren_flood),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));
    assert!(stream_start_t.elapsed().as_secs_f64() < 1.0);

    let mut page_stream_flood = Vec::from(
        &b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Count 1 /Kids [3 0 R] >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n4 0 obj << /Length 262144 >>\nstream\n"[..],
    );
    page_stream_flood.extend(std::iter::repeat_n(b'(', 262_144));
    page_stream_flood.extend_from_slice(
        b"\nendstream\nendobj\nxref\n0 5\ntrailer << /Size 5 /Root 1 0 R >>\nstartxref\n0\n%%EOF\n",
    );
    let page_stream_t = std::time::Instant::now();
    assert!(matches!(
        PdfSerializer::parse(&page_stream_flood),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));
    assert!(page_stream_t.elapsed().as_secs_f64() < 1.0);

    let id_before_encrypt_trailer = b"%PDF-1.4\n\
1 0 obj << /Type /Catalog >> endobj\n\
xref\n0 2\n\
trailer << /Size 2 /Root 1 0 R /ID [(ab(cd) (ef)] /Encrypt << /Filter /Standard /O (xy)zw) >> >>\n\
startxref\n0\n%%EOF\n";
    assert!(matches!(
        PdfSerializer::parse(id_before_encrypt_trailer),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let xref_id_before_encrypt_stream = b"%PDF-1.5\n\
4 0 obj << /ID [(a(b) (cd)] /Type /XRef /Size 5 /Root 1 0 R /Encrypt 9 0 R /Length 4 >>\n\
stream\nABCD)\nendstream\nendobj\n\
startxref\n0\n%%EOF\n";
    assert!(matches!(
        PdfSerializer::parse(xref_id_before_encrypt_stream),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let incremental_rev2_encrypt = b"%PDF-1.4\n\
1 0 obj << /Type /Catalog /Broken [\n\
trailer << /Size 2 /Root 1 0 R /Info (unclosed >>\n\
startxref\n0\n%%EOF\n\
xref\n0 2\n\
trailer << /Size 2 /Root 1 0 R /Encrypt << /Filter /Standard /O (ab)cd) >> >>\n\
startxref\n100\n%%EOF\n";
    assert!(matches!(
        PdfSerializer::parse(incremental_rev2_encrypt),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let broken_obj_before_xref_encrypt = b"%PDF-1.5\n\
1 0 obj << /Type /Catalog /Broken [\n\
4 0 obj << /Type /XRef /Size 5 /Root 1 0 R /Encrypt 9 0 R /Length 4 >>\n\
stream\nABCD\nendstream\nendobj\n\
startxref\n0\n%%EOF\n";
    assert!(matches!(
        PdfSerializer::parse(broken_obj_before_xref_encrypt),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let post_truncation_closed_note = format!(
        "%PDF-1.7\n\
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
2 0 obj << /Type /Pages /Count 1 /Kids [5 0 R] >> endobj\n\
5 0 obj << /Type /Page /Broken [\n\
6 0 obj << /Type /Annot /Title ({long_closed_note}) >> endobj\n\
xref\n0 7\n%%EOF\n"
    );
    assert!(matches!(
        PdfSerializer::parse(post_truncation_closed_note.as_bytes()),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    let mut comment_digit_flood = Vec::from(&b"%PDF-1.7\n1 0 obj << /Broken [\n"[..]);
    for _ in 0..20_000 {
        comment_digit_flood.extend_from_slice(b"% 1 0 obj\n");
    }
    comment_digit_flood.extend_from_slice(b"xref\n%%EOF\n");
    let comment_flood_t = std::time::Instant::now();
    assert!(matches!(
        PdfSerializer::parse(&comment_digit_flood),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));
    assert!(comment_flood_t.elapsed().as_secs_f64() < 1.0);

    let preceding_length_damaged_pdf = b"%PDF-1.5\n\
1 0 obj 24 endobj\n\
2 0 obj << /Type /Catalog /Broken [\n\
3 0 obj << /Length 1 0 R >>\n\
stream\n\
4 0 obj << /Type /XRef\n\n\
endstream\nendobj\n\
5 0 obj << /Type /XRef /Size 6 /Root 2 0 R /Encrypt 9 0 R /Length 4 >>\n\
stream\nABCD\nendstream\nendobj\n\
startxref\n0\n%%EOF\n";
    assert!(matches!(
        PdfSerializer::parse(preceding_length_damaged_pdf),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let cross_obj_paren_encrypt = b"%PDF-1.5\n\
1 0 obj << /Title (unclosed >> endobj\n\
4 0 obj << /Type /XRef /Size 5 /Root 1 0 R /Encrypt 9 0 R /Desc (ab)cd) >>\n\
stream\nABCD\nendstream\nendobj\n\
startxref\n0\n%%EOF\n";
    assert!(matches!(
        PdfSerializer::parse(cross_obj_paren_encrypt),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let mut deep_unclosed_before_trailer = Vec::from(&b"%PDF-1.4\n1 0 obj "[..]);
    deep_unclosed_before_trailer.extend(std::iter::repeat_n(b'(', 5_000));
    deep_unclosed_before_trailer.extend_from_slice(
        b"\ntrailer << /Size 2 /Root 1 0 R /Info (v1>>v2) /Encrypt 9 0 R >>\nstartxref\n0\n%%EOF\n",
    );
    assert!(matches!(
        PdfSerializer::parse(&deep_unclosed_before_trailer),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let binary_id_before_encrypt = b"%PDF-1.4\n\
1 0 obj << /Type /Catalog >> endobj\n\
xref\n0 2\n\
trailer << /Size 2 /Root 1 0 R /ID [(a%b[c>>d(e) (f)] /Encrypt 9 0 R >>\n\
startxref\n0\n%%EOF\n";
    assert!(matches!(
        PdfSerializer::parse(binary_id_before_encrypt),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let broken_array_body_before_xref = b"%PDF-1.5\n\
1 0 obj [ 10 20\n\
4 0 obj << /Type /XRef /Size 5 /Root 1 0 R /Encrypt 9 0 R /Length 4 >>\n\
stream\nABCD\nendstream\nendobj\n\
startxref\n0\n%%EOF\n";
    assert!(matches!(
        PdfSerializer::parse(broken_array_body_before_xref),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let tagged_struct_elem_pdf = b"%PDF-1.7\n\
1 0 obj << /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R >> endobj\n\
2 0 obj << /Type /Pages /Count 1 /Kids [3 0 R] >> endobj\n\
3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n\
4 0 obj << /Length 36 >>\nstream\nBT /F1 12 Tf 72 720 Td (Tagged) Tj ET\nendstream\nendobj\n\
5 0 obj << /Type /StructTreeRoot /K 6 0 R >> endobj\n\
6 0 obj << /Type /StructElem /S /P /P 5 0 R /ID (sec-node-1) >> endobj\n\
7 0 obj [(abc1234567890123) (def1234567890123)] endobj\n\
xref\n0 8\ntrailer << /Size 8 /Root 1 0 R /ID 7 0 R >>\nstartxref\n0\n%%EOF\n";
    let tagged_doc = PdfSerializer::parse(tagged_struct_elem_pdf).unwrap();
    assert_eq!(tagged_doc.pages.len(), 1);
    assert_eq!(tagged_doc.pages[0].body_text, "Tagged");

    let cross_obj_encrypt_after_paren = b"%PDF-1.5\n\
1 0 obj << /Title (unclosed >> endobj\n\
4 0 obj << /Type /XRef /Size 5 /Root 1 0 R /Desc (ab)cd) /Encrypt 9 0 R /Length 4 >>\n\
stream\nABCD\nendstream\nendobj\n\
startxref\n0\n%%EOF\n";
    assert!(matches!(
        PdfSerializer::parse(cross_obj_encrypt_after_paren),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let intra_trailer_spliced_encrypt = b"%PDF-1.4\n\
1 0 obj << /Type /Catalog >> endobj\n\
xref\n0 2\n\
trailer << /Size 2 /Root 1 0 R /Custom (unclosed /Encrypt 9 0 R /Desc (ab)cd) >>\n\
startxref\n0\n%%EOF\n";
    assert!(matches!(
        PdfSerializer::parse(intra_trailer_spliced_encrypt),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let valid_escaped_id_before_encrypt = b"%PDF-1.4\n\
1 0 obj << /Type /Catalog >> endobj\n\
xref\n0 2\n\
trailer << /Size 2 /Root 1 0 R /ID [(0123\\(45]>>abcdef) (0123456789abcdef)] %c\n/#45ncrypt 9 0 R >>\n\
startxref\n0\n%%EOF\n";
    assert!(matches!(
        PdfSerializer::parse(valid_escaped_id_before_encrypt),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let broken_id_inner_dict_end_and_comment = b"%PDF-1.4\n\
1 0 obj << /Type /Catalog >> endobj\n\
xref\n0 2\n\
trailer << /Size 2 /Root 1 0 R /ID [(a%b]>>d(e) (f)] %id\n/#45ncrypt 9 0 R >>\n\
startxref\n0\n%%EOF\n";
    assert!(matches!(
        PdfSerializer::parse(broken_id_inner_dict_end_and_comment),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let array_closed_trailer_note_truncated = b"%PDF-1.7\n\
1 0 obj [ (Note: trailer << /Encrypt 9 0 R >>) /Broken\n\
xref\n0 2\n%%EOF\n";
    assert!(matches!(
        PdfSerializer::parse(array_closed_trailer_note_truncated),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    let mut comment_endobj_flood = Vec::from(&b"%PDF-1.7\n1 0 obj (\n"[..]);
    for _ in 0..20_000 {
        comment_endobj_flood.extend_from_slice(b"% endobj\n");
    }
    comment_endobj_flood.extend_from_slice(b"broken\nxref\n%%EOF\n");
    let endobj_flood_t = std::time::Instant::now();
    assert!(matches!(
        PdfSerializer::parse(&comment_endobj_flood),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));
    assert!(endobj_flood_t.elapsed().as_secs_f64() < 1.0);

    let truncated_hdr_and_spoof_len = b"%PDF-1.5\n\
99 0 obj\n\
1 0 obj 24 endobj\n\
2 0 obj << /Broken [ /Length 1 0 R >>\n\
stream\n\
1 0 obj 999 endobj\n\
pad!\n\
endstream\nendobj\n\
5 0 obj << /Type /XRef /Size 6 /Root 2 0 R /Encrypt 9 0 R /Length 4 >>\n\
stream\nABCD\nendstream\nendobj\n\
startxref\n0\n%%EOF\n";
    assert!(matches!(
        PdfSerializer::parse(truncated_hdr_and_spoof_len),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let multiline_xref_prose_pdf = b"%PDF-1.7\n\
1 0 obj << /Type /Catalog /Pages 2 0 R /Title (Chapter 1:\nxref overview) >> endobj\n\
2 0 obj << /Type /Pages /Count 1 /Kids [3 0 R] >> endobj\n\
3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n\
4 0 obj << /Length 36 >>\nstream\nBT /F1 12 Tf 72 720 Td (Normal) Tj ET\nendstream\nendobj\n\
xref\n0 5\ntrailer << /Size 5 /Root 1 0 R /ID null >>\nstartxref\n0\n%%EOF\n";
    let multiline_doc = PdfSerializer::parse(multiline_xref_prose_pdf).unwrap();
    assert_eq!(multiline_doc.title, "Chapter 1:\nxref overview");
    assert_eq!(multiline_doc.pages[0].body_text, "Normal");

    let binary_hash45_and_prose_encrypt_pdf = b"%PDF-1.7\n\
1 0 obj << /Type /Catalog /Pages 2 0 R /Title (Note: /Encrypt 9 0 R prose) >> endobj\n\
2 0 obj << /Type /Pages /Count 1 /Kids [3 0 R] >> endobj\n\
3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n\
4 0 obj << /Length 35 >>\nstream\nBT /F1 12 Tf 72 720 Td (Clean) Tj ET\nendstream\nendobj\n\
xref\n0 5\ntrailer << /Size 5 /Root 1 0 R /ID [(0123456789ab/#45) (0123456789abcdef)] >>\nstartxref\n0\n%%EOF\n";
    let clean_doc = PdfSerializer::parse(binary_hash45_and_prose_encrypt_pdf).unwrap();
    assert_eq!(clean_doc.pages[0].body_text, "Clean");

    let rev2_stream = b"BT /F1 12 Tf 72 720 Td (Rev2Body) Tj ET";
    let incremental_stale_len_pdf = format!(
        "%PDF-1.7\n\
1 0 obj 999 endobj\n\
2 0 obj << /Type /Catalog /Pages 3 0 R >> endobj\n\
3 0 obj << /Type /Pages /Count 1 /Kids [4 0 R] >> endobj\n\
4 0 obj << /Type /Page /Parent 3 0 R /Contents 5 0 R >> endobj\n\
5 0 obj << /Length 1 0 R >>\nstream\n{}\nendstream\nendobj\n\
1 0 obj {} endobj\n\
xref\n0 6\ntrailer << /Size 6 /Root 2 0 R >>\nstartxref\n0\n%%EOF\n",
        std::str::from_utf8(rev2_stream).unwrap(),
        rev2_stream.len(),
    );
    let inc_doc = PdfSerializer::parse(incremental_stale_len_pdf.as_bytes()).unwrap();
    assert_eq!(inc_doc.pages[0].body_text, "Rev2Body");

    let obj_hdr_comment_xref_encrypt = b"%PDF-1.5\n\
1 0 obj << /Title (unclosed\n\
4 0 obj %header comment\n\
<< /Type /XRef /Size 5 /Root 1 0 R /Desc (ab)cd) /Encrypt 9 0 R /Length 4 >>\n\
stream\nABCD\nendstream\nendobj\n\
startxref\n0\n%%EOF\n";
    assert!(matches!(
        PdfSerializer::parse(obj_hdr_comment_xref_encrypt),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let spliced_before_type_xref_encrypt = b"%PDF-1.5\n\
4 0 obj << /Custom (unclosed /E#6ecrypt 9 0 R /Desc (ab)) /Type /XRef /Size 5 /Root 1 0 R /Length 4 >>\n\
stream\nABCD\nendstream\nendobj\n\
startxref\n0\n%%EOF\n";
    assert!(matches!(
        PdfSerializer::parse(spliced_before_type_xref_encrypt),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let broken_dict_closed_trailer_note = b"%PDF-1.7\n\
1 0 obj << /Broken [ (Note: trailer << /Encrypt 9 0 R >>) 123\n\
xref\n0 2\n%%EOF\n";
    assert!(matches!(
        PdfSerializer::parse(broken_dict_closed_trailer_note),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    let nested_array_closed_trailer_note = b"%PDF-1.7\n\
1 0 obj [ [ (Note: trailer << /Encrypt 9 0 R >>) /Broken\n\
xref\n0 2\n%%EOF\n";
    assert!(matches!(
        PdfSerializer::parse(nested_array_closed_trailer_note),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    let broken_trailer_closed_obj_note_encrypt = b"%PDF-1.4\n\
1 0 obj << /Type /Catalog >> endobj\n\
xref\n0 2\n\
trailer << /Size 2 /Root 1 0 R /Arr [ (See 2 0 obj) 1 ] /Broken (x)y) /E#6ecrypt 9 0 R >>\n\
startxref\n0\n%%EOF\n";
    assert!(matches!(
        PdfSerializer::parse(broken_trailer_closed_obj_note_encrypt),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Pdf
        ))
    ));

    let mut stream_endobj_comment_flood = Vec::from(&b"%PDF-1.7\n1 0 obj << >>\nstream\n"[..]);
    for _ in 0..20_000 {
        stream_endobj_comment_flood.extend_from_slice(b"% endobj\n");
    }
    stream_endobj_comment_flood.extend_from_slice(b"payload\nendstream\nendobj\nxref\n%%EOF\n");
    let stream_flood_t = std::time::Instant::now();
    assert!(matches!(
        PdfSerializer::parse(&stream_endobj_comment_flood),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));
    assert!(stream_flood_t.elapsed().as_secs_f64() < 1.0);
}
