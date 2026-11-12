//! Page breaks of rich documents: a paragraph that starts a page survives DOCX and ODT saves, and the readers
//! turn the forms word processors write (`w:pageBreakBefore`, `w:br w:type="page"` before or after text, ODT
//! `fo:break-before="page"`) into that flag on the paragraph after the break.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichBlock, RichDocument, RichParagraph, RichRun,
    UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy,
};

fn para(text: &str, page_break_before: bool) -> RichBlock {
    RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain(text)],
        page_break_before,
        ..RichParagraph::default()
    })
}

fn sample() -> RichDocument {
    RichDocument {
        title: "쪽".to_string(),
        blocks: vec![
            para("첫 쪽", false),
            para("둘째 쪽", true),
            para("같은 쪽", false),
            para("셋째 쪽", true),
        ],
        official_agency: None,
    }
}

fn flags(bytes: &[u8], name: &str) -> Vec<(String, bool)> {
    let DecodedContent::Document(doc) = UniversalFormatConverter::decode_rich(bytes, Some(name))
        .unwrap()
        .content
    else {
        panic!("document expected");
    };
    doc.blocks
        .iter()
        .map(|b| match b {
            RichBlock::Paragraph(p) => (p.plain_text(), p.page_break_before),
            _ => panic!("paragraph expected"),
        })
        .collect()
}

fn expected() -> Vec<(String, bool)> {
    [
        ("첫 쪽", false),
        ("둘째 쪽", true),
        ("같은 쪽", false),
        ("셋째 쪽", true),
    ]
    .into_iter()
    .map(|(t, b)| (t.to_string(), b))
    .collect()
}

fn part(bytes: &[u8], name: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let entry = entries.iter().find(|e| e.path == name).unwrap();
    String::from_utf8(entry.data.clone()).unwrap()
}

/// The package with one part replaced.
fn with_part(bytes: &[u8], name: &str, xml: String) -> Vec<u8> {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).unwrap();
    let swapped: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            let data = if e.path == name {
                xml.clone().into_bytes()
            } else {
                e.data
            };
            (e.path, data)
        })
        .collect();
    ZipArchiveCodec::pack_compressed(&swapped).unwrap()
}

fn docx_with_body(body: &str) -> Vec<u8> {
    let base =
        UniversalFormatConverter::encode_rich(&sample(), DocumentFormat::Docx, None).unwrap();
    let xml = part(&base, "word/document.xml");
    let start = xml.find("<w:body>").unwrap() + "<w:body>".len();
    let end = xml.find("<w:sectPr>").unwrap();
    with_part(
        &base,
        "word/document.xml",
        format!("{}{}{}", &xml[..start], body, &xml[end..]),
    )
}

#[test]
fn docx_writes_page_break_before_and_reads_it_back() {
    let bytes =
        UniversalFormatConverter::encode_rich(&sample(), DocumentFormat::Docx, None).unwrap();
    let xml = part(&bytes, "word/document.xml");
    assert_eq!(xml.matches("<w:pageBreakBefore/>").count(), 2);
    assert_eq!(flags(&bytes, "a.docx"), expected());
}

#[test]
fn odt_writes_break_before_page_and_reads_it_back() {
    let bytes =
        UniversalFormatConverter::encode_rich(&sample(), DocumentFormat::Odt, None).unwrap();
    let xml = part(&bytes, "content.xml");
    assert!(xml.contains("fo:break-before=\"page\""), "{xml}");
    assert_eq!(flags(&bytes, "a.odt"), expected());
}

