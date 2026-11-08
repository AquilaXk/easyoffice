//! Tauri wiring of the window model (PD-088): window creation, the native open dialog, drag and
//! drop, the close flow and the window commands the WebView may call. Every command here is narrow:
//! none takes or returns a file system path. Paths are read, canonicalized and tracked in Rust only.

use crate::recent::RecentList;
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
use tauri::{
    AppHandle, DragDropEvent, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_dialog::DialogExt;

/// Upper bound for one opened document (bytes read in Rust and handed to the new window).
pub const MAX_OPEN_BYTES: u64 = 256 * 1_048_576;
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

    /// The path behind an opaque recent id; it never leaves Rust.
    pub fn recent_path(&self, id: &str) -> Option<PathBuf> {
        locked(&self.recents).path_for(id)
    }

    fn next_label(&self) -> String {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        new_label(n, millis)
    }

    fn remember(&self, path: &Path) {
        let mut recents = locked(&self.recents);
        recents.add(path.to_path_buf());
        if let Some(dir) = &self.recents_dir {
            let _ = recents.save(dir);
        }
    }
}

/// Position of the n-th document window: centred on the monitor, shifted by a small cascade step.
pub fn cascade_origin(monitor: (f64, f64), size: (f64, f64), index: usize) -> (f64, f64) {
    let shift = CASCADE_STEP * (index % 8) as f64;
    let x = ((monitor.0 - size.0) / 2.0 + shift).max(0.0);
    let y = ((monitor.1 - size.1) / 2.0 + shift).max(0.0);
    (x, y)
}

/// A window never opens larger than 92% of the monitor, but never below its minimum.
pub fn clamp_size(monitor: (f64, f64), size: (f64, f64), min: (f64, f64)) -> (f64, f64) {
    (
        size.0.min(monitor.0 * 0.92).max(min.0),
        size.1.min(monitor.1 * 0.92).max(min.1),
    )
}

fn monitor_logical_size(app: &AppHandle) -> Option<(f64, f64)> {
    let monitor = app.primary_monitor().ok().flatten()?;
    let scale = monitor.scale_factor();
    if scale <= 0.0 {
        return None;
    }
    let size = monitor.size();
    Some((
        f64::from(size.width) / scale,
        f64::from(size.height) / scale,
    ))
}

fn build_window(
    app: &AppHandle,
    kind: WindowKind,
    label: &str,
    smoke_secondary: bool,
) -> Result<WebviewWindow, String> {
    let monitor = monitor_logical_size(app);
    let min = kind.min_inner_size();
    let remembered = remembered_geometry(app);
    let preferred = remembered.map_or_else(|| kind.inner_size(), WindowGeometry::size);
    let size = monitor
        .map(|m| clamp_size(m, preferred, min))
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
            .on_web_resource_request(|request, response| {
                crate::studio_frame::harden_studio_response(&request, response);
            });
    builder = match (remembered_origin(app, remembered), monitor) {
        (Some((x, y)), _) => builder.position(x, y),
        (None, Some(m)) => {
            let index = locked(&app.state::<AppWindows>().registry).document_window_count();
            let (x, y) = cascade_origin(m, size, index);
            builder.position(x, y)
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
    let window = builder
        .build()
        .map_err(|err| format!("새 창을 만들지 못했습니다: {err}"))?;
    attach_handlers(app, &window);
    Ok(window)
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
            let action = locked(&app.state::<AppWindows>().registry).begin_close(&label);
            if action == CloseAction::AskPage {
                api.prevent_close();
                if let Some(window) = app.get_webview_window(&label) {
                    let _ = window.eval(CLOSE_REQUEST_SCRIPT);
                }
            }
        }
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
            if let Some(window) = app.get_webview_window(&label) {
                note_geometry(&app, &window);
            }
        }
        WindowEvent::Destroyed => {
            persist_geometry(&app);
            let state = app.state::<AppWindows>();
            locked(&state.registry).remove(&label);
            locked(&state.pending).discard(&label);
            locked(&state.files).untrack(&label);
            if let Some(probe) = app.try_state::<SmokeProbe>() {
                probe.window_closed(&label);
            }
        }
        WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) => {
            open_dropped_paths(&app, paths.clone());
        }
        _ => {}
    });
}

