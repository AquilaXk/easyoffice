//! Horizontal 3-zone (`Left` · `Center` · `Right`) and first/even header/footer roundtrip tests
//! across OOXML (`DOCX` `w:ptab`), ODF (`ODT` `style:region-left|center|right`), OWPML (`HWPX`),
//! and `Pages` (`PD-249`).

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichAlignment, RichBlock, RichDocument, RichField,
    RichPageFrame, RichParagraph, RichRun, UniversalFormatConverter, ZipArchiveCodec,
    ZipSecurityPolicy,
};

fn zone_para(align: RichAlignment, runs: Vec<RichRun>) -> RichParagraph {
    RichParagraph {
        alignment: Some(align),
        runs,
        ..RichParagraph::default()
    }
}

fn field_run(field: RichField) -> RichRun {
    RichRun {
        field: Some(field),
        ..RichRun::default()
    }
}

fn decode_doc(bytes: &[u8], file_name: &str) -> RichDocument {
    let outcome =
        UniversalFormatConverter::decode_rich(bytes, Some(file_name)).expect("decode doc");
    match outcome.content {
        DecodedContent::Document(doc) => doc,
        other => panic!("expected DecodedContent::Document, got {other:?}"),
    }
}

fn entry_utf8(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).expect("unpack");
    let entry = entries
        .iter()
        .find(|e| e.path == path)
        .unwrap_or_else(|| panic!("missing zip entry {path}"));
    String::from_utf8(entry.data.clone()).expect("utf8 xml")
}

fn sample_three_zone_doc() -> RichDocument {
    RichDocument {
        title: "보고서".to_string(),
        blocks: vec![
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("표지 본문")],
                page_break_before: false,
                ..RichParagraph::default()
            }),
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("둘째 쪽 본문")],
                page_break_before: true,
                ..RichParagraph::default()
            }),
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("셋째 쪽 본문")],
                page_break_before: true,
                ..RichParagraph::default()
            }),
        ],
        official_agency: None,
        page_frame: RichPageFrame {
            header: vec![
                zone_para(RichAlignment::Left, vec![RichRun::plain("연구소")]),
                zone_para(RichAlignment::Center, vec![RichRun::plain("연례 보고서")]),
                zone_para(
                    RichAlignment::Right,
                    vec![
                        field_run(RichField::PageNumber),
                        RichRun::plain(" / "),
                        field_run(RichField::TotalPages),
                    ],
                ),
            ],
            footer: vec![
                zone_para(RichAlignment::Left, vec![field_run(RichField::FileName)]),
                zone_para(RichAlignment::Right, vec![RichRun::plain("대외비")]),
            ],
            first_header: vec![
                zone_para(RichAlignment::Center, vec![RichRun::plain("표지 안내")]),
                zone_para(RichAlignment::Right, vec![RichRun::plain("초판")]),
            ],
            first_footer: Vec::new(),
            even_header: vec![
                zone_para(RichAlignment::Left, vec![field_run(RichField::PageNumber)]),
                zone_para(
                    RichAlignment::Center,
                    vec![RichRun::plain("짝수 쪽 머리말")],
                ),
            ],
            even_footer: vec![
                zone_para(RichAlignment::Left, vec![RichRun::plain("짝수 좌측")]),
                zone_para(RichAlignment::Center, vec![RichRun::plain("짝수 중앙")]),
                zone_para(RichAlignment::Right, vec![RichRun::plain("짝수 우측")]),
            ],
            first_page_different: true,
            even_odd_different: true,
            ..RichPageFrame::default()
        },
        ..RichDocument::default()
    }
}

