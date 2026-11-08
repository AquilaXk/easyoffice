//! Streamed PDF export of the Hangul editor's pages (PD-101, PD-114, PD-123).
//!
//! The export is three commands, all behind the window's [`ShellCaller`] token:
//!
//! 1. `pdf_export_start(file_name)` asks where to save, **before** any page is rendered (cancelling
//!    costs nothing), and opens a job for the window. The page learns a job number, never the path.
//! 2. `pdf_export_pages(job_id, pages)` hands over the next pages, at most [`MAX_PAGES_PER_CALL`]
//!    (the page fetches them from the editor ten at a time, like the print path), and each call is
//!    converted into the growing PDF and dropped, so the page SVGs of a long document are never
//!    alive together, in the page or in the shell.
//! 3. `pdf_export_finish(job_id, commit)` writes the file atomically and opens it in the OS default
//!    app (the reference app's `openAfter`), or, with `commit = false`, throws the job away.
//!
//! Nothing is written before the last page converted: any failure drops the job and leaves the
//! target as it was. The reference app (MIT, Copyright (c) 2025-2026 Edward Kim; see
//! THIRD_PARTY_NOTICES.md) picks the file first and opens it after the export too
//! (`apps/studio-host/src/core/tauri-bridge.ts` 226-245, `apps/desktop/src-tauri/src/commands.rs`
//! 440-482), but renders the whole document in one call and stages the HWP bytes through a
//! temporary file; here the pages travel in batches and there is no staged copy. The OS "open" is
//! the `open` crate the shell already uses for the release page, as in the reference app.

use crate::pdf_export::{self, PdfAssembler};
use crate::save_file::{split_file_name, write_document_bytes, SaveRequest};
use crate::shell_caller::ShellCaller;
use crate::smoke::SmokeProbe;
use crate::{smoke_steps, IpcError};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use tauri::{AppHandle, Manager, State, WebviewWindow};

/// Most pages one `pdf_export_pages` call may carry; the page sends this many at a time.
pub const MAX_PAGES_PER_CALL: usize = 10;

/// Resource sub-directory that holds the bundled OFL fonts used when embedding PDF text.
const PDF_FONT_DIR: &str = "pdf-fonts";
/// What a page SVG holds when it has text, for the smoke check of the finished PDF.
const TEXT_TAG: &str = "<text";
/// The name a finished PDF is verified under, whatever the target is called.
const VERIFY_NAME: &str = "export.pdf";
const MSG_NO_JOB: &str = "진행 중인 PDF 내보내기가 없습니다.";
const MSG_DROPPED: &str = "PDF 내보내기가 취소됐습니다.";
const MSG_OTHER_JOB: &str = "다른 PDF 내보내기가 진행 중이어서 이 요청을 거부했습니다.";

/// Where a finished PDF goes.
pub enum ExportTarget {
    /// The file the user picked in the save dialog.
    File(PathBuf),
    /// The smoke run: the PDF is checked in memory, nothing is written (`smoke_steps.rs`).
    /// `bundled_font` says the font database holds the installer's bundled face.
    Check { bundled_font: bool },
}

/// One export in progress.
pub struct ExportJob {
    id: u64,
    target: ExportTarget,
    assembler: PdfAssembler,
    has_text: bool,
}

impl ExportJob {
    pub fn new(target: ExportTarget, assembler: PdfAssembler) -> Self {
        Self {
            id: 0,
            target,
            assembler,
            has_text: false,
        }
    }

    /// Converts the next pages and returns how many the PDF holds now.
    fn add_pages(&mut self, pages: &[String]) -> Result<usize, String> {
        if pages.len() > MAX_PAGES_PER_CALL {
            return Err(format!(
                "한 번에 받을 수 있는 쪽 수({MAX_PAGES_PER_CALL})를 초과했습니다."
            ));
        }
        self.has_text |= pages.iter().any(|svg| svg.contains(TEXT_TAG));
        self.assembler.add_pages(pages)?;
        Ok(self.assembler.page_count())
    }

