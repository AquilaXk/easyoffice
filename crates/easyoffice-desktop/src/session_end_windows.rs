//! Windows logoff and shutdown (PD-191). tao 0.37.2 does not handle `WM_QUERYENDSESSION` and ends
//! the process in `WM_ENDSESSION` (`event_loop.rs:2338`), so a logoff or shutdown over unsaved
//! documents lost them without a question. Neither tauri, tauri-runtime-wry nor tao has a hook for
//! it: the `msg_hook` of the event loop only sees posted messages, and `WM_QUERYENDSESSION` is sent.
//! Every window is therefore subclassed (`SetWindowSubclass`, the window procedure chain stays
//! intact). While a window has unsaved changes the subclass answers `WM_QUERYENDSESSION` with
//! FALSE, registers the reason (`ShutdownBlockReasonCreate`) and starts the quit queue
//! (`app_quit::request_session_end`); the queue's end lifts the block (`release_block`) and exits,
//! or only lifts it when the user cancelled. Nothing unsaved: the message passes on unanswered and
//! the shutdown is not held.
//!
//! The reason hangs on a window handle and goes with that window, so [`rehome_block`] moves it to
//! another open window before the one carrying it is destroyed: from the close request that lets a
//! window close and from the command that destroys one, not after the `Destroyed` event, when the
//! system has already dropped the reason and nothing holds the shutdown for a moment. The same call
//! after the event is the fallback for a window destroyed any other way.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::atomic::{AtomicUsize, Ordering};

/// What the system's shutdown screen shows next to the app.
const BLOCK_REASON: &str = "저장하지 않은 문서";

/// Identifies this subclass among others on the same window ("EOSE").
const SUBCLASS_ID: usize = 0x454F_5345;

/// The reason as the NUL-terminated UTF-16 string `ShutdownBlockReasonCreate` takes.
fn reason_utf16() -> Vec<u16> {
    BLOCK_REASON.encode_utf16().chain([0]).collect()
}

/// The window handle (0: none) carrying the shutdown block. One block per process: a second
/// `WM_QUERYENDSESSION` while it stands neither registers nor starts anything again.
#[derive(Default)]
struct BlockSlot(AtomicUsize);

