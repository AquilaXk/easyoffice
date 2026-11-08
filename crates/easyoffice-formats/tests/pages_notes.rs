//! Pages footnotes, headers, footers and shape text: footnote numbering at the marks, pictures
//! in nested text, duplicate header pictures counted once, hostile footnote tables. Synthetic
//! packages only.

#[allow(dead_code)]
mod image_fixture;
mod pages_fixture;
mod pages_picture_fixture;

use easyoffice_formats::{EasyOfficeError, RichBlock, RichScript};
use image_fixture::make_png;
use pages_fixture::*;
use pages_picture_fixture::*;

const FFFC: &str = "\u{FFFC}";

/// Footnote object (type 2012) with a one-paragraph text storage.
fn footnote(b: &mut DocBuilder, text: &str) -> u64 {
    let st = b.add(2001, &DocBuilder::storage_payload(text, &[], &[], &[], &[]));
    b.add(2012, &f_ref(2, st))
}

/// Body storage with a footnote table `(character index, footnote id)`.
fn body_with_footnotes(b: &mut DocBuilder, text: &str, marks: &[(u64, u64)]) -> u64 {
    let table: Vec<(u64, Option<u64>)> = marks.iter().map(|(i, f)| (*i, Some(*f))).collect();
    let payload = cat(&[
        DocBuilder::storage_payload(text, &[], &[], &[], &[]),
        f_bytes(11, &DocBuilder::attr_table(&table)),
    ]);
    b.add(2001, &payload)
}

/// Storage whose first line is `text` and whose second is an anchor for `att`.
fn storage_with_anchor(b: &mut DocBuilder, text: &str, att: u64) -> u64 {
    let anchor = utf16_len(text) + 1;
    b.add(
        2001,
        &DocBuilder::storage_payload(
            &format!("{text}\n{FFFC}"),
            &[],
            &[],
            &[],
            &[(anchor, Some(att))],
        ),
    )
}

fn png_zip(b: DocBuilder, body: u64) -> Vec<u8> {
    pages_zip_with_data(
        b.finish(body),
        vec![("Data/pic.bin".to_string(), make_png(4, 2, 2, 3))],
    )
}

fn header_list(headers: &[u64], footers: &[u64]) -> Vec<u8> {
    let mut parts: Vec<Vec<u8>> = headers
        .iter()
        .map(|s| f_bytes(1, &f_varint(1, *s)))
        .collect();
    parts.extend(footers.iter().map(|s| f_bytes(2, &f_varint(1, *s))));
    cat(&parts)
}

fn image_count(doc: &easyoffice_formats::RichDocument) -> usize {
    images(doc).len()
}

#[test]
fn footnote_numbers_follow_the_marks_in_text_and_appendix() {
    let mut b = DocBuilder::default();
    let first = footnote(&mut b, "첫째 주석");
    let second = footnote(&mut b, "둘째 주석");
    let text = format!("가{FFFC}나{FFFC}다");
    let body = body_with_footnotes(&mut b, &text, &[(1, first), (3, second)]);
    let (doc, warnings) = open(&pages_zip(b.finish(body))).expect("opens");
    let RichBlock::Paragraph(p) = &doc.blocks[0] else {
        panic!("paragraph")
    };
    let runs: Vec<(&str, RichScript)> =
        p.runs.iter().map(|r| (r.text.as_str(), r.script)).collect();
    assert_eq!(
        runs,
        vec![
            ("가", RichScript::Normal),
            ("1", RichScript::Superscript),
            ("나", RichScript::Normal),
            ("2", RichScript::Superscript),
            ("다", RichScript::Normal),
        ]
    );
    assert_eq!(
        paragraph_texts(&doc),
        vec!["가1나2다", "각주", "1 첫째 주석", "2 둘째 주석"]
    );
    assert!(!warned(&warnings, "가져오지 않은 요소"), "{warnings:?}");
}

#[test]
fn footnote_with_a_picture_keeps_it_under_its_number() {
    let mut b = DocBuilder::default();
    metadata(&mut b, &[(1, "pic.bin")]);
    let img = picture(&mut b, &Picture::default());
    let att = b.attachment(img);
    let st = b.add(
        2001,
        &DocBuilder::storage_payload(FFFC, &[], &[], &[], &[(0, Some(att))]),
    );
    let note = b.add(2012, &f_ref(2, st));
    let body = body_with_footnotes(&mut b, &format!("가{FFFC}"), &[(1, note)]);
    let (doc, _) = open(&png_zip(b, body)).expect("opens");
    assert_eq!(paragraph_texts(&doc), vec!["가1", "각주", "1 "]);
    assert_eq!(image_count(&doc), 1);
    assert!(matches!(doc.blocks.last(), Some(RichBlock::Image(_))));
}

