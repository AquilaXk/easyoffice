//! The PDF worker process, end to end (PD-170): the shell binary started as
//! `EasyOffice --pdf-edit-worker`, the supervisor that watches it, and files that a reader inside
//! the shell would not survive. Each hostile file must end with the worker killed, a fixed
//! sentence for the caller, and this test process (standing in for the shell and its other
//! windows) untouched and able to do the next request.
//!
//! The wire format, the supervisor and the fixtures are the shell's own source files, included by
//! path, because a binary crate has no library to link.

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

use pdf_edit_budget::{Jobs, MSG_BUSY, MSG_MEMORY, MSG_TIMEOUT, MSG_WORKER_DIED};
use pdf_edit_fixtures::{hand_written_with_xref, hidden_object_stream_bomb, sample_pdf};
use pdf_edit_supervisor::Supervisor;
use pdf_edit_wire::{BuildHead, OverlayKind, PagePlan, WireForm};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

const WORKER: &str = env!("CARGO_BIN_EXE_EasyOffice");
const MIB: u64 = 1_048_576;

fn jobs(max: usize) -> &'static Jobs {
    Box::leak(Box::new(Jobs::new(max)))
}

/// A supervisor with its own places and the given limits.
fn supervisor(deadline: Duration, memory_mib: u64) -> Supervisor {
    Supervisor::with_limits(PathBuf::from(WORKER), jobs(2), deadline, memory_mib * MIB)
}

fn normal() -> Supervisor {
    supervisor(Duration::from_secs(60), 1536)
}

fn plan(count: usize, turn: u16) -> Vec<PagePlan> {
    (0..count)
        .map(|index| PagePlan {
            source: 0,
            index,
            turn,
        })
        .collect()
}

fn head(pages: Vec<PagePlan>, sources: usize) -> BuildHead {
    BuildHead {
        sources,
        pages,
        watermark: None,
        stamp: None,
        markup: None,
        form: WireForm::default(),
    }
}

fn no_overlay(_: f32, _: f32, _: OverlayKind) -> Result<Vec<u8>, String> {
    Err("no overlay expected".to_string())
}

fn pages_of(pdf: &[u8]) -> usize {
    hayro_syntax::Pdf::new(Arc::new(pdf.to_vec()))
        .expect("a PDF")
        .pages()
        .len()
}

// ---- the ordinary work goes through the worker --------------------------------------------------

#[test]
fn a_file_is_inspected_and_built_by_the_worker() {
    let supervisor = normal();
    let source = Arc::new(sample_pdf(1, 4, &[0, 90]));
    assert_eq!(supervisor.inspect(&source), Ok(4));
    let out = supervisor
        .build(
            &[Arc::clone(&source)],
            &head(plan(4, 90), 1),
            &mut no_overlay,
        )
        .expect("built")
        .bytes;
    assert_eq!(pages_of(&out), 4);
    let pdf = hayro_syntax::Pdf::new(Arc::new(out)).unwrap();
    let rotates: Vec<_> = pdf
        .pages()
        .iter()
        .map(|page| format!("{:?}", page.rotation()))
        .collect();
    assert_eq!(
        rotates,
        ["Horizontal", "Flipped", "Horizontal", "Horizontal"]
    );
}

