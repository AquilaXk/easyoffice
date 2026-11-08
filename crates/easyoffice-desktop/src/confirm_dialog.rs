//! Native yes/no confirmations for the page (PD-120).
//!
//! WKWebView through wry does not implement `window.confirm`, which then answers "cancel" without
//! showing anything, so every page confirmation goes through a native dialog instead (the reference
//! app does the same). The page names only a [`ConfirmKind`]; the wording and the button labels are
//! constants here, so the page cannot put its own text into a native dialog.

use serde::Deserialize;
use tauri::{AppHandle, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::shell_caller::ShellCaller;
use crate::IpcError;

/// The confirmations the page may ask for. An unknown name fails to deserialize and the command is
/// refused before any dialog opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConfirmKind {
    /// Saving a document that was opened with a password writes a copy without it.
    PasswordFreeSave,
    /// A Hangul document with unsaved edits would be replaced by the file being opened.
    ReplaceUnsavedHwp,
    /// The HWPX copy failed its check; offer an HWP copy instead.
    HwpxFallbackHwp,
    /// An autosaved Hangul document from the previous session can be restored.
    RestoreHwpAutosave,
    /// A module with unsaved edits would be replaced by the file being opened.
    ReplaceUnsavedModule,
}

pub struct ConfirmSpec {
    pub title: &'static str,
    pub message: &'static str,
    pub accept: &'static str,
    pub cancel: &'static str,
}

const CANCEL: &str = "취소";

impl ConfirmKind {
    pub fn spec(self) -> ConfirmSpec {
        match self {
            Self::PasswordFreeSave => ConfirmSpec {
                title: "암호 없이 저장",
                message: "저장한 HWP 파일에는 암호가 적용되지 않습니다. 암호 없이 저장하시겠습니까?",
                accept: "암호 없이 저장",
                cancel: CANCEL,
            },
            Self::ReplaceUnsavedHwp => ConfirmSpec {
                title: "저장하지 않은 편집 내용",
                message: "저장하지 않은 한글 문서 편집 내용이 있습니다.\n다른 문서를 열면 현재 내용이 사라집니다. 계속할까요?",
                accept: "계속",
                cancel: CANCEL,
            },
            Self::HwpxFallbackHwp => ConfirmSpec {
                title: "HWP로 저장",
                message: "HWPX로 저장하기 전 검증에 실패했습니다.\nHWP 형식으로 대신 저장할까요?",
                accept: "HWP로 저장",
                cancel: CANCEL,
            },
            Self::RestoreHwpAutosave => ConfirmSpec {
                title: "자동 저장본 복원",
                message: "이전 세션의 저장하지 않은 한글 문서의 자동 저장본이 있습니다. 복원할까요?\n복원하지 않으면 자동 저장본은 삭제됩니다.",
                accept: "복원",
                cancel: "복원 안 함",
            },
            Self::ReplaceUnsavedModule => ConfirmSpec {
                title: "저장하지 않은 편집 내용",
                message: "이 모듈에 저장하지 않은 편집 내용이 있습니다.\n파일을 열면 현재 내용이 바뀝니다 (실행 취소로 되돌릴 수 있습니다). 계속할까요?",
                accept: "계속",
                cancel: CANCEL,
            },
        }
    }
}

/// Asks the user over the calling window and answers whether they accepted. Closing the dialog
/// counts as declining.
#[tauri::command]
pub async fn confirm_dialog(
    _caller: ShellCaller,
    app: AppHandle,
    window: WebviewWindow,
    kind: ConfirmKind,
) -> Result<bool, IpcError> {
    tauri::async_runtime::spawn_blocking(move || {
        let spec = kind.spec();
        app.dialog()
            .message(spec.message)
            .title(spec.title)
            .kind(MessageDialogKind::Warning)
            .parent(&window)
            .buttons(MessageDialogButtons::OkCancelCustom(
                spec.accept.to_string(),
                spec.cancel.to_string(),
            ))
            .blocking_show()
    })
    .await
    .map_err(|err| IpcError::new(format!("확인 창을 열지 못했습니다: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [ConfirmKind; 5] = [
        ConfirmKind::PasswordFreeSave,
        ConfirmKind::ReplaceUnsavedHwp,
        ConfirmKind::HwpxFallbackHwp,
        ConfirmKind::RestoreHwpAutosave,
        ConfirmKind::ReplaceUnsavedModule,
    ];

    fn parse(name: &str) -> Result<ConfirmKind, serde_json::Error> {
        serde_json::from_value(serde_json::Value::String(name.to_string()))
    }

    #[test]
    fn only_the_listed_kind_names_are_accepted() {
        assert_eq!(
            parse("password-free-save").unwrap(),
            ConfirmKind::PasswordFreeSave
        );
        assert_eq!(
            parse("replace-unsaved-hwp").unwrap(),
            ConfirmKind::ReplaceUnsavedHwp
        );
        assert_eq!(
            parse("hwpx-fallback-hwp").unwrap(),
            ConfirmKind::HwpxFallbackHwp
        );
        assert_eq!(
            parse("restore-hwp-autosave").unwrap(),
            ConfirmKind::RestoreHwpAutosave
        );
        assert_eq!(
            parse("replace-unsaved-module").unwrap(),
            ConfirmKind::ReplaceUnsavedModule
        );
        for bad in [
            "",
            "PasswordFreeSave",
            "password_free_save",
            "저장하시겠습니까?",
            "password-free-save ",
            "evil",
        ] {
            assert!(parse(bad).is_err(), "{bad:?} must be refused");
        }
        for not_a_string in [
            serde_json::json!(1),
            serde_json::json!(null),
            serde_json::json!({"message": "x"}),
            serde_json::json!(["password-free-save"]),
        ] {
            assert!(serde_json::from_value::<ConfirmKind>(not_a_string).is_err());
        }
    }

    #[test]
    fn every_kind_has_its_own_wording_and_the_password_one_follows_the_reference() {
        let specs: Vec<ConfirmSpec> = ALL.iter().map(|k| k.spec()).collect();
        for (i, a) in specs.iter().enumerate() {
            assert!(
                !a.title.is_empty()
                    && !a.message.is_empty()
                    && !a.accept.is_empty()
                    && !a.cancel.is_empty()
            );
            assert_ne!(a.accept, a.cancel);
            for b in &specs[i + 1..] {
                assert_ne!(a.message, b.message);
            }
        }
        let password = ConfirmKind::PasswordFreeSave.spec();
        assert_eq!(
            password.message,
            "저장한 HWP 파일에는 암호가 적용되지 않습니다. 암호 없이 저장하시겠습니까?"
        );
        assert_eq!(
            (password.accept, password.cancel),
            ("암호 없이 저장", "취소")
        );
    }
}