#[test]
fn docx_encodes_three_zone_frames_as_ptab_single_paragraph_and_roundtrips() {
    let src = sample_three_zone_doc();
    let bytes = UniversalFormatConverter::encode_rich(&src, DocumentFormat::Docx, None)
        .expect("encode docx");

    let hdr1 = entry_utf8(&bytes, "word/header1.xml");
    assert_eq!(
        hdr1.matches("<w:p>").count() + hdr1.matches("<w:p ").count(),
        1,
        "3-zone header must be one horizontal paragraph, got: {hdr1}"
    );
    assert!(
        hdr1.contains("<w:ptab w:alignment=\"center\" w:relativeTo=\"margin\" w:leader=\"none\"/>"),
        "missing center ptab in {hdr1}"
    );
    assert!(
        hdr1.contains("<w:ptab w:alignment=\"right\" w:relativeTo=\"margin\" w:leader=\"none\"/>"),
        "missing right ptab in {hdr1}"
    );

    // Sparse 2-zone [Center, Right] on first_header must start with center ptab so Left stays blank.
    let hdr2 = entry_utf8(&bytes, "word/header2.xml");
    assert!(hdr2.contains("<w:ptab w:alignment=\"center\""));
    assert!(hdr2.contains("<w:ptab w:alignment=\"right\""));

    let out = decode_doc(&bytes, "report.docx");
    assert!(out.page_frame.first_page_different);
    assert!(out.page_frame.even_odd_different);
    assert_eq!(out.page_frame.header.len(), 3);
    assert_eq!(
        out.page_frame.header[0].alignment,
        Some(RichAlignment::Left)
    );
    assert_eq!(out.page_frame.header[0].runs[0].text, "연구소");
    assert_eq!(
        out.page_frame.header[1].alignment,
        Some(RichAlignment::Center)
    );
    assert_eq!(out.page_frame.header[1].runs[0].text, "연례 보고서");
    assert_eq!(
        out.page_frame.header[2].alignment,
        Some(RichAlignment::Right)
    );
    assert_eq!(
        out.page_frame.header[2].runs[0].field,
        Some(RichField::PageNumber)
    );
    assert_eq!(
        out.page_frame.header[2].runs[2].field,
        Some(RichField::TotalPages)
    );

    assert_eq!(out.page_frame.footer.len(), 2);
    assert_eq!(
        out.page_frame.footer[0].alignment,
        Some(RichAlignment::Left)
    );
    assert_eq!(
        out.page_frame.footer[0].runs[0].field,
        Some(RichField::FileName)
    );
    assert_eq!(
        out.page_frame.footer[1].alignment,
        Some(RichAlignment::Right)
    );
    assert_eq!(out.page_frame.footer[1].runs[0].text, "대외비");

    assert_eq!(out.page_frame.first_header.len(), 2);
    assert_eq!(
        out.page_frame.first_header[0].alignment,
        Some(RichAlignment::Center)
    );
    assert_eq!(out.page_frame.first_header[0].runs[0].text, "표지 안내");
    assert_eq!(
        out.page_frame.first_header[1].alignment,
        Some(RichAlignment::Right)
    );
    assert_eq!(out.page_frame.first_header[1].runs[0].text, "초판");
    assert!(out.page_frame.first_footer.is_empty());

    assert_eq!(out.page_frame.even_header.len(), 2);
    assert_eq!(
        out.page_frame.even_header[0].alignment,
        Some(RichAlignment::Left)
    );
    assert_eq!(
        out.page_frame.even_header[1].alignment,
        Some(RichAlignment::Center)
    );
    assert_eq!(out.page_frame.even_footer.len(), 3);
}

#[test]
fn stacked_same_alignment_header_lines_remain_separate_paragraphs_in_docx_and_odt() {
    let doc = RichDocument {
        title: "적층".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain("본문")],
            ..RichParagraph::default()
        })],
        official_agency: None,
        page_frame: RichPageFrame {
            header: vec![
                zone_para(RichAlignment::Left, vec![RichRun::plain("첫째 줄")]),
                zone_para(RichAlignment::Left, vec![RichRun::plain("둘째 줄")]),
            ],
            ..RichPageFrame::default()
        },
        ..RichDocument::default()
    };
    let docx = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).unwrap();
    let hdr_xml = entry_utf8(&docx, "word/header1.xml");
    assert!(
        !hdr_xml.contains("w:ptab"),
        "stacked lines must not use ptab"
    );
    let decoded_docx = decode_doc(&docx, "stacked.docx");
    assert_eq!(decoded_docx.page_frame.header.len(), 2);

    let odt = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Odt, None).unwrap();
    let styles_xml = entry_utf8(&odt, "styles.xml");
    assert!(
        !styles_xml.contains("style:region-"),
        "stacked lines must not use ODF regions"
    );
    let decoded_odt = decode_doc(&odt, "stacked.odt");
    assert_eq!(decoded_odt.page_frame.header.len(), 2);
}

