//! The native print dialog of the calling window (PD-139).
//!
//! `window.print()` does nothing in WKWebView through wry: wry's UI delegate
//! (`wkwebview/class/wry_web_view_ui_delegate.rs`) implements only the file panel, media capture
//! and new-window callbacks, not the print one. The page therefore builds the hidden print root
//! (`hwp-print.js`) and asks the shell to print the webview, which `WebviewWindow::print` does with
//! the platform's own print operation: `NSPrintOperation` on macOS, `PrintOperation` on Linux
//! (WebKitGTK) and `window.print()` in WebView2 on Windows.
//!
//! Adapted from the reference app's `print_webview` command (`apps/desktop/src-tauri/src/commands.rs`,
//! MIT, Copyright (c) 2025-2026 Edward Kim; see THIRD_PARTY_NOTICES.md). The command takes no
//! argument, so the page cannot choose what is printed; it prints the document of the window that
//! called it.

use tauri::WebviewWindow;

use crate::shell_caller::ShellCaller;
use crate::IpcError;

/// Opens the print dialog for the calling window's webview. Returns once the dialog is requested;
/// the platform reports no completion, which the page handles with `afterprint` and a late cleanup.
#[tauri::command]
pub fn print_webview(_caller: ShellCaller, window: WebviewWindow) -> Result<(), IpcError> {
    window.print().map_err(|err| {
        eprintln!("print_webview: {err}");
        IpcError::new("인쇄 대화상자를 열지 못했습니다.")
    })
}
