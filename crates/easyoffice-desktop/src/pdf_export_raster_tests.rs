//! Pictures in the exported PDF (PD-145): a picture over the pixel limits is left out and the page
//! is exported, one within them is kept.

use super::*;
use easyoffice_formats::base64_codec;

fn page_with(pictures: &[Vec<u8>]) -> String {
    let images: String = pictures
        .iter()
        .map(|bytes| {
            format!(
                r#"<image x="0" y="0" width="40" height="40" xlink:href="data:image/png;base64,{}"/>"#,
                base64_codec::encode(bytes)
            )
        })
        .collect();
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="100" height="100">{images}</svg>"#
    )
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut pixmap = tiny_skia::Pixmap::new(width, height).unwrap();
    pixmap.fill(tiny_skia::Color::from_rgba8(200, 30, 30, 255));
    pixmap.encode_png().unwrap()
}

/// The signature and the header chunk: the declared size and no pixel data.
fn png_header(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
    bytes
}

fn export(pages: &[String]) -> (Vec<u8>, u32) {
    let mut assembler = PdfAssembler::new(&Arc::new(Database::new()), true);
    assembler.add_pages(pages).expect("pages");
    let skipped = assembler.skipped_pictures();
    (assembler.finish().expect("pdf"), skipped)
}

fn image_objects(pdf: &[u8]) -> usize {
    pdf.windows(b"/Subtype /Image".len())
        .filter(|w| *w == b"/Subtype /Image")
        .count()
}

#[test]
fn a_declared_size_bomb_is_left_out_and_the_page_is_exported() {
    let (pdf, skipped) = export(&[page_with(&[png_header(100_000, 100_000), png(8, 8)])]);
    assert_eq!(skipped, 1);
    assert_eq!(
        image_objects(&pdf),
        1,
        "only the real picture is in the PDF"
    );
    assert!(verify_exported_pdf(&pdf, 1, false).is_ok());
}

#[test]
fn a_large_photo_is_kept() {
    let (pdf, skipped) = export(&[page_with(&[png(6000, 4000)])]);
    assert_eq!(skipped, 0);
    assert_eq!(image_objects(&pdf), 1);
}

#[test]
fn every_page_has_a_budget_of_its_own() {
    let options = ExportOptions::new(Arc::new(Database::new()));
    let (_, first) = options.page();
    let (_, second) = options.page();
    for _ in 0..4 {
        assert!(first.admit(&png_header(8000, 8000)));
    }
    assert!(
        !first.admit(&png_header(8000, 8000)),
        "the first page is used up"
    );
    assert!(
        second.admit(&png_header(8000, 8000)),
        "the second page is not"
    );
}

#[test]
fn a_picture_inside_a_nested_svg_counts_in_the_same_page_budget() {
    let inner = page_with(&[png_header(100_000, 100_000)]);
    let outer = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="100" height="100"><image width="100" height="100" xlink:href="data:image/svg+xml;base64,{}"/></svg>"#,
        base64_codec::encode(inner.as_bytes())
    );
    let (pdf, skipped) = export(&[outer]);
    assert_eq!(skipped, 1);
    assert!(verify_exported_pdf(&pdf, 1, false).is_ok());
}

#[test]
fn export_with_pictures_is_deterministic() {
    let pages = [page_with(&[png(64, 64), png_header(100_000, 100_000)])];
    assert_eq!(export(&pages).0, export(&pages).0);
}
