//! Hyperlinks of rich documents: DOCX, ODT and HWPX writers store a run's link and the readers give it
//! back; an address outside the allowed schemes is never written and never read.

use easyoffice_formats::{
    safe_link_target, DecodedContent, DocumentFormat, RichBlock, RichDocument, RichParagraph,
    RichRun, UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy,
};

const URL: &str = "https://example.com/a?b=1&c=2";

fn run(text: &str, link: Option<&str>, bold: bool) -> RichRun {
    RichRun {
        bold,
        link: link.map(str::to_string),
        ..RichRun::plain(text)
    }
}

fn doc(runs: Vec<RichRun>) -> RichDocument {
    RichDocument {
        page_frame: Default::default(),
        title: "링크".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs,
            ..RichParagraph::default()
        })],
        official_agency: None,
    }
}

fn round_trip(
    doc: &RichDocument,
    format: DocumentFormat,
    name: &str,
) -> Vec<(String, Option<String>)> {
    let bytes = UniversalFormatConverter::encode_rich(doc, format, None).unwrap();
    let DecodedContent::Document(back) = UniversalFormatConverter::decode_rich(&bytes, Some(name))
        .unwrap()
        .content
    else {
        panic!("document expected");
    };
    let RichBlock::Paragraph(p) = &back.blocks[0] else {
        panic!("paragraph expected");
    };
    p.runs
        .iter()
        .map(|r| (r.text.clone(), r.link.clone()))
        .collect()
}

fn sample() -> RichDocument {
    doc(vec![
        run("앞 ", None, false),
        run("굵은 ", Some(URL), true),
        run("링크", Some(URL), false),
        run(" 뒤 ", None, false),
        run("메일", Some("mailto:a@example.com"), false),
    ])
}

fn expected() -> Vec<(String, Option<String>)> {
    vec![
        ("앞 ".to_string(), None),
        ("굵은 ".to_string(), Some(URL.to_string())),
        ("링크".to_string(), Some(URL.to_string())),
        (" 뒤 ".to_string(), None),
        ("메일".to_string(), Some("mailto:a@example.com".to_string())),
    ]
}

#[test]
fn docx_keeps_links_and_the_bold_run_inside_one_hyperlink() {
    assert_eq!(
        round_trip(&sample(), DocumentFormat::Docx, "a.docx"),
        expected()
    );
}

#[test]
fn odt_keeps_links() {
    assert_eq!(
        round_trip(&sample(), DocumentFormat::Odt, "a.odt"),
        expected()
    );
}

#[test]
fn hwpx_keeps_links() {
    assert_eq!(
        round_trip(&sample(), DocumentFormat::Hwpx, "a.hwpx"),
        expected()
    );
}

#[test]
fn docx_writes_one_external_relationship_per_address() {
    let bytes =
        UniversalFormatConverter::encode_rich(&sample(), DocumentFormat::Docx, None).unwrap();
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let part = |path: &str| {
        let e = entries.iter().find(|e| e.path == path).unwrap();
        String::from_utf8(e.data.clone()).unwrap()
    };
    let rels = part("word/_rels/document.xml.rels");
    assert_eq!(rels.matches("TargetMode=\"External\"").count(), 2);
    assert!(rels.contains("Target=\"https://example.com/a?b=1&amp;c=2\""));
    assert_eq!(
        part("word/document.xml").matches("<w:hyperlink ").count(),
        2
    );
}

#[test]
fn an_address_outside_the_allowed_schemes_is_written_as_plain_text() {
    let unsafe_doc = doc(vec![
        run("a", Some("javascript:alert(1)"), false),
        run("b", Some("file:///etc/passwd"), false),
        run("c", Some("data:text/html,x"), false),
    ]);
    for (format, name) in [
        (DocumentFormat::Docx, "a.docx"),
        (DocumentFormat::Odt, "a.odt"),
        (DocumentFormat::Hwpx, "a.hwpx"),
    ] {
        let runs = round_trip(&unsafe_doc, format, name);
        assert!(
            runs.iter().all(|(_, link)| link.is_none()),
            "{name}: {runs:?}"
        );
        assert_eq!(
            runs.iter().map(|(t, _)| t.as_str()).collect::<String>(),
            "abc"
        );
    }
}

#[test]
fn the_address_policy_accepts_web_and_mail_targets_only() {
    for ok in [
        "http://a.b",
        "HTTPS://a.b/x?y#z",
        "mailto:me@x.y",
        "  https://a.b  ",
    ] {
        assert!(safe_link_target(ok).is_some(), "{ok}");
    }
    let too_long = format!("https://a.b/{}", "x".repeat(2048));
    for bad in [
        "",
        "javascript:alert(1)",
        "JaVaScRiPt:x",
        "file:///etc/passwd",
        "data:text/html,x",
        "ftp://a.b",
        "tel:123",
        "https://",
        "http:///x",
        "https:// a.b",
        "https://a.b/\u{7}",
        "mailto:",
        "mailto:?subject=x",
        "a.b",
        "//a.b",
        too_long.as_str(),
    ] {
        assert_eq!(safe_link_target(bad), None, "{bad:?}");
    }
}
