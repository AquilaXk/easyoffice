//! Character-run formatting (size, font, color, highlight) survives encode -> decode in every
//! rich writer/reader pair, and the raw XML carries the native properties.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichBlock, RichDocument, RichParagraph, RichRun,
    UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy,
};

fn styled(text: &str) -> RichRun {
    RichRun {
        text: text.to_string(),
        bold: true,
        size_half_pt: Some(28),
        font: Some("함초롬바탕".to_string()),
        color: Some("FF0000".to_string()),
        highlight: Some("FFFF00".to_string()),
        ..RichRun::default()
    }
}

fn custom_highlight(text: &str) -> RichRun {
    RichRun {
        text: text.to_string(),
        italic: true,
        size_half_pt: Some(19),
        color: Some("1A2B3C".to_string()),
        highlight: Some("A1B2C3".to_string()),
        ..RichRun::default()
    }
}

fn sample() -> RichDocument {
    RichDocument {
        title: "런 서식".to_string(),
        blocks: vec![
            RichBlock::Paragraph(RichParagraph {
                runs: vec![
                    RichRun::plain("앞 문장 "),
                    styled("강조 구간"),
                    RichRun::plain(" 뒤 문장 "),
                    custom_highlight("다른 구간"),
                ],
                ..RichParagraph::default()
            }),
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("서식 없는 문단")],
                ..RichParagraph::default()
            }),
        ],
        official_agency: None,
    }
}

fn decode_blocks(bytes: &[u8], name: &str) -> Vec<RichBlock> {
    match UniversalFormatConverter::decode_rich(bytes, Some(name))
        .unwrap()
        .content
    {
        DecodedContent::Document(d) => d.blocks,
        other => panic!("document expected: {other:?}"),
    }
}

fn zip_text(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let entry = entries.iter().find(|e| e.path == path).unwrap();
    String::from_utf8(entry.data.clone()).unwrap()
}

fn assert_roundtrip(format: DocumentFormat, name: &str) -> Vec<u8> {
    let doc = sample();
    let bytes = UniversalFormatConverter::encode_rich(&doc, format, None).unwrap();
    let first = decode_blocks(&bytes, name);
    assert_eq!(first, doc.blocks, "{name}: decoded runs");
    let again = RichDocument {
        blocks: first.clone(),
        ..doc
    };
    let bytes2 = UniversalFormatConverter::encode_rich(&again, format, None).unwrap();
    assert_eq!(decode_blocks(&bytes2, name), first, "{name}: second pass");
    bytes
}

#[test]
fn docx_roundtrip_keeps_size_font_color_highlight_per_run() {
    let bytes = assert_roundtrip(DocumentFormat::Docx, "a.docx");
    let xml = zip_text(&bytes, "word/document.xml");
    assert!(
        xml.contains("<w:sz w:val=\"28\"/><w:szCs w:val=\"28\"/>"),
        "{xml}"
    );
    assert!(xml.contains("w:rFonts w:ascii=\"함초롬바탕\""), "{xml}");
    assert!(xml.contains("<w:color w:val=\"FF0000\"/>"), "{xml}");
    assert!(xml.contains("<w:highlight w:val=\"yellow\"/>"), "{xml}");
    assert!(xml.contains("<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"A1B2C3\"/>"));
    let plain_para = xml.split("<w:p>").nth(2).unwrap();
    assert!(!plain_para.contains("<w:sz"), "plain run stays unformatted");
}

#[test]
fn hwpx_roundtrip_keeps_size_font_color_highlight_per_run() {
    let bytes = assert_roundtrip(DocumentFormat::Hwpx, "a.hwpx");
    let header = zip_text(&bytes, "Contents/header.xml");
    assert!(header.contains("height=\"1400\" textColor=\"#FF0000\" shadeColor=\"#FFFF00\""));
    assert!(header.contains("face=\"함초롬바탕\""));
}

#[test]
fn odt_roundtrip_keeps_size_font_color_highlight_per_run() {
    let bytes = assert_roundtrip(DocumentFormat::Odt, "a.odt");
    let xml = zip_text(&bytes, "content.xml");
    assert!(xml.contains("fo:font-size=\"14pt\""), "{xml}");
    assert!(xml.contains("fo:font-size=\"9.5pt\""), "{xml}");
    assert!(xml.contains("fo:color=\"#FF0000\""), "{xml}");
    assert!(xml.contains("fo:background-color=\"#FFFF00\""), "{xml}");
    assert!(xml.contains("style:font-name=\"함초롬바탕\""), "{xml}");
}

#[test]
fn rtf_roundtrip_keeps_size_font_color_highlight_per_run() {
    let bytes = assert_roundtrip(DocumentFormat::Rtf, "a.rtf");
    let rtf = String::from_utf8(bytes).unwrap();
    assert!(rtf.contains("\\fs28"), "{rtf}");
    assert!(rtf.contains("{\\fonttbl"), "{rtf}");
    assert!(rtf.contains("{\\colortbl;"), "{rtf}");
    assert!(rtf.contains("\\red255\\green0\\blue0;"), "{rtf}");
    assert!(rtf.contains("\\highlight"), "{rtf}");
}

#[test]
fn rtf_plain_text_and_tables_still_decode_after_the_run_reader() {
    let doc = RichDocument {
        title: "표".to_string(),
        blocks: vec![
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("가 {중괄호} \\ 역슬래시 한글")],
                ..RichParagraph::default()
            }),
            RichBlock::Table(vec![
                vec!["a".into(), "b".into()],
                vec!["1".into(), "2".into()],
            ]),
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("끝")],
                ..RichParagraph::default()
            }),
        ],
        official_agency: None,
    };
    let bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Rtf, None).unwrap();
    assert_eq!(decode_blocks(&bytes, "t.rtf"), doc.blocks);
}

#[test]
fn heading_runs_keep_only_explicit_size_overrides() {
    let doc = RichDocument {
        title: "제목".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![
                RichRun::plain("기본 크기 "),
                RichRun {
                    text: "크게".to_string(),
                    size_half_pt: Some(60),
                    ..RichRun::default()
                },
            ],
            heading_level: 2,
            ..RichParagraph::default()
        })],
        official_agency: None,
    };
    for (format, name) in [
        (DocumentFormat::Docx, "h.docx"),
        (DocumentFormat::Hwpx, "h.hwpx"),
        (DocumentFormat::Odt, "h.odt"),
        (DocumentFormat::Rtf, "h.rtf"),
    ] {
        let bytes = UniversalFormatConverter::encode_rich(&doc, format, None).unwrap();
        assert_eq!(decode_blocks(&bytes, name), doc.blocks, "{name}");
    }
}
