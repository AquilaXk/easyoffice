//! Quit on every OS (the 끝내기 menu item, and on macOS also Cmd+Q, the Dock's 종료 and a system
//! shutdown, which arrive as `RunEvent::ExitRequested`): every window goes through its own close
//! confirmation, one after the other, and the app ends when the last one is gone.
//!
//! Adapted from the reference app `apps/desktop/src-tauri/src/app_quit.rs` (MIT, Copyright (c)
//! 2025-2026 Edward Kim; see THIRD_PARTY_NOTICES.md): the same queue of window labels (the focused
//! window first), advanced when a window is destroyed and ended by the user's "cancel". Differences:
//! the reference app asks each window with an event; EasyOffice asks with `WebviewWindow::close`, so
//! the existing close flow runs (`CloseRequested`: a window without unsaved changes closes, a dirty
//! one runs the page's 저장 / 저장 안 함 / 취소 prompt), and the page ends the quit through the
//! narrow `end_close_prompt` command (`window_commands.rs`) when the user cancels or a save fails;
//! it confirms the prompt with `ack_close_prompt`. The reference app does the same on macOS: its
//! application menu has a custom quit item and `ExitRequested` (no exit code) is prevented and sent
//! to the same queue (`app_quit.rs:62-153`, `lib.rs:133`). The updater's restart (`update_install.rs`)
//! does not go through here: it refuses to start while a window has unsaved changes.

use std::collections::VecDeque;
use std::sync::Mutex;

use tauri::{AppHandle, Manager};

/// The windows still to be closed by a running quit, in order.
#[derive(Default)]
pub struct AppQuit {
    pending: Mutex<VecDeque<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Advance {
    /// No quit is running.
    Idle,
    /// Ask this window next.
    Next(String),
    /// A window other than the one being asked is gone: it only leaves the queue, nobody is asked
    /// again (the asked window may be showing its save prompt).
    Waiting,
    /// Every window of the queue is gone.
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
    fn begin(&self, labels: Vec<String>) -> Option<String> {
        let mut pending = self.pending();
        if !pending.is_empty() {
            return None;
        }
        *pending = labels.into();
        pending.front().cloned()
    }

    pub fn cancel(&self) {
        self.pending().clear();
    }

    fn advance_after_close(&self, closed: &str) -> Advance {
        let mut pending = self.pending();
        if pending.is_empty() {
            return Advance::Idle;
        }
        let was_asked = pending.front().map(String::as_str) == Some(closed);
        pending.retain(|label| label != closed);
        match pending.front() {
            None => Advance::Complete,
            Some(next) if was_asked => Advance::Next(next.clone()),
            Some(_) => Advance::Waiting,
        }
    }
}

/// Focused window first, the others in label order.
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

/// Starts a quit: the 끝내기 item on every OS, and `ExitRequested` on macOS. A quit that is already
/// running ignores the request, so a second Cmd+Q cannot ask a window twice.
pub fn request_app_quit(app: &AppHandle) {
    let labels = open_labels(app, None);
    if labels.is_empty() {
        app.exit(0);
    } else if let Some(first) = app.state::<AppQuit>().begin(labels) {
        ask(app, &first);
    }
}

/// The windows still open, the focused one first; `except` is a window that is going away.
fn open_labels(app: &AppHandle, except: Option<&str>) -> Vec<String> {
    let windows = app.webview_windows();
    let focused = windows
        .values()
        .find(|window| window.is_focused().unwrap_or(false))
        .map(|window| window.label().to_string());
    let labels = windows
        .into_keys()
        .filter(|label| Some(label.as_str()) != except);
    ordered(labels.collect(), focused.as_deref())
}

/// A window is gone: ask the next one, or end the app after the last. A window opened while the
/// quit was running is asked too (the queue is built again before the app ends).
pub fn window_destroyed(app: &AppHandle, label: &str) {
    match app.state::<AppQuit>().advance_after_close(label) {
        Advance::Idle | Advance::Waiting => {}
        Advance::Next(next) => ask(app, &next),
        Advance::Complete => {
            let quit = app.state::<AppQuit>();
            match quit.begin(open_labels(app, Some(label))) {
                Some(first) => ask(app, &first),
                None => app.exit(0),
            }
        }
    }
}

/// Asks the window to close (`raise_and_close`): a window already showing its save prompt keeps it,
/// the quit waits for its answer (destroyed: next window; cancelled: quit stops). A page that never
/// confirmed its prompt is closed by the next request after `CLOSE_ACK_TIMEOUT` (PD-113).
fn ask(app: &AppHandle, label: &str) {
    let closed = app
        .get_webview_window(label)
        .is_some_and(|window| crate::window_commands::raise_and_close(&window));
    if !closed {
        // The window could not be asked: stop instead of leaving a quit that never ends.
        app.state::<AppQuit>().cancel();
    }
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
    fn a_window_closed_out_of_turn_only_leaves_the_queue() {
        let quit = AppQuit::default();
        quit.begin(labels(&["main", "doc-2", "doc-3"]));
        assert_eq!(quit.advance_after_close("doc-2"), Advance::Waiting);
        assert_eq!(quit.advance_after_close("other"), Advance::Waiting);
        assert_eq!(
            quit.advance_after_close("main"),
            Advance::Next("doc-3".to_string())
        );
    }

    #[test]
    fn the_asked_window_is_not_asked_again_when_another_window_closes() {
        let quit = AppQuit::default();
        assert_eq!(
            quit.begin(labels(&["main", "doc-2"])),
            Some("main".to_string())
        );
        // main shows its save prompt; doc-2 is closed meanwhile (by the user).
        assert_eq!(quit.advance_after_close("doc-2"), Advance::Waiting);
        assert_eq!(quit.advance_after_close("main"), Advance::Complete);
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

    /// Structural guard: no quit path calls `close()` itself. The queue asks windows through
    /// `raise_and_close`, which never closes over a save prompt the page is showing, and the macOS
    /// `ExitRequested` handler only hands the request to the queue (no loop over all windows).
    #[test]
    fn every_quit_goes_through_the_one_queue() {
        let queue = include_str!("app_quit.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(queue.contains("raise_and_close"));
        let main = include_str!("main.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        for production in [queue, main] {
            assert!(!production.contains(".close()"), "a direct close() call");
        }
        assert!(
            !main.contains("raise_and_close"),
            "main.rs never closes windows itself"
        );
        let handler = &main[main.find("fn handle_run_event").unwrap()..];
        assert!(handler.contains("ExitRequested"));
        assert!(handler.contains("api.prevent_exit()"));
        assert!(handler.contains("app_quit::request_app_quit(app)"));
    }
}
