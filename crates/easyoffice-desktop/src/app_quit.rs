//! Quit on every OS (the page's 끝내기 item and Ctrl+Q through [`quit_app`], and on macOS also the
//! native 끝내기 item (Cmd+Q) as `RunEvent::ExitRequested` and the Dock's 종료, AppleScript `quit`
//! and a system shutdown or logout as `applicationShouldTerminate:`, `macos_terminate.rs`): every
//! window goes through its own close confirmation, one after the other, and the app ends when the
//! last one is gone.
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
//!
//! A macOS `terminate:` raises no `ExitRequested` and destroys no window with the event loop in use
//! (PD-189). The queue then answers the system later: [`request_terminate`] returns "later", the
//! queue is marked as answering a terminate, and its end replies "terminate" (the last window is
//! gone) or "cancel" (the user cancelled or a save failed) to `NSApplication`.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Instant;

use tauri::{AppHandle, Manager};

use crate::shell_caller::ShellCaller;
use crate::windows::{PromptState, CLOSE_ACK_TIMEOUT};

/// The windows still to be closed by a running quit, in order, and when the quit last moved.
#[derive(Default)]
struct Queue {
    pending: VecDeque<String>,
    progressed: Option<Instant>,
    /// The system asked to terminate (macOS) and waits for this queue's answer.
    terminating: bool,
}

/// The windows still to be closed by a running quit.
#[derive(Default)]
pub struct AppQuit {
    queue: Mutex<Queue>,
}

