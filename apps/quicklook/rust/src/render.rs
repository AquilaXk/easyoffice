//! Parsing, page SVG, PNG and PDF output.
//!
//! The pages come from rhwp's own SVG renderer (the same engine and profile the editor and the
//! saved-file preview picture use, PD-132); `usvg` and `resvg` draw them, `svg2pdf` writes the PDF.
//! Every size is bounded before it is used: the file, each page SVG, the pixel size, the page count
//! and the PDF.

use crate::fonts;
use crate::raster::{self, RasterBudget};
use crate::status::{
    STATUS_EMPTY_DOCUMENT, STATUS_INVALID_INPUT, STATUS_PARSE_FAILED, STATUS_RENDER_FAILED,
    STATUS_TOO_LARGE,
};
use rhwp::DocumentCore;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Largest file read (Finder may pass anything).
pub const MAX_INPUT_BYTES: usize = 50 * 1024 * 1024;
/// Largest page SVG drawn; a page of text is tens of KB, one with pictures a few MB (PD-132).
pub const MAX_PAGE_SVG_BYTES: usize = 16 * 1024 * 1024;
/// Longest side of a PNG, in pixels. 2048 x 2048 x 4 bytes = 16 MiB of pixels at most.
pub const MAX_PNG_SIDE: u32 = 2048;
/// Pages put into the preview PDF; the rest of a long document is left out.
pub const MAX_PREVIEW_PAGES: u32 = 64;
/// Largest preview PDF (the page objects; the page tree adds a few KB). A page of a form can take 1 MB.
pub const MAX_PDF_BYTES: usize = 32 * 1024 * 1024;
/// Pages are added until this much time has passed; the pages done so far are the preview.
pub const PDF_PAGE_BUDGET: Duration = Duration::from_secs(15);
/// Deflate level of the page streams: a preview is read once, so speed wins over size (level 1 is
/// about 20% faster and 20% larger than the default 6, `docs/performance.md`).
const PDF_DEFLATE_LEVEL: u8 = 1;
/// CSS pixels (96 per inch) to PDF points (72 per inch).
const PX_TO_PT: f32 = 72.0 / 96.0;

/// A parsed page: the tree plus its size in CSS pixels.
pub struct Page {
    pub tree: usvg::Tree,
    pub width: f32,
    pub height: f32,
}

pub fn check_input(data: &[u8]) -> Result<(), i32> {
    if data.is_empty() {
        return Err(STATUS_INVALID_INPUT);
    }
    if data.len() > MAX_INPUT_BYTES {
        return Err(STATUS_TOO_LARGE);
    }
    Ok(())
}

/// Parses the document and returns it with its page count (at least one page).
pub fn open(data: &[u8]) -> Result<(DocumentCore, u32), i32> {
    check_input(data)?;
    let core = DocumentCore::from_bytes(data).map_err(|_| STATUS_PARSE_FAILED)?;
    match core.page_count() {
        0 => Err(STATUS_EMPTY_DOCUMENT),
        count => Ok((core, count)),
    }
}

/// Page `index` as a parsed SVG tree. The SVG is parsed without any file access: path and `file:`
/// hrefs are ignored, only data URLs are read, and a picture over the pixel limits is left out.
pub fn page(core: &DocumentCore, index: u32) -> Result<Page, i32> {
    let svg = core
        .render_page_svg_native(index)
        .map_err(|_| STATUS_RENDER_FAILED)?;
    parse_svg(&svg)
}

pub fn parse_svg(svg: &str) -> Result<Page, i32> {
    if svg.len() > MAX_PAGE_SVG_BYTES {
        return Err(STATUS_TOO_LARGE);
    }
    // usvg expands entity declarations without a bound, and no page the editor draws has one.
    easyoffice_formats::XmlSanitizer::validate_safe_xml(svg).map_err(|_| STATUS_RENDER_FAILED)?;
    crate::raster::svg_budget::check_svg_structure(svg).map_err(|_| STATUS_RENDER_FAILED)?;
    let svg = fonts::fallbacks::add_font_fallbacks(svg);
    let mut options = usvg::Options {
        fontdb: fonts::shared(),
        ..usvg::Options::default()
    };
    options.image_href_resolver = raster::bounded_image_resolver(RasterBudget::for_page());
    let tree = usvg::Tree::from_str(&svg, &options).map_err(|_| STATUS_RENDER_FAILED)?;
    let (width, height) = (tree.size().width(), tree.size().height());
    if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
        return Err(STATUS_RENDER_FAILED);
    }
    Ok(Page {
        tree,
        width,
        height,
    })
}

