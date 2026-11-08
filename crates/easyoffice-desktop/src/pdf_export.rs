//! Searchable PDF export of the Hangul editor's page SVGs (PD-101).
//!
//! The editor renders each page to SVG; this module turns those pages into one PDF whose text is
//! real text (embedded font subsets with a `ToUnicode` map), so the result can be searched and
//! copied. Adapted from the page assembly of the rhwp PDF renderer and from the reference app's
//! `pdf_export.rs` (MIT, see THIRD_PARTY_NOTICES.md): one `svg2pdf` chunk per page, chunks
//! renumbered into one object space, a form XObject per page placed on a media box in points.
//!
//! Hot path: page SVGs are parsed straight from their bytes (no copy), the font database is built
//! once per process, and every size is bounded with `checked_*` arithmetic before any work starts.

use pdf_writer::{Chunk, Content, Finish, Name, Pdf, Rect, Ref, TextStr};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use usvg::fontdb::Database;

/// Most pages one export may contain.
pub const MAX_PAGES: usize = 2_000;
/// Largest single page SVG (bytes).
pub const MAX_PAGE_SVG_BYTES: usize = 64 * 1_048_576;
/// Largest sum of all page SVGs (bytes).
pub const MAX_TOTAL_SVG_BYTES: usize = 512 * 1_048_576;
/// SVG user units are CSS pixels (96 per inch); PDF user units are points (72 per inch).
const PX_TO_PT: f32 = 72.0 / 96.0;
/// Name of the page's form XObject.
const PAGE_XOBJECT: Name<'static> = Name(b"S1");
const PRODUCER: &str = "EasyOffice";
/// Family names used for the generic `serif`/`sans-serif`/`monospace` fallbacks.
const FALLBACK_FAMILY: &str = "Noto Sans KR";

static FONT_DB: OnceLock<Arc<Database>> = OnceLock::new();

/// Font database shared by every export: the given directories (bundled OFL fonts) first, then
/// the system fonts. The first call decides the directories; later calls reuse the database, so
/// the font files are scanned once per process.
pub fn shared_font_db(font_dirs: &[PathBuf]) -> Arc<Database> {
    FONT_DB
        .get_or_init(|| Arc::new(build_font_db(font_dirs)))
        .clone()
}

fn build_font_db(font_dirs: &[PathBuf]) -> Database {
    let mut db = Database::new();
    for dir in font_dirs {
        db.load_fonts_dir(dir);
    }
    db.load_system_fonts();
    db.set_serif_family(FALLBACK_FAMILY);
    db.set_sans_serif_family(FALLBACK_FAMILY);
    db.set_monospace_family(FALLBACK_FAMILY);
    db
}

/// Checks the page count and byte limits before anything is parsed.
fn check_limits(pages: &[String]) -> Result<(), String> {
    if pages.is_empty() {
        return Err("내보낼 페이지가 없습니다.".to_string());
    }
    if pages.len() > MAX_PAGES {
        return Err(format!(
            "PDF로 내보낼 수 있는 쪽 수({MAX_PAGES})를 초과했습니다."
        ));
    }
    let mut total = 0usize;
    for (index, svg) in pages.iter().enumerate() {
        if svg.len() > MAX_PAGE_SVG_BYTES {
            return Err(format!("{}쪽 SVG가 허용 크기를 초과합니다.", index + 1));
        }
        total = total
            .checked_add(svg.len())
            .filter(|sum| *sum <= MAX_TOTAL_SVG_BYTES)
            .ok_or_else(|| "페이지 SVG 전체 크기가 허용 한도를 초과합니다.".to_string())?;
    }
    Ok(())
}

/// One converted page before assembly.
struct PageChunk {
    chunk: Chunk,
    xobject: Ref,
    width: f32,
    height: f32,
}

