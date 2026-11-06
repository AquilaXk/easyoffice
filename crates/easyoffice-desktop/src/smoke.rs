//! `EasyOffice.exe --smoke`: opens the real window and passes only when the embedded WebUI
//! (1) completed an IPC round trip through the WebView bridge and (2) completed the HWP editor
//! round trip: activate the HWP module, wait for the embedded rhwp editor, create a blank document,
//! type into it, export HWP bytes and verify them. The page reports (2) through an ordinary engine
//! call (`scrub_pii` with a marker text) that this probe observes, so no extra IPC permission exists.
//! Exits 0 on success, 1 on failure or timeout. Used by CI to prove the shell loads its bundled
//! assets (including the editor bundle) and reaches the engine without a server.

use std::sync::Mutex;
use std::time::Duration;

pub const SMOKE_TIMEOUT: Duration = Duration::from_secs(75);
const HWP_OK_MARKER: &str = "EASYOFFICE_SMOKE_HWP_ROUNDTRIP_OK ";
const HWP_FAIL_MARKER: &str = "EASYOFFICE_SMOKE_HWP_ROUNDTRIP_FAIL ";

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
        match step {
            SmokeStep::Passed(detail) => {
                println!("smoke: WebUI IPC round trip and HWP editor round trip passed ({detail})");
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
            }
            _ => {}
        }
        match (&self.hwp, self.bootstrapped && self.ribbon) {
            (Some(detail), true) => SmokeStep::Passed(detail.clone()),
            _ => SmokeStep::Waiting,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_requires_bootstrap_ribbon_and_hwp_round_trips() {
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
            SmokeStep::Passed("bytes=512".to_string())
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
