//! PD-372: the width scale, the raised or lowered position and the kerning threshold of a run survive
//! DOCX, ODT, RTF and HWPX in the unit each format keeps them in, the formats that cannot keep them
//! say so, and hostile values stay inside the model's bounds.

use easyoffice_formats::{
    pages_write_loss, CharAdvanced, DecodedContent, DocumentFormat, RichBlock, RichDocument,
    RichParagraph, RichRun, RichScript, UniversalFormatConverter, ZipArchiveCodec,
    ZipSecurityPolicy, MAX_KERNING_HALF_PT, MAX_POSITION_HALF_PT, MAX_SCALE_PERCENT,
};

fn adv(scale: Option<u16>, position: Option<i16>, kerning: Option<u16>) -> CharAdvanced {
    CharAdvanced {
        scale_percent: scale,
        position_half_pt: position,
        kerning_half_pt: kerning,
    }
}

fn run(text: &str, size: Option<u16>, advanced: CharAdvanced) -> RichRun {
    RichRun {
        text: text.to_string(),
        size_half_pt: size,
        advanced,
        ..RichRun::default()
    }
}

fn document(runs: Vec<RichRun>) -> RichDocument {
    RichDocument {
        title: "고급".to_string(),
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

fn settings(doc: &RichDocument) -> Vec<(String, CharAdvanced)> {
    doc.blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p),
            _ => None,
        })
        .flat_map(|p| p.runs.iter())
        .map(|r| (r.text.clone(), r.advanced))
        .collect()
}

fn part(bytes: &[u8], path: &str) -> String {
    let entries = ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).expect("unpack");
    let entry = entries.iter().find(|e| e.path == path).expect("part");
    String::from_utf8(entry.data.clone()).expect("utf8")
}

/// `bytes` with `from` in the part `path` replaced by `to`.
fn patched(bytes: &[u8], path: &str, from: &str, to: &str) -> Vec<u8> {
    let entries: Vec<(String, Vec<u8>)> =
        ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
            .unwrap()
            .into_iter()
            .map(|e| {
                if e.path == path {
                    let text = String::from_utf8(e.data).unwrap();
                    assert!(text.contains(from), "{from} not in {path}: {text}");
                    (e.path, text.replace(from, to).into_bytes())
                } else {
                    (e.path, e.data)
                }
            })
            .collect();
    ZipArchiveCodec::pack_compressed(&entries).unwrap()
}

fn sample() -> RichDocument {
    document(vec![
        run("전부", Some(24), adv(Some(150), Some(6), Some(24))),
        run("좁고 내림", Some(24), adv(Some(50), Some(-10), None)),
        run("보통", Some(24), CharAdvanced::default()),
        run("크기 없음", None, adv(None, Some(4), None)),
    ])
}

fn expect_in(back: &[(String, CharAdvanced)], text: &str, expected: CharAdvanced, what: &str) {
    assert!(
        back.iter().any(|(t, a)| t == text && *a == expected),
        "{what} {text}: expected {expected:?} in {back:?}"
    );
}

#[test]
fn docx_and_rtf_keep_scale_position_and_kerning_exactly() {
    for (fmt, name) in [
        (DocumentFormat::Docx, "a.docx"),
        (DocumentFormat::Rtf, "a.rtf"),
    ] {
        let bytes = UniversalFormatConverter::encode_rich(&sample(), fmt, None).unwrap();
        let back = settings(&decode(&bytes, name));
        expect_in(&back, "전부", adv(Some(150), Some(6), Some(24)), name);
        expect_in(&back, "좁고 내림", adv(Some(50), Some(-10), None), name);
        expect_in(&back, "보통", CharAdvanced::default(), name);
        expect_in(&back, "크기 없음", adv(None, Some(4), None), name);
    }
}

