//! The commands that edit the PDF of a window (PD-166): rotate, stamp and watermark, merge, split
//! and save, on the objects of the opened file instead of on the text model.
//!
//! Like the other shell commands they are behind the window's [`ShellCaller`] token, and the page
//! never sees a path (PD-088): the shell owns every dialog, reads the files the user picks, and the
//! page learns names, counts and sentences. The page hands over the opened PDF once
//! (`pdf_edit_open`, base64, the same 10 MiB the file picker allows); after that the shell holds it
//! and the edits are small requests (`pdf_edit_apply`). Merge and split ask in native dialogs.
//! Everything that parses a PDF runs in a worker process that is killed at its deadline or memory
//! limit (`pdf_edit_engine.rs`, `pdf_edit_supervisor.rs`), and
//! nothing is written before the whole result exists: a split that fails removes the files it made.

use crate::pdf_edit_budget::{Deadline, MAX_SOURCE_BYTES, MAX_SPLIT_TOTAL_BYTES, SPLIT_DEADLINE};
use crate::pdf_edit_engine::{ChildEngine, PdfEngine};
use crate::pdf_edit_overlay::Overlay;
use crate::pdf_edit_session::EditSession;
use crate::save_file::{split_file_name, write_document_bytes, SaveRequest};
use crate::shell_caller::ShellCaller;
use crate::{pdf_job, IpcError};
use easyoffice_app::EasyOfficeIpcRouter;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

const MSG_NO_SESSION: &str = "편집할 원본 PDF가 없습니다.";
const MSG_CHANGED: &str = "작업하는 사이 문서가 바뀌어 중단했습니다. 다시 시도해 주세요.";
const MSG_READ_FAILED: &str = "선택한 PDF 파일을 읽지 못했습니다.";
const MSG_BASE64: &str = "PDF 데이터가 올바른 Base64가 아닙니다.";
const MSG_ONLY_PDF: &str = "PDF 파일만 편집할 수 있습니다.";
/// The longest file name stem that is kept for the names of the results (characters).
const MAX_STEM_CHARS: usize = 80;

/// The versions of sessions are numbered from one counter for all windows and never reused, so a
/// long operation that started from an old version cannot overwrite a newer one, not even one
/// that replaced a session that was dropped in between.
static VERSIONS: AtomicU64 = AtomicU64::new(1);

fn next_version() -> u64 {
    VERSIONS.fetch_add(1, Ordering::Relaxed)
}

/// One window's session and the number of its version (every change makes a new one).
struct Entry {
    session: EditSession,
    version: u64,
}

/// The PDFs being edited, one session per window.
#[derive(Default)]
pub struct PdfEdits {
    table: Mutex<HashMap<String, Entry>>,
}

impl PdfEdits {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entry>> {
        self.table.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Starts (or replaces) the window's session.
    fn begin(&self, label: &str, session: EditSession) {
        let mut table = self.lock();
        table.insert(
            label.to_string(),
            Entry {
                session,
                version: next_version(),
            },
        );
    }

    /// A copy of the session and its version, to work on without holding the lock.
    fn snapshot(&self, label: &str) -> Result<(EditSession, u64), String> {
        self.lock()
            .get(label)
            .map(|e| (e.session.clone(), e.version))
            .ok_or_else(|| MSG_NO_SESSION.to_string())
    }

    /// Replaces the session if it is still the version `from`.
    fn commit(&self, label: &str, from: u64, session: EditSession) -> Result<(), String> {
        let mut table = self.lock();
        match table.get_mut(label) {
            Some(entry) if entry.version == from => {
                entry.session = session;
                entry.version = next_version();
                Ok(())
            }
            Some(_) => Err(MSG_CHANGED.to_string()),
            None => Err(MSG_NO_SESSION.to_string()),
        }
    }

    /// Changes the window's session in place (small, parse-free edits).
    fn update<T>(
        &self,
        label: &str,
        change: impl FnOnce(&mut EditSession) -> Result<T, String>,
    ) -> Result<(T, EditSession), String> {
        let mut table = self.lock();
        let entry = table
            .get_mut(label)
            .ok_or_else(|| MSG_NO_SESSION.to_string())?;
        let mut edited = entry.session.clone();
        let out = change(&mut edited)?;
        entry.session = edited.clone();
        entry.version = next_version();
        Ok((out, edited))
    }

