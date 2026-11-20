use super::*;

#[test]
fn test_ieee_crc32_standard_vector_and_zip_roundtrip() {
    // IEEE 802.3 standard test vector: "123456789" -> 0xCBF43926
    assert_eq!(IeeeCrc32::checksum(b"123456789"), 0xCBF4_3926);

    let entries = vec![
        (
            "[Content_Types].xml".to_string(),
            b"<Types/>".to_vec(),
        ),
        (
            "word/document.xml".to_string(),
            "<w:document><w:body><w:p><w:r><w:t>한글 및 English 본문</w:t></w:p></w:body></w:document>"
                .as_bytes()
                .to_vec(),
        ),
    ];

    let archive = ZipArchiveCodec::pack(&entries).unwrap();
    let policy = ZipSecurityPolicy::default();
    let unpacked = ZipArchiveCodec::unpack(&archive, &policy).unwrap();
    assert_eq!(unpacked.len(), 2);
    assert_eq!(unpacked[0].path, "[Content_Types].xml");
    assert_eq!(unpacked[0].data, b"<Types/>");
    assert_eq!(unpacked[1].path, "word/document.xml");

    // Tamper with a payload byte to verify CRC-32 Fail-Closed rejection
    let mut corrupted = archive.clone();
    let payload_idx = 30 + "[Content_Types].xml".len();
    corrupted[payload_idx] ^= 0xFF;
    assert!(matches!(
        ZipArchiveCodec::unpack(&corrupted, &policy),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    // Zip-Slip path rejection on pack
    let bad_entries = vec![("../evil.xml".to_string(), b"bad".to_vec())];
    assert!(matches!(
        ZipArchiveCodec::pack(&bad_entries),
        Err(EasyOfficeError::SecurityViolation(_))
    ));
}

#[test]
fn verify_runs_the_checks_of_unpack_and_returns_only_the_paths() {
    let entries = vec![
        ("Contents/header.xml".to_string(), b"<head/>".to_vec()),
        (
            "BinData/image1.bmp".to_string(),
            (0..=250).cycle().take(4096).collect(),
        ),
    ];
    let archive = ZipArchiveCodec::pack_deflate(&entries).unwrap();
    let policy = ZipSecurityPolicy::default();
    assert_eq!(
        ZipArchiveCodec::verify(&archive, &policy).unwrap(),
        ["Contents/header.xml", "BinData/image1.bmp"]
    );
    // A flipped payload byte (CRC-32) and a truncated archive are refused as by `unpack`.
    let mut corrupted = archive.clone();
    corrupted[30 + "Contents/header.xml".len()] ^= 0xFF;
    assert!(ZipArchiveCodec::verify(&corrupted, &policy).is_err());
    assert!(ZipArchiveCodec::verify(&archive[..archive.len() - 10], &policy).is_err());
}

#[test]
fn a_binary_budget_counts_non_xml_parts_apart_from_xml_parts() {
    let entries = vec![
        ("Contents/a.xml".to_string(), vec![b'a'; 3000]),
        ("Contents/b.XML".to_string(), vec![b'b'; 3000]),
        ("Contents/content.hpf".to_string(), vec![b'c'; 3000]),
        ("BinData/image1.bmp".to_string(), vec![1_u8; 20_000]),
        ("BinData/image2.bmp".to_string(), vec![2_u8; 20_000]),
    ];
    let archive = ZipArchiveCodec::pack_deflate(&entries).unwrap();
    let shared = ZipSecurityPolicy {
        max_single_entry_uncompressed_bytes: 20_000,
        max_total_uncompressed_bytes: 10_000,
        max_compression_ratio: f64::INFINITY,
        ..ZipSecurityPolicy::default()
    };
    // One budget for everything: 49 000 bytes against 10 000.
    assert!(ZipArchiveCodec::verify(&archive, &shared).is_err());
    let split = ZipSecurityPolicy {
        binary_budget: Some(crate::ZipBinaryBudget {
            max_single_entry_uncompressed_bytes: 20_000,
            max_total_uncompressed_bytes: 40_000,
        }),
        ..shared.clone()
    };
    // 9 000 bytes of text fit 10 000 and 40 000 bytes of pictures fit theirs.
    assert_eq!(ZipArchiveCodec::verify(&archive, &split).unwrap().len(), 5);
    // One byte less for pictures, or less for text, is refused by size alone, whichever
    // order the parts come in.
    let few_pictures = ZipSecurityPolicy {
        binary_budget: Some(crate::ZipBinaryBudget {
            max_single_entry_uncompressed_bytes: 20_000,
            max_total_uncompressed_bytes: 39_999,
        }),
        ..shared.clone()
    };
    assert!(ZipArchiveCodec::verify(&archive, &few_pictures).is_err());
    let little_text = ZipSecurityPolicy {
        max_total_uncompressed_bytes: 8_999,
        ..split
    };
    assert!(ZipArchiveCodec::verify(&archive, &little_text).is_err());
}

#[test]
fn test_twenty_formats_and_magic_signature_detection() {
    use crate::{DocumentFormat, FormatDetector};
    assert_eq!(DocumentFormat::ALL.len(), 20);
    let det = |b: &[u8], h: Option<&str>| FormatDetector::detect(b, h).unwrap();
    let zip_sample = b"PK\x03\x04\x14\x00\x00\x00word/document.xml";
    assert_eq!(det(zip_sample, None), DocumentFormat::Docx);
    for &(sample, expected) in &[
        (&b"PK\x03\x04Contents/header.xml"[..], DocumentFormat::Hwpx),
        (b"PK\x03\x04oasis.opendocument.text", DocumentFormat::Odt),
        (
            b"PK\x03\x04oasis.opendocument.spreadsheet",
            DocumentFormat::Ods,
        ),
        (
            b"PK\x03\x04oasis.opendocument.presentation",
            DocumentFormat::Odp,
        ),
        (b"PK\x03\x04xl/workbook.xml", DocumentFormat::Xlsx),
        (b"PK\x03\x04ppt/presentation.xml", DocumentFormat::Pptx),
    ] {
        assert_eq!(det(sample, None), expected);
    }
    assert_eq!(det(zip_sample, Some("report.docx")), DocumentFormat::Docx);
    assert_eq!(det(zip_sample, Some("template.dotx")), DocumentFormat::Docx);
    assert_eq!(det(zip_sample, Some("official.hwpx")), DocumentFormat::Hwpx);
    assert_eq!(det(zip_sample, Some("template.hwtx")), DocumentFormat::Hwtx);
    assert_eq!(det(zip_sample, Some("show.ppsx")), DocumentFormat::Ppsx);

    let ole2_sample = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1HWP Document File";
    assert_eq!(det(ole2_sample, Some("proposal.hwp")), DocumentFormat::Hwp);
    assert_eq!(det(ole2_sample, Some("template.hwt")), DocumentFormat::Hwt);
    let pdf_sample = b"%PDF-1.4\n1 0 obj\n<<>>\nendobj\n%%EOF";
    assert_eq!(det(pdf_sample, Some("contract.pdf")), DocumentFormat::Pdf);
    assert_eq!(
        det(b"{\\rtf1\\ansi\\deff0 Hello}", Some("memo.rtf")),
        DocumentFormat::Rtf
    );
    assert_eq!(
        FormatDetector::detect(b"", Some("empty.docx")),
        Err(EasyOfficeError::EmptyPayload)
    );
    assert!(matches!(
        FormatDetector::detect(b"NOT_A_ZIP_FILE", Some("spoofed.xlsx")),
        Err(EasyOfficeError::SignatureMismatch { .. })
    ));
    assert!(matches!(
        FormatDetector::detect(zip_sample, Some("spoofed.csv")),
        Err(EasyOfficeError::SignatureMismatch { .. })
    ));
}

#[test]
fn test_zip_security_guard_blocks_traversal_and_decompression_bomb() {
    let policy = ZipSecurityPolicy::default();
    let chk = |p: &str, c: u64, u: u64| {
        ZipSecurityGuard::validate_archive_entries(
            &[ZipEntryDescriptor::new(p, c, u, false)],
            &policy,
        )
    };
    assert!(chk("[Content_Types].xml", 512, 2048).is_ok());
    assert!(chk("word/document.xml", 1024, 4096).is_ok());
    assert!(matches!(
        chk("../../etc/passwd", 100, 200),
        Err(EasyOfficeError::SecurityViolation(_))
    ));
    assert!(matches!(
        chk("C:\\Windows\\win.ini", 100, 200),
        Err(EasyOfficeError::SecurityViolation(_))
    ));
    assert!(matches!(
        chk("word/document.xml", 100, 20_000),
        Err(EasyOfficeError::DecompressionBombDetected { .. })
    ));
}

#[test]
fn test_ole2_sector_cycle_and_hwp5_encryption_guard() {
    use crate::{DocumentFormat, FormatDetector, Ole2SecurityGuard};
    let mut container = vec![0u8; 512 * 4];
    container[..8].copy_from_slice(&FormatDetector::OLE2_MAGIC);
    container[44..48].copy_from_slice(&1u32.to_le_bytes());

    let cyclic_fat = vec![1u32, 0u32, Ole2SecurityGuard::END_OF_CHAIN];
    assert!(matches!(
        Ole2SecurityGuard::validate_sector_chain(&container, &cyclic_fat, 0),
        Err(EasyOfficeError::SecurityViolation(_))
    ));

    let valid_fat = vec![1u32, 2u32, Ole2SecurityGuard::END_OF_CHAIN];
    assert_eq!(
        Ole2SecurityGuard::validate_sector_chain(&container, &valid_fat, 0).unwrap(),
        vec![0, 1, 2]
    );

    let mut hwp_header = vec![0u8; 256];
    hwp_header[..17].copy_from_slice(b"HWP Document File");
    hwp_header[32..36].copy_from_slice(&0x0500_0300u32.to_le_bytes());
    hwp_header[36..40].copy_from_slice(&0x02u32.to_le_bytes());
    assert_eq!(
        Ole2SecurityGuard::validate_hwp5_file_header(&hwp_header),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Hwp
        ))
    );
}

