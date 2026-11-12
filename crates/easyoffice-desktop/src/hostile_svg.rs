//! SVGs that declare entities (or inflate) far past their size, for the tests of the places that
//! read page and picture SVGs. The expansion is kept small (a few MB) so a test that lets one
//! through fails by its assertion, not by exhausting the machine.

use easyoffice_formats::base64_codec;
use easyoffice_formats::deflate::deflate_raw;
use easyoffice_formats::IeeeCrc32;

const SVG_OPEN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="8" height="8">"#;

/// An SVG whose text is `&e{levels};`, each entity ten of the one below (`10^levels` copies of the
/// first), in the usual billion-laughs shape.
pub(crate) fn entity_bomb(levels: usize) -> String {
    let mut declarations = String::from(r#"<!ENTITY e0 "AAAA">"#);
    for level in 1..=levels {
        let below = format!("&e{};", level - 1).repeat(10);
        declarations.push_str(&format!(r#"<!ENTITY e{level} "{below}">"#));
    }
    format!(
        r#"<?xml version="1.0"?><!DOCTYPE svg [{declarations}]>{SVG_OPEN}<text x="0" y="8">&e{levels};</text></svg>"#
    )
}

/// A plain SVG that draws a rectangle.
pub(crate) fn square() -> String {
    format!(r#"{SVG_OPEN}<rect width="8" height="8"/></svg>"#)
}

/// `svg` as a gzip file (the `.svgz` form).
pub(crate) fn gzip(svg: &[u8]) -> Vec<u8> {
    let mut out = vec![0x1F, 0x8B, 8, 0, 0, 0, 0, 0, 0, 3];
    out.extend_from_slice(&deflate_raw(svg));
    out.extend_from_slice(&IeeeCrc32::checksum(svg).to_le_bytes());
    out.extend_from_slice(&(svg.len() as u32).to_le_bytes());
    out
}

/// A page that draws `picture` (SVG text or gzip bytes) as a `data:image/svg+xml` picture.
pub(crate) fn page_with_picture(picture: &[u8]) -> String {
    format!(
        r#"{SVG_OPEN}<image width="8" height="8" xlink:href="data:image/svg+xml;base64,{}"/></svg>"#,
        base64_codec::encode(picture)
    )
}
