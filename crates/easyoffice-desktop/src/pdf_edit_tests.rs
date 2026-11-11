//! Tests of the edit pipeline on well-formed files (PD-166): rotation, split, merge, overlays, and
//! that the original objects survive. The results are read back with the reader.

use crate::pdf_edit_budget::MAX_PAGES;
use crate::pdf_edit_engine::InProcessEngine;
use crate::pdf_edit_fixtures::{
    all_pictures, heavy_pdf, picture_bytes, pictures_by_page, read, sample_pdf, PAGE_HEIGHT,
    PAGE_WIDTH,
};
use crate::pdf_edit_overlay::Overlay;
use crate::pdf_edit_session::EditSession;
use crate::save_file::SaveRequest;
use hayro_syntax::object::dict::keys::{BASE_FONT, FONT, RESOURCES, XOBJECT};
use hayro_syntax::object::{Dict, Name, Object, Stream};
use hayro_syntax::page::Rotation;
use std::sync::Arc;
use usvg::fontdb::Database;

fn engine() -> InProcessEngine {
    InProcessEngine::new(Arc::new(Database::new()))
}

fn open(bytes: Vec<u8>) -> EditSession {
    EditSession::open(bytes, &engine()).expect("opens")
}

fn rotations(bytes: &[u8]) -> Vec<u16> {
    read(bytes)
        .pages()
        .iter()
        .map(|p| match p.rotation() {
            Rotation::None => 0,
            Rotation::Horizontal => 90,
            Rotation::Flipped => 180,
            Rotation::FlippedHorizontal => 270,
        })
        .collect()
}

fn expected_pictures(tag: u8, pages: impl IntoIterator<Item = usize>) -> Vec<Vec<u8>> {
    pages
        .into_iter()
        .map(|n| picture_bytes(tag, n).to_vec())
        .collect()
}

#[test]
fn a_file_opens_and_comes_back_whole() {
    let session = open(sample_pdf(1, 4, &[]));
    assert_eq!((session.page_count(), session.source_count()), (4, 1));
    let out = session.render(None, &engine()).unwrap();
    assert_eq!(read(&out).pages().len(), 4);
    assert_eq!(rotations(&out), [0, 0, 0, 0]);
    assert_eq!(pictures_by_page(&out), expected_pictures(1, 1..=4));
    // What the shell saves is checked as the engine checks a saved document.
    SaveRequest::from_bytes("편집.pdf", out).expect("the engine accepts the result");
}

#[test]
fn rotation_sets_rotate_and_adds_to_the_pages_own() {
    let mut session = open(sample_pdf(2, 4, &[0, 90, 180, 270]));
    session.rotate(None, 90).unwrap();
    session.rotate(Some("1"), 90).unwrap();
    session.rotate(Some("4"), -180).unwrap();
    let out = session.render(None, &engine()).unwrap();
    assert_eq!(rotations(&out), [180, 180, 270, 180]);
    // The size of the page does not change: only `/Rotate` does.
    let pdf = read(&out);
    for page in pdf.pages().iter() {
        let media = page.media_box();
        assert_eq!(
            (media.width() as f32, media.height() as f32),
            (PAGE_WIDTH, PAGE_HEIGHT)
        );
    }
    assert_eq!(pictures_by_page(&out), expected_pictures(2, 1..=4));
}

#[test]
fn an_unrotated_page_gets_exactly_the_requested_rotate() {
    let mut session = open(sample_pdf(3, 2, &[]));
    session.rotate(Some("2"), 270).unwrap();
    let out = session.render(None, &engine()).unwrap();
    assert_eq!(rotations(&out), [0, 270]);
    let pdf = read(&out);
    assert_eq!(
        pdf.pages()[1].render_dimensions(),
        (PAGE_HEIGHT, PAGE_WIDTH),
        "a page turned sideways is shown sideways"
    );
}

