//! Searchable PDF export of the Hangul editor's page SVGs (PD-101, PD-114).
//!
//! The editor renders each page to SVG; this module turns those pages into one PDF whose text is
//! real text (embedded font subsets with a `ToUnicode` map), so the result can be searched and
//! copied. Adapted from the page assembly of the rhwp PDF renderer (`src/renderer/pdf.rs`,
//! `svgs_to_pdf_with_links_and_to_chunk`, rhwp 0.8.7 at `1a76570`) and from the reference app's
//! `pdf_export.rs` (MIT, see THIRD_PARTY_NOTICES.md): one `svg2pdf` chunk per page, chunks
//! renumbered into one object space, a form XObject per page placed on a media box in points.
//!
//! Hot path: page SVGs are parsed straight from their bytes (no copy), the font database is built
//! once per process, a character's fallback face is looked up once per export (`pdf_fonts.rs`),
//! pages are converted on a few threads a batch at a time and each converted chunk is renumbered
//! straight into the output and dropped, so only one batch of chunks is alive at once. The pages
//! may themselves arrive a batch at a time ([`PdfAssembler`], PD-123), so the SVGs of a long
//! document are not alive together either. Every size is bounded with `checked_*` arithmetic
//! before any work starts.
//!
//! Text that asks for bold or italic and gets a regular upright face is synthesised before the page
//! is converted (`pdf_synthetic.rs`), and the finished file's text maps name the standard
//! character for private-use codes of Hangul documents (`pdf_tounicode.rs`), PD-131.

use crate::raster_budget::RasterBudget;
use pdf_writer::{Chunk, Content, Finish, Name, Pdf, Rect, Ref, TextStr};
use std::cell::Cell;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use usvg::fontdb::Database;

pub use crate::pdf_fonts::shared_font_db;

/// Most pages one export may contain.
pub const MAX_PAGES: usize = 2_000;
/// Largest single page SVG (bytes).
pub const MAX_PAGE_SVG_BYTES: usize = 64 * 1_048_576;
/// Largest sum of all page SVGs (bytes).
pub const MAX_TOTAL_SVG_BYTES: usize = 512 * 1_048_576;
/// Pages converted before their chunks are written out; it bounds the converted data kept alive.
const BATCH_PAGES: usize = 16;
/// Threads that convert pages at once. A dense page needs on the order of 100 MB to convert, so the
/// count stays small whatever the machine offers.
const MAX_CONVERT_THREADS: usize = 4;
/// Deepest `<image>` inside `<image>` of SVG data URLs that is kept; deeper pictures are dropped.
/// WMF/EMF pictures are one level (an SVG that holds a bitmap); the bound keeps a hostile chain of
/// nested data URLs from recursing without end.
const MAX_NESTED_SVG_DEPTH: u32 = 4;
/// SVG user units are CSS pixels (96 per inch); PDF user units are points (72 per inch).
const PX_TO_PT: f32 = 72.0 / 96.0;
/// Name of the page's form XObject.
const PAGE_XOBJECT: Name<'static> = Name(b"S1");
const PRODUCER: &str = "EasyOffice";

/// What the pages of one export have used so far, checked before each batch is parsed. The sums
/// are the limits of the whole export, whether its pages come in one slice or in batches.
#[derive(Debug, Default)]
struct Budget {
    pages: usize,
    bytes: usize,
}

impl Budget {
    /// Admits a batch, or says which limit it would pass. Nothing is parsed before this succeeds.
    fn admit(&mut self, batch: &[String]) -> Result<(), String> {
        if batch.is_empty() {
            return Err("내보낼 페이지가 없습니다.".to_string());
        }
        if self
            .pages
            .checked_add(batch.len())
            .is_none_or(|sum| sum > MAX_PAGES)
        {
            return Err(format!(
                "PDF로 내보낼 수 있는 쪽 수({MAX_PAGES})를 초과했습니다."
            ));
        }
        let mut bytes = self.bytes;
        for (offset, svg) in batch.iter().enumerate() {
            if svg.len() > MAX_PAGE_SVG_BYTES {
                return Err(format!(
                    "{}쪽 SVG가 허용 크기를 초과합니다.",
                    self.pages + offset + 1
                ));
            }
            bytes = bytes
                .checked_add(svg.len())
                .filter(|sum| *sum <= MAX_TOTAL_SVG_BYTES)
                .ok_or_else(|| "페이지 SVG 전체 크기가 허용 한도를 초과합니다.".to_string())?;
        }
        self.pages += batch.len();
        self.bytes = bytes;
        Ok(())
    }
}

thread_local! {
    /// How many nested SVG pictures this thread is parsing right now.
    static NESTED_SVG_DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Leaves one nesting level when dropped.
struct NestedLevel;

impl NestedLevel {
    /// Enters a level, or `None` when [`MAX_NESTED_SVG_DEPTH`] is reached.
    fn enter() -> Option<Self> {
        NESTED_SVG_DEPTH.with(|depth| {
            (depth.get() < MAX_NESTED_SVG_DEPTH).then(|| {
                depth.set(depth.get() + 1);
                Self
            })
        })
    }
}

impl Drop for NestedLevel {
    fn drop(&mut self) {
        NESTED_SVG_DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

/// Reads a `data:image/svg+xml` picture together with the images inside it. usvg's own resolver
/// drops every `<image>` of such a sub-SVG (the SVG rule), which turned WMF/EMF pictures that
/// hold a bitmap into blanks; the page and its sub-SVGs both come from the editor, so the
/// sub-SVG is parsed with the same options. Ported from rhwp `pdf_image_href_resolver` (rhwp
/// 0.8.7, `src/renderer/pdf.rs` lines 1188-1205; MIT), with the nesting bound added because that
/// version recurses as deep as the data does. Other data pictures go to usvg's default resolver;
/// path and `file:` hrefs are ignored, in the page and in nested SVGs alike.
fn image_href_resolver(budget: Arc<RasterBudget>) -> usvg::ImageHrefResolver<'static> {
    let default_data = usvg::ImageHrefResolver::default_data_resolver();
    usvg::ImageHrefResolver {
        resolve_data: Box::new(move |mime, data, options| match mime {
            "image/svg+xml" => {
                let _level = NestedLevel::enter()?;
                usvg::Tree::from_data(&data, options)
                    .ok()
                    .map(usvg::ImageKind::SVG)
            }
            // The decoders allocate by the size in the picture's header, so a page SVG (untrusted)
            // is kept from making a picture or a page cost more than a fixed number of pixels. A
            // refused picture is left out and the page is kept (PD-145).
            _ if !budget.admit(&data) => None,
            _ => default_data(mime, data, options),
        }),
        // A page SVG is untrusted text: a path or `file:` href must never make the export read a
        // file (usvg's default would). The editor embeds every picture as a data URL.
        resolve_string: Box::new(|_, _| None),
    }
}

/// Parse options of a single page with the picture limits of an exported page, as
/// [`ExportOptions::page`] makes them (for the tests that parse one page by itself).
#[cfg(test)]
pub(crate) fn parse_options(fonts: &Arc<Database>) -> usvg::Options<'static> {
    options_with(
        fonts,
        crate::pdf_font_select::memoized_stock_selector(),
        new_fallback_selector(),
        Arc::new(RasterBudget::for_export()),
    )
}

/// [`parse_options`] for a page that is drawn to a bitmap: its pictures are bounded by `budget`,
/// which counts this one page (a new budget for each page).
pub(crate) fn bitmap_parse_options(
    fonts: &Arc<Database>,
    budget: RasterBudget,
) -> usvg::Options<'static> {
    options_with(
        fonts,
        crate::pdf_font_select::memoized_stock_selector(),
        new_fallback_selector(),
        Arc::new(budget),
    )
}