    /// Drops the window's session (the window is gone).
    pub fn forget(&self, label: &str) {
        self.lock().remove(label);
    }
}

/// What the page is told about the session after each command.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PdfEditState {
    pages: usize,
    sources: usize,
    /// Turn of the first page (what the rotation badge shows).
    turn: u16,
    stamp: Option<String>,
    watermark: Option<String>,
}

impl PdfEditState {
    fn of(session: &EditSession) -> Self {
        Self {
            pages: session.page_count(),
            sources: session.source_count(),
            turn: session.first_turn(),
            stamp: session.overlay().stamp.clone(),
            watermark: session.overlay().watermark.clone(),
        }
    }
}

/// What `pdf_edit_apply` can be asked to do. An unknown kind or field fails to deserialize and the
/// command is refused.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum PdfEditOp {
    /// Set the turn of every page to `degrees` (the rotation control of the page shows one number
    /// for the file, and sends it before a save).
    #[serde(rename_all = "camelCase")]
    Turn { degrees: i32 },
    /// Turn the pages of `pages` ("1-3, 5"; all pages without it) `degrees` more, clockwise.
    #[serde(rename_all = "camelCase")]
    Rotate { degrees: i32, pages: Option<String> },
    /// Set the stamp (first page) and the watermark (every page); an empty text clears it.
    #[serde(rename_all = "camelCase")]
    Overlay {
        stamp: Option<String>,
        watermark: Option<String>,
    },
}

fn apply(session: &mut EditSession, op: PdfEditOp) -> Result<(), String> {
    match op {
        PdfEditOp::Turn { degrees } => session.set_turn(degrees),
        PdfEditOp::Rotate { degrees, pages } => session.rotate(pages.as_deref(), degrees),
        PdfEditOp::Overlay { stamp, watermark } => {
            session.set_overlay(Overlay::new(watermark.as_deref(), stamp.as_deref())?);
            Ok(())
        }
    }
}

/// The file name without its extension, with what a file name may not hold replaced and the length
/// limited: the base of the names of the results.
fn file_stem(file_name: &str) -> String {
    let base = file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem);
    let clean: String = base
        .trim()
        .chars()
        .take(MAX_STEM_CHARS)
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    if clean.is_empty() {
        "문서".to_string()
    } else {
        clean
    }
}

/// `보고서_1-3.pdf` for pages 1 to 3 and `보고서_5.pdf` for page 5 of the plan.
fn split_file_name_of(stem: &str, range: &Range<usize>) -> String {
    if range.len() == 1 {
        format!("{stem}_{}.pdf", range.start + 1)
    } else {
        format!("{stem}_{}-{}.pdf", range.start + 1, range.end)
    }
}

/// What `pdf_edit_split_merge` is asked to do. The page's dialog (`pdf-edit.js`) offers the three;
/// an unknown name fails to deserialize and the command is refused.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SplitMergeMode {
    /// Append other PDFs (a multi-file open dialog).
    Merge,
    /// One file for each range of pages the page typed (`1-3, 5, 7-`).
    Range,
    /// One file for each page.
    Each,
}

fn worker_failed(err: tauri::Error) -> IpcError {
    IpcError::new(format!("PDF 편집을 실행하지 못했습니다: {err}"))
}

/// Gives the shell the PDF the window opened, once, so that later edits work on its objects. A file
/// that fails the checks (encrypted, over a budget, damaged) is refused with the reason, and the
/// window keeps working on the text model as before.
#[tauri::command]
pub async fn pdf_edit_open(
    _caller: ShellCaller,
    app: AppHandle,
    window: WebviewWindow,
    edits: State<'_, Arc<PdfEdits>>,
    file_name: String,
    base64: String,
) -> Result<PdfEditState, IpcError> {
    let (base, extension) = split_file_name(&file_name).map_err(IpcError::new)?;
    if extension != "pdf" {
        return Err(IpcError::new(MSG_ONLY_PDF));
    }
    let stem = file_stem(&base);
    // A file that is refused leaves the window with no session, not with the previous file's.
    edits.forget(window.label());
    let engine = ChildEngine::new(pdf_job::pdf_font_dirs(&app)).map_err(IpcError::new)?;
    let session =
        tauri::async_runtime::spawn_blocking(move || open_session(&base64, &stem, &engine))
            .await
            .map_err(worker_failed)?
            .map_err(IpcError::new)?;
    let state = PdfEditState::of(&session);
    edits.begin(window.label(), session);
    // The window may have closed while the file was read: its session must not outlive it.
    if app.get_webview_window(window.label()).is_none() {
        edits.forget(window.label());
    }
    Ok(state)
}

