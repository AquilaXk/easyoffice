use super::*;
use crate::status::*;
use base64::Engine;
use std::io::Write;
use std::time::Instant;

fn sample(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../third_party/rhwp/rhwp-studio/public/samples")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|err| {
        panic!(
            "sample {} is missing ({err}); run `node scripts/checkout-rhwp.mjs` first",
            path.display()
        )
    })
}

fn take(bytes: Bytes) -> Vec<u8> {
    assert!(!bytes.ptr.is_null() && bytes.len > 0);
    let copy = unsafe { std::slice::from_raw_parts(bytes.ptr, bytes.len) }.to_vec();
    unsafe { eo_ql_free_bytes(bytes.ptr, bytes.len) };
    copy
}

fn png_size(png: &[u8]) -> (u32, u32) {
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    let at = |i: usize| u32::from_be_bytes(png[i..i + 4].try_into().unwrap());
    (at(16), at(20))
}

fn first_page_png(data: &[u8], side: u32) -> RenderResult {
    unsafe { eo_ql_render_first_page_png(data.as_ptr(), data.len(), side) }
}

fn preview_pdf(data: &[u8]) -> RenderResult {
    unsafe { eo_ql_render_preview_pdf(data.as_ptr(), data.len()) }
}

fn embedded(data: &[u8]) -> ThumbnailResult {
    unsafe { eo_ql_extract_embedded_thumbnail(data.as_ptr(), data.len()) }
}

fn count_of(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|w| *w == needle)
        .count()
}

/// Every call that failed hands back no buffer and zeroed sizes.
fn assert_clean_failure(result: RenderResult, status: i32) {
    assert_eq!(result.status, status);
    assert!(result.bytes.ptr.is_null() && result.bytes.len == 0);
    assert_eq!(result.page_count, 0);
}

// --- input checks --------------------------------------------------------------------------

#[test]
fn empty_and_null_input_are_invalid() {
    assert_clean_failure(
        unsafe { eo_ql_render_preview_pdf(std::ptr::null(), 0) },
        STATUS_INVALID_INPUT,
    );
    assert_clean_failure(
        unsafe { eo_ql_render_first_page_png(std::ptr::null(), 10, 512) },
        STATUS_INVALID_INPUT,
    );
    let one = [0_u8];
    assert_clean_failure(
        unsafe { eo_ql_render_first_page_png(one.as_ptr(), 0, 512) },
        STATUS_INVALID_INPUT,
    );
    let result = unsafe { eo_ql_extract_embedded_thumbnail(std::ptr::null(), 0) };
    assert_eq!(result.status, STATUS_INVALID_INPUT);
}

#[test]
fn a_file_over_the_limit_is_refused_before_it_is_read() {
    let huge = vec![0_u8; render::MAX_INPUT_BYTES + 1];
    assert_clean_failure(preview_pdf(&huge), STATUS_TOO_LARGE);
    assert_clean_failure(first_page_png(&huge, 256), STATUS_TOO_LARGE);
    assert_eq!(embedded(&huge).status, STATUS_TOO_LARGE);
}

#[test]
fn text_that_is_not_a_document_fails_closed() {
    let data = b"this is not a hwp document";
    assert_clean_failure(preview_pdf(data), STATUS_PARSE_FAILED);
    assert_clean_failure(first_page_png(data, 256), STATUS_PARSE_FAILED);
    let result = embedded(data);
    assert_eq!(result.status, STATUS_NO_THUMBNAIL);
    assert!(result.bytes.ptr.is_null());
}

#[test]
fn free_accepts_null_and_zero() {
    unsafe {
        eo_ql_free_bytes(std::ptr::null_mut(), 0);
        eo_ql_free_bytes(std::ptr::null_mut(), 7);
    }
}

// --- real documents --------------------------------------------------------------------------

#[test]
fn first_page_png_of_hwp_and_hwpx_fits_the_requested_side() {
    for name in ["field-01.hwp", "form-002.hwpx"] {
        let result = first_page_png(&sample(name), 512);
        assert_eq!(result.status, STATUS_OK, "{name}");
        assert!(result.page_count > 1, "{name}");
        assert!(result.width > 0.0 && result.height > result.width, "{name}");
        let (w, h) = png_size(&take(result.bytes));
        assert_eq!(w.max(h), 512, "{name}");
        assert!(w < h, "{name} keeps its portrait shape");
    }
}

#[test]
fn png_side_is_clamped_to_the_pixel_cap() {
    let data = sample("tac-case-001.hwp");
    for requested in [0, u32::MAX, render::MAX_PNG_SIDE + 1] {
        let result = first_page_png(&data, requested);
        assert_eq!(result.status, STATUS_OK, "side {requested}");
        let (w, h) = png_size(&take(result.bytes));
        assert_eq!(w.max(h), render::MAX_PNG_SIDE, "side {requested}");
    }
    let result = first_page_png(&data, 1);
    assert_eq!(result.status, STATUS_OK);
    let (w, h) = png_size(&take(result.bytes));
    assert_eq!(w.max(h), 1);
}

