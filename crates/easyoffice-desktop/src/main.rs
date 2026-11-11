//! EasyOffice desktop shell for Windows (WebView2) and macOS (WKWebView): a Tauri v2 window that
//! loads the WebUI from assets embedded in the executable and routes every UI request to `EasyOfficeIpcRouter::dispatch`
//! in-process. No HTTP server, browser, or remote content is involved.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_commands;
mod app_quit;
mod confirm_dialog;
mod doc_pdf;
mod doc_pdf_marks;
mod font_catalog;
// Linux webview runtime fixes (PD-125); other OSes compile it only for its decision-logic tests.
#[cfg(any(target_os = "linux", test))]
mod linux_runtime;
mod open_feedback;
mod os_recent;
mod pdf_edit_budget;
mod pdf_edit_build;
mod pdf_edit_engine;
mod pdf_edit_inspect;
mod pdf_edit_job;
mod pdf_edit_os;
mod pdf_edit_overlay;
mod pdf_edit_scan;
mod pdf_edit_session;
mod pdf_edit_supervisor;
mod pdf_edit_wire;
mod pdf_edit_worker;
mod pdf_export;
mod pdf_font_fallbacks;
mod pdf_font_select;
mod pdf_fonts;
mod pdf_job;
mod pdf_pua;
mod pdf_synthetic;
mod pdf_tounicode;
mod preview;
mod print_webview;
mod raster_budget;
mod recent;
mod save_error;
mod save_file;
mod save_thumbnail;
mod save_verify;
mod shell_caller;
mod smoke;
mod smoke_steps;
mod studio_frame;
mod tracked_files;
mod update_check;
mod update_install;
mod window_commands;
mod window_state;
mod windows;

// The reference app installs a native menu bar only on macOS; on Windows and Linux the page draws
// the menu row (PD-121). The macOS bar and the state it shows live in these two modules; the other
// OSes have no bar to keep in step.
#[cfg(target_os = "macos")]
mod menu_state;
#[cfg(target_os = "macos")]
mod native_menu;
#[cfg(not(target_os = "macos"))]
mod menu_state {
    use tauri::AppHandle;

    pub fn refresh_logged(_app: &AppHandle) {}

    pub fn forget(_app: &AppHandle, _label: &str) {}
}