fn new_fallback_selector() -> usvg::FallbackSelectionFn<'static> {
    crate::pdf_fonts::fallback_selector(
        crate::pdf_fonts::MAX_INDEXED_CHARS,
        crate::pdf_fonts::MAX_FACE_PROBES,
    )
}

fn options_with(
    fonts: &Arc<Database>,
    select_font: usvg::FontSelectionFn<'static>,
    select_fallback: usvg::FallbackSelectionFn<'static>,
    budget: Arc<RasterBudget>,
) -> usvg::Options<'static> {
    let mut options = usvg::Options {
        fontdb: Arc::clone(fonts),
        ..usvg::Options::default()
    };
    options.font_resolver.select_font = select_font;
    options.font_resolver.select_fallback = select_fallback;
    options.image_href_resolver = image_href_resolver(budget);
    options
}

/// The options of one export: the fonts, the font-request memo and the fallback selector (one
/// index per export) are shared by every page; each page gets a new picture budget, because the pages are converted
/// side by side and one page's photos must not use up another's.
struct ExportOptions {
    fonts: Arc<Database>,
    select_font: Arc<usvg::FontSelectionFn<'static>>,
    select_fallback: Arc<usvg::FallbackSelectionFn<'static>>,
}

impl ExportOptions {
    fn new(fonts: Arc<Database>) -> Self {
        Self {
            fonts,
            select_font: Arc::new(crate::pdf_font_select::memoized_stock_selector()),
            select_fallback: Arc::new(new_fallback_selector()),
        }
    }

    /// Options for one page and the budget they count its pictures in.
    fn page(&self) -> (usvg::Options<'static>, Arc<RasterBudget>) {
        let budget = Arc::new(RasterBudget::for_export());
        let shared_font = Arc::clone(&self.select_font);
        let select_font: usvg::FontSelectionFn<'static> =
            Box::new(move |font, db| shared_font.as_ref()(font, db));
        let shared = Arc::clone(&self.select_fallback);
        let select_fallback: usvg::FallbackSelectionFn<'static> =
            Box::new(move |c, used, db| shared.as_ref()(c, used, db));
        let options = options_with(
            &self.fonts,
            select_font,
            select_fallback,
            Arc::clone(&budget),
        );
        (options, budget)
    }
}

/// One converted page before assembly.
struct PageChunk {
    chunk: Chunk,
    xobject: Ref,
    width: f32,
    height: f32,
    /// Pictures left out of the page because they were over the pixel limits.
    skipped_pictures: u32,
}

/// Runs `to_chunk` with the page's text embedded; when the font subsetter fails on a page (a font
/// the subsetter cannot cut), the page is converted again with its text as outlines. The page is
/// then right on screen and in print but not searchable, and the export goes on. Any other error
/// or a second failure is returned. The reference app retries the same way, then skips the page;
/// this export fails instead, because a PDF with a page missing is worse than no PDF (PD-114).
fn convert_with_outline_retry<T>(
    index: usize,
    embed_text: bool,
    mut to_chunk: impl FnMut(bool) -> Result<T, svg2pdf::ConversionError>,
) -> Result<T, svg2pdf::ConversionError> {
    match to_chunk(embed_text) {
        Err(svg2pdf::ConversionError::SubsetError(_)) if embed_text => {
            eprintln!(
                "PDF export: font subsetting failed on page {}, its text is drawn as outlines",
                index + 1
            );
            to_chunk(false)
        }
        result => result,
    }
}

fn convert_page(
    index: usize,
    svg: &str,
    options: &ExportOptions,
    embed_text: bool,
) -> Result<PageChunk, String> {
    let svg = crate::pdf_font_fallbacks::add_font_fallbacks(svg);
    let (page_options, pictures) = options.page();
    let (tree, styled) = crate::pdf_synthetic::parse_page(&svg, &page_options)
        .map_err(|err| format!("{}쪽 SVG를 읽지 못했습니다: {err}", index + 1))?;
    if styled.unsupported > 0 {
        eprintln!(
            "PDF export: page {} has {} bold or italic text(s) that mix faces or paints and are not synthesised",
            index + 1,
            styled.unsupported
        );
    }
    let (chunk, xobject) = convert_with_outline_retry(index, embed_text, |embed_text| {
        let conversion = svg2pdf::ConversionOptions {
            embed_text,
            ..svg2pdf::ConversionOptions::default()
        };
        svg2pdf::to_chunk(&tree, conversion)
    })
    .map_err(|err| format!("{}쪽을 PDF로 바꾸지 못했습니다: {err:?}", index + 1))?;
    Ok(PageChunk {
        chunk,
        xobject,
        width: tree.size().width() * PX_TO_PT,
        height: tree.size().height() * PX_TO_PT,
        skipped_pictures: pictures.refused(),
    })
}