fn convert_page(
    index: usize,
    svg: &str,
    options: &usvg::Options,
    conversion: svg2pdf::ConversionOptions,
) -> Result<PageChunk, String> {
    let svg = crate::pdf_font_fallbacks::add_font_fallbacks(svg);
    let tree = usvg::Tree::from_data(svg.as_bytes(), options)
        .map_err(|err| format!("{}쪽 SVG를 읽지 못했습니다: {err}", index + 1))?;
    let (chunk, xobject) = svg2pdf::to_chunk(&tree, conversion)
        .map_err(|err| format!("{}쪽을 PDF로 바꾸지 못했습니다: {err:?}", index + 1))?;
    Ok(PageChunk {
        chunk,
        xobject,
        width: tree.size().width() * PX_TO_PT,
        height: tree.size().height() * PX_TO_PT,
    })
}

/// Converts page SVGs to one searchable PDF. Fails closed on any page that cannot be converted:
/// a partial document would silently lose pages.
pub fn svgs_to_searchable_pdf(pages: &[String], fonts: &Arc<Database>) -> Result<Vec<u8>, String> {
    svgs_to_pdf(pages, fonts, true)
}

/// `embed_text = false` turns glyphs into outlines (smaller work, but the text can no longer be
/// searched); it exists for the before/after measurement in the tests.
fn svgs_to_pdf(
    pages: &[String],
    fonts: &Arc<Database>,
    embed_text: bool,
) -> Result<Vec<u8>, String> {
    check_limits(pages)?;
    let conversion = svg2pdf::ConversionOptions {
        embed_text,
        ..svg2pdf::ConversionOptions::default()
    };
    let options = usvg::Options {
        fontdb: Arc::clone(fonts),
        ..usvg::Options::default()
    };
    let converted = pages
        .iter()
        .enumerate()
        .map(|(index, svg)| convert_page(index, svg, &options, conversion))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(assemble(converted))
}

/// Builds the page tree and places every page's form XObject.
fn assemble(pages: Vec<PageChunk>) -> Vec<u8> {
    let mut alloc = Ref::new(1);
    let catalog_ref = alloc.bump();
    let tree_ref = alloc.bump();
    let page_refs: Vec<Ref> = pages.iter().map(|_| alloc.bump()).collect();
    let mut pdf = Pdf::new();
    pdf.catalog(catalog_ref).pages(tree_ref);
    let count = i32::try_from(page_refs.len()).unwrap_or(i32::MAX);
    pdf.pages(tree_ref)
        .count(count)
        .kids(page_refs.iter().copied());
    let mut renumbered = Vec::with_capacity(pages.len());
    for (page, page_ref) in pages.iter().zip(&page_refs) {
        let mut map: HashMap<Ref, Ref> = HashMap::new();
        let chunk = page
            .chunk
            .renumber(|old| *map.entry(old).or_insert_with(|| alloc.bump()));
        let xobject = map.get(&page.xobject).copied().unwrap_or(page.xobject);
        let content_ref = alloc.bump();
        write_page(&mut pdf, *page_ref, tree_ref, content_ref, xobject, page);
        renumbered.push(chunk);
    }
    for chunk in &renumbered {
        pdf.extend(chunk);
    }
    let info_ref = alloc.bump();
    pdf.document_info(info_ref).producer(TextStr(PRODUCER));
    pdf.finish()
}