#[test]
fn test_xml_sanitizer_and_csv_commonmark_pdf_roundtrip() {
    use crate::{
        CommonMarkCodec, CsvCodec, PdfDocumentModel, PdfPageModel, PdfSerializer, XmlSanitizer,
    };
    assert!(XmlSanitizer::validate_safe_xml(
        "<w:document><w:body><w:p><w:t>안전한 문서</w:t></w:p></w:body></w:document>"
    )
    .is_ok());
    assert_eq!(
        XmlSanitizer::validate_safe_xml("<?xml version=\"1.0\"?><!DOCTYPE foo [<!ENTITY xxe SYSTEM \"file:///etc/passwd\">]><foo>&xxe;</foo>"),
        Err(EasyOfficeError::XxeOrExternalEntityDetected)
    );
    let escaped = XmlSanitizer::escape("A & B < C > \"D\" 'E'");
    assert_eq!(XmlSanitizer::unescape(&escaped), "A & B < C > \"D\" 'E'");
    let rows = vec![
        vec!["부서".to_string(), "메모, 특이사항".to_string()],
        vec!["연구개발".to_string(), "1행\n2행 \"강조\"".to_string()],
    ];
    assert_eq!(
        CsvCodec::parse(&CsvCodec::serialize(&rows, ','), ',').unwrap(),
        rows
    );
    let md_input =
        "# 제품 아키텍처\n\n본문 첫 문단입니다.\n\n> 핵심 설계 원칙\n\n- 항목 1\n- 항목 2";
    assert_eq!(
        CommonMarkCodec::render(&CommonMarkCodec::parse(md_input).unwrap()),
        md_input
    );
    let pdf_doc = PdfDocumentModel {
        title: "분기 보안 감사 보고서".to_string(),
        pages: vec![PdfPageModel {
            page_number: 1,
            rotation_degrees: 0,
            body_text: "ISO 32000-1 PDF 본문 텍스트\n2행 개행 보존".to_string(),
            approval_stamp: Some("APPROVED".to_string()),
        }],
        watermark_text: Some("CONFIDENTIAL".to_string()),
    };
    assert_eq!(
        PdfSerializer::parse(&PdfSerializer::serialize(&pdf_doc)).unwrap(),
        pdf_doc
    );
}

