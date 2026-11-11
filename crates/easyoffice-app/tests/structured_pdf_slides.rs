//! PDF Studio page settings travel as a `pdf` object next to `blocks`, and a slide deck saved as a
//! document keeps its titles, bullets and notes as document structure.

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::{
    DecodedContent, DocumentFormat, PdfSerializer, RichBlock, RichListKind,
    UniversalFormatConverter,
};

fn json_str_field<'a>(json: &'a str, key: &str) -> &'a str {
    let marker = format!("\"{key}\":\"");
    let start = json.find(&marker).unwrap() + marker.len();
    let end = json[start..].find('"').unwrap() + start;
    &json[start..end]
}

fn convert(payload: &str) -> Vec<u8> {
    let json = EasyOfficeIpcRouter::dispatch("convert_document_format", payload).unwrap();
    EasyOfficeIpcRouter::decode_base64(json_str_field(&json, "convertedBase64")).unwrap()
}

const BLOCKS: &str = r#""blocks":[{"type":"paragraph","runs":[{"text":"본문"}]}]"#;

#[test]
fn pdf_page_settings_become_pdf_page_fields() {
    let payload = format!(
        r#"{{"sourceFormat":"pdf","targetFormat":"pdf","title":"결재",{BLOCKS},"pdf":{{"rotationDegrees":270,"watermark":"대외비","stamp":"승인필"}}}}"#
    );
    let parsed = PdfSerializer::parse(&convert(&payload)).unwrap();
    assert_eq!(parsed.watermark_text.as_deref(), Some("대외비"));
    assert_eq!(parsed.pages[0].rotation_degrees, 270);
    assert_eq!(parsed.pages[0].approval_stamp.as_deref(), Some("승인필"));
    assert_eq!(parsed.pages[0].body_text, "본문");
}

#[test]
fn a_pdf_payload_without_settings_is_unchanged_and_other_targets_ignore_them() {
    let plain = format!(r#"{{"sourceFormat":"pdf","targetFormat":"pdf","title":"결재",{BLOCKS}}}"#);
    let parsed = PdfSerializer::parse(&convert(&plain)).unwrap();
    assert_eq!(parsed.watermark_text, None);
    assert_eq!(parsed.pages[0].rotation_degrees, 0);

    let docx = format!(
        r#"{{"sourceFormat":"pdf","targetFormat":"docx","title":"결재",{BLOCKS},"pdf":{{"rotationDegrees":90,"watermark":"대외비"}}}}"#
    );
    assert!(!convert(&docx).is_empty());
}

#[test]
fn invalid_pdf_page_settings_are_rejected() {
    for bad in [
        r#""pdf":{"rotationDegrees":45}"#,
        r#""pdf":{"rotationDegrees":-90}"#,
        r#""pdf":{"rotationDegrees":"90"}"#,
        r#""pdf":{"watermark":5}"#,
        r#""pdf":{"watermark":"a\u0007b"}"#,
        r#""pdf":[]"#,
    ] {
        let payload =
            format!(r#"{{"sourceFormat":"pdf","targetFormat":"pdf","title":"x",{BLOCKS},{bad}}}"#);
        assert!(
            EasyOfficeIpcRouter::dispatch("convert_document_format", &payload).is_err(),
            "{bad}"
        );
    }
}

const SLIDES: &str = r#""slides":[
  {"title":"개요","bullets":["첫째 항목","둘째 항목"],"notes":"발표자 메모"},
  {"title":"결론","bullets":[],"notes":""}
]"#;

fn slides_as_document(format: &str) -> Vec<u8> {
    convert(&format!(
        r#"{{"sourceFormat":"pptx","targetFormat":"{format}","title":"발표",{SLIDES}}}"#
    ))
}

#[test]
fn slides_saved_as_a_document_keep_titles_bullets_and_notes_as_structure() {
    for (format, name) in [
        (DocumentFormat::Docx, "docx"),
        (DocumentFormat::Odt, "odt"),
        (DocumentFormat::Hwpx, "hwpx"),
        (DocumentFormat::Rtf, "rtf"),
    ] {
        let decoded = UniversalFormatConverter::decode_rich(
            &slides_as_document(name),
            Some(&format!("a.{name}")),
        )
        .unwrap();
        assert_eq!(decoded.format, format);
        let DecodedContent::Document(doc) = decoded.content else {
            panic!("{name}: document expected");
        };
        let paragraphs: Vec<_> = doc
            .blocks
            .iter()
            .filter_map(|b| match b {
                RichBlock::Paragraph(p) => Some(p),
                _ => None,
            })
            .collect();
        let titles: Vec<_> = paragraphs
            .iter()
            .filter(|p| p.heading_level > 0)
            .map(|p| p.plain_text())
            .collect();
        assert_eq!(titles, ["개요", "결론"], "{name}");
        let bullets: Vec<_> = paragraphs
            .iter()
            .filter(|p| p.list == Some(RichListKind::Bullet))
            .map(|p| p.plain_text())
            .collect();
        assert_eq!(bullets, ["첫째 항목", "둘째 항목"], "{name}");
        let notes = paragraphs
            .iter()
            .find(|p| p.plain_text() == "발표자 메모")
            .unwrap_or_else(|| panic!("{name}: notes paragraph missing"));
        assert!(
            notes.runs.iter().all(|r| r.italic),
            "{name}: notes are italic"
        );
        assert_eq!(notes.heading_level, 0);
        assert_eq!(notes.list, None);
    }
}

#[test]
fn slides_saved_as_markdown_use_headings_and_list_items() {
    let md = String::from_utf8(slides_as_document("md")).unwrap();
    assert!(md.contains("## 개요"), "{md}");
    assert!(md.contains("- 첫째 항목"), "{md}");
    assert!(md.contains("발표자 메모"), "{md}");
}

#[test]
fn slides_saved_as_a_deck_are_unchanged() {
    let pptx = slides_as_document("pptx");
    let decoded = UniversalFormatConverter::decode_rich(&pptx, Some("a.pptx")).unwrap();
    let DecodedContent::Slides(slides) = decoded.content else {
        panic!("slides expected");
    };
    assert_eq!(slides[0].0, "개요");
    assert_eq!(slides[0].1, ["첫째 항목", "둘째 항목"]);
    assert_eq!(slides[0].2, "발표자 메모");
}

#[test]
fn a_chosen_pptx_file_converted_to_a_document_also_keeps_its_structure() {
    let pptx = slides_as_document("pptx");
    let payload = format!(
        r#"{{"fileName":"발표.pptx","sourceBase64":"{}","targetFormat":"docx"}}"#,
        EasyOfficeIpcRouter::encode_base64(&pptx)
    );
    let docx = convert(&payload);
    let decoded = UniversalFormatConverter::decode_rich(&docx, Some("a.docx")).unwrap();
    let DecodedContent::Document(doc) = decoded.content else {
        panic!("document expected");
    };
    assert!(doc.blocks.iter().any(|b| matches!(
        b,
        RichBlock::Paragraph(p) if p.heading_level > 0 && p.plain_text() == "개요"
    )));
    assert!(doc.blocks.iter().any(|b| matches!(
        b,
        RichBlock::Paragraph(p) if p.list == Some(RichListKind::Bullet) && p.plain_text() == "첫째 항목"
    )));
}

fn pdf_payload(extra: &str) -> String {
    format!(
        r#"{{"sourceFormat":"pdf","targetFormat":"pdf","title":"주석",{BLOCKS_WITH_TEXT},"pdf":{{{extra}}}}}"#
    )
}

const BLOCKS_WITH_TEXT: &str =
    r#""blocks":[{"type":"paragraph","runs":[{"text":"보고서 본문의 핵심 문구"}]}]"#;

#[test]
fn a_highlight_phrase_becomes_annotations_on_the_pdf() {
    let json = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &pdf_payload(r##""highlight":{"text":"핵심","kind":"highlight","color":"#ffee00"}"##),
    )
    .unwrap();
    let bytes =
        EasyOfficeIpcRouter::decode_base64(json_str_field(&json, "convertedBase64")).unwrap();
    let pdf = String::from_utf8_lossy(&bytes);
    assert_eq!(pdf.matches("/Subtype /Highlight").count(), 1, "{pdf}");
    assert!(pdf.contains("/C [1 0.933 0]"), "{pdf}");
    assert!(PdfSerializer::parse(&bytes).is_ok());

    let json = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &pdf_payload(r##""highlight":{"text":"핵심","kind":"underline","color":"#FF0000"}"##),
    )
    .unwrap();
    let bytes =
        EasyOfficeIpcRouter::decode_base64(json_str_field(&json, "convertedBase64")).unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("/Subtype /Underline"));
}

