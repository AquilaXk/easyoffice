//! First-page renders for the start window's cards (PD-142): the picture, its limits, the worker
//! bounds and the cache. The renderer is injected where the test is about the bounds, and real
//! where it is about the picture (the rhwp samples need `node scripts/checkout-rhwp.mjs`).

use super::bounded::{self, Gate, RenderError};
use super::*;
use crate::pdf_fonts::shared_font_db;
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

const SAMPLES: &str = "../../third_party/rhwp/rhwp-studio/public/samples";
const PNG_1X1: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R', 0, 0, 0,
    1, 0, 0, 0, 1, 8, 6, 0, 0, 0,
];

fn sample(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(SAMPLES)
        .join(name);
    assert!(
        path.is_file(),
        "sample {} is missing; run `node scripts/checkout-rhwp.mjs` first",
        path.display()
    );
    path
}

fn png_size(png: &[u8]) -> (u32, u32) {
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
    let at = |i: usize| u32::from_be_bytes(png[i..i + 4].try_into().unwrap());
    (at(16), at(20))
}

fn limits(deadline_ms: u64, wait_ms: u64) -> RenderLimits {
    RenderLimits {
        deadline: Duration::from_millis(deadline_ms),
        permit_wait: Duration::from_millis(wait_ms),
    }
}

fn state(workers: usize, deadline_ms: u64, wait_ms: u64) -> PreviewState {
    PreviewState::with_limits(Vec::new(), workers, limits(deadline_ms, wait_ms))
}

/// A `.hwp` file with the given bytes in a fresh directory.
fn hwp_file(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, bytes).unwrap();
    path
}

fn fake_png(_: Vec<u8>) -> Result<Vec<u8>, RenderError> {
    Ok(PNG_1X1.to_vec())
}

use std::fs;

// --- the real page ------------------------------------------------------------------------

/// A sample whose preview picture is renamed in place (same length, so every offset stays valid),
/// so the cheap path finds nothing and the page has to be drawn. The sample itself has one.
fn without_picture(name: &str, dir: &tempfile::TempDir) -> PathBuf {
    let mut bytes = fs::read(sample(name)).unwrap();
    assert!(
        extract_thumbnail(&mut Cursor::new(&bytes)).is_some(),
        "{name} has a picture"
    );
    // HWPX: the zip entry name (local header and central directory); HWP: the UTF-16 stream name.
    let utf16 = |text: &str| {
        text.encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<u8>>()
    };
    let renames = [
        (b"Preview/PrvImage".to_vec(), b"Preview/PrvImagX".to_vec()),
        (utf16("PrvImage"), utf16("PrvImagX")),
    ];
    for (from, to) in renames {
        let mut at = 0;
        while let Some(found) = bytes[at..].windows(from.len()).position(|w| w == from) {
            bytes[at + found..at + found + from.len()].copy_from_slice(&to);
            at += found + from.len();
        }
    }
    assert!(
        extract_thumbnail(&mut Cursor::new(&bytes)).is_none(),
        "{name} lost its picture"
    );
    hwp_file(dir, name, &bytes)
}

#[test]
fn a_hangul_file_without_a_picture_gets_its_first_page_drawn() {
    let dir = tempfile::tempdir().unwrap();
    let path = without_picture("biz_plan.hwp", &dir);
    let found =
        load_preview(&PreviewState::new(), "0000000000000001", &path).expect("a drawn page");
    assert_eq!(found.mime, "image/png");
    let (width, height) = png_size(&found.bytes);
    assert_eq!(width.max(height), render::CARD_SIDE_PX);
    assert!(found.bytes.len() <= MAX_RENDERED_PNG_BYTES);
    let pixmap = tiny_skia::Pixmap::decode_png(&found.bytes).unwrap();
    assert!(
        pixmap.pixels().iter().any(|p| p.red() < 200),
        "the page has text or lines, not a white sheet"
    );
}

#[test]
fn a_hwpx_file_is_drawn_too() {
    let dir = tempfile::tempdir().unwrap();
    let path = without_picture("form-002.hwpx", &dir);
    let found =
        load_preview(&PreviewState::new(), "0000000000000002", &path).expect("a drawn page");
    let (width, height) = png_size(&found.bytes);
    assert_eq!(width.max(height), render::CARD_SIDE_PX);
}

