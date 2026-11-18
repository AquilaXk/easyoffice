//! The preview picture of a saved document (PD-132, PD-239).
//!
//! Hangul saves send the first page as SVG next to the bytes (the print-profile SVG the PDF export
//! uses); non-Hangul saves (`DOCX`, `ODT`, `XLSX`, `ODS`, `PPTX`, `PPSX`, `ODP`, and converted
//! `HWP`/`HWPX`) draw their first page in Rust: `rhwp`'s page layout for documents, and the
//! structured sheet/slide card (`preview::card`) for workbooks and presentations. Every step can
//! fail without costing the save: the caller keeps the document as it was built.

use crate::doc_pdf::MAX_HWPX_BYTES;
use crate::pdf_export;
use crate::pdf_font_fallbacks::add_font_fallbacks;
use crate::preview::{card, card_source, extract_thumbnail, zip as preview_zip};
use crate::raster_budget::RasterBudget;
use crate::save_verify::verify_document_bytes;
use easyoffice_formats::{
    embed_thumbnail, DecodedContent, DocumentFormat, UniversalFormatConverter, XmlSanitizer,
    MAX_EMBED_DOCUMENT_BYTES, MAX_THUMBNAIL_SIDE_PX,
};
use rhwp::DocumentCore;
use std::io::Cursor;
use std::sync::Arc;
use std::thread;
use usvg::fontdb::Database;

/// Largest page SVG drawn. A page of text is tens of KB; one with embedded pictures a few MB. The
/// PDF export accepts 64MiB a page, but a preview that costs that much is not worth the wait.
pub const MAX_THUMBNAIL_SVG_BYTES: usize = 16 * 1_048_576;
/// Stack size for `rhwp` page layout, which recurses with document nesting (same as `doc_pdf.rs`).
const WORKER_STACK_BYTES: usize = 32 * 1024 * 1024;
const ZIP_MAGIC: [u8; 4] = *b"PK\x03\x04";
const PNG_IHDR_PREFIX: [u8; 16] = [
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R',
];

/// Returns `true` when `document` already carries an embedded thumbnail larger than a 1x1
/// placeholder, checked via central-directory/OLE2 seek without unpacking the rest of the package.
pub fn has_non_placeholder_thumbnail(document: &[u8]) -> bool {
    let Some(found) = extract_thumbnail(&mut Cursor::new(document)) else {
        return false;
    };
    if found.mime != "image/png" {
        return true;
    }
    if found.bytes.len() < 24 || found.bytes[..16] != PNG_IHDR_PREFIX {
        return false;
    }
    let width = u32::from_be_bytes([
        found.bytes[16],
        found.bytes[17],
        found.bytes[18],
        found.bytes[19],
    ]);
    let height = u32::from_be_bytes([
        found.bytes[20],
        found.bytes[21],
        found.bytes[22],
        found.bytes[23],
    ]);
    width > 1 || height > 1
}

/// Draws the page SVG as an opaque white-backed PNG whose longer side is
/// [`MAX_THUMBNAIL_SIDE_PX`] (the size the file formats keep as their preview picture).
pub fn render_png(svg: &str, fonts: &Arc<Database>) -> Result<Vec<u8>, String> {
    render_png_sized(svg, fonts, MAX_THUMBNAIL_SIDE_PX)
}

/// [`render_png`] with the longer side `side` pixels (the start window's cards draw larger than a
/// file's preview picture, PD-142). The SVG is parsed with the options of the PDF export (the
/// shared fonts and fallback faces, no file access from `href`s).
pub fn render_png_sized(svg: &str, fonts: &Arc<Database>, side: u32) -> Result<Vec<u8>, String> {
    if svg.len() > MAX_THUMBNAIL_SVG_BYTES {
        return Err("쪽 SVG가 미리보기 한도를 넘습니다.".to_string());
    }
    let svg = add_font_fallbacks(svg);
    // The SVG parser expands entity declarations without a bound: such a page is refused.
    XmlSanitizer::validate_safe_xml(&svg)
        .map_err(|err| format!("쪽 SVG를 읽지 못했습니다: {err}"))?;
    crate::svg_budget::check_svg_structure(&svg)
        .map_err(|err| format!("쪽 SVG를 읽지 못했습니다: {err}"))?;
    let tree = usvg::Tree::from_str(
        &svg,
        &pdf_export::bitmap_parse_options(fonts, RasterBudget::for_page()),
    )
    .map_err(|err| format!("쪽 SVG를 읽지 못했습니다: {err}"))?;
    let (width, height) = (tree.size().width(), tree.size().height());
    if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
        return Err("쪽 크기가 올바르지 않습니다.".to_string());
    }
    let scale = side as f32 / width.max(height);
    if !scale.is_finite() || scale <= 0.0 {
        return Err("쪽 크기가 올바르지 않습니다.".to_string());
    }
    let pixels = |extent: f32| ((extent * scale).round() as u32).clamp(1, side);
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

