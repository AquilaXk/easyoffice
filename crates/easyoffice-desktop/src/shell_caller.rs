//! Caller check for the app commands (PD-083).
//!
//! Tauri's capability check decides per window and per origin, never per frame, and on Windows the
//! initialization scripts (the IPC objects and the invoke key) are added to every frame
//! (`wry` 0.57 `webview2/mod.rs`, `tauri` `webview_window.rs` "Windows: scripts are always added to
//! subframes"). Whether a message that the sandboxed editor frame posts reaches the command handler
//! as a local request depends on what WebView2 reports as its source, which the documentation does
//! not settle. So the commands do not rely on it: every window gets a random token that exists only
//! in the top-level document (an initialization script that defines it when `window.top === window`)
//! and the page sends it as a header with each command. A command that does not carry the
//! window's token is refused by [`ShellCaller`], whatever the transport.
//!
//! The editor frame has an opaque origin, so it cannot read the token from the shell document, and
//! the script text with the token is never exposed to page code.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use tauri::ipc::{CommandArg, CommandItem, InvokeError};
use tauri::Runtime;

/// Request header that carries the token (lower case: `HeaderMap` keys are).
pub const TOKEN_HEADER: &str = "x-easyoffice-shell";
/// Global the shell document reads; defined only in the top-level document.
pub const TOKEN_GLOBAL: &str = "__EASYOFFICE_SHELL_TOKEN__";
const TOKEN_BYTES: usize = 16;

/// Tokens by window label.
#[derive(Default)]
pub struct ShellTokens {
    by_label: Mutex<HashMap<String, String>>,
}

impl ShellTokens {
    /// Creates (or replaces) the token of `label` and returns it.
    pub fn issue(&self, label: &str) -> Result<String, getrandom::Error> {
        let mut bytes = [0u8; TOKEN_BYTES];
        getrandom::fill(&mut bytes)?;
        let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        self.by_label
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(label.to_string(), token.clone());
        Ok(token)
    }

    pub fn forget(&self, label: &str) {
        self.by_label
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(label);
    }

    /// True when `presented` is the token of `label`. Compares every byte, so the time taken does not
    /// tell how much of a guess was right.
    pub fn verify(&self, label: &str, presented: Option<&[u8]>) -> bool {
        let map = self.by_label.lock().unwrap_or_else(PoisonError::into_inner);
        let (Some(expected), Some(presented)) = (map.get(label), presented) else {
            return false;
        };
        let expected = expected.as_bytes();
        let mut diff = u8::from(expected.len() != presented.len());
        for (i, byte) in expected.iter().enumerate() {
            diff |= byte ^ presented.get(i).copied().unwrap_or(0);
        }
        diff == 0
    }
}

/// The script that hands the token to the top-level document of a window.
pub fn init_script(token: &str) -> String {
    format!(
        "if (window.top === window) {{ Object.defineProperty(window, \"{TOKEN_GLOBAL}\", {{ value: \"{token}\" }}); }}"
    )
}

/// Command argument that succeeds only for a call made by the window's own document. Every app
/// command takes one (a structural test below enforces it).
pub struct ShellCaller;

impl<'de, R: Runtime> CommandArg<'de, R> for ShellCaller {
    fn from_command(command: CommandItem<'de, R>) -> Result<Self, InvokeError> {
        use tauri::Manager;
        let webview = command.message.webview_ref();
        let presented = command
            .message
            .headers()
            .get(TOKEN_HEADER)
            .map(|value| value.as_bytes());
        let allowed = webview
            .try_state::<ShellTokens>()
            .is_some_and(|tokens| tokens.verify(webview.label(), presented));
        if allowed {
            Ok(Self)
        } else {
            Err(InvokeError::from(
                "이 호출은 셸 창의 문서에서 온 요청이 아니어서 거부했습니다.",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_accepts_only_its_own_token() {
        let tokens = ShellTokens::default();
        let a = tokens.issue("main").unwrap();
        let b = tokens.issue("doc-1").unwrap();
        assert_eq!(a.len(), TOKEN_BYTES * 2);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
        assert!(tokens.verify("main", Some(a.as_bytes())));
        assert!(tokens.verify("doc-1", Some(b.as_bytes())));
        assert!(
            !tokens.verify("main", Some(b.as_bytes())),
            "another window's token"
        );
        assert!(!tokens.verify("main", None), "no header");
        assert!(!tokens.verify("main", Some(b"")));
        assert!(
            !tokens.verify("main", Some(&a.as_bytes()[..a.len() - 1])),
            "prefix"
        );
        assert!(
            !tokens.verify("main", Some(format!("{a}0").as_bytes())),
            "suffix"
        );
        assert!(
            !tokens.verify("unknown", Some(a.as_bytes())),
            "no token issued"
        );
        tokens.forget("main");
        assert!(!tokens.verify("main", Some(a.as_bytes())), "forgotten");
    }

    #[test]
    fn the_init_script_defines_the_token_in_the_top_level_document_only() {
        let script = init_script("abc123");
        assert!(script.starts_with("if (window.top === window) {"));
        assert!(script.contains(
            r#"Object.defineProperty(window, "__EASYOFFICE_SHELL_TOKEN__", { value: "abc123" })"#
        ));
        assert!(TOKEN_HEADER
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '-'));
    }

    /// Structural guard: every `#[tauri::command]` of the shell takes a [`ShellCaller`], and there
    /// are exactly as many as `APP_COMMANDS` lists.
    #[test]
    fn every_app_command_takes_a_shell_caller() {
        let sources = [
            include_str!("main.rs"),
            include_str!("window_commands.rs"),
            include_str!("update_check.rs"),
            include_str!("update_install.rs"),
            include_str!("preview/mod.rs"),
            include_str!("menu_state.rs"),
            include_str!("app_quit.rs"),
            include_str!("confirm_dialog.rs"),
            include_str!("pdf_job.rs"),
            include_str!("font_catalog.rs"),
        ];
        let mut checked = 0;
        for source in sources {
            let production = source.split("#[cfg(test)]").next().unwrap();
            for block in production.split("#[tauri::command]").skip(1) {
                let signature = block.split('{').next().unwrap();
                assert!(
                    signature.contains("ShellCaller"),
                    "command without a ShellCaller: {signature}"
                );
                checked += 1;
            }
        }
        assert_eq!(checked, crate::app_commands::APP_COMMANDS.len());
    }
}
