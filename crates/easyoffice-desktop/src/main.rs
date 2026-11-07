//! EasyOffice desktop shell for Windows (WebView2) and macOS (WKWebView): a Tauri v2 window that
//! loads the WebUI from assets embedded in the executable and routes every UI request to `EasyOfficeIpcRouter::dispatch`
//! in-process. No HTTP server, browser, or remote content is involved.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_commands;
mod recent;
mod save_file;
mod smoke;
mod tracked_files;
mod window_commands;
mod windows;

use easyoffice_app::EasyOfficeIpcRouter;
use save_file::{save_in_place, write_document_bytes, InPlaceResult, SaveOutcome, SaveRequest};
use serde::Serialize;
use smoke::{SmokeProbe, SMOKE_TIMEOUT};
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use window_commands::AppWindows;
use windows::WindowKind;

/// Error shape returned to the WebView. It matches the `--serve` HTTP error body
/// (`{"status":"error","message":...}`) so the UI reports both transports identically.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct IpcError {
    status: &'static str,
    message: String,
}

impl IpcError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            status: "error",
            message: message.into(),
        }
    }
}

/// Single engine entry point for the WebView: the same 15 commands the `--serve` mode exposes
/// over `POST /api/ipc`. Runs on a blocking worker so long engine calls never freeze the window.
#[tauri::command]
async fn ipc_dispatch(
    app: AppHandle,
    command: String,
    payload: String,
) -> Result<String, IpcError> {
    let probe_command = command.clone();
    let probe_payload = app.try_state::<SmokeProbe>().map(|_| payload.clone());
    let result = tauri::async_runtime::spawn_blocking(move || {
        EasyOfficeIpcRouter::dispatch(&command, &payload)
    })
    .await
    .map_err(|err| IpcError::new(format!("엔진 작업을 실행하지 못했습니다: {err}")))?
    .map_err(|err| IpcError::new(err.to_string()));
    if let Some(probe) = app.try_state::<SmokeProbe>() {
        probe.observe(
            &probe_command,
            probe_payload.as_deref().unwrap_or_default(),
            result.is_ok(),
        );
    }
    result
}

/// Saves converted document bytes to a location the user picks in the native save dialog.
/// The WebView never supplies a file-system path: it only proposes a file name, and the path
/// comes from the dialog shown by this process.
#[tauri::command]
async fn save_document_file(
    app: AppHandle,
    window: WebviewWindow,
    file_name: String,
    base64: String,
    in_place: Option<bool>,
) -> Result<SaveOutcome, IpcError> {
    let request = SaveRequest::parse(&file_name, &base64).map_err(IpcError::new)?;
    if in_place == Some(true) {
        // A document that came from (or was saved to) a file with the same format is written back to
        // that file; everything else falls through to the save dialog.
        let tracked = window_commands::tracked_file(&app, window.label())
            .filter(|tracked| tracked.extension_matches(request.extension()));
        if let Some(tracked) = tracked {
            return save_tracked(&app, &window, tracked, request).await;
        }
    }
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog()
        .file()
        .set_parent(&window)
        .set_title("문서 저장")
        .set_file_name(request.file_name())
        .add_filter(request.filter_label(), &[request.extension()])
        .save_file(move |picked| {
            let _ = tx.send(picked);
        });
    let picked = tauri::async_runtime::spawn_blocking(move || rx.recv().ok().flatten())
        .await
        .map_err(|err| IpcError::new(format!("저장 대화상자를 열지 못했습니다: {err}")))?;
    let Some(picked) = picked else {
        return Ok(SaveOutcome::cancelled());
    };
    let path = picked
        .into_path()
        .map_err(|err| IpcError::new(format!("저장 위치를 확인할 수 없습니다: {err}")))?;
    let written = tauri::async_runtime::spawn_blocking(move || {
        write_document_bytes(&path, request.bytes()).map(|n| (path, n))
    })
    .await
    .map_err(|err| IpcError::new(format!("파일을 쓰지 못했습니다: {err}")))?
    .map_err(IpcError::new)?;
    window_commands::note_saved_path(&app, window.label(), &written.0);
    Ok(SaveOutcome::saved(&written.0, written.1))
}

/// Writes the document back to its own file. If the file changed on disk since it was opened, a
/// native dialog asks before overwriting; declining leaves the file alone and reports "cancelled".
async fn save_tracked(
    app: &AppHandle,
    window: &WebviewWindow,
    tracked: tracked_files::TrackedFile,
    request: SaveRequest,
) -> Result<SaveOutcome, IpcError> {
    let dialog_app = app.clone();
    let parent = window.clone();
    let path = tracked.path.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        save_in_place(&path, &tracked.fingerprint, request.bytes(), || {
            dialog_app
                .dialog()
                .message(
                    "이 파일이 열린 뒤 다른 프로그램에서 변경되었습니다. 지금 저장하면 그 변경 내용을 덮어씁니다.",
                )
                .title("파일이 변경되었습니다")
                .kind(MessageDialogKind::Warning)
                .parent(&parent)
                .buttons(MessageDialogButtons::OkCancelCustom(
                    "덮어쓰기".to_string(),
                    "취소".to_string(),
                ))
                .blocking_show()
        })
    })
    .await
    .map_err(|err| IpcError::new(format!("파일을 쓰지 못했습니다: {err}")))?
    .map_err(IpcError::new)?;
    match result {
        InPlaceResult::Declined => Ok(SaveOutcome::cancelled()),
        InPlaceResult::Saved { bytes } => {
            window_commands::note_written(app, window.label(), &tracked.path);
            Ok(SaveOutcome::saved(&tracked.path, bytes))
        }
    }
}

