//! Native HWPX codec preservation tests (`PD-247`): shapes (`hp:rect`, `hp:ellipse`,
//! `hp:container`, `hp:textart`, `hp:caption`), equations (`hp:equation` -> `$$LaTeX$$`),
//! pictures inside table cells (`hp:tc` -> `hp:pic`), footnotes/endnotes (`hp:footNote`,
//! `hp:endNote`, `ON_SECTION` restart, WCHAR decorations), cross-format conversion, fail-closed
//! budgets and `--release` throughput measurement.

mod image_fixture;

use easyoffice_formats::{
    DecodedContent, DocumentFormat, EasyOfficeError, HwpxCodec, RichBlock, RichDocument,
    RichScript, UniversalFormatConverter, ZipArchiveCodec,
};
use image_fixture::{make_jpeg, make_png};
use std::time::Instant;

const HEADER_XML: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
    "<hh:head xmlns:hh=\"http://www.owpml.org/owpml/2011/head\" xmlns:hc=\"http://www.owpml.org/owpml/2011/core\">",
    "<hh:refList>",
    "<hh:charProperties itemCnt=\"2\">",
    "<hh:charPr id=\"0\" height=\"1000\" textColor=\"#000000\"/>",
    "<hh:charPr id=\"1\" height=\"1200\" textColor=\"#1F4E79\"><hh:bold/></hh:charPr>",
    "</hh:charProperties>",
    "<hh:paraProperties itemCnt=\"1\"><hh:paraPr id=\"0\"><hh:align horizontal=\"LEFT\"/></hh:paraPr></hh:paraProperties>",
    "</hh:refList></hh:head>"
);

fn build_hwpx(sections: &[(&str, &str)], media: &[(&str, &str, Vec<u8>)]) -> Vec<u8> {
    let mut manifest_items = String::new();
    for (id, href, _) in media {
        let mime = if href.ends_with(".png") {
            "image/png"
        } else {
            "image/jpeg"
        };
        manifest_items.push_str(&format!(
            "<opf:item id=\"{id}\" href=\"{href}\" media-type=\"{mime}\" isEmbeded=\"1\"/>"
        ));
    }
    let content_hpf = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>",
            "<opf:package xmlns:opf=\"http://www.idpf.org/2007/opf/\">",
            "<opf:metadata><opf:title>HWPX 확장 검증</opf:title></opf:metadata>",
            "<opf:manifest>",
            "<opf:item id=\"header\" href=\"Contents/header.xml\" media-type=\"application/xml\"/>",
            "{}",
            "</opf:manifest></opf:package>"
        ),
        manifest_items
    );
    let mut entries = vec![
        ("mimetype".to_string(), b"application/hwp+zip".to_vec()),
        ("Contents/content.hpf".to_string(), content_hpf.into_bytes()),
        (
            "Contents/header.xml".to_string(),
            HEADER_XML.as_bytes().to_vec(),
        ),
    ];
    for (path, body) in sections {
        let xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><hs:sec xmlns:hs=\"http://www.owpml.org/owpml/2011/section\" xmlns:hp=\"http://www.owpml.org/owpml/2011/paragraph\" xmlns:hc=\"http://www.owpml.org/owpml/2011/core\">{body}</hs:sec>"
        );
        entries.push(((*path).to_string(), xml.into_bytes()));
    }
    for (_, href, bytes) in media {
        entries.push(((*href).to_string(), bytes.clone()));
    }
    ZipArchiveCodec::pack_compressed(&entries).unwrap()
}

fn decode_rich_hwpx(bytes: &[u8]) -> (RichDocument, Vec<String>) {
    let decoded = UniversalFormatConverter::decode_rich(bytes, Some("doc.hwpx")).unwrap();
    assert_eq!(decoded.format, DocumentFormat::Hwpx);
    let DecodedContent::Document(doc) = decoded.content else {
        panic!("expected document");
    };
    (doc, decoded.warnings)
}

