//! HWP/HWPX defaults (PD-311) aligned with the reference Korean word processor (`KS X 6101`):
//! - Default typeface (`함초롬바탕`), `10pt` (`1000` HWPUNIT) body size, `160%` line spacing,
//!   `바탕글` (`Normal`) `JUSTIFY` body alignment, 3 default tab definitions, A4 (`59528 x 84188`
//!   HWPUNIT) with `30mm / 30mm / 20mm / 15mm / 15mm / 15mm` (`8504/8504/5668/4252/4252/4252`),
//!   and complete `hp:secPr` sub-elements (`lineNumberShape`, `footNotePr`, `endNotePr`, 3x `pageBorderFill`).
//! - Outline headings (`OUTLINE` level 0-based), `\t`/`\n` inline control elements, reference A4 height
//!   `84186` tolerance, and fail-closed rejection when margins exceed the paper bounds.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, HwpxCodec, RichAlignment, RichBlock, RichDocument,
    RichPageMargins, RichPaper, RichParagraph, RichRun, UniversalFormatConverter, ZipArchiveCodec,
    ZipSecurityPolicy,
};

fn unpack_parts(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .expect("unpack hwpx")
        .into_iter()
        .map(|entry| (entry.path, entry.data))
        .collect()
}

fn part_text<'a>(parts: &'a [(String, Vec<u8>)], path: &str) -> &'a str {
    parts
        .iter()
        .find(|(p, _)| p == path)
        .and_then(|(_, data)| std::str::from_utf8(data).ok())
        .unwrap_or_else(|| panic!("missing utf8 part {path}"))
}

fn decode_hwpx(bytes: &[u8], name: &str) -> RichDocument {
    match UniversalFormatConverter::decode_rich(bytes, Some(name))
        .expect("decode_rich hwpx")
        .content
    {
        DecodedContent::Document(doc) => doc,
        other => panic!("expected rich document: {other:?}"),
    }
}

fn with_section_xml(bytes: &[u8], transform: impl Fn(&str) -> String) -> Vec<u8> {
    let entries: Vec<(String, Vec<u8>)> = unpack_parts(bytes)
        .into_iter()
        .map(|(path, data)| {
            if path == "Contents/section0.xml" {
                let xml = String::from_utf8(data).expect("utf8 section0.xml");
                (path, transform(&xml).into_bytes())
            } else {
                (path, data)
            }
        })
        .collect();
    ZipArchiveCodec::pack(&entries).expect("repack hwpx")
}

