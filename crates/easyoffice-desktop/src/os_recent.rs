//! The operating system's own recent-documents list (PD-099): on macOS every document the shell
//! opens or saves is also noted with `NSDocumentController`, so it shows in the Dock menu and in
//! 사과 메뉴 ▸ 최근 사용 항목. Other platforms keep only the in-app list (`recent.rs`).
//!
//! Adapted from hop `apps/desktop/src-tauri/src/macos_recent_documents.rs` (MIT, Copyright (c)
//! 2025-2026 Edward Kim; see THIRD_PARTY_NOTICES.md). EasyOffice difference: failures are ignored
//! (the in-app list is the source of truth), so the function returns nothing.

use std::path::Path;
use tauri::AppHandle;

#[cfg(target_os = "macos")]
pub fn note_recent_document(app: &AppHandle, path: &Path) {
    use objc2_foundation::MainThreadMarker;
    if MainThreadMarker::new().is_some() {
        note_on_main_thread(path);
        return;
    }
    let path = path.to_path_buf();
    let _ = app.run_on_main_thread(move || note_on_main_thread(&path));
}

#[cfg(target_os = "macos")]
fn note_on_main_thread(path: &Path) {
    use objc2_app_kit::NSDocumentController;
    use objc2_foundation::{MainThreadMarker, NSURL};
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(url) = NSURL::from_file_path(path) else {
        return;
    };
    NSDocumentController::sharedDocumentController(mtm).noteNewRecentDocumentURL(&url);
}

#[cfg(not(target_os = "macos"))]
pub fn note_recent_document(_app: &AppHandle, _path: &Path) {}
