//! DOCX lists are read with their nesting level and number format (PD-183): `w:numPr/w:ilvl`
//! gives `list_level`, and the level's `w:numFmt` and `w:lvlText` in `word/numbering.xml` give the
//! kind and the number format. Before, every list paragraph read back at level 0 with the kind of
//! the first level.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, EasyOfficeError, RichBlock, RichDocument, RichListKind,
    RichListNumber, RichNumberEnclosure, RichNumberFormat, RichParagraph, RichRun,
    UniversalFormatConverter, ZipArchiveCodec,
};

const W: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"";

fn item(text: &str, kind: RichListKind, level: u8, number: Option<RichListNumber>) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain(text)],
        list: Some(kind),
        list_level: level,
        list_number: number,
        ..RichParagraph::default()
    })
}

fn number(format: RichNumberFormat, enclosure: RichNumberEnclosure) -> Option<RichListNumber> {
    Some(RichListNumber { format, enclosure })
}

fn decode_docx(bytes: &[u8]) -> Result<RichDocument, EasyOfficeError> {
    let decoded = UniversalFormatConverter::decode_rich(bytes, Some("w.docx"))?;
    let DecodedContent::Document(doc) = decoded.content else {
        panic!("document expected");
    };
    Ok(doc)
}

/// `(list kind, level, number format, text)` of every paragraph.
fn lists(doc: &RichDocument) -> Vec<(Option<RichListKind>, u8, Option<RichListNumber>, String)> {
    doc.blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some((p.list, p.list_level, p.list_number, p.plain_text())),
            _ => None,
        })
        .collect()
}

fn docx(document_body: &str, numbering: Option<&str>) -> Vec<u8> {
    let doc_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document {W}><w:body>{document_body}<w:sectPr/></w:body></w:document>"
    );
    let mut entries = vec![
        ("[Content_Types].xml".to_string(), b"<Types/>".to_vec()),
        ("word/document.xml".to_string(), doc_xml.into_bytes()),
    ];
    if let Some(numbering) = numbering {
        entries.push((
            "word/numbering.xml".to_string(),
            format!("<w:numbering {W}>{numbering}</w:numbering>").into_bytes(),
        ));
    }
    ZipArchiveCodec::pack(&entries).unwrap()
}

fn list_paragraph(level: &str, num_id: &str, text: &str) -> String {
    format!(
        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"{level}\"/><w:numId w:val=\"{num_id}\"/></w:numPr></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"
    )
}

#[test]
fn written_list_levels_and_number_formats_read_back() {
    let blocks = vec![
        item("점", RichListKind::Bullet, 0, None),
        item("점 하위", RichListKind::Bullet, 1, None),
        item("점 하위 둘째", RichListKind::Bullet, 2, None),
        RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain("본문")],
            ..RichParagraph::default()
        }),
        item("첫째", RichListKind::Ordered, 0, None),
        item(
            "첫째의 하위",
            RichListKind::Ordered,
            1,
            number(
                RichNumberFormat::LowerLatin,
                RichNumberEnclosure::RightParenthesis,
            ),
        ),
        item(
            "더 깊은 하위",
            RichListKind::Ordered,
            2,
            number(
                RichNumberFormat::UpperRoman,
                RichNumberEnclosure::Parentheses,
            ),
        ),
        item(
            "가나다",
            RichListKind::Ordered,
            3,
            number(RichNumberFormat::HangulSyllable, RichNumberEnclosure::Dot),
        ),
        item("둘째", RichListKind::Ordered, 0, None),
        item(
            "여덟째 수준",
            RichListKind::Ordered,
            8,
            number(RichNumberFormat::LowerRoman, RichNumberEnclosure::Dot),
        ),
    ];
    let original = RichDocument {
        title: "목록".into(),
        blocks,
        official_agency: None,
    };
    let bytes =
        UniversalFormatConverter::encode_rich(&original, DocumentFormat::Docx, None).unwrap();
    let read = decode_docx(&bytes).unwrap();
    assert_eq!(lists(&read), lists(&original));
}

