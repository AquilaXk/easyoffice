//! What the user is told when a document cannot be written (PD-088). The wording is the
//! reference app's `apps/studio-host/src/core/tauri-bridge.ts` lines 459 and 492-499: an error
//! dialog titled "저장 실패" whose body is "문서를 저장하지 못했습니다.\n{reason}").
//!
//! The reason is a user sentence picked from the kind of the operating system error. The path and
//! the system's own error text go to the log only; the page and the dialog see at most the file
//! name.

use crate::IpcError;
use std::io::ErrorKind;
use std::path::Path;
use tauri::{AppHandle, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

/// Title of the native error dialog.
pub const DIALOG_TITLE: &str = "저장 실패";
const DIALOG_LEAD: &str = "문서를 저장하지 못했습니다.";
const DISK_FULL: &str = "디스크 공간이 부족합니다.";
const NO_PERMISSION: &str = "쓰기 권한이 없습니다.";

/// Why a write failed, as a user sentence. Holds no path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveError {
    reason: String,
}

impl SaveError {
    /// Classifies `err` for the file at `path` and logs the details (path and system text) to stderr.
    pub fn from_io(path: &Path, err: &std::io::Error) -> Self {
        eprintln!("save: {} failed: {err}", path.display());
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        Self {
            reason: reason_for(err.kind(), &name),
        }
    }

    /// The body of the native error dialog.
    pub fn dialog_body(&self) -> String {
        format!("{DIALOG_LEAD}\n{}", self.reason)
    }

    /// The error the page receives: the reason alone.
    pub fn to_ipc(&self) -> IpcError {
        IpcError::new(self.reason.clone())
    }
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reason)
    }
}

impl std::error::Error for SaveError {}

/// The sentence for a kind of failure: full disk, missing permission (a read-only volume counts as
/// one), else the generic sentence with the file name.
fn reason_for(kind: ErrorKind, file_name: &str) -> String {
    match kind {
        ErrorKind::StorageFull | ErrorKind::QuotaExceeded => DISK_FULL.to_string(),
        ErrorKind::PermissionDenied | ErrorKind::ReadOnlyFilesystem => NO_PERMISSION.to_string(),
        _ if file_name.is_empty() => "파일을 저장하지 못했습니다.".to_string(),
        _ => format!("파일을 저장하지 못했습니다 ({file_name})"),
    }
}

/// Shows the failure in a native error dialog over `window`, like the reference app. The page gets
/// no say in the wording; it only learns the reason through the command's error.
pub fn show_dialog(app: &AppHandle, window: &WebviewWindow, err: &SaveError) {
    app.dialog()
        .message(err.dialog_body())
        .title(DIALOG_TITLE)
        .kind(MessageDialogKind::Error)
        .parent(window)
        .buttons(MessageDialogButtons::Ok)
        .show(|_| {});
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    fn classify(kind: ErrorKind) -> String {
        SaveError::from_io(
            Path::new("/private/dir/보고서.docx"),
            &io::Error::from(kind),
        )
        .to_string()
    }

    #[test]
    fn a_full_disk_and_a_quota_say_the_disk_is_full() {
        assert_eq!(
            classify(ErrorKind::StorageFull),
            "디스크 공간이 부족합니다."
        );
        assert_eq!(
            classify(ErrorKind::QuotaExceeded),
            "디스크 공간이 부족합니다."
        );
    }

    #[test]
    fn a_refused_write_and_a_read_only_volume_say_there_is_no_permission() {
        assert_eq!(
            classify(ErrorKind::PermissionDenied),
            "쓰기 권한이 없습니다."
        );
        assert_eq!(
            classify(ErrorKind::ReadOnlyFilesystem),
            "쓰기 권한이 없습니다."
        );
    }

    #[test]
    fn any_other_failure_names_the_file_and_never_the_folder() {
        for kind in [ErrorKind::NotFound, ErrorKind::Other, ErrorKind::TimedOut] {
            assert_eq!(classify(kind), "파일을 저장하지 못했습니다 (보고서.docx)");
        }
        let nameless = SaveError::from_io(Path::new("/"), &io::Error::from(ErrorKind::Other));
        assert_eq!(nameless.to_string(), "파일을 저장하지 못했습니다.");
    }

    #[test]
    fn neither_the_path_nor_the_system_text_reaches_the_user_text() {
        let os_text = "No space left on device (os error 28)";
        let err = SaveError::from_io(Path::new("/private/dir/a.docx"), &io::Error::other(os_text));
        let ipc = serde_json::to_string(&err.to_ipc()).unwrap();
        for text in [err.to_string(), err.dialog_body(), ipc] {
            assert!(
                !text.contains("/private") && !text.contains("os error"),
                "{text}"
            );
        }
        assert_eq!(
            err.dialog_body(),
            "문서를 저장하지 못했습니다.\n파일을 저장하지 못했습니다 (a.docx)"
        );
    }
}
