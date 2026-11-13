//! Paper size, orientation and margins of the page (PD-213): DOCX `w:pgSz`/`w:pgMar`, ODT page layout and
//! HWPX `hp:pagePr` keep them with or without a header and footer, and the readers drop a setup that leaves
//! no room for the text.

use easyoffice_formats::{
    DecodedContent, DocumentFormat, RichBlock, RichDocument, RichPageFrame, RichPageMargins,
    RichPaper, RichParagraph, RichRun, UniversalFormatConverter, ZipArchiveCodec,
    ZipSecurityPolicy, MAX_PAGE_MARGIN_TWIPS,
};

const FORMATS: [(DocumentFormat, &str); 3] = [
    (DocumentFormat::Docx, "s.docx"),
    (DocumentFormat::Odt, "s.odt"),
    (DocumentFormat::Hwpx, "s.hwpx"),
];

const MARGINS: RichPageMargins = RichPageMargins {
    top: 1417,
    bottom: 1134,
    header: 567,
    footer: 454,
    left: 1020,
    right: 1360,
};

fn doc(paper: Option<RichPaper>, margins: Option<RichPageMargins>) -> RichDocument {
    RichDocument {
        title: "쪽 설정".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain("본문")],
            ..RichParagraph::default()
        })],
        official_agency: None,
        page_frame: RichPageFrame {
            header: Vec::new(),
            footer: Vec::new(),
            margins,
            paper,
        },
    }
}

fn decode(bytes: &[u8], name: &str) -> RichDocument {
    match UniversalFormatConverter::decode_rich(bytes, Some(name))
        .expect("decode")
        .content
    {
        DecodedContent::Document(doc) => doc,
        _ => panic!("document expected"),
    }
}

fn round_trip(source: &RichDocument, format: DocumentFormat, name: &str) -> RichDocument {
    decode(
        &UniversalFormatConverter::encode_rich(source, format, None).expect("encode"),
        name,
    )
}

fn part(bytes: &[u8], path: &str) -> String {
    ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default())
        .expect("unpack")
        .into_iter()
        .find(|e| e.path == path)
        .map(|e| String::from_utf8_lossy(&e.data).into_owned())
        .unwrap_or_else(|| panic!("missing part {path}"))
}

#[test]
fn paper_and_margins_round_trip_without_a_header_or_footer() {
    for paper in [
        RichPaper::A4,
        RichPaper {
            width: 12_240,
            height: 15_840,
        },
        RichPaper {
            width: 16_838,
            height: 11_906,
        },
        RichPaper {
            width: 10_319,
            height: 14_571,
        },
    ] {
        let source = doc(Some(paper), Some(MARGINS));
        for (format, name) in FORMATS {
            let back = round_trip(&source, format, name);
            assert_eq!(back.page_frame.paper, Some(paper), "{name} {paper:?}");
            // An ODT keeps the header's distance only with a header (the layout has one top margin).
            let expect = if format == DocumentFormat::Odt {
                RichPageMargins {
                    header: MARGINS.top,
                    footer: MARGINS.bottom,
                    ..MARGINS
                }
            } else {
                MARGINS
            };
            assert_eq!(back.page_frame.margins, Some(expect), "{name} {paper:?}");
            let again = round_trip(&back, format, name);
            assert_eq!(again.page_frame, back.page_frame, "{name} second round");
        }
    }
}

#[test]
fn a_document_without_a_setup_gets_the_paper_each_format_defaults_to() {
    let source = doc(None, None);
    let docx = round_trip(&source, DocumentFormat::Docx, "d.docx");
    assert_eq!(docx.page_frame.margins, None);
    assert_eq!(docx.page_frame.paper, Some(RichPaper::A4));
    // An ODT without a setup, header or footer has no page layout part at all.
    assert_eq!(
        round_trip(&source, DocumentFormat::Odt, "d.odt")
            .page_frame
            .paper,
        None
    );
    let hwpx = round_trip(&source, DocumentFormat::Hwpx, "d.hwpx");
    assert_eq!(hwpx.page_frame.paper, Some(RichPaper::A4));
    assert!(hwpx.page_frame.margins.is_some_and(|m| m.top > 0));
}

