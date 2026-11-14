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

use crate::raster_budget::is_raster_mime;
use crate::raster_budget::RasterBudget;
use easyoffice_formats::checked_svg_text;
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
pub const BATCH_PAGES: usize = 16;
/// Threads that convert pages at once. A dense page needs on the order of 100 MB to convert, so the
/// count stays small whatever the machine offers.
const MAX_CONVERT_THREADS: usize = 4;
/// What a page-conversion thread that panicked turns into.
pub(crate) const MSG_CONVERT_PANICKED: &str =
    "PDF 변환 중 내부 오류가 발생해 내보내기를 중단했습니다.";
/// Deepest `<image>` inside `<image>` of SVG data URLs that is kept; deeper pictures are dropped.
/// WMF/EMF pictures are one level (an SVG that holds a bitmap); the bound keeps a hostile chain of
/// nested data URLs from recursing without end.
const MAX_NESTED_SVG_DEPTH: u32 = 4;
/// Largest nested SVG picture once inflated (bytes). The text a data URL holds is read by the SVG
/// parser, which expands entity declarations without a bound and inflates gzip without one, so
/// [`checked_svg_text`] reads every nested picture first; one it refuses is dropped.
const MAX_NESTED_SVG_BYTES: usize = 16 * 1_048_576;
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
        resolve_data: Box::new(move |mime, data, options| {
            if mime.eq_ignore_ascii_case("image/svg+xml") {
                let _level = NestedLevel::enter()?;
                let text = checked_svg_text(&data, MAX_NESTED_SVG_BYTES).ok()?;
                crate::svg_budget::check_svg_structure(&text).ok()?;
                return usvg::Tree::from_str(&text, options)
                    .ok()
                    .map(usvg::ImageKind::SVG);
            }
            // Only a raster type goes on to usvg's resolver (it reads `text/plain` as a picture of
            // any format, an SVG with entity declarations included). The decoders allocate by the
            // size in the picture's header, so a page SVG (untrusted) is kept from making a
            // picture or a page cost more than a fixed number of pixels. A refused picture is left
            // out and the page is kept (PD-145).
            if !is_raster_mime(mime) || !budget.admit(&data) {
                return None;
            }
            default_data(&mime.to_ascii_lowercase(), data, options)
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
    let done = std::thread::scope(|scope| {
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
        // Every thread is joined before the result is looked at: a scoped thread that is left
        // unjoined and panicked panics the scope.
        let joined: Vec<_> = workers.into_iter().map(|worker| worker.join()).collect();
        let mut done: Vec<(usize, Result<PageChunk, String>)> = Vec::new();
        for local in joined {
            // A thread that panicked (hostile page content) fails the export, not this thread.
            done.extend(local.map_err(|_| MSG_CONVERT_PANICKED.to_string())?);
        }
        Ok::<_, String>(done)
    });
    let mut done = done?;
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
    /// Clockwise turn of every page when it is shown (`/Rotate`), a multiple of 90.
    rotation: i32,
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
            rotation: 0,
            alloc,
            catalog_ref,
            tree_ref,
            page_refs: Vec::new(),
            renumber_map: HashMap::new(),
            pdf: Pdf::new(),
        }
    }

    /// Shows every page turned clockwise by `degrees` (0, 90, 180 or 270); the page content is not
    /// touched. Call before the first page is added.
    pub fn set_rotation(&mut self, degrees: u16) {
        self.rotation = i32::from(degrees % 360);
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
                    self.rotation,
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
    rotation: i32,
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
    if rotation != 0 {
        writer.rotate(rotation);
    }
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
pub(crate) mod tests;

#[cfg(test)]
#[path = "pdf_export_text_tests.rs"]
mod text_tests;

#[cfg(test)]
#[path = "pdf_export_raster_tests.rs"]
mod raster_tests;