#[test]
fn shapes_grouped_containers_textart_and_captions_keep_document_order_and_formatting() {
    let png = make_png(20, 10, 2, 3);
    let sec = concat!(
        "<hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\">",
        "<hp:t>도형 앞 문장</hp:t>",
        "<hp:rect><hp:drawText><hp:subList>",
        "<hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"1\"><hp:t>글상자 굵은 제목</hp:t></hp:run></hp:p>",
        "</hp:subList></hp:drawText>",
        "<hp:caption><hp:subList><hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:t>사각형 캡션</hp:t></hp:run></hp:p></hp:subList></hp:caption>",
        "</hp:rect>",
        "<hp:t>도형 뒤 문장</hp:t>",
        "</hp:run></hp:p>",
        "<hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\">",
        "<hp:container>",
        "<hp:pic><hp:curSz width=\"1000\" height=\"500\"/><hc:img binaryItemIDRef=\"img1\"/><hp:shapeComment>묶음 안 그림</hp:shapeComment></hp:pic>",
        "<hp:ellipse><hp:drawText><hp:subList><hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:t>타원 안내</hp:t></hp:run></hp:p></hp:subList></hp:drawText></hp:ellipse>",
        "<hp:textart text=\"글맵시 배너\"/>",
        "</hp:container>",
        "<hp:line/>",
        "</hp:run></hp:p>",
    );
    let bytes = build_hwpx(
        &[("Contents/section0.xml", sec)],
        &[("img1", "BinData/image1.png", png.clone())],
    );
    let (doc, warnings) = decode_rich_hwpx(&bytes);
    assert_eq!(
        warnings,
        vec!["글이 없는 도형은 가져오지 않았습니다.".to_string()]
    );

    let summary: Vec<String> = doc
        .blocks
        .iter()
        .map(|b| match b {
            RichBlock::Paragraph(p) => format!("P({}):{}", p.runs[0].bold, p.plain_text()),
            RichBlock::Image(i) => format!("I:{}:{:?}", i.alt(), i.display_emu()),
            RichBlock::Table(_) => "T".to_string(),
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            "P(false):도형 앞 문장",
            "P(true):글상자 굵은 제목",
            "P(false):사각형 캡션",
            "P(false):도형 뒤 문장",
            "I:묶음 안 그림:(127000, 63500)",
            "P(false):타원 안내",
            "P(false):글맵시 배너",
        ]
    );

    let flat = HwpxCodec::unpack(&bytes).unwrap();
    assert!(flat.paragraphs.iter().any(|p| p == "글상자 굵은 제목"));
    assert!(flat.paragraphs.iter().any(|p| p == "글맵시 배너"));
}