#[test]
fn shape_text_keeps_pictures_and_reports_nested_tables_and_shapes() {
    let mut b = DocBuilder::default();
    metadata(&mut b, &[(1, "pic.bin")]);
    let img = picture(&mut b, &Picture::default());
    let img_att = b.attachment(img);
    let table = b.table(&[vec!["A"]]);
    let table_att = b.attachment(table);
    let inner_shape = b.add(2011, &[]);
    let inner_att = b.attachment(inner_shape);
    let text = format!("도형 글\n{FFFC}{FFFC}{FFFC}");
    let at = utf16_len("도형 글\n");
    let st = b.add(
        2001,
        &DocBuilder::storage_payload(
            &text,
            &[],
            &[],
            &[],
            &[
                (at, Some(img_att)),
                (at + 1, Some(table_att)),
                (at + 2, Some(inner_att)),
            ],
        ),
    );
    let shape = b.add(2011, &f_ref(2, st));
    let shape_att = b.attachment(shape);
    let body = body_with(&mut b, "앞", &[shape_att], "뒤");
    let (doc, warnings) = open(&png_zip(b, body)).expect("opens");
    assert_eq!(paragraph_texts(&doc), vec!["앞", "도형 글", "뒤"]);
    assert_eq!(image_count(&doc), 1);
    assert!(
        warned(&warnings, "도형·각주·머리글·바닥글 안의 표·도형 2개"),
        "{warnings:?}"
    );
    assert!(!warned(&warnings, "표 안의 개체"), "{warnings:?}");
}

#[test]
fn header_pictures_repeated_in_every_variant_count_once() {
    // 300 header storages each carry the same logo: more than the 256-picture document limit
    // if every copy were counted, but they collapse into a single appendix picture.
    let mut b = DocBuilder::default();
    metadata(&mut b, &[(1, "pic.bin")]);
    let mut headers = Vec::new();
    for _ in 0..300 {
        let img = picture(&mut b, &Picture::default());
        let att = b.attachment(img);
        headers.push(storage_with_anchor(&mut b, "머리말", att));
    }
    b.add(10143, &header_list(&headers, &[]));
    let body = body_with(&mut b, "앞", &[], "뒤");
    let (doc, warnings) = open(&png_zip(b, body)).expect("opens");
    assert_eq!(image_count(&doc), 1);
    assert_eq!(paragraph_texts(&doc), vec!["앞", "뒤", "머리글", "머리말"]);
    assert!(!warned(&warnings, "가져오지 않은 요소"), "{warnings:?}");
}

#[test]
fn damaged_header_picture_is_reported_but_page_number_marks_are_not() {
    let mut b = DocBuilder::default();
    metadata(&mut b, &[(1, "pic.bin")]);
    let bad = picture(
        &mut b,
        &Picture {
            data_id: Some(1),
            ..Picture::default()
        },
    );
    let bad_att = b.attachment(bad);
    let mark = b.add(5555, &[]);
    let mark_att = b.attachment(mark);
    let st = b.add(
        2001,
        &DocBuilder::storage_payload(
            &format!("쪽 {FFFC}{FFFC}"),
            &[],
            &[],
            &[],
            &[
                (utf16_len("쪽 "), Some(mark_att)),
                (utf16_len("쪽 ") + 1, Some(bad_att)),
            ],
        ),
    );
    b.add(10143, &header_list(&[], &[st]));
    let body = body_with(&mut b, "앞", &[], "뒤");
    let damaged = pages_zip_with_data(
        b.finish(body),
        vec![(
            "Data/pic.bin".to_string(),
            b"\x89PNG\r\n\x1a\n-truncated".to_vec(),
        )],
    );
    let (doc, warnings) = open(&damaged).expect("opens");
    assert_eq!(image_count(&doc), 0);
    assert!(
        warned(&warnings, "손상되었거나 한도를 넘은 PNG·JPEG 그림 1개"),
        "{warnings:?}"
    );
    assert!(!warned(&warnings, "5555"), "{warnings:?}");
    assert!(!warned(&warnings, "알 수 없는"), "{warnings:?}");
}

#[test]
fn a_flood_of_footnote_entries_fails_closed() {
    // 65,000 table entries (below the per-message field cap) naming one four-paragraph
    // footnote: 260,000 paragraphs against the 200,000 limit.
    let mut b = DocBuilder::default();
    let note = footnote(&mut b, "가\n나\n다\n라");
    let marks: Vec<(u64, u64)> = (0..65_000u64).map(|i| (i, note)).collect();
    let body = body_with_footnotes(&mut b, "본문", &marks);
    assert!(matches!(
        open(&pages_zip(b.finish(body))),
        Err(EasyOfficeError::ResourceBudgetExceeded(_))
    ));
}

#[test]
fn footnote_marks_without_matching_text_or_object_do_not_break_reading() {
    let mut b = DocBuilder::default();
    let note = footnote(&mut b, "주석");
    // Marks far beyond the text, a mark on a character that is not an object slot, and a
    // reference to a missing footnote.
    let body = body_with_footnotes(
        &mut b,
        &format!("가나{FFFC}"),
        &[(0, note), (2, note), (999_999, note), (5, 424_242)],
    );
    let (doc, _) = open(&pages_zip(b.finish(body))).expect("opens");
    let texts = paragraph_texts(&doc);
    // The mark is the second table entry; the number is the footnote's place in the table.
    assert_eq!(texts[0], "가나2");
    assert!(texts.contains(&"각주".to_string()));
    assert!(texts.contains(&"1 주석".to_string()) && texts.contains(&"2 주석".to_string()));
}
