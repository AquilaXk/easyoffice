//! Korean document-format defaults (`PD-307`) across DOCX, ODT, and RTF:
//! - Default typeface (`맑은 고딕`), `10pt` body size, `108%` (`259/240`) line spacing,
//!   `8pt` (`160` twips) paragraph spacing after, justified body default alignment,
//!   `40pt` (`800` twips) default tab stop, and A4 `30mm / 25.4mm / 15mm / 17.5mm` margins.
//! - Untouched paragraph/run properties stay `None` on decode, while custom properties survive.
//! - Hostile/malformed RTF page setups fail closed without panicking.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichAlignment, RichBinding, RichBindingKind, RichBlock,
    RichDocument, RichPageFrame, RichPageMargins, RichPaper, RichParagraph, RichRun, RtfCodec,
    UniversalFormatConverter, ZipArchiveCodec, ZipSecurityPolicy, MAX_PAGE_MARGIN_TWIPS,
};

fn default_doc(paragraphs: Vec<RichParagraph>) -> RichDocument {
    RichDocument {
        title: "한국어 기본 문서".to_string(),
        blocks: paragraphs.into_iter().map(RichBlock::Paragraph).collect(),
        official_agency: None,
        ..RichDocument::default()
    }
}

fn plain_doc() -> RichDocument {
    default_doc(vec![RichParagraph {
        runs: vec![RichRun::plain("첫째 문단\t탭 다음 내용")],
        ..RichParagraph::default()
    }])
}

fn unpack(bytes: &[u8]) -> Vec<(String, String)> {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .expect("unpack zip")
        .into_iter()
        .map(|entry| {
            (
                entry.path,
                String::from_utf8_lossy(&entry.data).into_owned(),
            )
        })
        .collect()
}

fn part<'a>(parts: &'a [(String, String)], path: &str) -> &'a str {
    parts
        .iter()
        .find(|(p, _)| p == path)
        .map(|(_, s)| s.as_str())
        .unwrap_or_else(|| panic!("missing part {path}"))
}

fn decode_doc(bytes: &[u8], name: &str) -> RichDocument {
    match UniversalFormatConverter::decode_rich(bytes, Some(name))
        .expect("decode_rich")
        .content
    {
        DecodedContent::Document(doc) => doc,
        other => panic!("expected document for {name}, got {other:?}"),
    }
}

#[test]
fn docx_writes_korean_defaults_in_styles_section_and_settings() {
    let bytes = UniversalFormatConverter::encode_rich(&plain_doc(), DocumentFormat::Docx, None)
        .expect("encode docx");
    let parts = unpack(&bytes);

    let styles = part(&parts, "word/styles.xml");
    assert!(styles.contains("<w:docDefaults>"));
    assert!(
        styles.contains(
            "<w:rFonts w:ascii=\"맑은 고딕\" w:hAnsi=\"맑은 고딕\" w:eastAsia=\"맑은 고딕\" w:cs=\"Times New Roman\"/>"
        ),
        "{styles}"
    );
    assert!(styles.contains("<w:kern w:val=\"2\"/>"), "{styles}");
    assert!(styles.contains("<w:sz w:val=\"20\"/>"), "{styles}");
    assert!(styles.contains("<w:szCs w:val=\"22\"/>"), "{styles}");
    assert!(
        styles.contains("<w:lang w:val=\"en-US\" w:eastAsia=\"ko-KR\" w:bidi=\"ar-SA\"/>"),
        "{styles}"
    );
    assert!(
        styles.contains("<w:spacing w:after=\"160\" w:line=\"259\" w:lineRule=\"auto\"/>"),
        "{styles}"
    );
    assert!(styles.contains("<w:jc w:val=\"both\"/>"), "{styles}");
    assert!(!styles.contains("Calibri"), "legacy default font leaked");

    let document = part(&parts, "word/document.xml");
    assert!(
        document.contains("<w:pgSz w:w=\"11906\" w:h=\"16838\"/>"),
        "{document}"
    );
    assert!(
        document.contains(
            "<w:pgMar w:top=\"1701\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" w:header=\"851\" w:footer=\"992\" w:gutter=\"0\"/>"
        ),
        "{document}"
    );
    assert!(document.contains("<w:cols w:space=\"425\"/>"), "{document}");
    assert!(document.contains("<w:tab/>"), "{document}");

    let settings = part(&parts, "word/settings.xml");
    assert!(
        settings.contains("<w:defaultTabStop w:val=\"800\"/>"),
        "{settings}"
    );
    let rels = part(&parts, "word/_rels/document.xml.rels");
    assert!(rels.contains("Target=\"settings.xml\""), "{rels}");
    let content_types = part(&parts, "[Content_Types].xml");
    assert!(
        content_types.contains("PartName=\"/word/settings.xml\""),
        "{content_types}"
    );
}

