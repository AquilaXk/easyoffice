use easyoffice_formats::pages_package::{load_package, LEGACY_MESSAGE};
use easyoffice_formats::{
    DocumentFormat, EasyOfficeError, FormatDetector, HwpxCodec, LegacyDocPayload, LegacyOle2Codec,
    OdfCodec, Ole2ContainerCodec, UniversalFormatConverter, ZipArchiveCodec,
};

fn offcrypto_ole2_bytes() -> Vec<u8> {
    Ole2ContainerCodec::pack(&[
        ("EncryptionInfo".to_string(), vec![0, 4, 0, 4, 0, 0, 0, 0]),
        ("EncryptedPackage".to_string(), vec![0xDE, 0xAD, 0xBE, 0xEF]),
    ])
    .unwrap()
}

fn encrypted_odf_zip(mime: &str, body_bytes: Vec<u8>) -> Vec<u8> {
    let manifest = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\">",
            "<manifest:file-entry manifest:full-path=\"/\" manifest:media-type=\"{}\"/>",
            "<manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\">",
            "<manifest:encryption-data manifest:checksum-type=\"SHA256/1K\" manifest:checksum=\"AAAA\">",
            "<manifest:algorithm manifest:algorithm-name=\"http://www.w3.org/2001/04/xmlenc#aes256-cbc\" manifest:initialisation-vector=\"AAAA\"/>",
            "</manifest:encryption-data>",
            "</manifest:file-entry>",
            "</manifest:manifest>"
        ),
        mime
    );
    ZipArchiveCodec::pack(&[
        ("mimetype".to_string(), mime.as_bytes().to_vec()),
        ("META-INF/manifest.xml".to_string(), manifest.into_bytes()),
        ("content.xml".to_string(), body_bytes),
    ])
    .unwrap()
}

fn encrypted_hwpx_zip(is_template: bool) -> Vec<u8> {
    let manifest = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
        "<manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\">",
        "<manifest:file-entry manifest:full-path=\"/\" manifest:media-type=\"application/hwp+zip\"/>",
        "<manifest:file-entry manifest:full-path=\"Contents/section0.xml\" manifest:media-type=\"text/xml\">",
        "<manifest:encryption-data manifest:checksum-type=\"SHA256\" manifest:checksum=\"AAAA\"/>",
        "</manifest:file-entry>",
        "</manifest:manifest>"
    );
    let template_meta = if is_template {
        "<opf:meta name=\"isTemplate\" content=\"text\">1</opf:meta>"
    } else {
        ""
    };
    let hpf = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><opf:package xmlns:opf=\"http://www.idpf.org/2007/opf/\"><opf:metadata>{template_meta}</opf:metadata></opf:package>"
    );
    ZipArchiveCodec::pack(&[
        ("mimetype".to_string(), b"application/hwp+zip".to_vec()),
        (
            "META-INF/manifest.xml".to_string(),
            manifest.as_bytes().to_vec(),
        ),
        ("Contents/content.hpf".to_string(), hpf.into_bytes()),
        (
            "Contents/header.xml".to_string(),
            vec![0xFF, 0xFE, 0x00, 0x80],
        ),
        (
            "Contents/section0.xml".to_string(),
            vec![0xFF, 0x00, 0x81, 0x99],
        ),
    ])
    .unwrap()
}

fn bplist_root_dict(entries: &[(&str, bool)]) -> Vec<u8> {
    let count = 1 + entries.len() * 2;
    let mut body = Vec::new();
    let mut offsets = vec![8usize];
    body.push(0xD0 | (entries.len() as u8));
    for i in 0..entries.len() {
        body.push((1 + i) as u8);
    }
    for i in 0..entries.len() {
        body.push((1 + entries.len() + i) as u8);
    }
    for (k, _) in entries {
        offsets.push(8 + body.len());
        if k.len() < 15 {
            body.push(0x50 | (k.len() as u8));
        } else {
            body.push(0x5F);
            body.push(0x10);
            body.push(k.len() as u8);
        }
        body.extend_from_slice(k.as_bytes());
    }
    for (_, flag) in entries {
        offsets.push(8 + body.len());
        body.push(if *flag { 0x09 } else { 0x08 });
    }
    let table_at = 8 + body.len();
    let mut out = Vec::from(&b"bplist00"[..]);
    out.extend_from_slice(&body);
    for off in offsets {
        out.push(off as u8);
    }
    out.extend_from_slice(&[0; 6]);
    out.push(1);
    out.push(1);
    out.extend_from_slice(&(count as u64).to_be_bytes());
    out.extend_from_slice(&0u64.to_be_bytes());
    out.extend_from_slice(&(table_at as u64).to_be_bytes());
    out
}