    /// Finishes the PDF and delivers it; `open` shows it in the OS default app once it is on disk.
    fn complete(
        self,
        open: impl FnOnce(&Path) -> std::io::Result<()>,
    ) -> Result<PdfFinish, String> {
        let pages = self.assembler.page_count();
        let skipped_pictures = self.assembler.skipped_pictures();
        let pdf = self.assembler.finish()?;
        let path = match self.target {
            ExportTarget::Check { bundled_font } => {
                return smoke_steps::verify_pdf(pages, self.has_text, &pdf, bundled_font);
            }
            ExportTarget::File(path) => path,
        };
        // The same check every saved document passes: the bytes must be a PDF the engine accepts.
        let request = SaveRequest::from_bytes(VERIFY_NAME, pdf)?;
        let written =
            write_document_bytes(&path, request.bytes()).map_err(|err| err.to_string())?;
        // Only a file that is a PDF by name is handed to the OS: the dialog does not force the
        // extension, and a document named like a program must not be launched.
        let opened = is_pdf_name(&path)
            && open(&path)
                .inspect_err(|err| eprintln!("pdf export: the file could not be opened: {err}"))
                .is_ok();
        Ok(PdfFinish::saved(
            &path,
            written.bytes,
            pages,
            opened,
            skipped_pictures,
        ))
    }
}

fn is_pdf_name(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
}

/// Answer of `pdf_export_start`.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PdfStart {
    status: &'static str,
    job_id: Option<u64>,
}

impl PdfStart {
    fn started(job_id: u64) -> Self {
        Self {
            status: "started",
            job_id: Some(job_id),
        }
    }

    fn cancelled() -> Self {
        Self {
            status: "cancelled",
            job_id: None,
        }
    }
}

/// Answer of `pdf_export_pages`: the pages the PDF holds so far.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct PdfProgress {
    pages: usize,
}

/// Answer of `pdf_export_finish`. Like a save it names the file only, never a path (PD-088);
/// `opened` says whether the OS default app was started, `skippedPictures` how many pictures were
/// left out of the pages because they were over the pixel limits (PD-145).
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PdfFinish {
    status: &'static str,
    name: Option<String>,
    bytes: usize,
    pages: usize,
    opened: bool,
    skipped_pictures: u32,
}

impl PdfFinish {
    pub fn saved(
        path: &Path,
        bytes: usize,
        pages: usize,
        opened: bool,
        skipped_pictures: u32,
    ) -> Self {
        Self {
            status: "saved",
            name: path.file_name().map(|n| n.to_string_lossy().into_owned()),
            bytes,
            pages,
            opened,
            skipped_pictures,
        }
    }

    fn cancelled() -> Self {
        Self {
            status: "cancelled",
            name: None,
            bytes: 0,
            pages: 0,
            opened: false,
            skipped_pictures: 0,
        }
    }
}

/// The jobs of all windows and which of them are converting right now.
#[derive(Default)]
struct Table {
    jobs: HashMap<String, ExportJob>,
    /// Window -> number of the job that was taken out of `jobs` to convert pages.
    working: HashMap<String, u64>,
    /// Windows whose working job was dropped (`finish(false)`, the window closed) while it
    /// converted: it must not come back when the conversion ends.
    dropped: HashSet<String>,
}

/// The jobs in progress, one per window. A job is taken out of the table while it works, so a
/// second call for the same job (the page sends them in turn) finds nothing and is refused; a drop
/// that arrives meanwhile is remembered and applied when the conversion ends.
#[derive(Default)]
pub struct PdfExports {
    table: Mutex<Table>,
    last_id: AtomicU64,
}

impl PdfExports {
    /// Makes `job` the window's job (a job the window left behind is dropped) and returns its number.
    pub fn begin(&self, label: &str, mut job: ExportJob) -> u64 {
        job.id = self.last_id.fetch_add(1, Ordering::Relaxed) + 1;
        let id = job.id;
        self.lock().jobs.insert(label.to_string(), job);
        id
    }