/// Only the embedded app origin may be displayed; any other navigation is refused.
pub(crate) fn is_app_origin(url: &tauri::Url) -> bool {
    url.scheme() == "tauri" || url.host_str() == Some("tauri.localhost")
}

fn main() {
    let smoke_mode = std::env::args().skip(1).any(|arg| arg == "--smoke");
    let mut builder = tauri::Builder::default();
    if !smoke_mode {
        // First plugin, as the plugin requires. A second launch never starts another process: its
        // arguments reach this one through the open queue (the smoke run must not collide with a
        // running instance, so it skips this).
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            window_commands::handle_second_instance(app, argv);
        }));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            ipc_dispatch,
            save_document_file,
            window_commands::open_document_window,
            window_commands::open_file_in_new_window,
            window_commands::take_pending_open,
            window_commands::report_window_state,
            window_commands::close_current_window,
            window_commands::list_recent_documents,
            window_commands::open_recent_document,
        ])
        .setup(move |app| {
            app.manage(AppWindows::new(app.path().app_data_dir().ok()));
            let handle = app.handle().clone();
            if smoke_mode {
                app.manage(SmokeProbe::start(SMOKE_TIMEOUT));
                // The smoke run drives the HWP editor, so its first window is an HWP document window.
                window_commands::create_primary_window(&handle, WindowKind::Hwp)
                    .map_err(std::io::Error::other)?;
            } else if !window_commands::open_startup_files(&handle, std::env::args().skip(1)) {
                window_commands::create_primary_window(&handle, WindowKind::Start)
                    .map_err(std::io::Error::other)?;
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("EasyOffice desktop shell failed to start")
        .run(handle_run_event);
}

/// Application-level events. On macOS a Finder "open" (double click, drag onto the Dock icon,
/// "Open With") reaches a running or starting app as an Apple Event, not as a command-line
/// argument, and Cmd+Q must go through each window's own save prompt instead of ending the process.
fn handle_run_event(app: &AppHandle, event: tauri::RunEvent) {
    match event {
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Opened { urls } => window_commands::open_urls(app, urls),
        #[cfg(target_os = "macos")]
        tauri::RunEvent::ExitRequested {
            code: None, api, ..
        } => {
            let windows = app.webview_windows();
            if !windows.is_empty() {
                api.prevent_exit();
                for window in windows.values() {
                    let _ = window.close();
                }
            }
        }
        _ => {
            let _ = app;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipc_error_serializes_like_http_error_body() {
        let json = serde_json::to_string(&IpcError::new("bad")).unwrap();
        assert_eq!(json, r#"{"status":"error","message":"bad"}"#);
    }

    #[test]
    fn navigation_is_limited_to_embedded_origin() {
        let ok = |s: &str| is_app_origin(&tauri::Url::parse(s).unwrap());
        assert!(ok("http://tauri.localhost/index.html"));
        assert!(ok("tauri://localhost/index.html"));
        assert!(!ok("https://example.com/"));
        assert!(!ok("http://127.0.0.1:8787/"));
        assert!(!ok("file:///C:/Windows/win.ini"));
    }

    #[test]
    fn capability_grants_exactly_the_app_commands_to_document_windows() {
        let value: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/main-window.json")).unwrap();
        let mut granted: Vec<String> = value["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap().to_string())
            .collect();
        let mut expected: Vec<String> = app_commands::APP_COMMANDS
            .iter()
            .map(|c| format!("allow-{}", c.replace('_', "-")))
            .collect();
        granted.sort();
        expected.sort();
        assert_eq!(
            granted, expected,
            "capability file and command list must match exactly"
        );
        assert!(
            granted.iter().all(|p| !p.contains(':')),
            "no core or plugin permission is granted"
        );
        assert_eq!(value["windows"], serde_json::json!(["main", "doc-*"]));
        assert_eq!(value["local"], serde_json::json!(true));
        assert_eq!(app_commands::APP_COMMANDS.len(), 9);
    }

    #[test]
    fn only_the_main_window_capability_is_attached() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(
            conf["app"]["security"]["capabilities"],
            serde_json::json!(["main-window"])
        );
        assert!(
            conf["app"]["windows"].as_array().unwrap().is_empty(),
            "windows are created in Rust"
        );
    }

    #[test]
    fn macos_bundle_is_configured_unsigned_with_an_icns_icon() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let bundle = &conf["bundle"];
        let targets: Vec<&str> = bundle["targets"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t.as_str())
            .collect();
        assert!(targets.contains(&"app") && targets.contains(&"dmg"));
        assert_eq!(bundle["macOS"]["minimumSystemVersion"], "11.0");
        assert!(
            bundle["macOS"].get("signingIdentity").is_none(),
            "signing is supplied through APPLE_SIGNING_IDENTITY at release time, never committed"
        );
        let icons: Vec<&str> = bundle["icon"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|i| i.as_str())
            .collect();
        assert!(icons.contains(&"icons/icon.icns"));
        let icns = include_bytes!("../icons/icon.icns");
        assert_eq!(&icns[..4], b"icns");
    }

    #[test]
    fn dispatch_reaches_the_shared_router() {
        let boot = EasyOfficeIpcRouter::dispatch("get_workspace_bootstrap", "").unwrap();
        assert!(boot.contains("shortcutModifier"));
        assert!(
            !boot.contains("#5C6BC0") && !boot.contains("ribbonCategories"),
            "the boot JSON carries neither a palette nor ribbon categories"
        );
        assert!(EasyOfficeIpcRouter::dispatch("not_a_command", "").is_err());
    }
}