#[test]
fn an_encrypted_ooxml_object_inside_a_doc_does_not_make_the_doc_encrypted() {
    // PD-363: only the root storage decides; a nested storage is an embedded object.
    let doc_with_object = Ole2ContainerCodec::pack(&[
        ("WordDocument".to_string(), vec![0; 64]),
        (
            "ObjectPool/_1234567/EncryptionInfo".to_string(),
            vec![0, 4, 0, 4, 0, 0, 0, 0],
        ),
        (
            "ObjectPool/_1234567/EncryptedPackage".to_string(),
            vec![0xDE, 0xAD, 0xBE, 0xEF],
        ),
    ])
    .unwrap();
    assert_eq!(
        FormatDetector::detect(&doc_with_object, None),
        Ok(DocumentFormat::Doc)
    );
    // the root storage still decides
    let mixed = Ole2ContainerCodec::pack(&[
        ("ObjectPool/_1/EncryptionInfo".to_string(), vec![0; 8]),
        ("EncryptionInfo".to_string(), vec![0, 4, 0, 4, 0, 0, 0, 0]),
        ("EncryptedPackage".to_string(), vec![1, 2, 3, 4]),
    ])
    .unwrap();
    assert!(matches!(
        FormatDetector::detect(&mixed, None),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(_))
    ));
}

#[test]
fn offcrypto_ooxml_containers_are_reported_as_encrypted_and_plain_doc_keeps_signature_mismatch() {
    let offcrypto = offcrypto_ole2_bytes();
    for (ext, expected) in [
        ("secret.docx", DocumentFormat::Docx),
        ("secret.xlsx", DocumentFormat::Xlsx),
        ("secret.pptx", DocumentFormat::Pptx),
        ("secret.ppsx", DocumentFormat::Ppsx),
    ] {
        assert_eq!(
            FormatDetector::detect(&offcrypto, Some(ext)),
            Err(EasyOfficeError::EncryptedDocumentUnsupported(expected)),
            "hint {ext}"
        );
    }
    assert_eq!(
        FormatDetector::detect(&offcrypto, None),
        Err(EasyOfficeError::EncryptedDocumentUnsupported(
            DocumentFormat::Docx
        ))
    );

    let plain_doc = LegacyOle2Codec::pack_doc(&LegacyDocPayload {
        title: "Normal".to_string(),
        paragraphs: vec!["hello".to_string()],
        tables: Vec::new(),
    })
    .unwrap();
    assert!(matches!(
        FormatDetector::detect(&plain_doc, Some("wrong.docx")),
        Err(EasyOfficeError::SignatureMismatch { .. })
    ));
    assert_eq!(
        FormatDetector::detect(&plain_doc, None).unwrap(),
        DocumentFormat::Doc
    );
}

#[test]
fn encrypted_odf_and_hwpx_manifests_fail_with_format_specific_encrypted_error() {
    let non_utf8_cipher = vec![0xFF, 0xFE, 0x80, 0x99, 0x00, 0xA1];

    let odt = encrypted_odf_zip(OdfCodec::MIMETYPE_ODT, non_utf8_cipher.clone());
    assert_eq!(
        UniversalFormatConverter::decode(&odt, Some("enc.odt")).unwrap_err(),
        EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Odt)
    );
    assert_eq!(
        UniversalFormatConverter::decode_rich(&odt, Some("enc.odt")).unwrap_err(),
        EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Odt)
    );

    let ods = encrypted_odf_zip(OdfCodec::MIMETYPE_ODS, non_utf8_cipher.clone());
    assert_eq!(
        UniversalFormatConverter::decode(&ods, Some("enc.ods")).unwrap_err(),
        EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Ods)
    );
    assert_eq!(
        OdfCodec::unpack_ods(&ods).unwrap_err(),
        EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Ods)
    );

    let odp = encrypted_odf_zip(OdfCodec::MIMETYPE_ODP, non_utf8_cipher);
    assert_eq!(
        UniversalFormatConverter::decode(&odp, Some("enc.odp")).unwrap_err(),
        EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Odp)
    );
    assert_eq!(
        OdfCodec::unpack_odp_rich(&odp).unwrap_err(),
        EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Odp)
    );

    let hwpx = encrypted_hwpx_zip(false);
    assert_eq!(
        HwpxCodec::unpack(&hwpx).unwrap_err(),
        EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Hwpx)
    );
    assert_eq!(
        UniversalFormatConverter::decode_rich(&hwpx, Some("enc.hwpx")).unwrap_err(),
        EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Hwpx)
    );

    let hwtx = encrypted_hwpx_zip(true);
    assert_eq!(
        HwpxCodec::unpack(&hwtx).unwrap_err(),
        EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Hwtx)
    );
    assert_eq!(
        UniversalFormatConverter::decode_rich(&hwtx, Some("enc.hwtx")).unwrap_err(),
        EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Hwtx)
    );
}