    /// Converts the next pages of the window's job. An error ends the job.
    pub fn add_pages(&self, label: &str, id: u64, pages: &[String]) -> Result<usize, String> {
        let mut job = self.take(label, id)?;
        let result = job.add_pages(pages);
        let kept = self.settle(label, result.is_ok().then_some(job));
        result.and_then(|count| kept.then_some(count).ok_or_else(|| MSG_DROPPED.to_string()))
    }

    /// Ends the window's job: delivers the PDF, or with `commit = false` drops it unwritten.
    pub fn finish(
        &self,
        label: &str,
        id: u64,
        commit: bool,
        open: impl FnOnce(&Path) -> std::io::Result<()>,
    ) -> Result<PdfFinish, String> {
        if !commit {
            self.discard(label, id);
            return Ok(PdfFinish::cancelled());
        }
        self.take(label, id)?.complete(open)
    }

    /// Drops the window's job whatever its number (the window is gone).
    pub fn forget(&self, label: &str) {
        let mut table = self.lock();
        table.jobs.remove(label);
        if table.working.contains_key(label) {
            table.dropped.insert(label.to_string());
        }
    }

    /// Drops the job only if it is still number `id`, also while it converts: a late abort of an old
    /// export must not end a newer one.
    fn discard(&self, label: &str, id: u64) {
        let mut table = self.lock();
        if table.jobs.get(label).is_some_and(|job| job.id == id) {
            table.jobs.remove(label);
        } else if table.working.get(label) == Some(&id) {
            table.dropped.insert(label.to_string());
        }
    }

    /// Takes the job out of the table to work on it; `settle` puts it back.
    fn take(&self, label: &str, id: u64) -> Result<ExportJob, String> {
        let mut table = self.lock();
        match table.jobs.get(label) {
            None => Err(MSG_NO_JOB.to_string()),
            Some(job) if job.id != id => Err(MSG_OTHER_JOB.to_string()),
            Some(_) => {
                table.working.insert(label.to_string(), id);
                table
                    .jobs
                    .remove(label)
                    .ok_or_else(|| MSG_NO_JOB.to_string())
            }
        }
    }

