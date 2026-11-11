//! The first page of a PDF for a start window card, drawn by the PDF worker process (PD-179), end
//! to end: the shell binary as `EasyOffice --pdf-edit-worker`, the supervisor that watches it and
//! files that a renderer inside the shell would not survive. A hostile file must end with a
//! sentence and no picture, inside the deadline, and the process standing in for the shell must
//! go on with the next card.

#![allow(dead_code)]

#[path = "../src/pdf_edit_budget.rs"]
mod pdf_edit_budget;
#[path = "../src/pdf_edit_fixtures.rs"]
mod pdf_edit_fixtures;
#[path = "../src/pdf_edit_os.rs"]
mod pdf_edit_os;
#[path = "../src/pdf_edit_supervisor.rs"]
mod pdf_edit_supervisor;
#[path = "../src/pdf_edit_wire.rs"]
mod pdf_edit_wire;

use flate2::write::ZlibEncoder;
use flate2::Compression;
use pdf_edit_budget::{Jobs, MSG_MEMORY, MSG_TIMEOUT};
use pdf_edit_fixtures::{hand_written_with_xref, sample_pdf};
use pdf_edit_supervisor::Supervisor;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const WORKER: &str = env!("CARGO_BIN_EXE_EasyOffice");
const MIB: u64 = 1_048_576;
const CARD_WIDTH: u32 = 480;
const CARD_HEIGHT: u32 = 360;
const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

fn supervisor(deadline: Duration, memory_mib: u64) -> Supervisor {
    let jobs: &'static Jobs = Box::leak(Box::new(Jobs::new(2)));
    Supervisor::with_limits(PathBuf::from(WORKER), jobs, deadline, memory_mib * MIB)
}

/// The shell's own limits for a card.
fn card() -> Supervisor {
    supervisor(
        pdf_edit_budget::PREVIEW_DEADLINE,
        pdf_edit_budget::PREVIEW_MAX_RSS_BYTES / MIB,
    )
}

fn png_size(png: &[u8]) -> (u32, u32) {
    assert!(png.starts_with(&PNG_MAGIC), "not a PNG");
    let at = |i: usize| u32::from_be_bytes(png[i..i + 4].try_into().unwrap());
    (at(16), at(20))
}

/// How many pixels of the card are not white.
fn non_white_pixels(png: &[u8]) -> usize {
    let pixmap = tiny_skia::Pixmap::decode_png(png).expect("a PNG");
    pixmap
        .pixels()
        .iter()
        .filter(|p| (p.red(), p.green(), p.blue()) != (255, 255, 255))
        .count()
}