#[test]
fn test_sec_05_overlapping_zip_entries_rejected() {
    let entries = vec![
        ("file1.txt".to_string(), b"Hello World".to_vec()),
        ("file2.txt".to_string(), b"Foo Bar Baz".to_vec()),
    ];
    let valid_zip = ZipArchiveCodec::pack(&entries).unwrap();
    let eocd = valid_zip
        .windows(4)
        .rposition(|w| w == [0x50, 0x4b, 0x05, 0x06])
        .unwrap();
    let cd_offset =
        u32::from_le_bytes(valid_zip[eocd + 16..eocd + 20].try_into().unwrap()) as usize;
    let mut evil = valid_zip.clone();
    let cd2 = evil[cd_offset + 4..]
        .windows(4)
        .position(|w| w == [0x50, 0x4b, 0x01, 0x02])
        .unwrap()
        + cd_offset
        + 4;
    evil[cd2 + 42..cd2 + 46].copy_from_slice(&0u32.to_le_bytes());
    let nlen = u16::from_le_bytes(evil[cd_offset + 28..cd_offset + 30].try_into().unwrap());
    evil[cd2 + 28..cd2 + 30].copy_from_slice(&nlen.to_le_bytes());
    let name1 = &valid_zip[cd_offset + 46..cd_offset + 46 + nlen as usize];
    evil[cd2 + 46..cd2 + 46 + nlen as usize].copy_from_slice(name1);
    let res = ZipArchiveCodec::unpack(&evil, &crate::ZipSecurityPolicy::default());
    assert!(
        matches!(res, Err(EasyOfficeError::SecurityViolation(_))),
        "중첩 ZIP은 SecurityViolation이어야 함"
    );
}
