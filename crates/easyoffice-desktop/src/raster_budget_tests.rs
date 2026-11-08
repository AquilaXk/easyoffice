use super::*;

fn png(width: u32, height: u32) -> Vec<u8> {
    tiny_skia::Pixmap::new(width, height)
        .unwrap()
        .encode_png()
        .unwrap()
}

/// Only the signature and the header chunk: the declared size, no pixel data at all.
fn png_header(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
    bytes
}

#[test]
fn a_picture_within_the_limits_is_admitted() {
    assert!(RasterBudget::new(64, 64).admit(&png(8, 8)));
}

#[test]
fn a_picture_over_the_per_picture_limit_is_refused() {
    assert!(!RasterBudget::new(63, 1_000).admit(&png(8, 8)));
}

#[test]
fn a_declared_size_over_the_limit_is_refused_without_any_pixel_data() {
    let budget = RasterBudget::for_page();
    assert!(!budget.admit(&png_header(4097, 4096)));
    assert!(!budget.admit(&png_header(u32::MAX, u32::MAX)));
    assert!(budget.admit(&png_header(4096, 4096)));
}

#[test]
fn pictures_add_up_to_the_page_total_and_a_refused_one_costs_nothing() {
    let budget = RasterBudget::new(64, 100);
    assert!(budget.admit(&png(8, 8)));
    assert!(
        !budget.admit(&png(8, 8)),
        "64 + 64 is over the total of 100"
    );
    assert!(
        !budget.admit(&png(9, 9)),
        "81 is over the per-picture limit"
    );
    assert!(budget.admit(&png(6, 6)), "36 still fits in the 36 left");
}

#[test]
fn bytes_that_are_not_a_raster_picture_are_refused() {
    let budget = RasterBudget::for_page();
    assert!(!budget.admit(b""));
    assert!(!budget.admit(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"));
    assert!(!budget.admit(&[0xFF; 64]));
}