/// Lays out page 1 of an HWP or HWPX document with `rhwp` on a bounded-stack worker thread and
/// rasterizes it to a [`MAX_THUMBNAIL_SIDE_PX`] PNG (PD-239).
pub fn render_hwpx_first_page_png(hwpx: &[u8], fonts: &Arc<Database>) -> Result<Vec<u8>, String> {
    if hwpx.is_empty() || hwpx.len() > MAX_HWPX_BYTES {
        return Err("문서가 첫 쪽 미리보기로 그릴 수 있는 크기를 넘습니다.".to_string());
    }
    let svg = thread::scope(|scope| {
        thread::Builder::new()
            .name("eo-thumb-layout".into())
            .stack_size(WORKER_STACK_BYTES)
            .spawn_scoped(scope, || -> Result<String, String> {
                let core = DocumentCore::from_bytes(hwpx)
                    .map_err(|err| format!("문서를 배치하지 못했습니다: {err}"))?;
                if core.page_count() == 0 {
                    return Err("문서에 그릴 쪽이 없습니다.".to_string());
                }
                core.render_page_svg_native(0)
                    .map_err(|err| format!("첫 쪽을 그리지 못했습니다: {err}"))
            })
            .map_err(|err| format!("미리보기 작업을 시작하지 못했습니다: {err}"))?
            .join()
            .map_err(|_| "문서 미리보기 배치 중 오류가 났습니다.".to_string())?
    })?;
    render_png(&svg, fonts)
}

fn draw_document_png(
    format: DocumentFormat,
    extension: &str,
    document: &[u8],
    fonts: &Arc<Database>,
) -> Result<Vec<u8>, String> {
    if matches!(
        format,
        DocumentFormat::Hwp | DocumentFormat::Hwt | DocumentFormat::Hwpx | DocumentFormat::Hwtx
    ) {
        return render_hwpx_first_page_png(document, fonts);
    }
    if document.starts_with(&ZIP_MAGIC)
        && preview_zip::declared_total(&mut Cursor::new(document))
            .is_none_or(|total| total > card_source::MAX_DECLARED_PACKAGE_BYTES)
    {
        return Err("패키지 압축 풀림 크기가 미리보기 한도를 넘습니다.".to_string());
    }
    let decoded = UniversalFormatConverter::decode_rich(document, Some(extension))
        .map_err(|err| format!("문서를 읽지 못했습니다: {err}"))?;
    if let DecodedContent::Document(ref rich) = decoded.content {
        if let Ok(hwpx) = UniversalFormatConverter::encode_rich(rich, DocumentFormat::Hwpx, None) {
            if let Ok(png) = render_hwpx_first_page_png(&hwpx, fonts) {
                return Ok(png);
            }
        }
    }
    let svg = card::svg(&card_source::content_from(decoded));
    render_png(&svg, fonts)
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

/// `document` with its first page drawn from its own content when the caller supplied no page SVG
/// (PD-239). Verified with the same check as every saved document before returning.
pub fn with_drawn_first_page(
    extension: &str,
    document: &[u8],
    fonts: &Arc<Database>,
) -> Result<Vec<u8>, String> {
    let format = DocumentFormat::from_extension(extension)
        .filter(|format| easyoffice_formats::supports_thumbnail(*format))
        .ok_or_else(|| format!(".{extension}에는 미리보기 그림 자리가 없습니다."))?;
    if document.len() > MAX_EMBED_DOCUMENT_BYTES {
        return Err("문서가 커서 미리보기 그림을 넣지 않습니다.".to_string());
    }
    let png = draw_document_png(format, extension, document, fonts)?;
    let embedded = embed_thumbnail(format, document, &png).map_err(|err| err.to_string())?;
    verify_document_bytes(extension, &embedded).map_err(|err| err.to_string())?;
    Ok(embedded)
}

#[cfg(test)]
#[path = "save_thumbnail_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "save_thumbnail_raster_tests.rs"]
mod raster_tests;
