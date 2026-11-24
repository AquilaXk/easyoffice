//! Free text, ink and stamp annotations across an edit (PD-266): they are written again with their
//! own appearance streams, moved to the new page space, without the entries that would pull the
//! original page or an action along.

use crate::pdf_edit_carry_tests::rebuilt;
use crate::pdf_edit_engine::InProcessEngine;
use crate::pdf_edit_fixtures::{hand_written_with_xref, read};
use crate::pdf_edit_session::EditSession;
use hayro_syntax::object::{Array, Dict, MaybeRef, Name, Number, Object, Stream};
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
fn annotations_beyond_the_carried_kinds_are_still_reported() {
    let more = vec![
        "<< /Type /Annot /Subtype /Caret /Rect [10 10 50 50] >>".to_string(),
        "<< /Type /Annot /Subtype /Stamp /Rect [10 10 50 50] /AP << /N 7 0 R >> >>".to_string(),
        form(AP_CONTENT),
    ];
    let built = rebuilt(annotated("5 0 R 6 0 R", more));
    assert_eq!(
        built.warnings,
        ["지원하지 않는 주석 1개는 저장하지 못했습니다 (Caret 1)."]
    );
    assert_eq!(page_annots(&read(&built.bytes)).len(), 1);
}

#[test]
fn square_circle_line_polygon_and_file_attachment_survive_with_crop_shift_and_layers() {
    let objects: Vec<(u32, String)> = vec![
        (1, format!("<< /Type /Catalog /Pages 2 0 R {HIDE_OBJECT_12} >>")),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /CropBox [20 30 200 300] \
             /Contents 4 0 R /Annots [5 0 R 6 0 R 7 0 R 8 0 R 9 0 R] >>"
                .to_string(),
        ),
        (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
        (
            5,
            "<< /Type /Annot /Subtype /Square /Rect [40 200 140 260] /C [1 0 0] /IC [0 1 0] \
             /BS << /W 2 /S /D >> /BE << /S /C /I 1 >> /RD [4 4 4 4] /CA 0.5 /Contents (box) \
             /T (Kim) /AP << /N 10 0 R >> /OC 12 0 R /P 3 0 R /A << /S /URI /URI (https://example.org) >> >>"
                .to_string(),
        ),
        (
            6,
            "<< /Type /Annot /Subtype /Circle /Rect [30 50 110 130] /C [0 0 1] /IC [1 1 0] \
             /RD [2 2 2 2] /Contents (oval) >>"
                .to_string(),
        ),
        (
            7,
            "<< /Type /Annot /Subtype /Line /Rect [40 100 140 180] /L [140 180 40 100] \
             /LE [/OpenArrow /ClosedArrow] /LL 12 /LLE 6 /LLO 2 /Cap true /CP /Top /CO [0 4] \
             /IT /LineDimension /IC [1 0 1] /BS << /W 1.5 >> /OC 12 0 R >>"
                .to_string(),
        ),
        (
            8,
            "<< /Type /Annot /Subtype /Polygon /Rect [40 100 140 200] \
             /Vertices [40 100 140 100 90 200] /Path [[40 100 140 100 90 200]] \
             /BE << /S /C /I 2 >> /IT /PolygonCloud /IC [0.8 0.9 1] /C [0 0 1] >>"
                .to_string(),
        ),
        (
            9,
            "<< /Type /Annot /Subtype /FileAttachment /Rect [50 60 74 84] /Name /Paperclip \
             /Contents (Spec file) /FS 11 0 R /OC 12 0 R /P 3 0 R >>"
                .to_string(),
        ),
        (10, form(AP_CONTENT)),
        (
            11,
            "<< /Type /Filespec /F (report.txt) /UF <FEFFD55CAE00002E007400780074> \
             /Desc (Korean doc) /AFRelationship /Supplement /EF << /F 13 0 R >> \
             /DOS (C:\\\\evil.exe) /P 3 0 R >>"
                .to_string(),
        ),
        (12, "<< /Type /OCG /Name (Review) >>".to_string()),
        (
            13,
            "<< /Type /EmbeddedFile /Subtype /text#2Fplain /Params << /Size 22 >> /Length 22 >>\n\
             stream\nattached payload bytes\nendstream"
                .to_string(),
        ),
    ];
    let built = rebuilt(hand_written_with_xref(&objects, ""));
    assert!(built.warnings.is_empty(), "{:?}", built.warnings);

    let out = read(&built.bytes);
    let annots = page_annots(&out);
    let kinds: Vec<String> = annots.iter().map(subtype).collect();
    assert_eq!(
        kinds,
        ["Square", "Circle", "Line", "Polygon", "FileAttachment"]
    );

    let (sq, ci, ln, pg, fa) = (&annots[0], &annots[1], &annots[2], &annots[3], &annots[4]);
    assert_eq!(
        numbers(sq.get::<Array<'_>>(b"Rect").unwrap()),
        [20.0, 170.0, 120.0, 230.0]
    );
    assert_eq!(
        numbers(sq.get::<Array<'_>>(b"RD").unwrap()),
        [4.0, 4.0, 4.0, 4.0]
    );
    assert_eq!(
        numbers(sq.get::<Array<'_>>(b"IC").unwrap()),
        [0.0, 1.0, 0.0]
    );
    assert!(!sq.contains_key(b"P") && !sq.contains_key(b"A"));
    assert!(sq
        .get::<Dict<'_>>(b"AP")
        .and_then(|ap| ap.get::<Stream<'_>>(b"N"))
        .is_some());
    let sq_oc = sq.get_ref(b"OC").expect("Square OC");

    assert_eq!(
        numbers(ci.get::<Array<'_>>(b"Rect").unwrap()),
        [10.0, 20.0, 90.0, 100.0]
    );
    assert_eq!(text(ci, b"Contents").as_deref(), Some("oval"));

    // Line direction (start->end) is preserved when translated by (-20, -30).
    assert_eq!(
        numbers(ln.get::<Array<'_>>(b"L").unwrap()),
        [120.0, 150.0, 20.0, 70.0]
    );
    assert_eq!(ln.get::<i32>(b"LL"), Some(12));
    assert_eq!(ln.get::<bool>(b"Cap"), Some(true));
    assert_eq!(numbers(ln.get::<Array<'_>>(b"CO").unwrap()), [0.0, 4.0]);
    assert_eq!(
        ln.get::<Name<'_>>(b"CP")
            .map(|n| n.as_str().to_string())
            .as_deref(),
        Some("Top")
    );
    assert_eq!(ln.get_ref(b"OC"), Some(sq_oc));

    assert_eq!(
        numbers(pg.get::<Array<'_>>(b"Vertices").unwrap()),
        [20.0, 70.0, 120.0, 70.0, 70.0, 170.0]
    );
    let subpaths: Vec<Vec<f32>> = pg
        .get::<Array<'_>>(b"Path")
        .unwrap()
        .iter::<Array<'_>>()
        .map(numbers)
        .collect();
    assert_eq!(subpaths, [vec![20.0, 70.0, 120.0, 70.0, 70.0, 170.0]]);

    assert_eq!(
        numbers(fa.get::<Array<'_>>(b"Rect").unwrap()),
        [30.0, 30.0, 54.0, 54.0]
    );
    assert_eq!(
        fa.get::<Name<'_>>(b"Name")
            .map(|n| n.as_str().to_string())
            .as_deref(),
        Some("Paperclip")
    );
    let fs = fa.get::<Dict<'_>>(b"FS").expect("FS dict");
    assert!(!fs.contains_key(b"DOS") && !fs.contains_key(b"P"));
    let uf = fs.get::<hayro_syntax::object::String<'_>>(b"UF").unwrap();
    assert_eq!(
        crate::pdf_edit_preserve::decode_pdf_text(uf.as_bytes(), false, 64),
        "한글.txt"
    );
    let embedded = fs
        .get::<Dict<'_>>(b"EF")
        .and_then(|ef| ef.get::<Stream<'_>>(b"F"))
        .expect("embedded stream");
    assert_eq!(
        embedded.decoded().unwrap().as_ref(),
        b"attached payload bytes"
    );

    // Second save keeps all five types without loss or extra coordinate shift.
    let engine = InProcessEngine::new(Arc::new(Database::new()));
    let session = EditSession::open(built.bytes, &engine).unwrap();
    let again = session.render(None, &engine).unwrap();
    assert!(again.warnings.is_empty(), "{:?}", again.warnings);
    let out2 = read(&again.bytes);
    let annots2 = page_annots(&out2);
    assert_eq!(
        annots2.iter().map(subtype).collect::<Vec<_>>(),
        ["Square", "Circle", "Line", "Polygon", "FileAttachment"]
    );
    assert_eq!(
        numbers(annots2[2].get::<Array<'_>>(b"L").unwrap()),
        [120.0, 150.0, 20.0, 70.0]
    );
    let embedded2 = annots2[4]
        .get::<Dict<'_>>(b"FS")
        .and_then(|d| d.get::<Dict<'_>>(b"EF"))
        .and_then(|ef| ef.get::<Stream<'_>>(b"F"))
        .unwrap();
    assert_eq!(
        embedded2.decoded().unwrap().as_ref(),
        b"attached payload bytes"
    );
}

#[test]
fn indirect_line_polygon_and_compressed_file_attachment_merge_and_split() {
    let make_doc = |payload: &str, name: &str| {
        let objects = vec![
            (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
            (2, "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_string()),
            (
                3,
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Annots [5 0 R 6 0 R 7 0 R] >>"
                    .to_string(),
            ),
            (
                4,
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] >>".to_string(),
            ),
            (
                5,
                "<< /Type /Annot /Subtype /Line /Rect [10 10 90 90] /L 8 0 R >>".to_string(),
            ),
            (
                6,
                "<< /Type /Annot /Subtype /Polygon /Rect [10 10 90 90] /Vertices 9 0 R >>"
                    .to_string(),
            ),
            (
                7,
                format!(
                    "<< /Type /Annot /Subtype /FileAttachment /Rect [10 10 30 30] \
                     /FS << /Type /Filespec /F ({name}) /EF << /UF 10 0 R >> >> >>"
                ),
            ),
            (8, "[10 10 90 90]".to_string()),
            (9, "[10 10 90 10 50 90]".to_string()),
            (
                10,
                format!(
                    "<< /Type /EmbeddedFile /Params 11 0 R /Length {} >>\nstream\n{payload}\nendstream",
                    payload.len()
                ),
            ),
            (11, format!("<< /Size {} >>", payload.len())),
        ];
        hand_written_with_xref(&objects, "")
    };

    let engine = InProcessEngine::new(Arc::new(Database::new()));
    let mut session = EditSession::open(make_doc("first-doc", "a.txt"), &engine).unwrap();
    session
        .merge(vec![make_doc("second-doc", "b.txt")], &engine)
        .unwrap();
    session.rotate(None, 90).unwrap();
    let built = session.render(None, &engine).unwrap();
    assert!(built.warnings.is_empty(), "{:?}", built.warnings);

    let out = read(&built.bytes);
    assert_eq!(out.pages().len(), 4);
    let page0_fa = &out.pages()[0]
        .raw()
        .get::<Array<'_>>(b"Annots")
        .unwrap()
        .iter::<Dict<'_>>()
        .collect::<Vec<_>>()[2];
    let page2_fa = &out.pages()[2]
        .raw()
        .get::<Array<'_>>(b"Annots")
        .unwrap()
        .iter::<Dict<'_>>()
        .collect::<Vec<_>>()[2];
    let read_uf_stream = |fa: &Dict<'_>| {
        let s = fa
            .get::<Dict<'_>>(b"FS")
            .and_then(|fs| fs.get::<Dict<'_>>(b"EF"))
            .and_then(|ef| ef.get::<Stream<'_>>(b"UF"))
            .unwrap();
        String::from_utf8(s.decoded().unwrap().to_vec()).unwrap()
    };
    assert_eq!(read_uf_stream(page0_fa), "first-doc");
    assert_eq!(read_uf_stream(page2_fa), "second-doc");
}

#[test]
fn compressed_file_attachment_shared_with_catalog_is_copied_once() {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use hayro_write::pdf_writer::{Filter, Name as WName, Pdf, Rect, Ref, Str};
    use std::io::Write;

    let payload = b"shared flate attachment payload 1234567890";
    let deflated = {
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(payload).unwrap();
        enc.finish().unwrap()
    };

    let mut next = Ref::new(1);
    let (catalog_r, pages_r, page_r, annot_r, spec_r, ef_r) = (
        next.bump(),
        next.bump(),
        next.bump(),
        next.bump(),
        next.bump(),
        next.bump(),
    );
    let mut pdf = Pdf::new();
    {
        let mut cat = pdf.catalog(catalog_r);
        cat.pages(pages_r);
        let mut names = cat.insert(WName(b"Names")).dict();
        let mut ef_tree = names.insert(WName(b"EmbeddedFiles")).dict();
        let mut pairs = ef_tree.insert(WName(b"Names")).array();
        pairs.item(Str(b"shared.bin"));
        pairs.item(spec_r);
    }
    pdf.pages(pages_r).kids([page_r]).count(1);
    {
        let mut page = pdf.page(page_r);
        page.parent(pages_r)
            .media_box(Rect::new(0.0, 0.0, 200.0, 300.0))
            .annotations([annot_r]);
    }
    {
        let mut annot = pdf.indirect(annot_r).dict();
        annot.pair(WName(b"Type"), WName(b"Annot"));
        annot.pair(WName(b"Subtype"), WName(b"FileAttachment"));
        annot
            .insert(WName(b"Rect"))
            .array()
            .items([10.0, 10.0, 34.0, 34.0]);
        annot.pair(WName(b"FS"), spec_r);
    }
    {
        let mut spec = pdf.indirect(spec_r).dict();
        spec.pair(WName(b"Type"), WName(b"Filespec"));
        spec.pair(WName(b"F"), Str(b"shared.bin"));
        spec.pair(WName(b"UF"), Str(b"shared.bin"));
        spec.insert(WName(b"EF")).dict().pair(WName(b"F"), ef_r);
    }
    {
        let mut stream = pdf.stream(ef_r, &deflated);
        stream.filter(Filter::FlateDecode);
        stream.pair(WName(b"Type"), WName(b"EmbeddedFile"));
        stream
            .insert(WName(b"Params"))
            .dict()
            .pair(WName(b"Size"), i32::try_from(payload.len()).unwrap());
    }

    let built = rebuilt(pdf.finish());
    assert!(built.warnings.is_empty(), "{:?}", built.warnings);
    let out = read(&built.bytes);
    let page_ef = page_annots(&out)[0]
        .get::<Dict<'_>>(b"FS")
        .and_then(|fs| fs.get::<Dict<'_>>(b"EF"))
        .and_then(|ef| ef.get_ref(b"F"))
        .unwrap();
    let cat_ef = out
        .xref()
        .get::<Dict<'_>>(out.xref().root_id())
        .and_then(|cat| cat.get::<Dict<'_>>(b"Names"))
        .and_then(|n| n.get::<Dict<'_>>(b"EmbeddedFiles"))
        .and_then(|t| t.get::<Array<'_>>(b"Names"))
        .and_then(|arr| arr.iter::<Object<'_>>().nth(1))
        .and_then(|obj| match obj {
            Object::Dict(fs) => fs.get::<Dict<'_>>(b"EF"),
            _ => None,
        })
        .and_then(|ef| ef.get_ref(b"F"))
        .unwrap();
    assert_eq!(page_ef, cat_ef, "shared embedded stream copied once");
    let decoded = out
        .xref()
        .get::<Stream<'_>>(page_ef.into())
        .unwrap()
        .decoded()
        .unwrap();
    assert_eq!(decoded.as_ref(), payload);
}

#[test]
fn malformed_and_hostile_line_polygon_square_and_file_attachments_are_handled_safely() {
    // 1. Invalid geometries and external/unsafe FileAttachment specs are dropped and warned.
    let more = vec![
        // Line without /L
        "<< /Type /Annot /Subtype /Line /Rect [10 10 50 50] >>".to_string(),
        // Line with wrong-length /L
        "<< /Type /Annot /Subtype /Line /Rect [10 10 50 50] /L [10 10] >>".to_string(),
        // Polygon with odd-length /Vertices
        "<< /Type /Annot /Subtype /Polygon /Rect [10 10 50 50] /Vertices [10 10 50] >>"
            .to_string(),
        // FileAttachment with external path string
        "<< /Type /Annot /Subtype /FileAttachment /Rect [10 10 30 30] /FS (/etc/passwd) >>"
            .to_string(),
        // FileAttachment with Filespec lacking /EF
        "<< /Type /Annot /Subtype /FileAttachment /Rect [10 10 30 30] /FS << /Type /Filespec /F (evil.txt) >> >>"
            .to_string(),
        // FileAttachment whose /EF /F points to the page dictionary (not a stream)
        "<< /Type /Annot /Subtype /FileAttachment /Rect [10 10 30 30] /FS << /Type /Filespec /EF << /F 3 0 R >> >> >>"
            .to_string(),
        // FileAttachment whose stream dict /Params tries to pull page 3
        "<< /Type /Annot /Subtype /FileAttachment /Rect [10 10 30 30] /FS << /Type /Filespec /EF << /F 12 0 R >> >> >>"
            .to_string(),
        "<< /Type /EmbeddedFile /Params 3 0 R /Length 2 >>\nstream\nhi\nendstream".to_string(),
    ];
    let built = rebuilt(annotated(
        "5 0 R 6 0 R 7 0 R 8 0 R 9 0 R 10 0 R 11 0 R",
        more,
    ));
    assert_eq!(page_annots(&read(&built.bytes)).len(), 0);
    assert_eq!(
        built.warnings,
        ["지원하지 않는 주석 7개는 저장하지 못했습니다 (FileAttachment 4, Line 2, Polygon 1)."]
    );

    // 2. Square trying to smuggle page 3 via /BS 3 0 R does not drag the original page tree.
    let smuggle =
        vec!["<< /Type /Annot /Subtype /Square /Rect [30 40 70 80] /BS 3 0 R >>".to_string()];
    let smuggled = rebuilt(annotated("5 0 R", smuggle));
    assert!(smuggled.warnings.is_empty());
    let out = read(&smuggled.bytes);
    let sq = &page_annots(&out)[0];
    assert!(!sq.contains_key(b"BS"));
    assert!(out.objects().into_iter().count() < 15);

    // 3. Cyclic /Params on an EmbeddedFile stream is refused fail-closed.
    let engine = InProcessEngine::new(Arc::new(Database::new()));
    let cyclic = vec![
        "<< /Type /Annot /Subtype /FileAttachment /Rect [10 10 30 30] /FS << /Type /Filespec /EF << /F 6 0 R >> >> >>"
            .to_string(),
        "<< /Type /EmbeddedFile /Params 7 0 R /Length 2 >>\nstream\nhi\nendstream".to_string(),
        "<< /Sub 7 0 R >>".to_string(),
    ];
    let dropped_or_err = EditSession::open(annotated("5 0 R", cyclic), &engine)
        .unwrap()
        .render(None, &engine)
        .unwrap();
    // Nested indirect ref inside 7 0 R (/Sub 7 0 R at depth 1) is rejected safely so nothing is copied.
    assert_eq!(page_annots(&read(&dropped_or_err.bytes)).len(), 0);
    assert_eq!(
        dropped_or_err.warnings,
        ["지원하지 않는 주석 1개는 저장하지 못했습니다 (FileAttachment 1)."]
    );

    // Sibling keys on the stream dictionary may safely share one leaf indirect dictionary.
    let shared_leaf = vec![
        "<< /Type /Annot /Subtype /FileAttachment /Rect [10 10 30 30] /FS << /Type /Filespec /EF << /F 6 0 R >> >> >>"
            .to_string(),
        "<< /Type /EmbeddedFile /Params 7 0 R /DecodeParms 7 0 R /Length 2 >>\nstream\nhi\nendstream"
            .to_string(),
        "<< /Size 2 >>".to_string(),
    ];
    let shared_ok = EditSession::open(annotated("5 0 R", shared_leaf), &engine)
        .unwrap()
        .render(None, &engine)
        .unwrap();
    assert!(shared_ok.warnings.is_empty(), "{:?}", shared_ok.warnings);
    assert_eq!(page_annots(&read(&shared_ok.bytes)).len(), 1);

    // 4. Oversized /FS /F string is refused fail-closed.
    let long_name = "a".repeat(5_000);
    let oversized_fs = vec![
        format!(
            "<< /Type /Annot /Subtype /FileAttachment /Rect [10 10 30 30] /FS << /Type /Filespec /F ({long_name}) /EF << /F 6 0 R >> >> >>"
        ),
        "<< /Type /EmbeddedFile /Length 2 >>\nstream\nhi\nendstream".to_string(),
    ];
    let err = EditSession::open(annotated("5 0 R", oversized_fs), &engine)
        .unwrap()
        .render(None, &engine)
        .unwrap_err();
    assert!(err.contains("허용 한도를 초과"), "{err}");
}

#[test]
fn polygon_past_node_limit_and_too_many_file_attachments_are_refused() {
    let engine = InProcessEngine::new(Arc::new(Database::new()));
    let big = crate::pdf_edit_budget::MAX_VALUE_NODES + 10;
    let big_poly = format!(
        "<< /Type /Annot /Subtype /Polygon /Rect [0 0 10 10] /Vertices [{}] >>",
        "1 ".repeat(big)
    );
    let err = EditSession::open(annotated("5 0 R", vec![big_poly]), &engine)
        .unwrap()
        .render(None, &engine)
        .unwrap_err();
    assert!(err.contains("크기"), "{err}");

    let count = crate::pdf_edit_budget::MAX_ATTACHMENTS + 1;
    let mut objects = vec!["<< /Type /EmbeddedFile /Length 1 >>\nstream\nx\nendstream".to_string()];
    let mut refs = Vec::with_capacity(count);
    for idx in 0..count {
        refs.push(format!("{} 0 R", 6 + idx));
        objects.push(
            "<< /Type /Annot /Subtype /FileAttachment /Rect [0 0 10 10] \
             /FS << /Type /Filespec /EF << /F 5 0 R >> >> >>"
                .to_string(),
        );
    }
    let err = EditSession::open(annotated(&refs.join(" "), objects), &engine)
        .unwrap()
        .render(None, &engine)
        .unwrap_err();
    assert!(err.contains("첨부 파일 주석 수가 한도"), "{err}");
}

#[test]
fn a_page_placed_many_times_cannot_multiply_the_annotations_past_the_total() {
    use crate::pdf_edit_engine::PdfEngine;
    use crate::pdf_edit_overlay::Overlay;
    use crate::pdf_edit_wire::PagePlan;
    let engine = InProcessEngine::new(Arc::new(Database::new()));
    // One annotation of about 65,000 nodes, within its own limit and the total when read once.
    let each = crate::pdf_edit_budget::MAX_VALUE_NODES - 100;
    let file = Arc::new(annotated("5 0 R", vec![ink_with(each, 0)]));
    let copies = crate::pdf_edit_budget::MAX_CARRIED_NODES / each + 2;
    let place = |count: usize| {
        let pages = vec![
            PagePlan {
                source: 0,
                index: 0,
                turn: 0
            };
            count
        ];
        engine.build(&[Arc::clone(&file)], &pages, &Overlay::default())
    };
    assert!(place(2).is_ok());
    let err = place(copies).unwrap_err();
    assert!(err.contains("합계"), "{err}");
}

#[test]
fn ap_and_oc_smuggling_is_blocked_while_valid_ap_states_and_ocmd_survive() {
    let more = vec![
        // 5: Square trying to pull Page 3 via /AP /N and /OC 3 0 R
        "<< /Type /Annot /Subtype /Square /Rect [30 40 70 80] /AP << /N 3 0 R >> /OC 3 0 R >>"
            .to_string(),
        // 6: Circle trying to pull Page 3 via /AP state dict and /OCMD [3 0 R]
        "<< /Type /Annot /Subtype /Circle /Rect [30 40 70 80] \
         /AP << /N << /On 3 0 R >> >> /OC << /Type /OCMD /OCGs [3 0 R] >> >>"
            .to_string(),
        // 7: Line with external stream in /AP /N (must strip /AP) and valid OCMD (must keep)
        "<< /Type /Annot /Subtype /Line /Rect [30 40 70 80] /L [30 40 70 80] \
         /AP << /N 10 0 R >> /OC << /Type /OCMD /OCGs [12 0 R] /P /AllOn >> >>"
            .to_string(),
        // 8: Polygon with valid /AP state subdict (11 0 R)
        "<< /Type /Annot /Subtype /Polygon /Rect [30 40 70 80] /Vertices [30 40 70 40 50 80] \
         /AP << /N << /On 11 0 R >> >> >>"
            .to_string(),
        // 9: Circle with /AP 3 0 R directly
        "<< /Type /Annot /Subtype /Circle /Rect [30 40 70 80] /AP 3 0 R >>".to_string(),
        // 10: Hostile external /F stream pretending to be XObject
        "<< /Type /XObject /Subtype /Form /BBox [0 0 10 10] /F (/etc/passwd) /Length 0 >>\nstream\n\nendstream"
            .to_string(),
        // 11: Valid appearance Form stream
        form(AP_CONTENT),
        // 12: Valid OCG
        "<< /Type /OCG /Name (Layer) >>".to_string(),
    ];
    let built = rebuilt(annotated_with(
        HIDE_OBJECT_12,
        "5 0 R 6 0 R 7 0 R 8 0 R 9 0 R",
        more,
    ));
    assert!(built.warnings.is_empty(), "{:?}", built.warnings);
    let out = read(&built.bytes);
    let annots = page_annots(&out);
    assert_eq!(annots.len(), 5);
    assert!(!annots[0].contains_key(b"AP") && !annots[0].contains_key(b"OC"));
    assert!(!annots[1].contains_key(b"AP") && !annots[1].contains_key(b"OC"));
    assert!(!annots[2].contains_key(b"AP"));
    assert!(annots[2].get::<Dict<'_>>(b"OC").is_some());
    assert!(annots[3]
        .get::<Dict<'_>>(b"AP")
        .and_then(|ap| ap.get::<Dict<'_>>(b"N"))
        .and_then(|n| n.get::<Stream<'_>>(b"On"))
        .is_some());
    assert!(!annots[4].contains_key(b"AP"));
    assert!(out.objects().into_iter().count() < 24);
}

#[test]
fn nested_indirect_geometry_cycles_dags_and_null_filespec_entries_are_handled() {
    let engine = InProcessEngine::new(Arc::new(Database::new()));
    let more = vec![
        // 5: Square with indirect dash array inside /BS
        "<< /Type /Annot /Subtype /Square /Rect [30 40 70 80] /BS << /W 2 /S /D /D 10 0 R >> >>"
            .to_string(),
        // 6: Polygon with indirect /Path subpath AND 3-level nested subpath
        "<< /Type /Annot /Subtype /Polygon /Rect [30 40 100 100] \
         /Vertices [30 40 90 40 60 90] /Path [11 0 R [[40 50] [50 60 70 80 90 100]]] >>"
            .to_string(),
        // 7: Cyclic /Vertices [12 0 R] -> dropped cleanly without hang
        "<< /Type /Annot /Subtype /Polygon /Rect [30 40 70 80] /Vertices 12 0 R >>".to_string(),
        // 8: FileAttachment with explicit PDF `null` optional keys (ISO 32000-1 §7.3.9)
        "<< /Type /Annot /Subtype /FileAttachment /Rect [30 40 50 60] \
         /FS << /Type null /F (ok.txt) /UF null /Desc null /AFRelationship null /EF << /F 13 0 R /UF null >> >> >>"
            .to_string(),
        // 9: FileAttachment with /FS /URL + /EF (must reject) and /Params /Launch (must reject)
        "<< /Type /Annot /Subtype /FileAttachment /Rect [30 40 50 60] \
         /FS << /Type /Filespec /FS /URL /F (https://evil.example/a) /EF << /F 13 0 R >> >> >>"
            .to_string(),
        "[4 2]".to_string(),
        "[30 40 90 40 60 90]".to_string(),
        "[12 0 R]".to_string(),
        "<< /Type /EmbeddedFile /Length 2 >>\nstream\nok\nendstream".to_string(),
        // 14: FileAttachment whose stream /Params attempts /S /Launch /F smuggling
        "<< /Type /Annot /Subtype /FileAttachment /Rect [30 40 50 60] \
         /FS << /Type /Filespec /F (bad.txt) /EF << /F 15 0 R >> >> >>"
            .to_string(),
        "<< /Type /EmbeddedFile /Params << /S /Launch /F (/bin/sh) >> /Length 2 >>\nstream\nno\nendstream"
            .to_string(),
    ];
    let built = rebuilt(annotated("5 0 R 6 0 R 7 0 R 8 0 R 9 0 R 14 0 R", more));
    assert_eq!(
        built.warnings,
        ["지원하지 않는 주석 3개는 저장하지 못했습니다 (FileAttachment 2, Polygon 1)."]
    );
    let out = read(&built.bytes);
    let annots = page_annots(&out);
    assert_eq!(annots.len(), 3);
    assert_eq!(
        numbers(
            annots[0]
                .get::<Dict<'_>>(b"BS")
                .and_then(|bs| bs.get::<Array<'_>>(b"D"))
                .unwrap()
        ),
        [4.0, 2.0]
    );
    let path = annots[1].get::<Array<'_>>(b"Path").unwrap();
    let mut subpaths = path.iter::<Array<'_>>();
    assert_eq!(
        numbers(subpaths.next().unwrap()),
        [10.0, 10.0, 70.0, 10.0, 40.0, 60.0]
    );
    let nested_pts: Vec<Vec<f32>> = subpaths
        .next()
        .unwrap()
        .iter::<Array<'_>>()
        .map(numbers)
        .collect();
    assert_eq!(
        nested_pts,
        [vec![20.0, 20.0], vec![30.0, 30.0, 50.0, 50.0, 70.0, 70.0]]
    );
    assert_eq!(
        text(&annots[2].get::<Dict<'_>>(b"FS").unwrap(), b"F").as_deref(),
        Some("ok.txt")
    );

    // Exponential diamond DAG in /Vertices hits MAX_VALUE_NODES fail-closed in bounded time.
    let mut dag =
        vec!["<< /Type /Annot /Subtype /Polygon /Rect [0 0 10 10] /Vertices 6 0 R >>".to_string()];
    for idx in 0..22 {
        let next = 7 + idx;
        dag.push(format!("[{next} 0 R {next} 0 R]"));
    }
    dag.push("[1 2]".to_string());
    let err = EditSession::open(annotated("5 0 R", dag), &engine)
        .unwrap()
        .render(None, &engine)
        .unwrap_err();
    assert!(err.contains("크기"), "{err}");

    // Dropped invalid FileAttachments do not consume the carried MAX_ATTACHMENTS budget.
    let mut objs = vec!["<< /Type /EmbeddedFile /Length 2 >>\nstream\nhi\nendstream".to_string()];
    let bad_count = crate::pdf_edit_budget::MAX_ATTACHMENTS + 1;
    let mut refs = Vec::with_capacity(bad_count + 1);
    for idx in 0..bad_count {
        refs.push(format!("{} 0 R", 6 + idx));
        objs.push(
            "<< /Type /Annot /Subtype /FileAttachment /Rect [0 0 10 10] /FS (/tmp/ext) >>"
                .to_string(),
        );
    }
    refs.push(format!("{} 0 R", 6 + bad_count));
    objs.push(
        "<< /Type /Annot /Subtype /FileAttachment /Rect [0 0 10 10] \
         /FS << /Type /Filespec /F (kept.txt) /EF << /F 5 0 R >> >> >>"
            .to_string(),
    );
    let ok = rebuilt(annotated(&refs.join(" "), objs));
    assert_eq!(page_annots(&read(&ok.bytes)).len(), 1);
    assert_eq!(
        ok.warnings,
        [format!(
            "지원하지 않는 주석 {bad_count}개는 저장하지 못했습니다 (FileAttachment {bad_count})."
        )]
    );

    // Rotated + cropped page with all 5 annotation kinds renders cleanly after edit + rotate.
    let rotated = hand_written_with_xref(
        &[
            (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (
                3,
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /CropBox [20 30 200 300] \
                 /Rotate 90 /Contents 4 0 R /Annots [5 0 R] >>"
                    .to_string(),
            ),
            (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
            (
                5,
                "<< /Type /Annot /Subtype /Square /Rect [40 50 140 110] /AP << /N 6 0 R >> >>"
                    .to_string(),
            ),
            (6, form(AP_CONTENT)),
        ],
        "",
    );
    let mut s = EditSession::open(rotated, &engine).unwrap();
    s.rotate(None, 90).unwrap();
    let built_rot = s.render(None, &engine).unwrap();
    assert!(matches!(
        read(&built_rot.bytes).pages()[0].rotation(),
        hayro_syntax::page::Rotation::Flipped
    ));
    let png = crate::pdf_first_page::first_page_png(
        &Arc::new(built_rot.bytes),
        None,
        &crate::pdf_edit_fixtures::deadline(),
    )
    .unwrap();
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
}
