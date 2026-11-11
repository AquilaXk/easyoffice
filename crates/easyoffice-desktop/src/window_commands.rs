//! Tauri wiring of the window model (PD-088): window creation, the native open dialog, drag and
//! drop, the close flow and the window commands the WebView may call. Every command here is narrow:
//! none takes or returns a file system path. Paths are read, canonicalized and tracked in Rust only.

use crate::open_feedback::{report_open_failure, UNSUPPORTED};
use crate::recent::RecentList;
use crate::shell_caller::{self, ShellCaller};
use crate::smoke::SmokeProbe;
use crate::tracked_files::{Fingerprint, TrackedFile, TrackedFiles};
use crate::window_state::{MonitorRect, WindowGeometry};
use crate::windows::{
    display_name, is_window_label, new_label, path_key, window_title, window_url, CloseAction,
    PendingOpen, PendingOpenQueue, PendingOpenView, WindowKind, WindowRegistry, PRIMARY_LABEL,
};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;
use tauri::webview::PageLoadEvent;
use tauri::{
    AppHandle, DragDropEvent, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_dialog::DialogExt;

/// Runs the page's own 저장 / 저장 안 함 / 취소 flow when the native close button is used.
const CLOSE_REQUEST_SCRIPT: &str =
    "window.__EASYOFFICE_CLOSE_REQUEST__ && window.__EASYOFFICE_CLOSE_REQUEST__();";
const SMOKE_SECONDARY_SCRIPT: &str = "window.__EASYOFFICE_SMOKE__ = 'secondary';";
const CASCADE_STEP: f64 = 28.0;

/// Error shape shared with the other commands (`{"status":"error","message":...}`).
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct WindowError {
    status: &'static str,
    message: String,
}

impl WindowError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            status: "error",
            message: message.into(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct OpenOutcome {
    status: &'static str,
}

impl OpenOutcome {
    const OPENED: Self = Self { status: "opened" };
    const FOCUSED: Self = Self { status: "focused" };
    const CANCELLED: Self = Self {
        status: "cancelled",
    };
}

pub struct AppWindows {
    registry: Mutex<WindowRegistry>,
    pending: Mutex<PendingOpenQueue>,
    recents: Mutex<RecentList>,
    files: Mutex<TrackedFiles>,
    geometry: Mutex<Option<WindowGeometry>>,
    recents_dir: Option<PathBuf>,
    counter: AtomicU64,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl AppWindows {
    pub fn new(recents_dir: Option<PathBuf>) -> Self {
        let recents = recents_dir
            .as_deref()
            .map(RecentList::load)
            .unwrap_or_default();
        Self {
            registry: Mutex::new(WindowRegistry::default()),
            pending: Mutex::new(PendingOpenQueue::default()),
            recents: Mutex::new(recents),
            files: Mutex::new(TrackedFiles::default()),
            geometry: Mutex::new(recents_dir.as_deref().and_then(WindowGeometry::load)),
            recents_dir,
            counter: AtomicU64::new(1),
        }
    }

    /// The document name `label` reports, "제목 없음" while it has none.
    pub fn shown_name(&self, label: &str) -> String {
        locked(&self.registry).shown_name(label)
    }

    /// True while any window reports unsaved changes (the signed update waits for them).
    pub fn has_unsaved_documents(&self) -> bool {
        locked(&self.registry).has_unsaved()
    }

    /// The path behind an opaque recent id; it never leaves Rust.
    pub fn recent_path(&self, id: &str) -> Option<PathBuf> {
        locked(&self.recents).path_for(id)
    }

    /// Empties the recent list and its stored file.
    pub fn clear_recents(&self) -> std::io::Result<()> {
        locked(&self.recents).update(self.recents_dir.as_deref(), RecentList::clear)
    }

    fn next_label(&self) -> String {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        new_label(n, millis)
    }

    /// The window was opened for a file its page could not show: nothing of that file is left, so
    /// the window is no longer its owner, has no path to save to and holds none of its bytes.
    fn abandon_open(&self, label: &str) {
        locked(&self.registry).release_path(label);
        locked(&self.files).untrack(label);
        locked(&self.pending).discard(label);
    }

    fn remember(&self, app: &AppHandle, path: &Path) {
        match locked(&self.recents)
            .update(self.recents_dir.as_deref(), |r| r.add(path.to_path_buf()))
        {
            Ok(()) => crate::recent::notify_changed(app),
            Err(err) => eprintln!("recent: {err}"),
        }
    }
}

/// Margin kept free around a new window and the height the window frame takes beyond the inner
/// size, as in the reference app (`windows.rs` 12-13, 111-118).
const WORK_AREA_MARGIN: f64 = 24.0;
const FRAME_HEIGHT_RESERVE: f64 = 72.0;
/// A new window never opens larger than this share of the usable work area (`windows.rs` 11, 100-101).
const MAX_WORK_AREA_RATIO: f64 = 0.85;

/// The part of a monitor that windows can use (without the menu bar, dock or taskbar), in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
struct WorkArea {
    origin: (f64, f64),
    size: (f64, f64),
}

/// Position of the n-th document window inside the work area: centred (the frame's extra height
/// counted), shifted by a small cascade step, never closer than the margin to the work area's edge.
pub fn cascade_origin(work: (f64, f64), size: (f64, f64), index: usize) -> (f64, f64) {
    let shift = CASCADE_STEP * (index % 8) as f64;
    let x = ((work.0 - size.0) / 2.0 + shift).max(WORK_AREA_MARGIN);
    let y = ((work.1 - size.1 - FRAME_HEIGHT_RESERVE) / 2.0 + shift).max(WORK_AREA_MARGIN);
    (x, y)
}

/// A window never opens larger than 85% of the usable work area, but never below its minimum.
pub fn clamp_size(work: (f64, f64), size: (f64, f64), min: (f64, f64)) -> (f64, f64) {
    let max_width = ((work.0 - WORK_AREA_MARGIN * 2.0).max(1.0) * MAX_WORK_AREA_RATIO).floor();
    let max_height = ((work.1 - WORK_AREA_MARGIN * 2.0 - FRAME_HEIGHT_RESERVE).max(1.0)
        * MAX_WORK_AREA_RATIO)
        .floor();
    (
        size.0.min(max_width).max(min.0),
        size.1.min(max_height).max(min.1),
    )
}

fn monitor_logical_work_area(app: &AppHandle) -> Option<WorkArea> {
    let monitor = app.primary_monitor().ok().flatten()?;
    let scale = monitor.scale_factor();
    if scale <= 0.0 {
        return None;
    }
    let area = monitor.work_area();
    Some(WorkArea {
        origin: (
            f64::from(area.position.x) / scale,
            f64::from(area.position.y) / scale,
        ),
        size: (
            f64::from(area.size.width) / scale,
            f64::from(area.size.height) / scale,
        ),
    })
}

fn build_window(
    app: &AppHandle,
    kind: WindowKind,
    label: &str,
    smoke_secondary: bool,
) -> Result<WebviewWindow, String> {
    let work_area = monitor_logical_work_area(app);
    let min = kind.min_inner_size();
    let remembered = remembered_geometry(app);
    let preferred = remembered.map_or_else(|| kind.inner_size(), WindowGeometry::size);
    let size = work_area
        .map(|w| clamp_size(w.size, preferred, min))
        .unwrap_or(preferred);
    let title = if kind == WindowKind::Start {
        "EasyOffice".to_string()
    } else {
        window_title("", false)
    };
    let mut builder =
        WebviewWindowBuilder::new(app, label, WebviewUrl::App(window_url(kind, label).into()))
            .title(title)
            .inner_size(size.0, size.1)
            .min_inner_size(min.0, min.1)
            .on_navigation(crate::is_app_origin)
            .on_page_load(page_load_ends_close_prompt)
            .initialization_script(shell_token_script(app, label)?)
            .on_web_resource_request(|request, response| {
                crate::studio_frame::harden_studio_response(&request, response);
            });
    builder = match (remembered_origin(app, remembered), work_area) {
        (Some((x, y)), _) => builder.position(x, y),
        (None, Some(m)) => {
            let index = locked(&app.state::<AppWindows>().registry).document_window_count();
            let (x, y) = cascade_origin(m.size, size, index);
            builder.position(m.origin.0 + x, m.origin.1 + y)
        }
        (None, None) => builder.center(),
    };
    if remembered.is_some_and(|g| g.maximized) {
        builder = builder.maximized(true);
    }
    if smoke_secondary {
        builder = builder.initialization_script(SMOKE_SECONDARY_SCRIPT);
    } else if label == PRIMARY_LABEL && app.try_state::<SmokeProbe>().is_some() {
        builder = builder.initialization_script(crate::smoke::SMOKE_INIT_SCRIPT);
    }
    let window = builder.build().map_err(|err| {
        eprintln!("window: build failed: {err}");
        "새 창을 만들지 못했습니다.".to_string()
    })?;
    attach_handlers(app, &window);
    Ok(window)
}

/// A page that (re)loads has lost any save prompt it showed, so the shell forgets it too: a stale
/// confirmation would keep the window open for a prompt nobody can see.
fn page_load_ends_close_prompt(
    window: WebviewWindow,
    payload: tauri::webview::PageLoadPayload<'_>,
) {
    if payload.event() == PageLoadEvent::Started {
        let app = window.app_handle();
        locked(&app.state::<AppWindows>().registry).end_close_prompt(window.label());
    }
}

/// The script that gives the window's top-level document its command token (`shell_caller.rs`).
fn shell_token_script(app: &AppHandle, label: &str) -> Result<String, String> {
    app.state::<shell_caller::ShellTokens>()
        .issue(label)
        .map(|token| shell_caller::init_script(&token))
        .map_err(|err| format!("새 창의 호출 토큰을 만들지 못했습니다: {err}"))
}

/// Creates a window that has already been registered under `label`.
pub fn create_registered_window(
    app: &AppHandle,
    kind: WindowKind,
    label: &str,
) -> Result<(), String> {
    let secondary = app.try_state::<SmokeProbe>().is_some() && label != PRIMARY_LABEL;
    build_window(app, kind, label, secondary).map(|window| {
        let _ = window.set_focus();
    })
}

/// The first window: the start screen, or the HWP window of `--smoke`. Uses the fixed primary label.
pub fn create_primary_window(app: &AppHandle, kind: WindowKind) -> Result<(), String> {
    let state = app.state::<AppWindows>();
    locked(&state.registry).register(PRIMARY_LABEL, kind);
    create_registered_window(app, kind, PRIMARY_LABEL)
}

/// Registers and creates a window of `kind` with a fresh label.
pub fn create_new_window(app: &AppHandle, kind: WindowKind) -> Result<String, String> {
    let state = app.state::<AppWindows>();
    let label = state.next_label();
    locked(&state.registry).register(&label, kind);
    if let Err(err) = create_registered_window(app, kind, &label) {
        locked(&state.registry).remove(&label);
        return Err(err);
    }
    Ok(label)
}

/// The remembered geometry, except in `--smoke`, whose windows must not depend on earlier runs.
fn remembered_geometry(app: &AppHandle) -> Option<WindowGeometry> {
    if app.try_state::<SmokeProbe>().is_some() {
        return None;
    }
    *locked(&app.state::<AppWindows>().geometry)
}

/// The remembered position is reused only by the first window (later ones cascade) and only while
/// its title bar would land on a connected monitor.
fn remembered_origin(app: &AppHandle, remembered: Option<WindowGeometry>) -> Option<(f64, f64)> {
    let geometry = remembered?;
    if locked(&app.state::<AppWindows>().registry).len() > 1 {
        return None;
    }
    let monitors: Vec<MonitorRect> = app
        .available_monitors()
        .ok()?
        .iter()
        .filter_map(|m| {
            let scale = m.scale_factor();
            let origin = m.position().to_logical::<f64>(scale);
            let size = m.size().to_logical::<f64>(scale);
            WindowGeometry::from_logical((size.width, size.height), (origin.x, origin.y), false)
                .map(|g| MonitorRect {
                    x: g.x,
                    y: g.y,
                    width: i32::from(g.width),
                    height: i32::from(g.height),
                })
        })
        .collect();
    geometry.visible_origin(&monitors)
}

/// Records the window's normal-state geometry in memory (no disk write per move or resize). While
/// maximized only the flag changes, so restoring from maximized returns to the earlier size.
fn note_geometry(app: &AppHandle, window: &WebviewWindow) {
    if app.try_state::<SmokeProbe>().is_some() || window.is_minimized().unwrap_or(true) {
        return;
    }
    let maximized = window.is_maximized().unwrap_or(false);
    let state = app.state::<AppWindows>();
    let mut stored = locked(&state.geometry);
    if maximized {
        if let Some(geometry) = stored.as_mut() {
            geometry.maximized = true;
        }
        return;
    }
    let Ok(scale) = window.scale_factor() else {
        return;
    };
    let (Ok(size), Ok(origin)) = (window.inner_size(), window.outer_position()) else {
        return;
    };
    let size = size.to_logical::<f64>(scale);
    let origin = origin.to_logical::<f64>(scale);
    if let Some(geometry) =
        WindowGeometry::from_logical((size.width, size.height), (origin.x, origin.y), false)
    {
        *stored = Some(geometry);
    }
}

/// One small write when a window closes.
fn persist_geometry(app: &AppHandle) {
    let state = app.state::<AppWindows>();
    let (Some(dir), Some(geometry)) = (&state.recents_dir, *locked(&state.geometry)) else {
        return;
    };
    let _ = geometry.save(dir);
}

fn attach_handlers(app: &AppHandle, window: &WebviewWindow) {
    let app = app.clone();
    let label = window.label().to_string();
    window.on_window_event(move |event| match event {
        WindowEvent::CloseRequested { api, .. } => {
            let action =
                locked(&app.state::<AppWindows>().registry).begin_close(&label, Instant::now());
            if action == CloseAction::CloseNow {
                return;
            }
            api.prevent_close();
            let Some(window) = app.get_webview_window(&label) else {
                return;
            };
            if action == CloseAction::KeepPrompt {
                raise(&window);
            }
            // Also while the prompt is kept: the page joins the prompt it has open (and confirms it
            // again) or shows it anew if it lost it, e.g. after a reload.
            let _ = window.eval(CLOSE_REQUEST_SCRIPT);
        }
        WindowEvent::Focused(true) => crate::menu_state::refresh_logged(&app),
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
            if let Some(window) = app.get_webview_window(&label) {
                note_geometry(&app, &window);
            }
        }
        WindowEvent::Destroyed => {
            easyoffice_app::close_sheet_sessions_of(&label);
            app.state::<shell_caller::ShellTokens>().forget(&label);
            app.state::<std::sync::Arc<crate::pdf_job::PdfExports>>()
                .forget(&label);
            app.state::<std::sync::Arc<crate::pdf_edit_job::PdfEdits>>()
                .forget(&label);
            app.state::<std::sync::Arc<crate::font_catalog::SystemFonts>>()
                .forget(&label);
            persist_geometry(&app);
            let state = app.state::<AppWindows>();
            locked(&state.registry).remove(&label);
            locked(&state.pending).discard(&label);
            locked(&state.files).untrack(&label);
            crate::menu_state::forget(&app, &label);
            crate::app_quit::window_destroyed(&app, &label);
            if let Some(probe) = app.try_state::<SmokeProbe>() {
                probe.window_closed(&label);
            }
        }
        WindowEvent::DragDrop(DragDropEvent::Enter { paths, .. }) => {
            if paths.iter().any(|p| WindowKind::for_path(p).is_some()) {
                drag_signal(&app, &label, crate::open_feedback::DRAG_ON_SCRIPT);
            }
        }
        WindowEvent::DragDrop(DragDropEvent::Leave) => {
            drag_signal(&app, &label, crate::open_feedback::DRAG_OFF_SCRIPT);
        }
        WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) => {
            drag_signal(&app, &label, crate::open_feedback::DRAG_OFF_SCRIPT);
            open_dropped_paths(&app, paths.clone());
        }
        _ => {}
    });
}

/// Tells the window's page that a droppable file entered or left it (PD-124).
fn drag_signal(app: &AppHandle, label: &str, script: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.eval(script);
    }
}