#[test]
fn docx_settings_emits_ct_settings_elements_in_schema_sequence_order() {
    let mut doc = plain_doc();
    doc.page_frame.binding = RichBinding {
        kind: RichBindingKind::Duplex,
        gutter: 284,
    };
    doc.page_frame.even_odd_different = true;
    doc.track_changes = true;
    doc.blocks.push(RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun {
            text: "검토 추적".to_string(),
            revision: Some(easyoffice_formats::RichRevision {
                id: 1,
                kind: easyoffice_formats::RichRevisionKind::Insert,
                author: "편집자".to_string(),
                date: String::new(),
            }),
            ..RichRun::default()
        }],
        ..RichParagraph::default()
    }));

    let bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Docx, None)
        .expect("encode docx");
    let parts = unpack(&bytes);
    let settings = part(&parts, "word/settings.xml");

    let mirror = settings.find("<w:mirrorMargins/>").expect("mirrorMargins");
    let revisions = settings
        .find("<w:trackRevisions/>")
        .expect("trackRevisions");
    let even_odd = settings
        .find("<w:evenAndOddHeaders/>")
        .expect("evenAndOddHeaders");
    let tab_stop = settings
        .find("<w:defaultTabStop w:val=\"800\"/>")
        .expect("defaultTabStop");
    assert!(
        mirror < revisions && revisions < tab_stop && tab_stop < even_odd,
        "CT_Settings sequence order invalid: {settings}"
    );
}

#[test]
fn odt_writes_korean_defaults_in_manifest_styles_and_content() {
    let bytes = UniversalFormatConverter::encode_rich(&plain_doc(), DocumentFormat::Odt, None)
        .expect("encode odt");
    let parts = unpack(&bytes);

    let manifest = part(&parts, "META-INF/manifest.xml");
    assert!(
        manifest.contains("manifest:full-path=\"styles.xml\""),
        "{manifest}"
    );

    let styles = part(&parts, "styles.xml");
    assert!(styles.contains("style:name=\"맑은 고딕\""), "{styles}");
    assert!(
        styles.contains("<style:default-style style:family=\"paragraph\">"),
        "{styles}"
    );
    assert!(styles.contains("fo:text-align=\"justify\""), "{styles}");
    assert!(styles.contains("fo:margin-bottom=\"8pt\""), "{styles}");
    assert!(styles.contains("fo:line-height=\"108%\""), "{styles}");
    assert!(
        styles.contains("style:tab-stop-distance=\"40pt\""),
        "{styles}"
    );
    assert!(styles.contains("style:font-name=\"맑은 고딕\""), "{styles}");
    assert!(
        styles.contains("style:font-name-asian=\"맑은 고딕\""),
        "{styles}"
    );
    assert!(styles.contains("fo:font-size=\"10pt\""), "{styles}");
    assert!(
        styles.contains("style:font-size-asian=\"10pt\""),
        "{styles}"
    );
    assert!(styles.contains("fo:page-width=\"595.30pt\""), "{styles}");
    assert!(styles.contains("fo:page-height=\"841.90pt\""), "{styles}");
    assert!(styles.contains("fo:margin-top=\"85.05pt\""), "{styles}");
    assert!(styles.contains("fo:margin-bottom=\"72pt\""), "{styles}");
    assert!(styles.contains("fo:margin-left=\"72pt\""), "{styles}");
    assert!(styles.contains("fo:margin-right=\"72pt\""), "{styles}");
    assert!(!styles.contains("Calibri"), "{styles}");

    let content = part(&parts, "content.xml");
    assert!(content.contains("style:name=\"맑은 고딕\""), "{content}");
    assert!(content.contains("<text:tab/>"), "{content}");
    assert!(!content.contains("Calibri"), "{content}");
}

