//! PD-375: a double strikethrough read from DOCX (`w:dstrike`) or ODT is `fx.double_strike` with `strike` off, so the writers
//! that do not keep the effects (HWPX, Pages, Markdown) must still draw it as a strikethrough; HWPX has the double shape.

use easyoffice_formats::{
    CharAdvanced, DecodedContent, DocumentFormat, RichBlock, RichDocument, RichParagraph, RichRun,
    TextFx, UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy,
};

fn double_struck() -> RichDocument {
    let run = RichRun {
        text: "지움".to_string(),
        advanced: CharAdvanced {
            fx: TextFx {
                double_strike: true,
                ..TextFx::default()
            },
            ..CharAdvanced::default()
        },
        ..RichRun::default()
    };
    RichDocument {
        title: "취소".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![run],
            ..RichParagraph::default()
        })],
        ..RichDocument::default()
    }
}

fn decode(bytes: &[u8], name: &str) -> RichDocument {
    match UniversalFormatConverter::decode_rich(bytes, Some(name))
        .expect("decode")
        .content
    {
        DecodedContent::Document(d) => d,
        other => panic!("expected document: {other:?}"),
    }
}

/// The document as DOCX and back, the way a file read from DOCX or ODT reaches a writer.
fn read_from_docx() -> RichDocument {
    let bytes = UniversalFormatConverter::encode_rich(&double_struck(), DocumentFormat::Docx, None)
        .unwrap();
    let doc = decode(&bytes, "d.docx");
    let RichBlock::Paragraph(p) = &doc.blocks[0] else {
        panic!("paragraph")
    };
    assert!(
        !p.runs[0].strike && p.runs[0].advanced.fx.double_strike,
        "{:?}",
        p.runs[0]
    );
    doc
}

fn struck(doc: &RichDocument) -> bool {
    doc.blocks.iter().any(|b| match b {
        RichBlock::Paragraph(p) => p
            .runs
            .iter()
            .any(|r| r.text == "지움" && (r.strike || r.advanced.fx.double_strike)),
        _ => false,
    })
}

#[test]
fn a_double_strikethrough_from_docx_is_still_struck_in_hwpx_with_the_double_shape() {
    let bytes =
        UniversalFormatConverter::encode_rich(&read_from_docx(), DocumentFormat::Hwpx, None)
            .unwrap();
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let header = String::from_utf8(
        entries
            .iter()
            .find(|e| e.path == "Contents/header.xml")
            .unwrap()
            .data
            .clone(),
    )
    .unwrap();
    assert!(
        header.contains("<hh:strikeout shape=\"DOUBLE_SLIM\""),
        "{header}"
    );
    let back = decode(&bytes, "d.hwpx");
    assert!(struck(&back));
    let RichBlock::Paragraph(p) = &back.blocks[0] else {
        panic!("paragraph")
    };
    assert!(
        p.runs
            .iter()
            .any(|r| r.advanced.fx.double_strike && !r.strike),
        "the double shape reads back as double: {:?}",
        p.runs
    );
}

#[test]
fn a_double_strikethrough_from_docx_is_still_struck_in_pages() {
    let bytes =
        UniversalFormatConverter::encode_rich(&read_from_docx(), DocumentFormat::Pages, None)
            .unwrap();
    assert!(struck(&decode(&bytes, "d.pages")));
}

#[test]
fn a_double_strikethrough_from_docx_is_still_struck_in_markdown() {
    let bytes =
        UniversalFormatConverter::encode_rich(&read_from_docx(), DocumentFormat::Markdown, None)
            .unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("~~지움~~"), "{text}");
}
