// The app commands the WebView may call. This list is the single source for both the build script
// (which declares them deny-by-default in the app manifest) and the capability file; a unit test
// keeps `capabilities/main-window.json` in lockstep (PD-088).

#[allow(dead_code)]
pub const APP_COMMANDS: &[&str] = &[
    "ipc_dispatch",
    "save_document_file",
    "open_document_window",
    "open_file_in_new_window",
    "take_pending_open",
    "report_window_state",
    "close_current_window",
    "list_recent_documents",
    "open_recent_document",
];
