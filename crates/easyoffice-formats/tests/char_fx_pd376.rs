//! PD-376: double strikethrough, capitals, hidden text, the underline style and colour and the emphasis mark of a run survive
//! DOCX, ODT and RTF in the words each format has, the formats without them say so, and hostile values stay inside the bounds.

use easyoffice_formats::{
    pages_write_loss, CharAdvanced, DecodedContent, DocumentFormat, RichBlock, RichDocument,
    RichParagraph, RichRun, TextFx, UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy,
};

fn run(text: &str, underline: bool, fx: TextFx) -> RichRun {
    RichRun {
        text: text.to_string(),
        underline,
        advanced: CharAdvanced {
            fx,
            ..CharAdvanced::default()
        },
        ..RichRun::default()
    }
}

fn document(runs: Vec<RichRun>) -> RichDocument {
    RichDocument {
        title: "효과".to_string(),
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

fn runs_of(doc: &RichDocument) -> Vec<RichRun> {
    doc.blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p),
            _ => None,
        })
        .flat_map(|p| p.runs.iter().cloned())
        .collect()
}

fn part(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).expect("unpack");
    let entry = entries.iter().find(|e| e.path == path).expect("part");
    String::from_utf8(entry.data.clone()).expect("utf8")
}

fn all_effects() -> TextFx {
    TextFx {
        double_strike: true,
        small_caps: true,
        all_caps: true,
        hidden: true,
        emphasis: 1,
        underline_style: 14,
        underline_color: Some(0xFF_0000),
    }
}

fn the_run(doc: &RichDocument, text: &str) -> RichRun {
    runs_of(doc)
        .into_iter()
        .find(|r| r.text == text)
        .unwrap_or_else(|| panic!("run {text}"))
}

#[test]
fn docx_keeps_every_effect_and_writes_a_double_line_instead_of_a_single_one() {
    let mut r = run("모두", true, all_effects());
    r.strike = true;
    let bytes =
        UniversalFormatConverter::encode_rich(&document(vec![r]), DocumentFormat::Docx, None)
            .unwrap();
    let xml = part(&bytes, "word/document.xml");
    for word in [
        "<w:caps/>",
        "<w:smallCaps/>",
        "<w:dstrike/>",
        "<w:vanish/>",
        "<w:u w:val=\"wave\" w:color=\"FF0000\"/>",
        "<w:em w:val=\"dot\"/>",
    ] {
        assert!(xml.contains(word), "{word} in {xml}");
    }
    assert!(
        !xml.contains("<w:strike/>"),
        "a double line replaces the single one"
    );
    let back = the_run(&decode(&bytes, "a.docx"), "모두");
    assert_eq!(back.advanced.fx, all_effects());
    assert!(back.underline);
}

#[test]
fn every_underline_style_survives_docx_odt_and_rtf() {
    for style in 0..=16u8 {
        let fx = TextFx {
            underline_style: style,
            ..TextFx::default()
        };
        for (fmt, name) in [
            (DocumentFormat::Docx, "a.docx"),
            (DocumentFormat::Odt, "a.odt"),
            (DocumentFormat::Rtf, "a.rtf"),
        ] {
            let bytes = UniversalFormatConverter::encode_rich(
                &document(vec![run("밑줄", true, fx)]),
                fmt,
                None,
            )
            .unwrap();
            let back = the_run(&decode(&bytes, name), "밑줄");
            assert!(back.underline, "{name} style {style}");
            assert_eq!(
                back.advanced.fx.underline_style, style,
                "{name} style {style}"
            );
        }
    }
}

#[test]
fn odt_keeps_every_effect_and_rtf_keeps_all_but_the_underline_colour_and_two_marks() {
    let bytes = UniversalFormatConverter::encode_rich(
        &document(vec![run("모두", true, all_effects())]),
        DocumentFormat::Odt,
        None,
    )
    .unwrap();
    let content = part(&bytes, "content.xml");
    for word in [
        "style:text-line-through-type=\"double\"",
        "fo:font-variant=\"small-caps\"",
        "fo:text-transform=\"uppercase\"",
        "text:display=\"none\"",
        "style:text-emphasize=\"dot above\"",
        "style:text-underline-style=\"wave\"",
        "style:text-underline-color=\"#FF0000\"",
    ] {
        assert!(content.contains(word), "{word} in {content}");
    }
    assert_eq!(
        the_run(&decode(&bytes, "a.odt"), "모두").advanced.fx,
        all_effects()
    );

    let rtf = UniversalFormatConverter::encode_rich(
        &document(vec![run("모두", true, all_effects())]),
        DocumentFormat::Rtf,
        None,
    )
    .unwrap();
    let back = the_run(&decode(&rtf, "a.rtf"), "모두").advanced.fx;
    assert_eq!(
        back,
        TextFx {
            underline_color: None,
            ..all_effects()
        },
        "RTF has no underline colour word here"
    );
    for emphasis in [3u8, 4] {
        let fx = TextFx {
            emphasis,
            ..TextFx::default()
        };
        let rtf = UniversalFormatConverter::encode_rich(
            &document(vec![run("강조", false, fx)]),
            DocumentFormat::Rtf,
            None,
        )
        .unwrap();
        assert_eq!(
            the_run(&decode(&rtf, "a.rtf"), "강조").advanced.fx.emphasis,
            0,
            "no RTF word for mark {emphasis}"
        );
    }
}

