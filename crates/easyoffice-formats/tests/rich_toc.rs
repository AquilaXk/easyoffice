//! Table-of-contents blocks of rich documents: the DOCX writer wraps them in a `w:sdt` "Table of
//! Contents" and the ODT writer in `text:table-of-content`; both readers give the title and the entry
//! levels back, and a heading is never mistaken for a TOC paragraph.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichBlock, RichDocument, RichParagraph, RichRun, RichToc,
    UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy,
};

fn para(text: &str, heading_level: u8, toc: Option<RichToc>) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain(text)],
        heading_level,
        toc,
        ..RichParagraph::default()
    })
}

fn sample() -> RichDocument {
    RichDocument {
        page_frame: Default::default(),
        title: "목차".to_string(),
        blocks: vec![
            para("목차", 0, Some(RichToc::Title)),
            para("1. 개요", 0, Some(RichToc::Entry(1))),
            para("세부", 0, Some(RichToc::Entry(3))),
            para("개요", 1, None),
            para("본문", 0, None),
            para("세부", 3, None),
        ],
        official_agency: None,
    }
}

fn read_back(bytes: &[u8], name: &str) -> Vec<(String, u8, Option<RichToc>)> {
    let DecodedContent::Document(doc) = UniversalFormatConverter::decode_rich(bytes, Some(name))
        .unwrap()
        .content
    else {
        panic!("document expected");
    };
    doc.blocks
        .iter()
        .map(|b| match b {
            RichBlock::Paragraph(p) => (p.plain_text(), p.heading_level, p.toc),
            _ => panic!("paragraph expected"),
        })
        .collect()
}

fn expected() -> Vec<(String, u8, Option<RichToc>)> {
    vec![
        ("목차".to_string(), 0, Some(RichToc::Title)),
        ("1. 개요".to_string(), 0, Some(RichToc::Entry(1))),
        ("세부".to_string(), 0, Some(RichToc::Entry(3))),
        ("개요".to_string(), 1, None),
        ("본문".to_string(), 0, None),
        ("세부".to_string(), 3, None),
    ]
}

fn part(bytes: &[u8], name: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let entry = entries.iter().find(|e| e.path == name).unwrap();
    String::from_utf8(entry.data.clone()).unwrap()
}

#[test]
fn docx_wraps_the_block_in_a_table_of_contents_container_and_reads_it_back() {
    let bytes =
        UniversalFormatConverter::encode_rich(&sample(), DocumentFormat::Docx, None).unwrap();
    let xml = part(&bytes, "word/document.xml");
    assert_eq!(xml.matches("<w:sdt>").count(), 1);
    assert!(xml.contains("w:docPartGallery w:val=\"Table of Contents\""));
    assert!(xml.contains("<w:pStyle w:val=\"TOC3\"/>"));
    assert!(part(&bytes, "word/styles.xml").contains("w:styleId=\"TOCHeading\""));
    assert_eq!(read_back(&bytes, "a.docx"), expected());
}

#[test]
fn odt_wraps_the_block_in_a_table_of_contents_container_and_reads_it_back() {
    let bytes =
        UniversalFormatConverter::encode_rich(&sample(), DocumentFormat::Odt, None).unwrap();
    let xml = part(&bytes, "content.xml");
    assert_eq!(xml.matches("<text:table-of-content ").count(), 1);
    assert!(xml.contains("text:style-name=\"Contents_20_3\""));
    assert_eq!(read_back(&bytes, "a.odt"), expected());
}

#[test]
fn documents_without_a_table_of_contents_carry_no_container_or_styles() {
    let plain = RichDocument {
        blocks: vec![para("본문", 0, None)],
        ..RichDocument::default()
    };
    let docx = UniversalFormatConverter::encode_rich(&plain, DocumentFormat::Docx, None).unwrap();
    assert!(!part(&docx, "word/document.xml").contains("<w:sdt>"));
    assert!(!part(&docx, "word/styles.xml").contains("TOCHeading"));
    let odt = UniversalFormatConverter::encode_rich(&plain, DocumentFormat::Odt, None).unwrap();
    assert!(!part(&odt, "content.xml").contains("table-of-content"));
}

fn linked(text: &str, toc: RichToc, target: &str) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun {
            link: Some(target.to_string()),
            ..RichRun::plain(text)
        }],
        toc: Some(toc),
        ..RichParagraph::default()
    })
}

fn heading(text: &str, level: u8, bookmark: u32) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain(text)],
        heading_level: level,
        bookmark: Some(bookmark),
        ..RichParagraph::default()
    })
}

fn with_targets() -> RichDocument {
    RichDocument {
        page_frame: Default::default(),
        title: "목차".to_string(),
        blocks: vec![
            para("목차", 0, Some(RichToc::Title)),
            linked("개요", RichToc::Entry(1), "#_Toc1"),
            linked("세부", RichToc::Entry(3), "#_Toc2"),
            heading("개요", 1, 1),
            para("본문", 0, None),
            heading("세부", 3, 2),
        ],
        official_agency: None,
    }
}