/// What a macOS `applicationShouldTerminate:` answers at once.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub enum Terminate {
    /// No window is open: the system may end the app.
    Now,
    /// The queue runs; the system gets its answer when the queue ends. Holds the window to ask
    /// first (`None` when a running quit already waits on one).
    Later(Option<String>),
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
    fn queue(&self) -> std::sync::MutexGuard<'_, Queue> {
        self.queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Starts a quit over `labels`; returns the window to ask (`None` when there is none). A second
    /// request while one is running changes nothing, with two exceptions (`prompt`, PD-113). The
    /// window the quit waits on may have lost its prompt (its page was reloaded): it is returned
    /// to be asked again. And when the running one has not moved for [`CLOSE_ACK_TIMEOUT`] and that
    /// window never confirmed its prompt, the new request starts over, so a stuck quit cannot
    /// swallow every later Cmd+Q. A window whose prompt is confirmed is only waiting for the
    /// user's answer.
    fn begin(&self, labels: Vec<String>, prompt: impl Fn(&str) -> PromptState) -> Option<String> {
        self.begin_at(labels, Instant::now(), prompt)
    }

    fn begin_at(
        &self,
        labels: Vec<String>,
        now: Instant,
        prompt: impl Fn(&str) -> PromptState,
    ) -> Option<String> {
        let mut queue = self.queue();
        let running = !queue.pending.is_empty();
        let front = queue.pending.front().cloned();
        let front_prompt = front.as_deref().map(prompt);
        let stalled = front_prompt != Some(PromptState::Confirmed)
            && queue
                .progressed
                .is_some_and(|at| now.saturating_duration_since(at) >= CLOSE_ACK_TIMEOUT);
        if running && !stalled {
            return front.filter(|_| front_prompt == Some(PromptState::None));
        }
        queue.pending = labels.into();
        queue.progressed = Some(now);
        queue.pending.front().cloned()
    }

    /// The system's terminate over `labels`: ends at once without windows, else the queue answers.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    fn begin_terminate(
        &self,
        labels: Vec<String>,
        prompt: impl Fn(&str) -> PromptState,
    ) -> Terminate {
        if labels.is_empty() {
            return Terminate::Now;
        }
        self.queue().terminating = true;
        Terminate::Later(self.begin(labels, prompt))
    }

    /// True once per terminate the queue still owes an answer to.
    fn take_terminating(&self) -> bool {
        std::mem::take(&mut self.queue().terminating)
    }

    /// Stops the quit; true when a system terminate waits for the answer "cancel".
    fn cancel(&self) -> bool {
        let mut queue = self.queue();
        queue.pending.clear();
        std::mem::take(&mut queue.terminating)
    }

    fn advance_after_close(&self, closed: &str) -> Advance {
        let mut queue = self.queue();
        let pending = &mut queue.pending;
        if pending.is_empty() {
            return Advance::Idle;
        }
        let was_asked = pending.front().map(String::as_str) == Some(closed);
        pending.retain(|label| label != closed);
        let next = pending.front().cloned();
        queue.progressed = Some(Instant::now());
        match next {
            None => Advance::Complete,
            Some(next) if was_asked => Advance::Next(next),
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

/// Starts a quit: the 끝내기 items and `ExitRequested` on macOS, [`quit_app`] elsewhere. A quit that
/// is already running ignores the request, so a second Cmd+Q cannot start a second prompt; the
/// only window it asks again is the one that lost its prompt (a window that shows its prompt keeps
/// it when asked again, PD-113).
pub fn request_app_quit(app: &AppHandle) {
    let labels = open_labels(app, None);
    if labels.is_empty() {
        app.exit(0);
    } else if let Some(first) = app
        .state::<AppQuit>()
        .begin(labels, |label| close_prompt(app, label))
    {
        ask(app, &first);
    }
}

/// The system's terminate (`applicationShouldTerminate:`, macOS): the same queue, answered when it
/// ends. `Now` when no window is open; otherwise the system waits (`NSTerminateLater`) while every
/// window confirms its own close, and [`window_destroyed`] or [`cancel_quit`] replies.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn request_terminate(app: &AppHandle) -> Terminate {
    // Called by AppKit: a panic here would unwind through its frames, so a terminate that comes
    // before `setup` has managed the queue ends at once instead of reading missing state.
    let Some(quit) = app.try_state::<AppQuit>() else {
        return Terminate::Now;
    };
    let decision = quit.begin_terminate(open_labels(app, None), |label| close_prompt(app, label));
    if let Terminate::Later(Some(first)) = &decision {
        // Not inside the system's call: a window that closes at once would answer the system
        // before this call has returned "later" (see [`defer_to_main`]).
        let first = first.clone();
        defer_to_main(app, move |app| ask(app, &first));
    }
    decision
}

/// Runs `task` on the main thread after the current call stack has unwound: from another thread,
/// so the task is queued to the event loop instead of running inline. The system's terminate
/// replies need it twice. The call that returns "later" must be over before any reply, or AppKit
/// would wait for an answer it already got; and the reply that ends the app must not run inside
/// the window event that triggered it (the event loop is not re-entered).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn defer_to_main(app: &AppHandle, task: impl FnOnce(&AppHandle) + Send + 'static) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let handle = app.clone();
        let _ = handle.run_on_main_thread(move || task(&app));
    });
}

/// The page's 끝내기 item and Ctrl+Q (Windows and Linux have no native menu; macOS uses its own quit
/// item and the same queue). Takes no argument: what is asked and closed is decided here.
#[tauri::command]
pub fn quit_app(_caller: ShellCaller, app: AppHandle) {
    request_app_quit(&app);
}

/// What the shell knows of the save prompt of `label`.
fn close_prompt(app: &AppHandle, label: &str) -> PromptState {
    app.state::<crate::window_commands::AppWindows>()
        .close_prompt_state(label)
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
            match quit.begin(open_labels(app, Some(label)), |next| {
                close_prompt(app, next)
            }) {
                Some(first) => ask(app, &first),
                None if quit.take_terminating() => reply_to_terminate(app, true),
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
        cancel_quit(app);
    }
}

/// Stops a running quit (the user cancelled a prompt, a save failed, a window could not be asked);
/// a system terminate waiting on it is answered "cancel" (a shutdown or logout stops too).
pub fn cancel_quit(app: &AppHandle) {
    if app.state::<AppQuit>().cancel() {
        reply_to_terminate(app, false);
    }
}

#[cfg(target_os = "macos")]
use crate::macos_terminate::reply_to_terminate;

