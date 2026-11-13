//! RTF writer: paragraph properties and run formatting survive write -> read, and a heading's size
//! does not leak into the paragraph after it.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichAlignment, RichBlock, RichDocument, RichListKind,
    RichParagraph, RichRun, RichScript, UniversalFormatConverter,
};

fn paragraph(runs: Vec<RichRun>) -> RichParagraph {
    RichParagraph {
        runs,
        ..RichParagraph::default()
    }
}

fn plain(text: &str) -> RichRun {
    RichRun::plain(text)
}

fn encode(blocks: Vec<RichBlock>) -> String {
    let doc = RichDocument {
        page_frame: Default::default(),
        title: "RTF 왕복".to_string(),
        blocks,
        official_agency: None,
    };
    let bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Rtf, None).unwrap();
    String::from_utf8(bytes).unwrap()
}

fn decode(rtf: &str) -> Vec<RichBlock> {
    let decoded = UniversalFormatConverter::decode_rich(rtf.as_bytes(), Some("a.rtf")).unwrap();
    match decoded.content {
        DecodedContent::Document(doc) => doc.blocks,
        other => panic!("document expected, got {other:?}"),
    }
}

fn roundtrip(blocks: Vec<RichBlock>) -> Vec<RichBlock> {
    decode(&encode(blocks))
}

#[test]
fn every_paragraph_starts_from_plain_character_state_so_a_heading_size_cannot_leak() {
    let rtf = encode(vec![
        RichBlock::Paragraph(RichParagraph {
            heading_level: 1,
            ..paragraph(vec![plain("제목")])
        }),
        RichBlock::Paragraph(paragraph(vec![plain("본문")])),
    ]);
    let paragraph_lines: Vec<&str> = rtf.lines().filter(|l| l.starts_with("\\pard")).collect();
    assert_eq!(paragraph_lines.len(), 2, "{rtf}");
    for line in paragraph_lines {
        assert!(line.starts_with("\\pard\\plain"), "{line}");
    }
    let blocks = decode(&rtf);
    let RichBlock::Paragraph(body) = &blocks[1] else {
        panic!("paragraph expected");
    };
    assert_eq!(body.runs[0].size_half_pt, None);
}

#[test]
fn a_heading_keeps_its_size_after_a_run_with_its_own_format() {
    let styled = RichRun {
        text: "굵은 글꼴".to_string(),
        font: Some("바탕".to_string()),
        color: Some("FF0000".to_string()),
        ..RichRun::default()
    };
    let rtf = encode(vec![RichBlock::Paragraph(RichParagraph {
        heading_level: 1,
        ..paragraph(vec![styled, plain("뒤 런")])
    })]);
    let line = rtf.lines().find(|l| l.starts_with("\\pard")).unwrap();
    let plain_at = line.find("\\plain ").expect("run format is reset");
    assert!(
        line[plain_at..].starts_with("\\plain \\fs32 "),
        "the heading size is restored after \\plain: {line}"
    );
}

#[test]
fn scripts_indents_line_spacing_and_lists_roundtrip() {
    let blocks = vec![
        RichBlock::Paragraph(paragraph(vec![
            plain("H"),
            RichRun {
                text: "2".to_string(),
                script: RichScript::Subscript,
                ..RichRun::default()
            },
            plain("O 와 x"),
            RichRun {
                text: "2".to_string(),
                script: RichScript::Superscript,
                bold: true,
                ..RichRun::default()
            },
            plain(" 끝"),
        ])),
        RichBlock::Paragraph(RichParagraph {
            first_line_indent_twips: Some(480),
            left_indent_twips: Some(360),
            line_spacing_percent: Some(150),
            alignment: Some(RichAlignment::Justify),
            ..paragraph(vec![plain("들여쓰기와 줄 간격")])
        }),
        RichBlock::Paragraph(RichParagraph {
            first_line_indent_twips: Some(-240),
            line_spacing_percent: Some(115),
            ..paragraph(vec![plain("내어쓰기")])
        }),
        RichBlock::Paragraph(RichParagraph {
            list: Some(RichListKind::Bullet),
            ..paragraph(vec![plain("글머리표")])
        }),
        RichBlock::Paragraph(RichParagraph {
            list: Some(RichListKind::Ordered),
            ..paragraph(vec![plain("번호 목록")])
        }),
        RichBlock::Paragraph(paragraph(vec![plain("일반 문단")])),
    ];
    assert_eq!(roundtrip(blocks.clone()), blocks);
}

#[test]
fn lists_are_written_as_a_list_table_and_list_overrides() {
    let rtf = encode(vec![
        RichBlock::Paragraph(RichParagraph {
            list: Some(RichListKind::Bullet),
            ..paragraph(vec![plain("가")])
        }),
        RichBlock::Paragraph(RichParagraph {
            list: Some(RichListKind::Ordered),
            ..paragraph(vec![plain("나")])
        }),
    ]);
    assert!(rtf.contains("{\\*\\listtable"), "{rtf}");
    assert!(rtf.contains("{\\*\\listoverridetable"), "{rtf}");
    assert!(rtf.contains("\\ls1\\ilvl0"), "{rtf}");
    assert!(rtf.contains("\\ls2\\ilvl0"), "{rtf}");
    let plain_doc = encode(vec![RichBlock::Paragraph(paragraph(vec![plain(
        "목록 없음",
    )]))]);
    assert!(!plain_doc.contains("listtable"), "no list, no list table");
}

#[test]
fn format_words_never_leak_into_the_line_based_text_decoder() {
    let rtf = encode(vec![RichBlock::Paragraph(RichParagraph {
        first_line_indent_twips: Some(480),
        left_indent_twips: Some(360),
        line_spacing_percent: Some(150),
        list: Some(RichListKind::Ordered),
        ..paragraph(vec![
            plain("물 "),
            RichRun {
                text: "2".to_string(),
                script: RichScript::Superscript,
                ..RichRun::default()
            },
        ])
    })]);
    let payload = easyoffice_formats::RtfCodec::decode(rtf.as_bytes()).unwrap();
    assert_eq!(payload.paragraphs, vec!["물 2".to_string()], "{rtf}");
}