#[test]
fn what_the_result_does_not_keep_comes_back_as_warnings_before_the_answer() {
    let supervisor = normal();
    let page = "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Annots [4 0 R] >>";
    let file = hand_written_with_xref(
        &[
            (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (3, page.to_string()),
            (
                4,
                "<< /Type /Annot /Subtype /Caret /Rect [10 10 50 50] >>".to_string(),
            ),
        ],
        "",
    );
    let built = supervisor
        .build(&[Arc::new(file)], &head(plan(1, 0), 1), &mut no_overlay)
        .expect("built");
    assert_eq!(pages_of(&built.bytes), 1);
    assert_eq!(
        built.warnings,
        ["지원하지 않는 주석 1개는 저장하지 못했습니다 (Caret 1)."]
    );
    let clean = supervisor
        .build(
            &[Arc::new(sample_pdf(1, 1, &[]))],
            &head(plan(1, 0), 1),
            &mut no_overlay,
        )
        .expect("built");
    assert!(clean.warnings.is_empty());
}

#[test]
fn files_are_merged_and_the_overlay_comes_from_the_shell() {
    let supervisor = normal();
    let (first, second) = (
        Arc::new(sample_pdf(1, 2, &[])),
        Arc::new(sample_pdf(2, 1, &[])),
    );
    let pages = vec![
        PagePlan {
            source: 0,
            index: 0,
            turn: 0,
        },
        PagePlan {
            source: 1,
            index: 0,
            turn: 0,
        },
        PagePlan {
            source: 0,
            index: 1,
            turn: 0,
        },
    ];
    let mut asked = Vec::new();
    // The shell answers with a PDF of its own making (here any one-page file).
    let stand_in = sample_pdf(9, 1, &[]);
    let mut overlay = |width: f32, height: f32, kind: OverlayKind| {
        asked.push((width, height, kind));
        Ok(stand_in.clone())
    };
    let mut plan = head(pages, 2);
    plan.watermark = Some("대외비".to_string());
    plan.stamp = Some("승인필".to_string());
    let out = supervisor
        .build(&[first, second], &plan, &mut overlay)
        .expect("built")
        .bytes;
    assert_eq!(pages_of(&out), 3);
    // All three pages are 200 x 300: one overlay for the first page (stamp), one for the others.
    asked.sort_by_key(|a| a.2);
    assert_eq!(
        asked,
        [
            (200.0, 300.0, OverlayKind::Watermark),
            (200.0, 300.0, OverlayKind::FirstPage)
        ]
    );
}

#[test]
fn an_overlay_the_shell_cannot_make_ends_the_build_with_its_sentence() {
    let supervisor = normal();
    let source = Arc::new(sample_pdf(1, 1, &[]));
    let mut plan = head(plan(1, 0), 1);
    plan.watermark = Some("x".to_string());
    let err = supervisor
        .build(&[source], &plan, &mut |_, _, _| {
            Err("글꼴을 찾지 못했습니다.".to_string())
        })
        .unwrap_err();
    assert_eq!(err, "글꼴을 찾지 못했습니다.");
}

#[test]
fn an_overlay_maker_that_panics_ends_the_worker_and_leaves_the_shell_standing() {
    let places = jobs(1);
    let supervisor = Supervisor::with_limits(
        PathBuf::from(WORKER),
        places,
        Duration::from_secs(60),
        1536 * MIB,
    );
    let source = Arc::new(sample_pdf(1, 1, &[]));
    let mut plan = head(plan(1, 0), 1);
    plan.watermark = Some("x".to_string());
    let started = Instant::now();
    let err = supervisor
        .build(&[Arc::clone(&source)], &plan, &mut |_, _, _| {
            panic!("the font code broke")
        })
        .unwrap_err();
    assert_eq!(err, pdf_edit_budget::MSG_PANIC);
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "no wait for the worker's own deadline"
    );
    // The single place is back and no worker is left running behind the panic.
    assert_eq!(supervisor.inspect(&source), Ok(1));
}

#[test]
fn a_file_the_checks_refuse_comes_back_as_the_checks_sentence() {
    let supervisor = normal();
    let err = supervisor
        .inspect(&Arc::new(b"not a pdf at all".to_vec()))
        .unwrap_err();
    assert_eq!(err, "PDF 파일이 아니거나 읽을 수 없는 파일입니다.");
}

// ---- hostile files: the worker is killed, the shell is not --------------------------------------

/// 5,000 pages that all inherit one resource dictionary of 100,000 entries: 2 MB, and it passes
/// every count, but the reader builds the dictionary again for every page.
fn shared_resources_file(pages: usize, entries: usize) -> Vec<u8> {
    let xobjects: String = (0..entries).map(|n| format!("/X{n} 4 0 R ")).collect();
    let kids: String = (0..pages).map(|n| format!("{} 0 R ", n + 5)).collect();
    let mut objects = vec![
        (1u32, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
        (
            2,
            format!("<< /Type /Pages /Kids [{kids}] /Count {pages} /Resources 3 0 R >>"),
        ),
        (3, format!("<< /XObject << {xobjects}>> >>")),
        (
            4,
            "<< /Type /XObject /Subtype /Form /BBox [0 0 1 1] >>".to_string(),
        ),
    ];
    for n in 0..pages {
        objects.push((
            n as u32 + 5,
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] >>".to_string(),
        ));
    }
    hand_written_with_xref(&objects, "")
}