#[test]
fn the_picture_inside_the_file_is_used_without_drawing() {
    let dir = tempfile::tempdir().unwrap();
    // OOXML-style container whose thumbnail is a GIF: found by the cheap path.
    let bytes = super::tests::zip(&[(
        "Preview/PrvImage.gif",
        0,
        GIF_BYTES.to_vec(),
        GIF_BYTES.len(),
    )]);
    let path = hwp_file(&dir, "a.hwpx", &bytes);
    let drawn = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&drawn);
    let found = load_preview_with(&state(2, 1000, 100), "00000000000000a1", &path, move |b| {
        counter.fetch_add(1, Ordering::SeqCst);
        fake_png(b)
    })
    .expect("the embedded picture");
    assert_eq!(found.mime, "image/gif");
    assert_eq!(
        drawn.load(Ordering::SeqCst),
        0,
        "a file with a picture is not drawn"
    );
}

const GIF_BYTES: &[u8] = b"GIF89a\x01\x00\x01\x00\x00\x00\x00;";

#[test]
fn damaged_and_hostile_hangul_files_give_no_picture_and_no_panic() {
    let dir = tempfile::tempdir().unwrap();
    let whole = fs::read(sample("biz_plan.hwp")).unwrap();
    let mut flipped = whole.clone();
    for at in (0..flipped.len()).step_by(97) {
        flipped[at] ^= 0xA5;
    }
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("garbage.hwp", vec![0x5A; 4096]),
        ("empty.hwp", Vec::new()),
        ("cut.hwp", whole[..whole.len() / 2].to_vec()),
        ("flipped.hwp", flipped),
        ("pdf.hwpx", b"%PDF-1.7 not a hangul file".to_vec()),
    ];
    for (n, (name, bytes)) in cases.into_iter().enumerate() {
        let path = hwp_file(&dir, name, &bytes);
        let started = Instant::now();
        let found = load_preview(&PreviewState::new(), &format!("{n:016x}"), &path);
        assert!(found.is_none(), "{name}");
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{name} took too long"
        );
    }
}

// --- limits --------------------------------------------------------------------------------

#[test]
fn only_hangul_files_are_drawn() {
    let dir = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    for (n, name) in ["a.docx", "b.pdf", "c.txt", "noext"]
        .into_iter()
        .enumerate()
    {
        let path = hwp_file(&dir, name, b"data");
        let seen = Arc::clone(&calls);
        let found = load_preview_with(
            &state(2, 1000, 100),
            &format!("{n:016x}"),
            &path,
            move |b| {
                seen.fetch_add(1, Ordering::SeqCst);
                fake_png(b)
            },
        );
        assert!(found.is_none(), "{name}");
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn a_file_over_the_limit_never_reaches_the_renderer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.hwp");
    // Sparse: the length is the file's, no bytes are written.
    File::create(&path)
        .unwrap()
        .set_len(render::MAX_RENDER_INPUT_BYTES as u64 + 1)
        .unwrap();
    let reached = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&reached);
    let found = load_preview_with(&state(2, 1000, 100), "00000000000000b1", &path, move |b| {
        counter.fetch_add(1, Ordering::SeqCst);
        fake_png(b)
    });
    assert!(found.is_none());
    assert_eq!(
        reached.load(Ordering::SeqCst),
        0,
        "an oversized file never reaches the renderer"
    );
    // Exactly at the limit it is read and handed over.
    File::create(&path)
        .unwrap()
        .set_len(render::MAX_RENDER_INPUT_BYTES as u64)
        .unwrap();
    let seen = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&seen);
    let found = load_preview_with(
        &state(2, 5000, 100),
        "00000000000000b2",
        &path,
        move |bytes| {
            counter.store(bytes.len(), Ordering::SeqCst);
            fake_png(bytes)
        },
    );
    assert!(found.is_some());
    assert_eq!(seen.load(Ordering::SeqCst), render::MAX_RENDER_INPUT_BYTES);
}