fn open_dropped_paths(app: &AppHandle, paths: Vec<PathBuf>) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        open_files(&app, &paths);
    });
}

fn focus_window(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// The file an open names. `canonical` is where it really is: the registry key, the tracked path
/// and the file that is read and saved. `shown` is the path the document goes by: its title, the
/// file type the page reads it as and the recent list. They differ only when the opened path is a
/// symbolic link, which keeps its own name.
struct OpenTarget {
    canonical: PathBuf,
    shown: PathBuf,
}

impl OpenTarget {
    fn of(path: &Path) -> std::io::Result<Self> {
        let canonical = fs::canonicalize(path)?;
        let is_link = fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink());
        let shown = if is_link {
            std::path::absolute(path).unwrap_or_else(|_| canonical.clone())
        } else {
            canonical.clone()
        };
        Ok(Self { canonical, shown })
    }
}

/// Opens the document at `path` in its own window, or focuses the window that already has it.
pub fn open_path(app: &AppHandle, path: &Path) -> Result<OpenOutcome, String> {
    let OpenTarget { canonical, shown } =
        OpenTarget::of(path).map_err(|err| open_failure("파일을 찾을 수 없습니다", path, &err))?;
    let name = display_name(&shown);
    let kind = WindowKind::for_path(&shown).ok_or_else(|| format!("{UNSUPPORTED} ({name})"))?;
    let state = app.state::<AppWindows>();
    let key = path_key(&canonical);
    let existing = locked(&state.registry).find_by_path(&key).map(String::from);
    if let Some(label) = existing {
        focus_window(app, &label);
        close_empty_start_windows(app);
        return Ok(OpenOutcome::FOCUSED);
    }
    let meta = fs::metadata(&canonical)
        .map_err(|err| open_failure("파일 정보를 읽지 못했습니다", &canonical, &err))?;
    check_open_size(kind, meta.len(), &name)?;
    let bytes = fs::read(&canonical)
        .map_err(|err| open_failure("파일을 읽지 못했습니다", &canonical, &err))?;
    // Metadata from before the read, hash of the bytes the editor receives: a change in between
    // makes the next save ask instead of silently overwriting.
    let fingerprint = Fingerprint::from_metadata_and_bytes(&meta, &bytes);
    let label = state.next_label();
    {
        let mut registry = locked(&state.registry);
        registry.register(&label, kind);
        if let Err(owner) = registry.claim_path(&label, key) {
            registry.remove(&label);
            drop(registry);
            focus_window(app, &owner);
            close_empty_start_windows(app);
            return Ok(OpenOutcome::FOCUSED);
        }
    }
    locked(&state.files).track(&label, canonical.clone(), fingerprint);
    locked(&state.pending).push(
        &label,
        PendingOpen {
            name: name.clone(),
            bytes,
        },
    );
    if let Err(err) = create_registered_window(app, kind, &label) {
        locked(&state.registry).remove(&label);
        locked(&state.pending).discard(&label);
        locked(&state.files).untrack(&label);
        eprintln!("open: {err}");
        return Err(format!("문서 창을 만들지 못했습니다 ({name})"));
    }
    state.remember(app, &shown);
    crate::os_recent::note_recent_document(app, &shown);
    close_empty_start_windows(app);
    Ok(OpenOutcome::OPENED)
}