#[test]
fn a_range_becomes_its_own_file_with_only_its_pictures() {
    let session = open(sample_pdf(4, 6, &[]));
    let ranges = session.split_ranges(Some("2-3, 6")).unwrap();
    let files: Vec<Vec<u8>> = ranges
        .into_iter()
        .map(|r| session.render(Some(r), &engine()).unwrap())
        .collect();
    assert_eq!(files.len(), 2);
    assert_eq!(read(&files[0]).pages().len(), 2);
    assert_eq!(read(&files[1]).pages().len(), 1);
    assert_eq!(pictures_by_page(&files[0]), expected_pictures(4, 2..=3));
    assert_eq!(pictures_by_page(&files[1]), expected_pictures(4, 6..=6));
    // Nothing of the other pages is carried along.
    let mut only = all_pictures(&files[0]);
    only.sort();
    let mut wanted = expected_pictures(4, 2..=3);
    wanted.sort();
    assert_eq!(only, wanted);
}

#[test]
fn a_split_keeps_the_turns_chosen_before_it() {
    let mut session = open(sample_pdf(5, 3, &[]));
    session.rotate(Some("2"), 90).unwrap();
    let ranges = session.split_ranges(None).unwrap();
    let files: Vec<Vec<u8>> = ranges
        .into_iter()
        .map(|r| session.render(Some(r), &engine()).unwrap())
        .collect();
    let turns: Vec<u16> = files.iter().map(|f| rotations(f)[0]).collect();
    assert_eq!(turns, [0, 90, 0]);
}

#[test]
fn merging_appends_the_pages_of_the_other_files_in_order() {
    let mut session = open(sample_pdf(6, 3, &[]));
    let merged = session
        .merge(
            vec![sample_pdf(7, 2, &[90, 0]), sample_pdf(8, 1, &[])],
            &engine(),
        )
        .unwrap();
    assert_eq!(
        (merged, session.page_count(), session.source_count()),
        (2, 6, 3)
    );
    let out = session.render(None, &engine()).unwrap();
    assert_eq!(read(&out).pages().len(), 6);
    assert_eq!(rotations(&out), [0, 0, 0, 90, 0, 0]);
    let mut expected = expected_pictures(6, 1..=3);
    expected.extend(expected_pictures(7, 1..=2));
    expected.extend(expected_pictures(8, 1..=1));
    assert_eq!(pictures_by_page(&out), expected);
}