fn open_session(base64: &str, stem: &str, engine: &dyn PdfEngine) -> Result<EditSession, String> {
    if base64.len() / 4 * 3 > MAX_SOURCE_BYTES + 3 {
        return Err(too_big_message());
    }
    let bytes =
        EasyOfficeIpcRouter::decode_base64(base64.trim()).ok_or_else(|| MSG_BASE64.to_string())?;
    EditSession::open(bytes, engine).map(|session| session.with_stem(stem))
}

fn too_big_message() -> String {
    format!(
        "PDF가 허용 크기({}MiB)를 초과합니다.",
        MAX_SOURCE_BYTES / 1_048_576
    )
}

/// Rotates pages or sets the stamp and watermark of the window's PDF. It changes the plan only, so
/// it is instant and can be repeated or undone by setting the opposite.
#[tauri::command]
pub fn pdf_edit_apply(
    _caller: ShellCaller,
    window: WebviewWindow,
    edits: State<'_, Arc<PdfEdits>>,
    op: PdfEditOp,
) -> Result<PdfEditState, IpcError> {
    edits
        .update(window.label(), |session| apply(session, op))
        .map(|(_, session)| PdfEditState::of(&session))
        .map_err(IpcError::new)
}

/// Answer of `pdf_edit_split_merge`.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PdfSplitMerge {
    /// `merged`, `split` or `cancelled`.
    action: &'static str,
    /// PDFs appended (merge) or files written (split).
    count: usize,
    #[serde(flatten)]
    state: PdfEditState,
}

/// Merges or splits the window's PDF as the page's dialog chose. A merge asks which PDFs in a
/// native open dialog and appends their pages after the current ones; a split asks for a folder
/// and writes one file for each range (`ranges`, or each page when the mode is `each`). Ranges
/// that cannot be read are refused before any dialog opens.
#[tauri::command]
pub async fn pdf_edit_split_merge(
    _caller: ShellCaller,
    app: AppHandle,
    window: WebviewWindow,
    edits: State<'_, Arc<PdfEdits>>,
    mode: SplitMergeMode,
    ranges: Option<String>,
) -> Result<PdfSplitMerge, IpcError> {
    let label = window.label().to_string();
    let (session, version) = edits.snapshot(&label).map_err(IpcError::new)?;
    let (merged, action, count) = match mode {
        SplitMergeMode::Merge => match merge_flow(&app, &window, session.clone()).await? {
            Some((merged, count)) => (Some(merged), "merged", count),
            None => (None, "cancelled", 0),
        },
        SplitMergeMode::Range | SplitMergeMode::Each => {
            let spec = (mode == SplitMergeMode::Range).then_some(ranges.as_deref().unwrap_or(""));
            let parts = session.split_ranges(spec).map_err(IpcError::new)?;
            match split_flow(&app, &window, &session, parts).await? {
                Some(count) => (None, "split", count),
                None => (None, "cancelled", 0),
            }
        }
    };
    // A split writes files and leaves the session as it was; only a merge changes it.
    let session = match merged {
        Some(merged) => {
            edits
                .commit(&label, version, merged.clone())
                .map_err(IpcError::new)?;
            merged
        }
        None => session,
    };
    Ok(PdfSplitMerge {
        action,
        count,
        state: PdfEditState::of(&session),
    })
}

