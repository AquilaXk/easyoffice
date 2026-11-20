//! Free text, ink and stamp annotations across an edit (PD-266): they are written again with their
//! own appearance streams, moved to the new page space, without the entries that would pull the
//! original page or an action along.

use crate::pdf_edit_carry_tests::rebuilt;
use crate::pdf_edit_engine::InProcessEngine;
use crate::pdf_edit_fixtures::{hand_written_with_xref, read};
use crate::pdf_edit_session::EditSession;
use hayro_syntax::object::{Array, Dict, MaybeRef, Name, Number, Stream};
use hayro_syntax::Pdf as ReadPdf;
use std::sync::Arc;
use usvg::fontdb::Database;

const AP_CONTENT: &str = "0 1 0 rg 0 0 100 60 re f";

fn form(content: &str) -> String {
    format!(
        "<< /Type /XObject /Subtype /Form /BBox [0 0 100 60] /Length {} >>\nstream\n{content}\nendstream",
        content.len()
    )
}

/// The catalog entry of a layer list that hides object 12.
const HIDE_OBJECT_12: &str = "/OCProperties << /OCGs [12 0 R] /D << /OFF [12 0 R] >> >>";

/// One page whose crop box starts at (20, 30) and that carries the objects `more` (from number 5
/// on) as annotations listed in `annots`.
fn annotated(annots: &str, more: Vec<String>) -> Vec<u8> {
    annotated_with("", annots, more)
}

fn annotated_with(catalog: &str, annots: &str, more: Vec<String>) -> Vec<u8> {
    let mut objects = vec![
        (1, format!("<< /Type /Catalog /Pages 2 0 R {catalog} >>")),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /CropBox [20 30 200 300] \
                 /Contents 4 0 R /Annots [{annots}] >>"
            ),
        ),
        (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
    ];
    objects.extend((5u32..).zip(more));
    hand_written_with_xref(&objects, "")
}

/// Objects 5 to 12 of a page with the three kinds of annotation, a pop-up, an action and a layer.
fn three_kinds() -> Vec<String> {
    vec![
        "<< /Type /Annot /Subtype /FreeText /Rect [40 200 140 260] /Contents (Memo text) \
         /DA (/Helv 12 Tf 0 g) /Q 1 /C [1 1 0] /F 4 /T (Kim) /AP << /N 8 0 R >> /P 3 0 R \
         /Popup 11 0 R /A << /S /JavaScript /JS (app.alert\\(1\\)) >> /OC 12 0 R /BS << /W 2 >> >>"
            .to_string(),
        "<< /Type /Annot /Subtype /Ink /Rect [50 100 150 150] \
         /InkList [[50 100 100 150 150 100] [60 110 70 120]] /C [1 0 0] /BS << /W 3 >> \
         /AP << /N 9 0 R >> /Contents (stroke) >>"
            .to_string(),
        "<< /Type /Annot /Subtype /Stamp /Rect [30 40 90 70] /Name /Approved \
         /AP << /N 10 0 R >> /Contents (stamp) >>"
            .to_string(),
        form(AP_CONTENT),
        form(AP_CONTENT),
        form(AP_CONTENT),
        "<< /Type /Annot /Subtype /Popup /Rect [0 0 10 10] /Parent 5 0 R >>".to_string(),
        "<< /Type /OCG /Name (Notes) >>".to_string(),
    ]
}

fn page_annots(pdf: &ReadPdf) -> Vec<Dict<'_>> {
    pdf.pages()[0]
        .raw()
        .get::<Array<'_>>(b"Annots")
        .map(|array| array.iter::<Dict<'_>>().collect())
        .unwrap_or_default()
}

fn subtype(annot: &Dict<'_>) -> String {
    annot
        .get::<Name<'_>>(b"Subtype")
        .map(|n| n.as_str().to_string())
        .unwrap_or_default()
}

fn numbers(array: Array<'_>) -> Vec<f32> {
    array.iter::<Number>().map(|n| n.as_f32()).collect()
}

fn text(annot: &Dict<'_>, key: &[u8]) -> Option<String> {
    annot
        .get::<hayro_syntax::object::String<'_>>(key)
        .map(|s| String::from_utf8_lossy(s.as_bytes()).into_owned())
}