fn write_page(
    pdf: &mut Pdf,
    page_ref: Ref,
    tree_ref: Ref,
    content_ref: Ref,
    xobject: Ref,
    page: &PageChunk,
) {
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
const KEY_MEDIA_BOX: &[u8] = b"/MediaBox";
const KEY_TO_UNICODE: &[u8] = b"/ToUnicode";
const KEYS_FONT_FILE: [&[u8]; 2] = [b"/FontFile2", b"/FontFile3"];

/// What [`verify_exported_pdf`] found.
#[derive(Debug, PartialEq, Eq)]
pub struct PdfCheck {
    pub pages: usize,
    /// Set when the PDF is valid but its text cannot be searched (see [`verify_exported_pdf`]).
    pub warning: Option<String>,
}

fn count_of(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|w| *w == needle)
        .count()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// The numbers that follow each `/Count ` key (page tree sizes).
fn declared_counts(pdf: &[u8]) -> Vec<usize> {
    pdf.windows(KEY_COUNT.len())
        .enumerate()
        .filter(|(_, window)| *window == KEY_COUNT)
        .filter_map(|(at, _)| {
            let digits: Vec<u8> = pdf[at + KEY_COUNT.len()..]
                .iter()
                .copied()
                .take_while(u8::is_ascii_digit)
                .take(MAX_COUNT_DIGITS)
                .collect();
            std::str::from_utf8(&digits).ok()?.parse().ok()
        })
        .collect()
}

/// Digits read after `/Count `: more than this cannot be a page count.
const MAX_COUNT_DIGITS: usize = 7;

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
    if !declared_counts(pdf).contains(&expected_pages) {
        return Err(format!(
            "PDF 쪽 수(/Count)가 {expected_pages}쪽이 아닙니다."
        ));
    }
    let media_boxes = count_of(pdf, KEY_MEDIA_BOX);
    if media_boxes != expected_pages {
        return Err(format!(
            "PDF에 쪽 크기(/MediaBox)가 {media_boxes}개 있습니다. {expected_pages}개여야 합니다."
        ));
    }
    let has_font = KEYS_FONT_FILE.iter().any(|key| contains(pdf, key));
    let has_map = contains(pdf, KEY_TO_UNICODE);
    if has_font && !has_map {
        return Err("PDF에 글꼴은 있으나 글자 대응표(/ToUnicode)가 없습니다.".to_string());
    }
    let warning = (has_text && !has_font).then(|| {
        "글꼴이 포함되지 않아 글자가 윤곽선으로 나갔습니다(PDF는 올바르지만 글자 검색 불가)."
            .to_string()
    });
    Ok(PdfCheck {
        pages: expected_pages,
        warning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const SAMPLE_TEXT: &str = "Searchable PDF 12345";

    fn page_svg(text: &str) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="320" height="120"><rect width="320" height="120" fill="white"/><text x="16" y="64" font-family="DejaVu Sans" font-size="24">{text}</text></svg>"#
        )
    }

    fn system_font_db() -> Option<Arc<Database>> {
        let mut db = Database::new();
        for dir in [
            "/usr/share/fonts",
            "/System/Library/Fonts",
            "C:/Windows/Fonts",
        ] {
            if Path::new(dir).is_dir() {
                db.load_fonts_dir(dir);
            }
        }
        db.load_system_fonts();
        (!db.is_empty()).then(|| Arc::new(db))
    }

    #[test]
    fn limits_reject_empty_oversized_and_overflowing_inputs() {
        assert!(check_limits(&[]).is_err());
        let many = vec![String::new(); MAX_PAGES + 1];
        assert!(check_limits(&many).unwrap_err().contains("쪽 수"));
        let big = vec!["x".repeat(MAX_PAGE_SVG_BYTES + 1)];
        assert!(check_limits(&big).unwrap_err().contains("1쪽"));
        let page = "x".repeat(MAX_PAGE_SVG_BYTES);
        let too_many_bytes = vec![page; MAX_TOTAL_SVG_BYTES / MAX_PAGE_SVG_BYTES + 1];
        assert!(check_limits(&too_many_bytes)
            .unwrap_err()
            .contains("전체 크기"));
        assert!(check_limits(&[page_svg("ok")]).is_ok());
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

    #[test]
    fn export_is_deterministic() {
        let Some(db) = system_font_db() else { return };
        let pages = vec![page_svg(SAMPLE_TEXT)];
        assert_eq!(
            svgs_to_searchable_pdf(&pages, &db),
            svgs_to_searchable_pdf(&pages, &db)
        );
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

    #[test]
    fn the_shared_font_database_is_built_once() {
        let first = shared_font_db(&[]);
        let second = shared_font_db(&[PathBuf::from("/nonexistent")]);
        assert!(Arc::ptr_eq(&first, &second));
    }
}