use easyoffice_app::EasyOfficeIpcRouter;
use easyoffice_formats::PdfWriteOptions;
use save_error::SaveError;
use save_file::{save_in_place, write_document_bytes, InPlaceResult, SaveOutcome, SaveRequest};
use serde::Serialize;
use shell_caller::ShellCaller;
use smoke::{SmokeProbe, SMOKE_TIMEOUT};
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tracked_files::DiskState;
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
    _caller: ShellCaller,
    app: AppHandle,
    window: WebviewWindow,
    command: String,
    payload: String,
) -> Result<String, IpcError> {
    let probe_command = command.clone();
    let probe_payload = app.try_state::<SmokeProbe>().map(|_| payload.clone());
    // The window label owns the sheet sessions the command opens; they close with the window.
    let owner = window.label().to_string();
    let font_dirs = pdf_job::pdf_font_dirs(&app);
    let result = tauri::async_runtime::spawn_blocking(move || {
        // A rich document converted to PDF is laid out and drawn here (PD-167); the engine keeps
        // its own PDF as the fallback. The font database is built when a PDF is first drawn.
        let draw = |hwpx: &[u8], options: &PdfWriteOptions| {
            doc_pdf::hwpx_to_pdf(hwpx, options, &pdf_fonts::shared_font_db(&font_dirs))
        };
        EasyOfficeIpcRouter::dispatch_for_owner_with_pdf_renderer(&command, &payload, &owner, &draw)
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
    _caller: ShellCaller,
    app: AppHandle,
    window: WebviewWindow,
    file_name: String,
    base64: String,
    in_place: Option<bool>,
    thumbnail_svg: Option<String>,
) -> Result<SaveOutcome, IpcError> {
    let font_dirs = pdf_job::pdf_font_dirs(&app);
    // Decoding and checking a document of up to 256MiB takes a while: off the async workers. The
    // preview picture (first page as SVG, PD-132) is made here too and never fails the save.
    let request = tauri::async_runtime::spawn_blocking(move || {
        let request = SaveRequest::parse(&file_name, &base64)?;
        Ok::<_, String>(match thumbnail_svg {
            Some(svg) => request.with_thumbnail(&svg, &font_dirs),
            None => request,
        })
    })
    .await
    .map_err(|err| IpcError::new(format!("문서를 확인하지 못했습니다: {err}")))?
    .map_err(IpcError::new)?;
    if in_place == Some(true) {
        // A document that came from (or was saved to) a file with the same format is written back to
        // that file; everything else falls through to the save dialog.
        let tracked = window_commands::tracked_file(&app, window.label())
            .filter(|tracked| tracked.extension_matches(request.extension()));
        if let Some(tracked) = tracked {
            return save_tracked(&app, &window, tracked, request).await;
        }
    }
    save_via_dialog(&app, &window, request).await
}

/// Asks where to save with the native save dialog; `None` when the user cancels. The page proposes
/// the file name only (`file_name`, `label` for the filter), the path stays in Rust.
pub(crate) async fn pick_save_path(
    app: &AppHandle,
    window: &WebviewWindow,
    title: &str,
    file_name: &str,
    label: &str,
    extension: &str,
) -> Result<Option<std::path::PathBuf>, IpcError> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog()
        .file()
        .set_parent(window)
        .set_title(title)
        .set_file_name(file_name)
        .add_filter(label, &[extension])
        .save_file(move |picked| {
            let _ = tx.send(picked);
        });
    let picked = tauri::async_runtime::spawn_blocking(move || rx.recv().ok().flatten())
        .await
        .map_err(|err| IpcError::new(format!("저장 대화상자를 열지 못했습니다: {err}")))?;
    picked
        .map(|picked| {
            picked
                .into_path()
                .map_err(|err| IpcError::new(format!("저장 위치를 확인할 수 없습니다: {err}")))
        })
        .transpose()
}

/// Asks where to save with the native dialog and writes the request there.
async fn save_via_dialog(
    app: &AppHandle,
    window: &WebviewWindow,
    request: SaveRequest,
) -> Result<SaveOutcome, IpcError> {
    let Some(path) = pick_save_path(
        app,
        window,
        "문서 저장",
        request.file_name(),
        &request.filter_label(),
        request.extension(),
    )
    .await?
    else {
        return Ok(SaveOutcome::cancelled());
    };
    let written = tauri::async_runtime::spawn_blocking(move || {
        write_document_bytes(&path, request.bytes()).map(|written| (path, written))
    })
    .await
    .map_err(|err| IpcError::new(format!("파일을 쓰지 못했습니다: {err}")))?
    .map_err(|err| report_save_failure(app, window, &err))?;
    let (path, written) = written;
    window_commands::note_saved_path(app, window.label(), &path, written.fingerprint);
    Ok(SaveOutcome::saved(&path, written.bytes))
}

/// A document that could not be written: the shell shows the reference app's error dialog itself
/// (the page never words it) and the command fails with the reason alone, no path (PD-088).
fn report_save_failure(app: &AppHandle, window: &WebviewWindow, err: &SaveError) -> IpcError {
    save_error::show_dialog(app, window, err);
    err.to_ipc()
}

const EXTERNAL_CHANGE_TITLE: &str = "외부 변경 감지";

/// The reference app's wording for a file that changed behind the app's back, or that was deleted
/// or moved (its reason line, `state.rs` `external_modification_status`); the file is named by its
/// name only (PD-088).
fn external_change_message(file_name: &str, state: DiskState) -> String {
    let reason = if state == DiskState::Missing {
        "원본 파일이 삭제되었거나 이동되었습니다\n"
    } else {
        ""
    };
    format!(
        "원본 파일이 EasyOffice 밖에서 변경되었습니다.\n파일: {file_name}\n{reason}\n그대로 저장하면 외부에서 변경된 내용이 사라질 수 있습니다."
    )
}

/// Writes the document back to its own file. If the file changed on disk since it was opened, or was
/// deleted or moved, a native dialog asks before writing over it; declining leaves the file alone
/// (nothing is recreated) and reports "cancelled".
async fn save_tracked(
    app: &AppHandle,
    window: &WebviewWindow,
    tracked: tracked_files::TrackedFile,
    request: SaveRequest,
) -> Result<SaveOutcome, IpcError> {
    let dialog_app = app.clone();
    let parent = window.clone();
    let path = tracked.path.clone();
    let file_name = windows::display_name(&path);
    let result = tauri::async_runtime::spawn_blocking(move || {
        save_in_place(&path, &tracked.fingerprint, request.bytes(), |state| {
            dialog_app
                .dialog()
                .message(external_change_message(&file_name, state))
                .title(EXTERNAL_CHANGE_TITLE)
                .kind(MessageDialogKind::Warning)
                .parent(&parent)
                .buttons(MessageDialogButtons::OkCancelCustom(
                    "덮어쓰기".to_string(),
                    "저장 취소".to_string(),
                ))
                .blocking_show()
        })
    })
    .await
    .map_err(|err| IpcError::new(format!("파일을 쓰지 못했습니다: {err}")))?
    .map_err(|err| report_save_failure(app, window, &err))?;
    match result {
        InPlaceResult::Declined => Ok(SaveOutcome::cancelled()),
        InPlaceResult::Saved(written) => {
            window_commands::note_written(app, window.label(), written.fingerprint);
            Ok(SaveOutcome::saved(&tracked.path, written.bytes))
        }
    }
}

/// Only the embedded app origin may be displayed; any other navigation is refused.
pub(crate) fn is_app_origin(url: &tauri::Url) -> bool {
    url.scheme() == "tauri" || url.host_str() == Some("tauri.localhost")
}

fn main() {
    // The PDF worker process (PD-168): the shell's own binary, started by the shell with its
    // standard input and output connected to it. It must not start anything of the shell.
    if std::env::args().nth(1).as_deref() == Some(pdf_edit_wire::WORKER_ARG) {
        std::process::exit(pdf_edit_worker::run());
    }
    // Before any webview exists: WebKitGTK reads these variables when it starts.
    #[cfg(target_os = "linux")]
    linux_runtime::apply_linux_runtime_fixes();

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
        // Used from Rust only (update_install.rs): none of its JS commands is granted to a window.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            ipc_dispatch,
            save_document_file,
            pdf_job::pdf_export_start,
            pdf_job::pdf_export_pages,
            pdf_job::pdf_export_finish,
            pdf_edit_job::pdf_edit_open,
            pdf_edit_job::pdf_edit_apply,
            pdf_edit_job::pdf_edit_split_merge,
            pdf_edit_job::pdf_edit_save,
            font_catalog::list_system_fonts,
            font_catalog::read_system_font,
            update_check::get_update_notice,
            update_check::set_update_check,
            update_check::open_release_page,
            update_install::install_update,
            window_commands::open_document_window,
            window_commands::open_file_in_new_window,
            window_commands::take_pending_open,
            window_commands::report_window_state,
            window_commands::close_current_window,
            window_commands::list_recent_documents,
            window_commands::clear_recent_documents,
            #[cfg(target_os = "macos")]
            menu_state::report_menu_state,
            app_quit::quit_app,
            window_commands::end_close_prompt,
            confirm_dialog::confirm_dialog,
            confirm_dialog::confirm_close,
            print_webview::print_webview,
            window_commands::ack_close_prompt,
            window_commands::open_recent_document,
            preview::get_recent_preview,
        ])
        .setup(move |app| {
            app.manage(shell_caller::ShellTokens::default());
            app.manage(AppWindows::new(app.path().app_data_dir().ok()));
            app.manage(app_quit::AppQuit::default());
            app.manage(std::sync::Arc::new(pdf_job::PdfExports::default()));
            app.manage(std::sync::Arc::new(pdf_edit_job::PdfEdits::default()));
            app.manage(std::sync::Arc::new(font_catalog::SystemFonts::default()));
            // macOS gets the native menu bar beside the page's menu row; Windows and Linux have only
            // the page's row, as in the reference app (PD-121).
            #[cfg(target_os = "macos")]
            {
                app.manage(menu_state::MenuStates::default());
                native_menu::install(app)?;
            }
            update_check::start(app.handle(), smoke_mode);
            #[cfg(target_os = "macos")]
            menu_state::refresh(app.handle()).map_err(std::io::Error::other)?;
            let handle = app.handle().clone();
            app.manage(preview::PreviewState::with_font_dirs(
                pdf_job::pdf_font_dirs(&handle),
            ));
            if smoke_mode {
                app.manage(SmokeProbe::start(SMOKE_TIMEOUT));
                smoke::run_rust_steps(&handle);
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
/// argument, and a quit from the Dock must go through each window's own save prompt in turn instead
/// of ending the process.
fn handle_run_event(app: &AppHandle, event: tauri::RunEvent) {
    match event {
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Opened { urls } => window_commands::open_urls(app, urls),
        // Cmd+Q is the application menu's own quit item (`native_menu.rs`; Windows and Linux reach the
        // queue from the page, `app_quit::quit_app`); the Dock's 종료 and a
        // system shutdown arrive here. All of them take the same queue: one window at a time, a
        // cancelled save prompt stops the quit (`app_quit.rs`). An exit with a code (the queue's own
        // `app.exit(0)`) and the end after the last window closed pass.
        #[cfg(target_os = "macos")]
        tauri::RunEvent::ExitRequested {
            code: None, api, ..
        } => {
            if !app.webview_windows().is_empty() {
                api.prevent_exit();
                app_quit::request_app_quit(app);
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
    fn the_external_change_dialog_names_the_file_and_never_a_path() {
        let text = external_change_message("보고서.docx", DiskState::Changed);
        assert_eq!(
            text,
            "원본 파일이 EasyOffice 밖에서 변경되었습니다.\n파일: 보고서.docx\n\n그대로 저장하면 외부에서 변경된 내용이 사라질 수 있습니다."
        );
        assert_eq!(EXTERNAL_CHANGE_TITLE, "외부 변경 감지");
    }

    #[test]
    fn a_missing_original_asks_with_the_reference_apps_reason() {
        let text = external_change_message("보고서.docx", DiskState::Missing);
        assert_eq!(
            text,
            "원본 파일이 EasyOffice 밖에서 변경되었습니다.\n파일: 보고서.docx\n원본 파일이 삭제되었거나 이동되었습니다\n\n그대로 저장하면 외부에서 변경된 내용이 사라질 수 있습니다."
        );
        assert!(
            !text.contains('/') && !text.contains('\\'),
            "a name, never a path"
        );
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
        assert_eq!(app_commands::APP_COMMANDS.len(), 31);
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

#[cfg(test)]
mod path_leak_tests;

#[cfg(test)]
mod pdf_edit_fixtures;
#[cfg(test)]
mod pdf_edit_hostile_tests;
#[cfg(test)]
mod pdf_edit_tests;
#[cfg(test)]
mod perf_bench;