/// The file is read only when its size fits what the page opens of this kind (PD-177).
fn check_open_size(kind: WindowKind, size: u64, name: &str) -> Result<(), String> {
    let limit = kind.max_open_bytes();
    if size == 0 || size > limit {
        return Err(format!(
            "열 수 없는 파일 크기입니다 ({name}): 빈 파일이거나 {}MiB를 넘습니다",
            limit / 1_048_576
        ));
    }
    Ok(())
}

/// A user sentence naming the file; the operating system's own error text stays in the log.
fn open_failure(reason: &str, path: &Path, err: &std::io::Error) -> String {
    eprintln!("open: {reason} ({}): {err}", display_name(path));
    format!("{reason} ({})", display_name(path))
}

/// A document window exists now, so a start window with nothing in it has done its job (the page
/// closes its own start window after the opens it starts; this covers the ones Rust starts).
fn close_empty_start_windows(app: &AppHandle) {
    let labels = locked(&app.state::<AppWindows>().registry).empty_start_labels();
    for window in labels
        .iter()
        .filter_map(|label| app.get_webview_window(label))
    {
        let _ = window.close();
    }
}

/// A document saved under a new path belongs to that path from now on (re-opening focuses it).
pub fn note_saved_path(
    app: &AppHandle,
    label: &str,
    path: &Path,
    fingerprint: Option<Fingerprint>,
) {
    let Ok(OpenTarget { canonical, shown }) = OpenTarget::of(path) else {
        return;
    };
    let state = app.state::<AppWindows>();
    // A path another window shows keeps its owner; the saving window simply stays untracked.
    let claimed = locked(&state.registry)
        .claim_path(label, path_key(&canonical))
        .is_ok();
    match fingerprint {
        Some(fingerprint) if claimed => {
            locked(&state.files).track(label, canonical.clone(), fingerprint)
        }
        _ => locked(&state.files).untrack(label),
    }
    state.remember(app, &shown);
    crate::os_recent::note_recent_document(app, &shown);
}