/// Converts the pages of one batch on up to [`MAX_CONVERT_THREADS`] threads and returns them in
/// page order. `first` is the page number of `batch[0]`. With several failures the lowest page
/// number is reported.
fn convert_batch(
    first: usize,
    batch: &[String],
    options: &ExportOptions,
    embed_text: bool,
) -> Result<Vec<PageChunk>, String> {
    let threads = std::thread::available_parallelism()
        .map_or(1, NonZeroUsize::get)
        .min(MAX_CONVERT_THREADS)
        .min(batch.len());
    if threads <= 1 {
        return batch
            .iter()
            .enumerate()
            .map(|(offset, svg)| convert_page(first + offset, svg, options, embed_text))
            .collect();
    }
    let next = AtomicUsize::new(0);
    let mut done: Vec<(usize, Result<PageChunk, String>)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut local = Vec::new();
                    loop {
                        let offset = next.fetch_add(1, Ordering::Relaxed);
                        let Some(svg) = batch.get(offset) else { break };
                        let page = convert_page(first + offset, svg, options, embed_text);
                        let failed = page.is_err();
                        local.push((offset, page));
                        if failed {
                            break;
                        }
                    }
                    local
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("a page conversion thread panicked"))
            .collect()
    });
    done.sort_unstable_by_key(|(offset, _)| *offset);
    done.into_iter().map(|(_, page)| page).collect()
}

/// A PDF built from page SVGs that arrive a batch at a time (PD-123): each batch is converted and
/// written into the output at once and then dropped, so the page SVGs of a long document are never
/// alive together. The limits of [`MAX_PAGES`], [`MAX_PAGE_SVG_BYTES`] and [`MAX_TOTAL_SVG_BYTES`]
/// are counted over all batches. Any error ends the export (the caller drops the assembler), so
/// a half-built PDF is never finished.
pub struct PdfAssembler {
    options: ExportOptions,
    embed_text: bool,
    budget: Budget,
    skipped_pictures: u32,
    alloc: Ref,
    catalog_ref: Ref,
    tree_ref: Ref,
    page_refs: Vec<Ref>,
    renumber_map: HashMap<Ref, Ref>,
    pdf: Pdf,
}

impl PdfAssembler {
    pub fn new(fonts: &Arc<Database>, embed_text: bool) -> Self {
        let mut alloc = Ref::new(1);
        let catalog_ref = alloc.bump();
        let tree_ref = alloc.bump();
        Self {
            options: ExportOptions::new(Arc::new(crate::pdf_fonts::with_mapped_faces(fonts))),
            embed_text,
            budget: Budget::default(),
            skipped_pictures: 0,
            alloc,
            catalog_ref,
            tree_ref,
            page_refs: Vec::new(),
            renumber_map: HashMap::new(),
            pdf: Pdf::new(),
        }
    }

    /// Pages written so far.
    pub fn page_count(&self) -> usize {
        self.page_refs.len()
    }

    /// Pictures left out of the pages so far because they were over the pixel limits (PD-145).
    pub fn skipped_pictures(&self) -> u32 {
        self.skipped_pictures
    }

    /// Converts the next pages, in order, [`BATCH_PAGES`] at a time.
    pub fn add_pages(&mut self, pages: &[String]) -> Result<(), String> {
        self.budget.admit(pages)?;
        for batch in pages.chunks(BATCH_PAGES) {
            let converted =
                convert_batch(self.page_refs.len(), batch, &self.options, self.embed_text)?;
            for page in converted {
                self.skipped_pictures = self.skipped_pictures.saturating_add(page.skipped_pictures);
                let page_ref = self.alloc.bump();
                self.page_refs.push(page_ref);
                place_page(
                    &mut self.pdf,
                    &mut self.alloc,
                    &mut self.renumber_map,
                    self.tree_ref,
                    page_ref,
                    page,
                );
            }
        }
        Ok(())
    }

    /// The finished PDF: the catalog, the page tree over every page written, the document info,
    /// and the text maps with readable private-use characters (same bytes, same offsets).
    pub fn finish(mut self) -> Result<Vec<u8>, String> {
        if self.page_refs.is_empty() {
            return Err("내보낼 페이지가 없습니다.".to_string());
        }
        self.pdf.catalog(self.catalog_ref).pages(self.tree_ref);
        let count = i32::try_from(self.page_refs.len()).unwrap_or(i32::MAX);
        self.pdf
            .pages(self.tree_ref)
            .count(count)
            .kids(self.page_refs.iter().copied());
        let info_ref = self.alloc.bump();
        self.pdf.document_info(info_ref).producer(TextStr(PRODUCER));
        let mut bytes = self.pdf.finish();
        crate::pdf_tounicode::remap_private_use(&mut bytes);
        Ok(bytes)
    }
}

/// Writes one page: its objects renumbered straight into the output (the chunk is dropped
/// afterwards), the page dictionary, and the content stream that places the form XObject.
fn place_page(
    pdf: &mut Pdf,
    alloc: &mut Ref,
    renumber_map: &mut HashMap<Ref, Ref>,
    tree_ref: Ref,
    page_ref: Ref,
    page: PageChunk,
) {
    renumber_map.clear();
    renumber_map.reserve(page.chunk.refs().len());
    page.chunk.renumber_into(pdf, |old| {
        *renumber_map.entry(old).or_insert_with(|| alloc.bump())
    });
    let xobject = renumber_map
        .get(&page.xobject)
        .copied()
        .unwrap_or(page.xobject);
    let content_ref = alloc.bump();
    let mut writer = pdf.page(page_ref);
    writer.media_box(Rect::new(0.0, 0.0, page.width, page.height));
    writer.parent(tree_ref);
    writer.contents(content_ref);
    writer.resources().x_objects().pair(PAGE_XOBJECT, xobject);
    writer.finish();
    let mut content = Content::new();
    content.transform([page.width, 0.0, 0.0, page.height, 0.0, 0.0]);
    content.x_object(PAGE_XOBJECT);
    pdf.stream(content_ref, &content.finish());
}

