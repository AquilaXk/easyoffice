//! The rich-document PDF (PD-167): a document the engine wrote as HWPX comes out as laid-out pages
//! with real text, a table, a picture, the page rotation and the watermark and stamp. The fonts are
//! the bundled face (the tests return early where the rhwp checkout is missing, like the other
//! font tests).

use super::*;
use crate::pdf_export::verify_exported_pdf;
use easyoffice_formats::{
    DocumentFormat, RichAlignment, RichBlock, RichDocument, RichImage, RichListKind, RichParagraph,
    RichRun, UniversalFormatConverter,
};
use std::collections::HashSet;

fn fonts() -> Option<Arc<Database>> {
    crate::pdf_fonts::tests::bundled_only_db().map(Arc::new)
}

fn paragraph(text: &str) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain(text)],
        ..RichParagraph::default()
    })
}

/// A 2x2 opaque PNG (one stored zlib block), enough for the layout to place a picture.
fn png() -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        png.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend(data);
        png.extend(&body);
        let mut crc = 0xFFFF_FFFFu32;
        for byte in &body {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(crc & 1));
            }
        }
        png.extend((!crc).to_be_bytes());
    };
    chunk(b"IHDR", &[0, 0, 0, 2, 0, 0, 0, 2, 8, 2, 0, 0, 0]);
    let raw: Vec<u8> = (0..2).flat_map(|_| [0, 200, 30, 30, 30, 30, 200]).collect();
    let mut zlib = vec![
        0x78,
        0x01,
        0x01,
        raw.len() as u8,
        0,
        !(raw.len() as u8),
        0xFF,
    ];
    zlib.extend(&raw);
    let (mut a, mut b) = (1u32, 0u32);
    for byte in &raw {
        a = (a + u32::from(*byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    zlib.extend(((b << 16) | a).to_be_bytes());
    chunk(b"IDAT", &zlib);
    chunk(b"IEND", &[]);
    png
}

fn document(extra_paragraphs: usize) -> RichDocument {
    let mut blocks = vec![
        RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain("분기 보고서 Quarterly")],
            heading_level: 1,
            alignment: Some(RichAlignment::Center),
            ..RichParagraph::default()
        }),
        paragraph("한글과 English가 섞인 본문입니다."),
        RichBlock::Table(
            vec![
                vec!["구분".to_string(), "합계".to_string()],
                vec!["매출".to_string(), "2,650".to_string()],
            ]
            .into(),
        ),
        RichBlock::Image(RichImage::new(png(), "그림").expect("png")),
    ];
    blocks.extend((0..extra_paragraphs).map(|n| paragraph(&format!("{n}번째 줄 filler text"))));
    RichDocument {
        page_frame: Default::default(),
        title: "보고서".into(),
        blocks,
        official_agency: None,
    }
}

fn hwpx(doc: &RichDocument) -> Vec<u8> {
    UniversalFormatConverter::encode_rich(doc, DocumentFormat::Hwpx, None).expect("hwpx")
}

fn page_count(pdf: &[u8]) -> usize {
    let text = String::from_utf8_lossy(pdf);
    text.matches("/Type /Page\n").count() + text.matches("/Type /Page ").count()
}

/// Every character the PDF's text maps (`/ToUnicode`) can give back: what a viewer can search and
/// copy. The maps are written as plain text, one `<code> <UTF-16 hex>` line per glyph.
fn searchable_chars(pdf: &[u8]) -> HashSet<char> {
    let text = String::from_utf8_lossy(pdf);
    let mut chars = HashSet::new();
    for block in text.split("beginbfchar").skip(1) {
        let block = block.split("endbfchar").next().unwrap_or_default();
        for line in block.lines() {
            let Some(target) = line.rsplit('<').next().map(|t| t.trim_end_matches('>')) else {
                continue;
            };
            let units: Vec<u16> = (0..target.len() / 4)
                .filter_map(|i| u16::from_str_radix(target.get(i * 4..i * 4 + 4)?, 16).ok())
                .collect();
            chars.extend(char::decode_utf16(units).flatten());
        }
    }
    chars
}

fn assert_searchable(pdf: &[u8], phrase: &str) {
    let chars = searchable_chars(pdf);
    for c in phrase.chars().filter(|c| !c.is_whitespace()) {
        assert!(chars.contains(&c), "{c:?} of {phrase:?} is not searchable");
    }
}

#[test]
fn a_rich_document_becomes_laid_out_pages_with_text_a_table_and_a_picture() {
    let Some(fonts) = fonts() else { return };
    let pdf = hwpx_to_pdf(&hwpx(&document(0)), &PdfWriteOptions::default(), &fonts).expect("pdf");
    verify_exported_pdf(&pdf, 1, true).expect("one page with embedded text");
    for wanted in [
        "분기 보고서 Quarterly",
        "한글과 English가 섞인 본문",
        "구분 합계 매출 2,650",
    ] {
        assert_searchable(&pdf, wanted);
    }
    assert!(
        String::from_utf8_lossy(&pdf).contains("/Subtype /Image"),
        "the picture is in the PDF"
    );
}

