//! Comprehensive adversarial regression tests for format security budgets and low-level optimizations.

use easyoffice_formats::{
    base64_codec, check_thumbnail_png, decode_hwp5_eqedit_script, embed_thumbnail, read_thumbnail,
    CellBox, DocumentFormat, EasyOfficeError, HwpxCodec, IeeeCrc32, OdfCodec, OoxmlCodec,
    PdfSerializer, SheetFilter, SheetStyling, UniversalDocumentModel, UniversalFormatConverter,
    ZipArchiveCodec, ZipSecurityPolicy,
};

#[test]
fn test_pdf_objstm_duplicate_offset_fails_closed() {
    // Construct a PDF with an /ObjStm where header pairs share the identical relative offset.
    let objstm_body = b"10 0 11 0\n<< /Type /Catalog >>";
    let stream_len = objstm_body.len();
    let pdf = format!(
        "%PDF-1.5\n\
         1 0 obj << /Type /ObjStm /N 2 /First 10 /Length {stream_len} >>\n\
         stream\n\
         {}\n\
         endstream\n\
         endobj\n\
         xref\n0 2\n\
         trailer << /Root 10 0 R >>\n\
         startxref\n0\n%%EOF\n",
        std::str::from_utf8(objstm_body).unwrap()
    );

    let res = PdfSerializer::parse(pdf.as_bytes());
    assert!(
        res.is_err(),
        "PDF with duplicate /ObjStm offsets must fail closed"
    );
    match res.unwrap_err() {
        EasyOfficeError::CorruptedContainer(msg) => {
            assert!(
                msg.contains("중복") || msg.contains("오프셋"),
                "unexpected error message: {msg}"
            );
        }
        other => panic!("expected CorruptedContainer error, got: {other:?}"),
    }
}

#[test]
fn test_odf_spaces_bomb_fails_closed() {
    // Massive repeat count inside <text:s text:c="..."/>
    let hostile_ods_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
 xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"
 xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
 <office:body>
  <office:spreadsheet>
   <table:table table:name="Sheet1">
    <table:table-row>
     <table:table-cell office:value-type="string">
      <text:p>Boom<text:s text:c="2000000000"/></text:p>
     </table:table-cell>
    </table:table-row>
   </table:table>
  </office:spreadsheet>
 </office:body>
</office:document-content>"#;

    let ods_bytes = ZipArchiveCodec::pack(&[
        (
            "mimetype".to_string(),
            b"application/vnd.oasis.opendocument.spreadsheet".to_vec(),
        ),
        (
            "content.xml".to_string(),
            hostile_ods_content.as_bytes().to_vec(),
        ),
    ])
    .expect("pack test ods");

    let res = OdfCodec::unpack_ods(&ods_bytes);
    assert!(
        res.is_err(),
        "ODF <text:s> space bomb must exceed budget and fail closed"
    );
    assert!(matches!(
        res.unwrap_err(),
        EasyOfficeError::ResourceBudgetExceeded(_)
    ));
}

#[test]
fn test_crc32_and_base64_parity_and_correctness() {
    // CRC-32 validation against standard reference vectors
    let test_data = b"The quick brown fox jumps over the lazy dog";
    let crc = IeeeCrc32::checksum(test_data);
    assert_eq!(crc, 0x414FA339);

    // A large buffer matches the incremental form over two halves
    let large_data = vec![0xABu8; 1024 * 128];
    let (head, tail) = large_data.split_at(large_data.len() / 2);
    assert_eq!(
        IeeeCrc32::checksum(&large_data),
        IeeeCrc32::update(IeeeCrc32::checksum(head), tail)
    );

    // Base64 roundtrip
    let b64_encoded = base64_codec::encode(test_data);
    let b64_decoded = base64_codec::decode(&b64_encoded).expect("must decode");
    assert_eq!(&b64_decoded[..], &test_data[..]);
}

#[test]
fn test_hwp_eqedit_binary_search_keywords() {
    // EQEDIT script conversion utilizing binary searched command list
    let script = b"\x00\x00\x00\x00\x09\x00alpha over beta";
    let mut payload = vec![0u8; 4];
    payload.extend_from_slice(&(script.len() as u16).to_le_bytes());
    for &b in b"alpha over beta" {
        payload.push(b);
        payload.push(0);
    }
    payload[4] = 15;
    payload[5] = 0;

    let decoded = decode_hwp5_eqedit_script(&payload).expect("should convert eqedit script");
    assert!(
        decoded.contains("\\frac"),
        "expected latex fraction, got: {decoded}"
    );
}

#[test]
fn test_selective_thumbnail_roundtrip() {
    // Generate valid 1x1 PNG for thumbnail
    let png = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    check_thumbnail_png(png).expect("valid 1x1 PNG");

    let model = UniversalDocumentModel {
        title: "썸네일 테스트".to_string(),
        paragraphs: vec!["내용".to_string()],
        ..Default::default()
    };
    let docx_bytes =
        UniversalFormatConverter::encode(&model, DocumentFormat::Docx).expect("encode docx");
    let docx_with_thumb =
        embed_thumbnail(DocumentFormat::Docx, &docx_bytes, png).expect("embed docx thumb");

    // Selective read without unpacking all entries
    let extracted = read_thumbnail(DocumentFormat::Docx, &docx_with_thumb)
        .expect("read thumbnail")
        .expect("thumbnail must exist");
    assert_eq!(extracted, png);
}