/// Largest PDF [`verify_exported_pdf`] scans (bytes).
const MAX_VERIFIED_PDF_BYTES: usize = 256 * 1_048_576;
/// Dictionary keys the check looks for. The writer emits them as plain text (no object streams).
const KEY_COUNT: &[u8] = b"/Count ";

/// What [`verify_exported_pdf`] found.
#[derive(Debug, PartialEq, Eq)]
pub struct PdfCheck {
    pub pages: usize,
    /// Set when the PDF is valid but its text cannot be searched (see [`verify_exported_pdf`]).
    pub warning: Option<String>,
}

/// Digits read after `/Count `: more than this cannot be a page count.
const MAX_COUNT_DIGITS: usize = 7;

fn parse_count_at(bytes: &[u8]) -> Option<usize> {
    let mut value = 0usize;
    let mut len = 0usize;
    for &b in bytes.iter().take(MAX_COUNT_DIGITS) {
        if !b.is_ascii_digit() {
            break;
        }
        value = value * 10 + usize::from(b - b'0');
        len += 1;
    }
    (len > 0).then_some(value)
}

#[cfg(test)]
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
fn declared_counts(pdf: &[u8]) -> Vec<usize> {
    pdf.windows(KEY_COUNT.len())
        .enumerate()
        .filter(|(_, window)| *window == KEY_COUNT)
        .filter_map(|(at, _)| parse_count_at(&pdf[at + KEY_COUNT.len()..]))
        .collect()
}

struct ScannedKeys {
    count_matched: bool,
    media_boxes: usize,
    has_font: bool,
    has_map: bool,
}

fn scan_pdf_keys(pdf: &[u8], expected_pages: usize) -> ScannedKeys {
    const WORD_COUNT: u32 = u32::from_le_bytes(*b"/Cou");
    const WORD_MEDIA: u32 = u32::from_le_bytes(*b"/Med");
    const WORD_FONT: u32 = u32::from_le_bytes(*b"/Fon");
    const WORD_TO_UNI: u32 = u32::from_le_bytes(*b"/ToU");
    const SLASH_MASK: u64 = 0x2F2F_2F2F_2F2F_2F2F;
    const LO_BITS: u64 = 0x0101_0101_0101_0101;
    const HI_BITS: u64 = 0x8080_8080_8080_8080;

    let mut out = ScannedKeys {
        count_matched: false,
        media_boxes: 0,
        has_font: false,
        has_map: false,
    };
    let len = pdf.len();
    let mut i = 0;
    while i + 4 <= len {
        if i + 8 <= len {
            let chunk = u64::from_le_bytes([
                pdf[i],
                pdf[i + 1],
                pdf[i + 2],
                pdf[i + 3],
                pdf[i + 4],
                pdf[i + 5],
                pdf[i + 6],
                pdf[i + 7],
            ]);
            let x = chunk ^ SLASH_MASK;
            let hits = x.wrapping_sub(LO_BITS) & !x & HI_BITS;
            if hits == 0 {
                i += 8;
                continue;
            }
            i += (hits.trailing_zeros() >> 3) as usize;
            if i + 4 > len {
                break;
            }
        }
        if pdf[i] == b'/' {
            let word = u32::from_le_bytes([pdf[i], pdf[i + 1], pdf[i + 2], pdf[i + 3]]);
            match word {
                WORD_COUNT if pdf.get(i + 4..i + 7) == Some(b"nt ") => {
                    if parse_count_at(&pdf[i + KEY_COUNT.len()..]) == Some(expected_pages) {
                        out.count_matched = true;
                    }
                }
                WORD_MEDIA if pdf.get(i + 4..i + 9) == Some(b"iaBox") => {
                    out.media_boxes += 1;
                }
                WORD_FONT if matches!(pdf.get(i + 4..i + 10), Some(b"tFile2" | b"tFile3")) => {
                    out.has_font = true;
                }
                WORD_TO_UNI if pdf.get(i + 4..i + 10) == Some(b"nicode") => {
                    out.has_map = true;
                }
                _ => {}
            }
        }
        i += 1;
    }
    out
}

