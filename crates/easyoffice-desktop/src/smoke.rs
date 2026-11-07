//! `EasyOffice.exe --smoke`: opens the real window and passes only when the embedded WebUI
//! (1) completed an IPC round trip through the WebView bridge and (2) completed the HWP editor
//! round trip: activate the HWP module, wait for the embedded rhwp editor, create a blank document,
//! type into it, export HWP bytes and verify them. The page reports (2) through an ordinary engine
//! call (`scrub_pii` with a marker text) that this probe observes, so no extra IPC permission exists.
//! The window round trip (PD-088): after the HWP round trip the first window opens a second document
//! window; that window reports that it loaded (same marker mechanism) and closes itself, and the
//! probe must see its `Destroyed` event. Exits 0 on success, 1 on failure or timeout. Used by CI to prove the shell loads its bundled
//! assets (including the editor bundle) and reaches the engine without a server.

use crate::windows::PRIMARY_LABEL;
use std::sync::Mutex;
use std::time::Duration;

pub const SMOKE_TIMEOUT: Duration = Duration::from_secs(75);
const HWP_OK_MARKER: &str = "EASYOFFICE_SMOKE_HWP_ROUNDTRIP_OK ";
const HWP_FAIL_MARKER: &str = "EASYOFFICE_SMOKE_HWP_ROUNDTRIP_FAIL ";
const WINDOW_OK_MARKER: &str = "EASYOFFICE_SMOKE_WINDOW_OK ";

/// Injected before the page scripts run, in smoke mode only; the page then drives the HWP round trip.
pub const SMOKE_INIT_SCRIPT: &str = "window.__EASYOFFICE_SMOKE__ = true;";

pub struct SmokeProbe {
    progress: Mutex<SmokeProgress>,
}

impl SmokeProbe {
    /// Starts the watchdog that fails the smoke run if the UI never completes the round trips.
    pub fn start(timeout: Duration) -> Self {
        std::thread::spawn(move || {
            std::thread::sleep(timeout);
            eprintln!("smoke: not completed within {}s", timeout.as_secs());
            std::process::exit(1);
        });
        Self {
            progress: Mutex::new(SmokeProgress::default()),
        }
    }

    /// Observes every engine call the UI makes through the shell bridge.
    pub fn observe(&self, command: &str, payload: &str, ok: bool) {
        let step = match self.progress.lock() {
            Ok(mut progress) => progress.step(command, payload, ok),
            Err(_) => SmokeStep::Failed("smoke state unavailable".to_string()),
        };
        Self::finish(step);
    }

    /// Observes that a window was destroyed (the second window closing cleanly).
    pub fn window_closed(&self, label: &str) {
        let step = match self.progress.lock() {
            Ok(mut progress) => progress.window_closed(label),
            Err(_) => SmokeStep::Failed("smoke state unavailable".to_string()),
        };
        Self::finish(step);
    }

