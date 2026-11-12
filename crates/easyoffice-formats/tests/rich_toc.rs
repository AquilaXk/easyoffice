//! Table-of-contents blocks of rich documents: the DOCX writer wraps them in a `w:sdt` "Table of
//! Contents" and the ODT writer in `text:table-of-content`; both readers give the title and the entry
//! levels back, and a heading is never mistaken for a TOC paragraph.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichBlock, RichDocument, RichParagraph, RichRun, RichToc,
    UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy,
};

fn para(text: &str, heading_level: u8, toc: Option<RichToc>) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain(text)],
        heading_level,
        toc,
        ..RichParagraph::default()
    })
}

fn sample() -> RichDocument {
    RichDocument {
        title: "목차".to_string(),
        blocks: vec![
            para("목차", 0, Some(RichToc::Title)),
            para("1. 개요", 0, Some(RichToc::Entry(1))),
            para("세부", 0, Some(RichToc::Entry(3))),
            para("개요", 1, None),
            para("본문", 0, None),
            para("세부", 3, None),
        ],
        official_agency: None,
    }
}

fn read_back(bytes: &[u8], name: &str) -> Vec<(String, u8, Option<RichToc>)> {
    let DecodedContent::Document(doc) = UniversalFormatConverter::decode_rich(bytes, Some(name))
        .unwrap()
        .content
    else {
        panic!("document expected");
    };
    doc.blocks
        .iter()
        .map(|b| match b {
            RichBlock::Paragraph(p) => (p.plain_text(), p.heading_level, p.toc),
            _ => panic!("paragraph expected"),
        })
        .collect()
}

fn expected() -> Vec<(String, u8, Option<RichToc>)> {
    vec![
        ("목차".to_string(), 0, Some(RichToc::Title)),
        ("1. 개요".to_string(), 0, Some(RichToc::Entry(1))),
        ("세부".to_string(), 0, Some(RichToc::Entry(3))),
        ("개요".to_string(), 1, None),
        ("본문".to_string(), 0, None),
        ("세부".to_string(), 3, None),
    ]
}

fn part(bytes: &[u8], name: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let entry = entries.iter().find(|e| e.path == name).unwrap();
    String::from_utf8(entry.data.clone()).unwrap()
}

#[test]
fn docx_wraps_the_block_in_a_table_of_contents_container_and_reads_it_back() {
    let bytes =
        UniversalFormatConverter::encode_rich(&sample(), DocumentFormat::Docx, None).unwrap();
    let xml = part(&bytes, "word/document.xml");
    assert_eq!(xml.matches("<w:sdt>").count(), 1);
    assert!(xml.contains("w:docPartGallery w:val=\"Table of Contents\""));
    assert!(xml.contains("<w:pStyle w:val=\"TOC3\"/>"));
    assert!(part(&bytes, "word/styles.xml").contains("w:styleId=\"TOCHeading\""));
    assert_eq!(read_back(&bytes, "a.docx"), expected());
}

#[test]
fn odt_wraps_the_block_in_a_table_of_contents_container_and_reads_it_back() {
    let bytes =
        UniversalFormatConverter::encode_rich(&sample(), DocumentFormat::Odt, None).unwrap();
    let xml = part(&bytes, "content.xml");
    assert_eq!(xml.matches("<text:table-of-content ").count(), 1);
    assert!(xml.contains("text:style-name=\"Contents_20_3\""));
    assert_eq!(read_back(&bytes, "a.odt"), expected());
}

#[test]
fn documents_without_a_table_of_contents_carry_no_container_or_styles() {
    let plain = RichDocument {
        blocks: vec![para("본문", 0, None)],
        ..RichDocument::default()
    };
    let docx = UniversalFormatConverter::encode_rich(&plain, DocumentFormat::Docx, None).unwrap();
    assert!(!part(&docx, "word/document.xml").contains("<w:sdt>"));
    assert!(!part(&docx, "word/styles.xml").contains("TOCHeading"));
    let odt = UniversalFormatConverter::encode_rich(&plain, DocumentFormat::Odt, None).unwrap();
    assert!(!part(&odt, "content.xml").contains("table-of-content"));
}
