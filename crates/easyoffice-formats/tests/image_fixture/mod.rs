//! Shared builders for small, structurally valid PNG and JPEG test pictures.

use easyoffice_formats::{deflate_zlib, IeeeCrc32};

pub const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

pub fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = IeeeCrc32::update(IeeeCrc32::checksum(kind), data);
    out.extend_from_slice(&crc.to_be_bytes());
    out
}

/// A valid PNG with unfiltered scanlines of a gradient pattern.
pub fn make_png(width: u32, height: u32, color_type: u8, channels: usize) -> Vec<u8> {
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, color_type, 0, 0, 0]);
    let mut raw = Vec::new();
    for y in 0..height as usize {
        raw.push(0);
        for x in 0..width as usize * channels {
            raw.push((x * 7 + y * 13) as u8);
        }
    }
    let mut png = PNG_SIGNATURE.to_vec();
    png.extend(png_chunk(b"IHDR", &ihdr));
    png.extend(png_chunk(b"IDAT", &deflate_zlib(&raw)));
    png.extend(png_chunk(b"IEND", &[]));
    png
}

pub fn make_jpeg(width: u16, height: u16) -> Vec<u8> {
    let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00];
    jpeg.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 8]);
    jpeg.extend_from_slice(&height.to_be_bytes());
    jpeg.extend_from_slice(&width.to_be_bytes());
    jpeg.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
    jpeg.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x0C, 3, 1, 0, 2, 0x11, 3, 0x11, 0, 63, 0]);
    jpeg.extend_from_slice(&[0x12, 0x34, 0x56, 0xFF, 0xD9]);
    jpeg
}