    fn finish(step: SmokeStep) {
        match step {
            SmokeStep::Passed(detail) => {
                println!(
                    "smoke: WebUI IPC, HWP editor and second-window round trips passed ({detail})"
                );
                std::process::exit(0);
            }
            SmokeStep::Failed(reason) => {
                eprintln!("smoke: failed: {reason}");
                std::process::exit(1);
            }
            SmokeStep::Waiting => {}
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SmokeStep {
    Waiting,
    Passed(String),
    Failed(String),
}

#[derive(Default)]
struct SmokeProgress {
    bootstrapped: bool,
    ribbon: bool,
    hwp: Option<String>,
    window_loaded: Option<String>,
    window_closed: bool,
}

impl SmokeProgress {
    /// The UI requests `get_workspace_bootstrap` first and only asks for ribbon commands after it
    /// has parsed that response, so a successful second step proves a full round trip.
    fn step(&mut self, command: &str, payload: &str, ok: bool) -> SmokeStep {
        if !ok {
            return SmokeStep::Failed(format!("IPC command {command} failed"));
        }
        match command {
            "get_workspace_bootstrap" => self.bootstrapped = true,
            "get_ribbon_category_commands" if self.bootstrapped => self.ribbon = true,
            "scrub_pii" => {
                if let Some(reason) = payload.strip_prefix(HWP_FAIL_MARKER) {
                    return SmokeStep::Failed(format!("HWP editor round trip: {reason}"));
                }
                if let Some(detail) = payload.strip_prefix(HWP_OK_MARKER) {
                    self.hwp = Some(detail.to_string());
                }
                if let Some(module) = payload.strip_prefix(WINDOW_OK_MARKER) {
                    self.window_loaded = Some(module.to_string());
                }
            }
            _ => {}
        }
        self.verdict()
    }

    /// The second window counts as closed only after it reported that it had loaded.
    fn window_closed(&mut self, label: &str) -> SmokeStep {
        if label != PRIMARY_LABEL && self.window_loaded.is_some() {
            self.window_closed = true;
        }
        self.verdict()
    }

    fn verdict(&self) -> SmokeStep {
        match (&self.hwp, &self.window_loaded) {
            (Some(detail), Some(module))
                if self.bootstrapped && self.ribbon && self.window_closed =>
            {
                SmokeStep::Passed(format!("{detail}, window={module}"))
            }
            _ => SmokeStep::Waiting,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_requires_bootstrap_ribbon_hwp_and_second_window_round_trips() {
        let mut p = SmokeProgress::default();
        assert_eq!(
            p.step("get_ribbon_category_commands", "", true),
            SmokeStep::Waiting
        );
        assert!(!p.ribbon, "ribbon before bootstrap does not count");
        assert_eq!(
            p.step("get_workspace_bootstrap", "", true),
            SmokeStep::Waiting
        );
        assert_eq!(
            p.step("get_ribbon_category_commands", "HWP|module", true),
            SmokeStep::Waiting
        );
        assert_eq!(
            p.step(
                "scrub_pii",
                "EASYOFFICE_SMOKE_HWP_ROUNDTRIP_OK bytes=512",
                true
            ),
            SmokeStep::Waiting,
            "the second window is still missing"
        );
        assert_eq!(
            p.window_closed("doc-1-a"),
            SmokeStep::Waiting,
            "closing before loading does not count"
        );
        assert_eq!(
            p.step("scrub_pii", "EASYOFFICE_SMOKE_WINDOW_OK DOC", true),
            SmokeStep::Waiting,
            "loaded but not yet closed"
        );
        assert_eq!(
            p.window_closed("main"),
            SmokeStep::Waiting,
            "the first window is not the second window"
        );
        assert_eq!(
            p.window_closed("doc-1-a"),
            SmokeStep::Passed("bytes=512, window=DOC".to_string())
        );
    }

    #[test]
    fn ipc_round_trips_alone_do_not_pass() {
        let mut p = SmokeProgress::default();
        p.step("get_workspace_bootstrap", "", true);
        assert_eq!(
            p.step("get_ribbon_category_commands", "", true),
            SmokeStep::Waiting
        );
        assert_eq!(
            p.step("scrub_pii", "ordinary text", true),
            SmokeStep::Waiting
        );
    }

    #[test]
    fn hwp_and_window_round_trips_without_the_ipc_round_trip_do_not_pass() {
        let mut p = SmokeProgress::default();
        p.step(
            "scrub_pii",
            "EASYOFFICE_SMOKE_HWP_ROUNDTRIP_OK bytes=1",
            true,
        );
        p.step("scrub_pii", "EASYOFFICE_SMOKE_WINDOW_OK DOC", true);
        assert_eq!(p.window_closed("doc-1-a"), SmokeStep::Waiting);
    }

    #[test]
    fn hwp_failure_marker_and_failed_commands_fail_the_run() {
        let mut p = SmokeProgress::default();
        assert_eq!(
            p.step(
                "scrub_pii",
                "EASYOFFICE_SMOKE_HWP_ROUNDTRIP_FAIL 편집기 준비 시간 초과",
                true
            ),
            SmokeStep::Failed("HWP editor round trip: 편집기 준비 시간 초과".to_string())
        );
        assert!(matches!(
            p.step("scrub_pii", "x", false),
            SmokeStep::Failed(_)
        ));
    }
}