impl BlockSlot {
    /// Takes the slot for `hwnd`; false when a window already carries the block.
    fn claim(&self, hwnd: usize) -> bool {
        self.0
            .compare_exchange(0, hwnd, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    fn current(&self) -> usize {
        self.0.load(Ordering::Acquire)
    }

    /// Hands the block to `hwnd`; the window that carried it, or `None` when there is no block or
    /// `hwnd` carries it already.
    fn move_to(&self, hwnd: usize) -> Option<usize> {
        let mut old = self.current();
        while old != 0 && old != hwnd {
            match self
                .0
                .compare_exchange(old, hwnd, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return Some(old),
                Err(now) => old = now,
            }
        }
        None
    }

    /// Frees the slot; the window that carried the block, if any.
    fn release(&self) -> Option<usize> {
        Some(self.0.swap(0, Ordering::AcqRel)).filter(|&hwnd| hwnd != 0)
    }
}

static BLOCK: BlockSlot = BlockSlot(AtomicUsize::new(0));

/// Whether the block has to move off its carrier because the window `own` (the one being closed,
/// if it still exists) is the carrier, or the carrier is gone. A carrier that is another open window
/// keeps it.
fn carrier_must_move(carrier: usize, own: Option<usize>, carrier_exists: bool) -> bool {
    carrier != 0 && (own == Some(carrier) || !carrier_exists)
}

#[cfg(windows)]
pub use native::{attach, rehome_block, release_block};

#[cfg(windows)]
mod native {
    use std::sync::OnceLock;

    use tauri::{AppHandle, Manager, WebviewWindow};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::Shutdown::{
        ShutdownBlockReasonCreate, ShutdownBlockReasonDestroy,
    };
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsWindow, WM_NCDESTROY, WM_QUERYENDSESSION};

    use super::{carrier_must_move, reason_utf16, BLOCK, SUBCLASS_ID};
    use crate::window_commands::AppWindows;

    /// The window procedure has no context pointer.
    static APP: OnceLock<AppHandle> = OnceLock::new();

    /// The result of `WM_QUERYENDSESSION` that keeps the session running.
    const FALSE: LRESULT = 0;

    /// Subclasses `window` so its `WM_QUERYENDSESSION` is answered here. Call once per window,
    /// right after it is built.
    pub fn attach(window: &WebviewWindow) {
        let app = window.app_handle();
        let _ = APP.set(app.clone());
        let hwnd = match window.hwnd() {
            Ok(hwnd) => hwnd.0 as usize,
            Err(err) => {
                eprintln!("session_end: no window handle: {err}");
                return;
            }
        };
        // `SetWindowSubclass` must run on the thread of the window.
        if let Err(err) = app.run_on_main_thread(move || subclass(hwnd)) {
            eprintln!("session_end: {err}");
        }
    }

    fn subclass(hwnd: usize) {
        // SAFETY: `hwnd` is a live window of this thread (windows are created on the main thread,
        // which runs this) and `subclass_proc` has the `SUBCLASSPROC` signature. The subclass is
        // removed in `WM_NCDESTROY`.
        let added = unsafe { SetWindowSubclass(hwnd as HWND, Some(subclass_proc), SUBCLASS_ID, 0) };
        if added == 0 {
            eprintln!("session_end: SetWindowSubclass failed");
        }
    }

    unsafe extern "system" fn subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        if msg == WM_QUERYENDSESSION && block_session_end(hwnd as usize) {
            return FALSE;
        }
        if msg == WM_NCDESTROY {
            // SAFETY: this window, this procedure and this id are what `subclass` registered; the
            // window is going away and the chain below still gets the message.
            unsafe { RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID) };
        }
        // SAFETY: the next procedure of the chain gets this call's own arguments.
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    /// True when the session end is held: a window has unsaved changes. The first hold registers the
    /// reason on `hwnd` and starts the queue (not inside this window procedure: it is deferred to
    /// the event loop).
    fn block_session_end(hwnd: usize) -> bool {
        let Some(app) = APP.get() else { return false };
        let Some(windows) = app.try_state::<AppWindows>() else {
            return false;
        };
        if !windows.has_unsaved_documents() {
            return false;
        }
        // A carrier that no longer exists took its reason with it: the block must be made anew.
        let carrier = BLOCK.current();
        if carrier != 0 && !window_exists(carrier) {
            BLOCK.release();
        }
        if BLOCK.claim(hwnd) {
            create_reason(hwnd);
            crate::app_quit::defer_to_main(app, crate::app_quit::request_session_end);
        }
        true
    }

    fn window_exists(hwnd: usize) -> bool {
        // SAFETY: a handle value only; `IsWindow` answers false for one that is not a window.
        unsafe { IsWindow(hwnd as HWND) != 0 }
    }

    fn create_reason(hwnd: usize) {
        let reason = reason_utf16();
        // SAFETY: `reason` is NUL-terminated UTF-16 that outlives the call (the system copies it)
        // and `hwnd` is a top-level window of this thread.
        let created = unsafe { ShutdownBlockReasonCreate(hwnd as HWND, reason.as_ptr()) };
        if created == 0 {
            eprintln!("session_end: ShutdownBlockReasonCreate failed");
        }
    }

    fn destroy_reason(hwnd: usize) {
        // SAFETY: a handle value only; a window that is gone makes the call fail, which is harmless.
        unsafe { ShutdownBlockReasonDestroy(hwnd as HWND) };
    }

    /// The queue ended (the app exits) or the user cancelled it: the shutdown is no longer held.
    pub fn release_block() {
        if let Some(hwnd) = BLOCK.release() {
            destroy_reason(hwnd);
        }
    }