    /// Ends a conversion: the job returns to the table unless it failed (`None`), was dropped
    /// meanwhile, or the window started a newer job. True when it returned.
    fn settle(&self, label: &str, job: Option<ExportJob>) -> bool {
        let mut table = self.lock();
        table.working.remove(label);
        let dropped = table.dropped.remove(label);
        match job {
            Some(job) if !dropped && !table.jobs.contains_key(label) => {
                table.jobs.insert(label.to_string(), job);
                true
            }
            _ => false,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Table> {
        self.table.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The folders of the shared font database: the installer's bundled font. The first caller of
/// `shared_font_db` decides them, so the PDF export and the editor's font list both pass these.
pub fn pdf_font_dirs(app: &AppHandle) -> Vec<PathBuf> {
    app.path()
        .resource_dir()
        .map(|dir| vec![dir.join(PDF_FONT_DIR)])
        .unwrap_or_default()
}

fn worker_failed(err: tauri::Error) -> IpcError {
    IpcError::new(format!("PDF 내보내기를 실행하지 못했습니다: {err}"))
}

/// Asks where to save (nothing is rendered yet) and opens the window's export job. In the smoke
/// run there is no dialog: the job only checks the PDF.
#[tauri::command]
pub async fn pdf_export_start(
    _caller: ShellCaller,
    app: AppHandle,
    window: WebviewWindow,
    exports: State<'_, Arc<PdfExports>>,
    file_name: String,
) -> Result<PdfStart, IpcError> {
    let (name, extension) = split_file_name(&file_name).map_err(IpcError::new)?;
    if extension != "pdf" {
        return Err(IpcError::new("PDF 파일 이름은 .pdf 확장자여야 합니다."));
    }
    let picked = if app.try_state::<SmokeProbe>().is_some() {
        None
    } else {
        match crate::pick_save_path(
            &app,
            &window,
            "PDF로 내보내기",
            &name,
            "PDF 문서",
            &extension,
        )
        .await?
        {
            Some(path) => Some(path),
            None => return Ok(PdfStart::cancelled()),
        }
    };
    let font_dirs = pdf_font_dirs(&app);
    let job = tauri::async_runtime::spawn_blocking(move || {
        let fonts = pdf_export::shared_font_db(&font_dirs);
        let target = match picked {
            Some(path) => ExportTarget::File(path),
            None => ExportTarget::Check {
                bundled_font: crate::pdf_fonts::has_face_from(&fonts, &font_dirs),
            },
        };
        ExportJob::new(target, PdfAssembler::new(&fonts, true))
    })
    .await
    .map_err(worker_failed)?;
    Ok(PdfStart::started(exports.begin(window.label(), job)))
}

/// Converts the next pages (at most [`MAX_PAGES_PER_CALL`]) of the window's export.
#[tauri::command]
pub async fn pdf_export_pages(
    _caller: ShellCaller,
    app: AppHandle,
    window: WebviewWindow,
    exports: State<'_, Arc<PdfExports>>,
    job_id: u64,
    pages: Vec<String>,
) -> Result<PdfProgress, IpcError> {
    let label = window.label().to_string();
    let jobs = Arc::clone(&exports);
    let job_label = label.clone();
    let count =
        tauri::async_runtime::spawn_blocking(move || jobs.add_pages(&job_label, job_id, &pages))
            .await
            .map_err(worker_failed)?
            .map_err(IpcError::new)?;
    // The window may have closed while the pages were converted: its job must not outlive it.
    if app.get_webview_window(&label).is_none() {
        exports.forget(&label);
    }
    Ok(PdfProgress { pages: count })
}

/// Writes the PDF and opens it, or (`commit = false`) drops the export.
#[tauri::command]
pub async fn pdf_export_finish(
    _caller: ShellCaller,
    window: WebviewWindow,
    exports: State<'_, Arc<PdfExports>>,
    job_id: u64,
    commit: bool,
) -> Result<PdfFinish, IpcError> {
    let label = window.label().to_string();
    let jobs = Arc::clone(&exports);
    tauri::async_runtime::spawn_blocking(move || {
        jobs.finish(&label, job_id, commit, |path| open::that(path))
    })
    .await
    .map_err(worker_failed)?
    .map_err(IpcError::new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::fs;
    use usvg::fontdb::Database;

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("easyoffice-pdf-job-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn page() -> String {
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="30"><rect width="40" height="30" fill="white"/></svg>"#
            .to_string()
    }

    fn job_to(path: &Path) -> ExportJob {
        let fonts = Arc::new(Database::new());
        ExportJob::new(
            ExportTarget::File(path.to_path_buf()),
            PdfAssembler::new(&fonts, true),
        )
    }

    fn no_open(_: &Path) -> std::io::Result<()> {
        panic!("the file must not be opened");
    }

    #[test]
    fn pages_arrive_in_batches_and_the_pdf_is_written_once_at_the_end() {
        let dir = scratch_dir("batches");
        let target = dir.join("보고서.pdf");
        let exports = PdfExports::default();
        let id = exports.begin("doc-1", job_to(&target));
        assert_eq!(exports.add_pages("doc-1", id, &vec![page(); 10]), Ok(10));
        assert_eq!(exports.add_pages("doc-1", id, &vec![page(); 3]), Ok(13));
        assert!(!target.exists(), "nothing is on disk before the end");
        let opened = RefCell::new(Vec::new());
        let done = exports
            .finish("doc-1", id, true, |path| {
                opened.borrow_mut().push(path.to_path_buf());
                Ok(())
            })
            .expect("finished");
        assert_eq!(
            serde_json::to_value(&done).unwrap(),
            serde_json::json!({
                "status": "saved", "name": "보고서.pdf",
                "bytes": fs::metadata(&target).unwrap().len(), "pages": 13, "opened": true, "skippedPictures": 0
            })
        );
        assert_eq!(
            *opened.borrow(),
            vec![target.clone()],
            "opened once, after the write"
        );
        let pdf = fs::read(&target).unwrap();
        assert!(pdf_export::verify_exported_pdf(&pdf, 13, false).is_ok());
        let json = serde_json::to_string(&done).unwrap();
        assert!(
            !json.contains('/') && !json.contains('\\'),
            "no path leaks: {json}"
        );
        assert_eq!(
            exports.finish("doc-1", id, true, no_open),
            Err(MSG_NO_JOB.to_string()),
            "a finished job is gone"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn dropping_a_job_writes_nothing_and_does_not_open_anything() {
        let dir = scratch_dir("drop");
        let target = dir.join("a.pdf");
        let exports = PdfExports::default();
        let id = exports.begin("doc-1", job_to(&target));
        exports.add_pages("doc-1", id, &[page()]).unwrap();
        let done = exports.finish("doc-1", id, false, no_open).unwrap();
        assert_eq!(done, PdfFinish::cancelled());
        assert_eq!(
            fs::read_dir(&dir).unwrap().count(),
            0,
            "no file, no temp file"
        );
        assert!(exports.add_pages("doc-1", id, &[page()]).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failing_page_ends_the_job_and_leaves_the_target_as_it_was() {
        let dir = scratch_dir("fail");
        let target = dir.join("a.pdf");
        fs::write(&target, b"the old file").unwrap();
        let exports = PdfExports::default();
        let id = exports.begin("doc-1", job_to(&target));
        exports.add_pages("doc-1", id, &vec![page(); 10]).unwrap();
        let err = exports
            .add_pages("doc-1", id, &[page(), "not svg".to_string()])
            .unwrap_err();
        assert!(err.starts_with("12쪽"), "{err}");
        assert_eq!(
            exports.finish("doc-1", id, true, no_open),
            Err(MSG_NO_JOB.to_string())
        );
        assert_eq!(fs::read(&target).unwrap(), b"the old file");
        assert_eq!(
            fs::read_dir(&dir).unwrap().count(),
            1,
            "no temporary file left"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_batch_over_the_call_limit_is_refused_before_any_page_is_converted() {
        let dir = scratch_dir("limit");
        let exports = PdfExports::default();
        let id = exports.begin("doc-1", job_to(&dir.join("a.pdf")));
        let err = exports
            .add_pages("doc-1", id, &vec![page(); MAX_PAGES_PER_CALL + 1])
            .unwrap_err();
        assert!(err.contains("한 번에"), "{err}");
        assert!(
            exports.add_pages("doc-1", id, &[page()]).is_err(),
            "the refusal ended the job"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_job_has_nothing_to_write() {
        let dir = scratch_dir("empty");
        let target = dir.join("a.pdf");
        let exports = PdfExports::default();
        let id = exports.begin("doc-1", job_to(&target));
        assert!(exports.add_pages("doc-1", id, &[]).is_err());
        let id = exports.begin("doc-1", job_to(&target));
        assert!(exports.finish("doc-1", id, true, no_open).is_err());
        assert!(!target.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_job_answers_only_to_its_own_window_and_number() {
        let dir = scratch_dir("owner");
        let exports = PdfExports::default();
        let first = exports.begin("doc-1", job_to(&dir.join("a.pdf")));
        assert_eq!(
            exports.add_pages("doc-2", first, &[page()]),
            Err(MSG_NO_JOB.to_string()),
            "another window has no such job"
        );
        let second = exports.begin("doc-1", job_to(&dir.join("b.pdf")));
        assert_ne!(first, second);
        assert_eq!(
            exports.add_pages("doc-1", first, &[page()]),
            Err(MSG_OTHER_JOB.to_string()),
            "a replaced job's number is stale"
        );
        assert_eq!(
            exports.finish("doc-1", first, false, no_open),
            Ok(PdfFinish::cancelled())
        );
        assert_eq!(
            exports.add_pages("doc-1", second, &[page()]),
            Ok(1),
            "a late abort of the old job did not end the new one"
        );
        exports.forget("doc-1");
        assert!(exports.add_pages("doc-1", second, &[page()]).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    /// `finish(false)`, a closing window or a newer export arriving while pages convert (the job is
    /// out of the table then) must not be undone when the conversion ends.
    #[test]
    fn a_drop_during_a_conversion_is_applied_when_it_ends() {
        let dir = scratch_dir("in-flight");
        let target = dir.join("a.pdf");
        let exports = PdfExports::default();

        let id = exports.begin("doc-1", job_to(&target));
        let job = exports.take("doc-1", id).unwrap();
        assert_eq!(
            exports.finish("doc-1", id, false, no_open),
            Ok(PdfFinish::cancelled())
        );
        assert!(
            !exports.settle("doc-1", Some(job)),
            "the job does not come back"
        );
        assert_eq!(
            exports.add_pages("doc-1", id, &[page()]),
            Err(MSG_NO_JOB.to_string())
        );
        assert_eq!(
            exports.finish("doc-1", id, true, no_open),
            Err(MSG_NO_JOB.to_string())
        );

        let id = exports.begin("doc-1", job_to(&target));
        let job = exports.take("doc-1", id).unwrap();
        exports.forget("doc-1");
        assert!(!exports.settle("doc-1", Some(job)), "the window closed");

        let old = exports.begin("doc-1", job_to(&target));
        let job = exports.take("doc-1", old).unwrap();
        let new = exports.begin("doc-1", job_to(&target));
        assert!(
            !exports.settle("doc-1", Some(job)),
            "a newer export owns the window"
        );
        assert_eq!(exports.add_pages("doc-1", new, &[page()]), Ok(1));

        // A drop that names another number, or arrives after the conversion, changes nothing.
        let job = exports.take("doc-1", new).unwrap();
        exports.discard("doc-1", new + 5);
        assert!(exports.settle("doc-1", Some(job)));
        assert_eq!(exports.add_pages("doc-1", new, &[page()]), Ok(2));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_could_not_be_opened_is_still_saved() {
        let dir = scratch_dir("open-fails");
        let target = dir.join("a.pdf");
        let exports = PdfExports::default();
        let id = exports.begin("doc-1", job_to(&target));
        exports.add_pages("doc-1", id, &[page()]).unwrap();
        let done = exports
            .finish("doc-1", id, true, |_| {
                Err(std::io::Error::other("no application"))
            })
            .expect("saved");
        assert!(!done.opened && done.status == "saved");
        assert!(target.is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_a_file_named_pdf_is_handed_to_the_os() {
        let dir = scratch_dir("names");
        for (name, opens) in [
            ("a.PDF", true),
            ("a.pdf", true),
            ("run.bat", false),
            ("noext", false),
        ] {
            let target = dir.join(name);
            let exports = PdfExports::default();
            let id = exports.begin("doc-1", job_to(&target));
            exports.add_pages("doc-1", id, &[page()]).unwrap();
            let calls = RefCell::new(0);
            let done = exports
                .finish("doc-1", id, true, |_| {
                    *calls.borrow_mut() += 1;
                    Ok(())
                })
                .expect("saved");
            assert_eq!(
                (*calls.borrow() == 1, done.opened),
                (opens, opens),
                "{name}"
            );
            assert!(target.is_file(), "{name} is written either way");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_smoke_target_checks_the_pdf_in_memory_and_writes_nothing() {
        let fonts = Arc::new(Database::new());
        let exports = PdfExports::default();
        let job = ExportJob::new(
            ExportTarget::Check { bundled_font: true },
            PdfAssembler::new(&fonts, true),
        );
        let id = exports.begin("main", job);
        exports.add_pages("main", id, &vec![page(); 2]).unwrap();
        let done = exports.finish("main", id, true, no_open).expect("checked");
        assert_eq!(done.name.as_deref(), Some("smoke.pdf"));
        assert_eq!(done.pages, 2);
        assert!(!done.opened);
    }

    #[test]
    fn the_start_command_takes_only_a_pdf_file_name() {
        for name in ["noext", "a.docx", "a.pdf.exe", ".pdf"] {
            let refused = split_file_name(name).map_or(true, |(_, ext)| ext != "pdf");
            assert!(refused, "{name}");
        }
        assert_eq!(
            split_file_name("C:\\x\\보고서.PDF").unwrap(),
            ("보고서.PDF".to_string(), "pdf".to_string())
        );
    }
}
