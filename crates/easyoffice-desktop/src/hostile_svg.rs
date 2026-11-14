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

/// An SVG that a header reader takes for an Aseprite picture (`<?p `, then `E0 A5`, which is also
/// the UTF-8 of one letter) and whose entity declaration sets the page width: with a default
/// resolver that reads `text/plain` as any picture, the entity is expanded (`width="&w;"`).
pub(crate) fn picture_that_looks_like_a_raster_header() -> Vec<u8> {
    let mut bytes = b"<?p ".to_vec();
    bytes.extend_from_slice("\u{0940}".as_bytes());
    bytes.extend_from_slice(b" \t\t\t\t?>");
    bytes.extend_from_slice(
        br#"<!DOCTYPE svg [<!ENTITY w "20">]><svg xmlns="http://www.w3.org/2000/svg" width="&w;" height="8"><rect width="8" height="8"/></svg>"#,
    );
    bytes
}

/// A page that draws the picture `href` (a whole data URL).
pub(crate) fn page_with_href(href: &str) -> String {
    format!(r#"{SVG_OPEN}<image width="8" height="8" xlink:href="{href}"/></svg>"#)
}

/// Data URLs that carry `payload` under every spelling that names no raster type: `text/plain`
/// with and without parameters, no type at all, the type in capitals, and percent-encoded text.
pub(crate) fn untyped_data_urls(payload: &[u8]) -> Vec<String> {
    let b64 = base64_codec::encode(payload);
    let percent: String = payload.iter().map(|b| format!("%{b:02X}")).collect();
    vec![
        format!("data:text/plain;base64,{b64}"),
        format!("data:TEXT/PLAIN;base64,{b64}"),
        format!("data:text/plain;charset=utf-8;base64,{b64}"),
        format!("data:;base64,{b64}"),
        format!("data:application/octet-stream;base64,{b64}"),
        format!("data:,{percent}"),
        format!("data:text/plain,{percent}"),
    ]
}

/// An SVG DAG of `<use>` elements that expands to `2^levels` rectangles from `2 * levels + 1` tags.
pub(crate) fn use_explosion_dag(levels: usize) -> String {
    let mut defs = String::from(r#"<g id="u0"><rect width="4" height="4"/></g>"#);
    for lvl in 1..=levels {
        let prev = lvl - 1;
        defs.push_str(&format!(
            r##"<g id="u{lvl}"><use href="#u{prev}"/><use href="#u{prev}"/></g>"##
        ));
    }
    format!(r##"{SVG_OPEN}<defs>{defs}</defs><use href="#u{levels}"/></svg>"##)
}

/// An SVG containing `filters` `<filter>` elements with `primitives` `<feGaussianBlur>` children each.
pub(crate) fn excessive_filters(filters: usize, primitives: usize) -> String {
    let prims = r#"<feGaussianBlur stdDeviation="2"/>"#.repeat(primitives);
    let mut defs = String::new();
    for i in 0..filters {
        defs.push_str(&format!(r#"<filter id="f{i}">{prims}</filter>"#));
    }
    format!(r#"{SVG_OPEN}<defs>{defs}</defs><rect width="8" height="8" filter="url(#f0)"/></svg>"#)
}

/// An SVG containing `tags` empty `<g/>` elements to exhaust element budgets.
pub(crate) fn tag_flood(tags: usize) -> String {
    format!(r#"{SVG_OPEN}{}</svg>"#, "<g/>".repeat(tags))
}