type Shape = (String, u8, Option<RichToc>, Option<u32>, Option<String>);

fn targets_back(bytes: &[u8], name: &str) -> Vec<Shape> {
    let DecodedContent::Document(doc) = UniversalFormatConverter::decode_rich(bytes, Some(name))
        .unwrap()
        .content
    else {
        panic!("document expected");
    };
    doc.blocks
        .iter()
        .map(|b| match b {
            RichBlock::Paragraph(p) => (
                p.plain_text(),
                p.heading_level,
                p.toc,
                p.bookmark,
                p.runs.first().and_then(|r| r.link.clone()),
            ),
            _ => panic!("paragraph expected"),
        })
        .collect()
}

fn expected_targets() -> Vec<Shape> {
    let some = |s: &str| Some(s.to_string());
    vec![
        ("목차".to_string(), 0, Some(RichToc::Title), None, None),
        (
            "개요".to_string(),
            0,
            Some(RichToc::Entry(1)),
            None,
            some("#_Toc1"),
        ),
        (
            "세부".to_string(),
            0,
            Some(RichToc::Entry(3)),
            None,
            some("#_Toc2"),
        ),
        ("개요".to_string(), 1, None, Some(1), None),
        ("본문".to_string(), 0, None, None, None),
        ("세부".to_string(), 3, None, Some(2), None),
    ]
}

#[test]
fn docx_writes_bookmarks_anchor_links_and_an_updatable_toc_field() {
    let bytes =
        UniversalFormatConverter::encode_rich(&with_targets(), DocumentFormat::Docx, None).unwrap();
    let xml = part(&bytes, "word/document.xml");
    assert!(xml.contains("<w:bookmarkStart w:id=\"2\" w:name=\"_Toc2\"/>"));
    assert_eq!(xml.matches("<w:bookmarkEnd ").count(), 2);
    assert!(xml.contains("<w:hyperlink w:anchor=\"_Toc1\" w:history=\"1\">"));
    assert!(xml.contains("TOC \\o \"1-3\" \\h"));
    assert_eq!(xml.matches("w:fldCharType=\"begin\"").count(), 1);
    assert_eq!(xml.matches("w:fldCharType=\"separate\"").count(), 1);
    assert_eq!(xml.matches("w:fldCharType=\"end\"").count(), 1);
    assert!(
        !part(&bytes, "word/_rels/document.xml.rels").contains("TargetMode=\"External\""),
        "a bookmark link is not an external relationship"
    );
    assert_eq!(targets_back(&bytes, "a.docx"), expected_targets());
}

#[test]
fn odt_writes_bookmarks_links_and_the_entry_templates_and_reads_them_back() {
    let bytes =
        UniversalFormatConverter::encode_rich(&with_targets(), DocumentFormat::Odt, None).unwrap();
    let xml = part(&bytes, "content.xml");
    assert!(xml.contains("<text:bookmark-start text:name=\"_Toc1\"/>"));
    assert!(xml.contains("xlink:href=\"#_Toc2\""));
    assert!(xml.contains("<text:table-of-content-source text:outline-level=\"3\">"));
    assert_eq!(
        xml.matches("<text:table-of-content-entry-template ")
            .count(),
        3
    );
    assert_eq!(targets_back(&bytes, "a.odt"), expected_targets());
}

#[test]
fn hwpx_leaves_a_bookmark_link_as_plain_text() {
    let bytes =
        UniversalFormatConverter::encode_rich(&with_targets(), DocumentFormat::Hwpx, None).unwrap();
    let all: String = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default())
        .unwrap()
        .iter()
        .filter(|e| e.path.ends_with(".xml"))
        .map(|e| String::from_utf8_lossy(&e.data).into_owned())
        .collect();
    assert!(!all.contains("#_Toc"));
}

#[test]
fn only_a_canonical_toc_bookmark_is_an_in_document_target() {
    use easyoffice_formats::safe_link_target;
    assert_eq!(safe_link_target("#_Toc12"), Some("#_Toc12"));
    assert_eq!(safe_link_target("#_Toc4294967295"), Some("#_Toc4294967295"));
    for bad in [
        "#",
        "#top",
        "#_Toc",
        "#_Toc0",
        "#_Toc012",
        "#_Toc4294967296",
        "#_Toc1 ",
        "#_Toc-1",
        "#_toc1",
        "#_Toc1#x",
        "_Toc1",
    ] {
        assert_eq!(
            safe_link_target(bad.trim_end()).filter(|_| bad != "#_Toc1 "),
            None,
            "{bad}"
        );
    }
}

