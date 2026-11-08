use super::*;
use crate::preview::{extract_thumbnail, MAX_THUMBNAIL_BYTES};
use crate::save_file::{write_document_bytes, SaveRequest};
use easyoffice_formats::{
    base64_codec, check_thumbnail_png, RichBlock, RichDocument, RichParagraph, RichRun,
    UniversalDocumentModel, UniversalFormatConverter,
};
use std::fs::File;
use std::io::Cursor;
use std::time::Instant;

/// A4 at 96 dpi, the size of a Hangul page SVG, with a red box near the top left corner.
const A4_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="793.7" height="1122.5" viewBox="0 0 793.7 1122.5"><rect x="100" y="100" width="300" height="200" fill="#ff0000"/></svg>"##;

fn fonts() -> Arc<Database> {
    crate::pdf_fonts::shared_font_db(&[])
}

fn model() -> UniversalDocumentModel {
    UniversalDocumentModel {
        title: "저장 시험".to_string(),
        paragraphs: vec!["본문".to_string()],
        ..Default::default()
    }
}

fn rich() -> RichDocument {
    RichDocument {
        title: "저장 시험".to_string(),
        blocks: vec![RichBlock::Paragraph(RichParagraph {
            runs: vec![RichRun::plain("본문")],
            ..RichParagraph::default()
        })],
        official_agency: None,
    }
}

fn hwp() -> Vec<u8> {
    UniversalFormatConverter::encode(&model(), DocumentFormat::Hwp).unwrap()
}

fn hwpx() -> Vec<u8> {
    UniversalFormatConverter::encode_rich(&rich(), DocumentFormat::Hwpx, None).unwrap()
}

fn docx() -> Vec<u8> {
    UniversalFormatConverter::encode_rich(&rich(), DocumentFormat::Docx, None).unwrap()
}

fn pixel(png: &[u8], x: u32, y: u32) -> [u8; 4] {
    let pixmap = tiny_skia::Pixmap::decode_png(png).unwrap();
    let p = pixmap.pixel(x, y).unwrap();
    [p.red(), p.green(), p.blue(), p.alpha()]
}

#[test]
fn a_page_is_drawn_with_its_longer_side_at_the_limit() {
    let png = render_png(A4_SVG, &fonts()).unwrap();
    check_thumbnail_png(&png).unwrap();
    let pixmap = tiny_skia::Pixmap::decode_png(&png).unwrap();
    assert_eq!(pixmap.height(), MAX_THUMBNAIL_SIDE_PX);
    assert_eq!(pixmap.width(), 181);
    // The box sits at 100..400 x 100..300 of 793.7 wide: scaled by 256/1122.5.
    assert_eq!(pixel(&png, 50, 40), [255, 0, 0, 255]);
    // Paper is opaque white wherever the page draws nothing.
    assert_eq!(pixel(&png, 170, 240), [255, 255, 255, 255]);
    assert!(png.len() < MAX_THUMBNAIL_BYTES / 8, "{} bytes", png.len());
}

#[test]
fn a_landscape_page_keeps_its_proportions() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="1122.5" height="793.7" viewBox="0 0 1122.5 793.7"/>"#;
    let pixmap = tiny_skia::Pixmap::decode_png(&render_png(svg, &fonts()).unwrap()).unwrap();
    assert_eq!((pixmap.width(), pixmap.height()), (256, 181));
}

#[test]
fn a_very_thin_page_still_has_a_pixel_on_each_side() {
    let svg =
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="5000" height="2" viewBox="0 0 5000 2"/>"#;
    let pixmap = tiny_skia::Pixmap::decode_png(&render_png(svg, &fonts()).unwrap()).unwrap();
    assert_eq!((pixmap.width(), pixmap.height()), (256, 1));
}

#[test]
fn unreadable_oversized_or_pathological_svg_is_an_error() {
    let fonts = fonts();
    assert!(render_png("not svg at all", &fonts).is_err());
    assert!(render_png("", &fonts).is_err());
    let zero = r#"<svg xmlns="http://www.w3.org/2000/svg" width="0" height="0"/>"#;
    assert!(render_png(zero, &fonts).is_err());
    let big = format!("{A4_SVG}{}", " ".repeat(MAX_THUMBNAIL_SVG_BYTES));
    assert!(render_png(&big, &fonts).unwrap_err().contains("한도"));
    // An external picture is never read from the file system.
    let leak = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><image href="/etc/hosts" width="100" height="100"/></svg>"#;
    assert!(render_png(leak, &fonts).is_ok());
}

#[test]
fn a_hostile_page_with_a_huge_declared_size_costs_a_small_picture_only() {
    // 2^31 user units on a side: the scale shrinks it to the limit, memory stays at 256 pixels.
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="2147483647" height="2147483647" viewBox="0 0 10 10"><rect width="10" height="10" fill="#00f"/></svg>"##;
    let png = render_png(svg, &fonts()).unwrap();
    assert!(tiny_skia::Pixmap::decode_png(&png).unwrap().width() <= MAX_THUMBNAIL_SIDE_PX);
}