fn open_dropped_paths(app: &AppHandle, paths: Vec<PathBuf>) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        for path in paths
            .into_iter()
            .filter(|p| WindowKind::for_path(p).is_some())
        {
            if let Err(message) = open_path(&app, &path) {
                eprintln!("open: {message}");
            }
        }
    });
}

fn focus_window(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Opens the document at `path` in its own window, or focuses the window that already has it.
pub fn open_path(app: &AppHandle, path: &Path) -> Result<OpenOutcome, String> {
    let canonical = fs::canonicalize(path)
        .map_err(|err| format!("파일을 열 수 없습니다 ({}): {err}", display_name(path)))?;
    let kind = WindowKind::for_path(&canonical).ok_or_else(|| {
        format!(
            "지원하지 않는 파일 형식입니다 ({})",
            display_name(&canonical)
        )
    })?;
    let state = app.state::<AppWindows>();
    let key = path_key(&canonical);
    let existing = locked(&state.registry).find_by_path(&key).map(String::from);
    if let Some(label) = existing {
        focus_window(app, &label);
        return Ok(OpenOutcome::FOCUSED);
    }
    let meta =
        fs::metadata(&canonical).map_err(|err| format!("파일 정보를 읽지 못했습니다: {err}"))?;
    let size = meta.len();
    if size == 0 || size > MAX_OPEN_BYTES {
        return Err(format!(
            "열 수 없는 파일 크기입니다 ({}): 빈 파일이거나 256MiB를 넘습니다",
            display_name(&canonical)
        ));
    }
    let bytes = fs::read(&canonical).map_err(|err| {
        format!(
            "파일을 읽지 못했습니다 ({}): {err}",
            display_name(&canonical)
        )
    })?;
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
            return Ok(OpenOutcome::FOCUSED);
        }
    }
    locked(&state.files).track(&label, canonical.clone(), fingerprint);
    locked(&state.pending).push(
        &label,
        PendingOpen {
            name: display_name(&canonical),
            bytes,
        },
    );
    if let Err(err) = create_registered_window(app, kind, &label) {
        locked(&state.registry).remove(&label);
        locked(&state.pending).discard(&label);
        locked(&state.files).untrack(&label);
        return Err(err);
    }
    state.remember(&canonical);
    crate::os_recent::note_recent_document(app, &canonical);
    Ok(OpenOutcome::OPENED)
}

/// A document saved under a new path belongs to that path from now on (re-opening focuses it).
pub fn note_saved_path(
    app: &AppHandle,
    label: &str,
    path: &Path,
    fingerprint: Option<Fingerprint>,
) {
    let Ok(canonical) = fs::canonicalize(path) else {
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
    state.remember(&canonical);
    crate::os_recent::note_recent_document(app, &canonical);
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

/// Command-line arguments that name an existing file of a supported kind, in order. Everything else
/// (flags such as `--smoke`, unknown extensions, directories) is ignored.
pub fn launch_files(args: impl Iterator<Item = String>) -> Vec<PathBuf> {
    args.map(PathBuf::from)
        .filter(|path| WindowKind::for_path(path).is_some() && path.is_file())
        .collect()
}

/// The files of a second launch: `argv[0]` is the executable, the rest are the launch arguments.
pub fn second_instance_files(argv: Vec<String>) -> Vec<PathBuf> {
    launch_files(argv.into_iter().skip(1))
}

/// Opens every file through `open` (the shared open queue in production). Returns whether any file
/// was opened in a new window or focused in the window that already shows it.
pub fn route_launch_files(
    files: &[PathBuf],
    mut open: impl FnMut(&Path) -> Result<OpenOutcome, String>,
) -> bool {
    let mut handled = false;
    for path in files {
        match open(path) {
            Ok(_) => handled = true,
            Err(message) => eprintln!("open: {message}"),
        }
    }
    handled
}

/// Files named on the command line each open in their own window. Returns whether any did.
pub fn open_startup_files(app: &AppHandle, args: impl Iterator<Item = String>) -> bool {
    route_launch_files(&launch_files(args), |path| open_path(app, path))
}

/// The local files among `file:` URLs handed over by the macOS "open" Apple Event.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn file_url_paths(urls: &[tauri::Url]) -> Vec<PathBuf> {
    urls.iter()
        .filter_map(|url| url.to_file_path().ok())
        .filter(|path| WindowKind::for_path(path).is_some() && path.is_file())
        .collect()
}

/// Opens the files Finder handed to the app (macOS) through the same queue as every other launch.
#[cfg(target_os = "macos")]
pub fn open_urls(app: &AppHandle, urls: Vec<tauri::Url>) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        route_launch_files(&file_url_paths(&urls), |path| open_path(&app, path));
    });
}

