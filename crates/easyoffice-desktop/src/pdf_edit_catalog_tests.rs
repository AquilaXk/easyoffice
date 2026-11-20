//! Page labels, attachments and the metadata stream across an edit (PD-266): what the catalog of a
//! file holds besides its pages is written again for the pages that are kept.

use crate::pdf_edit_budget::{MAX_ATTACHMENTS, MAX_LABEL_RANGES, MAX_XMP_BYTES};
use crate::pdf_edit_carry_tests::{one_page, rebuilt};
use crate::pdf_edit_engine::InProcessEngine;
use crate::pdf_edit_fixtures::{hand_written_with_xref, read};
use crate::pdf_edit_session::EditSession;
use hayro_syntax::object::{Array, Dict, Name, Object, Stream};
use hayro_syntax::Pdf as ReadPdf;
use std::sync::Arc;
use usvg::fontdb::Database;

fn engine() -> InProcessEngine {
    InProcessEngine::new(Arc::new(Database::new()))
}

/// A file of `count` empty pages; the objects of `more` are numbered from `3 + count`.
fn pages_file(count: usize, catalog: &str, more: &[String]) -> Vec<u8> {
    let kids: Vec<String> = (0..count).map(|n| format!("{} 0 R", 3 + n)).collect();
    let mut objects = vec![
        (1, format!("<< /Type /Catalog /Pages 2 0 R {catalog} >>")),
        (
            2,
            format!(
                "<< /Type /Pages /Kids [{}] /Count {count} >>",
                kids.join(" ")
            ),
        ),
    ];
    for n in 0..count {
        objects.push((
            3 + n as u32,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>".to_string(),
        ));
    }
    objects.extend(((3 + count) as u32..).zip(more.iter().cloned()));
    hand_written_with_xref(&objects, "")
}

fn catalog_of(pdf: &ReadPdf) -> Dict<'_> {
    pdf.xref().get::<Dict<'_>>(pdf.xref().root_id()).unwrap()
}

// ---- page labels --------------------------------------------------------------------------------

/// The label of every page, as the file says it, worked out here from the number tree: one string
/// `style|prefix|number` for each page.
fn labels_of(pdf: &ReadPdf) -> Vec<String> {
    fn walk(node: &Dict<'_>, ranges: &mut Vec<(usize, String, String, i32)>, depth: usize) {
        assert!(depth < 8, "the result's tree is shallow");
        if let Some(nums) = node.get::<Array<'_>>(b"Nums") {
            let mut items = nums.iter::<Object<'_>>();
            while let (Some(Object::Number(at)), Some(Object::Dict(label))) =
                (items.next(), items.next())
            {
                let text = |key: &[u8]| {
                    label
                        .get::<hayro_syntax::object::String<'_>>(key)
                        .map(|s| String::from_utf8_lossy(s.as_bytes()).into_owned())
                        .unwrap_or_default()
                };
                let style = label
                    .get::<Name<'_>>(b"S")
                    .map(|n| n.as_str().to_string())
                    .unwrap_or_default();
                ranges.push((
                    at.as_i64() as usize,
                    style,
                    text(b"P"),
                    label.get::<i32>(b"St").unwrap_or(1),
                ));
            }
        }
        if let Some(kids) = node.get::<Array<'_>>(b"Kids") {
            for kid in kids.iter::<Dict<'_>>() {
                walk(&kid, ranges, depth + 1);
            }
        }
    }
    let Some(tree) = catalog_of(pdf).get::<Dict<'_>>(b"PageLabels") else {
        return Vec::new();
    };
    let mut ranges = Vec::new();
    walk(&tree, &mut ranges, 0);
    ranges.sort_by_key(|r| r.0);
    (0..pdf.pages().len())
        .map(|page| {
            let (start, style, prefix, first) = ranges
                .iter()
                .rev()
                .find(|r| r.0 <= page)
                .expect("a range covers the page");
            format!("{style}|{prefix}|{}", *first as usize + page - start)
        })
        .collect()
}

const FOUR_PAGES_WITH_LABELS: &str =
    "/PageLabels << /Nums [0 << /S /r >> 2 << /S /D /P (A-) /St 5 >>] >>";

