// The app commands the WebView may call. This list is the single source for both the build script
// (which declares them deny-by-default in the app manifest) and the capability file; a unit test
// keeps `capabilities/main-window.json` in lockstep (PD-088).

#[allow(dead_code)]
pub const APP_COMMANDS: &[&str] = &[
    "ipc_dispatch",
    "save_document_file",
    "pdf_export_start",
    "pdf_export_pages",
    "pdf_export_finish",
    "pdf_edit_open",
    "pdf_edit_apply",
    "pdf_edit_split_merge",
    "pdf_edit_save",
    "list_system_fonts",
    "read_system_font",
    "get_update_notice",
    "set_update_check",
    "open_release_page",
    "install_update",
    "open_document_window",
    "open_file_in_new_window",
    "take_pending_open",
    "abandon_tracked_open",
    "report_window_state",
    "close_current_window",
    "list_recent_documents",
    "clear_recent_documents",
    "open_recent_document",
    "get_recent_preview",
    "report_menu_state",
    "quit_app",
    "end_close_prompt",
    "ack_close_prompt",
    "confirm_dialog",
    "confirm_close",
    "print_webview",
];