#[test]
fn blank_and_default_hwpx_match_korean_wordprocessor_defaults() {
    let doc = RichDocument {
        title: "한글 기본값".to_string(),
        blocks: vec![
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("기본 문단")],
                ..RichParagraph::default()
            }),
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("명시 양쪽정렬")],
                alignment: Some(RichAlignment::Justify),
                ..RichParagraph::default()
            }),
        ],
        official_agency: None,
        ..RichDocument::default()
    };

    let bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Hwpx, None)
        .expect("encode hwpx");
    let parts = unpack_parts(&bytes);
    let header = part_text(&parts, "Contents/header.xml");
    let section = part_text(&parts, "Contents/section0.xml");

    assert!(
        header.contains("<hh:font id=\"0\" face=\"함초롬바탕\" type=\"TTF\" isEmbedded=\"0\"/>"),
        "{header}"
    );
    assert!(
        header.contains(
            "<hh:charPr id=\"0\" height=\"1000\" textColor=\"#000000\" shadeColor=\"none\""
        ),
        "{header}"
    );
    assert!(
        header.contains("<hh:fontRef hangul=\"0\" latin=\"0\" hanja=\"0\" japanese=\"0\" other=\"0\" symbol=\"0\" user=\"0\"/>"),
        "{header}"
    );
    assert!(
        header.contains("<hh:tabProperties itemCnt=\"3\"><hh:tabPr id=\"0\" autoTabLeft=\"0\" autoTabRight=\"0\"/><hh:tabPr id=\"1\" autoTabLeft=\"1\" autoTabRight=\"0\"/><hh:tabPr id=\"2\" autoTabLeft=\"0\" autoTabRight=\"1\"/></hh:tabProperties>"),
        "{header}"
    );
    assert!(
        header.contains("<hh:paraPr id=\"0\" tabPrIDRef=\"0\""),
        "{header}"
    );
    assert!(
        header.contains("<hh:align horizontal=\"JUSTIFY\" vertical=\"BASELINE\"/><hh:heading type=\"NONE\" idRef=\"0\" level=\"0\"/>"),
        "{header}"
    );
    assert!(
        header.contains("<hh:lineSpacing type=\"PERCENT\" value=\"160\" unit=\"HWPUNIT\"/>"),
        "{header}"
    );
    assert!(
        header.contains("<hh:style id=\"0\" type=\"PARA\" name=\"바탕글\" engName=\"Normal\" paraPrIDRef=\"0\" charPrIDRef=\"0\""),
        "{header}"
    );
    assert!(
        header.contains("<hh:paraPr id=\"1\" tabPrIDRef=\"0\""),
        "{header}"
    );

    assert!(
        section.contains("xmlns:hc=\"http://www.owpml.org/owpml/2011/core\""),
        "{section}"
    );
    assert!(section.contains("<hp:lineNumberShape "), "{section}");
    assert!(
        section.contains("<hp:pagePr landscape=\"WIDELY\" width=\"59528\" height=\"84188\" gutterType=\"LEFT_ONLY\"><hp:margin header=\"4252\" footer=\"4252\" gutter=\"0\" left=\"8504\" right=\"8504\" top=\"5668\" bottom=\"4252\"/></hp:pagePr>"),
        "{section}"
    );
    assert!(section.contains("<hp:footNotePr>"), "{section}");
    assert!(section.contains("<hp:endNotePr>"), "{section}");
    for kind in ["BOTH", "EVEN", "ODD"] {
        assert!(
            section.contains(&format!("<hp:pageBorderFill type=\"{kind}\"")),
            "missing pageBorderFill {kind}: {section}"
        );
    }

    let back = decode_hwpx(&bytes, "default.hwpx");
    assert_eq!(back.page_frame.paper, Some(RichPaper::A4));
    assert_eq!(back.page_frame.margins, Some(RichPageMargins::HWP_DEFAULT));
    let RichBlock::Paragraph(first) = &back.blocks[0] else {
        panic!("expected paragraph 0");
    };
    assert_eq!(first.alignment, None);
    assert_eq!(first.line_spacing, None);
    assert_eq!(first.runs[0].font, None);
    assert_eq!(first.runs[0].size_half_pt, None);

    let RichBlock::Paragraph(second) = &back.blocks[1] else {
        panic!("expected paragraph 1");
    };
    assert_eq!(second.alignment, Some(RichAlignment::Justify));
}

#[test]
fn outline_headings_tabs_and_line_breaks_roundtrip_in_rich_and_flat_unpack() {
    let doc = RichDocument {
        title: "개요와 탭".to_string(),
        blocks: vec![
            RichBlock::Paragraph(RichParagraph {
                heading_level: 1,
                runs: vec![RichRun::plain("제목 1")],
                ..RichParagraph::default()
            }),
            RichBlock::Paragraph(RichParagraph {
                heading_level: 3,
                runs: vec![RichRun::plain("제목 3")],
                ..RichParagraph::default()
            }),
            RichBlock::Paragraph(RichParagraph {
                runs: vec![RichRun::plain("첫칸\t둘째칸\n다음줄")],
                ..RichParagraph::default()
            }),
            RichBlock::Table(vec![vec!["셀1\t탭".into(), "셀2\n줄".into()]].into()),
        ],
        official_agency: None,
        ..RichDocument::default()
    };

    let bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Hwpx, None)
        .expect("encode hwpx");
    let parts = unpack_parts(&bytes);
    let header = part_text(&parts, "Contents/header.xml");
    let section = part_text(&parts, "Contents/section0.xml");

    assert!(header.contains("tabPrIDRef=\"1\""), "{header}");
    assert!(
        header.contains("<hh:heading type=\"OUTLINE\" idRef=\"0\" level=\"0\"/>"),
        "{header}"
    );
    assert!(
        header.contains("<hh:heading type=\"OUTLINE\" idRef=\"0\" level=\"2\"/>"),
        "{header}"
    );
    assert!(
        section.contains("<hp:tab width=\"0\" leader=\"0\" type=\"1\"/>"),
        "{section}"
    );
    assert!(section.contains("<hp:lineBreak/>"), "{section}");

    let back = decode_hwpx(&bytes, "outline.hwpx");
    assert_eq!(
        match &back.blocks[0] {
            RichBlock::Paragraph(p) => (p.heading_level, p.alignment),
            other => panic!("unexpected block 0: {other:?}"),
        },
        (1, None)
    );
    assert_eq!(
        match &back.blocks[1] {
            RichBlock::Paragraph(p) => (p.heading_level, p.alignment),
            other => panic!("unexpected block 1: {other:?}"),
        },
        (3, None)
    );
    assert_eq!(
        match &back.blocks[2] {
            RichBlock::Paragraph(p) => p.runs.iter().map(|r| r.text.as_str()).collect::<String>(),
            other => panic!("unexpected block 2: {other:?}"),
        },
        "첫칸\t둘째칸\n다음줄"
    );

    let flat = HwpxCodec::unpack(&bytes).expect("flat unpack");
    assert!(
        flat.paragraphs.iter().any(|p| p == "첫칸\t둘째칸\n다음줄"),
        "{:?}",
        flat.paragraphs
    );
    assert_eq!(flat.tables[0][0], vec!["셀1\t탭", "셀2\n줄"]);
}