#[test]
fn a_file_that_makes_the_reader_allocate_without_end_costs_the_worker_and_nothing_else() {
    let file = Arc::new(shared_resources_file(5000, 100_000));
    assert!(file.len() < 3 * MIB as usize, "{} bytes", file.len());
    let supervisor = supervisor(Duration::from_secs(60), 300);
    let started = Instant::now();
    let err = supervisor.inspect(&file).unwrap_err();
    // Where the shell measures memory (macOS, Linux) it is the memory sentence; elsewhere the
    // kernel's limit may end the worker first.
    if cfg!(any(target_os = "macos", target_os = "linux")) {
        assert_eq!(err, MSG_MEMORY);
    } else {
        assert!(err == MSG_MEMORY || err == MSG_WORKER_DIED, "{err}");
    }
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "{:?}",
        started.elapsed()
    );
    // This process is alive and the next request works: a window is not tied to the one that failed.
    assert_eq!(supervisor.inspect(&Arc::new(sample_pdf(1, 3, &[]))), Ok(3));
}

#[test]
fn a_request_past_the_deadline_is_killed_at_the_deadline() {
    let file = Arc::new(shared_resources_file(2000, 20_000));
    let supervisor = supervisor(Duration::from_millis(300), 4096);
    let started = Instant::now();
    let err = supervisor.inspect(&file).unwrap_err();
    assert_eq!(err, MSG_TIMEOUT);
    let took = started.elapsed();
    assert!(
        took >= Duration::from_millis(300) && took < Duration::from_secs(5),
        "{took:?}"
    );
}

#[test]
fn a_killed_worker_gives_its_place_back_at_once() {
    let places = jobs(1);
    let slow_one = Supervisor::with_limits(
        PathBuf::from(WORKER),
        places,
        Duration::from_millis(300),
        4096 * MIB,
    );
    let next = Supervisor::with_limits(
        PathBuf::from(WORKER),
        places,
        Duration::from_secs(60),
        4096 * MIB,
    );
    let slow = Arc::new(shared_resources_file(2000, 20_000));
    assert_eq!(slow_one.inspect(&slow).unwrap_err(), MSG_TIMEOUT);
    // No thread is left running behind the deadline: the one place is free again.
    assert_eq!(next.inspect(&Arc::new(sample_pdf(1, 1, &[]))), Ok(1));
}

#[test]
fn requests_past_the_number_of_places_are_refused_while_the_first_runs() {
    let places = jobs(1);
    let first = Supervisor::with_limits(
        PathBuf::from(WORKER),
        places,
        Duration::from_secs(3),
        4096 * MIB,
    );
    let second = Supervisor::with_limits(
        PathBuf::from(WORKER),
        places,
        Duration::from_secs(60),
        4096 * MIB,
    );
    let slow = Arc::new(shared_resources_file(2000, 20_000));
    let (ran, refused) = std::thread::scope(|scope| {
        let handle = scope.spawn(|| first.inspect(&slow));
        std::thread::sleep(Duration::from_millis(500));
        let refused = second.inspect(&Arc::new(sample_pdf(1, 1, &[])));
        (handle.join().unwrap(), refused)
    });
    assert_eq!(refused.unwrap_err(), MSG_BUSY);
    assert_eq!(ran.unwrap_err(), MSG_TIMEOUT);
}

#[test]
fn a_worker_that_is_not_there_is_a_sentence_not_a_panic() {
    let missing = Supervisor::with_limits(
        PathBuf::from("/nonexistent/easyoffice-worker"),
        jobs(2),
        Duration::from_secs(5),
        MIB * 1024,
    );
    assert_eq!(
        missing
            .inspect(&Arc::new(sample_pdf(1, 1, &[])))
            .unwrap_err(),
        MSG_WORKER_DIED
    );
}

#[test]
fn a_bomb_the_scan_cannot_see_is_stopped_by_the_memory_limit() {
    let file = Arc::new(hidden_object_stream_bomb(25));
    assert!(file.len() < MIB as usize, "{} bytes", file.len());
    // The scan before the parse lets it through (within the 256 MiB scan cap); only the
    // worker's limit stands between it and 200 MiB.
    let supervisor = supervisor(Duration::from_secs(60), 150);
    let started = Instant::now();
    let err = supervisor.inspect(&file).unwrap_err();
    // Where the shell measures memory (macOS, Linux) it is the memory sentence; elsewhere the
    // kernel's limit may end the worker first.
    if cfg!(any(target_os = "macos", target_os = "linux")) {
        assert_eq!(err, MSG_MEMORY);
    } else {
        assert!(err == MSG_MEMORY || err == MSG_WORKER_DIED, "{err}");
    }
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(
        supervisor.inspect(&Arc::new(sample_pdf(1, 2, &[]))),
        Ok(2),
        "the shell goes on"
    );
}

