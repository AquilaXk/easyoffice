//! Word-style document features (PD-395): each one is written to DOCX and read back, and the readers refuse what
//! a hostile file claims beyond the bounds.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichBinding, RichBindingKind, RichBlock, RichDocument,
    RichPageFrame, RichPageMargins, RichPaper, RichParagraph, RichRun, UniversalFormatConverter,
    ZipArchiveCodec, ZipSecurityPolicy,
};

pub fn decode(bytes: &[u8], name: &str) -> RichDocument {
    match UniversalFormatConverter::decode_rich(bytes, Some(name))
        .expect("decode")
        .content
    {
        DecodedContent::Document(doc) => doc,
        _ => panic!("document expected"),
    }
}

pub fn encode(doc: &RichDocument, format: DocumentFormat) -> Vec<u8> {
    UniversalFormatConverter::encode_rich(doc, format, None).expect("encode")
}

pub fn round_trip(doc: &RichDocument, format: DocumentFormat, name: &str) -> RichDocument {
    decode(&encode(doc, format), name)
}

pub fn part(bytes: &[u8], path: &str) -> String {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .expect("unpack")
        .into_iter()
        .find(|e| e.path == path)
        .map(|e| String::from_utf8_lossy(&e.data).into_owned())
        .unwrap_or_else(|| panic!("missing part {path}"))
}

pub fn paragraph(text: &str) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain(text)],
        ..RichParagraph::default()
    })
}

pub fn doc_of(blocks: Vec<RichBlock>) -> RichDocument {
    RichDocument {
        title: "속성".to_string(),
        blocks,
        ..RichDocument::default()
    }
}

#[test]
fn a_folded_booklet_binding_is_written_and_read_as_book_fold_printing() {
    let mut doc = doc_of(vec![paragraph("본문")]);
    doc.page_frame = RichPageFrame {
        margins: Some(RichPageMargins::DOC_DEFAULT),
        paper: Some(RichPaper {
            width: 16_838,
            height: 11_906,
        }),
        binding: RichBinding {
            kind: RichBindingKind::BookFold,
            gutter: 567,
        },
        ..RichPageFrame::default()
    };
    let bytes = encode(&doc, DocumentFormat::Docx);
    let settings = part(&bytes, "word/settings.xml");
    assert!(settings.contains("<w:bookFoldPrinting/>"), "{settings}");
    assert!(!settings.contains("mirrorMargins"), "{settings}");
    let back = decode(&bytes, "b.docx");
    assert_eq!(back.page_frame.binding, doc.page_frame.binding);
    assert_eq!(back.page_frame.paper, doc.page_frame.paper);
}