/// The file this window's document came from (or was last saved to), if any. Rust-internal.
pub fn tracked_file(app: &AppHandle, label: &str) -> Option<TrackedFile> {
    locked(&app.state::<AppWindows>().files).get(label)
}

/// After an in-place write: the file now looks like this, so the next save compares against it.
/// Without a fingerprint the old one stays, so the next save asks instead of assuming.
pub fn note_written(app: &AppHandle, label: &str, fingerprint: Option<Fingerprint>) {
    if let Some(fingerprint) = fingerprint {
        locked(&app.state::<AppWindows>().files).set_fingerprint(label, fingerprint);
    }
}

/// Command-line arguments that name an existing file, in order; opening reports the ones of an
/// unsupported kind. Everything else (flags such as `--smoke`, missing paths, directories) is ignored.
pub fn launch_files(args: impl Iterator<Item = String>) -> Vec<PathBuf> {
    args.map(PathBuf::from)
        .filter(|path| path.is_file())
        .collect()
}

/// The files of a second launch: `argv[0]` is the executable, the rest are the launch arguments.
pub fn second_instance_files(argv: Vec<String>) -> Vec<PathBuf> {
    launch_files(argv.into_iter().skip(1))
}

/// Opens every file through `open` (the shared open queue in production) and hands each failure to
/// `fail`. Returns whether any file was opened in a new window or focused in the window that
/// already shows it.
pub fn route_launch_files(
    files: &[PathBuf],
    mut open: impl FnMut(&Path) -> Result<OpenOutcome, String>,
    mut fail: impl FnMut(&str),
) -> bool {
    let mut handled = false;
    for path in files {
        match open(path) {
            Ok(_) => handled = true,
            Err(message) => fail(&message),
        }
    }
    handled
}

