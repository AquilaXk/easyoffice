//! PD-263: a DOCX with column sections writes the header and footer references and `titlePg` on the first section's `sectPr`
//! (the later sections inherit them), and the document reads back with its frame.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichBlock, RichColumns, RichDocument, RichPageFrame,
    RichParagraph, RichRun, UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy,
};

fn para(text: &str, columns: Option<RichColumns>) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain(text)],
        columns,
        ..RichParagraph::default()
    })
}

fn frame_para(text: &str) -> RichParagraph {
    RichParagraph {
        runs: vec![RichRun::plain(text)],
        ..RichParagraph::default()
    }
}

fn two_columns() -> Option<RichColumns> {
    Some(RichColumns {
        count: 2,
        equal: true,
        gap_twips: 720,
    })
}

fn document() -> RichDocument {
    RichDocument {
        blocks: vec![
            para("한 단", None),
            para("두 단 첫째", two_columns()),
            para("두 단 둘째", two_columns()),
            para("다시 한 단", None),
        ],
        page_frame: RichPageFrame {
            header: vec![frame_para("머리글")],
            footer: vec![frame_para("바닥글")],
            first_header: vec![frame_para("표지 머리글")],
            first_page_different: true,
            ..RichPageFrame::default()
        },
        ..RichDocument::default()
    }
}

fn document_xml(bytes: &[u8]) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let entry = entries
        .iter()
        .find(|e| e.path == "word/document.xml")
        .unwrap();
    String::from_utf8(entry.data.clone()).unwrap()
}

fn sect_prs(xml: &str) -> Vec<&str> {
    xml.match_indices("<w:sectPr>")
        .map(|(start, _)| &xml[start..start + xml[start..].find("</w:sectPr>").unwrap()])
        .collect()
}

#[test]
fn only_the_first_section_carries_the_header_references_and_title_page() {
    let docx =
        UniversalFormatConverter::encode_rich(&document(), DocumentFormat::Docx, None).unwrap();
    let xml = document_xml(&docx);
    let sections = sect_prs(&xml);
    assert_eq!(sections.len(), 3, "{xml}");
    assert!(
        sections[0].contains("<w:headerReference"),
        "{}",
        sections[0]
    );
    assert!(sections[0].contains("<w:footerReference"));
    assert!(sections[0].contains("<w:titlePg/>"));
    for later in &sections[1..] {
        assert!(!later.contains("Reference"), "inherits: {later}");
        assert!(!later.contains("titlePg"), "{later}");
    }
    let cols = sections[0].find("<w:cols").unwrap_or(0);
    assert!(
        sections[0].find("<w:titlePg/>").unwrap() > cols,
        "titlePg follows cols"
    );
}

#[test]
fn the_frame_reads_back_from_the_first_section() {
    let docx =
        UniversalFormatConverter::encode_rich(&document(), DocumentFormat::Docx, None).unwrap();
    let decoded = UniversalFormatConverter::decode_rich(&docx, Some("a.docx")).unwrap();
    let DecodedContent::Document(doc) = decoded.content else {
        panic!("document");
    };
    assert_eq!(doc.page_frame.header[0].plain_text(), "머리글");
    assert_eq!(doc.page_frame.footer[0].plain_text(), "바닥글");
    assert!(doc.page_frame.first_page_different);
    assert_eq!(doc.page_frame.first_header[0].plain_text(), "표지 머리글");
    assert!(
        !decoded.warnings.iter().any(|w| w.contains("구역마다")),
        "{:?}",
        decoded.warnings
    );
}