#[test]
fn rtf_writes_korean_defaults_and_roundtrips_tabs_with_boundaries() {
    let mut doc = plain_doc();
    doc.blocks.push(RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain("a\tbc\t\td")],
        ..RichParagraph::default()
    }));
    let bytes =
        UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Rtf, None).expect("encode rtf");
    let raw = String::from_utf8(bytes.clone()).expect("utf8 rtf");

    assert!(raw.starts_with("{\\rtf1\\ansi\\ansicpg949\\deff0"), "{raw}");
    let escaped_font = RtfCodec::escape_rtf_text("맑은 고딕");
    assert!(
        raw.contains(&format!(
            "{{\\fonttbl{{\\f0\\fnil\\fcharset129 {escaped_font};}}"
        )),
        "{raw}"
    );
    assert!(!raw.contains("Calibri"), "{raw}");
    assert!(
        raw.contains("{\\*\\defchp \\fs20}{\\*\\defpap \\qj\\sa160\\sl259\\slmult1}"),
        "{raw}"
    );
    assert!(
        raw.contains("{\\stylesheet{\\s0\\qj\\sa160\\sl259\\slmult1\\f0\\fs20 Normal;}}"),
        "{raw}"
    );
    assert!(
        raw.contains(
            "\\paperw11906\\paperh16838\\margl1440\\margr1440\\margt1701\\margb1440\\headery851\\footery992\\deftab800"
        ),
        "{raw}"
    );
    assert!(raw.contains("\\tab "), "{raw}");

    let back = decode_doc(&bytes, "기본.rtf");
    assert_eq!(back.page_frame.paper, Some(RichPaper::A4));
    assert_eq!(back.page_frame.margins, Some(RichPageMargins::DOC_DEFAULT));
    assert_eq!(back.blocks, doc.blocks);

    // Ensure `\tabc` is not misparsed as `\tab` + `c`, while `\tab ` decodes cleanly.
    let probe = "{\\rtf1\\ansi\\ansicpg949\\deff0\n\\pard\\plain \\tab foo\\tab\\tab bar\\par\n}";
    let parsed = decode_doc(probe.as_bytes(), "tab_boundary.rtf");
    assert_eq!(
        parsed.blocks,
        vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain("\tfoo\t\tbar")],
            ..RichParagraph::default()
        })]
    );
}

#[test]
fn defaults_preserve_none_on_untouched_text_while_keeping_explicit_overrides() {
    let doc = default_doc(vec![
        RichParagraph {
            runs: vec![RichRun::plain("기본 문단\t두번째")],
            ..RichParagraph::default()
        },
        RichParagraph {
            runs: vec![RichRun {
                text: "명시 서식".to_string(),
                font: Some("함초롬바탕".to_string()),
                size_half_pt: Some(28),
                ..RichRun::default()
            }],
            alignment: Some(RichAlignment::Center),
            line_spacing_percent: Some(150),
            ..RichParagraph::default()
        },
    ]);

    for (fmt, name) in [
        (DocumentFormat::Docx, "mix.docx"),
        (DocumentFormat::Odt, "mix.odt"),
        (DocumentFormat::Rtf, "mix.rtf"),
    ] {
        let bytes = UniversalFormatConverter::encode_rich(&doc, fmt, None).expect("encode rich");
        let back = decode_doc(&bytes, name);
        assert_eq!(back.page_frame.paper, Some(RichPaper::A4), "{name}");
        // Without a header/footer zone, ODT has no header-style height and reports top/bottom.
        let expected_margins = if fmt == DocumentFormat::Odt {
            RichPageMargins {
                header: RichPageMargins::DOC_DEFAULT.top,
                footer: RichPageMargins::DOC_DEFAULT.bottom,
                ..RichPageMargins::DOC_DEFAULT
            }
        } else {
            RichPageMargins::DOC_DEFAULT
        };
        assert_eq!(back.page_frame.margins, Some(expected_margins), "{name}");
        let RichBlock::Paragraph(first) = &back.blocks[0] else {
            panic!("first block not paragraph");
        };
        assert_eq!(first.alignment, None, "{name}");
        assert_eq!(first.line_spacing_percent, None, "{name}");
        assert_eq!(first.runs[0].font, None, "{name}");
        assert_eq!(first.runs[0].size_half_pt, None, "{name}");
        assert_eq!(first.runs[0].text, "기본 문단\t두번째", "{name}");

        let RichBlock::Paragraph(second) = &back.blocks[1] else {
            panic!("second block not paragraph");
        };
        assert_eq!(second.alignment, Some(RichAlignment::Center), "{name}");
        assert_eq!(second.line_spacing_percent, Some(150), "{name}");
        assert_eq!(second.runs[0].font.as_deref(), Some("함초롬바탕"), "{name}");
        assert_eq!(second.runs[0].size_half_pt, Some(28), "{name}");
    }
}