/// Opens each file in its own window and tells the user about every one that failed.
fn open_files(app: &AppHandle, files: &[PathBuf]) -> bool {
    route_launch_files(
        files,
        |path| open_path(app, path),
        |message| report_open_failure(app, message),
    )
}

/// Files named on the command line each open in their own window. Returns whether any did.
pub fn open_startup_files(app: &AppHandle, args: impl Iterator<Item = String>) -> bool {
    open_files(app, &launch_files(args))
}

/// The existing local files among `file:` URLs handed over by the macOS "open" Apple Event.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn file_url_paths(urls: &[tauri::Url]) -> Vec<PathBuf> {
    urls.iter()
        .filter_map(|url| url.to_file_path().ok())
        .filter(|path| path.is_file())
        .collect()
}

/// Opens the files Finder handed to the app (macOS) through the same queue as every other launch.
#[cfg(target_os = "macos")]
pub fn open_urls(app: &AppHandle, urls: Vec<tauri::Url>) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        open_files(&app, &file_url_paths(&urls));
    });
}

/// A second launch (file association, shell "open with", taskbar) is handed to this running process:
/// its files open through the same queue as the start window, or the window that has a file
/// already comes to the front. Without files, the existing window is simply focused.
pub fn handle_second_instance(app: &AppHandle, argv: Vec<String>) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let files = second_instance_files(argv);
        let handled = open_files(&app, &files);
        if !handled && files.is_empty() {
            focus_existing_window(&app);
        }
    });
}