#[test]
fn preview_pdf_has_one_page_per_document_page() {
    let result = preview_pdf(&sample("field-01.hwp"));
    assert_eq!(result.status, STATUS_OK);
    assert_eq!(result.page_count, 3);
    let pdf = take(result.bytes);
    assert!(pdf.starts_with(b"%PDF-"));
    assert!(pdf
        .windows(9)
        .any(|w| w == b"/Count 3\n" || w == b"/Count 3 "));
    assert_eq!(
        count_of(&pdf, b"/Type /Page\n") + count_of(&pdf, b"/Type /Page "),
        3
    );
}

#[test]
fn embedded_picture_is_returned_as_stored_or_reported_missing() {
    let result = embedded(&sample("field-01.hwp"));
    assert_eq!(result.status, STATUS_OK);
    assert_eq!((result.width, result.height), (724, 1024));
    assert_eq!(png_size(&take(result.bytes)), (724, 1024));
    let hwpx = embedded(&sample("form-002.hwpx"));
    assert_eq!(hwpx.status, STATUS_OK);
    take(hwpx.bytes);
    assert_eq!(
        embedded(&sample("tac-case-001.hwp")).status,
        STATUS_NO_THUMBNAIL
    );
}

// --- hostile files ---------------------------------------------------------------------------

/// Every outcome is a status (never a crash or a hang), and a failure carries no buffer.
fn exercise_all_calls(data: &[u8]) {
    let started = Instant::now();
    let png = first_page_png(data, 256);
    if png.status == STATUS_OK {
        png_size(&take(png.bytes));
    } else {
        assert_clean_failure(png, png.status);
    }
    let pdf = preview_pdf(data);
    if pdf.status == STATUS_OK {
        assert!(take(pdf.bytes).starts_with(b"%PDF-"));
    } else {
        assert_clean_failure(pdf, pdf.status);
    }
    let picture = embedded(data);
    if picture.status == STATUS_OK {
        take(picture.bytes);
    } else {
        assert!(picture.bytes.ptr.is_null());
    }
    assert!(
        started.elapsed().as_secs() < 25,
        "a request outlived its deadline"
    );
}

#[test]
fn truncated_documents_never_crash() {
    for name in ["field-01.hwp", "form-002.hwpx"] {
        let data = sample(name);
        for tenth in [1, 3, 5, 7, 9] {
            exercise_all_calls(&data[..data.len() * tenth / 10]);
        }
    }
}

#[test]
fn corrupted_documents_never_crash() {
    let mut seed = 0x9E37_79B9_7F4A_7C15_u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for name in ["field-01.hwp", "form-002.hwpx"] {
        let original = sample(name);
        for _ in 0..4 {
            let mut data = original.clone();
            for _ in 0..48 {
                let at = (next() % data.len() as u64) as usize;
                data[at] ^= 1 << (next() % 8);
            }
            // The container header is where parsers decide what to allocate.
            for at in 0..64.min(data.len()) {
                if next() % 5 == 0 {
                    data[at] = next() as u8;
                }
            }
            exercise_all_calls(&data);
        }
    }
}

fn hwpx_with_entry(name: &str, body_len: usize) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("mimetype", options).unwrap();
    zip.write_all(b"application/hwp+zip").unwrap();
    zip.start_file(name, options).unwrap();
    let block = vec![b' '; 1 << 20];
    for _ in 0..body_len >> 20 {
        zip.write_all(&block).unwrap();
    }
    zip.finish().unwrap();
    out.into_inner()
}

#[test]
fn a_decompression_bomb_in_an_hwpx_stays_bounded() {
    // 24 KB or so of ZIP that inflates to 512 MiB of spaces in the preview picture and the header.
    for entry in [
        "Preview/PrvImage.png",
        "Contents/header.xml",
        "Contents/section0.xml",
    ] {
        let bomb = hwpx_with_entry(entry, 512 << 20);
        assert!(
            bomb.len() < render::MAX_INPUT_BYTES / 100,
            "{entry} compresses well"
        );
        exercise_all_calls(&bomb);
    }
}

// --- page SVG: the part that reads text from the document ----------------------------------

fn svg(body: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="100" height="100" viewBox="0 0 100 100">{body}</svg>"#
    )
}

fn rendered(svg: &str) -> tiny_skia::Pixmap {
    let page = render::parse_svg(svg).expect("svg");
    tiny_skia::Pixmap::decode_png(&render::png(&page, 20).expect("png")).expect("decode")
}