/// A source that counts the reads made from it and notes the thread that made them.
struct CountingSource {
    inner: Cursor<Vec<u8>>,
    reads: Arc<AtomicUsize>,
    reader_thread: Arc<Mutex<Option<String>>>,
}

impl Read for CountingSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        *self.reader_thread.lock().unwrap() = thread::current().name().map(str::to_string);
        self.inner.read(buf)
    }
}

impl Seek for CountingSource {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(to)
    }
}

#[test]
fn the_file_is_read_on_the_worker_and_only_when_a_permit_is_free() {
    let dir = tempfile::tempdir().unwrap();
    let path = hwp_file(&dir, "a.hwp", b"data");
    let meta = fs::metadata(&path).unwrap();
    let source = || {
        let reads = Arc::new(AtomicUsize::new(0));
        let thread_name = Arc::new(Mutex::new(None));
        let source = CountingSource {
            inner: Cursor::new(b"data".to_vec()),
            reads: Arc::clone(&reads),
            reader_thread: Arc::clone(&thread_name),
        };
        (source, reads, thread_name)
    };

    // Every permit taken: the request gives up as busy and has read nothing.
    let preview = state(1, 1000, 50);
    let held = preview.gate.acquire(Duration::from_millis(10)).unwrap();
    let (busy_source, reads, _) = source();
    let outcome = draw_first_page(&preview, busy_source, &path, &meta, fake_png);
    assert_eq!(outcome.unwrap_err(), RenderError::Busy);
    assert_eq!(reads.load(Ordering::SeqCst), 0, "no permit, no read");
    drop(held);

    // A free permit: the read happens, on the render worker, not on the caller's thread.
    let (free_source, reads, thread_name) = source();
    let drawn = draw_first_page(&preview, free_source, &path, &meta, fake_png).unwrap();
    assert_eq!(drawn.mime, "image/png");
    assert!(reads.load(Ordering::SeqCst) > 0);
    assert_eq!(
        thread_name.lock().unwrap().as_deref(),
        Some("eo-home-preview")
    );
}

#[test]
fn the_renderer_refuses_empty_and_oversized_input_itself() {
    let fonts = shared_font_db(&[]);
    assert_eq!(
        render::first_page_png(&[], &fonts),
        Err(RenderError::TooLarge)
    );
    let big = vec![0_u8; render::MAX_RENDER_INPUT_BYTES + 1];
    assert_eq!(
        render::first_page_png(&big, &fonts),
        Err(RenderError::TooLarge)
    );
    assert_eq!(
        render::first_page_png(b"not a document", &fonts),
        Err(RenderError::Unreadable)
    );
}

#[test]
fn a_picture_that_is_too_large_or_not_a_png_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let path = hwp_file(&dir, "a.hwp", b"data");
    let huge = |_: Vec<u8>| {
        let mut png = PNG_1X1.to_vec();
        png.resize(MAX_RENDERED_PNG_BYTES + 1, 0);
        Ok(png)
    };
    assert!(load_preview_with(&state(2, 1000, 100), "00000000000000c1", &path, huge).is_none());
    let gif = |_: Vec<u8>| Ok(GIF_BYTES.to_vec());
    assert!(load_preview_with(&state(2, 1000, 100), "00000000000000c2", &path, gif).is_none());
    let mut limit_png = PNG_1X1.to_vec();
    limit_png.resize(MAX_RENDERED_PNG_BYTES, 0);
    let at_limit = move |_: Vec<u8>| Ok(limit_png);
    assert!(load_preview_with(&state(2, 1000, 100), "00000000000000c3", &path, at_limit).is_some());
}

// --- the worker ----------------------------------------------------------------------------

