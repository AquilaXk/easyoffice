//! PD-367: the letter spacing of a run survives DOCX, ODT, RTF, HWPX and Pages, in the unit each
//! format keeps it in, and hostile values stay inside the model's bounds.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichBlock, RichDocument, RichParagraph, RichRun,
    UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy, MAX_LETTER_SPACING_TWIPS,
};

fn run(text: &str, size: Option<u16>, spacing: Option<i16>) -> RichRun {
    RichRun {
        text: text.to_string(),
        size_half_pt: size,
        letter_spacing_twips: spacing,
        ..RichRun::default()
    }
}

fn document(runs: Vec<RichRun>) -> RichDocument {
    RichDocument {
        title: "자간".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs,
            ..RichParagraph::default()
        })],
        ..RichDocument::default()
    }
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

fn spacings(doc: &RichDocument) -> Vec<(String, Option<i16>)> {
    doc.blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p),
            _ => None,
        })
        .flat_map(|p| p.runs.iter())
        .map(|r| (r.text.clone(), r.letter_spacing_twips))
        .collect()
}

fn part(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).expect("unpack");
    let entry = entries.iter().find(|e| e.path == path).expect("part");
    String::from_utf8(entry.data.clone()).expect("utf8")
}

fn sample() -> RichDocument {
    document(vec![
        run("넓게", Some(24), Some(60)),
        run("좁게", Some(24), Some(-24)),
        run("보통", Some(24), None),
        run("크기 없음", None, Some(40)),
    ])
}

#[test]
fn spacing_in_twips_roundtrips_in_docx_odt_and_rtf() {
    for (fmt, name) in [
        (DocumentFormat::Docx, "s.docx"),
        (DocumentFormat::Odt, "s.odt"),
        (DocumentFormat::Rtf, "s.rtf"),
    ] {
        let bytes = UniversalFormatConverter::encode_rich(&sample(), fmt, None).unwrap();
        let back = spacings(&decode(&bytes, name));
        let expected = [
            ("넓게", Some(60)),
            ("좁게", Some(-24)),
            ("보통", None),
            ("크기 없음", Some(40)),
        ];
        for (text, spacing) in expected {
            assert!(
                back.iter().any(|(t, s)| t == text && *s == spacing),
                "{fmt:?} {text}: {back:?}"
            );
        }
    }
}

#[test]
fn size_relative_formats_keep_the_spacing_at_the_run_size() {
    for (fmt, name) in [
        (DocumentFormat::Hwpx, "s.hwpx"),
        (DocumentFormat::Pages, "s.pages"),
    ] {
        let bytes = UniversalFormatConverter::encode_rich(&sample(), fmt, None).unwrap();
        let back = spacings(&decode(&bytes, name));
        for (text, spacing) in [("넓게", Some(60)), ("좁게", Some(-24)), ("보통", None)] {
            assert!(
                back.iter().any(|(t, s)| t == text && *s == spacing),
                "{fmt:?} {text}: {back:?}"
            );
        }
        let unsized_run = back.iter().find(|(t, _)| t == "크기 없음").unwrap();
        let twips = unsized_run.1.expect("kept");
        assert!((i32::from(twips) - 40).abs() <= 3, "{fmt:?}: {twips}");
    }
}

#[test]
fn each_format_writes_its_own_words() {
    let doc = document(vec![run("가", Some(24), Some(60))]);
    let docx = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).unwrap();
    assert!(
        part(&docx, "word/document.xml").contains("<w:spacing w:val=\"60\"/>"),
        "{}",
        part(&docx, "word/document.xml")
    );
    let odt = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Odt, None).unwrap();
    assert!(part(&odt, "content.xml").contains("fo:letter-spacing=\"3pt\""));
    let rtf = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Rtf, None).unwrap();
    assert!(String::from_utf8_lossy(&rtf).contains("\\expndtw60 "));
    let hwpx = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Hwpx, None).unwrap();
    assert!(part(&hwpx, "Contents/header.xml").contains("<hh:spacing hangul=\"25\""));
}

#[test]
fn pages_keeps_the_space_before_and_after_a_paragraph() {
    let doc = RichDocument {
        title: "간격".to_string(),
        blocks: vec![
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("앞뒤")],
                space_before_twips: Some(240),
                space_after_twips: Some(120),
                ..RichParagraph::default()
            }),
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("뒤만")],
                space_after_twips: Some(8000),
                ..RichParagraph::default()
            }),
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("없음")],
                ..RichParagraph::default()
            }),
        ],
        ..RichDocument::default()
    };
    let bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Pages, None).unwrap();
    let back = decode(&bytes, "space.pages");
    let spaces: Vec<(Option<u32>, Option<u32>)> = back
        .blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some((p.space_before_twips, p.space_after_twips)),
            _ => None,
        })
        .collect();
    assert_eq!(
        spaces,
        vec![(Some(240), Some(120)), (None, Some(8000)), (None, None)]
    );
    assert!(easyoffice_formats::pages_write_loss(&doc)
        .warnings()
        .is_empty());
}

fn docx_with(run_props: &str) -> Vec<u8> {
    let xml = format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>"#,
            r#"<w:p><w:r><w:rPr>{}</w:rPr><w:t>x</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"#,
        ),
        run_props
    );
    let bytes = UniversalFormatConverter::encode_rich(
        &document(vec![run("x", None, None)]),
        DocumentFormat::Docx,
        None,
    )
    .unwrap();
    let swapped: Vec<(String, Vec<u8>)> =
        ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default())
            .unwrap()
            .into_iter()
            .map(|e| {
                if e.path == "word/document.xml" {
                    (e.path, xml.clone().into_bytes())
                } else {
                    (e.path, e.data)
                }
            })
            .collect();
    ZipArchiveCodec::pack_compressed(&swapped).unwrap()
}

#[test]
fn hostile_values_are_clamped_or_dropped() {
    let max = MAX_LETTER_SPACING_TWIPS;
    for (props, expected) in [
        (r#"<w:spacing w:val="9223372036854775807"/>"#, Some(max)),
        (r#"<w:spacing w:val="-9223372036854775808"/>"#, Some(-max)),
        (r#"<w:spacing w:val="0"/>"#, None),
        (r#"<w:spacing w:val="abc"/>"#, None),
        (r#"<w:spacing/>"#, None),
        (r#"<w:spacing w:val="1.5"/>"#, None),
    ] {
        let doc = decode(&docx_with(props), "h.docx");
        assert_eq!(spacings(&doc), vec![("x".to_string(), expected)], "{props}");
    }
    for (words, expected) in [
        ("\\expndtw2147483647", Some(max)),
        ("\\expndtw-2147483647", Some(-max)),
        ("\\expnd8", Some(40)),
        ("\\expnd2147483647", Some(max)),
        ("\\expndtw0", None),
    ] {
        let rtf = format!("{{\\rtf1\\ansi\\pard {words} x\\par}}");
        let doc = decode(rtf.as_bytes(), "h.rtf");
        assert_eq!(spacings(&doc), vec![("x".to_string(), expected)], "{words}");
    }
}
