//! Tests of the commands that edit the PDF of a window (PD-170, PD-245, PD-266).

use super::*;
use crate::pdf_edit_engine::InProcessEngine;
use crate::pdf_edit_fixtures::{deadline, read, sample_pdf};
use crate::pdf_edit_overlay::Overlay;
use crate::pdf_edit_wire::PagePlan;
use usvg::fontdb::Database;

fn engine() -> InProcessEngine {
    InProcessEngine::new(Arc::new(Database::new()))
}

fn session(pages: usize) -> EditSession {
    EditSession::open(sample_pdf(1, pages, &[]), &engine())
        .unwrap()
        .with_stem("보고서")
}

fn scratch_dir(tag: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("easyoffice-pdf-edit-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_table_keeps_one_session_per_window_and_refuses_stale_commits() {
    let edits = PdfEdits::default();
    assert_eq!(edits.snapshot("doc-1").unwrap_err(), MSG_NO_SESSION);
    edits.begin("doc-1", session(3));
    edits.begin("doc-2", session(2));
    let (taken, version) = edits.snapshot("doc-1").unwrap();
    assert_eq!(taken.page_count(), 3);
    // A change that arrives while a long operation works on `taken` ...
    edits.update("doc-1", |s| s.rotate(None, 90)).unwrap();
    // ... makes the operation's result stale.
    assert_eq!(
        edits.commit("doc-1", version, taken.clone()).unwrap_err(),
        MSG_CHANGED
    );
    let (_, now) = edits.snapshot("doc-1").unwrap();
    edits.commit("doc-1", now, taken).unwrap();
    assert_eq!(edits.snapshot("doc-2").unwrap().0.page_count(), 2);
    edits.forget("doc-1");
    assert_eq!(
        edits.commit("doc-1", now, session(1)).unwrap_err(),
        MSG_NO_SESSION
    );
}

#[test]
fn an_edit_that_fails_leaves_the_session_as_it_was() {
    let edits = PdfEdits::default();
    edits.begin("doc-1", session(3));
    assert!(edits.update("doc-1", |s| s.rotate(None, 45)).is_err());
    assert!(edits.update("doc-1", |s| s.rotate(Some("9"), 90)).is_err());
    assert_eq!(edits.snapshot("doc-1").unwrap().0.first_turn(), 0);
}

#[test]
fn ops_are_read_from_the_page_and_nothing_else_is_accepted() {
    let rotate: PdfEditOp = serde_json::from_str(r#"{"kind":"rotate","degrees":90}"#).unwrap();
    assert_eq!(
        rotate,
        PdfEditOp::Rotate {
            degrees: 90,
            pages: None
        }
    );
    let ranged: PdfEditOp =
        serde_json::from_str(r#"{"kind":"rotate","degrees":-90,"pages":"2-3"}"#).unwrap();
    assert_eq!(
        ranged,
        PdfEditOp::Rotate {
            degrees: -90,
            pages: Some("2-3".into())
        }
    );
    let overlay: PdfEditOp =
        serde_json::from_str(r#"{"kind":"overlay","stamp":"승인필","watermark":null}"#).unwrap();
    assert_eq!(
        overlay,
        PdfEditOp::Overlay {
            stamp: Some("승인필".into()),
            watermark: None
        }
    );
    for bad in [
        r#"{"kind":"delete"}"#,
        r#"{"kind":"rotate"}"#,
        r#"{"kind":"rotate","degrees":90,"path":"/etc/passwd"}"#,
        r#"{"kind":"overlay","stamp":1}"#,
        r#"{"degrees":90}"#,
        r#""rotate""#,
    ] {
        assert!(serde_json::from_str::<PdfEditOp>(bad).is_err(), "{bad}");
    }
}

#[test]
fn applying_an_op_changes_the_state_the_page_sees() {
    let mut s = session(3);
    apply(
        &mut s,
        PdfEditOp::Rotate {
            degrees: 90,
            pages: None,
        },
    )
    .unwrap();
    apply(
        &mut s,
        PdfEditOp::Overlay {
            stamp: Some(" 승인필 ".into()),
            watermark: Some(String::new()),
        },
    )
    .unwrap();
    let state = PdfEditState::of(&s);
    assert_eq!(
        serde_json::to_value(&state).unwrap(),
        serde_json::json!({
            "pages": 3, "sources": 1, "turn": 90, "stamp": "승인필", "watermark": null
        })
    );
    let too_long = PdfEditOp::Overlay {
        stamp: Some("가".repeat(101)),
        watermark: None,
    };
    assert!(apply(&mut s, too_long).is_err());
}

#[test]
fn names_of_the_results_are_safe_and_say_which_pages() {
    assert_eq!(file_stem("보고서.pdf"), "보고서");
    assert_eq!(file_stem("a.b.pdf"), "a.b");
    assert_eq!(file_stem("a/b:c?.pdf"), "a_b_c_");
    assert_eq!(file_stem(".pdf"), "문서");
    assert_eq!(file_stem(&"가".repeat(200)).chars().count(), MAX_STEM_CHARS);
    assert_eq!(split_file_name_of("보고서", &(4..5)), "보고서_5.pdf");
    assert_eq!(split_file_name_of("보고서", &(0..3)), "보고서_1-3.pdf");
}

#[test]
fn the_modes_of_a_split_or_merge_are_the_three_the_dialog_offers() {
    for (name, mode) in [
        ("merge", SplitMergeMode::Merge),
        ("range", SplitMergeMode::Range),
        ("each", SplitMergeMode::Each),
    ] {
        let parsed: SplitMergeMode = serde_json::from_value(serde_json::json!(name)).unwrap();
        assert_eq!(parsed, mode);
    }
    for bad in ["", "Merge", "split", "ask", "merge ", "range,each"] {
        assert!(serde_json::from_value::<SplitMergeMode>(serde_json::json!(bad)).is_err());
    }
}

#[test]
fn opening_takes_base64_within_the_size_limit_and_refuses_the_rest() {
    let pdf = sample_pdf(1, 2, &[]);
    let base64 = base64_of(&pdf);
    let opened = open_session(&base64, "보고서", &engine()).unwrap();
    assert_eq!((opened.page_count(), opened.stem()), (2, "보고서"));
    assert_eq!(
        open_session("!!!not base64!!!", "x", &engine())
            .err()
            .as_deref(),
        Some(MSG_BASE64)
    );
    let huge = "A".repeat(MAX_SOURCE_BYTES / 3 * 4 + 400);
    assert_eq!(
        open_session(&huge, "x", &engine()).err(),
        Some(too_big_message())
    );
    let not_pdf = base64_of(b"hello world, this is not a pdf");
    assert!(open_session(&not_pdf, "x", &engine()).is_err());
}

fn base64_of(bytes: &[u8]) -> String {
    EasyOfficeIpcRouter::encode_base64(bytes)
}

#[test]
fn a_split_writes_one_file_per_range_and_never_overwrites() {
    let dir = scratch_dir("split");
    let s = session(3);
    let ranges = s.split_ranges(None).unwrap();
    assert_eq!(
        write_split(&s, &ranges, &dir, &engine(), &deadline()),
        Ok(SplitWritten {
            files: 3,
            warnings: Vec::new()
        })
    );
    for n in 1..=3 {
        let bytes = std::fs::read(dir.join(format!("보고서_{n}.pdf"))).unwrap();
        assert_eq!(read(&bytes).pages().len(), 1);
    }
    // A second run finds the names taken and writes nothing more.
    let err = write_split(&s, &ranges, &dir, &engine(), &deadline()).unwrap_err();
    assert!(err.contains("같은 이름"), "{err}");
    assert_eq!(
        std::fs::read_dir(&dir).unwrap().count(),
        3,
        "no temporary files either"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_split_that_fails_halfway_removes_what_it_wrote() {
    let dir = scratch_dir("split-fail");
    let s = session(4);
    let ranges = s.split_ranges(None).unwrap();
    // The deadline passes after the first file: the rest never starts and the first goes away.
    let tight = Deadline::after(std::time::Duration::from_millis(1));
    std::thread::sleep(std::time::Duration::from_millis(5));
    assert!(write_split(&s, &ranges, &dir, &engine(), &tight).is_err());
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn two_ranges_that_give_one_name_write_nothing() {
    let dir = scratch_dir("split-twice");
    let s = session(3);
    let ranges = s.split_ranges(Some("1, 1")).unwrap();
    let err = write_split(&s, &ranges, &dir, &engine(), &deadline()).unwrap_err();
    assert!(err.contains("두 번"), "{err}");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// An engine that, while it builds the file of one range, lets somebody else create that name.
struct Racing {
    inner: InProcessEngine,
    name: PathBuf,
    builds: std::cell::Cell<usize>,
}

impl PdfEngine for Racing {
    fn inspect(&self, bytes: &Arc<Vec<u8>>) -> Result<usize, String> {
        self.inner.inspect(bytes)
    }

    fn build(
        &self,
        sources: &[Arc<Vec<u8>>],
        pages: &[PagePlan],
        overlay: &Overlay,
    ) -> Result<Built, String> {
        self.builds.set(self.builds.get() + 1);
        if self.builds.get() == 2 {
            std::fs::write(&self.name, b"someone else's file").unwrap();
        }
        self.inner.build(sources, pages, overlay)
    }
}

#[test]
fn a_name_taken_while_the_files_are_built_is_refused_and_left_alone() {
    let dir = scratch_dir("split-race");
    let s = session(3);
    let ranges = s.split_ranges(None).unwrap();
    let racing = Racing {
        inner: engine(),
        name: dir.join("보고서_2.pdf"),
        builds: std::cell::Cell::new(0),
    };
    let err = write_split(&s, &ranges, &dir, &racing, &deadline()).unwrap_err();
    assert!(err.contains("같은 이름"), "{err}");
    assert_eq!(
        std::fs::read(dir.join("보고서_2.pdf")).unwrap(),
        b"someone else's file",
        "the file that appeared is neither replaced nor removed by the rollback"
    );
    assert_eq!(
        std::fs::read_dir(&dir).unwrap().count(),
        1,
        "the first file this call wrote is gone again"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn a_named_pipe_is_not_read_as_a_pdf() {
    let dir = scratch_dir("pipe");
    let pipe = dir.join("a.pdf");
    crate::named_pipe_support::make_pipe(&pipe);
    let paths = vec![pipe.clone()];
    let read = crate::named_pipe_support::within_time(&pipe, move || read_pdfs(&paths, 0));
    assert_eq!(
        read.expect("read_pdfs returned"),
        Err(MSG_READ_FAILED.to_string())
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn files_are_read_with_their_size_checked_first_and_no_path_in_the_message() {
    let dir = scratch_dir("read");
    let a = dir.join("a.pdf");
    std::fs::write(&a, sample_pdf(1, 1, &[])).unwrap();
    assert_eq!(read_pdfs(std::slice::from_ref(&a), 0).unwrap().len(), 1);
    let missing = read_pdfs(&[dir.join("missing.pdf")], 0).unwrap_err();
    assert_eq!(missing, MSG_READ_FAILED);
    assert!(!missing.contains(dir.to_str().unwrap()));
    // The files together with what the session already holds stay within the session limit,
    // checked as the bytes come in.
    let near_the_limit = read_pdfs(std::slice::from_ref(&a), MAX_SESSION_BYTES - 10);
    assert_eq!(near_the_limit.unwrap_err(), session_too_big_message());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn merging_through_the_helper_returns_the_grown_session() {
    let (merged, count) = merge_files(session(2), vec![sample_pdf(2, 3, &[])], &engine()).unwrap();
    assert_eq!((count, merged.page_count()), (1, 5));
    assert!(merge_files(session(2), vec![b"not a pdf".to_vec()], &engine()).is_err());
}

#[test]
fn a_split_gathers_the_warnings_of_its_files_without_repeats() {
    use crate::pdf_edit_carry_tests::one_page;
    let dir = scratch_dir("split-warnings");
    let annot = "<< /Type /Annot /Subtype /Square /Rect [10 10 50 50] >>".to_string();
    let file = one_page("", "5 0 R", &[annot]);
    let mut s = EditSession::open(file.clone(), &engine()).unwrap();
    s.merge(vec![file], &engine()).unwrap();
    let ranges = s.split_ranges(None).unwrap();
    let written = write_split(&s, &ranges, &dir, &engine(), &deadline()).unwrap();
    assert_eq!(written.files, 2);
    assert_eq!(
        written.warnings,
        ["지원하지 않는 주석 1개는 저장하지 못했습니다 (Square 1)."],
        "both files have the same loss; it is told once"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_save_answers_with_the_warnings_beside_the_name() {
    let saved = PdfEditSaved::saved(Path::new("a.pdf"), 10, 2, vec!["빠진 것".to_string()]);
    assert_eq!(
        serde_json::to_value(&saved).unwrap(),
        serde_json::json!({
            "status": "saved", "name": "a.pdf", "bytes": 10, "pages": 2, "warnings": ["빠진 것"]
        })
    );
    assert_eq!(
        serde_json::to_value(PdfEditSaved::cancelled()).unwrap()["warnings"],
        serde_json::json!([])
    );
}