#[test]
fn free_text_ink_and_stamp_keep_their_content_and_appearance() {
    let built = rebuilt(annotated_with(
        HIDE_OBJECT_12,
        "5 0 R 6 0 R 7 0 R",
        three_kinds(),
    ));
    assert!(built.warnings.is_empty(), "{:?}", built.warnings);
    let out = read(&built.bytes);
    let annots = page_annots(&out);
    let kinds: Vec<String> = annots.iter().map(subtype).collect();
    assert_eq!(kinds, ["FreeText", "Ink", "Stamp"]);

    let (free, ink, stamp) = (&annots[0], &annots[1], &annots[2]);
    // The page's crop box starts at (20, 30), which is the origin of the new page.
    assert_eq!(
        numbers(free.get::<Array<'_>>(b"Rect").unwrap()),
        [20.0, 170.0, 120.0, 230.0]
    );
    assert_eq!(text(free, b"Contents").as_deref(), Some("Memo text"));
    assert_eq!(text(free, b"DA").as_deref(), Some("/Helv 12 Tf 0 g"));
    assert_eq!(text(free, b"T").as_deref(), Some("Kim"));
    assert_eq!(free.get::<i32>(b"Q"), Some(1));
    assert_eq!(free.get::<i32>(b"F"), Some(4));
    assert_eq!(
        numbers(free.get::<Array<'_>>(b"C").unwrap()),
        [1.0, 1.0, 0.0]
    );
    assert_eq!(
        free.get::<Dict<'_>>(b"BS")
            .and_then(|bs| bs.get::<i32>(b"W")),
        Some(2)
    );

    assert_eq!(
        numbers(ink.get::<Array<'_>>(b"Rect").unwrap()),
        [30.0, 70.0, 130.0, 120.0]
    );
    let strokes: Vec<Vec<f32>> = ink
        .get::<Array<'_>>(b"InkList")
        .unwrap()
        .iter::<Array<'_>>()
        .map(numbers)
        .collect();
    assert_eq!(
        strokes,
        [
            vec![30.0, 70.0, 80.0, 120.0, 130.0, 70.0],
            vec![40.0, 80.0, 50.0, 90.0]
        ]
    );

    assert_eq!(
        stamp
            .get::<Name<'_>>(b"Name")
            .map(|n| n.as_str().to_string())
            .as_deref(),
        Some("Approved")
    );
    assert_eq!(
        numbers(stamp.get::<Array<'_>>(b"Rect").unwrap()),
        [10.0, 10.0, 70.0, 40.0]
    );

    // Each keeps its own appearance stream, byte for byte once decoded.
    for annot in &annots {
        let normal = annot
            .get::<Dict<'_>>(b"AP")
            .and_then(|ap| ap.get::<Stream<'_>>(b"N"))
            .expect("an appearance stream");
        assert_eq!(normal.decoded().unwrap().as_ref(), AP_CONTENT.as_bytes());
    }
}

#[test]
fn nothing_that_pulls_in_the_original_page_or_runs_something_is_written() {
    let out = read(&rebuilt(annotated_with(HIDE_OBJECT_12, "5 0 R", three_kinds())).bytes);
    let free = &page_annots(&out)[0];
    for key in [&b"P"[..], b"Popup", b"A", b"AA", b"Parent", b"StructParent"] {
        assert!(!free.contains_key(key), "{}", String::from_utf8_lossy(key));
    }
    // The pop-up was not carried either, and the file is as small as three small forms and the
    // page allow: the original page tree is not in it.
    assert_eq!(out.pages().len(), 1);
    let objects = out.objects().into_iter().count();
    assert!(objects < 20, "{objects} objects");
}

#[test]
fn an_annotation_keeps_the_layer_it_is_switched_by() {
    let out = read(&rebuilt(annotated_with(HIDE_OBJECT_12, "5 0 R", three_kinds())).bytes);
    let free = &page_annots(&out)[0];
    let layer = free
        .get_ref(b"OC")
        .expect("the layer the annotation belongs to");
    let catalog = out.xref().get::<Dict<'_>>(out.xref().root_id()).unwrap();
    let off = catalog
        .get::<Dict<'_>>(b"OCProperties")
        .and_then(|p| p.get::<Dict<'_>>(b"D"))
        .and_then(|d| d.get::<Array<'_>>(b"OFF"))
        .expect("the hidden layers");
    let hidden: Vec<_> = off
        .raw_iter()
        .filter_map(|item| match item {
            MaybeRef::Ref(r) => Some(r),
            MaybeRef::NotRef(_) => None,
        })
        .collect();
    assert_eq!(
        hidden,
        [layer],
        "the annotation and the layer list name one object"
    );
}

#[test]
fn a_saved_file_with_these_annotations_is_saved_again_without_loss() {
    let engine = InProcessEngine::new(Arc::new(Database::new()));
    let first = rebuilt(annotated_with(
        HIDE_OBJECT_12,
        "5 0 R 6 0 R 7 0 R",
        three_kinds(),
    ))
    .bytes;
    let session = EditSession::open(first, &engine).unwrap();
    let again = read(&session.render(None, &engine).unwrap().bytes);
    let kinds: Vec<String> = page_annots(&again).iter().map(subtype).collect();
    assert_eq!(kinds, ["FreeText", "Ink", "Stamp"]);
    assert_eq!(
        numbers(page_annots(&again)[0].get::<Array<'_>>(b"Rect").unwrap()),
        [20.0, 170.0, 120.0, 230.0],
        "the origin of the saved page is zero, so a second save moves nothing"
    );
}

#[test]
fn an_annotation_without_a_usable_rectangle_is_counted_as_not_kept() {
    let more = vec![
        "<< /Type /Annot /Subtype /Stamp /Name /Draft >>".to_string(),
        "<< /Type /Annot /Subtype /Ink /Rect [1 2 3] /InkList [[1 2 3 4]] >>".to_string(),
    ];
    let built = rebuilt(annotated("5 0 R 6 0 R", more));
    assert_eq!(page_annots(&read(&built.bytes)).len(), 0);
    assert_eq!(
        built.warnings,
        ["지원하지 않는 주석 2개는 저장하지 못했습니다 (Ink 1, Stamp 1)."]
    );
}

fn ink_with(points: usize, nested: usize) -> String {
    let stroke = format!("[{}]", "1 ".repeat(points));
    let list = format!("{}{stroke}{}", "[".repeat(nested), "]".repeat(nested));
    format!("<< /Type /Annot /Subtype /Ink /Rect [0 0 10 10] /InkList [{list}] >>")
}

#[test]
fn a_hostile_annotation_is_refused_by_size_depth_or_the_sum_of_all() {
    let engine = InProcessEngine::new(Arc::new(Database::new()));
    let refused = |file: Vec<u8>| {
        EditSession::open(file, &engine)
            .unwrap()
            .render(None, &engine)
            .unwrap_err()
    };
    let big = crate::pdf_edit_budget::MAX_VALUE_NODES + 10;
    let err = refused(annotated("5 0 R", vec![ink_with(big, 0)]));
    assert!(err.contains("크기"), "{err}");
    let err = refused(annotated("5 0 R", vec![ink_with(4, 300)]));
    assert!(err.contains("중첩"), "{err}");

    // Each annotation alone is within its limit; together they hold more than a build may carry.
    let each = crate::pdf_edit_budget::MAX_VALUE_NODES - 100;
    let count = crate::pdf_edit_budget::MAX_CARRIED_NODES / each + 2;
    let annots: Vec<String> = (0..count).map(|_| ink_with(each, 0)).collect();
    let list: Vec<String> = (0..count).map(|n| format!("{} 0 R", 5 + n)).collect();
    let err = refused(annotated(&list.join(" "), annots));
    assert!(err.contains("합계"), "{err}");
}

#[test]
fn annotations_beyond_the_three_kinds_are_still_reported() {
    let more = vec![
        "<< /Type /Annot /Subtype /Square /Rect [10 10 50 50] >>".to_string(),
        "<< /Type /Annot /Subtype /Stamp /Rect [10 10 50 50] /AP << /N 7 0 R >> >>".to_string(),
        form(AP_CONTENT),
    ];
    let built = rebuilt(annotated("5 0 R 6 0 R", more));
    assert_eq!(
        built.warnings,
        ["지원하지 않는 주석 1개는 저장하지 못했습니다 (Square 1)."]
    );
    assert_eq!(page_annots(&read(&built.bytes)).len(), 1);
}