#[test]
fn odt_encodes_style_regions_and_infers_region_alignment_without_paragraph_style() {
    let src = sample_three_zone_doc();
    let bytes =
        UniversalFormatConverter::encode_rich(&src, DocumentFormat::Odt, None).expect("encode odt");
    let styles_xml = entry_utf8(&bytes, "styles.xml");
    assert!(styles_xml.contains("<style:region-left>"), "{styles_xml}");
    assert!(styles_xml.contains("<style:region-center>"), "{styles_xml}");
    assert!(styles_xml.contains("<style:region-right>"), "{styles_xml}");

    let out = decode_doc(&bytes, "report.odt");
    assert!(out.page_frame.first_page_different);
    assert!(out.page_frame.even_odd_different);
    assert_eq!(out.page_frame.header.len(), 3);
    assert_eq!(
        out.page_frame.header[0].alignment,
        Some(RichAlignment::Left)
    );
    assert_eq!(
        out.page_frame.header[1].alignment,
        Some(RichAlignment::Center)
    );
    assert_eq!(
        out.page_frame.header[2].alignment,
        Some(RichAlignment::Right)
    );
    assert_eq!(out.page_frame.first_header.len(), 2);
    assert_eq!(
        out.page_frame.first_header[0].alignment,
        Some(RichAlignment::Center)
    );
    assert_eq!(
        out.page_frame.first_header[1].alignment,
        Some(RichAlignment::Right)
    );

    // External ODT where `<text:p>` inside `<style:region-*>` has no `text:style-name` at all:
    let mut entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default())
        .unwrap()
        .into_iter()
        .map(|e| (e.path, e.data))
        .collect::<Vec<_>>();
    for (path, data) in &mut entries {
        if path == "styles.xml" {
            let custom = concat!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
                "<office:document-styles ",
                "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" ",
                "xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" ",
                "xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" ",
                "xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" ",
                "office:version=\"1.2\">",
                "<office:automatic-styles><style:page-layout style:name=\"pm1\"/></office:automatic-styles>",
                "<office:master-styles><style:master-page style:name=\"Standard\" style:page-layout-name=\"pm1\">",
                "<style:header>",
                "<style:region-left><text:p>외부좌측</text:p></style:region-left>",
                "<style:region-center><text:p>외부중앙</text:p></style:region-center>",
                "<style:region-right><text:p>외부우측</text:p></style:region-right>",
                "</style:header>",
                "</style:master-page></office:master-styles>",
                "</office:document-styles>"
            );
            *data = custom.as_bytes().to_vec();
        }
    }
    let repacked = ZipArchiveCodec::pack_compressed(&entries).unwrap();
    let ext_doc = decode_doc(&repacked, "external.odt");
    assert_eq!(ext_doc.page_frame.header.len(), 3);
    assert_eq!(
        ext_doc.page_frame.header[0].alignment,
        Some(RichAlignment::Left)
    );
    assert_eq!(
        ext_doc.page_frame.header[1].alignment,
        Some(RichAlignment::Center)
    );
    assert_eq!(
        ext_doc.page_frame.header[2].alignment,
        Some(RichAlignment::Right)
    );
}

#[test]
fn hwpx_and_pages_roundtrip_three_zone_headers_and_footers() {
    let src = sample_three_zone_doc();

    let hwpx = UniversalFormatConverter::encode_rich(&src, DocumentFormat::Hwpx, None)
        .expect("encode hwpx");
    let hwpx_out = decode_doc(&hwpx, "report.hwpx");
    assert!(hwpx_out.page_frame.first_page_different);
    assert!(hwpx_out.page_frame.even_odd_different);
    assert_eq!(hwpx_out.page_frame.header.len(), 3);
    assert_eq!(
        hwpx_out.page_frame.header[1].alignment,
        Some(RichAlignment::Center)
    );
    assert_eq!(hwpx_out.page_frame.even_footer.len(), 3);

    let pages = UniversalFormatConverter::encode_rich(&src, DocumentFormat::Pages, None)
        .expect("encode pages");
    let pages_out = decode_doc(&pages, "report.pages");
    assert!(pages_out.page_frame.first_page_different);
    assert!(pages_out.page_frame.even_odd_different);
    assert_eq!(pages_out.page_frame.header.len(), 3);
    assert_eq!(
        pages_out.page_frame.header[0].alignment,
        Some(RichAlignment::Left)
    );
    assert_eq!(
        pages_out.page_frame.header[1].alignment,
        Some(RichAlignment::Center)
    );
    assert_eq!(
        pages_out.page_frame.header[2].alignment,
        Some(RichAlignment::Right)
    );
    assert_eq!(pages_out.page_frame.first_header.len(), 2);
    assert_eq!(
        pages_out.page_frame.first_header[0].alignment,
        Some(RichAlignment::Center)
    );
}

#[test]
#[ignore]
fn bench_three_zone_docx_odt_roundtrip_release() {
    let src = sample_three_zone_doc();
    let rounds = 40u32;
    let start = std::time::Instant::now();
    for _ in 0..rounds {
        let docx = UniversalFormatConverter::encode_rich(&src, DocumentFormat::Docx, None).unwrap();
        let d1 = decode_doc(&docx, "bench.docx");
        assert_eq!(d1.page_frame.header.len(), 3);
        let odt = UniversalFormatConverter::encode_rich(&src, DocumentFormat::Odt, None).unwrap();
        let d2 = decode_doc(&odt, "bench.odt");
        assert_eq!(d2.page_frame.header.len(), 3);
    }
    let avg_us = start.elapsed().as_secs_f64() * 1_000_000.0 / f64::from(rounds);
    eprintln!("PD-249 3-zone DOCX+ODT encode+decode roundtrip avg: {avg_us:.2} us");
    assert!(avg_us < 50_000.0);
}