#[test]
fn custom_landscape_margins_and_duplex_binding_roundtrip_across_docx_odt_and_rtf() {
    let custom_margins = RichPageMargins {
        top: 1985,
        bottom: 1590,
        header: 1134,
        footer: 680,
        left: 1700,
        right: 1500,
    };
    let doc = RichDocument {
        title: "사용자 용지".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain("내용")],
            ..RichParagraph::default()
        })],
        page_frame: RichPageFrame {
            paper: Some(RichPaper {
                width: 16_838,
                height: 11_906,
            }),
            margins: Some(custom_margins),
            binding: RichBinding {
                kind: RichBindingKind::Duplex,
                gutter: 567,
            },
            ..RichPageFrame::default()
        },
        ..RichDocument::default()
    };

    for (fmt, name) in [
        (DocumentFormat::Docx, "custom.docx"),
        (DocumentFormat::Odt, "custom.odt"),
        (DocumentFormat::Rtf, "custom.rtf"),
    ] {
        let bytes = UniversalFormatConverter::encode_rich(&doc, fmt, None).expect("encode custom");
        let back = decode_doc(&bytes, name);
        assert_eq!(back.page_frame.paper, doc.page_frame.paper, "{name}");
        if fmt != DocumentFormat::Odt {
            assert_eq!(back.page_frame.margins, doc.page_frame.margins, "{name}");
        }
        assert_eq!(
            back.page_frame.binding.kind,
            RichBindingKind::Duplex,
            "{name}"
        );
        if fmt != DocumentFormat::Odt {
            assert_eq!(back.page_frame.binding.gutter, 567, "{name}");
        }
    }
}

#[test]
fn hostile_rtf_page_setup_inputs_fail_closed_by_dropping_invalid_geometry() {
    let oversized_side = MAX_PAGE_MARGIN_TWIPS + 1;
    let bad_papers = [
        // Zero / negative paper dimensions.
        "\\paperw0\\paperh16838\\margl1440\\margr1440\\margt1701\\margb1440".to_string(),
        "\\paperw-11906\\paperh16838\\margl1440\\margr1440\\margt1701\\margb1440".to_string(),
        // Overflow past MAX_PAGE_MARGIN_TWIPS.
        format!(
            "\\paperw{oversized_side}\\paperh16838\\margl1440\\margr1440\\margt1701\\margb1440"
        ),
        // Truncated page control word near EOF.
        "\\paperw11906\\paperh".to_string(),
    ];
    for setup in bad_papers {
        let rtf =
            format!("{{\\rtf1\\ansi\\ansicpg949\\deff0\n{setup}\n\\pard\\plain 본문\\par\n}}");
        let doc = decode_doc(rtf.as_bytes(), "bad_paper.rtf");
        assert!(doc.page_frame.paper.is_none(), "bad paper kept: {setup}");
    }

    let bad_margins = [
        // Negative or overflowing margins.
        "\\paperw11906\\paperh16838\\margl-500\\margr1440\\margt1701\\margb1440".to_string(),
        format!(
            "\\paperw11906\\paperh16838\\margl1440\\margr1440\\margt{oversized_side}\\margb1440"
        ),
        // Margins + gutter consuming the page body (< MIN_PAGE_BODY_TWIPS).
        "\\paperw3000\\paperh16838\\margl1400\\margr1400\\margt1701\\margb1440".to_string(),
        "\\paperw11906\\paperh16838\\margl1440\\margr1440\\margt1701\\margb1440\\gutter10000"
            .to_string(),
        // Incomplete setup without all four margins.
        "\\paperw11906\\paperh16838\\margl1440\\margr1440".to_string(),
    ];
    for setup in bad_margins {
        let rtf =
            format!("{{\\rtf1\\ansi\\ansicpg949\\deff0\n{setup}\n\\pard\\plain 본문\\par\n}}");
        let doc = decode_doc(rtf.as_bytes(), "bad_margins.rtf");
        assert!(
            doc.page_frame.margins.is_none(),
            "bad margins kept: {setup}"
        );
        if setup.contains("\\gutter10000") || setup.contains("\\paperw3000") {
            assert!(
                doc.page_frame.paper.is_none(),
                "unfitting paper kept: {setup}"
            );
            assert_eq!(doc.page_frame.binding, RichBinding::default(), "{setup}");
        }
    }
}