#[test]
fn equations_and_table_cell_pictures_preserve_across_rich_decode_and_docx_pages_conversion() {
    let png = make_png(16, 12, 2, 3);
    let jpg = make_jpeg(12, 8);
    let sec = concat!(
        "<hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\">",
        "<hp:t>공식: </hp:t>",
        "<hp:equation lineMode=\"CHAR\"><hp:script>{a + 1} over {b - 1}</hp:script></hp:equation>",
        "</hp:run></hp:p>",
        "<hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\">",
        "<hp:tbl rowCnt=\"1\" colCnt=\"2\">",
        "<hp:caption><hp:subList><hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:t>표 1. 실험 결과</hp:t></hp:run></hp:p></hp:subList></hp:caption>",
        "<hp:tr>",
        "<hp:tc><hp:subList><hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\">",
        "<hp:t>수식 셀 </hp:t><hp:equation><hp:script>sqrt {x^2 + 1}</hp:script></hp:equation>",
        "<hp:pic><hp:curSz width=\"2000\" height=\"1500\"/><hc:img binaryItemIDRef=\"img1\"/><hp:shapeComment>셀 첫 그림</hp:shapeComment></hp:pic>",
        "</hp:run></hp:p></hp:subList><hp:cellAddr rowAddr=\"0\" colAddr=\"0\"/></hp:tc>",
        "<hp:tc><hp:subList><hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\">",
        "<hp:pic><hc:img binaryItemIDRef=\"img2\"/><hp:caption><hp:subList><hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:t>캡션 대체 설명</hp:t></hp:run></hp:p></hp:subList></hp:caption></hp:pic>",
        "</hp:run></hp:p></hp:subList><hp:cellAddr rowAddr=\"0\" colAddr=\"1\"/></hp:tc>",
        "</hp:tr></hp:tbl>",
        "</hp:run></hp:p>",
    );
    let bytes = build_hwpx(
        &[("Contents/section0.xml", sec)],
        &[
            ("img1", "BinData/pic1.png", png.clone()),
            ("img2", "BinData/pic2.jpg", jpg.clone()),
        ],
    );
    let (doc, warnings) = decode_rich_hwpx(&bytes);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(doc.blocks.len(), 3);

    let RichBlock::Paragraph(p0) = &doc.blocks[0] else {
        panic!("para 0");
    };
    assert_eq!(p0.plain_text(), "공식: $$\\frac{a + 1}{b - 1}$$");

    let RichBlock::Table(tbl) = &doc.blocks[1] else {
        panic!("table");
    };
    assert_eq!(tbl.rows[0][0], "수식 셀 $$\\sqrt{x^2 + 1}$$");
    assert_eq!(tbl.rows[0][1], "캡션 대체 설명");
    assert_eq!(tbl.cell_images.len(), 2);

    assert_eq!((tbl.cell_images[0].row, tbl.cell_images[0].col), (0, 0));
    assert_eq!(tbl.cell_images[0].image.bytes(), png.as_slice());
    assert_eq!(tbl.cell_images[0].image.alt(), "셀 첫 그림");
    assert_eq!(
        tbl.cell_images[0].image.display_emu(),
        (2000 * 127, 1500 * 127)
    );

    assert_eq!((tbl.cell_images[1].row, tbl.cell_images[1].col), (0, 1));
    assert_eq!(tbl.cell_images[1].image.bytes(), jpg.as_slice());
    assert_eq!(tbl.cell_images[1].image.alt(), "캡션 대체 설명");

    let RichBlock::Paragraph(tcap) = &doc.blocks[2] else {
        panic!("table caption");
    };
    assert_eq!(tcap.plain_text(), "표 1. 실험 결과");

    // Verify end-to-end conversion from HWPX into DOCX preserves cell_images and Pages preserves total images!
    for target in [DocumentFormat::Docx, DocumentFormat::Pages] {
        let converted =
            UniversalFormatConverter::convert(&bytes, Some("hwpx"), target).expect("convert");
        let back =
            UniversalFormatConverter::decode_rich(&converted, Some(target.extension())).unwrap();
        let DecodedContent::Document(back_doc) = back.content else {
            panic!();
        };
        if target == DocumentFormat::Docx {
            let back_tbl = back_doc
                .blocks
                .iter()
                .find_map(|b| match b {
                    RichBlock::Table(t) => Some(t),
                    _ => None,
                })
                .expect("converted DOCX table");
            assert_eq!(
                back_tbl.cell_images.len(),
                2,
                "Docx kept both table-cell pictures in tbl.cell_images"
            );
        } else {
            let total_images: usize = back_doc
                .blocks
                .iter()
                .map(|b| match b {
                    RichBlock::Image(_) => 1,
                    RichBlock::Table(t) => t.cell_images.len(),
                    RichBlock::Paragraph(_) => 0,
                })
                .sum();
            assert_eq!(
                total_images, 2,
                "{target:?} kept both table-cell pictures (top-level + cell_images)"
            );
        }
        assert!(back_doc.blocks.iter().any(
            |b| matches!(b, RichBlock::Paragraph(p) if p.plain_text().contains("\\frac{a + 1}{b - 1}"))
        ));
    }
}

