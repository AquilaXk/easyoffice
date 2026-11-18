//! A list paragraph in HWPX names the numbering or bullet definition that draws its marker
//! (`<hh:heading idRef>`, 1-based). The writer defines them, so a layout engine draws the marker
//! instead of leaving the item bare.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichBlock, RichDocument, RichListKind, RichListNumber,
    RichNumberEnclosure, RichNumberFormat, RichParagraph, RichRun, UniversalFormatConverter,
    ZipArchiveCodec, ZipSecurityPolicy,
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
        page_frame: Default::default(),
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

#[test]
fn hwpx_writes_and_roundtrips_list_levels_number_formats_and_separate_runs() {
    let nested = |kind, level, num: Option<(RichNumberFormat, RichNumberEnclosure)>, text: &str| {
        RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain(text)],
            list: Some(kind),
            list_level: level,
            list_number: num.map(|(format, enclosure)| RichListNumber { format, enclosure }),
            ..RichParagraph::default()
        })
    };
    let blocks = vec![
        nested(RichListKind::Bullet, 0, None, "점 0"),
        nested(RichListKind::Bullet, 1, None, "점 1"),
        nested(RichListKind::Bullet, 2, None, "점 2"),
        nested(RichListKind::Ordered, 0, None, "일"),
        nested(
            RichListKind::Ordered,
            1,
            Some((
                RichNumberFormat::LowerLatin,
                RichNumberEnclosure::RightParenthesis,
            )),
            "일-a",
        ),
        nested(
            RichListKind::Ordered,
            2,
            Some((
                RichNumberFormat::HangulSyllable,
                RichNumberEnclosure::Parentheses,
            )),
            "일-가",
        ),
        RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain("본문")],
            ..RichParagraph::default()
        }),
        nested(
            RichListKind::Ordered,
            0,
            Some((RichNumberFormat::UpperRoman, RichNumberEnclosure::Dot)),
            "새 목록 I",
        ),
    ];
    let doc = RichDocument {
        page_frame: Default::default(),
        title: "다단계".into(),
        blocks,
        official_agency: None,
    };
    let bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Hwpx, None).unwrap();
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
    assert!(header.contains("<hh:bullets itemCnt=\"3\">"));
    assert!(header.contains("char=\"◦\"") && header.contains("char=\"▪\""));
    assert!(header.contains("<hh:numberings itemCnt=\"2\">"));
    assert!(header.contains("level=\"2\" align=\"LEFT\" useInstWidth=\"1\" autoIndent=\"1\" widthAdjust=\"0\" textOffsetType=\"PERCENT\" textOffset=\"50\" numFormat=\"LATIN_SMALL\" charPrIDRef=\"4294967295\" checkable=\"0\">^2)</hh:paraHead>"));
    assert!(header.contains("numFormat=\"HANGUL_SYLLABLE\" charPrIDRef=\"4294967295\" checkable=\"0\">(^3)</hh:paraHead>"));
    assert!(header.contains("<hh:numbering id=\"2\" start=\"0\"><hh:paraHead start=\"1\" level=\"1\" align=\"LEFT\" useInstWidth=\"1\" autoIndent=\"1\" widthAdjust=\"0\" textOffsetType=\"PERCENT\" textOffset=\"50\" numFormat=\"ROMAN_CAPITAL\""));
    assert!(header.contains("<hh:heading type=\"NUMBER\" idRef=\"1\" level=\"1\"/>"));
    assert!(header.contains("<hh:heading type=\"NUMBER\" idRef=\"2\" level=\"0\"/>"));

    let decoded = UniversalFormatConverter::decode_rich(&bytes, Some("x.hwpx")).unwrap();
    let DecodedContent::Document(rt) = decoded.content else {
        panic!("document");
    };
    let got: Vec<_> = rt
        .blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => {
                Some((p.list, p.list_level, p.list_number, p.left_indent_twips))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        got,
        vec![
            (Some(RichListKind::Bullet), 0, None, None),
            (Some(RichListKind::Bullet), 1, None, None),
            (Some(RichListKind::Bullet), 2, None, None),
            (Some(RichListKind::Ordered), 0, None, None),
            (
                Some(RichListKind::Ordered),
                1,
                Some(RichListNumber {
                    format: RichNumberFormat::LowerLatin,
                    enclosure: RichNumberEnclosure::RightParenthesis,
                }),
                None,
            ),
            (
                Some(RichListKind::Ordered),
                2,
                Some(RichListNumber {
                    format: RichNumberFormat::HangulSyllable,
                    enclosure: RichNumberEnclosure::Parentheses,
                }),
                None,
            ),
            (None, 0, None, None),
            (
                Some(RichListKind::Ordered),
                0,
                Some(RichListNumber {
                    format: RichNumberFormat::UpperRoman,
                    enclosure: RichNumberEnclosure::Dot,
                }),
                None,
            ),
        ]
    );
}

#[test]
fn hwpx_header_with_excessive_numbering_heads_fails_closed() {
    use easyoffice_formats::EasyOfficeError;
    let doc = RichDocument {
        page_frame: Default::default(),
        title: "상한".into(),
        blocks: vec![item(RichListKind::Ordered, "일")],
        official_agency: None,
    };
    let bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Hwpx, None).unwrap();
    let mut entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let mut bloated = String::with_capacity(8_250 * 48);
    bloated.push_str("<hh:numberings itemCnt=\"821\">");
    for id in 1..=821u32 {
        bloated.push_str(&format!("<hh:numbering id=\"{id}\" start=\"0\">"));
        for lvl in 1..=10u8 {
            bloated.push_str(&format!(
                "<hh:paraHead level=\"{lvl}\" numFormat=\"DIGIT\">^{lvl}.</hh:paraHead>"
            ));
        }
        bloated.push_str("</hh:numbering>");
    }
    bloated.push_str("</hh:numberings>");
    for entry in &mut entries {
        if entry.path == "Contents/header.xml" {
            let xml = String::from_utf8(entry.data.clone()).unwrap();
            let start = xml.find("<hh:numberings ").unwrap();
            let end = xml.find("</hh:numberings>").unwrap() + "</hh:numberings>".len();
            let mut tampered = String::with_capacity(xml.len() + bloated.len());
            tampered.push_str(&xml[..start]);
            tampered.push_str(&bloated);
            tampered.push_str(&xml[end..]);
            entry.data = tampered.into_bytes();
        }
    }
    let pairs: Vec<(String, Vec<u8>)> = entries.into_iter().map(|e| (e.path, e.data)).collect();
    let repackaged = ZipArchiveCodec::pack(&pairs).unwrap();
    let err = UniversalFormatConverter::decode_rich(&repackaged, Some("tampered.hwpx"))
        .expect_err("must reject excessive numbering heads");
    assert!(
        matches!(err, EasyOfficeError::ResourceBudgetExceeded(_)),
        "expected ResourceBudgetExceeded, got {err:?}"
    );
}
