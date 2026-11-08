//! Quit from the menu on Windows and Linux: every window goes through its own close confirmation,
//! one after the other, and the app ends when the last one is gone.
//!
//! Adapted from the reference app `apps/desktop/src-tauri/src/app_quit.rs` (MIT, Copyright (c)
//! 2025-2026 Edward Kim; see THIRD_PARTY_NOTICES.md): the same queue of window labels (the focused
//! window first), advanced when a window is destroyed and ended by the user's "cancel". Differences:
//! the reference app asks each window with an event; EasyOffice asks with `WebviewWindow::close`, so
//! the existing close flow runs (`CloseRequested`: a window without unsaved changes closes, a dirty
//! one runs the page's 저장 / 저장 안 함 / 취소 prompt), and the page ends the quit through the
//! narrow `cancel_app_quit` command when the user cancels or a save fails. macOS keeps its system
//! quit item (`RunEvent::ExitRequested` in `main.rs`).

use std::collections::VecDeque;
use std::sync::Mutex;

use tauri::{AppHandle, Manager};

use crate::shell_caller::ShellCaller;

/// The windows still to be closed by a running quit, in order.
#[derive(Default)]
pub struct AppQuit {
    pending: Mutex<VecDeque<String>>,
}

#[cfg(any(not(target_os = "macos"), test))]
#[derive(Debug, Clone, PartialEq, Eq)]
enum Advance {
    /// No quit is running.
    Idle,
    /// Ask this window next.
    Next(String),
    /// Every window is gone.
    Complete,
}

impl AppQuit {
    fn pending(&self) -> std::sync::MutexGuard<'_, VecDeque<String>> {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Starts a quit over `labels`; returns the first window to ask (`None` when there is none).
    /// A second request while one is running changes nothing.
    #[cfg(any(not(target_os = "macos"), test))]
    fn begin(&self, labels: Vec<String>) -> Option<String> {
        let mut pending = self.pending();
        if !pending.is_empty() {
            return None;
        }
        *pending = labels.into();
        pending.front().cloned()
    }

    fn cancel(&self) {
        self.pending().clear();
    }

    #[cfg(any(not(target_os = "macos"), test))]
    fn advance_after_close(&self, closed: &str) -> Advance {
        let mut pending = self.pending();
        if pending.is_empty() {
            return Advance::Idle;
        }
        pending.retain(|label| label != closed);
        match pending.front() {
            Some(next) => Advance::Next(next.clone()),
            None => Advance::Complete,
        }
    }
}

/// Focused window first, the others in label order.
#[cfg(any(not(target_os = "macos"), test))]
fn ordered(mut labels: Vec<String>, focused: Option<&str>) -> Vec<String> {
    labels.sort();
    if let Some(focused) = focused {
        if let Some(at) = labels.iter().position(|label| label == focused) {
            let label = labels.remove(at);
            labels.insert(0, label);
        }
    }
    labels
}

#[cfg(not(target_os = "macos"))]
pub fn request_app_quit(app: &AppHandle) {
    let windows = app.webview_windows();
    let focused = windows
        .values()
        .find(|window| window.is_focused().unwrap_or(false))
        .map(|window| window.label().to_string());
    let labels = ordered(windows.keys().cloned().collect(), focused.as_deref());
    match app.state::<AppQuit>().begin(labels) {
        Some(first) => ask(app, &first),
        None if windows.is_empty() => app.exit(0),
        None => {}
    }
}

/// A window is gone: ask the next one, or end the app after the last.
#[cfg(not(target_os = "macos"))]
pub fn window_destroyed(app: &AppHandle, label: &str) {
    match app.state::<AppQuit>().advance_after_close(label) {
        Advance::Idle => {}
        Advance::Next(next) => ask(app, &next),
        Advance::Complete => app.exit(0),
    }
}

#[cfg(not(target_os = "macos"))]
fn ask(app: &AppHandle, label: &str) {
    let closed = app
        .get_webview_window(label)
        .is_some_and(|window| window.close().is_ok());
    if !closed {
        // The window could not be asked: stop instead of leaving a quit that never ends.
        app.state::<AppQuit>().cancel();
    }
}

/// The user kept a window open (cancel, or the save failed): the quit ends.
#[tauri::command]
pub fn cancel_app_quit(_caller: ShellCaller, app: AppHandle) {
    app.state::<AppQuit>().cancel();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn windows_are_asked_in_order_and_the_last_one_completes() {
        let quit = AppQuit::default();
        assert_eq!(
            quit.begin(labels(&["main", "doc-2"])),
            Some("main".to_string())
        );
        assert_eq!(
            quit.advance_after_close("main"),
            Advance::Next("doc-2".to_string())
        );
        assert_eq!(quit.advance_after_close("doc-2"), Advance::Complete);
    }

    #[test]
    fn a_window_closed_out_of_turn_is_dropped_from_the_queue() {
        let quit = AppQuit::default();
        quit.begin(labels(&["main", "doc-2", "doc-3"]));
        assert_eq!(
            quit.advance_after_close("doc-2"),
            Advance::Next("main".to_string())
        );
        assert_eq!(
            quit.advance_after_close("other"),
            Advance::Next("main".to_string())
        );
    }

    #[test]
    fn cancel_ends_the_quit_and_a_second_request_while_running_is_ignored() {
        let quit = AppQuit::default();
        quit.begin(labels(&["main"]));
        assert_eq!(quit.begin(labels(&["main", "doc-2"])), None);
        quit.cancel();
        assert_eq!(quit.advance_after_close("main"), Advance::Idle);
        assert_eq!(quit.begin(labels(&["doc-2"])), Some("doc-2".to_string()));
    }

    #[test]
    fn the_focused_window_comes_first_and_the_rest_are_sorted() {
        let all = labels(&["doc-3", "main", "doc-1"]);
        assert_eq!(
            ordered(all.clone(), Some("doc-3")),
            labels(&["doc-3", "doc-1", "main"])
        );
        assert_eq!(
            ordered(all.clone(), None),
            labels(&["doc-1", "doc-3", "main"])
        );
        assert_eq!(
            ordered(all, Some("gone")),
            labels(&["doc-1", "doc-3", "main"])
        );
    }
}