#[test]
fn each_format_shows_the_drawn_page_through_the_start_window_reader() {
    let fonts = fonts();
    let expected = render_png(A4_SVG, &fonts).unwrap();
    for (extension, document) in [("hwp", hwp()), ("hwpx", hwpx()), ("docx", docx())] {
        let saved = with_first_page(extension, &document, A4_SVG, &fonts).unwrap();
        let found =
            extract_thumbnail(&mut Cursor::new(saved)).unwrap_or_else(|| panic!("{extension}"));
        assert_eq!(found.mime, "image/png", "{extension}");
        assert_eq!(found.bytes, expected, "{extension}");
    }
}

#[test]
fn a_format_without_a_preview_part_and_a_document_over_the_budget_are_refused() {
    let fonts = fonts();
    assert!(with_first_page("pdf", b"%PDF-1.4", A4_SVG, &fonts).is_err());
    assert!(with_first_page("xlsx", b"x", A4_SVG, &fonts).is_err());
    let huge = vec![0_u8; MAX_EMBED_DOCUMENT_BYTES + 1];
    assert!(with_first_page("hwp", &huge, A4_SVG, &fonts).is_err());
}

#[test]
fn a_failed_preview_never_fails_the_save() {
    let fonts = fonts();
    let docx = docx();
    let base64 = base64_codec::encode(&docx);
    for svg in [
        "broken",
        "",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"0\" height=\"0\"/>",
    ] {
        let request = SaveRequest::parse("문서.docx", &base64)
            .unwrap()
            .with_thumbnail(svg, &fonts);
        assert_eq!(request.bytes(), docx.as_slice(), "{svg:?}");
    }
    // A format that has nowhere to keep the picture is saved as it is.
    let pdf = UniversalFormatConverter::encode(&model(), DocumentFormat::Pdf).unwrap();
    let request = SaveRequest::from_bytes("a.pdf", pdf.clone())
        .unwrap()
        .with_thumbnail(A4_SVG, &fonts);
    assert_eq!(request.bytes(), pdf.as_slice());
}

#[test]
fn the_saved_file_shows_the_picture_and_still_opens_as_a_document() {
    let fonts = fonts();
    let dir = tempfile::tempdir().unwrap();
    for (name, document) in [("a.hwp", hwp()), ("b.hwpx", hwpx()), ("c.docx", docx())] {
        let request = SaveRequest::parse(name, &base64_codec::encode(&document))
            .unwrap()
            .with_thumbnail(A4_SVG, &fonts);
        assert_ne!(request.bytes(), document.as_slice(), "{name}");
        let path = dir.path().join(name);
        write_document_bytes(&path, request.bytes()).unwrap();
        let found = extract_thumbnail(&mut File::open(&path).unwrap()).unwrap();
        assert_eq!(found.mime, "image/png", "{name}");
        // The same bytes pass the check a save is made to pass.
        crate::save_verify::verify_document_bytes(
            request.extension(),
            &std::fs::read(&path).unwrap(),
        )
        .unwrap();
    }
}

/// First-page SVGs of real documents (set `EASYOFFICE_THUMBNAIL_SVG_DIR`), drawn and embedded:
/// `EASYOFFICE_THUMBNAIL_SVG_DIR=<dir> cargo test --release -p easyoffice-desktop thumbnails_are_measured -- --ignored --nocapture`.
#[test]
#[ignore = "measurement"]
fn thumbnails_are_measured() {
    let dir = std::env::var("EASYOFFICE_THUMBNAIL_SVG_DIR").expect("EASYOFFICE_THUMBNAIL_SVG_DIR");
    let started = Instant::now();
    let fonts = crate::pdf_fonts::shared_font_db(&[]);
    println!("font database {:?}", started.elapsed());
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    names.sort();
    let document = hwp();
    for path in names
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "svg"))
    {
        let svg = std::fs::read_to_string(path).unwrap();
        let started = Instant::now();
        let png = render_png(&svg, &fonts);
        let drawn = started.elapsed();
        let started = Instant::now();
        let embedded = with_first_page("hwp", &document, &svg, &fonts).map(|d| d.len());
        println!(
            "{}: svg {} B, png {:?} B in {drawn:?}; whole step {:?} in {:?}",
            path.file_name().unwrap().to_string_lossy(),
            svg.len(),
            png.as_ref().map(Vec::len),
            embedded.map(|len| len - document.len()),
            started.elapsed(),
        );
        if let Ok(png) = png {
            let out = path.with_extension("png");
            std::fs::write(out, png).unwrap();
        }
    }
}