#[test]
fn each_format_writes_its_own_words() {
    let doc = document(vec![run("가", Some(24), adv(Some(150), Some(6), Some(24)))]);
    let docx = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None).unwrap();
    let xml = part(&docx, "word/document.xml");
    for word in [
        "<w:w w:val=\"150\"/>",
        "<w:kern w:val=\"24\"/>",
        "<w:position w:val=\"6\"/>",
    ] {
        assert!(xml.contains(word), "{word} in {xml}");
    }
    let odt = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Odt, None).unwrap();
    let xml = part(&odt, "content.xml");
    for word in [
        "style:text-scale=\"150%\"",
        "style:text-position=\"25% 100%\"",
        "style:letter-kerning=\"true\"",
    ] {
        assert!(xml.contains(word), "{word} in {xml}");
    }
    let rtf = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Rtf, None).unwrap();
    let rtf = String::from_utf8_lossy(&rtf);
    for word in ["\\charscalex150 ", "\\up6 ", "\\kerning24 "] {
        assert!(rtf.contains(word), "{word} in {rtf}");
    }
    let lowered = document(vec![run("나", Some(24), adv(None, Some(-10), None))]);
    let rtf = UniversalFormatConverter::encode_rich(&lowered, DocumentFormat::Rtf, None).unwrap();
    assert!(String::from_utf8_lossy(&rtf).contains("\\dn10 "));
    let hwpx = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Hwpx, None).unwrap();
    let header = part(&hwpx, "Contents/header.xml");
    for word in [
        "<hh:ratio hangul=\"150\"",
        "<hh:offset hangul=\"25\"",
        "useKerning=\"1\"",
    ] {
        assert!(header.contains(word), "{word} in {header}");
    }
}

#[test]
fn odt_keeps_scale_exactly_and_the_others_at_the_run_size() {
    let bytes =
        UniversalFormatConverter::encode_rich(&sample(), DocumentFormat::Odt, None).unwrap();
    let back = settings(&decode(&bytes, "a.odt"));
    // Kerning is a switch in ODT, so the threshold comes back as the 1 pt default (2 half-points).
    expect_in(&back, "전부", adv(Some(150), Some(6), Some(2)), "odt");
    expect_in(&back, "좁고 내림", adv(Some(50), Some(-10), None), "odt");
    expect_in(&back, "보통", CharAdvanced::default(), "odt");
    // 2 pt on the 10 pt default size is 20 %, which reads back at the same default size.
    expect_in(&back, "크기 없음", adv(None, Some(4), None), "odt");
}

#[test]
fn hwpx_keeps_the_position_at_the_run_size_and_limits_the_scale_to_its_range() {
    let doc = document(vec![
        run("전부", Some(24), adv(Some(150), Some(6), Some(24))),
        run("넓음", Some(24), adv(Some(300), None, None)),
        run("좁음", Some(24), adv(Some(5), None, None)),
        run(
            "멀리",
            Some(24),
            adv(None, Some(MAX_POSITION_HALF_PT), None),
        ),
        run("크기 없음", None, adv(None, Some(4), None)),
    ]);
    let bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Hwpx, None).unwrap();
    let back = settings(&decode(&bytes, "a.hwpx"));
    expect_in(&back, "전부", adv(Some(150), Some(6), Some(2)), "hwpx");
    expect_in(
        &back,
        "넓음",
        adv(Some(200), None, None),
        "hwpx scale above 200",
    );
    expect_in(
        &back,
        "좁음",
        adv(Some(10), None, None),
        "hwpx scale below 10",
    );
    // 1,584 pt on 12 pt is far past 100 %, so the position stops at the size (12 pt = 24 half-points).
    expect_in(
        &back,
        "멀리",
        adv(None, Some(24), None),
        "hwpx position above 100 %",
    );
    expect_in(
        &back,
        "크기 없음",
        adv(None, Some(4), None),
        "hwpx default size",
    );
}

#[test]
fn pages_cannot_keep_them_and_the_save_says_so() {
    let doc = sample();
    let loss = pages_write_loss(&doc);
    assert_eq!(loss.formats, 3, "three runs carry a setting");
    assert!(loss.warnings().iter().any(|w| w.contains("서식 3곳")));
    let plain = document(vec![run("보통", Some(24), CharAdvanced::default())]);
    assert!(pages_write_loss(&plain).warnings().is_empty());
    let bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Pages, None).unwrap();
    let back = settings(&decode(&bytes, "a.pages"));
    assert!(back.iter().all(|(_, a)| a.is_none()), "{back:?}");
}