#[test]
fn a_render_past_its_deadline_gives_no_picture_and_keeps_its_slot_until_it_ends() {
    let dir = tempfile::tempdir().unwrap();
    let path = hwp_file(&dir, "slow.hwp", b"data");
    let other = hwp_file(&dir, "other.hwp", b"data");
    let preview = state(1, 50, 30);
    let started = Instant::now();
    let slow = |bytes: Vec<u8>| {
        thread::sleep(Duration::from_millis(500));
        fake_png(bytes)
    };
    assert!(load_preview_with(&preview, "00000000000000d1", &path, slow).is_none());
    assert!(
        started.elapsed() < Duration::from_millis(400),
        "the caller did not wait for the worker"
    );
    // The timed-out file is remembered as having no picture: it is not drawn again.
    assert!(load_preview_with(&preview, "00000000000000d1", &path, fake_png).is_none());
    // The stuck worker still holds the only slot: the next file is not tried and not remembered.
    assert!(load_preview_with(&preview, "00000000000000d2", &other, fake_png).is_none());
    let stamp = FileStamp::of(&fs::metadata(&other).unwrap());
    assert!(
        preview.cached("00000000000000d2", &stamp).is_none(),
        "a busy answer is not cached"
    );
    // When the worker ends, the slot is free again.
    thread::sleep(Duration::from_millis(600));
    assert!(load_preview_with(&preview, "00000000000000d2", &other, fake_png).is_some());
}

#[test]
fn a_panic_in_the_renderer_is_caught() {
    let dir = tempfile::tempdir().unwrap();
    let path = hwp_file(&dir, "boom.hwp", b"data");
    let preview = state(1, 1000, 100);
    assert!(
        load_preview_with(&preview, "00000000000000e1", &path, |_| panic!(
            "layout blew up"
        ))
        .is_none()
    );
    // The slot came back with the panicking thread.
    let other = hwp_file(&dir, "ok.hwp", b"data");
    assert!(load_preview_with(&preview, "00000000000000e2", &other, fake_png).is_some());
}

#[test]
fn requests_at_once_never_run_more_renders_than_the_limit() {
    let dir = tempfile::tempdir().unwrap();
    let preview = Arc::new(state(2, 5000, 5000));
    let running = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let handles: Vec<_> = (0..8)
        .map(|n| {
            let path = hwp_file(&dir, &format!("f{n}.hwp"), b"data");
            let (preview, running, peak) = (
                Arc::clone(&preview),
                Arc::clone(&running),
                Arc::clone(&peak),
            );
            thread::spawn(move || {
                load_preview_with(&preview, &format!("{n:016x}"), &path, move |bytes| {
                    let now = running.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(60));
                    running.fetch_sub(1, Ordering::SeqCst);
                    fake_png(bytes)
                })
                .is_some()
            })
        })
        .collect();
    let drawn = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .filter(|ok| *ok)
        .count();
    assert_eq!(drawn, 8, "every request is served, a few at a time");
    assert_eq!(peak.load(Ordering::SeqCst), 2);
}

#[test]
fn the_gate_hands_out_its_size_and_takes_slots_back() {
    let gate = Gate::new(2);
    let first = gate.acquire(Duration::ZERO).expect("first slot");
    let _second = gate.acquire(Duration::ZERO).expect("second slot");
    let started = Instant::now();
    assert!(gate.acquire(Duration::from_millis(40)).is_none());
    assert!(started.elapsed() >= Duration::from_millis(40));
    drop(first);
    assert!(gate.acquire(Duration::ZERO).is_some());
    let waiting = Arc::clone(&gate);
    let _third = gate.acquire(Duration::ZERO);
    let waiter = thread::spawn(move || waiting.acquire(Duration::from_secs(5)).is_some());
    thread::sleep(Duration::from_millis(30));
    drop(_second);
    assert!(
        waiter.join().unwrap(),
        "a waiting request gets the freed slot"
    );
}

#[test]
fn the_worker_returns_its_value_or_the_reason() {
    let gate = Gate::new(1);
    let wait = Duration::from_millis(50);
    let ok = bounded::run(&gate, wait, Duration::from_secs(2), || Ok(7_u8));
    assert_eq!(ok, Ok(7));
    let err = bounded::run(&gate, wait, Duration::from_secs(2), || {
        Err::<u8, _>(RenderError::Empty)
    });
    assert_eq!(err, Err(RenderError::Empty));
    let panicked = bounded::run(
        &gate,
        wait,
        Duration::from_secs(2),
        || -> Result<u8, RenderError> { panic!("x") },
    );
    assert_eq!(panicked, Err(RenderError::Panicked));
}