#[test]
fn footnotes_and_endnotes_preserve_anchor_marks_section_restart_and_appendix_blocks() {
    let sec0 = concat!(
        "<hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\">",
        "<hp:secPr><hp:footNotePr>",
        "<hp:autoNumFormat type=\"CIRCLED_DIGIT\" suffixChar=\")\"/>",
        "<hp:numbering type=\"ON_SECTION\" newNum=\"1\"/>",
        "</hp:footNotePr></hp:secPr>",
        "<hp:t>첫 구역 본문</hp:t>",
        "<hp:ctrl><hp:footNote><hp:subList>",
        "<hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:ctrl><hp:autoNum numType=\"FOOTNOTE\"/></hp:ctrl><hp:t>첫 각주 설명</hp:t></hp:run></hp:p>",
        "</hp:subList></hp:footNote></hp:ctrl>",
        "<hp:t> 이어짐</hp:t>",
        "<hp:ctrl><hp:endNote number=\"1\" prefixChar=\"47928\" suffixChar=\"65289\"><hp:subList>",
        "<hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"1\"><hp:t>미주 참고문헌</hp:t></hp:run></hp:p>",
        "</hp:subList></hp:endNote></hp:ctrl>",
        "</hp:run></hp:p>"
    );
    let sec1 = concat!(
        "<hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\">",
        "<hp:secPr><hp:footNotePr>",
        "<hp:autoNumFormat type=\"CIRCLED_DIGIT\" suffixChar=\")\"/>",
        "<hp:numbering type=\"ON_SECTION\" newNum=\"1\"/>",
        "</hp:footNotePr></hp:secPr>",
        "<hp:t>둘째 구역</hp:t>",
        "<hp:ctrl><hp:footNote><hp:subList>",
        "<hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:t>재시작 각주</hp:t></hp:run></hp:p>",
        "</hp:subList></hp:footNote></hp:ctrl>",
        "</hp:run></hp:p>"
    );
    let bytes = build_hwpx(
        &[
            ("Contents/section0.xml", sec0),
            ("Contents/section1.xml", sec1),
        ],
        &[],
    );
    let (doc, warnings) = decode_rich_hwpx(&bytes);
    assert!(warnings.is_empty(), "{warnings:?}");

    let RichBlock::Paragraph(b0) = &doc.blocks[0] else {
        panic!();
    };
    assert_eq!(b0.plain_text(), "첫 구역 본문①) 이어짐문1）");
    assert_eq!(b0.runs[1].script, RichScript::Superscript);
    assert_eq!(b0.runs[1].text, "①)");
    assert_eq!(b0.runs[3].script, RichScript::Superscript);
    assert_eq!(b0.runs[3].text, "문1）");

    let RichBlock::Paragraph(b1) = &doc.blocks[1] else {
        panic!();
    };
    assert_eq!(b1.plain_text(), "둘째 구역①)", "ON_SECTION restarted at ①)");

    let RichBlock::Paragraph(fn_h) = &doc.blocks[2] else {
        panic!();
    };
    assert_eq!(
        (fn_h.heading_level, fn_h.plain_text().as_str()),
        (2, "각주")
    );
    let RichBlock::Paragraph(fn_1) = &doc.blocks[3] else {
        panic!();
    };
    assert_eq!(fn_1.plain_text(), "①) 첫 각주 설명");
    let RichBlock::Paragraph(fn_2) = &doc.blocks[4] else {
        panic!();
    };
    assert_eq!(fn_2.plain_text(), "①) 재시작 각주");

    let RichBlock::Paragraph(en_h) = &doc.blocks[5] else {
        panic!();
    };
    assert_eq!(
        (en_h.heading_level, en_h.plain_text().as_str()),
        (2, "미주")
    );
    let RichBlock::Paragraph(en_1) = &doc.blocks[6] else {
        panic!();
    };
    assert_eq!(en_1.plain_text(), "문1） 미주 참고문헌");
    assert!(en_1.runs[2].bold);

    // Flat unpack must NOT splice footnote text into the middle of the body sentence!
    let flat = HwpxCodec::unpack(&bytes).unwrap();
    assert_eq!(flat.paragraphs[0], "첫 구역 본문 이어짐");
    assert!(flat.paragraphs.contains(&"첫 각주 설명".to_string()));
    assert!(flat.paragraphs.contains(&"미주 참고문헌".to_string()));
}

