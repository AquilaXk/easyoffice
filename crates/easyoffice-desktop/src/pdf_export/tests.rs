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
    let pdf =
        svgs_to_searchable_pdf(&[KOREAN_SVG.to_string()], &Arc::new(Database::new())).expect("pdf");
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
    const DIGITS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
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
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, 0x49, 0x48, 0x44, 0x52, 0,
        0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1F, 0x15, 0xC4, 0x89, 0, 0, 0, 0x0D, 0x49, 0x44,
        0x41, 0x54, 0x78, 0x9C, 0x63, 0, 1, 0, 0, 5, 0, 1, 0x0D, 0x0A, 0x2D, 0xB4, 0, 0, 0, 0,
        0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
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