#[test]
fn pages_that_share_a_huge_dictionary_are_refused_by_the_size_the_copy_would_have() {
    // 2,000 pages x 5,000 entries: the library writes every entry again for every page, so this
    // 0.2 MB file would become hundreds of MB. The worker counts that before it copies anything.
    let file = Arc::new(shared_resources_file(2000, 5000));
    assert!(file.len() < MIB as usize, "{} bytes", file.len());
    let started = Instant::now();
    let err = normal().inspect(&file).unwrap_err();
    assert!(err.contains("리소스"), "{err}");
    assert!(
        started.elapsed() < Duration::from_secs(60),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn a_contents_array_of_two_hundred_thousand_items_is_refused_by_its_length() {
    let items: String = (0..200_000).map(|_| "4 0 R ").collect();
    let file = hand_written_with_xref(
        &[
            (1, "<< /Type /Catalog /Pages 2 0 R >>".to_string()),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (
                3,
                format!(
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] /Contents [{items}] >>"
                ),
            ),
            (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
        ],
        "",
    );
    let err = normal().inspect(&Arc::new(file)).unwrap_err();
    assert!(err.contains("내용 스트림"), "{err}");
}

#[test]
fn tagged_and_signed_pdf_incremental_save_survives_worker_isolation() {
    let supervisor = normal();
    let orig = Arc::new(hand_written_with_xref(
        &[
            (
                1,
                "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R /AcroForm << /Fields [6 0 R] /SigFlags 3 >> >>"
                    .to_string(),
            ),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (
                3,
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Contents 4 0 R /Annots [6 0 R] >>"
                    .to_string(),
            ),
            (4, "<< /Length 0 >>\nstream\n\nendstream".to_string()),
            (5, "<< /Type /StructTreeRoot >>".to_string()),
            (
                6,
                "<< /Type /Annot /Subtype /Widget /FT /Sig /T (Sig1) /Rect [10 10 90 40] /V << /Type /Sig /ByteRange [0 1 2 3] >> >>"
                    .to_string(),
            ),
        ],
        "",
    ));
    assert_eq!(supervisor.inspect(&orig), Ok(1));
    let built = supervisor
        .build(&[Arc::clone(&orig)], &head(plan(1, 90), 1), &mut no_overlay)
        .expect("worker incremental build succeeds");
    assert_eq!(&built.bytes[..orig.len()], orig.as_slice());
    let pdf = hayro_syntax::Pdf::new(Arc::new(built.bytes)).expect("result opens");
    let catalog = pdf
        .xref()
        .get::<hayro_syntax::object::Dict<'_>>(pdf.xref().root_id())
        .unwrap();
    assert!(catalog
        .get::<hayro_syntax::object::Dict<'_>>(b"StructTreeRoot")
        .is_some());
    assert!(catalog
        .get::<hayro_syntax::object::Dict<'_>>(b"AcroForm")
        .is_some());
    assert!(built.warnings.iter().any(|w| w.contains("전자서명")));
}

#[test]
fn hostile_circular_prev_falls_back_cleanly_in_worker() {
    let supervisor = normal();
    let base = hand_written_with_xref(
        &[
            (
                1,
                "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 4 0 R >>".to_string(),
            ),
            (2, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string()),
            (
                3,
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] >>".to_string(),
            ),
            (4, "<< /Type /StructTreeRoot >>".to_string()),
        ],
        "",
    );
    let xref_off = base.windows(4).rposition(|w| w == b"xref").unwrap();
    let sxref_off = base.windows(9).rposition(|w| w == b"startxref").unwrap();
    let end_dict = base[..sxref_off]
        .windows(2)
        .rposition(|w| w == b">>")
        .unwrap();
    let mut circ = base[..end_dict].to_vec();
    circ.extend_from_slice(
        format!(" /Prev {xref_off} >>\nstartxref\n{xref_off}\n%%EOF\n").as_bytes(),
    );
    let file = Arc::new(circ);
    let built = supervisor
        .build(&[file], &head(plan(1, 0), 1), &mut no_overlay)
        .expect("fallback rewrite succeeds in worker");
    assert_eq!(pages_of(&built.bytes), 1);
    assert!(!built.warnings.is_empty());
}