fn focus_existing_window(app: &AppHandle) {
    let label = if app.get_webview_window(PRIMARY_LABEL).is_some() {
        Some(PRIMARY_LABEL.to_string())
    } else {
        app.webview_windows().keys().min().cloned()
    };
    if let Some(label) = label {
        focus_window(app, &label);
    }
}

fn parse_document_kind(kind: &str) -> Result<WindowKind, WindowError> {
    match WindowKind::parse(kind) {
        Some(WindowKind::Start) | None => Err(WindowError::new(format!(
            "알 수 없는 문서 종류입니다: {kind}"
        ))),
        Some(parsed) => Ok(parsed),
    }
}

#[tauri::command]
pub async fn open_document_window(
    _caller: ShellCaller,
    app: AppHandle,
    kind: String,
) -> Result<OpenOutcome, WindowError> {
    let kind = parse_document_kind(&kind)?;
    create_new_window(&app, kind).map_err(WindowError::new)?;
    Ok(OpenOutcome::OPENED)
}

/// Shows the native open dialog, reads the chosen file in Rust and opens it in a new window. The
/// page learns only whether a window was opened, focused, or the dialog was cancelled.
#[tauri::command]
pub async fn open_file_in_new_window(
    _caller: ShellCaller,
    app: AppHandle,
    window: WebviewWindow,
) -> Result<OpenOutcome, WindowError> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog()
        .file()
        .set_parent(&window)
        .set_title("문서 열기")
        .add_filter(
            "문서",
            &[
                "docx", "doc", "odt", "rtf", "md", "markdown", "txt", "pages", "hwp", "hwpx",
                "hwt", "hwtx", "xlsx", "xls", "ods", "csv", "tsv", "pptx", "ppt", "ppsx", "odp",
                "pdf",
            ],
        )
        .pick_file(move |picked| {
            let _ = tx.send(picked);
        });
    let picked = tauri::async_runtime::spawn_blocking(move || rx.recv().ok().flatten())
        .await
        .map_err(|err| WindowError::new(format!("열기 대화상자를 열지 못했습니다: {err}")))?;
    let Some(picked) = picked else {
        return Ok(OpenOutcome::CANCELLED);
    };
    let path = picked
        .into_path()
        .map_err(|err| WindowError::new(format!("선택한 파일을 확인할 수 없습니다: {err}")))?;
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || open_path(&handle, &path))
        .await
        .map_err(|err| WindowError::new(format!("파일을 열지 못했습니다: {err}")))?
        .map_err(WindowError::new)
}