#[test]
fn landscape_is_written_as_each_format_spells_it() {
    let landscape = doc(
        Some(RichPaper {
            width: 16_838,
            height: 11_906,
        }),
        Some(MARGINS),
    );
    let docx =
        UniversalFormatConverter::encode_rich(&landscape, DocumentFormat::Docx, None).unwrap();
    assert!(part(&docx, "word/document.xml")
        .contains("<w:pgSz w:w=\"16838\" w:h=\"11906\" w:orient=\"landscape\"/>"));
    let odt = UniversalFormatConverter::encode_rich(&landscape, DocumentFormat::Odt, None).unwrap();
    assert!(part(&odt, "styles.xml").contains("style:print-orientation=\"landscape\""));
    let hwpx =
        UniversalFormatConverter::encode_rich(&landscape, DocumentFormat::Hwpx, None).unwrap();
    // The paper is stored upright and `NARROWLY` turns it (the reference editor's reader).
    assert!(part(&hwpx, "Contents/section0.xml")
        .contains("<hp:pagePr landscape=\"NARROWLY\" width=\"59530\" height=\"84190\""));
    let portrait = doc(Some(RichPaper::A4), Some(MARGINS));
    let hwpx =
        UniversalFormatConverter::encode_rich(&portrait, DocumentFormat::Hwpx, None).unwrap();
    assert!(part(&hwpx, "Contents/section0.xml").contains("<hp:pagePr landscape=\"WIDELY\""));
}

#[test]
fn a_hwpx_with_the_paper_stored_upright_reads_as_landscape() {
    let source = doc(
        Some(RichPaper {
            width: 16_838,
            height: 11_906,
        }),
        Some(MARGINS),
    );
    let hwpx = UniversalFormatConverter::encode_rich(&source, DocumentFormat::Hwpx, None).unwrap();
    let back = decode(&hwpx, "h.hwpx");
    assert_eq!(
        back.page_frame.paper,
        Some(RichPaper {
            width: 16_838,
            height: 11_906
        })
    );
}

#[test]
fn a_hwpx_table_is_as_wide_as_the_text_of_its_own_page() {
    let mut source = doc(
        Some(RichPaper {
            width: 16_838,
            height: 11_906,
        }),
        Some(MARGINS),
    );
    source.blocks.push(RichBlock::Table(
        vec![vec!["a".to_string(), "b".to_string()]].into(),
    ));
    let hwpx = UniversalFormatConverter::encode_rich(&source, DocumentFormat::Hwpx, None).unwrap();
    // 16838 - 1020 - 1360 twips of text, five HWPUNIT each, in two columns.
    let cell = (16_838 - 1_020 - 1_360) * 5 / 2;
    assert!(part(&hwpx, "Contents/section0.xml").contains(&format!("<hp:cellSz width=\"{cell}\"")));
}

fn docx_with(props: &str) -> Vec<u8> {
    const W: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"";
    let document = format!(
        "<w:document {W}><w:body><w:p><w:r><w:t>본문</w:t></w:r></w:p><w:sectPr>{props}</w:sectPr></w:body></w:document>"
    );
    ZipArchiveCodec::pack_compressed(&[
        (
            "[Content_Types].xml".to_string(),
            br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>"#.to_vec(),
        ),
        ("word/document.xml".to_string(), document.into_bytes()),
    ])
    .expect("pack")
}

const PG_MAR: &str = "<w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" w:header=\"720\" w:footer=\"720\"/>";

fn pg_mar(top: u32, side: u32) -> String {
    format!("<w:pgMar w:top=\"{top}\" w:right=\"{side}\" w:bottom=\"1440\" w:left=\"{side}\" w:header=\"720\" w:footer=\"720\"/>")
}