fn flate(data: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

/// A one-page file around `page_extra` (page dictionary entries) with the given extra objects
/// (numbers from 5).
fn one_page(page_extra: &str, extra: &[(u32, String)]) -> Vec<u8> {
    let mut objects = vec![
        (1u32, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
        (3, format!("<< /Type /Page /Parent 2 0 R {page_extra} >>")),
        (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
    ];
    objects.extend(extra.iter().cloned());
    hand_written_with_xref(&objects, "")
}

fn drawn(file: Vec<u8>) -> Vec<u8> {
    card()
        .render_first_page(&Arc::new(file), &[])
        .expect("a drawn page")
}

#[test]
fn the_first_page_is_drawn_at_the_card_size() {
    let first = drawn(sample_pdf(1, 3, &[]));
    assert_eq!(png_size(&first), (CARD_WIDTH, CARD_HEIGHT));
    let blank = drawn(one_page("/MediaBox [0 0 200 300]", &[]));
    assert_eq!(png_size(&blank), (CARD_WIDTH, CARD_HEIGHT));
    assert_eq!(
        non_white_pixels(&blank),
        0,
        "a page without content is white"
    );
    assert!(
        non_white_pixels(&first) > 10_000,
        "the picture of the page is on the card"
    );
    assert_ne!(
        first,
        drawn(sample_pdf(2, 3, &[])),
        "another page, another picture"
    );
    assert!(first.len() < 1_048_576);
}

/// The path of the bundled Korean font of the rhwp checkout (`node scripts/checkout-rhwp.mjs`).
fn korean_font() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../third_party/rhwp/ttfs/opensource/NotoSansKR-Regular.ttf");
    std::fs::read(&path).unwrap_or_else(|_| {
        panic!(
            "{} is missing; run `node scripts/checkout-rhwp.mjs`",
            path.display()
        )
    })
}

/// One page of Hangul text in a font the file names but does not embed (what the shell's own
/// text PDFs do).
fn unembedded_korean_page() -> Vec<u8> {
    one_page(
        "/MediaBox [0 0 300 200] /Contents 5 0 R /Resources << /Font << /F1 6 0 R >> >>",
        &[
            (
                5,
                "<< /Length 64 >>\nstream\nBT /F1 36 Tf 20 120 Td <AC00B098B2E4B77CB9C8BC14> Tj ET\nendstream"
                    .to_string(),
            ),
            (
                6,
                "<< /Type /Font /Subtype /Type0 /BaseFont /HYGoThic-Medium-UniKS-UCS2-H /Encoding /UniKS-UCS2-H /DescendantFonts [7 0 R] >>"
                    .to_string(),
            ),
            (
                7,
                "<< /Type /Font /Subtype /CIDFontType0 /BaseFont /HYGoThic-Medium /CIDSystemInfo << /Registry (Adobe) /Ordering (Korea1) /Supplement 1 >> /FontDescriptor 8 0 R /DW 1000 >>"
                    .to_string(),
            ),
            (
                8,
                "<< /Type /FontDescriptor /FontName /HYGoThic-Medium /Flags 4 /FontBBox [0 -200 1000 900] /ItalicAngle 0 /Ascent 880 /Descent -120 /CapHeight 880 /StemV 80 >>"
                    .to_string(),
            ),
        ],
    )
}

#[test]
fn korean_text_the_file_does_not_embed_is_drawn_with_the_font_the_shell_sends() {
    let file = Arc::new(unembedded_korean_page());
    let without = card().render_first_page(&file, &[]).expect("a page");
    let with = card()
        .render_first_page(&file, &korean_font())
        .expect("a page");
    assert_eq!(non_white_pixels(&without), 0, "no font, no glyphs");
    assert!(non_white_pixels(&with) > 1_000, "the text is there");
    // A font file that is no font is no harm.
    let junk = card()
        .render_first_page(&file, &[0x42; 4096])
        .expect("a page");
    assert_eq!(non_white_pixels(&junk), 0);
}

#[test]
fn only_the_first_page_is_drawn_and_a_turned_page_is_drawn_upright() {
    assert_eq!(
        drawn(sample_pdf(1, 3, &[])),
        drawn(sample_pdf(1, 40, &[])),
        "the other pages do not change the card"
    );
    let upright = drawn(sample_pdf(1, 1, &[0]));
    let turned = drawn(sample_pdf(1, 1, &[90]));
    assert_ne!(upright, turned);
    assert_eq!(png_size(&turned), (CARD_WIDTH, CARD_HEIGHT));
}

#[test]
fn a_page_of_any_declared_size_gives_the_same_card_size() {
    for media_box in [
        "[0 0 100000 100000]",
        "[0 0 1000000000 1000000000]",
        "[0 0 20 800000]",
        "[0 0 612 792]",
        "[0 0 900 20]",
    ] {
        let png = drawn(one_page(&format!("/MediaBox {media_box}"), &[]));
        assert_eq!(png_size(&png), (CARD_WIDTH, CARD_HEIGHT), "{media_box}");
    }
}

const ENCRYPT_ID: &str =
    "/ID [<00112233445566778899aabbccddeeff> <00112233445566778899aabbccddeeff>]";

fn encrypted_file() -> Vec<u8> {
    hand_written_with_xref(
        &[
            (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (
                3,
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>".to_string(),
            ),
            (
                4,
                "<< /Filter /Standard /V 1 /R 2 /O (aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa) /U (bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb) /P -4 >>"
                    .to_string(),
            ),
        ],
        &format!("/Encrypt 4 0 R {ENCRYPT_ID}"),
    )
}

#[test]
fn files_that_are_no_page_to_draw_end_with_a_sentence() {
    let no_pages = hand_written_with_xref(
        &[
            (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
            (2, "<< /Type /Pages /Kids [] /Count 0 >>".to_string()),
        ],
        "",
    );
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("empty", Vec::new()),
        ("text", b"not a pdf at all".to_vec()),
        ("cut", sample_pdf(1, 2, &[])[..300].to_vec()),
        ("no pages", no_pages),
        ("too narrow", one_page("/MediaBox [0 0 0.001 500]", &[])),
        ("encrypted", encrypted_file()),
    ];
    for (name, file) in cases {
        let started = Instant::now();
        let outcome = card().render_first_page(&Arc::new(file), &[]);
        assert!(outcome.is_err(), "{name} was drawn");
        assert!(started.elapsed() < Duration::from_secs(8), "{name}");
    }
}

#[test]
fn damaged_bytes_and_unreadable_sizes_are_refused_or_drawn_never_a_crash() {
    let supervisor = card();
    for seed in 0..24usize {
        let mut damaged = sample_pdf(1, 2, &[]);
        for k in 0..8 {
            let at = (seed * 131 + k * 977) % damaged.len();
            damaged[at] ^= 0x5a + u8::try_from(k).unwrap();
        }
        let started = Instant::now();
        let _ = supervisor.render_first_page(&Arc::new(damaged), &[]);
        assert!(started.elapsed() < Duration::from_secs(10), "seed {seed}");
    }
    // The reader falls back to a default page for a size it cannot read: a blank card.
    for media_box in ["[0 0 0 0]", "[0 0 (a) (b)]", "[0 0 500 0.001]"] {
        let png = drawn(one_page(&format!("/MediaBox {media_box}"), &[]));
        assert_eq!(png_size(&png), (CARD_WIDTH, CARD_HEIGHT), "{media_box}");
    }
    assert!(supervisor
        .render_first_page(&Arc::new(sample_pdf(1, 1, &[])), &[])
        .is_ok());
}

#[test]
fn a_file_over_the_size_the_pdf_window_opens_is_refused() {
    let mut big = sample_pdf(1, 1, &[]);
    big.resize(10 * MIB as usize + 1, b' ');
    let err = card().render_first_page(&Arc::new(big), &[]).unwrap_err();
    assert!(err.contains("허용 크기"), "{err}");
}

/// Two content streams that each inflate to 200 MiB of spaces: 100 MiB past what a page's content
/// may be, from a file of 400 KiB.
#[test]
fn a_page_that_inflates_past_its_budget_is_refused_before_it_is_drawn() {
    let zeros = flate(&vec![b' '; 200 << 20]);
    let mut file = b"%PDF-1.7\n".to_vec();
    let mut objects: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents [4 0 R 5 0 R] >>".into(),
    ];
    for _ in 0..2 {
        objects.push(format!(
            "<< /Filter /FlateDecode /Length {} >>\nstream\n",
            zeros.len()
        ));
    }
    for (n, body) in objects.iter().enumerate() {
        file.extend_from_slice(format!("{} 0 obj\n{body}", n + 1).as_bytes());
        if n >= 3 {
            file.extend_from_slice(&zeros);
            file.extend_from_slice(b"\nendstream");
        }
        file.extend_from_slice(b"\nendobj\n");
    }
    file.extend_from_slice(b"trailer\n<< /Root 1 0 R >>\n%%EOF\n");
    let started = Instant::now();
    assert!(card().render_first_page(&Arc::new(file), &[]).is_err());
    assert!(started.elapsed() < Duration::from_secs(8));
}

/// A form that draws itself: the renderer must stop (or the worker is killed at the deadline).
#[test]
fn a_form_that_draws_itself_ends_inside_the_deadline() {
    let file = one_page(
        "/MediaBox [0 0 200 200] /Contents 5 0 R /Resources << /XObject << /F 6 0 R >> >>",
        &[
            (5, "<< /Length 5 >>\nstream\n/F Do\nendstream".to_string()),
            (
                6,
                "<< /Type /XObject /Subtype /Form /BBox [0 0 200 200] /Resources << /XObject << /F 6 0 R >> >> /Length 13 >>\nstream\n/F Do /F Do\nendstream"
                    .to_string(),
            ),
        ],
    );
    let supervisor = supervisor(Duration::from_secs(4), 1024);
    let started = Instant::now();
    let outcome = supervisor.render_first_page(&Arc::new(file), &[]);
    assert!(
        started.elapsed() < Duration::from_secs(6),
        "{:?}",
        started.elapsed()
    );
    if let Err(err) = outcome {
        assert!(
            err == MSG_TIMEOUT || err == MSG_MEMORY || err.contains("PDF"),
            "{err}"
        );
    }
    assert!(
        supervisor
            .render_first_page(&Arc::new(sample_pdf(1, 1, &[])), &[])
            .is_ok(),
        "the next card is drawn"
    );
}

/// A picture of 7,000 x 7,000 RGB pixels (147 MB) that is 140 KB in the file: it passes the
/// size counts (under 256 MiB), so what stops a worse one is the memory limit of the worker.
fn picture_file(side: usize) -> Vec<u8> {
    let data = flate(&vec![0x80_u8; side * side * 3]);
    let mut file = b"%PDF-1.7\n".to_vec();
    let head = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Contents 4 0 R /Resources << /XObject << /Im 5 0 R >> >> >>".to_string(),
        "<< /Length 31 >>\nstream\nq 300 0 0 300 0 0 cm /Im Do Q\nendstream".to_string(),
    ];
    for (n, body) in head.iter().enumerate() {
        file.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", n + 1).as_bytes());
    }
    file.extend_from_slice(
        format!(
            "5 0 obj\n<< /Type /XObject /Subtype /Image /Width {side} /Height {side} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
            data.len()
        )
        .as_bytes(),
    );
    file.extend_from_slice(&data);
    file.extend_from_slice(b"\nendstream\nendobj\ntrailer\n<< /Root 1 0 R >>\n%%EOF\n");
    file
}

#[test]
fn a_large_picture_is_drawn_or_stopped_by_the_limits_and_the_shell_goes_on() {
    let file = Arc::new(picture_file(7000));
    assert!(file.len() < MIB as usize, "{} bytes", file.len());
    let started = Instant::now();
    let supervisor = card();
    match supervisor.render_first_page(&file, &[]) {
        Ok(png) => assert_eq!(png_size(&png), (CARD_WIDTH, CARD_HEIGHT)),
        Err(err) => assert!(
            err == MSG_MEMORY || err == MSG_TIMEOUT || err.contains("PDF"),
            "{err}"
        ),
    }
    assert!(started.elapsed() < Duration::from_secs(20));
    // With a limit far below what the picture needs, the worker is ended and says so.
    let tight = supervisor_with_memory(64);
    if cfg!(any(target_os = "macos", target_os = "linux")) {
        assert_eq!(tight.render_first_page(&file, &[]).unwrap_err(), MSG_MEMORY);
    }
    assert!(supervisor
        .render_first_page(&Arc::new(sample_pdf(1, 1, &[])), &[])
        .is_ok());
}

fn supervisor_with_memory(memory_mib: u64) -> Supervisor {
    supervisor(Duration::from_secs(30), memory_mib)
}

/// 5,000 pages that inherit one resource dictionary of 100,000 entries (2 MB): the gate counts it
/// before the first page is touched.
#[test]
fn a_shared_resource_bomb_costs_the_worker_and_nothing_else() {
    let entries: String = (0..100_000).map(|n| format!("/X{n} 4 0 R ")).collect();
    let kids: String = (0..5000).map(|n| format!("{} 0 R ", n + 5)).collect();
    let mut objects = vec![
        (1u32, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (
            2,
            format!("<< /Type /Pages /Kids [{kids}] /Count 5000 /Resources 3 0 R >>"),
        ),
        (3, format!("<< /XObject << {entries}>> >>")),
        (
            4,
            "<< /Type /XObject /Subtype /Form /BBox [0 0 1 1] >>".to_string(),
        ),
    ];
    for n in 0..5000u32 {
        objects.push((
            n + 5,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] >>".to_string(),
        ));
    }
    let file = Arc::new(hand_written_with_xref(&objects, ""));
    let supervisor = card();
    let started = Instant::now();
    assert!(supervisor.render_first_page(&file, &[]).is_err());
    assert!(started.elapsed() < Duration::from_secs(20));
    assert!(supervisor
        .render_first_page(&Arc::new(sample_pdf(1, 1, &[])), &[])
        .is_ok());
}

#[test]
fn a_worker_that_is_not_there_gives_a_sentence() {
    let jobs: &'static Jobs = Box::leak(Box::new(Jobs::new(1)));
    let missing = Supervisor::with_limits(
        PathBuf::from("/nonexistent/easyoffice-worker"),
        jobs,
        Duration::from_secs(5),
        1024 * MIB,
    );
    assert!(missing
        .render_first_page(&Arc::new(sample_pdf(1, 1, &[])), &[])
        .is_err());
}

/// Saves the pictures of the samples for a look and times the PDFs named in
/// `EASYOFFICE_PREVIEW_PDFS` (paths separated by `:`): the worker process starts for each one, as it
/// does for a card. `EASYOFFICE_PREVIEW_DUMP=<folder> cargo test --release --test pdf_preview dump`.
#[test]
fn dump_for_a_look() {
    let Ok(folder) = std::env::var("EASYOFFICE_PREVIEW_DUMP") else {
        return;
    };
    let folder = PathBuf::from(folder);
    std::fs::write(folder.join("pdf-sample.png"), drawn(sample_pdf(1, 3, &[]))).unwrap();
    for path in std::env::var("EASYOFFICE_PREVIEW_PDFS")
        .unwrap_or_default()
        .split(':')
        .filter(|path| !path.is_empty())
    {
        let pdf = std::fs::read(path).unwrap();
        let font = korean_font();
        for round in ["first", "second"] {
            let started = Instant::now();
            let png = card().render_first_page(&Arc::new(pdf.clone()), &font);
            println!(
                "{path} {} bytes  {round}={:?}  png={:?} bytes",
                pdf.len(),
                started.elapsed(),
                png.as_ref().map(Vec::len)
            );
            if let (Ok(png), "second") = (png, round) {
                let name = PathBuf::from(path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                std::fs::write(folder.join(format!("sample-{name}.png")), png).unwrap();
            }
        }
    }
}