/// Which PDFs to append: a multi-file open dialog, the files read in Rust, then merged under the
/// guard. `None` when the dialog was cancelled.
async fn merge_flow(
    app: &AppHandle,
    window: &WebviewWindow,
    session: EditSession,
) -> Result<Option<(EditSession, usize)>, IpcError> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog()
        .file()
        .set_parent(window)
        .set_title("이어 붙일 PDF 선택")
        .add_filter("PDF 문서", &["pdf"])
        .pick_files(move |picked| {
            let _ = tx.send(picked);
        });
    let picked = tauri::async_runtime::spawn_blocking(move || rx.recv().ok().flatten())
        .await
        .map_err(|err| IpcError::new(format!("열기 대화상자를 열지 못했습니다: {err}")))?;
    let Some(picked) = picked else {
        return Ok(None);
    };
    let paths: Vec<PathBuf> = picked
        .into_iter()
        .map(|p| p.into_path().map_err(|_| MSG_READ_FAILED.to_string()))
        .collect::<Result<_, _>>()
        .map_err(IpcError::new)?;
    let engine = ChildEngine::new(pdf_job::pdf_font_dirs(app)).map_err(IpcError::new)?;
    let merged = tauri::async_runtime::spawn_blocking(move || {
        let files = read_pdfs(&paths)?;
        merge_files(session, files, &engine)
    })
    .await
    .map_err(worker_failed)?
    .map_err(IpcError::new)?;
    Ok(Some(merged))
}

fn merge_files(
    mut session: EditSession,
    files: Vec<Vec<u8>>,
    engine: &dyn PdfEngine,
) -> Result<(EditSession, usize), String> {
    let merged = session.merge(files, engine)?;
    Ok((session, merged))
}

/// Reads the chosen files, each no larger than [`MAX_SOURCE_BYTES`] (the size is read before the
/// file is, so a huge file is never loaded to find out). The reason goes to the log only.
fn read_pdfs(paths: &[PathBuf]) -> Result<Vec<Vec<u8>>, String> {
    paths
        .iter()
        .map(|path| {
            let size = std::fs::metadata(path).map(|m| m.len()).map_err(|err| {
                eprintln!("pdf edit: {} could not be inspected: {err}", path.display());
                MSG_READ_FAILED.to_string()
            })?;
            if size > MAX_SOURCE_BYTES as u64 {
                return Err(too_big_message());
            }
            std::fs::read(path).map_err(|err| {
                eprintln!("pdf edit: {} could not be read: {err}", path.display());
                MSG_READ_FAILED.to_string()
            })
        })
        .collect()
}

/// Which folder gets the files: a folder dialog, then one file for each range (`보고서_1-3.pdf`). Nothing is
/// overwritten: if any name is taken, nothing is written. `None` when the dialog was cancelled.
async fn split_flow(
    app: &AppHandle,
    window: &WebviewWindow,
    session: &EditSession,
    ranges: Vec<Range<usize>>,
) -> Result<Option<usize>, IpcError> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog()
        .file()
        .set_parent(window)
        .set_title("분할한 PDF를 저장할 폴더 선택")
        .pick_folder(move |picked| {
            let _ = tx.send(picked);
        });
    let picked = tauri::async_runtime::spawn_blocking(move || rx.recv().ok().flatten())
        .await
        .map_err(|err| IpcError::new(format!("폴더 대화상자를 열지 못했습니다: {err}")))?;
    let Some(picked) = picked else {
        return Ok(None);
    };
    let folder = picked
        .into_path()
        .map_err(|_| IpcError::new("저장할 폴더를 확인할 수 없습니다."))?;
    let engine = ChildEngine::new(pdf_job::pdf_font_dirs(app)).map_err(IpcError::new)?;
    let session = session.clone();
    let written = tauri::async_runtime::spawn_blocking(move || {
        write_split(
            &session,
            &ranges,
            &folder,
            &engine,
            &Deadline::after(SPLIT_DEADLINE),
        )
    })
    .await
    .map_err(worker_failed)?
    .map_err(IpcError::new)?;
    Ok(Some(written))
}

/// Writes the file of each range into `folder`. The names are checked first; the files are built,
/// checked and written one at a time (so they are never all in memory), and if one fails the ones
/// written so far are removed again.
fn write_split(
    session: &EditSession,
    ranges: &[Range<usize>],
    folder: &Path,
    engine: &dyn PdfEngine,
    deadline: &Deadline,
) -> Result<usize, String> {
    let names: Vec<String> = ranges
        .iter()
        .map(|range| split_file_name_of(&file_stem(session.stem()), range))
        .collect();
    if let Some(taken) = names.iter().find(|name| folder.join(name).exists()) {
        return Err(format!(
            "같은 이름의 파일이 이미 있어 분할하지 않았습니다: {taken}"
        ));
    }
    let mut written: Vec<PathBuf> = Vec::new();
    let mut total = 0usize;
    let outcome = (|| {
        for (name, range) in names.iter().zip(ranges) {
            deadline.check()?;
            let bytes = session.render(Some(range.clone()), engine)?;
            total = total.saturating_add(bytes.len());
            if total > MAX_SPLIT_TOTAL_BYTES {
                return Err(format!(
                    "분할한 파일의 합계가 허용 크기({}MiB)를 초과합니다.",
                    MAX_SPLIT_TOTAL_BYTES / 1_048_576
                ));
            }
            let request = SaveRequest::from_bytes(name, bytes)?;
            let target = folder.join(name);
            write_document_bytes(&target, request.bytes()).map_err(|err| err.to_string())?;
            written.push(target);
        }
        Ok(())
    })();
    match outcome {
        Ok(()) => Ok(written.len()),
        Err(message) => {
            for target in &written {
                let _ = std::fs::remove_file(target);
            }
            Err(message)
        }
    }
}