#[test]
fn a_position_set_with_a_script_keeps_the_script_in_odt() {
    let both = RichRun {
        text: "위첨자".to_string(),
        size_half_pt: Some(24),
        script: RichScript::Superscript,
        advanced: adv(None, Some(6), None),
        ..RichRun::default()
    };
    let bytes =
        UniversalFormatConverter::encode_rich(&document(vec![both]), DocumentFormat::Odt, None)
            .unwrap();
    let xml = part(&bytes, "content.xml");
    assert!(xml.contains("style:text-position=\"super 58%\""), "{xml}");
    let doc = decode(&bytes, "b.odt");
    let RichBlock::Paragraph(p) = &doc.blocks[0] else {
        panic!("paragraph");
    };
    assert_eq!(p.runs[0].script, RichScript::Superscript);
    assert!(p.runs[0].advanced.position_half_pt.is_none());
}

#[test]
fn the_reference_script_positions_stay_scripts_in_odt() {
    let bytes = UniversalFormatConverter::encode_rich(
        &document(vec![run("x", Some(24), adv(None, Some(6), None))]),
        DocumentFormat::Odt,
        None,
    )
    .unwrap();
    for (position, script, half_pt) in [
        ("33% 58%", RichScript::Superscript, None),
        ("-33% 58%", RichScript::Subscript, None),
        ("super 58%", RichScript::Superscript, None),
        ("20%", RichScript::Normal, Some(5)),
        ("-20% 100%", RichScript::Normal, Some(-5)),
    ] {
        let doc = decode(
            &patched(&bytes, "content.xml", "25% 100%", position),
            "p.odt",
        );
        let RichBlock::Paragraph(p) = &doc.blocks[0] else {
            panic!("paragraph");
        };
        assert_eq!(p.runs[0].script, script, "{position}");
        assert_eq!(p.runs[0].advanced.position_half_pt, half_pt, "{position}");
    }
}

fn docx_with(run_props: &str) -> Vec<u8> {
    let bytes = UniversalFormatConverter::encode_rich(
        &document(vec![run("x", None, adv(Some(150), Some(6), Some(24)))]),
        DocumentFormat::Docx,
        None,
    )
    .unwrap();
    let xml = part(&bytes, "word/document.xml");
    let start = xml.find("<w:rPr>").unwrap();
    let end = xml.find("</w:rPr>").unwrap();
    let hostile = format!("{}<w:rPr>{run_props}{}", &xml[..start], &xml[end..]);
    patched(&bytes, "word/document.xml", &xml, &hostile)
}

fn only(doc: &RichDocument) -> CharAdvanced {
    let back = settings(doc);
    assert_eq!(back.len(), 1, "{back:?}");
    back[0].1
}

