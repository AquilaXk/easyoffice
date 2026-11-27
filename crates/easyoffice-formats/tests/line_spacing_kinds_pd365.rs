//! PD-365: the four line spacing kinds (multiple, at least, exact, between lines) survive
//! DOCX, ODT, RTF, HWPX and Pages; formats without a between-lines rule write the equal
//! multiple; hostile values stay inside the model's bounds.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichBlock, RichDocument, RichLineSpacing, RichParagraph,
    RichRun, UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy, MAX_LINE_PERCENT,
    MAX_LINE_SPACING_TWIPS,
};

const RULES: [RichLineSpacing; 6] = [
    RichLineSpacing::Multiple(150),
    RichLineSpacing::Multiple(1000),
    RichLineSpacing::AtLeast(360),
    RichLineSpacing::Exact(300),
    RichLineSpacing::Between(120),
    RichLineSpacing::Between(0),
];

fn document(spacings: &[Option<RichLineSpacing>]) -> RichDocument {
    RichDocument {
        title: "줄 간격".to_string(),
        blocks: spacings
            .iter()
            .enumerate()
            .map(|(i, spacing)| {
                RichBlock::Paragraph(RichParagraph {
                    runs: vec![RichRun::plain(format!("문단 {i}"))],
                    line_spacing: *spacing,
                    ..RichParagraph::default()
                })
            })
            .collect(),
        ..RichDocument::default()
    }
}

fn spacings_of(doc: &RichDocument) -> Vec<Option<RichLineSpacing>> {
    doc.blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p.line_spacing),
            _ => None,
        })
        .collect()
}

fn decode(bytes: &[u8], name: &str) -> RichDocument {
    match UniversalFormatConverter::decode_rich(bytes, Some(name))
        .expect("decode")
        .content
    {
        DecodedContent::Document(d) => d,
        other => panic!("expected document: {other:?}"),
    }
}

fn part(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).expect("unpack");
    let entry = entries
        .iter()
        .find(|e| e.path == path)
        .unwrap_or_else(|| panic!("missing part {path}"));
    String::from_utf8(entry.data.clone()).expect("utf8")
}

/// The multiple a format without the between-lines rule writes in its place.
fn without_between(spacing: RichLineSpacing) -> RichLineSpacing {
    match spacing {
        RichLineSpacing::Between(_) => RichLineSpacing::Multiple(spacing.multiple_percent()),
        other => other,
    }
}

#[test]
fn every_rule_roundtrips_in_the_formats_that_have_it() {
    let input: Vec<Option<RichLineSpacing>> = RULES.iter().copied().map(Some).collect();
    let doc = document(&input);
    for (fmt, name) in [
        (DocumentFormat::Odt, "kinds.odt"),
        (DocumentFormat::Hwpx, "kinds.hwpx"),
        (DocumentFormat::Pages, "kinds.pages"),
    ] {
        let bytes = UniversalFormatConverter::encode_rich(&doc, fmt, None).expect("encode");
        let back = decode(&bytes, name);
        assert_eq!(spacings_of(&back), input, "{fmt:?}");
    }
}

#[test]
fn docx_and_rtf_keep_the_length_rules_and_write_between_lines_as_its_multiple() {
    let input: Vec<Option<RichLineSpacing>> = RULES.iter().copied().map(Some).collect();
    let doc = document(&input);
    let expected: Vec<Option<RichLineSpacing>> = RULES
        .iter()
        .copied()
        .map(|s| Some(without_between(s)))
        .collect();
    for (fmt, name) in [
        (DocumentFormat::Docx, "kinds.docx"),
        (DocumentFormat::Rtf, "kinds.rtf"),
    ] {
        let bytes = UniversalFormatConverter::encode_rich(&doc, fmt, None).expect("encode");
        assert_eq!(spacings_of(&decode(&bytes, name)), expected, "{fmt:?}");
    }
    assert_eq!(
        without_between(RichLineSpacing::Between(120)),
        RichLineSpacing::Multiple(150)
    );
    assert_eq!(
        without_between(RichLineSpacing::Between(0)),
        RichLineSpacing::Multiple(100)
    );
}

#[test]
fn each_format_writes_its_own_rule_words() {
    let doc = document(&[
        Some(RichLineSpacing::AtLeast(360)),
        Some(RichLineSpacing::Exact(300)),
        Some(RichLineSpacing::Between(120)),
    ]);
    let docx = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).unwrap();
    let xml = part(&docx, "word/document.xml");
    assert!(
        xml.contains("w:line=\"360\" w:lineRule=\"atLeast\""),
        "{xml}"
    );
    assert!(xml.contains("w:line=\"300\" w:lineRule=\"exact\""), "{xml}");
    assert!(xml.contains("w:line=\"360\" w:lineRule=\"auto\""), "{xml}");

    let odt = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Odt, None).unwrap();
    let content = part(&odt, "content.xml");
    assert!(
        content.contains("style:line-height-at-least=\"18pt\""),
        "{content}"
    );
    assert!(content.contains("fo:line-height=\"15pt\""), "{content}");
    assert!(content.contains("style:line-spacing=\"6pt\""), "{content}");

    let rtf = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Rtf, None).unwrap();
    let rtf = String::from_utf8_lossy(&rtf);
    assert!(rtf.contains("\\sl360\\slmult0"), "{rtf}");
    assert!(rtf.contains("\\sl-300\\slmult0"), "{rtf}");
    assert!(rtf.contains("\\sl360\\slmult1"), "{rtf}");

    let hwpx = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Hwpx, None).unwrap();
    let header = part(&hwpx, "Contents/header.xml");
    assert!(
        header.contains("<hh:lineSpacing type=\"AT_LEAST\" value=\"1800\""),
        "{header}"
    );
    assert!(
        header.contains("<hh:lineSpacing type=\"FIXED\" value=\"1500\""),
        "{header}"
    );
    assert!(
        header.contains("<hh:lineSpacing type=\"BETWEEN_LINES\" value=\"600\""),
        "{header}"
    );
}