/// Only macOS has a system terminate to answer.
#[cfg(not(target_os = "macos"))]
fn reply_to_terminate(_app: &AppHandle, _terminate: bool) {}

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
            quit.begin(labels(&["main", "doc-2"]), |_| PromptState::Asked),
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
        quit.begin(labels(&["main", "doc-2", "doc-3"]), |_| PromptState::Asked);
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
            quit.begin(labels(&["main", "doc-2"]), |_| PromptState::Asked),
            Some("main".to_string())
        );
        // main shows its save prompt; doc-2 is closed meanwhile (by the user).
        assert_eq!(quit.advance_after_close("doc-2"), Advance::Waiting);
        assert_eq!(quit.advance_after_close("main"), Advance::Complete);
    }

    #[test]
    fn cancel_ends_the_quit_and_a_second_request_while_running_is_ignored() {
        let quit = AppQuit::default();
        quit.begin(labels(&["main"]), |_| PromptState::Asked);
        assert_eq!(
            quit.begin(labels(&["main", "doc-2"]), |_| PromptState::Asked),
            None
        );
        quit.cancel();
        assert_eq!(quit.advance_after_close("main"), Advance::Idle);
        assert_eq!(
            quit.begin(labels(&["doc-2"]), |_| PromptState::Asked),
            Some("doc-2".to_string())
        );
    }

    #[test]
    fn a_quit_that_has_not_moved_for_the_ack_timeout_is_started_over_by_the_next_request() {
        let quit = AppQuit::default();
        let start = Instant::now();
        assert_eq!(
            quit.begin_at(labels(&["main", "doc-2"]), start, |_| PromptState::Asked),
            Some("main".to_string())
        );
        let just_before = start + CLOSE_ACK_TIMEOUT - std::time::Duration::from_millis(1);
        assert_eq!(
            quit.begin_at(labels(&["doc-2"]), just_before, |_| PromptState::Asked),
            None
        );
        assert_eq!(
            quit.begin_at(
                labels(&["doc-2", "main"]),
                start + CLOSE_ACK_TIMEOUT,
                |_| PromptState::Asked
            ),
            Some("doc-2".to_string()),
            "the stuck quit is replaced"
        );
        // Progress (a window gone) restarts the clock.
        assert_eq!(
            quit.advance_after_close("doc-2"),
            Advance::Next("main".to_string())
        );
        assert_eq!(
            quit.begin_at(labels(&["main"]), Instant::now(), |_| PromptState::Asked),
            None
        );
    }

    /// A window whose page confirmed the save prompt is not stuck: the user is answering it. A new
    /// request must not start the queue over and ask another window (a second prompt) meanwhile.
    #[test]
    fn a_quit_waiting_on_a_confirmed_prompt_is_not_started_over() {
        let quit = AppQuit::default();
        let start = Instant::now();
        assert_eq!(
            quit.begin_at(labels(&["main", "doc-2"]), start, |_| PromptState::Asked),
            Some("main".to_string())
        );
        let later = start + CLOSE_ACK_TIMEOUT * 3;
        assert_eq!(
            quit.begin_at(labels(&["doc-2", "main"]), later, confirmed("main")),
            None,
            "main shows its prompt"
        );
        assert_eq!(
            quit.begin_at(labels(&["doc-2", "main"]), later, confirmed("doc-2")),
            Some("doc-2".to_string()),
            "the asked window never confirmed: the quit is stuck"
        );
    }

    /// The page of the window the quit waits on was reloaded: it lost the prompt it had confirmed
    /// and nobody is answering. The next request asks that window again (it shows the prompt
    /// anew) without starting the queue over.
    #[test]
    fn a_quit_whose_prompt_was_lost_asks_the_same_window_again() {
        let quit = AppQuit::default();
        let start = Instant::now();
        assert_eq!(
            quit.begin_at(labels(&["main", "doc-2"]), start, |_| PromptState::Asked),
            Some("main".to_string())
        );
        let soon = start + std::time::Duration::from_secs(1);
        assert_eq!(
            quit.begin_at(labels(&["doc-2", "main"]), soon, |_| PromptState::None),
            Some("main".to_string()),
            "main lost its prompt: asked again"
        );
        assert_eq!(
            quit.begin_at(labels(&["doc-2", "main"]), soon, |_| PromptState::Asked),
            None,
            "a prompt that was asked and not yet confirmed is waited for"
        );
        // The queue was not replaced: doc-2 is still behind main.
        assert_eq!(
            quit.advance_after_close("main"),
            Advance::Next("doc-2".to_string())
        );
    }

    /// The terminate path is called by AppKit and must not unwind into it, and its replies never
    /// run inside the event that triggered them.
    #[test]
    fn the_terminate_path_reads_missing_state_softly_and_replies_deferred() {
        let queue = include_str!("app_quit.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let request = &queue[queue.find("pub fn request_terminate").unwrap()..];
        let request = &request[..request.find("\n}\n").unwrap()];
        assert!(request.contains("try_state::<AppQuit>()"));
        assert!(request.contains("defer_to_main("));
        assert!(!request.contains("run_on_main_thread"));
        let terminate = include_str!("macos_terminate.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let reply = &terminate[terminate.find("pub fn reply_to_terminate").unwrap()..];
        assert!(reply.contains("defer_to_main("));
        assert!(!reply.contains("run_on_main_thread"));
    }

    #[test]
    fn a_terminate_without_windows_ends_at_once() {
        let quit = AppQuit::default();
        assert_eq!(
            quit.begin_terminate(Vec::new(), |_| PromptState::None),
            Terminate::Now
        );
        assert!(!quit.take_terminating(), "nothing is owed to the system");
    }

    #[test]
    fn a_terminate_with_windows_asks_the_first_and_is_answered_when_the_last_is_gone() {
        let quit = AppQuit::default();
        assert_eq!(
            quit.begin_terminate(labels(&["main", "doc-2"]), |_| PromptState::None),
            Terminate::Later(Some("main".to_string()))
        );
        // Every window confirms its own close in turn (a clean one closes, a dirty one prompts).
        assert_eq!(
            quit.advance_after_close("main"),
            Advance::Next("doc-2".to_string())
        );
        assert_eq!(quit.advance_after_close("doc-2"), Advance::Complete);
        assert!(quit.take_terminating(), "the system is owed its answer");
        assert!(!quit.take_terminating(), "answered once");
    }

    #[test]
    fn a_cancelled_prompt_answers_the_terminate_with_cancel_once() {
        let quit = AppQuit::default();
        quit.begin_terminate(labels(&["main", "doc-2"]), |_| PromptState::None);
        assert!(quit.cancel(), "the system gets 'cancel'");
        assert_eq!(quit.advance_after_close("main"), Advance::Idle);
        assert!(!quit.cancel(), "a second cancel owes nothing");
        assert!(!quit.take_terminating());
    }

    #[test]
    fn a_menu_quit_cancel_owes_the_system_nothing() {
        let quit = AppQuit::default();
        quit.begin(labels(&["main"]), |_| PromptState::Asked);
        assert!(!quit.cancel());
    }

    #[test]
    fn a_terminate_during_a_running_quit_waits_on_it_and_asks_nobody_again() {
        let quit = AppQuit::default();
        quit.begin(labels(&["main", "doc-2"]), |_| PromptState::Asked);
        assert_eq!(
            quit.begin_terminate(labels(&["main", "doc-2"]), |_| PromptState::Confirmed),
            Terminate::Later(None),
            "main shows its prompt: the running quit decides"
        );
        assert_eq!(
            quit.advance_after_close("main"),
            Advance::Next("doc-2".to_string())
        );
        assert_eq!(quit.advance_after_close("doc-2"), Advance::Complete);
        assert!(quit.take_terminating());
    }

    fn confirmed(only: &str) -> impl Fn(&str) -> PromptState + '_ {
        move |label| {
            if label == only {
                PromptState::Confirmed
            } else {
                PromptState::Asked
            }
        }
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
        // The terminate path (Dock, AppleScript, shutdown) is hooked before the loop runs.
        assert!(main.contains("macos_terminate::install(app.handle())"));
    }
}