#[test]
fn a_font_the_pages_share_is_copied_once_and_stays_a_font() {
    let session = open(sample_pdf(9, 5, &[]));
    let out = session.render(None, &engine()).unwrap();
    let pdf = read(&out);
    let mut fonts = 0;
    for object in pdf.objects() {
        if let Object::Dict(dict) = object {
            if dict
                .get::<Name<'_>>(BASE_FONT)
                .is_some_and(|n| n.as_str() == "Helvetica")
            {
                fonts += 1;
            }
        }
    }
    assert_eq!(fonts, 1, "one font object for five pages");
    // The page's form still names it as its font.
    let page = &pdf.pages()[0];
    let form = page
        .raw()
        .get::<Dict<'_>>(RESOURCES)
        .and_then(|r| r.get::<Dict<'_>>(XOBJECT))
        .and_then(|x| x.keys().next().and_then(|k| x.get::<Stream<'_>>(&*k)))
        .expect("form");
    let used = form
        .dict()
        .get::<Dict<'_>>(RESOURCES)
        .and_then(|r| r.get::<Dict<'_>>(FONT));
    assert!(used.is_some_and(|d| !d.is_empty()));
}

#[test]
fn the_text_of_a_page_is_still_in_its_content() {
    let session = open(sample_pdf(10, 3, &[]));
    let out = session.render(Some(1..2), &engine()).unwrap();
    let pdf = read(&out);
    let form = pdf.pages()[0]
        .raw()
        .get::<Dict<'_>>(RESOURCES)
        .and_then(|r| r.get::<Dict<'_>>(XOBJECT))
        .and_then(|x| x.keys().next().and_then(|k| x.get::<Stream<'_>>(&*k)))
        .expect("form");
    let content = form.decoded().expect("decodes");
    assert!(
        String::from_utf8_lossy(&content).contains("(Page 2 of 10) Tj"),
        "{}",
        String::from_utf8_lossy(&content)
    );
}

#[test]
fn a_result_opens_again_and_can_be_edited_again() {
    let mut first = open(sample_pdf(11, 3, &[0, 90, 0]));
    first.rotate(Some("1"), 90).unwrap();
    let out = first.render(None, &engine()).unwrap();
    let mut second = open(out);
    assert_eq!(second.page_count(), 3);
    second.rotate(Some("1"), 90).unwrap();
    let again = second.render(None, &engine()).unwrap();
    assert_eq!(rotations(&again), [180, 90, 0]);
    assert_eq!(pictures_by_page(&again), expected_pictures(11, 1..=3));
}

#[test]
fn the_title_of_the_first_file_is_kept() {
    let mut pdf = hayro_write::pdf_writer::Pdf::new();
    let mut next = hayro_write::pdf_writer::Ref::new(1);
    let (catalog, tree, page, info) = (next.bump(), next.bump(), next.bump(), next.bump());
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree).count(1).kids([page]);
    pdf.page(page)
        .media_box(hayro_write::pdf_writer::Rect::new(0.0, 0.0, 50.0, 50.0))
        .parent(tree);
    pdf.document_info(info)
        .title(hayro_write::pdf_writer::TextStr("보고서 제목"));
    let bytes = pdf.finish();
    let source_title = read(&bytes).metadata().title.clone().expect("source title");
    let out = open(bytes).render(None, &engine()).unwrap();
    assert_eq!(read(&out).metadata().title.clone(), Some(source_title));
}

#[test]
fn merged_pages_past_the_page_limit_are_refused_and_nothing_is_added() {
    let mut session = open(sample_pdf(12, 2, &[]));
    session.pages_for_test(MAX_PAGES - 1);
    let before = session.page_count();
    let err = session
        .merge(vec![sample_pdf(13, 2, &[])], &engine())
        .unwrap_err();
    assert!(err.contains("쪽 수"), "{err}");
    assert_eq!((session.page_count(), session.source_count()), (before, 1));
}

#[test]
fn the_overlay_text_is_a_real_font_and_the_original_is_still_there() {
    let Some(db) = crate::pdf_fonts::tests::bundled_only_db() else {
        return;
    };
    let db = Arc::new(db);
    let mut session = open(sample_pdf(14, 3, &[0, 90, 0]));
    session.set_overlay(Overlay::new(Some("대외비 · 기획조정실"), Some("승인필")).unwrap());
    let out = session
        .render(None, &InProcessEngine::new(Arc::clone(&db)))
        .unwrap();
    let pdf = read(&out);
    assert_eq!(pdf.pages().len(), 3);
    assert_eq!(pictures_by_page(&out), expected_pictures(14, 1..=3));
    // Each page draws two forms (the original and the overlay) ...
    for page in pdf.pages().iter() {
        let forms = page
            .raw()
            .get::<Dict<'_>>(RESOURCES)
            .and_then(|r| r.get::<Dict<'_>>(XOBJECT))
            .expect("forms");
        assert_eq!(forms.keys().count(), 2);
    }
    // ... and the overlay carries an embedded font (a Type0 font with its glyph subset).
    let embedded = pdf
        .objects()
        .into_iter()
        .filter(|o| matches!(o, Object::Dict(d) if d.get::<Name<'_>>(BASE_FONT).is_some_and(|n| n.as_str().contains("Noto"))))
        .count();
    assert!(embedded >= 1, "no embedded Noto font in the overlay");
    // The overlay is searchable text.
    let raw = String::from_utf8_lossy(&out).into_owned();
    assert!(raw.contains("/ToUnicode"));
    assert!(
        out.len() > sample_pdf(14, 3, &[0, 90, 0]).len(),
        "the overlay adds objects"
    );
}

#[test]
fn only_pages_of_equal_size_share_an_overlay_form() {
    let Some(db) = crate::pdf_fonts::tests::bundled_only_db() else {
        return;
    };
    let db = Arc::new(db);
    let mut session = open(sample_pdf(15, 5, &[]));
    session.set_overlay(Overlay::new(Some("초안"), None).unwrap());
    let out = session
        .render(None, &InProcessEngine::new(Arc::clone(&db)))
        .unwrap();
    let pdf = read(&out);
    let mut forms = std::collections::HashSet::new();
    for page in pdf.pages().iter() {
        let x = page
            .raw()
            .get::<Dict<'_>>(RESOURCES)
            .and_then(|r| r.get::<Dict<'_>>(XOBJECT))
            .unwrap();
        let overlay = x.get_ref(b"W").expect("overlay form");
        forms.insert((overlay.obj_number, overlay.gen_number));
    }
    assert_eq!(forms.len(), 1, "five equal pages, one overlay");
}

/// The matrix the library puts on the copy and the one this module undoes it with agree with the
/// rotation direction of `/Rotate` (clockwise): a corner of the page lands where the turned page
/// shows it.
#[test]
fn the_form_matrix_agrees_with_the_direction_of_rotate() {
    for (rotate, expect_corner) in [
        // The lower left corner of the unrotated page (0,0) is shown at ...
        (90, (0.0, PAGE_WIDTH)),
        (180, (PAGE_WIDTH, PAGE_HEIGHT)),
        (270, (PAGE_HEIGHT, 0.0)),
    ] {
        let pdf = read(&sample_pdf(16, 1, &[rotate]));
        let m = pdf.pages()[0].initial_transform(false).as_coeffs();
        let (x, y) = (m[4], m[5]);
        let shown = (x as f32, y as f32);
        assert!(
            (shown.0 - expect_corner.0).abs() < 1e-3 && (shown.1 - expect_corner.1).abs() < 1e-3,
            "rotate {rotate}: origin goes to {shown:?}, expected {expect_corner:?}"
        );
    }
}

#[test]
fn a_hundred_pages_merge_in_a_moment() {
    let mut session = open(sample_pdf(17, 50, &[]));
    session
        .merge(vec![sample_pdf(18, 50, &[0, 90])], &engine())
        .unwrap();
    assert_eq!(session.page_count(), 100);
    let started = std::time::Instant::now();
    let out = session.render(None, &engine()).unwrap();
    let elapsed = started.elapsed();
    eprintln!("merge of 100 pages: {elapsed:?}, {} bytes", out.len());
    assert_eq!(read(&out).pages().len(), 100);
    assert!(elapsed < std::time::Duration::from_secs(20), "{elapsed:?}");
    let mut expected = expected_pictures(17, 1..=50);
    expected.extend(expected_pictures(18, 1..=50));
    assert_eq!(pictures_by_page(&out), expected);
}

#[test]
fn a_hundred_heavy_pages_merge_without_touching_their_pictures() {
    // Two reports of 50 pages: 400 KiB of drawing operators and a 300 KiB picture on every page.
    let (first, second) = (
        heavy_pdf(50, 400 * 1024, 300 * 1024),
        heavy_pdf(50, 400 * 1024, 300 * 1024),
    );
    let input = first.len() + second.len();
    let mut session = open(first);
    session.merge(vec![second], &engine()).unwrap();
    let started = std::time::Instant::now();
    let out = session.render(None, &engine()).unwrap();
    let elapsed = started.elapsed();
    eprintln!(
        "merge of 100 heavy pages: {elapsed:?} ({} MiB in, {} MiB out)",
        input / 1_048_576,
        out.len() / 1_048_576
    );
    assert_eq!(read(&out).pages().len(), 100);
    // Every picture is in the result byte for byte (the pictures are copied, not decoded), and the
    // size is what the inputs add up to, give or take the page contents' new compression.
    assert_eq!(all_pictures_len(&out, 300 * 1024), 100);
    assert!(out.len() < input + input / 4, "{} vs {input}", out.len());
    assert!(elapsed < std::time::Duration::from_secs(60), "{elapsed:?}");
}

fn all_pictures_len(bytes: &[u8], size: usize) -> usize {
    read(bytes)
        .objects()
        .into_iter()
        .filter(|o| matches!(o, Object::Stream(s) if s.raw_data().len() == size))
        .count()
}