/// A second launch (file association, shell "open with", taskbar) is handed to this running process:
/// its files open through the same queue as the start window, or the window that has a file
/// already comes to the front. Without files, the existing window is simply focused.
pub fn handle_second_instance(app: &AppHandle, argv: Vec<String>) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let files = second_instance_files(argv);
        let handled = route_launch_files(&files, |path| open_path(&app, path));
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
                "docx", "doc", "odt", "rtf", "md", "markdown", "txt", "hwp", "hwpx", "hwt", "hwtx",
                "xlsx", "xls", "ods", "csv", "tsv", "pptx", "ppt", "ppsx", "odp", "pdf",
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
pub fn take_pending_open(app: AppHandle, window: WebviewWindow) -> Option<PendingOpenView> {
    if !is_window_label(window.label()) {
        return None;
    }
    let state = app.state::<AppWindows>();
    let taken = locked(&state.pending).take(window.label());
    taken.map(|open| PendingOpenView::from_pending(&open))
}

#[tauri::command]
pub fn report_window_state(app: AppHandle, window: WebviewWindow, name: String, dirty: bool) {
    if !is_window_label(window.label()) {
        return;
    }
    let state = app.state::<AppWindows>();
    let title = locked(&state.registry).set_state(window.label(), &name, dirty);
    if let Some(title) = title {
        let _ = window.set_title(&title);
    }
}

/// Destroys the calling window without another close request (the page already asked the user).
#[tauri::command]
pub fn close_current_window(window: WebviewWindow) {
    let _ = window.destroy();
}

#[tauri::command]
pub fn list_recent_documents(app: AppHandle) -> Vec<crate::recent::RecentView> {
    locked(&app.state::<AppWindows>().recents).views()
}

