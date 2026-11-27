//! PD-376: a run keeps a Latin font and a separate East Asian font through DOCX, ODT, RTF and HWPX; a document with one font keeps
//! its single value (opening and saving it again writes the same bytes of the font words), and Pages says it cannot keep the pair.

use easyoffice_formats::{
    pages_write_loss, DecodedContent, DocumentFormat, RichBlock, RichDocument, RichParagraph,
    RichRun, UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy,
};

fn run(text: &str, font: Option<&str>, east: Option<&str>) -> RichRun {
    RichRun {
        text: text.to_string(),
        font: font.map(str::to_string),
        font_east_asia: east.map(str::to_string),
        ..RichRun::default()
    }
}

fn document(runs: Vec<RichRun>) -> RichDocument {
    RichDocument {
        title: "글꼴".to_string(),
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

fn fonts_of(doc: &RichDocument, text: &str) -> (Option<String>, Option<String>) {
    let run = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p),
            _ => None,
        })
        .flat_map(|p| p.runs.iter())
        .find(|r| r.text == text)
        .unwrap_or_else(|| panic!("run {text}"));
    (run.font.clone(), run.font_east_asia.clone())
}

fn part(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).expect("unpack");
    let entry = entries.iter().find(|e| e.path == path).expect("part");
    String::from_utf8(entry.data.clone()).expect("utf8")
}

fn sample() -> RichDocument {
    document(vec![
        run("둘", Some("Calibri"), Some("맑은 고딕")),
        run("하나", Some("Arial"), None),
        run("한글만", None, Some("바탕")),
    ])
}

#[test]
fn every_format_keeps_the_pair_and_the_single_font() {
    for (fmt, name) in [
        (DocumentFormat::Docx, "a.docx"),
        (DocumentFormat::Odt, "a.odt"),
        (DocumentFormat::Rtf, "a.rtf"),
        (DocumentFormat::Hwpx, "a.hwpx"),
    ] {
        let bytes = UniversalFormatConverter::encode_rich(&sample(), fmt, None).unwrap();
        let back = decode(&bytes, name);
        assert_eq!(
            fonts_of(&back, "둘"),
            (Some("Calibri".into()), Some("맑은 고딕".into())),
            "{name}"
        );
        assert_eq!(
            fonts_of(&back, "하나"),
            (Some("Arial".into()), None),
            "{name}: one font stays one font"
        );
    }
}

#[test]
fn a_run_with_only_an_east_asian_font_keeps_it_in_docx_odt_and_rtf() {
    // HWPX and RTF always name a Latin face in their tables, so a face for the East Asian text alone reads back as the one font.
    for (fmt, name) in [
        (DocumentFormat::Docx, "a.docx"),
        (DocumentFormat::Odt, "a.odt"),
        (DocumentFormat::Rtf, "a.rtf"),
    ] {
        let bytes = UniversalFormatConverter::encode_rich(&sample(), fmt, None).unwrap();
        let (font, east) = fonts_of(&decode(&bytes, name), "한글만");
        assert_eq!(font.or(east), Some("바탕".to_string()), "{name}");
    }
}

#[test]
fn the_words_of_each_format() {
    let docx =
        UniversalFormatConverter::encode_rich(&sample(), DocumentFormat::Docx, None).unwrap();
    let xml = part(&docx, "word/document.xml");
    assert!(xml.contains("<w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:eastAsia=\"맑은 고딕\" w:cs=\"Calibri\"/>"), "{xml}");
    assert!(
        xml.contains(
            "<w:rFonts w:ascii=\"Arial\" w:hAnsi=\"Arial\" w:eastAsia=\"Arial\" w:cs=\"Arial\"/>"
        ),
        "one font writes all four as before"
    );
    assert!(xml.contains("<w:rFonts w:eastAsia=\"바탕\"/>"));
    let odt = UniversalFormatConverter::encode_rich(&sample(), DocumentFormat::Odt, None).unwrap();
    let content = part(&odt, "content.xml");
    assert!(
        content.contains("style:font-name=\"Calibri\"")
            && content.contains("style:font-name-asian=\"맑은 고딕\""),
        "{content}"
    );
    assert!(
        content.contains("style:font-face style:name=\"맑은 고딕\""),
        "the East Asian face is declared"
    );
    let rtf = String::from_utf8(
        UniversalFormatConverter::encode_rich(&sample(), DocumentFormat::Rtf, None).unwrap(),
    )
    .unwrap();
    assert!(rtf.contains("\\af"), "{rtf}");
    let hwpx =
        UniversalFormatConverter::encode_rich(&sample(), DocumentFormat::Hwpx, None).unwrap();
    let header = part(&hwpx, "Contents/header.xml");
    assert!(header.contains("hangul=\"2\" latin=\"1\""), "{header}");
}