/// The bytes and display name queued for this window, once; `null` when nothing is pending.
#[tauri::command]
pub fn take_pending_open(
    _caller: ShellCaller,
    app: AppHandle,
    window: WebviewWindow,
) -> Option<PendingOpenView> {
    if !is_window_label(window.label()) {
        return None;
    }
    let state = app.state::<AppWindows>();
    let taken = locked(&state.pending).take(window.label());
    taken.map(|open| PendingOpenView::from_pending(&open))
}

/// The page could not show the file this window was opened for (too large for its kind, damaged,
/// a password asked for and not given): the shell forgets the file, so a save from the empty
/// window asks where to write instead of replacing the original, and the file can be opened again.
/// Takes no argument: what is forgotten is decided here, by the window.
#[tauri::command]
pub fn abandon_tracked_open(_caller: ShellCaller, app: AppHandle, window: WebviewWindow) {
    app.state::<AppWindows>().abandon_open(window.label());
}

#[tauri::command]
pub fn report_window_state(
    _caller: ShellCaller,
    app: AppHandle,
    window: WebviewWindow,
    name: String,
    dirty: bool,
) {
    if !is_window_label(window.label()) {
        return;
    }
    let state = app.state::<AppWindows>();
    let title = locked(&state.registry).set_state(window.label(), &name, dirty);
    if let Some(title) = title {
        let _ = window.set_title(&title);
    }
}

