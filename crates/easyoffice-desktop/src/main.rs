//! EasyOffice Windows desktop shell: a Tauri v2 (WebView2) window that loads the WebUI from assets
//! embedded in the executable and routes every UI request to `EasyOfficeIpcRouter::dispatch`
//! in-process. No HTTP server, browser, or remote content is involved.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod save_file;
mod smoke;

use easyoffice_app::EasyOfficeIpcRouter;
use save_file::{write_document_bytes, SaveOutcome, SaveRequest};
use serde::Serialize;
use smoke::{SmokeProbe, SMOKE_TIMEOUT};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;

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
    let result = tauri::async_runtime::spawn_blocking(move || {
        EasyOfficeIpcRouter::dispatch(&command, &payload)
    })
    .await
    .map_err(|err| IpcError::new(format!("엔진 작업을 실행하지 못했습니다: {err}")))?
    .map_err(|err| IpcError::new(err.to_string()));
    if let Some(probe) = app.try_state::<SmokeProbe>() {
        probe.observe(&probe_command, result.is_ok());
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
) -> Result<SaveOutcome, IpcError> {
    let request = SaveRequest::parse(&file_name, &base64).map_err(IpcError::new)?;
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
    Ok(SaveOutcome::saved(&written.0, written.1))
}

/// Only the embedded app origin may be displayed; any other navigation is refused.
fn is_app_origin(url: &tauri::Url) -> bool {
    url.scheme() == "tauri" || url.host_str() == Some("tauri.localhost")
}

fn main() {
    let smoke_mode = std::env::args().skip(1).any(|arg| arg == "--smoke");
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![ipc_dispatch, save_document_file])
        .setup(move |app| {
            if smoke_mode {
                app.manage(SmokeProbe::start(SMOKE_TIMEOUT));
            }
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("EasyOffice")
                .inner_size(1440.0, 900.0)
                .min_inner_size(1024.0, 680.0)
                .center()
                .on_navigation(is_app_origin)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("EasyOffice desktop shell failed to start");
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
    fn dispatch_reaches_the_shared_router() {
        let boot = EasyOfficeIpcRouter::dispatch("get_workspace_bootstrap", "").unwrap();
        assert!(boot.contains("#5C6BC0"));
        assert!(EasyOfficeIpcRouter::dispatch("not_a_command", "").is_err());
    }
}