// --- the cache -----------------------------------------------------------------------------

#[test]
fn a_file_is_drawn_once_until_it_changes() {
    let dir = tempfile::tempdir().unwrap();
    let path = hwp_file(&dir, "doc.hwp", b"first");
    let preview = state(2, 1000, 100);
    let calls = Arc::new(AtomicUsize::new(0));
    let draw = |calls: &Arc<AtomicUsize>| {
        let calls = Arc::clone(calls);
        move |bytes: Vec<u8>| {
            calls.fetch_add(1, Ordering::SeqCst);
            fake_png(bytes)
        }
    };
    let first = load_preview_with(&preview, "00000000000000f1", &path, draw(&calls)).unwrap();
    let again = load_preview_with(&preview, "00000000000000f1", &path, draw(&calls)).unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "the second ask came from the cache"
    );
    assert!(Arc::ptr_eq(&first, &again));
    fs::write(&path, b"second, longer").unwrap();
    load_preview_with(&preview, "00000000000000f1", &path, draw(&calls)).unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "a changed file is drawn again"
    );
}

#[test]
fn a_failed_render_is_remembered_too() {
    let dir = tempfile::tempdir().unwrap();
    let path = hwp_file(&dir, "bad.hwp", b"data");
    let preview = state(2, 1000, 100);
    let calls = Arc::new(AtomicUsize::new(0));
    for _ in 0..3 {
        let calls = Arc::clone(&calls);
        let found = load_preview_with(&preview, "00000000000000f2", &path, move |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(RenderError::Unreadable)
        });
        assert!(found.is_none());
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn the_cache_holds_a_bounded_number_of_drawn_pages() {
    let dir = tempfile::tempdir().unwrap();
    let path = hwp_file(&dir, "doc.hwp", b"data");
    let preview = state(2, 1000, 100);
    for n in 0..(cache::MAX_CACHED + 5) {
        assert!(load_preview_with(&preview, &format!("{n:016x}"), &path, fake_png).is_some());
    }
    let entries = preview.cache.lock().unwrap().len();
    assert_eq!(entries, cache::MAX_CACHED);
}

// --- timing --------------------------------------------------------------------------------

/// The start window's first screen: ten cards, one after the other as the page asks. Run with
/// `cargo test -p easyoffice-desktop --release measure_ten_cards -- --ignored --nocapture`; set
/// `EASYOFFICE_PREVIEW_DUMP` to a folder to keep the pictures.
#[test]
#[ignore = "timing measurement, not a check"]
fn measure_ten_cards() {
    let dir = tempfile::tempdir().unwrap();
    let names = [
        "biz_plan.hwp",
        "BlogForm_BookReview.hwp",
        "field-01.hwp",
        "footnote-01.hwp",
        "form-002.hwpx",
        "kps-ai.hwp",
        "number-bullet.hwp",
        "oullim-01.hwp",
        "para-head-num-2.hwp",
        "tac-case-001.hwp",
    ];
    let paths: Vec<PathBuf> = names
        .iter()
        .map(|name| without_picture(name, &dir))
        .collect();
    let preview = PreviewState::new();
    let run = |label: &str| {
        let started = Instant::now();
        let mut per_card = Vec::new();
        for (n, path) in paths.iter().enumerate() {
            let card = Instant::now();
            let found = load_preview(&preview, &format!("{n:016x}"), path);
            per_card.push(format!(
                "{}ms/{}KB",
                card.elapsed().as_millis(),
                found.as_ref().map_or(0, |f| f.bytes.len() / 1024)
            ));
            if let (Some(found), Ok(out)) = (found, std::env::var("EASYOFFICE_PREVIEW_DUMP")) {
                fs::write(Path::new(&out).join(format!("{n}.png")), &found.bytes).unwrap();
            }
        }
        println!(
            "{label}: total {}ms [{}]",
            started.elapsed().as_millis(),
            per_card.join(" ")
        );
    };
    run("cold (fonts and layout)");
    run("cached");
}