/// The page's close prompt ended with the window staying open (cancel, or the save failed): the
/// window asks again on the next close request, and a running menu quit stops.
#[tauri::command]
pub fn end_close_prompt(_caller: ShellCaller, app: AppHandle, window: WebviewWindow) {
    locked(&app.state::<AppWindows>().registry).end_close_prompt(window.label());
    app.state::<crate::app_quit::AppQuit>().cancel();
}

/// The page confirms that its save prompt is on screen. Until then a second close request is
/// treated as coming from a hung page and closes the window (`CLOSE_ACK_TIMEOUT`, PD-113). The page
/// sends it again whenever the shell asks while its prompt is open.
#[tauri::command]
pub fn ack_close_prompt(_caller: ShellCaller, app: AppHandle, window: WebviewWindow) {
    locked(&app.state::<AppWindows>().registry).ack_close_prompt(window.label());
}

fn raise(window: &WebviewWindow) {
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

/// Brings a window to the front and asks it to close, which runs its own close confirmation. A
/// window already showing its save prompt only stays in front (`CloseRequested` keeps the prompt);
/// one whose page never showed it closes after the timeout. Returns false when the window could
/// not be asked.
pub fn raise_and_close(window: &WebviewWindow) -> bool {
    raise(window);
    window.close().is_ok()
}

/// Destroys the calling window without another close request (the page already asked the user).
#[tauri::command]
pub fn close_current_window(_caller: ShellCaller, window: WebviewWindow) {
    let _ = window.destroy();
}

#[tauri::command]
pub fn list_recent_documents(
    _caller: ShellCaller,
    app: AppHandle,
) -> Vec<crate::recent::RecentView> {
    locked(&app.state::<AppWindows>().recents).views()
}

#[tauri::command]
pub fn clear_recent_documents(_caller: ShellCaller, app: AppHandle) -> Result<(), WindowError> {
    app.state::<AppWindows>().clear_recents().map_err(|err| {
        eprintln!("recent: clear failed: {err}");
        WindowError::new("최근 문서를 지우지 못했습니다.")
    })?;
    crate::recent::notify_changed(&app);
    Ok(())
}

#[tauri::command]
pub async fn open_recent_document(
    _caller: ShellCaller,
    app: AppHandle,
    id: String,
) -> Result<OpenOutcome, WindowError> {
    let path = locked(&app.state::<AppWindows>().recents)
        .path_for(&id)
        .ok_or_else(|| WindowError::new("최근 문서 목록에 없는 항목입니다."))?;
    let handle = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || open_path(&handle, &path))
        .await
        .map_err(|err| WindowError::new(format!("파일을 열지 못했습니다: {err}")))?;
    result.map_err(WindowError::new)
}

#[cfg(test)]
mod tests;