#[test]
fn docx_reads_the_page_break_forms_other_programs_write() {
    let body = concat!(
        "<w:p><w:r><w:t>첫 쪽</w:t></w:r></w:p>",
        // a paragraph that holds only the break: the next paragraph starts the page
        "<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>",
        "<w:p><w:r><w:t>둘째 쪽</w:t></w:r></w:p>",
        // a break after the text of a paragraph
        "<w:p><w:r><w:t>같은 쪽</w:t></w:r><w:r><w:br w:type=\"page\"/></w:r></w:p>",
        "<w:p><w:r><w:t>셋째 쪽</w:t></w:r></w:p>",
        // a break before the text of the same paragraph, and the property form
        "<w:p><w:r><w:br w:type=\"page\"/><w:t>넷째 쪽</w:t></w:r></w:p>",
        "<w:p><w:pPr><w:pageBreakBefore/></w:pPr><w:r><w:t>다섯째 쪽</w:t></w:r></w:p>",
        "<w:p><w:pPr><w:pageBreakBefore w:val=\"0\"/></w:pPr><w:r><w:t>같은 쪽 끝</w:t></w:r></w:p>",
    );
    let got = flags(&docx_with_body(body), "a.docx");
    let want: Vec<(String, bool)> = [
        ("첫 쪽", false),
        ("둘째 쪽", true),
        ("같은 쪽", false),
        ("셋째 쪽", true),
        ("넷째 쪽", true),
        ("다섯째 쪽", true),
        ("같은 쪽 끝", false),
    ]
    .into_iter()
    .map(|(t, b)| (t.to_string(), b))
    .collect();
    assert_eq!(got, want);
}

#[test]
fn odt_reads_break_before_from_automatic_and_inherited_styles() {
    let base = UniversalFormatConverter::encode_rich(&sample(), DocumentFormat::Odt, None).unwrap();
    let xml = part(&base, "content.xml");
    let start = xml.find("<office:automatic-styles>").unwrap();
    let end = xml.find("</office:body>").unwrap();
    let styles = concat!(
        "<office:automatic-styles>",
        "<style:style style:name=\"Brk\" style:family=\"paragraph\"><style:paragraph-properties fo:break-before=\"page\"/></style:style>",
        "<style:style style:name=\"Kid\" style:family=\"paragraph\" style:parent-style-name=\"Brk\"/>",
        "</office:automatic-styles>",
        "<office:body><office:text>",
        "<text:p>첫 쪽</text:p>",
        "<text:p text:style-name=\"Brk\"/>",
        "<text:p>둘째 쪽</text:p>",
        "<text:p text:style-name=\"Kid\">셋째 쪽</text:p>",
        "</office:text>",
    );
    let bytes = with_part(
        &base,
        "content.xml",
        format!("{}{}{}", &xml[..start], styles, &xml[end..]),
    );
    let got = flags(&bytes, "a.odt");
    assert_eq!(
        got.iter().map(|(_, b)| *b).collect::<Vec<_>>(),
        vec![false, true, true],
        "{got:?}"
    );
}

/// A heading that carries a table-of-contents bookmark and starts a page keeps both through a save.
fn bookmarked_page_start() -> RichDocument {
    let heading = |text: &str, bookmark: u32, page_break_before: bool| {
        RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain(text)],
            heading_level: 1,
            bookmark: Some(bookmark),
            page_break_before,
            ..RichParagraph::default()
        })
    };
    RichDocument {
        title: "쪽".to_string(),
        blocks: vec![
            heading("개요", 1, false),
            para("본문", false),
            heading("결론", 2, true),
        ],
        official_agency: None,
    }
}

#[test]
fn a_toc_bookmark_and_a_page_break_on_the_same_heading_survive_docx_and_odt() {
    for (format, name) in [
        (DocumentFormat::Docx, "a.docx"),
        (DocumentFormat::Odt, "a.odt"),
    ] {
        let bytes =
            UniversalFormatConverter::encode_rich(&bookmarked_page_start(), format, None).unwrap();
        let DecodedContent::Document(doc) =
            UniversalFormatConverter::decode_rich(&bytes, Some(name))
                .unwrap()
                .content
        else {
            panic!("document expected");
        };
        let got: Vec<(String, Option<u32>, bool)> = doc
            .blocks
            .iter()
            .map(|b| match b {
                RichBlock::Paragraph(p) => (p.plain_text(), p.bookmark, p.page_break_before),
                _ => panic!("paragraph expected"),
            })
            .collect();
        let want = vec![
            ("개요".to_string(), Some(1), false),
            ("본문".to_string(), None, false),
            ("결론".to_string(), Some(2), true),
        ];
        assert_eq!(got, want, "{name}");
    }
}