#[test]
fn a_setup_that_leaves_no_room_for_the_text_is_dropped() {
    let a4 = "<w:pgSz w:w=\"11906\" w:h=\"16838\"/>";
    for (props, why) in [
        (format!("{a4}{}", pg_mar(1440, 6000)), "sides meet"),
        (
            format!("<w:pgSz w:w=\"11906\" w:h=\"2000\"/>{}", pg_mar(1440, 1440)),
            "page too low",
        ),
        (
            format!(
                "<w:pgSz w:w=\"99999999\" w:h=\"16838\"/>{}",
                pg_mar(1440, 1440)
            ),
            "paper over the bound",
        ),
    ] {
        let back = decode(&docx_with(&props), "bad.docx");
        assert_eq!(back.page_frame.margins, None, "{why}");
        assert_eq!(back.page_frame.paper, None, "{why}");
    }
    let over = decode(
        &docx_with(&format!("{a4}{}", pg_mar(MAX_PAGE_MARGIN_TWIPS + 1, 1440))),
        "bad.docx",
    );
    assert_eq!(over.page_frame.margins, None, "a margin over the bound");
    assert_eq!(
        over.page_frame.paper,
        Some(RichPaper::A4),
        "the paper is still good"
    );
    for (props, why) in [
        ("<w:pgSz w:w=\"0\" w:h=\"16838\"/>", "paper without a side"),
        (
            "<w:pgSz w:w=\"x\" w:h=\"16838\"/>",
            "paper that is no number",
        ),
    ] {
        assert_eq!(
            decode(&docx_with(props), "bad.docx").page_frame.paper,
            None,
            "{why}"
        );
    }
}

#[test]
fn a_negative_or_exact_margin_counts_by_its_size_and_a_missing_paper_keeps_the_margins() {
    let props = "<w:pgMar w:top=\"-1440\" w:right=\"1000\" w:bottom=\"1440\" w:left=\"1100\" w:header=\"720\" w:footer=\"720\"/>";
    let back = decode(&docx_with(props), "neg.docx");
    let margins = back.page_frame.margins.expect("margins");
    assert_eq!(
        (margins.top, margins.left, margins.right),
        (1440, 1100, 1000)
    );
    assert_eq!(back.page_frame.paper, None);
}

#[test]
fn the_first_section_gives_the_setup() {
    const W: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"";
    let first = format!("<w:pgSz w:w=\"16838\" w:h=\"11906\"/>{PG_MAR}");
    let document = format!(
        "<w:document {W}><w:body><w:p><w:pPr><w:sectPr>{first}</w:sectPr></w:pPr><w:r><w:t>가</w:t></w:r></w:p>\
         <w:p><w:r><w:t>나</w:t></w:r></w:p><w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/>{PG_MAR}</w:sectPr></w:body></w:document>"
    );
    let bytes = ZipArchiveCodec::pack_compressed(&[
        ("[Content_Types].xml".to_string(), br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>"#.to_vec()),
        ("word/document.xml".to_string(), document.into_bytes()),
    ])
    .unwrap();
    let back = decode(&bytes, "two.docx");
    assert_eq!(
        back.page_frame.paper,
        Some(RichPaper {
            width: 16_838,
            height: 11_906
        })
    );
}

#[test]
fn rich_paper_holds_checks_the_text_room_on_both_axes() {
    let paper = RichPaper::A4;
    assert!(paper.holds(&MARGINS));
    assert!(!paper.holds(&RichPageMargins {
        left: 6000,
        right: 6000,
        ..MARGINS
    }));
    assert!(!paper.holds(&RichPageMargins {
        top: 9000,
        bottom: 8000,
        ..MARGINS
    }));
    assert!(!RichPaper {
        width: MAX_PAGE_MARGIN_TWIPS + 1,
        height: 16_838
    }
    .holds(&MARGINS));
}