/// The markers are drawn by the layout from the definitions the writer puts in the header; the
/// items hold no digits or bullets themselves.
#[test]
fn list_items_get_their_bullet_and_number() {
    let Some(fonts) = fonts() else { return };
    let item = |kind, text: &str| {
        RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain(text)],
            list: Some(kind),
            ..RichParagraph::default()
        })
    };
    let doc = RichDocument {
        page_frame: Default::default(),
        title: "목록".into(),
        blocks: vec![
            item(RichListKind::Ordered, "가나"),
            item(RichListKind::Ordered, "다라"),
            item(RichListKind::Ordered, "마바"),
        ],
        official_agency: None,
    };
    let numbered = hwpx_to_pdf(&hwpx(&doc), &PdfWriteOptions::default(), &fonts).unwrap();
    assert_searchable(&numbered, "123.");
    let doc = RichDocument {
        blocks: vec![item(RichListKind::Bullet, "가나")],
        ..doc
    };
    let bulleted = hwpx_to_pdf(&hwpx(&doc), &PdfWriteOptions::default(), &fonts).unwrap();
    assert_searchable(&bulleted, "•");
}

#[test]
fn a_long_document_crosses_the_conversion_batches() {
    let Some(fonts) = fonts() else { return };
    let pdf =
        hwpx_to_pdf(&hwpx(&document(1500)), &PdfWriteOptions::default(), &fonts).expect("pdf");
    let pages = page_count(&pdf);
    assert!(pages > crate::pdf_export::BATCH_PAGES, "{pages} pages");
    verify_exported_pdf(&pdf, pages, true).expect("every page is in the tree");
    assert_searchable(&pdf, "1499번째 줄 filler text");
}

#[test]
fn page_settings_become_rotation_watermark_and_stamp() {
    let Some(fonts) = fonts() else { return };
    let options = PdfWriteOptions {
        rotation_degrees: 90,
        watermark: Some("대외비".into()),
        approval_stamp: Some("승인필".into()),
        ..PdfWriteOptions::default()
    };
    let pdf = hwpx_to_pdf(&hwpx(&document(120)), &options, &fonts).expect("pdf");
    let pages = page_count(&pdf);
    assert!(pages >= 2, "{pages} pages");
    assert_eq!(
        String::from_utf8_lossy(&pdf).matches("/Rotate 90").count(),
        pages
    );
    // Neither label is in the body, so their characters can only come from the drawn marks.
    assert_searchable(&pdf, "대외비 승인필");

    let plain = hwpx_to_pdf(&hwpx(&document(0)), &PdfWriteOptions::default(), &fonts).unwrap();
    assert!(!String::from_utf8_lossy(&plain).contains("/Rotate"));
}

#[test]
fn bytes_that_are_no_document_are_an_error_not_a_panic() {
    let Some(fonts) = fonts() else { return };
    let options = PdfWriteOptions::default();
    assert!(hwpx_to_pdf(b"not a document", &options, &fonts).is_err());
    assert!(hwpx_to_pdf(&[], &options, &fonts).is_err());
    // Over the size limit it is refused before any layout starts.
    assert!(hwpx_to_pdf(&vec![0; MAX_HWPX_BYTES + 1], &options, &fonts).is_err());
    let mut broken = hwpx(&document(0));
    broken.truncate(broken.len() / 2);
    assert!(hwpx_to_pdf(&broken, &options, &fonts).is_err());
}

/// The whole save path of a converted document: the engine's request handler hands the HWPX to the
/// renderer, answers with the PDF, and the shell's save check accepts that PDF.
#[test]
fn a_converted_document_passes_through_the_engine_response_and_the_save_check() {
    let Some(fonts) = fonts() else { return };
    let draw = |hwpx: &[u8], options: &PdfWriteOptions| hwpx_to_pdf(hwpx, options, &fonts);
    let payload = r#"{"sourceFormat":"docx","targetFormat":"pdf","title":"보고서","blocks":[{"type":"paragraph","heading":1,"runs":[{"text":"제목"}]},{"type":"paragraph","runs":[{"text":"본문 text","bold":true}]}],"pdf":{"watermark":"초안"}}"#;
    let response = easyoffice_app::EasyOfficeIpcRouter::dispatch_with_pdf_renderer(
        "convert_document_format",
        payload,
        &draw,
    )
    .expect("response");
    assert!(response.contains("\"status\":\"OK\""), "{response}");
    assert!(!response.contains("기본 PDF"), "no fallback notice");
    let marker = "\"convertedBase64\":\"";
    let start = response.find(marker).unwrap() + marker.len();
    let base64 = &response[start..start + response[start..].find('"').unwrap()];
    let request = crate::save_file::SaveRequest::parse("보고서.pdf", base64).expect("save check");
    let pdf = request.bytes();
    verify_exported_pdf(pdf, 1, true).expect("one page");
    assert_searchable(pdf, "제목 본문 text 초안");
}