/// Answer of `pdf_edit_save`. Like a save it names the file only, never a path (PD-088).
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PdfEditSaved {
    status: &'static str,
    name: Option<String>,
    bytes: usize,
    pages: usize,
}

impl PdfEditSaved {
    fn saved(path: &Path, bytes: usize, pages: usize) -> Self {
        Self {
            status: "saved",
            name: path.file_name().map(|n| n.to_string_lossy().into_owned()),
            bytes,
            pages,
        }
    }

    fn cancelled() -> Self {
        Self {
            status: "cancelled",
            name: None,
            bytes: 0,
            pages: 0,
        }
    }
}

/// Asks where to save, then writes the edited PDF (all pages of the plan) atomically, after it
/// passes the check every saved document passes. The suggested name is the opened file's name with
/// "-편집" so the original is not the default target.
#[tauri::command]
pub async fn pdf_edit_save(
    _caller: ShellCaller,
    app: AppHandle,
    window: WebviewWindow,
    edits: State<'_, Arc<PdfEdits>>,
) -> Result<PdfEditSaved, IpcError> {
    let (session, _) = edits.snapshot(window.label()).map_err(IpcError::new)?;
    let suggestion = format!("{}-편집.pdf", file_stem(session.stem()));
    let Some(target) = crate::pick_save_path(
        &app,
        &window,
        "편집한 PDF 저장",
        &suggestion,
        "PDF 문서",
        "pdf",
    )
    .await?
    else {
        return Ok(PdfEditSaved::cancelled());
    };
    let pages = session.page_count();
    let engine = ChildEngine::new(pdf_job::pdf_font_dirs(&app)).map_err(IpcError::new)?;
    let bytes = tauri::async_runtime::spawn_blocking(move || session.render(None, &engine))
        .await
        .map_err(worker_failed)?
        .map_err(IpcError::new)?;
    let name = target.file_name().map_or_else(
        || "edited.pdf".to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let request = SaveRequest::from_bytes(&name, bytes).map_err(IpcError::new)?;
    let written = tauri::async_runtime::spawn_blocking(move || {
        write_document_bytes(&target, request.bytes()).map(|written| (target, written))
    })
    .await
    .map_err(worker_failed)?
    .map_err(|err| IpcError::new(err.to_string()))?;
    Ok(PdfEditSaved::saved(&written.0, written.1.bytes, pages))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf_edit_engine::InProcessEngine;
    use crate::pdf_edit_fixtures::{deadline, read, sample_pdf};
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
            serde_json::from_str(r#"{"kind":"overlay","stamp":"승인필","watermark":null}"#)
                .unwrap();
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
            Ok(3)
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
    fn files_are_read_with_their_size_checked_first_and_no_path_in_the_message() {
        let dir = scratch_dir("read");
        let a = dir.join("a.pdf");
        std::fs::write(&a, sample_pdf(1, 1, &[])).unwrap();
        assert_eq!(read_pdfs(std::slice::from_ref(&a)).unwrap().len(), 1);
        let missing = read_pdfs(&[dir.join("missing.pdf")]).unwrap_err();
        assert_eq!(missing, MSG_READ_FAILED);
        assert!(!missing.contains(dir.to_str().unwrap()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn merging_through_the_helper_returns_the_grown_session() {
        let (merged, count) =
            merge_files(session(2), vec![sample_pdf(2, 3, &[])], &engine()).unwrap();
        assert_eq!((count, merged.page_count()), (1, 5));
        assert!(merge_files(session(2), vec![b"not a pdf".to_vec()], &engine()).is_err());
    }
}
