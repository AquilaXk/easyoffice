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

// --- SVG pictures and pages that declare entities --------------------------------------------

fn images_in(group: &usvg::Group) -> usize {
    group
        .children()
        .iter()
        .map(|node| match node {
            usvg::Node::Group(inner) => images_in(inner),
            usvg::Node::Image(_) => 1,
            _ => 0,
        })
        .sum()
}

fn kept_pictures(page: &str) -> usize {
    let options = parse_options(&Arc::new(Database::new()));
    images_in(usvg::Tree::from_str(page, &options).expect("page").root())
}

#[test]
fn a_nested_svg_that_declares_entities_is_dropped_and_the_page_survives() {
    use crate::hostile_svg::{entity_bomb, gzip, page_with_picture, square};
    let started = std::time::Instant::now();
    // Controls: a plain and a gzip picture are read.
    assert_eq!(kept_pictures(&page_with_picture(square().as_bytes())), 1);
    assert_eq!(
        kept_pictures(&page_with_picture(&gzip(square().as_bytes()))),
        1
    );
    let bomb = entity_bomb(6);
    for picture in [
        bomb.clone().into_bytes(),
        gzip(bomb.as_bytes()),
        // A declaration that is not shaped like the usual bomb is refused as well.
        format!("<!DOCTYPE svg>{}", square()).into_bytes(),
        format!("<!  doctype svg>{}", square()).into_bytes(),
    ] {
        assert_eq!(kept_pictures(&page_with_picture(&picture)), 0);
    }
    let (pdf, _) = export(&[page_with_picture(bomb.as_bytes())]);
    assert!(verify_exported_pdf(&pdf, 1, false).is_ok());
    assert!(started.elapsed().as_secs() < 5, "{:?}", started.elapsed());
}

#[test]
fn a_gzip_picture_that_inflates_past_the_bound_is_dropped() {
    use crate::hostile_svg::{gzip, page_with_picture};
    let big = format!("<svg>{}</svg>", " ".repeat(MAX_NESTED_SVG_BYTES + 1));
    let packed = gzip(big.as_bytes());
    assert!(
        packed.len() < MAX_NESTED_SVG_BYTES / 100,
        "{}",
        packed.len()
    );
    assert_eq!(kept_pictures(&page_with_picture(&packed)), 0);
}

#[test]
fn a_page_that_declares_entities_is_refused_before_it_is_parsed() {
    use crate::hostile_svg::entity_bomb;
    let started = std::time::Instant::now();
    let mut assembler = PdfAssembler::new(&Arc::new(Database::new()), true);
    let refused = assembler.add_pages(&[entity_bomb(6)]).unwrap_err();
    assert!(refused.contains("1쪽"), "{refused}");
    assert!(started.elapsed().as_secs() < 5);
}

#[test]
fn only_raster_and_svg_types_reach_the_picture_readers() {
    use crate::hostile_svg::{
        page_with_href, picture_that_looks_like_a_raster_header, square, untyped_data_urls,
    };
    // The picture passes the header reader, so only its type keeps it out of usvg's default
    // resolver, which would parse it as an SVG with its entity declarations.
    let trap = picture_that_looks_like_a_raster_header();
    assert!(
        RasterBudget::for_export().admit(&trap),
        "the header reader takes it"
    );
    for payload in [trap.as_slice(), square().as_bytes()] {
        for href in untyped_data_urls(payload) {
            assert_eq!(kept_pictures(&page_with_href(&href)), 0, "{href:.60}");
        }
    }
    let svg_href = |mime: &str, payload: &[u8]| {
        format!("data:{mime};base64,{}", base64_codec::encode(payload))
    };
    // The SVG type is matched whatever its case; a declaration still keeps the picture out.
    assert_eq!(
        kept_pictures(&page_with_href(&svg_href(
            "IMAGE/SVG+XML",
            square().as_bytes()
        ))),
        1
    );
    assert_eq!(
        kept_pictures(&page_with_href(&svg_href("Image/Svg+Xml", &trap))),
        0
    );
    // A raster type of any case still goes on to the decoders.
    let png = base64_codec::encode(&png(8, 8));
    for mime in ["image/png", "IMAGE/PNG"] {
        assert_eq!(
            kept_pictures(&page_with_href(&format!("data:{mime};base64,{png}"))),
            1
        );
    }
}