#[test]
fn hwpx_and_pages_drop_them_and_pages_says_so() {
    let doc = document(vec![
        run("모두", true, all_effects()),
        run("보통", false, TextFx::default()),
    ]);
    assert_eq!(pages_write_loss(&doc).formats, 1);
    for (fmt, name) in [
        (DocumentFormat::Hwpx, "a.hwpx"),
        (DocumentFormat::Pages, "a.pages"),
    ] {
        let bytes = UniversalFormatConverter::encode_rich(&doc, fmt, None).unwrap();
        let back = the_run(&decode(&bytes, name), "모두");
        // HWPX keeps the double line (`DOUBLE_SLIM`), the underline shape and colour and the mark (PD-379), and drops the
        // capitals and the hidden text; Pages drops all of it.
        let kept = if name == "a.hwpx" {
            TextFx {
                double_strike: true,
                emphasis: 1,
                underline_style: 14,
                underline_color: Some(0xFF_0000),
                ..TextFx::default()
            }
        } else {
            TextFx::default()
        };
        assert_eq!(back.advanced.fx, kept, "{name}");
    }
}

/// `docx` with the first run's `w:rPr` content replaced by `props`.
fn docx_with(props: &str) -> Vec<u8> {
    let bytes = UniversalFormatConverter::encode_rich(
        &document(vec![run("x", true, all_effects())]),
        DocumentFormat::Docx,
        None,
    )
    .unwrap();
    let xml = part(&bytes, "word/document.xml");
    let start = xml.find("<w:rPr>").unwrap();
    let end = xml.find("</w:rPr>").unwrap();
    let hostile = format!("{}<w:rPr>{props}{}", &xml[..start], &xml[end..]);
    let entries: Vec<(String, Vec<u8>)> =
        ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default())
            .unwrap()
            .into_iter()
            .map(|e| {
                if e.path == "word/document.xml" {
                    (e.path, hostile.clone().into_bytes())
                } else {
                    (e.path, e.data)
                }
            })
            .collect();
    ZipArchiveCodec::pack_compressed(&entries).unwrap()
}

#[test]
fn hostile_docx_values_read_as_not_set() {
    for (props, expected) in [
        (
            r#"<w:u w:val="nonsense" w:color="ZZZZZZ"/>"#,
            TextFx::default(),
        ),
        (
            r#"<w:u w:val="double" w:color="auto"/>"#,
            TextFx {
                underline_style: 2,
                ..TextFx::default()
            },
        ),
        (
            r#"<w:u w:val="single" w:color="1234567"/>"#,
            TextFx::default(),
        ),
        (
            r##"<w:u w:val="wavyDouble" w:color="#0000FF"/>"##,
            TextFx {
                underline_style: 16,
                underline_color: Some(0x00_00FF),
                ..TextFx::default()
            },
        ),
        (
            r#"<w:em w:val="sparkle"/><w:em w:val="99999999999999999999"/>"#,
            TextFx::default(),
        ),
        (
            r#"<w:caps w:val="0"/><w:smallCaps w:val="false"/><w:vanish w:val="off"/><w:dstrike w:val="0"/>"#,
            TextFx::default(),
        ),
        (
            r#"<w:caps/><w:smallCaps w:val="1"/><w:vanish w:val="true"/><w:dstrike/>"#,
            TextFx {
                double_strike: true,
                small_caps: true,
                all_caps: true,
                hidden: true,
                ..TextFx::default()
            },
        ),
    ] {
        let doc = decode(&docx_with(props), "h.docx");
        let back = the_run(&doc, "x");
        assert_eq!(back.advanced.fx, expected, "{props}");
    }
}

#[test]
fn hostile_rtf_and_odt_values_read_as_not_set() {
    let rtf = |body: &str| {
        format!("{{\\rtf1\\ansi\\deff0{{\\fonttbl{{\\f0 Arial;}}}}\\pard {body}\\par}}")
    };
    for (word, expected) in [
        ("\\striked99999999999999999999 x", true),
        ("\\striked0 x", false),
        ("\\striked x", true),
        ("\\strikedx x", false),
    ] {
        let doc = decode(rtf(word).as_bytes(), "h.rtf");
        let back = the_run(&doc, "x");
        assert_eq!(back.advanced.fx.double_strike, expected, "{word}");
    }
    let doc = decode(rtf("\\ululdbwave\\ulw\\ulnone x").as_bytes(), "h.rtf");
    assert!(!the_run(&doc, "x").underline, "\\ulnone turns the line off");

    let bytes = UniversalFormatConverter::encode_rich(
        &document(vec![run("x", true, all_effects())]),
        DocumentFormat::Odt,
        None,
    )
    .unwrap();
    let content = part(&bytes, "content.xml");
    let hostile = content
        .replace(
            "style:text-emphasize=\"dot above\"",
            "style:text-emphasize=\"sparkle\"",
        )
        .replace(
            "style:text-underline-style=\"wave\"",
            "style:text-underline-style=\"zigzag\"",
        )
        .replace(
            "style:text-underline-color=\"#FF0000\"",
            "style:text-underline-color=\"red\"",
        );
    let entries: Vec<(String, Vec<u8>)> =
        ZipArchiveCodec::unpack(&bytes, &ZipSecurityPolicy::default())
            .unwrap()
            .into_iter()
            .map(|e| {
                if e.path == "content.xml" {
                    (e.path, hostile.clone().into_bytes())
                } else {
                    (e.path, e.data)
                }
            })
            .collect();
    let back = the_run(
        &decode(
            &ZipArchiveCodec::pack_compressed(&entries).unwrap(),
            "h.odt",
        ),
        "x",
    )
    .advanced
    .fx;
    assert_eq!(
        (back.emphasis, back.underline_style, back.underline_color),
        (0, 0, None)
    );
}
