//! PD-312 Pages defaults (Korean ISO A4 template) and multi-section `frameCopy` snapshot tests.

mod pages_fixture;

use easyoffice_formats::pages_iwa::{decompress_iwa, split_records, IwaRecord};
use easyoffice_formats::pages_proto::Message;
use easyoffice_formats::{
    pages_write_loss, DecodedContent, DocumentFormat, PagesWriteLoss, RichBlock, RichDocument,
    RichPageMargins, RichParagraph, RichRun, UniversalFormatConverter, ZipArchiveCodec,
    ZipSecurityPolicy,
};
use pages_fixture::{f_bytes, f_ref, f_varint, pages_zip, DocBuilder};

fn plain_st(b: &mut DocBuilder, text: &str) -> u64 {
    b.add(2001, &DocBuilder::storage_payload(text, &[], &[], &[], &[]))
}

fn table_field(field: u32, entries: &[(u64, Option<u64>)]) -> Vec<u8> {
    f_bytes(field, &DocBuilder::attr_table(entries))
}

fn all_iwa_records(bytes: &[u8]) -> Vec<IwaRecord> {
    let mut all = Vec::new();
    for entry in
        ZipArchiveCodec::unpack(bytes, &ZipSecurityPolicy::default()).expect("unpack pages")
    {
        if entry.path.starts_with("Index/") && entry.path.ends_with(".iwa") {
            let mut budget = usize::MAX;
            let stream = decompress_iwa(&entry.data, &mut budget).expect("decompress iwa");
            all.extend(split_records(&stream).expect("split records"));
        }
    }
    all
}

fn body_texts(doc: &RichDocument) -> Vec<String> {
    doc.body_blocks()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => Some(p.plain_text()),
            _ => None,
        })
        .collect()
}

fn assert_approx(actual: Option<f32>, expected: f32, tol: f32, label: &str) {
    let v = actual.unwrap_or_else(|| panic!("missing float for {label}"));
    assert!(
        (v - expected).abs() <= tol,
        "{label}: expected {expected} +/- {tol}, got {v}"
    );
}

#[test]
fn default_pages_package_matches_korean_iso_a4_template() {
    let mut blocks = vec![RichBlock::Paragraph(RichParagraph {
        runs: vec![RichRun::plain("본문 가나다")],
        ..RichParagraph::default()
    })];
    for level in 1..=6u8 {
        blocks.push(RichBlock::Paragraph(RichParagraph {
            heading_level: level,
            runs: vec![RichRun::plain(format!("제목 {level}"))],
            ..RichParagraph::default()
        }));
    }
    let mut doc = RichDocument {
        title: "기본 Pages 문서".to_string(),
        blocks,
        ..RichDocument::default()
    };

    assert_eq!(pages_write_loss(&doc), PagesWriteLoss::default());
    doc.page_frame.margins = Some(RichPageMargins::PAGES_DEFAULT);
    assert_eq!(pages_write_loss(&doc), PagesWriteLoss::default());

    let bytes = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Pages, None)
        .expect("encode pages");
    let records = all_iwa_records(&bytes);

    let root_rec = records
        .iter()
        .find(|r| r.info.identifier == 1 && r.info.messages[0].type_id == 10000)
        .expect("root TP.DocumentArchive");
    let root = Message::parse(&root_rec.payloads[0]).expect("parse root");
    assert_approx(root.float(30), 595.28, 1e-2, "page width");
    assert_approx(root.float(31), 841.89, 1e-2, "page height");
    for (field, label) in [
        (32, "left margin"),
        (33, "right margin"),
        (34, "top margin"),
        (35, "bottom margin"),
    ] {
        assert_approx(root.float(field), 56.692_913, 1e-3, label);
    }
    assert_approx(root.float(36), 35.433_07, 1e-3, "header margin");
    assert_approx(root.float(37), 42.519_684, 1e-3, "footer margin");
    assert_eq!(
        root.string(44).expect("paper name").as_deref(),
        Some("iso-a4")
    );

    let mut style_map = std::collections::BTreeMap::new();
    for rec in records
        .iter()
        .filter(|r| r.info.messages[0].type_id == 2022)
    {
        let msg = Message::parse(&rec.payloads[0]).expect("parse style");
        if let Some(hdr) = msg.sub(1).expect("header sub") {
            if let Some(id) = hdr.string(2).expect("style id") {
                style_map.insert(id, rec.payloads[0].as_slice());
            }
        }
    }

    let check_style = |id: &str, bold: u64, size: f32, font: &str| {
        let payload = style_map.get(id).unwrap_or_else(|| {
            panic!(
                "missing style {id}: {:?}",
                style_map.keys().collect::<Vec<_>>()
            )
        });
        let msg = Message::parse(payload).expect("style payload");
        assert_eq!(msg.varint(10), Some(9), "{id} override_count");
        let char_bag = msg.sub(11).expect("char bag sub").expect("char bag");
        assert_eq!(char_bag.varint(1), Some(bold), "{id} bold");
        assert_approx(char_bag.float(3), size, 1e-3, id);
        assert_eq!(
            char_bag.string(5).expect("font").as_deref(),
            Some(font),
            "{id} font"
        );
        let para_bag = msg.sub(12).expect("para bag sub").expect("para bag");
        assert_approx(para_bag.float(4), 36.0, 1e-3, id);
    };

    check_style(
        "text-0-paragraphstyle-Body",
        0,
        11.0,
        "AppleSDGothicNeo-Regular",
    );
    for (idx, pt) in [18.0, 16.0, 14.0, 13.0, 12.0, 11.0].into_iter().enumerate() {
        let id = format!("text-0-paragraphstyle-Heading {}", idx + 1);
        check_style(&id, 1, pt, "AppleSDGothicNeo-Bold");
    }

    let DecodedContent::Document(reopened) =
        UniversalFormatConverter::decode_rich(&bytes, Some("default.pages"))
            .expect("decode pages")
            .content
    else {
        panic!("expected document");
    };
    assert_eq!(
        reopened.page_frame.margins,
        Some(RichPageMargins::PAGES_DEFAULT)
    );
    let RichBlock::Paragraph(first_para) = &reopened.blocks[0] else {
        panic!("first block must be paragraph");
    };
    assert_eq!(first_para.plain_text(), "본문 가나다");
    assert_eq!(first_para.runs[0].size_half_pt, Some(22));
    assert_eq!(first_para.runs[0].font, None);
}