#[test]
fn reference_a4_height_84186_snaps_to_a4_and_oversized_margins_fail_closed() {
    let doc = RichDocument {
        title: "용지 검증".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain("본문")],
            ..RichParagraph::default()
        })],
        official_agency: None,
        ..RichDocument::default()
    };
    let bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Hwpx, None)
        .expect("encode hwpx");

    let ref_a4 = with_section_xml(&bytes, |xml| {
        xml.replace("height=\"84188\"", "height=\"84186\"")
    });
    let ref_doc = decode_hwpx(&ref_a4, "ref_a4.hwpx");
    assert_eq!(ref_doc.page_frame.paper, Some(RichPaper::A4));
    assert_eq!(
        ref_doc.page_frame.margins,
        Some(RichPageMargins::HWP_DEFAULT)
    );

    let wide_margins = with_section_xml(&bytes, |xml| {
        xml.replace(
            "left=\"8504\" right=\"8504\"",
            "left=\"40000\" right=\"40000\"",
        )
    });
    let wide_doc = decode_hwpx(&wide_margins, "wide.hwpx");
    assert_eq!(wide_doc.page_frame.margins, None);

    let no_page_pr_wide = with_section_xml(&bytes, |xml| {
        xml.replace(
            "<hp:pagePr landscape=\"WIDELY\" width=\"59528\" height=\"84188\" gutterType=\"LEFT_ONLY\">",
            "",
        )
        .replace("</hp:pagePr>", "")
        .replace("left=\"8504\" right=\"8504\"", "left=\"40000\" right=\"40000\"")
    });
    let no_page_doc = decode_hwpx(&no_page_pr_wide, "no_page_pr.hwpx");
    assert_eq!(no_page_doc.page_frame.margins, None);
}

#[test]
fn asymmetric_bottom_and_footer_margins_roundtrip_symmetrically_with_hwpx_margin_attrs() {
    let margins = RichPageMargins {
        top: 1984,
        bottom: 1984,
        header: 1134,
        footer: 1417,
        left: 1701,
        right: 1701,
    };
    let mut doc = RichDocument {
        title: "비대칭 꼬리말 여백".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain("본문")],
            ..RichParagraph::default()
        })],
        official_agency: None,
        ..RichDocument::default()
    };
    doc.page_frame.paper = Some(RichPaper::A4);
    doc.page_frame.margins = Some(margins);

    let bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Hwpx, None)
        .expect("encode hwpx");
    let parts = unpack_parts(&bytes);
    let section = part_text(&parts, "Contents/section0.xml");
    assert!(
        section.contains(
            "<hp:margin header=\"4250\" footer=\"2835\" gutter=\"0\" left=\"8505\" right=\"8505\" top=\"5670\" bottom=\"7085\"/>"
        ),
        "{section}"
    );

    let back = decode_hwpx(&bytes, "asymmetric_footer.hwpx");
    assert_eq!(back.page_frame.margins, Some(margins));
}