#[test]
fn one_font_opens_and_saves_to_the_same_font_words() {
    let one = document(vec![run("하나", Some("Arial"), None)]);
    for (fmt, name, path) in [
        (DocumentFormat::Docx, "a.docx", "word/document.xml"),
        (DocumentFormat::Odt, "a.odt", "content.xml"),
        (DocumentFormat::Hwpx, "a.hwpx", "Contents/header.xml"),
    ] {
        let first = UniversalFormatConverter::encode_rich(&one, fmt, None).unwrap();
        let second =
            UniversalFormatConverter::encode_rich(&decode(&first, name), fmt, None).unwrap();
        assert_eq!(part(&first, path), part(&second, path), "{name}");
    }
}

#[test]
fn pages_cannot_keep_the_pair_and_the_save_says_so() {
    let loss = pages_write_loss(&sample());
    assert_eq!(loss.formats, 2, "the pair and the East Asian-only run");
    assert_eq!(
        pages_write_loss(&document(vec![run("하나", Some("Arial"), None)])).formats,
        0
    );
}

/// `docx` with the first run's `w:rPr` content replaced by `props`.
fn docx_with(props: &str) -> Vec<u8> {
    let bytes = UniversalFormatConverter::encode_rich(
        &document(vec![run("x", Some("Calibri"), None)]),
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
fn docx_font_attributes_read_as_a_pair_a_single_font_or_nothing() {
    for (props, expected) in [
        (
            r#"<w:rFonts w:ascii="A" w:eastAsia="B"/>"#,
            (Some("A"), Some("B")),
        ),
        (
            r#"<w:rFonts w:ascii="A" w:hAnsi="A" w:eastAsia="A" w:cs="A"/>"#,
            (Some("A"), None),
        ),
        (r#"<w:rFonts w:eastAsia="B"/>"#, (Some("B"), None)),
        (r#"<w:rFonts w:hAnsi="A"/>"#, (Some("A"), None)),
        (
            r#"<w:rFonts w:cs="C" w:eastAsia="B"/>"#,
            (Some("C"), Some("B")),
        ),
        (r#"<w:rFonts w:ascii="" w:eastAsia="  "/>"#, (None, None)),
        (
            r#"<w:rFonts w:asciiTheme="minorHAnsi" w:eastAsiaTheme="minorEastAsia"/>"#,
            (None, None),
        ),
        (r#"<w:rFonts/>"#, (None, None)),
    ] {
        let doc = decode(&docx_with(props), "h.docx");
        let (font, east) = fonts_of(&doc, "x");
        assert_eq!((font.as_deref(), east.as_deref()), expected, "{props}");
    }
}

#[test]
fn hostile_rtf_and_odt_font_words_read_as_not_set() {
    let rtf = |body: &str| {
        format!(
            "{{\\rtf1\\ansi\\deff0{{\\fonttbl{{\\f0 Arial;}}{{\\f1 Batang;}}}}\\pard {body}\\par}}"
        )
    };
    for (body, expected) in [
        // The default face is a name too once the other face is not it: the pair is kept.
        ("\\f1\\af0 x", (Some("Batang"), Some("Arial"))),
        ("\\f0\\af1 x", (Some("Arial"), Some("Batang"))),
        ("\\f0\\af0 x", (None, None)),
        ("\\f0 x", (None, None)),
        ("\\f1\\af1 x", (Some("Batang"), None)),
        ("\\f1\\af99 x", (Some("Batang"), None)),
        ("\\af-5\\f1 x", (Some("Batang"), None)),
        ("\\f1\\af99999999999999999999 x", (Some("Batang"), None)),
        ("\\af x", (None, None)),
    ] {
        let doc = decode(rtf(body).as_bytes(), "h.rtf");
        let (font, east) = fonts_of(&doc, "x");
        assert_eq!((font.as_deref(), east.as_deref()), expected, "{body}");
    }
}

#[test]
fn a_latin_font_with_the_default_hangul_face_keeps_both_through_hwpx_and_rtf_twice() {
    // `함초롬바탕` is the HWPX writer's default face: it must not read back as "no East Asian font" and fold into the Latin one.
    let doc = document(vec![run("둘", Some("Arial"), Some("함초롬바탕"))]);
    for (fmt, name) in [
        (DocumentFormat::Hwpx, "a.hwpx"),
        (DocumentFormat::Rtf, "a.rtf"),
    ] {
        let first = UniversalFormatConverter::encode_rich(&doc, fmt, None).unwrap();
        let once = decode(&first, name);
        assert_eq!(
            fonts_of(&once, "둘"),
            (Some("Arial".into()), Some("함초롬바탕".into())),
            "{name}"
        );
        let second = UniversalFormatConverter::encode_rich(&once, fmt, None).unwrap();
        assert_eq!(
            fonts_of(&decode(&second, name), "둘"),
            (Some("Arial".into()), Some("함초롬바탕".into())),
            "{name} twice"
        );
    }
}
