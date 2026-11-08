//! The preview picture of a saved Hangul document (PD-132).
//!
//! The editor's saved bytes keep whatever preview the file had when it was opened (the stale first
//! page of the original, or a 1x1 placeholder for a new HWPX), and the editor has no call to change
//! it. So the page sends the first page as SVG next to the bytes (the same print-profile SVG the
//! PDF export uses), this module draws it small and the engine puts the PNG where the format keeps
//! its preview (`easyoffice_formats::embed_thumbnail`). Every step can fail without costing the
//! save: the caller keeps the document as the editor made it.

use crate::pdf_export;
use crate::pdf_font_fallbacks::add_font_fallbacks;
use crate::save_verify::verify_document_bytes;
use easyoffice_formats::{
    embed_thumbnail, DocumentFormat, MAX_EMBED_DOCUMENT_BYTES, MAX_THUMBNAIL_SIDE_PX,
};
use std::sync::Arc;
use usvg::fontdb::Database;

/// Largest page SVG drawn. A page of text is tens of KB; one with embedded pictures a few MB. The
/// PDF export accepts 64MiB a page, but a preview that costs that much is not worth the wait.
pub const MAX_THUMBNAIL_SVG_BYTES: usize = 16 * 1_048_576;

/// Draws the page SVG as an opaque white-backed PNG whose longer side is
/// [`MAX_THUMBNAIL_SIDE_PX`]. The SVG is parsed with the options of the PDF export (the shared
/// fonts and fallback faces, no file access from `href`s).
pub fn render_png(svg: &str, fonts: &Arc<Database>) -> Result<Vec<u8>, String> {
    if svg.len() > MAX_THUMBNAIL_SVG_BYTES {
        return Err("쪽 SVG가 미리보기 한도를 넘습니다.".to_string());
    }
    let svg = add_font_fallbacks(svg);
    let tree = usvg::Tree::from_data(svg.as_bytes(), &pdf_export::parse_options(fonts))
        .map_err(|err| format!("쪽 SVG를 읽지 못했습니다: {err}"))?;
    let (width, height) = (tree.size().width(), tree.size().height());
    if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
        return Err("쪽 크기가 올바르지 않습니다.".to_string());
    }
    let scale = MAX_THUMBNAIL_SIDE_PX as f32 / width.max(height);
    let pixels = |side: f32| ((side * scale).round() as u32).clamp(1, MAX_THUMBNAIL_SIDE_PX);
    let mut pixmap = tiny_skia::Pixmap::new(pixels(width), pixels(height))
        .ok_or_else(|| "미리보기 그림을 만들지 못했습니다.".to_string())?;
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
        .encode_png()
        .map_err(|err| format!("미리보기 그림을 PNG로 만들지 못했습니다: {err}"))
}

/// `document` with the first page drawn from `svg` as its preview picture. The result passed the
/// same check every saved document passes; any failure is returned for the caller to log.
pub fn with_first_page(
    extension: &str,
    document: &[u8],
    svg: &str,
    fonts: &Arc<Database>,
) -> Result<Vec<u8>, String> {
    let format = DocumentFormat::from_extension(extension)
        .filter(|format| easyoffice_formats::supports_thumbnail(*format))
        .ok_or_else(|| format!(".{extension}에는 미리보기 그림 자리가 없습니다."))?;
    if document.len() > MAX_EMBED_DOCUMENT_BYTES {
        return Err("문서가 커서 미리보기 그림을 넣지 않습니다.".to_string());
    }
    let png = render_png(svg, fonts)?;
    let embedded = embed_thumbnail(format, document, &png).map_err(|err| err.to_string())?;
    verify_document_bytes(extension, &embedded).map_err(|err| err.to_string())?;
    Ok(embedded)
}

#[cfg(test)]
#[path = "save_thumbnail_tests.rs"]
mod tests;