#[test]
fn a_phrase_that_is_not_found_is_reported_in_the_response_warnings() {
    let json = EasyOfficeIpcRouter::dispatch(
        "convert_document_format",
        &pdf_payload(r##""highlight":{"text":"없는 문구","kind":"highlight","color":"#FFFF00"}"##),
    )
    .unwrap();
    assert!(json.contains("찾지 못"), "{json}");
}

#[test]
fn invalid_highlight_settings_are_rejected() {
    let long = "가".repeat(201);
    let bad = [
        r##""highlight":{"text":"","kind":"highlight","color":"#FFFF00"}"##.to_string(),
        r##""highlight":{"kind":"highlight","color":"#FFFF00"}"##.to_string(),
        format!(r##""highlight":{{"text":"{long}","kind":"highlight","color":"#FFFF00"}}"##),
        r##""highlight":{"text":"a\u0007b","kind":"highlight","color":"#FFFF00"}"##.to_string(),
        r##""highlight":{"text":"a\nb","kind":"highlight","color":"#FFFF00"}"##.to_string(),
        r##""highlight":{"text":"가","kind":"squiggly","color":"#FFFF00"}"##.to_string(),
        r##""highlight":{"text":"가","kind":"highlight","color":"red"}"##.to_string(),
        r##""highlight":{"text":"가","kind":"highlight","color":"#FFF"}"##.to_string(),
        r##""highlight":{"text":"가","kind":"highlight","color":"FFFF00"}"##.to_string(),
        r##""highlight":{"text":"가","kind":"highlight"}"##.to_string(),
        r##""highlight":"가""##.to_string(),
        r##""highlight":{"text":5,"kind":"highlight","color":"#FFFF00"}"##.to_string(),
    ];
    for extra in bad {
        assert!(
            EasyOfficeIpcRouter::dispatch("convert_document_format", &pdf_payload(&extra)).is_err(),
            "{extra}"
        );
    }
}

#[test]
fn the_highlight_is_ignored_for_other_targets() {
    let docx = format!(
        r##"{{"sourceFormat":"pdf","targetFormat":"docx","title":"x",{BLOCKS_WITH_TEXT},"pdf":{{"highlight":{{"text":"핵심","kind":"highlight","color":"#FFFF00"}}}}}}"##
    );
    assert!(!convert(&docx).is_empty());
}
