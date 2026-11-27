//! PD-368: the font variant of a run (`font_face`, a PostScript name) is written into a `.pages` file's font field
//! in place of the family, DOCX/ODT/RTF/HWPX keep writing the run's font name, and hostile names are bounded.

use easyoffice_formats::{
    font_face, DecodedContent, DocumentFormat, RichBlock, RichDocument, RichParagraph, RichRun,
    UniversalFormatConverter, MAX_FONT_FACE_BYTES,
};

fn document(font: &str, face: Option<&str>) -> RichDocument {
    RichDocument {
        title: "서체".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun {
                text: "가는체".to_string(),
                font: Some(font.to_string()),
                font_face: face.map(str::to_string),
                ..RichRun::default()
            }],
            ..RichParagraph::default()
        })],
        ..RichDocument::default()
    }
}

fn fonts_after(doc: &RichDocument, format: DocumentFormat, name: &str) -> Vec<Option<String>> {
    let bytes = UniversalFormatConverter::encode_rich(doc, format, None).unwrap();
    let DecodedContent::Document(back) = UniversalFormatConverter::decode_rich(&bytes, Some(name))
        .unwrap()
        .content
    else {
        panic!("document expected");
    };
    back.blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p),
            _ => None,
        })
        .flat_map(|p| p.runs.iter())
        .map(|r| r.font.clone())
        .collect()
}

#[test]
fn pages_writes_the_postscript_name_of_the_variant() {
    let with_face = document("Apple SD Gothic Neo", Some("AppleSDGothicNeo-Light"));
    assert_eq!(
        fonts_after(&with_face, DocumentFormat::Pages, "f.pages"),
        vec![Some("AppleSDGothicNeo-Light".to_string())]
    );
    let family_only = document("Apple SD Gothic Neo", None);
    assert_eq!(
        fonts_after(&family_only, DocumentFormat::Pages, "f.pages"),
        vec![Some("Apple SD Gothic Neo".to_string())]
    );
}

#[test]
fn other_formats_write_the_font_name_of_the_run() {
    let doc = document("Apple SD Gothic Neo Light", Some("AppleSDGothicNeo-Light"));
    for (format, name) in [
        (DocumentFormat::Docx, "f.docx"),
        (DocumentFormat::Odt, "f.odt"),
        (DocumentFormat::Rtf, "f.rtf"),
        (DocumentFormat::Hwpx, "f.hwpx"),
    ] {
        assert_eq!(
            fonts_after(&doc, format, name),
            vec![Some("Apple SD Gothic Neo Light".to_string())],
            "{format:?}"
        );
    }
}

#[test]
fn a_font_face_is_bounded_and_free_of_control_characters() {
    assert_eq!(
        font_face(" AppleSDGothicNeo-Light "),
        Some("AppleSDGothicNeo-Light".to_string())
    );
    assert_eq!(font_face(""), None);
    assert_eq!(font_face("   "), None);
    assert_eq!(font_face("a\u{0}b"), None);
    assert_eq!(font_face("a\nb"), None);
    assert!(font_face(&"가".repeat(MAX_FONT_FACE_BYTES / 3)).is_some());
    assert_eq!(font_face(&"a".repeat(MAX_FONT_FACE_BYTES + 1)), None);
    assert_eq!(
        font_face(&"가".repeat(MAX_FONT_FACE_BYTES / 3 + 1)),
        None,
        "the limit counts bytes"
    );
}

#[test]
fn an_oversized_face_is_not_written_to_pages() {
    let long = "a".repeat(MAX_FONT_FACE_BYTES + 1);
    let doc = document("Apple SD Gothic Neo", Some(&long));
    assert_eq!(
        fonts_after(&doc, DocumentFormat::Pages, "f.pages"),
        vec![Some("Apple SD Gothic Neo".to_string())],
        "the family is written instead"
    );
}