#[test]
fn test_hwpx_huge_row_addr_table_bomb_fails_closed() {
    let table_xml = r#"
 <hp:p>
  <hp:run>
   <hp:tbl rowCnt="1" colCnt="1">
    <hp:tr>
     <hp:tc>
      <hp:cellAddr rowAddr="1048575" colAddr="0"/>
      <hp:subList>
       <hp:p><hp:run><hp:t>boom</hp:t></hp:run></hp:p>
      </hp:subList>
     </hp:tc>
    </hp:tr>
   </hp:tbl>
  </hp:run>
 </hp:p>"#;
    let hostile_hwpx_section = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<hs:sec xmlns:hs="http://www.owpml.org/owpml/2011/section"
 xmlns:hp="http://www.owpml.org/owpml/2011/paragraph">
{}{}{}{}{}{}
</hs:sec>"#,
        table_xml, table_xml, table_xml, table_xml, table_xml, table_xml
    );

    let base_doc = easyoffice_formats::RichDocument {
        page_frame: Default::default(),
        title: "HWPX 표 폭탄".to_string(),
        blocks: vec![easyoffice_formats::RichBlock::Paragraph(
            easyoffice_formats::RichParagraph {
                runs: vec![easyoffice_formats::RichRun::plain("단락")],
                ..Default::default()
            },
        )],
        official_agency: None,
        ..easyoffice_formats::RichDocument::default()
    };
    let base_hwpx =
        UniversalFormatConverter::encode_rich(&base_doc, DocumentFormat::Hwpx, None).unwrap();

    let mut entries = ZipArchiveCodec::unpack(&base_hwpx, &ZipSecurityPolicy::default()).unwrap();
    let mut replaced = false;
    for entry in &mut entries {
        if entry.path == "Contents/section0.xml" {
            entry.data = hostile_hwpx_section.as_bytes().to_vec();
            replaced = true;
        }
    }
    assert!(
        replaced,
        "Contents/section0.xml must be present in base_hwpx"
    );
    let hostile_hwpx = ZipArchiveCodec::pack_compressed(&entries).unwrap();

    let res = UniversalFormatConverter::decode_rich(&hostile_hwpx, Some("test.hwpx"));
    assert!(
        res.is_err(),
        "HWPX huge rowAddr table bomb must exceed budget and fail closed"
    );
    assert!(matches!(
        res.unwrap_err(),
        EasyOfficeError::ResourceBudgetExceeded(_)
    ));
}

#[test]
fn test_xlsx_full_column_filter_does_not_explode_rows() {
    let cells = vec![
        (0, 0, "A1".to_string()),
        (1, 0, "A2".to_string()),
        (2, 0, "A3".to_string()),
    ];
    let mut filter = SheetFilter::new(CellBox {
        row0: 0,
        col0: 0,
        row1: 1_048_575,
        col1: 0,
    });
    filter.hidden = vec![(3, 1_048_575)];
    let styling = SheetStyling {
        filter: Some(filter),
        ..Default::default()
    };

    let sheets = vec![("Sheet1".to_string(), cells)];
    let caches = vec![std::collections::HashMap::new()];
    let stylings = vec![styling];

    let res = OoxmlCodec::pack_xlsx_multi_styled(&sheets, &caches, &stylings);
    assert!(
        matches!(res, Err(EasyOfficeError::ResourceBudgetExceeded(_))),
        "full-column autoFilter must fail closed before allocating millions of rows"
    );
}

#[test]
fn test_hwpx_table_outside_p_does_not_duplicate_paragraphs() {
    let raw_hwpx_section = r#"<?xml version="1.0" encoding="UTF-8"?>
<hs:sec xmlns:hs="http://www.owpml.org/owpml/2011/section"
 xmlns:hp="http://www.owpml.org/owpml/2011/paragraph">
 <hp:p>
  <hp:run>
   <hp:t>본문 단락</hp:t>
  </hp:run>
 </hp:p>
 <hp:tbl rowCnt="1" colCnt="1">
  <hp:tr>
   <hp:tc>
    <hp:subList>
     <hp:p>
      <hp:run>
       <hp:t>표 내부 셀 단락</hp:t>
      </hp:run>
     </hp:p>
    </hp:subList>
   </hp:tc>
  </hp:tr>
 </hp:tbl>
</hs:sec>"#;

    let base_doc = easyoffice_formats::RichDocument {
        page_frame: Default::default(),
        title: "HWPX 표 중복 방지".to_string(),
        blocks: vec![easyoffice_formats::RichBlock::Paragraph(
            easyoffice_formats::RichParagraph {
                runs: vec![easyoffice_formats::RichRun::plain("본문")],
                ..Default::default()
            },
        )],
        official_agency: None,
        ..easyoffice_formats::RichDocument::default()
    };
    let base_hwpx =
        UniversalFormatConverter::encode_rich(&base_doc, DocumentFormat::Hwpx, None).unwrap();

    let mut entries = ZipArchiveCodec::unpack(&base_hwpx, &ZipSecurityPolicy::default()).unwrap();
    for entry in &mut entries {
        if entry.path == "Contents/section0.xml" {
            entry.data = raw_hwpx_section.as_bytes().to_vec();
        }
    }
    let modified_hwpx = ZipArchiveCodec::pack_compressed(&entries).unwrap();

    let decoded = HwpxCodec::unpack(&modified_hwpx).expect("must unpack hwpx");
    assert_eq!(
        decoded.paragraphs,
        vec!["본문 단락".to_string()],
        "table cell paragraphs must not leak into body paragraphs"
    );
    assert_eq!(decoded.tables.len(), 1);
    assert_eq!(decoded.tables[0], vec![vec!["표 내부 셀 단락".to_string()]]);
}