fn with_pages(dots: bool) -> RichDocument {
    let mut doc = with_targets();
    for (block, page) in doc.blocks.iter_mut().skip(1).take(2).zip([3, 12]) {
        if let RichBlock::Paragraph(p) = block {
            p.toc_page = Some(page);
            p.toc_dots = dots;
        }
    }
    doc
}

/// `(text, toc page, dots, link)` of the TOC entries read back.
fn pages_back(bytes: &[u8], name: &str) -> Vec<(String, Option<u32>, bool, Option<String>)> {
    let DecodedContent::Document(doc) = UniversalFormatConverter::decode_rich(bytes, Some(name))
        .unwrap()
        .content
    else {
        panic!("document expected");
    };
    doc.blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) if matches!(p.toc, Some(RichToc::Entry(_))) => Some((
                p.plain_text(),
                p.toc_page,
                p.toc_dots,
                p.runs.first().and_then(|r| r.link.clone()),
            )),
            _ => None,
        })
        .collect()
}

fn expected_pages(dots: bool) -> Vec<(String, Option<u32>, bool, Option<String>)> {
    vec![
        (
            "개요".to_string(),
            Some(3),
            dots,
            Some("#_Toc1".to_string()),
        ),
        (
            "세부".to_string(),
            Some(12),
            dots,
            Some("#_Toc2".to_string()),
        ),
    ]
}

#[test]
fn docx_writes_a_right_tab_and_a_pageref_field_per_entry_and_reads_the_page_back() {
    for dots in [true, false] {
        let bytes =
            UniversalFormatConverter::encode_rich(&with_pages(dots), DocumentFormat::Docx, None)
                .unwrap();
        let xml = part(&bytes, "word/document.xml");
        let leader = if dots { "dot" } else { "none" };
        assert_eq!(
            xml.matches(&format!(
                "<w:tab w:val=\"right\" w:leader=\"{leader}\" w:pos=\"9026\"/>"
            ))
            .count(),
            2
        );
        assert!(xml.contains("PAGEREF _Toc2 \\h"));
        assert!(xml.contains("<w:t>12</w:t>"));
        assert!(
            xml.contains("TOC \\o \"1-3\" \\h </w:instrText>"),
            "pages shown: no \\n switch"
        );
        assert_eq!(
            xml.matches("w:fldCharType=\"begin\"").count(),
            3,
            "TOC and two PAGEREF"
        );
        assert_eq!(
            pages_back(&bytes, "a.docx"),
            expected_pages(dots),
            "{leader}"
        );
    }
    let bytes =
        UniversalFormatConverter::encode_rich(&with_targets(), DocumentFormat::Docx, None).unwrap();
    let xml = part(&bytes, "word/document.xml");
    assert!(
        xml.contains("\\h \\n </w:instrText>"),
        "no pages: the TOC field omits them"
    );
    assert!(!xml.contains("PAGEREF") && !xml.contains("<w:tabs>"));
}

#[test]
fn odt_writes_the_tab_stop_template_and_the_entry_page_and_reads_them_back() {
    for dots in [true, false] {
        let bytes =
            UniversalFormatConverter::encode_rich(&with_pages(dots), DocumentFormat::Odt, None)
                .unwrap();
        let xml = part(&bytes, "content.xml");
        let fill = if dots { "." } else { " " };
        assert!(xml.contains(&format!(
            "<text:index-entry-tab-stop style:type=\"right\" style:leader-char=\"{fill}\"/><text:index-entry-page-number/>"
        )));
        assert!(xml.contains("<text:tab/>12"));
        assert_eq!(
            xml.contains("<text:p text:style-name=\"Contents_20_1_20_Plain\">"),
            !dots
        );
        assert_eq!(pages_back(&bytes, "a.odt"), expected_pages(dots));
    }
    let bytes =
        UniversalFormatConverter::encode_rich(&with_targets(), DocumentFormat::Odt, None).unwrap();
    assert!(!part(&bytes, "content.xml").contains("index-entry-page-number"));
}

#[test]
fn a_number_after_a_tab_in_a_body_paragraph_stays_text() {
    let doc = RichDocument {
        blocks: vec![
            para("표\t3", 0, None),
            para("개요\t3", 0, Some(RichToc::Entry(1))),
        ],
        ..RichDocument::default()
    };
    let bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).unwrap();
    let DecodedContent::Document(back) =
        UniversalFormatConverter::decode_rich(&bytes, Some("a.docx"))
            .unwrap()
            .content
    else {
        panic!("document expected");
    };
    let RichBlock::Paragraph(body) = &back.blocks[0] else {
        panic!()
    };
    let RichBlock::Paragraph(entry) = &back.blocks[1] else {
        panic!()
    };
    assert_eq!(
        (body.plain_text(), body.toc_page),
        ("표\t3".to_string(), None)
    );
    assert_eq!(
        (entry.plain_text(), entry.toc_page),
        ("개요".to_string(), Some(3))
    );
}