fn docx_with(spacing_attrs: &str) -> Vec<u8> {
    let document = format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>"#,
            r#"<w:p><w:pPr><w:spacing {}/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>"#,
            r#"<w:sectPr/></w:body></w:document>"#,
        ),
        spacing_attrs
    );
    let doc = RichDocument {
        title: "x".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain("x")],
            ..RichParagraph::default()
        })],
        ..RichDocument::default()
    };
    let bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).unwrap();
    let entries = ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default()).unwrap();
    let swapped: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|e| {
            if e.path == "word/document.xml" {
                (e.path, document.clone().into_bytes())
            } else {
                (e.path, e.data)
            }
        })
        .collect();
    ZipArchiveCodec::pack_compressed(&swapped).unwrap()
}

#[test]
fn hostile_docx_values_stay_inside_the_bounds_or_drop() {
    let cases = [
        (
            r#"w:line="9223372036854775807" w:lineRule="atLeast""#,
            Some(RichLineSpacing::AtLeast(MAX_LINE_SPACING_TWIPS)),
        ),
        (
            r#"w:line="9223372036854775807" w:lineRule="exact""#,
            Some(RichLineSpacing::Exact(MAX_LINE_SPACING_TWIPS)),
        ),
        (r#"w:line="9223372036854775807" w:lineRule="auto""#, None),
        (r#"w:line="-360" w:lineRule="exact""#, None),
        (r#"w:line="0" w:lineRule="atLeast""#, None),
        (r#"w:line="360" w:lineRule="between""#, None),
        (r#"w:line="abc" w:lineRule="exact""#, None),
        (
            r#"w:line="2400" w:lineRule="auto""#,
            Some(RichLineSpacing::Multiple(MAX_LINE_PERCENT)),
        ),
        (r#"w:line="2410" w:lineRule="auto""#, None),
    ];
    for (attrs, expected) in cases {
        let doc = decode(&docx_with(attrs), "hostile.docx");
        assert_eq!(spacings_of(&doc), vec![expected], "{attrs}");
    }
}

fn odt_with(properties: &str) -> Vec<u8> {
    let content = format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            r#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" "#,
            r#"xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" "#,
            r#"xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" "#,
            r#"xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" office:version="1.2">"#,
            r#"<office:automatic-styles><style:style style:name="P1" style:family="paragraph">"#,
            r#"<style:paragraph-properties {}/></style:style></office:automatic-styles>"#,
            r#"<office:body><office:text><text:p text:style-name="P1">x</text:p></office:text></office:body>"#,
            r#"</office:document-content>"#,
        ),
        properties
    );
    let entries = vec![
        (
            "mimetype".to_string(),
            b"application/vnd.oasis.opendocument.text".to_vec(),
        ),
        ("content.xml".to_string(), content.into_bytes()),
    ];
    ZipArchiveCodec::pack_compressed(&entries).unwrap()
}

#[test]
fn hostile_odt_values_stay_inside_the_bounds_or_drop() {
    let cases = [
        (
            r#"style:line-height-at-least="10in""#,
            Some(RichLineSpacing::AtLeast(14400)),
        ),
        (
            r#"style:line-spacing="6pt""#,
            Some(RichLineSpacing::Between(120)),
        ),
        (
            r#"style:line-spacing="0pt""#,
            Some(RichLineSpacing::Between(0)),
        ),
        (r#"style:line-spacing="-6pt""#, None),
        (r#"style:line-height-at-least="-6pt""#, None),
        (r#"style:line-height-at-least="99999999in""#, None),
        (r#"fo:line-height="NaN%""#, None),
        (r#"fo:line-height="inf%""#, None),
        (r#"fo:line-height="1e400%""#, None),
        (r#"fo:line-height="normal""#, None),
        (r#"fo:line-height="0pt""#, None),
        (
            r#"fo:line-height="150%" style:line-spacing="6pt""#,
            Some(RichLineSpacing::Multiple(150)),
        ),
    ];
    for (props, expected) in cases {
        let doc = decode(&odt_with(props), "hostile.odt");
        assert_eq!(spacings_of(&doc), vec![expected], "{props}");
    }
}

#[test]
fn hostile_rtf_values_stay_inside_the_bounds_or_drop() {
    let cases = [
        ("\\sl360\\slmult0", Some(RichLineSpacing::AtLeast(360))),
        ("\\sl-360\\slmult0", Some(RichLineSpacing::Exact(360))),
        ("\\sl-360", Some(RichLineSpacing::Exact(360))),
        ("\\sl360", Some(RichLineSpacing::AtLeast(360))),
        ("\\sl360\\slmult1", Some(RichLineSpacing::Multiple(150))),
        ("\\sl0\\slmult0", None),
        ("\\sl-360\\slmult1", None),
        (
            "\\sl2147483647\\slmult0",
            Some(RichLineSpacing::AtLeast(MAX_LINE_SPACING_TWIPS)),
        ),
        (
            "\\sl-2147483647\\slmult0",
            Some(RichLineSpacing::Exact(MAX_LINE_SPACING_TWIPS)),
        ),
        ("\\sl2147483647\\slmult1", None),
    ];
    for (words, expected) in cases {
        let rtf = format!("{{\\rtf1\\ansi\\pard{words} 줄\\par}}");
        let doc = decode(rtf.as_bytes(), "hostile.rtf");
        assert_eq!(spacings_of(&doc), vec![expected], "{words}");
    }
}
