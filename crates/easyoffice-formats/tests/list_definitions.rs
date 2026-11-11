//! Lists are defined where the specifications define them (PD-172): DOCX has `word/numbering.xml`
//! (ECMA-376 17.9) and `w:numPr` in each list paragraph; ODT has `text:list-style` definitions and
//! nested `text:list` / `text:list-item` around the paragraphs (ODF 1.2 5.3). Before, a list was
//! only a paragraph style (DOCX) or an undefined style name (ODT), so no bullet or number showed.

use easyoffice_formats::{
    DocumentFormat, RichBlock, RichDocument, RichListKind, RichListNumber, RichNumberEnclosure,
    RichNumberFormat, RichParagraph, RichRun, UniversalFormatConverter, ZipArchiveCodec,
    ZipArchiveEntry, ZipSecurityPolicy,
};

fn item(text: &str, kind: RichListKind, level: u8, number: Option<RichListNumber>) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain(text)],
        list: Some(kind),
        list_level: level,
        list_number: number,
        ..RichParagraph::default()
    })
}

fn body(text: &str) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain(text)],
        ..RichParagraph::default()
    })
}

fn parenthesized_latin() -> RichListNumber {
    RichListNumber {
        format: RichNumberFormat::LowerLatin,
        enclosure: RichNumberEnclosure::RightParenthesis,
    }
}

/// bullet, ordered (1.) with a nested a) level, a body paragraph, then a second ordered list.
fn document() -> RichDocument {
    RichDocument {
        title: "목록".into(),
        blocks: vec![
            item("점 하나", RichListKind::Bullet, 0, None),
            item("점 둘", RichListKind::Bullet, 0, None),
            item("첫째", RichListKind::Ordered, 0, None),
            item(
                "첫째의 하위",
                RichListKind::Ordered,
                1,
                Some(parenthesized_latin()),
            ),
            item("둘째", RichListKind::Ordered, 0, None),
            body("본문"),
            item("다시 첫째", RichListKind::Ordered, 0, None),
        ],
        official_agency: None,
    }
}

fn entries(format: DocumentFormat) -> Vec<ZipArchiveEntry> {
    let bytes = UniversalFormatConverter::encode_rich(&document(), format, None).unwrap();
    ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap()
}

fn part<'a>(entries: &'a [ZipArchiveEntry], path: &str) -> &'a str {
    let entry = entries
        .iter()
        .find(|e| e.path == path)
        .unwrap_or_else(|| panic!("part missing: {path}"));
    std::str::from_utf8(&entry.data).unwrap()
}

/// The value of attribute `name` in every occurrence of `<tag ...>`.
fn attrs(xml: &str, tag: &str, name: &str) -> Vec<String> {
    let open = format!("<{tag} ");
    let key = format!("{name}=\"");
    xml.match_indices(&open)
        .map(|(at, _)| {
            let rest = &xml[at..xml[at..].find('>').map_or(xml.len(), |end| at + end)];
            let start = rest
                .find(&key)
                .unwrap_or_else(|| panic!("{tag} has no {name}"))
                + key.len();
            rest[start..start + rest[start..].find('"').unwrap()].to_string()
        })
        .collect()
}

/// `(numId, ilvl)` of each `w:p` with a `w:numPr`, in order.
fn paragraph_lists(document: &str) -> Vec<Option<(String, String)>> {
    document
        .split("<w:p>")
        .skip(1)
        .map(|p| {
            let p = &p[..p.find("</w:p>").unwrap()];
            p.contains("<w:numPr>").then(|| {
                (
                    attrs(p, "w:numId", "w:val").remove(0),
                    attrs(p, "w:ilvl", "w:val").remove(0),
                )
            })
        })
        .collect()
}

