//! A list paragraph in HWPX names the numbering or bullet definition that draws its marker
//! (`<hh:heading idRef>`, 1-based). The writer defines them, so a layout engine draws the marker
//! instead of leaving the item bare.

use easyoffice_formats::{
    DocumentFormat, RichBlock, RichDocument, RichListKind, RichParagraph, RichRun,
    UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy,
};

fn item(kind: RichListKind, text: &str) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain(text)],
        list: Some(kind),
        ..RichParagraph::default()
    })
}

fn header_of(blocks: Vec<RichBlock>) -> String {
    let doc = RichDocument {
        title: "목록".into(),
        blocks,
        official_agency: None,
    };
    let bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Hwpx, None).unwrap();
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let header = entries
        .iter()
        .find(|entry| entry.path == "Contents/header.xml")
        .expect("header.xml");
    String::from_utf8(header.data.clone()).unwrap()
}

#[test]
fn a_bullet_item_refers_to_a_bullet_definition() {
    let header = header_of(vec![item(RichListKind::Bullet, "항목")]);
    assert!(
        header.contains("<hh:bullets itemCnt=\"1\"><hh:bullet id=\"1\" char=\"•\""),
        "no bullet definition"
    );
    assert!(header.contains("<hh:heading type=\"BULLET\" idRef=\"1\" level=\"0\"/>"));
    assert!(!header.contains("<hh:numberings"), "no numbering is used");
}

#[test]
fn an_ordered_item_refers_to_a_numbering_definition() {
    let header = header_of(vec![item(RichListKind::Ordered, "항목")]);
    assert!(
        header.contains("<hh:numberings itemCnt=\"1\"><hh:numbering id=\"1\""),
        "no numbering definition"
    );
    assert!(header.contains(">^1.</hh:paraHead>"));
    assert!(header.contains("<hh:heading type=\"NUMBER\" idRef=\"1\" level=\"0\"/>"));
    assert!(!header.contains("<hh:bullets"));
}

#[test]
fn a_document_without_lists_defines_neither() {
    let header = header_of(vec![RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain("본문")],
        ..RichParagraph::default()
    })]);
    assert!(!header.contains("<hh:bullets") && !header.contains("<hh:numberings"));
    assert!(header.contains("<hh:heading type=\"NONE\" idRef=\"0\" level=\"0\"/>"));
}