#[test]
fn broken_equations_and_excess_container_nesting_fail_closed_in_rich_hwpx_reader() {
    let broken_eq = build_hwpx(
        &[(
            "Contents/section0.xml",
            "<hp:p><hp:run><hp:equation><hp:script>sqrt { x + 1</hp:script></hp:equation></hp:run></hp:p>",
        )],
        &[],
    );
    assert!(matches!(
        UniversalFormatConverter::decode_rich(&broken_eq, Some("a.hwpx")),
        Err(EasyOfficeError::CorruptedContainer(_))
    ));

    let huge_eq = format!(
        "<hp:p><hp:run><hp:equation><hp:script>{}</hp:script></hp:equation></hp:run></hp:p>",
        "x + ".repeat(20_000)
    );
    let huge_eq_bytes = build_hwpx(&[("Contents/section0.xml", &huge_eq)], &[]);
    assert!(matches!(
        UniversalFormatConverter::decode_rich(&huge_eq_bytes, Some("a.hwpx")),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));

    let deep_containers = format!(
        "<hp:p><hp:run>{}<hp:textart text=\"x\"/>{}</hp:run></hp:p>",
        "<hp:container>".repeat(18),
        "</hp:container>".repeat(18)
    );
    let deep_bytes = build_hwpx(&[("Contents/section0.xml", &deep_containers)], &[]);
    assert!(matches!(
        UniversalFormatConverter::decode_rich(&deep_bytes, Some("a.hwpx")),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn hwpx_rich_codec_benchmark_measures_decode_throughput() {
    let png = make_png(16, 16, 2, 3);
    let mut body = String::new();
    for i in 0..30 {
        body.push_str(&format!(
            concat!(
                "<hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\">",
                "<hp:t>문단 {i} </hp:t>",
                "<hp:equation><hp:script>sum from k=1 to n x_k</hp:script></hp:equation>",
                "<hp:rect><hp:drawText><hp:subList><hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"1\"><hp:t>글상자 {i}</hp:t></hp:run></hp:p></hp:subList></hp:drawText></hp:rect>",
                "<hp:ctrl><hp:footNote><hp:subList><hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:t>각주 {i}</hp:t></hp:run></hp:p></hp:subList></hp:footNote></hp:ctrl>",
                "</hp:run></hp:p>"
            ),
            i = i
        ));
    }
    body.push_str(
        "<hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:tbl rowCnt=\"1\" colCnt=\"1\"><hp:tr><hp:tc><hp:subList><hp:p paraPrIDRef=\"0\"><hp:run charPrIDRef=\"0\"><hp:pic><hc:img binaryItemIDRef=\"img1\"/></hp:pic></hp:run></hp:p></hp:subList><hp:cellAddr rowAddr=\"0\" colAddr=\"0\"/></hp:tc></hp:tr></hp:tbl></hp:run></hp:p>",
    );
    let bytes = build_hwpx(
        &[("Contents/section0.xml", &body)],
        &[("img1", "BinData/p.png", png)],
    );
    let iters = 30u32;
    let t0 = Instant::now();
    for _ in 0..iters {
        let (doc, _) = decode_rich_hwpx(&bytes);
        assert!(doc.blocks.len() >= 90);
    }
    let avg_us = t0.elapsed().as_micros() as f64 / f64::from(iters);
    println!(
        "hwpx_rich_codec_benchmark: size={}B decode_rich_avg={avg_us:.2}us",
        bytes.len()
    );
}