#[test]
fn pages_encrypted_variants_and_valid_non_encrypted_plist_work_accurately() {
    let iwph_only =
        ZipArchiveCodec::pack(&[("Index/PasswordHint.iwph".to_string(), vec![1])]).unwrap();
    assert_eq!(
        FormatDetector::detect(&iwph_only, None).unwrap(),
        DocumentFormat::Pages
    );
    assert_eq!(
        load_package(&iwph_only).unwrap_err(),
        EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Pages)
    );

    let iwpv2_zip = ZipArchiveCodec::pack(&[
        ("My.pages/Index.zip".to_string(), vec![0x50, 0x4B]),
        ("My.pages/Index/foo.iwpv2".to_string(), vec![9]),
    ])
    .unwrap();
    assert_eq!(
        FormatDetector::detect(&iwpv2_zip, None).unwrap(),
        DocumentFormat::Pages
    );
    assert_eq!(
        load_package(&iwpv2_zip).unwrap_err(),
        EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Pages)
    );

    let xml_true_pkg = ZipArchiveCodec::pack(&[
        ("Index/Metadata.iwa".to_string(), vec![1, 2, 3]),
        (
            "Metadata/Properties.plist".to_string(),
            br#"<?xml version="1.0"?><plist version="1.0"><dict><key>isEncrypted</key><true/></dict></plist>"#.to_vec(),
        ),
    ])
    .unwrap();
    assert_eq!(
        FormatDetector::detect(&xml_true_pkg, None).unwrap(),
        DocumentFormat::Pages
    );
    assert_eq!(
        load_package(&xml_true_pkg).unwrap_err(),
        EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Pages)
    );

    let bplist_true_pkg = ZipArchiveCodec::pack(&[
        ("Index/Metadata.iwa".to_string(), vec![1, 2, 3]),
        (
            "Metadata/Properties.plist".to_string(),
            bplist_root_dict(&[("isEncrypted", true)]),
        ),
    ])
    .unwrap();
    assert_eq!(
        load_package(&bplist_true_pkg).unwrap_err(),
        EasyOfficeError::EncryptedDocumentUnsupported(DocumentFormat::Pages)
    );

    // Valid Pages document with explicit false flags in XML and binary plists must decode normally.
    let valid_model = easyoffice_formats::RichDocument {
        title: "Normal".to_string(),
        blocks: vec![easyoffice_formats::RichBlock::Paragraph(
            easyoffice_formats::RichParagraph {
                runs: vec![easyoffice_formats::RichRun::plain("Body text")],
                ..Default::default()
            },
        )],
        ..Default::default()
    };
    let valid_pages =
        UniversalFormatConverter::encode_rich(&valid_model, DocumentFormat::Pages, None).unwrap();
    let base_entries = ZipArchiveCodec::unpack(
        &valid_pages,
        &easyoffice_formats::ZipSecurityPolicy::default(),
    )
    .unwrap();
    for false_plist in [
        br#"<?xml version="1.0"?><plist version="1.0"><dict><key>isEncrypted</key><false/></dict></plist>"#.to_vec(),
        bplist_root_dict(&[("isEncrypted", false)]),
    ] {
        let mut modified: Vec<(String, Vec<u8>)> = base_entries
            .iter()
            .filter(|e| e.path != "Metadata/Properties.plist")
            .map(|e| (e.path.clone(), e.data.clone()))
            .collect();
        modified.push(("Metadata/Properties.plist".to_string(), false_plist));
        let pkg = ZipArchiveCodec::pack(&modified).unwrap();
        assert!(load_package(&pkg).is_ok());
    }

    let legacy_wrapped = ZipArchiveCodec::pack(&[(
        "Doc.pages/index.xml".to_string(),
        b"<sl:document/>".to_vec(),
    )])
    .unwrap();
    assert_eq!(
        load_package(&legacy_wrapped).unwrap_err(),
        EasyOfficeError::InvalidArgument(LEGACY_MESSAGE.to_string())
    );
}