fn two_section_distinct_headers_zip() -> Vec<u8> {
    let mut b = DocBuilder::default();
    let hdr0 = plain_st(&mut b, "1구역 머리글");
    let hdr1 = plain_st(&mut b, "2구역 머리글");
    let sec0_tpl = b.add(
        10143,
        &pages_fixture::cat(&[f_ref(1, hdr0), f_ref(1, 0), f_ref(1, 0)]),
    );
    let sec1_tpl = b.add(
        10143,
        &pages_fixture::cat(&[f_ref(1, hdr1), f_ref(1, 0), f_ref(1, 0)]),
    );
    let sec0 = b.add(10011, &f_ref(25, sec0_tpl));
    let sec1 = b.add(
        10011,
        &pages_fixture::cat(&[f_varint(17, 0), f_ref(25, sec1_tpl)]),
    );
    let body = b.add(
        2001,
        &pages_fixture::cat(&[
            DocBuilder::storage_payload("일장\u{4}이장", &[], &[], &[], &[]),
            table_field(17, &[(0, Some(sec0)), (3, Some(sec1))]),
        ]),
    );
    pages_zip(b.finish(body))
}

#[test]
fn multi_section_distinct_headers_carry_snapshot_and_do_not_accumulate() {
    let zip = two_section_distinct_headers_zip();
    let DecodedContent::Document(mut doc) =
        UniversalFormatConverter::decode_rich(&zip, Some("two.pages"))
            .expect("decode 2-section pages")
            .content
    else {
        panic!("expected document");
    };
    assert_eq!(doc.page_frame.header.len(), 1);
    assert_eq!(doc.page_frame.header[0].plain_text(), "1구역 머리글");

    let copies: Vec<&str> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            RichBlock::Paragraph(p) => p.frame_copy.as_deref(),
            _ => None,
        })
        .collect();
    assert!(copies.contains(&"머리글"), "{copies:?}");
    assert!(copies.contains(&"1구역 머리글"), "{copies:?}");
    assert!(copies.contains(&"2구역 머리글"), "{copies:?}");
    assert_eq!(body_texts(&doc), vec!["일장", "이장"]);

    for generation in 1..=2 {
        let saved = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Pages, None)
            .expect("encode pages");
        let DecodedContent::Document(next) =
            UniversalFormatConverter::decode_rich(&saved, Some("next.pages"))
                .expect("decode next pages")
                .content
        else {
            panic!("expected next document");
        };
        assert_eq!(
            body_texts(&next),
            vec!["일장", "이장"],
            "generation {generation} accumulated header copy into body"
        );
        doc = next;
    }
}

#[test]
fn edited_second_section_frame_copy_is_saved_as_body_paragraph() {
    let zip = two_section_distinct_headers_zip();
    let DecodedContent::Document(mut doc) =
        UniversalFormatConverter::decode_rich(&zip, Some("two.pages"))
            .expect("decode 2-section pages")
            .content
    else {
        panic!("expected document");
    };

    let target = doc
        .blocks
        .iter_mut()
        .find_map(|b| match b {
            RichBlock::Paragraph(p) if p.frame_copy.as_deref() == Some("2구역 머리글") => {
                Some(p)
            }
            _ => None,
        })
        .expect("missing 2구역 머리글 appendix paragraph");
    target.runs = vec![RichRun::plain("2구역 본문으로 편집")];

    let saved = UniversalFormatConverter::encode_rich(&doc, DocumentFormat::Pages, None)
        .expect("encode edited pages");
    let DecodedContent::Document(reopened) =
        UniversalFormatConverter::decode_rich(&saved, Some("edited.pages"))
            .expect("decode edited pages")
            .content
    else {
        panic!("expected reopened document");
    };
    assert_eq!(
        body_texts(&reopened),
        vec!["일장", "이장", "2구역 본문으로 편집"]
    );
}