/// Checks the bytes of an exported PDF against what the export promises: the `%PDF-` header, a page
/// tree of `expected_pages` pages with one `/MediaBox` per page, and, when a font is embedded, a
/// `/ToUnicode` map (a font without one would give text that cannot be copied or searched).
///
/// `has_text` says that the page SVGs held `<text`. When they did but no font was embedded (a
/// machine without a usable font draws the glyphs as outlines) the PDF is still correct, only not
/// searchable, so that is a warning in the result and not an error.
pub fn verify_exported_pdf(
    pdf: &[u8],
    expected_pages: usize,
    has_text: bool,
) -> Result<PdfCheck, String> {
    if pdf.len() > MAX_VERIFIED_PDF_BYTES {
        return Err("PDF가 검사할 수 있는 크기를 넘습니다.".to_string());
    }
    if !pdf.starts_with(b"%PDF-") {
        return Err("PDF 머리글(%PDF-)이 없습니다.".to_string());
    }
    let scanned = scan_pdf_keys(pdf, expected_pages);
    if !scanned.count_matched {
        return Err(format!(
            "PDF 쪽 수(/Count)가 {expected_pages}쪽이 아닙니다."
        ));
    }
    if scanned.media_boxes != expected_pages {
        return Err(format!(
            "PDF에 쪽 크기(/MediaBox)가 {}개 있습니다. {expected_pages}개여야 합니다.",
            scanned.media_boxes
        ));
    }
    if scanned.has_font && !scanned.has_map {
        return Err("PDF에 글꼴은 있으나 글자 대응표(/ToUnicode)가 없습니다.".to_string());
    }
    let warning = (has_text && !scanned.has_font).then(|| {
        "글꼴이 포함되지 않아 글자가 윤곽선으로 나갔습니다(PDF는 올바르지만 글자 검색 불가)."
            .to_string()
    });
    Ok(PdfCheck {
        pages: expected_pages,
        warning,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::path::Path;

    /// All pages in one slice: the whole export through [`PdfAssembler`]. Fails closed on any page
    /// that cannot be converted, since a partial document would silently lose pages.
    fn svgs_to_searchable_pdf(pages: &[String], fonts: &Arc<Database>) -> Result<Vec<u8>, String> {
        svgs_to_pdf(pages, fonts, true)
    }

    /// `embed_text = false` turns glyphs into outlines (smaller work, but the text can no longer be
    /// searched); it exists for the before/after measurement below.
    fn svgs_to_pdf(
        pages: &[String],
        fonts: &Arc<Database>,
        embed_text: bool,
    ) -> Result<Vec<u8>, String> {
        let mut assembler = PdfAssembler::new(fonts, embed_text);
        assembler.add_pages(pages)?;
        assembler.finish()
    }

    const SAMPLE_TEXT: &str = "Searchable PDF 12345";

    fn page_svg(text: &str) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="320" height="120"><rect width="320" height="120" fill="white"/><text x="16" y="64" font-family="DejaVu Sans" font-size="24">{text}</text></svg>"#
        )
    }

    pub(super) fn system_font_db() -> Option<Arc<Database>> {
        static DB: std::sync::OnceLock<Option<Arc<Database>>> = std::sync::OnceLock::new();
        DB.get_or_init(|| {
            let mut db = Database::new();
            db.load_system_fonts();
            if db.is_empty() {
                for dir in [
                    "/usr/share/fonts",
                    "/System/Library/Fonts",
                    "C:/Windows/Fonts",
                ] {
                    if Path::new(dir).is_dir() {
                        db.load_fonts_dir(dir);
                    }
                }
            }
            (!db.is_empty()).then(|| Arc::new(crate::pdf_fonts::with_mapped_faces(&db)))
        })
        .clone()
    }

    fn admit(pages: &[String]) -> Result<(), String> {
        Budget::default().admit(pages)
    }

    #[test]
    fn limits_reject_empty_oversized_and_overflowing_inputs() {
        assert!(admit(&[]).is_err());
        let many = vec![String::new(); MAX_PAGES + 1];
        assert!(admit(&many).unwrap_err().contains("쪽 수"));
        let big = vec!["x".repeat(MAX_PAGE_SVG_BYTES + 1)];
        assert!(admit(&big).unwrap_err().contains("1쪽"));
        let page = "x".repeat(MAX_PAGE_SVG_BYTES);
        let too_many_bytes = vec![page; MAX_TOTAL_SVG_BYTES / MAX_PAGE_SVG_BYTES + 1];
        assert!(admit(&too_many_bytes).unwrap_err().contains("전체 크기"));
        assert!(admit(&[page_svg("ok")]).is_ok());
    }

    /// The limits belong to the export, not to a batch: pages and bytes add up over the batches,
    /// a refused batch changes nothing, and a page number in a message is the document's.
    #[test]
    fn limits_add_up_over_batches_and_a_refused_batch_changes_nothing() {
        let mut budget = Budget::default();
        let ten = vec![String::new(); 10];
        for _ in 0..MAX_PAGES / 10 {
            budget.admit(&ten).expect("within the page limit");
        }
        assert_eq!(budget.pages, MAX_PAGES);
        let err = budget.admit(&ten[..1]).unwrap_err();
        assert!(err.contains("쪽 수") && err.contains("2000"), "{err}");
        assert_eq!(budget.pages, MAX_PAGES, "a refused batch is not counted");

        let mut budget = Budget::default();
        let big = vec!["x".repeat(MAX_PAGE_SVG_BYTES)];
        for _ in 0..MAX_TOTAL_SVG_BYTES / MAX_PAGE_SVG_BYTES {
            budget.admit(&big).expect("within the byte limit");
        }
        let err = budget.admit(&[String::from("x")]).unwrap_err();
        assert!(err.contains("전체 크기"), "{err}");
        assert_eq!(budget.bytes, MAX_TOTAL_SVG_BYTES);

        let mut budget = Budget::default();
        budget.admit(&ten).expect("first batch");
        let mut second = vec![String::new(); 2];
        second.push("x".repeat(MAX_PAGE_SVG_BYTES + 1));
        let err = budget.admit(&second).unwrap_err();
        assert!(err.starts_with("13쪽"), "page 10 + 3: {err}");
    }

    #[test]
    fn a_broken_page_fails_the_whole_export_with_its_page_number() {
        let Some(db) = system_font_db() else { return };
        let pages = vec![page_svg("one"), "not svg".to_string()];
        let err = svgs_to_searchable_pdf(&pages, &db).unwrap_err();
        assert!(err.starts_with("2쪽"), "{err}");
    }

    #[test]
    fn export_embeds_real_text_with_a_unicode_map_and_one_media_box_per_page() {
        let Some(db) = system_font_db() else { return };
        let pages = vec![page_svg(SAMPLE_TEXT), page_svg("Second page")];
        let pdf = svgs_to_searchable_pdf(&pages, &db).expect("pdf");
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(contains(&pdf, b"/ToUnicode"), "text keeps a Unicode map");
        assert!(
            contains(&pdf, b"/FontFile2") || contains(&pdf, b"/FontFile3"),
            "font subsets are embedded"
        );
        assert!(contains(&pdf, b"/Count 2"));
        assert_eq!(
            pdf.windows(b"/MediaBox".len())
                .filter(|w| *w == b"/MediaBox")
                .count(),
            2
        );
        // 320 x 120 px at 72/96 pt per px.
        assert!(contains(&pdf, b"[0 0 240 90]"));
        assert!(contains(&pdf, b"EasyOffice"));
    }

    /// Measurement against the outline-only conversion (the same pages with the text turned into
    /// shapes, i.e. what a picture-like PDF gives): the text stays searchable at a cost in time and
    /// size that this test prints for the PR record (`cargo test -- --nocapture`).
    #[test]
    fn embedding_text_keeps_it_searchable_and_the_cost_is_measured() {
        let Some(db) = system_font_db() else { return };
        const PAGES: usize = 20;
        let pages: Vec<String> = (0..PAGES)
            .map(|i| page_svg(&format!("{SAMPLE_TEXT} page {i}")))
            .collect();
        let run = |embed: bool| {
            let started = std::time::Instant::now();
            let pdf = svgs_to_pdf(&pages, &db, embed).expect("pdf");
            (started.elapsed(), pdf)
        };
        let (searchable_time, searchable) = run(true);
        let (outline_time, outlines) = run(false);
        eprintln!(
            "PDF export {PAGES} pages: searchable {:?} / {} bytes, outlines {:?} / {} bytes",
            searchable_time,
            searchable.len(),
            outline_time,
            outlines.len()
        );
        assert!(contains(&searchable, b"/ToUnicode"));
        assert!(
            !contains(&outlines, b"/ToUnicode"),
            "outlined text has no text map"
        );
        assert!(!contains(&outlines, b"/FontFile2") && !contains(&outlines, b"/FontFile3"));
    }

    /// Synthetic page of a Hangul report: 46 lines of mixed Korean/Latin text in document faces that
    /// are not installed (so the fallback chain runs) plus a table border.
    pub(crate) fn report_page_svg(page: usize) -> String {
        let mut svg = String::from(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="794" height="1123"><rect width="794" height="1123" fill="white"/>"#,
        );
        for line in 0..46 {
            let y = 80 + line * 21;
            let family = ["함초롬바탕", "맑은 고딕", "휴먼명조", "굴림"][line % 4];
            svg.push_str(&format!(
                r#"<text x="72" y="{y}" font-family="{family}" font-size="14">제{page}쪽 {line}째 줄: 검색 가능한 PDF를 만드는 한글 문서 내보내기 시험 문장입니다 ①②③ ABC abc 12345</text><line x1="72" y1="{}" x2="722" y2="{}" stroke="black" stroke-width="0.5"/>"#,
                y + 4,
                y + 4
            ));
        }
        svg.push_str("</svg>");
        svg
    }

    /// Measurement for the PR record: `cargo test --release -p easyoffice-desktop
    /// hundreds_of_pages -- --ignored --nocapture` under `/usr/bin/time -l` (peak RSS).
    #[test]
    #[ignore = "measurement, run by hand"]
    fn hundreds_of_pages_are_measured() {
        let Some(db) = system_font_db() else { return };
        let pages: Vec<String> = (0..300).map(report_page_svg).collect();
        let started = std::time::Instant::now();
        let pdf = svgs_to_searchable_pdf(&pages, &db).expect("pdf");
        eprintln!(
            "MEASURE pages=300 svg_bytes={} pdf_bytes={} elapsed={:?}",
            pages.iter().map(String::len).sum::<usize>(),
            pdf.len(),
            started.elapsed()
        );
        assert!(verify_exported_pdf(&pdf, 300, true).is_ok());
    }

    /// The same 300 pages as above, but the page SVGs are made ten at a time and dropped after
    /// each batch, the way the shell now receives them: `cargo test --release -p
    /// easyoffice-desktop hundreds_of_streamed_pages -- --ignored --nocapture` under
    /// `/usr/bin/time -l` (peak RSS).
    #[test]
    #[ignore = "measurement, run by hand"]
    fn hundreds_of_streamed_pages_are_measured() {
        let Some(db) = system_font_db() else { return };
        let started = std::time::Instant::now();
        let mut assembler = PdfAssembler::new(&db, true);
        let mut svg_bytes = 0;
        for first in (0..300).step_by(10) {
            let batch: Vec<String> = (first..first + 10).map(report_page_svg).collect();
            svg_bytes += batch.iter().map(String::len).sum::<usize>();
            assembler.add_pages(&batch).expect("batch");
        }
        let pdf = assembler.finish().expect("pdf");
        eprintln!(
            "MEASURE streamed pages=300 svg_bytes={svg_bytes} pdf_bytes={} elapsed={:?}",
            pdf.len(),
            started.elapsed()
        );
        assert!(verify_exported_pdf(&pdf, 300, true).is_ok());
    }

    fn bundled_only_db() -> Option<Arc<Database>> {
        crate::pdf_fonts::tests::bundled_only_db().map(Arc::new)
    }

    const KOREAN_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="320" height="120"><text x="16" y="64" font-family="함초롬바탕, serif" font-size="24">한글 검색</text></svg>"#;

    /// The environment PD-114 is about: no Korean system font at all, only the bundled face. The
    /// document face is not installed, the generic family maps to the bundled face, and the text
    /// comes out as an embedded font with a text map that names the syllables (the map is plain
    /// text in the file; `<D55C>` is U+D55C, `<AE00>` is U+AE00).
    #[test]
    fn korean_text_is_searchable_with_only_the_bundled_font() {
        let Some(db) = bundled_only_db() else { return };
        let pdf = svgs_to_searchable_pdf(&[KOREAN_SVG.to_string()], &db).expect("pdf");
        assert_eq!(
            verify_exported_pdf(&pdf, 1, true),
            Ok(PdfCheck {
                pages: 1,
                warning: None
            })
        );
        assert!(contains(&pdf, b"NotoSansKR"));
        assert!(contains(&pdf, b"<D55C>") && contains(&pdf, b"<AE00>"));
    }

    /// Without any usable font the text is dropped from the page; the check says so (a warning,
    /// not an error: the PDF itself is valid). This is what the bundled font prevents.
    #[test]
    fn without_any_font_the_text_is_lost_and_the_check_warns() {
        let pdf = svgs_to_searchable_pdf(&[KOREAN_SVG.to_string()], &Arc::new(Database::new()))
            .expect("pdf");
        let check = verify_exported_pdf(&pdf, 1, true).expect("valid pdf");
        assert!(check.warning.is_some_and(|w| w.contains("글꼴")));
    }

    fn blank_page(width: usize) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="40"><rect width="{width}" height="40" fill="gray"/></svg>"#
        )
    }

    /// Page widths in file order, read from the `/MediaBox [0 0 W H]` entries.
    fn media_box_widths(pdf: &[u8]) -> Vec<f32> {
        let key = b"/MediaBox [0 0 ";
        pdf.windows(key.len())
            .enumerate()
            .filter(|(_, w)| *w == key)
            .map(|(at, _)| {
                let rest = &pdf[at + key.len()..];
                let end = rest.iter().position(|b| *b == b' ').expect("width");
                std::str::from_utf8(&rest[..end])
                    .expect("ascii")
                    .parse()
                    .expect("number")
            })
            .collect()
    }

    #[test]
    fn pages_stay_in_order_across_batches_and_threads() {
        const PAGES: usize = BATCH_PAGES * 2 + 5;
        let pages: Vec<String> = (0..PAGES).map(|i| blank_page(96 + 4 * i)).collect();
        let pdf = svgs_to_searchable_pdf(&pages, &Arc::new(Database::new())).expect("pdf");
        let widths = media_box_widths(&pdf);
        let expected: Vec<f32> = (0..PAGES).map(|i| (96 + 4 * i) as f32 * 0.75).collect();
        assert_eq!(widths, expected);
        assert!(verify_exported_pdf(&pdf, PAGES, false).is_ok());
    }

    #[test]
    fn the_lowest_failing_page_of_a_later_batch_is_reported() {
        let mut pages: Vec<String> = (0..BATCH_PAGES * 2 + 4).map(|_| blank_page(100)).collect();
        pages[BATCH_PAGES + 3] = "not svg".to_string();
        pages[BATCH_PAGES + 9] = "also not svg".to_string();
        let err = svgs_to_searchable_pdf(&pages, &Arc::new(Database::new())).unwrap_err();
        assert!(err.starts_with(&format!("{}쪽", BATCH_PAGES + 4)), "{err}");
    }

    #[test]
    fn a_subset_failure_retries_the_page_with_outlined_text_once() {
        let subset = || svg2pdf::ConversionError::SubsetError(usvg::fontdb::ID::dummy());
        let mut calls = Vec::new();
        let result = convert_with_outline_retry(0, true, |embed| {
            calls.push(embed);
            if embed {
                Err(subset())
            } else {
                Ok(7)
            }
        });
        assert_eq!((result.ok(), calls), (Some(7), vec![true, false]));

        let mut calls = Vec::new();
        let result: Result<i32, _> = convert_with_outline_retry(0, true, |embed| {
            calls.push(embed);
            Err(subset())
        });
        assert!(
            result.is_err(),
            "a second failure is an error, not a skipped page"
        );
        assert_eq!(calls, vec![true, false]);

        let mut calls = Vec::new();
        let result: Result<i32, _> = convert_with_outline_retry(0, true, |embed| {
            calls.push(embed);
            Err(svg2pdf::ConversionError::InvalidImage)
        });
        assert!(result.is_err());
        assert_eq!(calls, vec![true], "only a subset failure is retried");
    }

    fn base64(bytes: &[u8]) -> String {
        const DIGITS: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let n = chunk
                .iter()
                .enumerate()
                .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(DIGITS[(n >> (18 - 6 * i) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    /// `depth` SVGs, each holding the next as a `data:image/svg+xml` picture; the innermost is a
    /// bitmap-free square.
    fn nested_svg(depth: usize) -> String {
        let mut svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8"/></svg>"#.to_string();
        for _ in 0..depth {
            svg = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="8" height="8"><image width="8" height="8" xlink:href="data:image/svg+xml;base64,{}"/></svg>"#,
                base64(svg.as_bytes())
            );
        }
        svg
    }

    fn picture_depth(group: &usvg::Group) -> u32 {
        group
            .children()
            .iter()
            .map(|node| match node {
                usvg::Node::Group(inner) => picture_depth(inner),
                usvg::Node::Image(image) => match image.kind() {
                    usvg::ImageKind::SVG(tree) => 1 + picture_depth(tree.root()),
                    _ => 0,
                },
                _ => 0,
            })
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn a_picture_inside_a_picture_is_kept_but_a_hostile_chain_is_cut() {
        let options = parse_options(&Arc::new(Database::new()));
        let parse =
            |depth| usvg::Tree::from_data(nested_svg(depth).as_bytes(), &options).expect("tree");
        assert_eq!(
            picture_depth(parse(2).root()),
            2,
            "an image inside a sub-SVG (a WMF holding a bitmap) is kept; usvg's own resolver drops it"
        );
        assert_eq!(
            picture_depth(parse(MAX_NESTED_SVG_DEPTH as usize).root()),
            MAX_NESTED_SVG_DEPTH
        );
        // Base64 grows a level by a third, so a chain this deep is already tens of kilobytes.
        let hostile = parse(MAX_NESTED_SVG_DEPTH as usize + 8);
        assert_eq!(picture_depth(hostile.root()), MAX_NESTED_SVG_DEPTH);
        // The counter is back to zero afterwards, so the next page starts clean.
        assert_eq!(picture_depth(parse(1).root()), 1);
        let pdf = svgs_to_searchable_pdf(
            &[nested_svg(MAX_NESTED_SVG_DEPTH as usize + 8)],
            &Arc::new(Database::new()),
        );
        assert!(pdf.is_ok());
    }

    fn image_count(group: &usvg::Group) -> usize {
        group
            .children()
            .iter()
            .map(|node| match node {
                usvg::Node::Group(inner) => image_count(inner),
                usvg::Node::Image(image) => match image.kind() {
                    usvg::ImageKind::SVG(tree) => 1 + image_count(tree.root()),
                    _ => 1,
                },
                _ => 0,
            })
            .sum()
    }

    /// A page must not make the export read files: path, relative and `file:` hrefs are ignored, at
    /// the top level and inside a nested SVG. The same SVG with usvg's own options does load the file,
    /// so the test would notice a resolver that lets paths through.
    #[test]
    fn path_and_file_hrefs_are_ignored_in_pages_and_nested_svgs() {
        const PNG: [u8; 67] = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, 0x49, 0x48, 0x44, 0x52,
            0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1F, 0x15, 0xC4, 0x89, 0, 0, 0, 0x0D, 0x49,
            0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0, 1, 0, 0, 5, 0, 1, 0x0D, 0x0A, 0x2D, 0xB4, 0, 0,
            0, 0, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        let dir = std::env::temp_dir().join(format!("easyoffice-href-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let file = dir.join("x.png");
        std::fs::write(&file, PNG).expect("png");
        let path = file.to_str().expect("utf-8 path").to_string();
        let page = |href: &str| {
            format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="8" height="8"><image width="8" height="8" xlink:href="{href}"/></svg>"#
            )
        };
        let nested = |href: &str| {
            format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="8" height="8"><image width="8" height="8" xlink:href="data:image/svg+xml;base64,{}"/></svg>"#,
                base64(page(href).as_bytes())
            )
        };
        let hrefs = [
            path.clone(),
            format!("file://{path}"),
            "/etc/passwd".to_string(),
            "../x.png".to_string(),
            "x.png".to_string(),
        ];
        let stock = usvg::Options {
            resources_dir: Some(dir.clone()),
            ..usvg::Options::default()
        };
        let ours = usvg::Options {
            resources_dir: Some(dir.clone()),
            ..parse_options(&Arc::new(Database::new()))
        };
        let count = |svg: &str, options: &usvg::Options| {
            image_count(
                usvg::Tree::from_data(svg.as_bytes(), options)
                    .expect("tree")
                    .root(),
            )
        };
        assert_eq!(
            count(&page(&path), &stock),
            1,
            "control: usvg reads a file path"
        );
        for href in &hrefs {
            assert_eq!(count(&page(href), &ours), 0, "page: {href}");
            assert_eq!(
                count(&nested(href), &ours),
                1,
                "nested: only the data URL itself, {href}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_is_deterministic() {
        let Some(db) = system_font_db() else { return };
        let pages = vec![page_svg(SAMPLE_TEXT)];
        assert_eq!(
            svgs_to_searchable_pdf(&pages, &db),
            svgs_to_searchable_pdf(&pages, &db)
        );
    }

    /// The batches an export arrives in do not show in the file: the same pages give the same bytes
    /// in one slice, ten at a time, or one at a time.
    #[test]
    fn pages_added_in_batches_give_the_bytes_of_one_slice() {
        let Some(db) = system_font_db() else { return };
        let pages: Vec<String> = (0..25)
            .map(|i| page_svg(&format!("{SAMPLE_TEXT} page {i}")))
            .collect();
        let whole = svgs_to_searchable_pdf(&pages, &db).expect("pdf");
        for size in [1, 10] {
            let mut assembler = PdfAssembler::new(&db, true);
            for batch in pages.chunks(size) {
                assembler.add_pages(batch).expect("batch");
            }
            assert_eq!(assembler.page_count(), pages.len());
            assert_eq!(assembler.finish().expect("pdf"), whole, "batches of {size}");
        }
    }

    #[test]
    fn a_failing_page_of_a_later_batch_is_named_by_its_place_in_the_document() {
        let db = Arc::new(Database::new());
        let mut assembler = PdfAssembler::new(&db, true);
        assembler
            .add_pages(&vec![blank_page(8); 10])
            .expect("first batch");
        let err = assembler
            .add_pages(&[blank_page(8), "not svg".to_string()])
            .unwrap_err();
        assert!(err.starts_with("12쪽"), "{err}");
    }

    #[test]
    fn an_assembler_with_no_pages_has_no_pdf() {
        let db = Arc::new(Database::new());
        assert!(PdfAssembler::new(&db, true).finish().is_err());
        assert!(PdfAssembler::new(&db, true).add_pages(&[]).is_err());
    }

    #[test]
    fn a_real_export_passes_the_check_with_one_media_box_per_page() {
        let Some(db) = system_font_db() else { return };
        let pages = vec![page_svg(SAMPLE_TEXT), page_svg("Second page")];
        let pdf = svgs_to_searchable_pdf(&pages, &db).expect("pdf");
        assert_eq!(
            verify_exported_pdf(&pdf, 2, true),
            Ok(PdfCheck {
                pages: 2,
                warning: None
            })
        );
        assert!(verify_exported_pdf(&pdf, 3, true)
            .unwrap_err()
            .contains("/Count"));
        assert!(verify_exported_pdf(&pdf, 1, true).is_err());
    }

    #[test]
    fn an_export_without_text_needs_no_font_and_outlined_text_is_a_warning() {
        let Some(db) = system_font_db() else { return };
        let pages = vec![page_svg(SAMPLE_TEXT)];
        let outlined = svgs_to_pdf(&pages, &db, false).expect("pdf");
        let check = verify_exported_pdf(&outlined, 1, true).expect("outlines are valid");
        assert!(check.warning.is_some_and(|w| w.contains("윤곽선")));
        assert_eq!(
            verify_exported_pdf(&outlined, 1, false).unwrap().warning,
            None
        );
    }

    #[test]
    fn damaged_or_incomplete_pdfs_are_refused() {
        let Some(db) = system_font_db() else { return };
        let good = svgs_to_searchable_pdf(&[page_svg(SAMPLE_TEXT)], &db).expect("pdf");
        let broken = |from: &[u8], to: &[u8]| {
            let at = good
                .windows(from.len())
                .position(|w| w == from)
                .expect("key");
            let mut bytes = good.clone();
            bytes[at..at + to.len()].copy_from_slice(to);
            bytes
        };
        let header = verify_exported_pdf(&good[5..], 1, true).unwrap_err();
        assert!(header.contains("%PDF-"), "{header}");
        let media = verify_exported_pdf(&broken(b"/MediaBox", b"/Mediabox"), 1, true);
        assert!(media.unwrap_err().contains("/MediaBox"));
        let map = verify_exported_pdf(&broken(b"/ToUnicode", b"/ToUnicodf"), 1, true);
        assert!(map.unwrap_err().contains("/ToUnicode"));
        assert!(verify_exported_pdf(b"", 1, false).is_err());
        assert!(verify_exported_pdf(b"%PDF-1.7", 1, false).is_err());
    }

    #[test]
    fn page_counts_are_read_as_bounded_digit_runs() {
        assert_eq!(declared_counts(b"/Count 12 /Count 3>>"), vec![12, 3]);
        assert_eq!(declared_counts(b"/Count x /Count "), Vec::<usize>::new());
        assert_eq!(
            declared_counts(b"/Count 99999999999999999999"),
            vec![9_999_999],
            "reading stops after the digit limit"
        );
    }
}

#[cfg(test)]
#[path = "pdf_export_text_tests.rs"]
mod text_tests;

#[cfg(test)]
#[path = "pdf_export_raster_tests.rs"]
mod raster_tests;
