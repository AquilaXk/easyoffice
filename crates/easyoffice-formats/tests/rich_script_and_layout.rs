//! Superscript/subscript runs and paragraph indents / line spacing in the DOCX, ODT and HWPX
//! writers, plus the DOCX and ODT read-back.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichBlock, RichDocument, RichParagraph, RichRun, RichScript,
    UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy,
};

fn run(text: &str, script: RichScript) -> RichRun {
    RichRun {
        text: text.to_string(),
        script,
        ..RichRun::default()
    }
}

fn sample() -> RichDocument {
    RichDocument {
        title: "배치".to_string(),
        blocks: vec![
            RichBlock::Paragraph(RichParagraph {
                runs: vec![
                    run("X", RichScript::Normal),
                    run("2", RichScript::Superscript),
                    run(" H", RichScript::Normal),
                    run("2", RichScript::Subscript),
                ],
                ..RichParagraph::default()
            }),
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("들여쓴 문단")],
                left_indent_twips: Some(720),
                first_line_indent_twips: Some(360),
                line_spacing_percent: Some(200),
                ..RichParagraph::default()
            }),
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("내어쓴 문단")],
                left_indent_twips: Some(720),
                first_line_indent_twips: Some(-360),
                ..RichParagraph::default()
            }),
        ],
        official_agency: None,
    }
}

fn zip_text(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let entry = entries.iter().find(|e| e.path == path).unwrap();
    String::from_utf8(entry.data.clone()).unwrap()
}

fn encode(format: DocumentFormat) -> Vec<u8> {
    UniversalFormatConverter::encode_rich(&sample(), format, None).unwrap()
}

fn decode(bytes: &[u8], name: &str) -> Vec<RichBlock> {
    match UniversalFormatConverter::decode_rich(bytes, Some(name))
        .unwrap()
        .content
    {
        DecodedContent::Document(d) => d.blocks,
        other => panic!("document expected: {other:?}"),
    }
}

#[test]
fn docx_writes_vert_align_indent_and_line_spacing_and_reads_them_back() {
    let bytes = encode(DocumentFormat::Docx);
    let xml = zip_text(&bytes, "word/document.xml");
    assert!(
        xml.contains("<w:vertAlign w:val=\"superscript\"/>"),
        "{xml}"
    );
    assert!(xml.contains("<w:vertAlign w:val=\"subscript\"/>"));
    assert!(xml.contains(
        "<w:pPr><w:spacing w:line=\"480\" w:lineRule=\"auto\"/><w:ind w:left=\"720\" w:firstLine=\"360\"/></w:pPr>"
    ), "{xml}");
    assert!(xml.contains("<w:ind w:left=\"720\" w:hanging=\"360\"/>"));
    assert_eq!(decode(&bytes, "a.docx"), sample().blocks);
}

#[test]
fn odt_writes_text_position_margins_and_line_height_and_reads_script_back() {
    let bytes = encode(DocumentFormat::Odt);
    let xml = zip_text(&bytes, "content.xml");
    assert!(xml.contains("style:text-position=\"super 58%\""), "{xml}");
    assert!(xml.contains("style:text-position=\"sub 58%\""));
    assert!(xml.contains("fo:margin-left=\"36pt\""), "{xml}");
    assert!(xml.contains("fo:text-indent=\"18pt\""));
    assert!(xml.contains("fo:text-indent=\"-18pt\""));
    assert!(xml.contains("fo:line-height=\"200%\""));
    let back = decode(&bytes, "a.odt");
    let RichBlock::Paragraph(first) = &back[0] else {
        panic!("paragraph");
    };
    let scripts: Vec<RichScript> = first.runs.iter().map(|r| r.script).collect();
    assert_eq!(
        scripts,
        vec![
            RichScript::Normal,
            RichScript::Superscript,
            RichScript::Normal,
            RichScript::Subscript
        ]
    );
}

#[test]
fn hwpx_writes_script_marks_intent_left_margin_and_line_spacing() {
    let bytes = encode(DocumentFormat::Hwpx);
    let header = zip_text(&bytes, "Contents/header.xml");
    assert!(header.contains("<hh:supscript/>"), "{header}");
    assert!(header.contains("<hh:subscript/>"));
    assert!(header.contains("<hc:intent value=\"1800\" unit=\"HWPUNIT\"/><hc:left value=\"3600\""));
    assert!(header.contains("<hc:intent value=\"-1800\" unit=\"HWPUNIT\"/><hc:left value=\"3600\""));
    assert!(header.contains("<hh:lineSpacing type=\"PERCENT\" value=\"200\""));
    let blocks = decode(&bytes, "a.hwpx");
    assert_eq!(blocks.len(), 3);
}