#[test]
fn docx_defines_numbering_and_every_list_paragraph_refers_to_it() {
    let parts = entries(DocumentFormat::Docx);
    let numbering = part(&parts, "word/numbering.xml");
    assert!(part(&parts, "[Content_Types].xml").contains(
        "PartName=\"/word/numbering.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml\""
    ));
    let rels = part(&parts, "word/_rels/document.xml.rels");
    assert!(rels.contains("relationships/numbering\" Target=\"numbering.xml\""));

    let lists = paragraph_lists(part(&parts, "word/document.xml"));
    let refs: Vec<_> = lists
        .iter()
        .map(|l| l.as_ref().map(|(n, l)| (n.as_str(), l.as_str())))
        .collect();
    // Three runs: the bullets (1), the first ordered list (2), the ordered list after the body (3).
    assert_eq!(
        refs,
        [
            Some(("1", "0")),
            Some(("1", "0")),
            Some(("2", "0")),
            Some(("2", "1")),
            Some(("2", "0")),
            None,
            Some(("3", "0")),
        ]
    );

    // Every numId resolves to an abstractNum through w:num (schema order: abstractNum*, num*).
    assert!(numbering.find("<w:abstractNum ").unwrap() < numbering.find("<w:num ").unwrap());
    let abstract_of = |num: &str| {
        let at = numbering
            .find(&format!("<w:num w:numId=\"{num}\""))
            .unwrap();
        attrs(&numbering[at..], "w:abstractNumId", "w:val").remove(0)
    };
    let level_of = |abstract_id: &str, ilvl: usize| {
        let at = numbering
            .find(&format!("<w:abstractNum w:abstractNumId=\"{abstract_id}\""))
            .unwrap();
        let end = at + numbering[at..].find("</w:abstractNum>").unwrap();
        let block = &numbering[at..end];
        let lvl = block.split("<w:lvl ").nth(ilvl + 1).unwrap();
        let first = |tag: &str| {
            attrs(
                &format!("<{tag} {}", lvl.split(&format!("<{tag} ")).nth(1).unwrap()),
                tag,
                "w:val",
            )
            .remove(0)
        };
        (first("w:numFmt"), first("w:lvlText"))
    };
    let bullet = abstract_of("1");
    let ordered = abstract_of("2");
    assert_eq!(
        level_of(&bullet, 0),
        ("bullet".to_string(), "\u{2022}".to_string())
    );
    assert_eq!(
        level_of(&ordered, 0),
        ("decimal".to_string(), "%1.".to_string())
    );
    assert_eq!(
        level_of(&ordered, 1),
        ("lowerLetter".to_string(), "%2)".to_string())
    );
    // The list after the body paragraph has no nested level of its own: its own definition, and it
    // restarts at 1.
    let third = abstract_of("3");
    assert_ne!(third, ordered);
    assert_eq!(
        level_of(&third, 0),
        ("decimal".to_string(), "%1.".to_string())
    );
    let second = &numbering[numbering.find("<w:num w:numId=\"3\"").unwrap()..];
    assert!(second.contains("<w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"1\"/>"));
}

#[test]
fn docx_without_lists_has_no_numbering_part() {
    let doc = RichDocument {
        title: "본문".into(),
        blocks: vec![body("문단")],
        official_agency: None,
    };
    let bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).unwrap();
    let parts = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    assert!(parts.iter().all(|e| e.path != "word/numbering.xml"));
    assert!(!part(&parts, "[Content_Types].xml").contains("numbering"));
}

#[test]
fn odt_defines_list_styles_and_nests_paragraphs_in_list_items() {
    let parts = entries(DocumentFormat::Odt);
    let content = part(&parts, "content.xml");
    // Three list styles: bullets, ordered 1. with a) at level 2, and plain ordered 1.
    let styles = attrs(content, "text:list-style", "style:name");
    assert_eq!(styles, ["L1", "L2", "L3"]);
    assert!(content
        .contains("<text:list-level-style-bullet text:level=\"1\" text:bullet-char=\"\u{2022}\">"));
    assert!(content.contains("<text:list-level-style-number text:level=\"1\" style:num-suffix=\".\" style:num-format=\"1\">"));
    assert!(content.contains("<text:list-level-style-number text:level=\"2\" style:num-suffix=\")\" style:num-format=\"a\">"));

    // Three top-level lists refer to them; the nested item is a list inside the "첫째" item.
    assert_eq!(
        attrs(content, "text:list", "text:style-name"),
        ["L1", "L2", "L3"]
    );
    assert!(content.contains(
        "<text:list-item><text:p>첫째</text:p><text:list><text:list-item><text:p>첫째의 하위</text:p></text:list-item></text:list></text:list-item><text:list-item><text:p>둘째</text:p></text:list-item></text:list><text:p>본문</text:p>"
    ));
    // Balanced elements.
    for tag in ["text:list", "text:list-item"] {
        let open = content.matches(&format!("<{tag}>")).count()
            + content.matches(&format!("<{tag} ")).count();
        assert_eq!(open, content.matches(&format!("</{tag}>")).count(), "{tag}");
    }
    // No paragraph style points at an undefined list style any more.
    assert!(!content.contains("style:list-style-name"));
}

#[test]
fn lists_survive_a_round_trip_through_the_engine_reader() {
    for format in [DocumentFormat::Docx, DocumentFormat::Odt] {
        let bytes = UniversalFormatConverter::encode_rich(&document(), format, None).unwrap();
        let read = UniversalFormatConverter::decode_rich(&bytes, Some(format.extension())).unwrap();
        let easyoffice_formats::DecodedContent::Document(doc) = read.content else {
            panic!("document");
        };
        let kinds: Vec<_> = doc
            .blocks
            .iter()
            .filter_map(|b| match b {
                RichBlock::Paragraph(p) => Some(p.list),
                _ => None,
            })
            .collect();
        let bullet = Some(RichListKind::Bullet);
        let ordered = Some(RichListKind::Ordered);
        assert_eq!(
            kinds,
            [bullet, bullet, ordered, ordered, ordered, None, ordered],
            "{format:?}"
        );
    }
}