#[test]
fn a_foreign_document_resolves_each_level_through_its_abstract_definition() {
    let numbering = concat!(
        "<w:abstractNum w:abstractNumId=\"0\">",
        "<w:lvl w:ilvl=\"0\"><w:numFmt w:val=\"decimal\"/><w:lvlText w:val=\"%1.\"/></w:lvl>",
        "<w:lvl w:ilvl=\"1\"><w:numFmt w:val=\"bullet\"/><w:lvlText w:val=\"o\"/></w:lvl>",
        "<w:lvl w:ilvl=\"2\"><w:numFmt w:val=\"lowerLetter\"/><w:lvlText w:val=\"(%3)\"/></w:lvl>",
        "</w:abstractNum>",
        "<w:num w:numId=\"5\"><w:abstractNumId w:val=\"0\"/></w:num>",
        "<w:num w:numId=\"6\"><w:abstractNumId w:val=\"0\"/>",
        "<w:lvlOverride w:ilvl=\"0\"><w:lvl w:ilvl=\"0\"><w:numFmt w:val=\"upperLetter\"/><w:lvlText w:val=\"%1)\"/></w:lvl></w:lvlOverride></w:num>"
    );
    let body = [
        list_paragraph("0", "5", "하나"),
        list_paragraph("1", "5", "점"),
        list_paragraph("2", "5", "가"),
        list_paragraph("0", "6", "다른 번호"),
        list_paragraph("0", "0", "목록 아님"),
    ]
    .concat();
    let read = decode_docx(&docx(&body, Some(numbering))).unwrap();
    assert_eq!(
        lists(&read),
        vec![
            (Some(RichListKind::Ordered), 0, None, "하나".to_string()),
            (Some(RichListKind::Bullet), 1, None, "점".to_string()),
            (
                Some(RichListKind::Ordered),
                2,
                number(
                    RichNumberFormat::LowerLatin,
                    RichNumberEnclosure::Parentheses
                ),
                "가".to_string()
            ),
            (
                Some(RichListKind::Ordered),
                0,
                number(
                    RichNumberFormat::UpperLatin,
                    RichNumberEnclosure::RightParenthesis
                ),
                "다른 번호".to_string()
            ),
            (None, 0, None, "목록 아님".to_string()),
        ]
    );
}

#[test]
fn unreadable_levels_and_missing_definitions_fall_back_without_failing() {
    let numbering = concat!(
        "<w:abstractNum w:abstractNumId=\"0\"><w:lvl w:ilvl=\"0\"><w:numFmt w:val=\"decimal\"/></w:lvl></w:abstractNum>",
        "<w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num>"
    );
    let body = [
        list_paragraph("99", "1", "너무 깊음"),
        list_paragraph("4294967296", "1", "u32 밖"),
        list_paragraph("-1", "1", "음수"),
        list_paragraph("x", "1", "숫자 아님"),
        list_paragraph("1", "1", "정의 없는 수준"),
        list_paragraph("0", "77", "없는 목록"),
    ]
    .concat();
    let read = decode_docx(&docx(&body, Some(numbering))).unwrap();
    let ordered = Some(RichListKind::Ordered);
    assert_eq!(
        lists(&read)
            .into_iter()
            .map(|(kind, level, _, text)| (kind, level, text))
            .collect::<Vec<_>>(),
        vec![
            (ordered, 8, "너무 깊음".to_string()),
            (ordered, 8, "u32 밖".to_string()),
            (ordered, 0, "음수".to_string()),
            (ordered, 0, "숫자 아님".to_string()),
            (ordered, 1, "정의 없는 수준".to_string()),
            (Some(RichListKind::Bullet), 0, "없는 목록".to_string()),
        ]
    );
    // Without numbering.xml a list paragraph is a bullet at its own level.
    let read = decode_docx(&docx(&list_paragraph("2", "1", "정의 파일 없음"), None)).unwrap();
    assert_eq!(
        lists(&read),
        vec![(
            Some(RichListKind::Bullet),
            2,
            None,
            "정의 파일 없음".to_string()
        )]
    );
}

#[test]
fn a_numbering_part_with_too_many_definitions_is_rejected() {
    let mut numbering = String::new();
    for id in 0..9_000 {
        numbering.push_str(&format!(
            "<w:num w:numId=\"{id}\"><w:abstractNumId w:val=\"0\"/></w:num>"
        ));
    }
    let bytes = docx(&list_paragraph("0", "1", "x"), Some(&numbering));
    assert!(matches!(
        decode_docx(&bytes),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}