/// The page on white, its longer side `max_side` pixels (clamped to 1..=[`MAX_PNG_SIDE`]).
pub fn png(page: &Page, max_side: u32) -> Result<Vec<u8>, i32> {
    let side = max_side.clamp(1, MAX_PNG_SIDE);
    let scale = side as f32 / page.width.max(page.height);
    let pixels = |extent: f32| ((extent * scale).round() as u32).clamp(1, side);
    let mut pixmap = tiny_skia::Pixmap::new(pixels(page.width), pixels(page.height))
        .ok_or(STATUS_RENDER_FAILED)?;
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(
        &page.tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().map_err(|_| STATUS_RENDER_FAILED)
}

/// One PDF page per document page; text is embedded as subset fonts (outlines on a page whose font
/// cannot be subset). It stops at [`MAX_PREVIEW_PAGES`],
/// when [`MAX_PDF_BYTES`] would be passed or when [`PDF_PAGE_BUDGET`] has run out, and keeps the
/// pages before (a first page that alone is too large is `STATUS_TOO_LARGE`).
/// Ported from the reference app's `apps/desktop/quicklook/rust/src/pdf.rs` (MIT, see
/// THIRD_PARTY_NOTICES.md): chunks of the pages are renumbered into one object space. The pages go
/// in as SVG trees directly, where the reference app rasterized them first.
pub fn pdf(core: &DocumentCore, page_count: u32) -> Result<(Vec<u8>, Page), i32> {
    pdf_of(page_count, |index| page(core, index))
}

/// Converts a page with its text embedded; when the font subsetter fails on it (a font it cannot
/// cut), the page is converted again with its text as outlines: right on screen, not searchable
/// (the same retry as the shell's PDF export, PD-114).
fn to_chunk_with_text(
    tree: &usvg::Tree,
) -> Result<(pdf_writer::Chunk, pdf_writer::Ref), svg2pdf::ConversionError> {
    let options = |embed_text| svg2pdf::ConversionOptions {
        embed_text,
        compression_level: PDF_DEFLATE_LEVEL,
        ..svg2pdf::ConversionOptions::default()
    };
    match svg2pdf::to_chunk(tree, options(true)) {
        Err(svg2pdf::ConversionError::SubsetError(_)) => svg2pdf::to_chunk(tree, options(false)),
        result => result,
    }
}

/// [`pdf`] over any source of pages. A page after the first that cannot be drawn or converted ends
/// the PDF there, like the other limits; the first page failing fails the call.
pub(crate) fn pdf_of(
    page_count: u32,
    mut next_page: impl FnMut(u32) -> Result<Page, i32>,
) -> Result<(Vec<u8>, Page), i32> {
    use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, TextStr};
    let count = page_count.min(MAX_PREVIEW_PAGES);
    let mut alloc = Ref::new(1);
    let catalog_ref = alloc.bump();
    let tree_ref = alloc.bump();
    let mut first = None;
    let mut pages = Vec::with_capacity(count as usize);
    let mut renumbered_to = HashMap::new();
    let mut total = 0_usize;
    let started = Instant::now();
    for index in 0..count {
        if index > 0 && started.elapsed() > PDF_PAGE_BUDGET {
            break;
        }
        let converted = next_page(index).and_then(|parsed| {
            let chunk = to_chunk_with_text(&parsed.tree).map_err(|_| STATUS_RENDER_FAILED);
            chunk.map(|(chunk, xobject)| (parsed, chunk, xobject))
        });
        let (parsed, chunk, xobject) = match converted {
            Ok(done) => done,
            Err(_) if index > 0 => break,
            Err(code) => return Err(code),
        };
        renumbered_to.clear();
        renumbered_to.reserve(chunk.refs().len());
        let chunk = chunk.renumber(|old| *renumbered_to.entry(old).or_insert_with(|| alloc.bump()));
        let xobject = renumbered_to.get(&xobject).copied().unwrap_or(xobject);
        total = total.saturating_add(chunk.len());
        if total > MAX_PDF_BYTES {
            if pages.is_empty() {
                return Err(STATUS_TOO_LARGE);
            }
            break;
        }
        pages.push((
            chunk,
            xobject,
            alloc.bump(),
            alloc.bump(),
            parsed.width * PX_TO_PT,
            parsed.height * PX_TO_PT,
        ));
        if first.is_none() {
            first = Some(parsed);
        }
    }
    let first = first.ok_or(STATUS_EMPTY_DOCUMENT)?;

    let mut out = Pdf::new();
    out.catalog(catalog_ref).pages(tree_ref);
    out.pages(tree_ref)
        .kids(pages.iter().map(|page| page.2))
        .count(pages.len() as i32);
    let name = Name(b"S1");
    for (_, xobject, page_ref, content_ref, width, height) in &pages {
        let mut page = out.page(*page_ref);
        page.media_box(Rect::new(0.0, 0.0, *width, *height));
        page.parent(tree_ref);
        page.contents(*content_ref);
        page.resources().x_objects().pair(name, *xobject);
        page.finish();
        let mut content = Content::new();
        content.transform([*width, 0.0, 0.0, *height, 0.0, 0.0]);
        content.x_object(name);
        out.stream(*content_ref, &content.finish());
    }
    for (chunk, ..) in &pages {
        out.extend(chunk);
    }
    out.document_info(alloc.bump())
        .producer(TextStr("easyoffice-quicklook"));
    Ok((out.finish(), first))
}