    /// The window `gone` is about to be destroyed (or has been): a block carried by it moves to
    /// another open window, the new reason registered before the old one is dropped. Nothing to do
    /// without a block, when another window carries it, or without another window.
    pub fn rehome_block(app: &AppHandle, gone: &str) {
        let carrier = BLOCK.current();
        let own = app
            .get_webview_window(gone)
            .and_then(|window| window.hwnd().ok())
            .map(|hwnd| hwnd.0 as usize);
        if carrier == 0 || !carrier_must_move(carrier, own, window_exists(carrier)) {
            return;
        }
        let next = app
            .webview_windows()
            .into_iter()
            .filter(|(label, _)| label != gone)
            .find_map(|(_, window)| window.hwnd().ok());
        if let Some(next) = next.map(|hwnd| hwnd.0 as usize) {
            if let Some(old) = BLOCK.move_to(next) {
                create_reason(next);
                destroy_reason(old);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reason_is_a_nul_terminated_utf16_string() {
        let reason = reason_utf16();
        assert_eq!(reason.last(), Some(&0));
        assert_eq!(
            String::from_utf16(&reason[..reason.len() - 1]).unwrap(),
            "저장하지 않은 문서"
        );
        assert_eq!(reason.iter().filter(|&&unit| unit == 0).count(), 1);
    }

    #[test]
    fn one_window_carries_the_block_and_a_second_query_starts_nothing_again() {
        let slot = BlockSlot::default();
        assert!(slot.claim(7));
        assert!(!slot.claim(9), "the block stands: no second registration");
        assert_eq!(slot.current(), 7);
        assert_eq!(slot.release(), Some(7));
        assert_eq!(slot.release(), None, "released once");
        assert!(slot.claim(9), "a later session end blocks again");
    }

    #[test]
    fn the_block_leaves_a_carrier_that_is_closing_or_gone_and_stays_on_another_open_window() {
        // No block.
        assert!(!carrier_must_move(0, Some(7), true));
        // The window being closed carries it: move before it is destroyed.
        assert!(carrier_must_move(7, Some(7), true));
        // Another open window carries it.
        assert!(!carrier_must_move(7, Some(5), true));
        assert!(!carrier_must_move(7, None, true));
        // The carrier is gone already (the event came first): move now.
        assert!(carrier_must_move(7, None, false));
        assert!(carrier_must_move(7, Some(5), false));
    }

    #[test]
    fn the_block_moves_to_another_window_only_while_it_stands() {
        let slot = BlockSlot::default();
        assert_eq!(slot.move_to(5), None, "no block: nothing to move");
        assert_eq!(slot.current(), 0);
        slot.claim(7);
        assert_eq!(slot.move_to(7), None, "already there");
        assert_eq!(slot.move_to(5), Some(7));
        assert_eq!(slot.current(), 5);
    }

    /// The hook is wired the way the module's header says: every built window is subclassed, the
    /// subclass answers `WM_QUERYENDSESSION` and registers and lifts the reason, the queue starts
    /// through the shared session-end entry, and no unsafe block lacks its SAFETY comment.
    #[test]
    fn the_windows_hook_is_wired_to_the_shared_queue() {
        let own = include_str!("session_end_windows.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        for needed in [
            "SetWindowSubclass",
            "RemoveWindowSubclass",
            "WM_QUERYENDSESSION",
            "ShutdownBlockReasonCreate",
            "ShutdownBlockReasonDestroy",
            "crate::app_quit::request_session_end",
            "has_unsaved_documents()",
        ] {
            assert!(own.contains(needed), "{needed}");
        }
        let lines: Vec<&str> = own.lines().collect();
        for (at, line) in lines.iter().enumerate() {
            if line.contains("unsafe {") {
                assert!(
                    lines[at.saturating_sub(5)..at]
                        .iter()
                        .any(|before| before.contains("// SAFETY:")),
                    "an unsafe block without a SAFETY comment: {line}"
                );
            }
        }
        let windows = include_str!("window_commands.rs");
        assert!(windows.contains("crate::session_end_windows::attach(&window)"));
        // The block moves before the window is destroyed, on both ways a window closes.
        let close_now = windows.split("CloseAction::CloseNow").nth(1).unwrap();
        assert!(
            close_now.contains("hand_over_shutdown_block"),
            "close request"
        );
        let destroy = windows.split("pub fn close_current_window").nth(1).unwrap();
        let before_destroy = destroy.split("window.destroy()").next().unwrap();
        assert!(
            before_destroy.contains("hand_over_shutdown_block"),
            "destroy command"
        );
        let quit = include_str!("app_quit.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(quit.contains("crate::session_end_windows::release_block()"));
        assert!(quit.contains("crate::session_end_windows::rehome_block(app, label)"));
    }
}