#[tauri::command]
pub async fn open_recent_document(app: AppHandle, id: String) -> Result<OpenOutcome, WindowError> {
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
mod tests {
    use super::*;

    #[test]
    fn document_kinds_exclude_the_start_screen_and_unknown_names() {
        assert_eq!(parse_document_kind("sheet"), Ok(WindowKind::Sheet));
        assert!(parse_document_kind("START").is_err());
        assert!(parse_document_kind("../../x").is_err());
    }

    #[test]
    fn windows_cascade_from_the_monitor_centre_and_wrap() {
        let monitor = (1920.0, 1080.0);
        let size = (1280.0, 860.0);
        assert_eq!(cascade_origin(monitor, size, 0), (320.0, 110.0));
        assert_eq!(cascade_origin(monitor, size, 1), (348.0, 138.0));
        assert_eq!(cascade_origin(monitor, size, 8), (320.0, 110.0));
        assert_eq!(cascade_origin((1000.0, 700.0), size, 0), (0.0, 0.0));
    }

    #[test]
    fn size_is_limited_to_the_monitor_but_keeps_the_minimum() {
        let min = (1024.0, 680.0);
        assert_eq!(
            clamp_size((2560.0, 1440.0), (1280.0, 860.0), min),
            (1280.0, 860.0)
        );
        let (w, h) = clamp_size((1366.0, 768.0), (1280.0, 860.0), min);
        assert!(
            (w - 1256.72).abs() < 1e-6 && (h - 706.56).abs() < 1e-6,
            "{w}x{h}"
        );
        assert_eq!(
            clamp_size((1024.0, 600.0), (1280.0, 860.0), min),
            (1024.0, 680.0)
        );
    }

    #[test]
    fn open_outcomes_and_errors_serialize_without_paths() {
        assert_eq!(
            serde_json::to_string(&OpenOutcome::OPENED).unwrap(),
            r#"{"status":"opened"}"#
        );
        assert_eq!(
            serde_json::to_string(&OpenOutcome::FOCUSED).unwrap(),
            r#"{"status":"focused"}"#
        );
        assert_eq!(
            serde_json::to_string(&OpenOutcome::CANCELLED).unwrap(),
            r#"{"status":"cancelled"}"#
        );
        let err = serde_json::to_string(&WindowError::new("bad")).unwrap();
        assert_eq!(err, r#"{"status":"error","message":"bad"}"#);
    }

    /// Structural guard: no window command takes or returns a file system path.
    #[test]
    fn window_command_signatures_never_carry_a_path() {
        let source = include_str!("window_commands.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        let mut checked = 0;
        for block in production.split("#[tauri::command]").skip(1) {
            let signature = block.split('{').next().unwrap();
            let lowered = signature.to_lowercase();
            assert!(
                !lowered.contains("path"),
                "command signature mentions a path: {signature}"
            );
            assert!(
                !lowered.contains("pathbuf"),
                "command signature uses PathBuf: {signature}"
            );
            checked += 1;
        }
        assert_eq!(checked, 7, "every window command is covered");
    }

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "easyoffice-desktop-launch-{tag}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn second_instance_args_are_routed_to_the_open_queue() {
        let dir = scratch_dir("second");
        let a = dir.join("a.docx");
        let b = dir.join("b.hwpx");
        fs::write(&a, b"x").unwrap();
        fs::write(&b, b"x").unwrap();
        let argv = vec![
            "C:\\Program Files\\EasyOffice\\EasyOffice.exe".to_string(),
            "--flag".to_string(),
            a.display().to_string(),
            dir.join("missing.docx").display().to_string(),
            dir.join("notes.exe").display().to_string(),
            b.display().to_string(),
        ];
        let files = second_instance_files(argv);
        assert_eq!(
            files,
            vec![a.clone(), b.clone()],
            "only existing supported files, exe skipped"
        );
        let mut queued = Vec::new();
        let handled = route_launch_files(&files, |path| {
            queued.push(path.to_path_buf());
            Ok(OpenOutcome::OPENED)
        });
        assert!(handled);
        assert_eq!(queued, vec![a, b]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn finder_open_urls_keep_only_existing_supported_local_files() {
        let dir = std::env::temp_dir().join(format!("eo-urls-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let doc = dir.join("보고서 1.docx");
        fs::write(&doc, b"x").unwrap();
        let exe = dir.join("tool.exe");
        fs::write(&exe, b"x").unwrap();
        let urls = vec![
            tauri::Url::from_file_path(&doc).unwrap(),
            tauri::Url::from_file_path(&exe).unwrap(),
            tauri::Url::from_file_path(dir.join("missing.xlsx")).unwrap(),
            tauri::Url::parse("https://example.com/a.docx").unwrap(),
        ];
        assert_eq!(file_url_paths(&urls), vec![doc]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn launch_routing_reports_whether_anything_was_handled() {
        let none: Vec<PathBuf> = Vec::new();
        assert!(!route_launch_files(&none, |_| Ok(OpenOutcome::OPENED)));
        let files = vec![PathBuf::from("a.docx")];
        assert!(!route_launch_files(&files, |_| Err("nope".into())));
        assert!(route_launch_files(&files, |_| Ok(OpenOutcome::FOCUSED)));
    }

    #[test]
    fn labels_are_unique_and_pass_the_capability_pattern() {
        let state = AppWindows::new(None);
        let a = state.next_label();
        let b = state.next_label();
        assert_ne!(a, b);
        assert!(crate::windows::is_window_label(&a));
        assert!(
            a.starts_with("doc-"),
            "matches the doc-* capability window pattern"
        );
    }

    #[test]
    fn app_state_shares_the_registry_and_queue_by_label() {
        let state = AppWindows::new(None);
        locked(&state.registry).register("doc-1-a", WindowKind::Doc);
        locked(&state.pending).push(
            "doc-1-a",
            PendingOpen {
                name: "a.docx".into(),
                bytes: vec![1],
            },
        );
        assert_eq!(
            locked(&state.pending).take("doc-1-a").unwrap().name,
            "a.docx"
        );
        assert!(locked(&state.pending).take("doc-1-a").is_none());
        assert!(locked(&state.registry).remove("doc-1-a"));
    }
}