#[test]
fn hostile_docx_values_are_clamped_or_dropped() {
    let (max_scale, max_pos, max_kern) =
        (MAX_SCALE_PERCENT, MAX_POSITION_HALF_PT, MAX_KERNING_HALF_PT);
    for (props, expected) in [
        (
            r#"<w:w w:val="9223372036854775807"/><w:position w:val="9223372036854775807"/><w:kern w:val="9223372036854775807"/>"#,
            adv(Some(max_scale), Some(max_pos), Some(max_kern)),
        ),
        (
            r#"<w:w w:val="-9223372036854775808"/><w:position w:val="-9223372036854775808"/><w:kern w:val="-9223372036854775808"/>"#,
            adv(None, Some(-max_pos), None),
        ),
        (
            r#"<w:w w:val="0"/><w:position w:val="0"/><w:kern w:val="0"/>"#,
            CharAdvanced::default(),
        ),
        (r#"<w:w w:val="100"/>"#, CharAdvanced::default()),
        (
            r#"<w:w w:val="NaN"/><w:position w:val="1.5"/><w:kern w:val="abc"/>"#,
            CharAdvanced::default(),
        ),
        (r#"<w:w/><w:position/><w:kern/>"#, CharAdvanced::default()),
        (
            r#"<w:w w:val="99999999999999999999999"/>"#,
            CharAdvanced::default(),
        ),
    ] {
        let doc = decode(&docx_with(props), "h.docx");
        assert_eq!(only(&doc), expected, "{props}");
    }
}

#[test]
fn hostile_rtf_values_are_clamped_or_dropped() {
    let (max_scale, max_pos, max_kern) =
        (MAX_SCALE_PERCENT, MAX_POSITION_HALF_PT, MAX_KERNING_HALF_PT);
    for (words, expected) in [
        (
            "\\charscalex2147483647\\up2147483647\\kerning2147483647",
            adv(Some(max_scale), Some(max_pos), Some(max_kern)),
        ),
        (
            "\\charscalex-2147483647\\dn2147483647\\kerning-5",
            adv(None, Some(-max_pos), None),
        ),
        ("\\charscalex0\\up0\\kerning0", CharAdvanced::default()),
        ("\\charscalex100", CharAdvanced::default()),
        ("\\up", adv(None, Some(6), None)),
        ("\\dn", adv(None, Some(-6), None)),
        ("\\up8\\up0", CharAdvanced::default()),
        ("\\up8\\dn4", adv(None, Some(-4), None)),
        ("\\charscalex150\\plain", CharAdvanced::default()),
    ] {
        let rtf = format!("{{\\rtf1\\ansi\\pard {words} x\\par}}");
        let doc = decode(rtf.as_bytes(), "h.rtf");
        assert_eq!(only(&doc), expected, "{words}");
    }
}

#[test]
fn hostile_odt_values_are_clamped_or_dropped() {
    let bytes = UniversalFormatConverter::encode_rich(
        &document(vec![run("x", Some(24), adv(Some(150), Some(6), Some(24)))]),
        DocumentFormat::Odt,
        None,
    )
    .unwrap();
    for (scale, position, kerning, expected) in [
        (
            "999999999999%",
            "99999999999999%",
            "true",
            adv(Some(MAX_SCALE_PERCENT), Some(MAX_POSITION_HALF_PT), Some(2)),
        ),
        (
            "-5%",
            "-99999999999999%",
            "false",
            adv(None, Some(-MAX_POSITION_HALF_PT), None),
        ),
        ("NaN%", "NaN%", "maybe", CharAdvanced::default()),
        ("inf%", "inf%", "", CharAdvanced::default()),
        ("1e999%", "1e999%", "TRUE", CharAdvanced::default()),
        ("150", "20", "0", CharAdvanced::default()),
        ("100%", "0%", "false", CharAdvanced::default()),
        ("100%", "5% 58%", "1", adv(None, None, Some(2))),
    ] {
        let hostile = patched(&bytes, "content.xml", "150%", scale);
        let hostile = patched(
            &hostile,
            "content.xml",
            "25% 100%",
            &format!("{position} 100%"),
        );
        let hostile = patched(
            &hostile,
            "content.xml",
            "letter-kerning=\"true\"",
            &format!("letter-kerning=\"{kerning}\""),
        );
        let doc = decode(&hostile, "h.odt");
        assert_eq!(only(&doc), expected, "{scale} {position} {kerning}");
    }
}

#[test]
fn hostile_hwpx_values_are_clamped_or_dropped() {
    let bytes = UniversalFormatConverter::encode_rich(
        &document(vec![run("x", Some(24), adv(Some(150), Some(6), Some(24)))]),
        DocumentFormat::Hwpx,
        None,
    )
    .unwrap();
    for (ratio, offset, kerning, expected) in [
        (
            "4294967296",
            "9223372036854775807",
            "1",
            adv(Some(MAX_SCALE_PERCENT), Some(24), Some(2)),
        ),
        (
            "-1",
            "-9223372036854775808",
            "0",
            adv(None, Some(-24), None),
        ),
        ("0", "0", "0", CharAdvanced::default()),
        ("100", "0", "false", CharAdvanced::default()),
        ("NaN", "1.5", "yes", CharAdvanced::default()),
        ("", "", "", CharAdvanced::default()),
    ] {
        let hostile = patched(
            &bytes,
            "Contents/header.xml",
            "hangul=\"150\"",
            &format!("hangul=\"{ratio}\""),
        );
        let hostile = patched(
            &hostile,
            "Contents/header.xml",
            "<hh:offset hangul=\"25\"",
            &format!("<hh:offset hangul=\"{offset}\""),
        );
        let hostile = patched(
            &hostile,
            "Contents/header.xml",
            "useKerning=\"1\"",
            &format!("useKerning=\"{kerning}\""),
        );
        let doc = decode(&hostile, "h.hwpx");
        assert_eq!(only(&doc), expected, "{ratio} {offset} {kerning}");
    }
}

#[test]
fn documents_without_the_settings_read_as_before() {
    let docx = r#"<w:rPr><w:b/><w:sz w:val="24"/></w:rPr>"#;
    let doc = decode(&docx_with(docx), "n.docx");
    assert_eq!(only(&doc), CharAdvanced::default());
}
