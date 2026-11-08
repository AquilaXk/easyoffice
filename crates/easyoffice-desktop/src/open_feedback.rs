//! What the user sees when a file handed to the app from outside the page (drag and drop, Finder,
//! a second launch) cannot be opened, and while a droppable file is dragged over a window (PD-124).
//!
//! The wording is the reference app's, widened to every document family. Only user sentences and
//! the file name leave this module: no path and no operating system error text (PD-088). The
//! message goes to the page as its shared toast; without any window it opens a native dialog.

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

/// A file whose extension no window kind opens. Also the start of the message `open_path` returns.
pub const UNSUPPORTED: &str = "지원하지 않는 형식입니다";
/// Shown while a file the app can open is dragged over a window.
pub const DROP_HINT: &str = "파일을 놓으면 문서를 엽니다";
const FAILURE_PREFIX: &str = "파일 로드 실패: ";
const DIALOG_TITLE: &str = "파일 열기";

/// The text for an open failure: unsupported formats stand alone, every other reason follows the
/// reference app's "파일 로드 실패:" lead-in.
pub fn failure_text(reason: &str) -> String {
    if reason.starts_with(UNSUPPORTED) {
        reason.to_string()
    } else {
        format!("{FAILURE_PREFIX}{reason}")
    }
}

/// The script that shows `message` as the page's toast. The message is embedded as a JSON string,
/// so no text can end the string literal.
pub fn notice_script(message: &str) -> String {
    let literal = serde_json::to_string(message).unwrap_or_else(|_| "\"\"".into());
    format!("window.__EASYOFFICE_NOTICE__ && window.__EASYOFFICE_NOTICE__({literal});")
}

/// The window that shows a notice: the focused one, else the first by label.
pub fn notice_target(windows: &[(String, bool)]) -> Option<&str> {
    windows
        .iter()
        .find(|(_, focused)| *focused)
        .or_else(|| windows.iter().min_by(|a, b| a.0.cmp(&b.0)))
        .map(|(label, _)| label.as_str())
}

/// Shows `message` as a toast in the focused window. Returns false when no window took it.
pub fn show_notice(app: &AppHandle, message: &str) -> bool {
    let windows: Vec<(String, bool)> = app
        .webview_windows()
        .into_iter()
        .map(|(label, window)| (label, window.is_focused().unwrap_or(false)))
        .collect();
    let Some(window) = notice_target(&windows).and_then(|label| app.get_webview_window(label))
    else {
        return false;
    };
    window.eval(notice_script(message)).is_ok()
}

/// Tells the user an open failed: as a toast, or a native error dialog when there is no window.
pub fn report_open_failure(app: &AppHandle, reason: &str) {
    eprintln!("open: {reason}");
    let text = failure_text(reason);
    if !show_notice(app, &text) {
        app.dialog()
            .message(text)
            .title(DIALOG_TITLE)
            .kind(MessageDialogKind::Error)
            .buttons(MessageDialogButtons::Ok)
            .show(|_| {});
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_formats_stand_alone_and_other_reasons_get_the_lead_in() {
        let unsupported = format!("{UNSUPPORTED} (a.exe)");
        assert_eq!(failure_text(&unsupported), unsupported);
        assert_eq!(
            failure_text("파일을 찾을 수 없습니다 (a.docx)"),
            "파일 로드 실패: 파일을 찾을 수 없습니다 (a.docx)"
        );
    }

    #[test]
    fn the_notice_script_escapes_the_message() {
        let script = notice_script("a\"b\\c\n</script>");
        assert_eq!(
            script,
            "window.__EASYOFFICE_NOTICE__ && window.__EASYOFFICE_NOTICE__(\"a\\\"b\\\\c\\n</script>\");"
        );
    }

    #[test]
    fn the_notice_goes_to_the_focused_window_else_the_first_by_label() {
        let windows = vec![
            ("doc-2".to_string(), false),
            ("doc-1".to_string(), false),
            ("doc-3".to_string(), true),
        ];
        assert_eq!(notice_target(&windows), Some("doc-3"));
        assert_eq!(notice_target(&windows[..2]), Some("doc-1"));
        assert_eq!(notice_target(&[]), None);
    }
}