#[test]
fn page_labels_follow_the_pages_through_a_split() {
    let file = pages_file(4, FOUR_PAGES_WITH_LABELS, &[]);
    assert_eq!(
        labels_of(&read(&file)),
        ["r||1", "r||2", "D|A-|5", "D|A-|6"],
        "the oracle reads the source as expected"
    );
    let engine = engine();
    let session = EditSession::open(file, &engine).unwrap();
    let all = session.render(None, &engine).unwrap().bytes;
    assert_eq!(labels_of(&read(&all)), ["r||1", "r||2", "D|A-|5", "D|A-|6"]);
    // The pages 2 to 4: the label of the first one is not "i" any more.
    let part = session.render(Some(1..4), &engine).unwrap().bytes;
    assert_eq!(labels_of(&read(&part)), ["r||2", "D|A-|5", "D|A-|6"]);
    let tail = session.render(Some(3..4), &engine).unwrap().bytes;
    assert_eq!(labels_of(&read(&tail)), ["D|A-|6"]);
}

#[test]
fn merged_files_keep_each_files_labels_and_number_a_file_without_labels_in_decimal() {
    let engine = engine();
    let plain = pages_file(2, "", &[]);
    let lettered = pages_file(3, "/PageLabels << /Nums [0 << /S /a /P (x) >>] >>", &[]);
    let mut session = EditSession::open(plain, &engine).unwrap();
    session.merge(vec![lettered], &engine).unwrap();
    let out = session.render(None, &engine).unwrap().bytes;
    assert_eq!(
        labels_of(&read(&out)),
        ["D||1", "D||2", "a|x|1", "a|x|2", "a|x|3"]
    );
    // Labels that run on across the join are one range.
    let first = pages_file(2, "/PageLabels << /Nums [0 << /S /D >>] >>", &[]);
    let second = pages_file(2, "/PageLabels << /Nums [0 << /S /D /St 3 >>] >>", &[]);
    let mut session = EditSession::open(first, &engine).unwrap();
    session.merge(vec![second], &engine).unwrap();
    let out = read(&session.render(None, &engine).unwrap().bytes);
    assert_eq!(labels_of(&out), ["D||1", "D||2", "D||3", "D||4"]);
    let nums = catalog_of(&out)
        .get::<Dict<'_>>(b"PageLabels")
        .and_then(|t| t.get::<Array<'_>>(b"Nums"))
        .unwrap();
    assert_eq!(
        nums.iter::<Object<'_>>().count(),
        2,
        "one entry: a key and a dictionary"
    );
}

#[test]
fn a_file_without_labels_gets_none() {
    let out = read(&rebuilt(pages_file(2, "", &[])).bytes);
    assert!(!catalog_of(&out).contains_key(b"PageLabels"));
}

#[test]
fn a_number_tree_with_kids_is_read() {
    let tree = "/PageLabels 7 0 R";
    let more = vec![
        "<< /Kids [8 0 R 9 0 R] >>".to_string(),
        "<< /Limits [0 1] /Nums [0 << /S /R >>] >>".to_string(),
        "<< /Limits [2 3] /Nums [2 << /S /D /St 10 >>] >>".to_string(),
    ];
    let out = read(&rebuilt(pages_file(4, tree, &more)).bytes);
    assert_eq!(labels_of(&out), ["R||1", "R||2", "D||10", "D||11"]);
}

fn refused(file: Vec<u8>) -> String {
    let engine = engine();
    EditSession::open(file, &engine)
        .expect("the file opens")
        .render(None, &engine)
        .expect_err("the build refuses")
}

#[test]
fn hostile_number_trees_are_refused() {
    let looped = pages_file(1, "/PageLabels 4 0 R", &["<< /Kids [4 0 R] >>".to_string()]);
    assert!(refused(looped).contains("페이지 레이블"));

    // A chain of nodes deeper than the limit: object 4 holds 5, 5 holds 6, and so on.
    let depth = 60;
    let chain: Vec<String> = (0..depth)
        .map(|n| format!("<< /Kids [{} 0 R] >>", 5 + n))
        .collect();
    assert!(refused(pages_file(1, "/PageLabels 4 0 R", &chain)).contains("페이지 레이블"));

    let ranges: String = (0..=MAX_LABEL_RANGES)
        .map(|n| format!("{n} << /S /D >> "))
        .collect();
    let many = pages_file(1, &format!("/PageLabels << /Nums [{ranges}] >>"), &[]);
    assert!(refused(many).contains("페이지 레이블"));
}

// ---- attachments --------------------------------------------------------------------------------

fn spec(name: &str, data_object: u32) -> String {
    format!(
        "<< /Type /Filespec /F ({name}) /UF ({name}) /Desc (attached) /EF << /F {data_object} 0 R >> >>"
    )
}

fn embedded(data: &str) -> String {
    format!(
        "<< /Type /EmbeddedFile /Subtype /text#2Fplain /Params << /Size {len} >> /Length {len} >>\nstream\n{data}\nendstream",
        len = data.len()
    )
}

/// A page with two attachments, `a.txt` and `b.txt`.
fn with_attachments() -> Vec<u8> {
    let more = vec![
        spec("a.txt", 7),
        spec("b.txt", 8),
        embedded("hello"),
        embedded("world!"),
    ];
    one_page(
        "/Names << /EmbeddedFiles << /Names [(b.txt) 6 0 R (a.txt) 5 0 R] >> >>",
        "",
        &more,
    )
}

/// A key as text; a control character at its end (a renamed key) is kept.
fn renamed_key(key: &[u8]) -> String {
    let mut text = crate::pdf_edit_preserve::decode_pdf_text(key, false, 256);
    if key.last() == Some(&1) {
        text.push('\u{1}');
    }
    text
}

/// The attachments of a result: key, file name and data.
fn attachments_of(pdf: &ReadPdf) -> Vec<(String, String, String)> {
    let tree = catalog_of(pdf)
        .get::<Dict<'_>>(b"Names")
        .and_then(|names| names.get::<Dict<'_>>(b"EmbeddedFiles"))
        .expect("the attachment tree");
    let entries = tree.get::<Array<'_>>(b"Names").expect("a flat tree");
    let mut items = entries.iter::<Object<'_>>();
    let mut found = Vec::new();
    while let (Some(Object::String(key)), Some(Object::Dict(file))) = (items.next(), items.next()) {
        let data = file
            .get::<Dict<'_>>(b"EF")
            .and_then(|ef| ef.get::<Stream<'_>>(b"F"))
            .expect("the file's data");
        let name = file
            .get::<hayro_syntax::object::String<'_>>(b"UF")
            .map(|s| String::from_utf8_lossy(s.as_bytes()).into_owned())
            .unwrap_or_default();
        found.push((
            renamed_key(key.as_bytes()),
            name,
            String::from_utf8_lossy(&data.decoded().unwrap()).into_owned(),
        ));
    }
    found
}

#[test]
fn attachments_are_carried_with_their_data_and_sorted_by_name() {
    let out = read(&rebuilt(with_attachments()).bytes);
    assert_eq!(
        attachments_of(&out),
        [
            (
                "a.txt".to_string(),
                "a.txt".to_string(),
                "hello".to_string()
            ),
            (
                "b.txt".to_string(),
                "b.txt".to_string(),
                "world!".to_string()
            ),
        ]
    );
    // The stream keeps its own entries.
    let tree = catalog_of(&out)
        .get::<Dict<'_>>(b"Names")
        .and_then(|n| n.get::<Dict<'_>>(b"EmbeddedFiles"))
        .unwrap();
    let first = tree
        .get::<Array<'_>>(b"Names")
        .unwrap()
        .iter::<Object<'_>>()
        .nth(1);
    let Some(Object::Dict(file)) = first else {
        panic!("a filespec")
    };
    let stream = file
        .get::<Dict<'_>>(b"EF")
        .and_then(|ef| ef.get::<Stream<'_>>(b"F"))
        .unwrap();
    assert_eq!(
        stream
            .dict()
            .get::<Name<'_>>(b"Subtype")
            .map(|n| n.as_str().to_string())
            .as_deref(),
        Some("text/plain")
    );
    assert_eq!(
        stream
            .dict()
            .get::<Dict<'_>>(b"Params")
            .and_then(|p| p.get::<i32>(b"Size")),
        Some(5)
    );
}

#[test]
fn two_files_with_one_attachment_name_keep_both() {
    let engine = engine();
    let mut session = EditSession::open(with_attachments(), &engine).unwrap();
    session.merge(vec![with_attachments()], &engine).unwrap();
    let out = read(&session.render(None, &engine).unwrap().bytes);
    let keys: Vec<String> = attachments_of(&out)
        .into_iter()
        .map(|(key, _, _)| key)
        .collect();
    assert_eq!(keys, ["a.txt", "a.txt\u{1}", "b.txt", "b.txt\u{1}"]);
}

#[test]
fn a_korean_attachment_name_in_a_tree_with_kids_survives() {
    let more = [spec("한글.txt", 6), embedded("data")];
    let file = one_page(
        "/Names << /EmbeddedFiles << /Kids [7 0 R] >> >>",
        "",
        &[
            more[0].clone(),
            more[1].clone(),
            "<< /Limits [(a) (z)] /Names [<FEFFD55CAE00002E007400780074> 5 0 R] >>".to_string(),
        ],
    );
    let out = read(&rebuilt(file).bytes);
    let found = attachments_of(&out);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, "한글.txt");
    assert_eq!(found[0].2, "data");
}

#[test]
fn hostile_attachment_trees_are_refused() {
    let looped = one_page(
        "/Names << /EmbeddedFiles 5 0 R >>",
        "",
        &["<< /Kids [5 0 R] >>".to_string()],
    );
    assert!(refused(looped).contains("첨부 파일"));
    let pairs: String = (0..=MAX_ATTACHMENTS)
        .map(|n| format!("(n{n:05}) << >> "))
        .collect();
    let many = one_page(
        &format!("/Names << /EmbeddedFiles << /Names [{pairs}] >> >>"),
        "",
        &[],
    );
    assert!(refused(many).contains("첨부 파일"));
}

#[test]
fn the_attachments_of_all_joined_files_together_are_limited() {
    let names = |from: usize| -> String {
        (from..from + MAX_ATTACHMENTS / 2 + 1)
            .map(|n| format!("(n{n:05}) << >> "))
            .collect()
    };
    let file = |from: usize| {
        let catalog = format!("/Names << /EmbeddedFiles << /Names [{}] >> >>", names(from));
        one_page(&catalog, "", &[])
    };
    let engine = engine();
    let mut session = EditSession::open(file(0), &engine).unwrap();
    session.merge(vec![file(10_000)], &engine).unwrap();
    let err = session.render(None, &engine).unwrap_err();
    assert!(err.contains("첨부 파일"), "{err}");
}

// ---- metadata -----------------------------------------------------------------------------------

fn xmp(body: &str) -> String {
    format!(
        "<< /Type /Metadata /Subtype /XML /Length {} >>\nstream\n{body}\nendstream",
        body.len()
    )
}

#[test]
fn the_metadata_stream_is_carried() {
    const PACKET: &str = "<?xpacket begin=\"\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?><x:xmpmeta><dc:title>계약서</dc:title></x:xmpmeta><?xpacket end=\"w\"?>";
    let out = read(&rebuilt(one_page("/Metadata 5 0 R", "", &[xmp(PACKET)])).bytes);
    let stream = catalog_of(&out)
        .get::<Stream<'_>>(b"Metadata")
        .expect("the metadata stream");
    assert_eq!(stream.decoded().unwrap().as_ref(), PACKET.as_bytes());
    assert_eq!(
        stream
            .dict()
            .get::<Name<'_>>(b"Subtype")
            .map(|n| n.as_str().to_string())
            .as_deref(),
        Some("XML")
    );
}

#[test]
fn the_metadata_of_the_first_page_s_file_is_the_one_kept() {
    let engine = engine();
    let mut session =
        EditSession::open(one_page("/Metadata 5 0 R", "", &[xmp("first")]), &engine).unwrap();
    session
        .merge(
            vec![one_page("/Metadata 5 0 R", "", &[xmp("second")])],
            &engine,
        )
        .unwrap();
    let out = read(&session.render(None, &engine).unwrap().bytes);
    let stream = catalog_of(&out).get::<Stream<'_>>(b"Metadata").unwrap();
    assert_eq!(stream.decoded().unwrap().as_ref(), b"first");
}

#[test]
fn a_metadata_stream_past_the_limit_is_refused() {
    let big = "x".repeat(MAX_XMP_BYTES + 1);
    let err = refused(one_page("/Metadata 5 0 R", "", &[xmp(&big)]));
    assert!(err.contains("메타데이터"), "{err}");
    let missing = read(&rebuilt(one_page("/Metadata 99 0 R", "", &[])).bytes);
    assert!(
        !catalog_of(&missing).contains_key(b"Metadata"),
        "a dangling reference is no stream"
    );
}

#[test]
fn what_the_catalog_holds_survives_a_second_save() {
    let more = vec![spec("a.txt", 6), embedded("hello"), xmp("<x/>")];
    let catalog = "/PageLabels << /Nums [0 << /S /R >>] >> /Metadata 7 0 R \
                   /Names << /EmbeddedFiles << /Names [(a.txt) 5 0 R] >> >>";
    let first = rebuilt(one_page(catalog, "", &more)).bytes;
    let second = rebuilt(first).bytes;
    let out = read(&second);
    assert_eq!(labels_of(&out), ["R||1"]);
    assert_eq!(attachments_of(&out).len(), 1);
    assert!(catalog_of(&out).get::<Stream<'_>>(b"Metadata").is_some());
}