/// The PDF the engine's request handler answers for a source file of `file`, drawn by `fonts`.
fn converted_source_pdf(source: &[u8], file: &str, fonts: &Arc<Database>) -> Vec<u8> {
    let draw = |bytes: &[u8], options: &PdfWriteOptions| hwpx_to_pdf(bytes, options, fonts);
    let request = format!(
        r#"{{"fileName":"{file}","sourceBase64":"{}","targetFormat":"pdf","pdf":{{"watermark":"초안"}}}}"#,
        easyoffice_app::EasyOfficeIpcRouter::encode_base64(source)
    );
    let response = easyoffice_app::EasyOfficeIpcRouter::dispatch_with_pdf_renderer(
        "convert_document_format",
        &request,
        &draw,
    )
    .expect("response");
    assert!(!response.contains("기본 PDF"), "{file}: {response}");
    let marker = "\"convertedBase64\":\"";
    let start = response.find(marker).unwrap() + marker.len();
    let base64 = &response[start..start + response[start..].find('"').unwrap()];
    easyoffice_app::EasyOfficeIpcRouter::decode_base64(base64).unwrap()
}

/// A converted `.hwpx` or `.hwp` is laid out from its own bytes (the request handler no longer
/// writes it back through the engine's model), with the request's page settings drawn in.
#[test]
fn a_native_hangul_source_is_drawn_from_its_own_bytes() {
    let Some(fonts) = fonts() else { return };
    let hwpx = converted_source_pdf(&hwpx(&document(0)), "보고서.hwpx", &fonts);
    verify_exported_pdf(&hwpx, 1, true).expect("one page");
    assert_searchable(&hwpx, "분기 보고서 Quarterly 초안");

    let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../third_party/rhwp/saved/blank2010.hwp");
    let Ok(blank_hwp) = std::fs::read(sample) else {
        return;
    };
    let hwp = converted_source_pdf(&blank_hwp, "빈.hwp", &fonts);
    verify_exported_pdf(&hwp, 1, true).expect("one page");
    assert_searchable(&hwp, "초안");
}

/// First `/MediaBox` of the PDF as `(width, height)` in points.
fn first_media_box(pdf: &[u8]) -> (f64, f64) {
    let text = String::from_utf8_lossy(pdf);
    let at = text.find("/MediaBox").expect("a media box");
    let numbers: Vec<f64> = text[at..]
        .split(['[', ']'])
        .nth(1)
        .expect("a box")
        .split_whitespace()
        .filter_map(|n| n.parse().ok())
        .collect();
    (numbers[2] - numbers[0], numbers[3] - numbers[1])
}

fn framed_document(landscape: bool) -> RichDocument {
    use easyoffice_formats::{RichField, RichPageFrame, RichPageMargins, RichPaper};
    let field = |field| RichRun {
        field: Some(field),
        ..RichRun::default()
    };
    let mut doc = document(120);
    doc.page_frame = RichPageFrame {
        header: vec![RichParagraph {
            runs: vec![RichRun::plain("쪽머리글 Header")],
            alignment: Some(RichAlignment::Center),
            ..RichParagraph::default()
        }],
        footer: vec![RichParagraph {
            runs: vec![
                RichRun::plain("- "),
                field(RichField::PageNumber),
                RichRun::plain(" / "),
                field(RichField::TotalPages),
                RichRun::plain(" -"),
            ],
            alignment: Some(RichAlignment::Center),
            ..RichParagraph::default()
        }],
        margins: Some(RichPageMargins {
            top: 1984,
            bottom: 1700,
            header: 1134,
            footer: 850,
            left: 1701,
            right: 1701,
        }),
        paper: Some(if landscape {
            RichPaper {
                width: 16_838,
                height: 11_906,
            }
        } else {
            RichPaper::A4
        }),
        binding: Default::default(),
    };
    doc
}

/// The header and footer of the page and the page numbers come out of the layout on every page, and the paper
/// of the page setup is the page's size (PD-213).
#[test]
fn the_header_footer_and_the_page_setup_of_a_document_are_in_its_pdf() {
    let Some(fonts) = fonts() else { return };
    for landscape in [false, true] {
        let pdf = hwpx_to_pdf(
            &hwpx(&framed_document(landscape)),
            &PdfWriteOptions::default(),
            &fonts,
        )
        .expect("pdf");
        let pages = page_count(&pdf);
        assert!(pages >= 3, "{pages} pages");
        verify_exported_pdf(&pdf, pages, true).expect("every page is in the tree");
        assert_searchable(&pdf, "쪽머리글 Header");
        let (width, height) = first_media_box(&pdf);
        assert_eq!(width > height, landscape, "{width} x {height}");
        // A4 in points (595 x 842), within a point.
        let (short, long) = (width.min(height), width.max(height));
        assert!(
            (short - 595.0).abs() < 2.0 && (long - 842.0).abs() < 2.0,
            "{short} x {long}"
        );
    }
}