#[test]
fn a_page_svg_over_the_limit_is_refused() {
    let big = svg(&" ".repeat(render::MAX_PAGE_SVG_BYTES));
    assert_eq!(render::parse_svg(&big).err(), Some(STATUS_TOO_LARGE));
}

#[test]
fn a_path_href_is_never_read() {
    let dir = std::env::temp_dir().join(format!("eo-ql-href-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut red = tiny_skia::Pixmap::new(8, 8).unwrap();
    red.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 255));
    let file = dir.join("red.png");
    std::fs::write(&file, red.encode_png().unwrap()).unwrap();
    for href in [
        file.display().to_string(),
        format!("file://{}", file.display()),
    ] {
        let page = rendered(&svg(&format!(
            r#"<image x="0" y="0" width="100" height="100" href="{href}"/>"#
        )));
        assert!(
            page.pixels()
                .iter()
                .all(|p| p.red() == 255 && p.green() == 255),
            "the file's pixels must not appear for {href}"
        );
    }
    // A data URL is the document's own picture and does draw.
    let data = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(red.encode_png().unwrap())
    );
    let page = rendered(&svg(&format!(
        r#"<image x="0" y="0" width="100" height="100" href="{data}"/>"#
    )));
    assert!(page.pixels().iter().any(|p| p.green() < 128));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn absurd_page_sizes_are_drawn_within_the_pixel_cap_or_refused() {
    for (w, h) in [("1e9", "1e9"), ("100000", "1"), ("0.0001", "0.0001")] {
        let text = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}"><rect width="10" height="10"/></svg>"#
        );
        if let Ok(page) = render::parse_svg(&text) {
            let (pw, ph) = png_size(&render::png(&page, 4096).expect("png"));
            assert!(pw.max(ph) <= render::MAX_PNG_SIDE, "{w}x{h}");
        }
    }
    for text in [
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="0" height="10"/>"#,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="-5" height="10"/>"#,
        "not an svg",
        "",
    ] {
        assert!(render::parse_svg(text).is_err(), "{text:?}");
    }
}

#[test]
fn deeply_nested_and_self_referencing_svg_does_not_crash() {
    let depth = 20_000;
    let nested = svg(&format!("{}{}", "<g>".repeat(depth), "</g>".repeat(depth)));
    let looping = svg(
        r##"<defs><g id="a"><use href="#b"/></g><g id="b"><use href="#a"/></g></defs><use href="#a"/>"##,
    );
    for text in [nested, looping] {
        let outcome = bounded::run(Duration::from_secs(20), move || {
            let page = render::parse_svg(&text)?;
            render::png(&page, 64)
        });
        assert!(outcome.is_ok() || outcome.is_err());
    }
}

// --- the worker --------------------------------------------------------------------------------

#[test]
fn a_slow_request_times_out() {
    let started = Instant::now();
    let outcome: Result<(), i32> = bounded::run(Duration::from_millis(100), || {
        std::thread::sleep(Duration::from_secs(3));
        Ok(())
    });
    assert_eq!(outcome, Err(STATUS_TIMEOUT));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn a_panic_becomes_a_status() {
    let outcome: Result<(), i32> = bounded::run(Duration::from_secs(5), || panic!("boom"));
    assert_eq!(outcome, Err(STATUS_RENDER_FAILED));
}

#[test]
fn abandoned_workers_are_capped_and_overlapping_requests_are_not() {
    static LIMIT: bounded::Limiter = bounded::Limiter::new(2);
    let (release, hold) = std::sync::mpsc::channel::<()>();
    let hold = std::sync::Arc::new(std::sync::Mutex::new(hold));
    for _ in 0..2 {
        let hold = hold.clone();
        let outcome: Result<(), i32> =
            bounded::run_limited(&LIMIT, Duration::from_millis(50), move || {
                let _ = hold.lock().unwrap().recv();
                Ok(())
            });
        assert_eq!(outcome, Err(STATUS_TIMEOUT));
    }
    // Both timed-out workers still run: a third request is refused without starting a thread.
    let third: Result<u8, i32> = bounded::run_limited(&LIMIT, Duration::from_secs(5), || Ok(1));
    assert_eq!(third, Err(STATUS_TIMEOUT));
    drop(release);
    // They end when the channel closes, and requests are served again.
    let started = Instant::now();
    loop {
        let again: Result<u8, i32> = bounded::run_limited(&LIMIT, Duration::from_secs(5), || Ok(1));
        if again == Ok(1) {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "slots never came back"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    // Many requests at once are all served: only abandoned workers count.
    let outcomes: Vec<_> = (0..12)
        .map(|n| {
            std::thread::spawn(move || {
                bounded::run_limited(&LIMIT, Duration::from_secs(5), move || Ok(n))
            })
        })
        .collect();
    for (n, handle) in outcomes.into_iter().enumerate() {
        assert_eq!(handle.join().unwrap(), Ok(n));
    }
}
