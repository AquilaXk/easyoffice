//! Pictures inside a page in the thumbnail and the start-window card (PD-132, PD-145).

use super::*;
use easyoffice_formats::base64_codec;

const SAMPLE_JPEG: &[u8] =
    include_bytes!("../../../third_party/rhwp/rhwp-studio/public/samples/images/younghi.jpg");

fn fonts() -> Arc<Database> {
    crate::pdf_fonts::shared_font_db(&[])
}

fn page_with(mime: &str, bytes: &[u8]) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="200" height="200" viewBox="0 0 200 200"><image x="0" y="0" width="200" height="200" preserveAspectRatio="none" xlink:href="data:{mime};base64,{}"/></svg>"#,
        base64_codec::encode(bytes)
    )
}

fn solid_png(width: u32, height: u32, color: tiny_skia::Color) -> Vec<u8> {
    let mut pixmap = tiny_skia::Pixmap::new(width, height).unwrap();
    pixmap.fill(color);
    pixmap.encode_png().unwrap()
}

fn is_white(png: &[u8], x: u32, y: u32) -> bool {
    let pixmap = tiny_skia::Pixmap::decode_png(png).unwrap();
    let p = pixmap.pixel(x, y).unwrap();
    (p.red(), p.green(), p.blue()) == (255, 255, 255)
}

#[test]
fn a_png_inside_the_page_is_drawn() {
    let svg = page_with("image/png", &solid_png(8, 8, tiny_skia::Color::BLACK));
    let png = render_png(&svg, &fonts()).unwrap();
    assert!(!is_white(&png, 10, 10), "the picture is a white box");
}

#[test]
fn a_jpeg_inside_the_page_is_drawn() {
    let png = render_png(&page_with("image/jpeg", SAMPLE_JPEG), &fonts()).unwrap();
    let drawn = (0..png_width(&png)).any(|x| !is_white(&png, x, png_width(&png) / 2));
    assert!(drawn, "the photo is a white box");
}

fn png_width(png: &[u8]) -> u32 {
    u32::from_be_bytes(png[16..20].try_into().unwrap())
}

/// The picture declares 5000 x 5000 pixels in its header and carries none: it is refused before
/// any decoder looks at it, and the rest of the page is still drawn.
#[test]
fn a_picture_declaring_too_many_pixels_is_left_out_and_the_page_is_still_drawn() {
    let mut header = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    header.extend_from_slice(b"IHDR");
    header.extend_from_slice(&5000u32.to_be_bytes());
    header.extend_from_slice(&5000u32.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    let svg = page_with("image/png", &header).replace(
        "</svg>",
        r##"<rect x="0" y="0" width="20" height="20" fill="#ff0000"/></svg>"##,
    );
    let png = render_png(&svg, &fonts()).unwrap();
    assert!(!is_white(&png, 5, 5));
    assert!(is_white(&png, 100, 100));
}

/// A GIF whose logical screen is 1 x 1 but whose frame says 65535 x 65535: the header size passes
/// the budget, and the decoder's own memory limit has to refuse the 16 GiB frame without an abort.
#[test]
fn a_gif_with_a_huge_frame_in_a_tiny_screen_is_dropped_without_exhausting_memory() {
    let mut gif = b"GIF89a".to_vec();
    gif.extend_from_slice(&[1, 0, 1, 0, 0, 0, 0]);
    gif.extend_from_slice(&[
        0x2C, 0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0, 2, 1, 0, 0, 0x3B,
    ]);
    let png = render_png(&page_with("image/gif", &gif), &fonts()).unwrap();
    assert!(is_white(&png, 100, 100));
}

/// A cut-off photo may come out partly drawn; a damaged header or junk leaves the white box. None
/// of them panics or fails the thumbnail.
#[test]
fn damaged_pictures_never_fail_the_thumbnail() {
    let cut = SAMPLE_JPEG[..SAMPLE_JPEG.len() / 2].to_vec();
    render_png(&page_with("image/jpeg", &cut), &fonts()).unwrap();
    let mut junk = vec![0xFF, 0xD8, 0xFF, 0xE0];
    junk.extend((0..2_000u32).map(|i| (i * 31) as u8));
    for (mime, bytes) in [
        ("image/jpeg", junk),
        ("image/webp", b"RIFF\0\0\0\0WEBP".to_vec()),
    ] {
        let png = render_png(&page_with(mime, &bytes), &fonts()).unwrap();
        assert!(is_white(&png, 100, 100));
    }
}

/// Seventeen pictures of 4096 x 1024, each fine alone, are 71 Mpixel: the page total is 64 Mpixel.
#[test]
fn pictures_past_the_page_total_are_left_out() {
    let one = solid_png(4096, 1024, tiny_skia::Color::BLACK);
    let image = |x: u32| {
        format!(
            r#"<image x="{x}" y="0" width="10" height="10" xlink:href="data:image/png;base64,{}"/>"#,
            base64_codec::encode(&one)
        )
    };
    let body: String = (0..17).map(|n| image(n * 11)).collect();
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="200" height="200">{body}</svg>"#
    );
    let png = render_png(&svg, &fonts()).unwrap();
    let scale = png_width(&png) as f32 / 200.0;
    let at = |n: u32| ((n * 11 + 5) as f32 * scale) as u32;
    let row = (5.0 * scale) as u32;
    assert!(!is_white(&png, at(0), row), "the first picture is drawn");
    assert!(
        !is_white(&png, at(15), row),
        "the 16th still fits in the page total"
    );
    assert!(
        is_white(&png, at(16), row),
        "the 17th is past the 64 Mpixel total"
    );
}
