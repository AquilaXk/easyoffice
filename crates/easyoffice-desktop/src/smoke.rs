//! `EasyOffice.exe --smoke`: opens the real window, waits until the embedded WebUI has completed
//! an IPC round trip through the WebView bridge, then exits 0 (or 1 on failure/timeout). Used by
//! CI to prove the shell loads its bundled assets and reaches the engine without a server.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub const SMOKE_TIMEOUT: Duration = Duration::from_secs(60);

pub struct SmokeProbe {
    bootstrapped: AtomicBool,
}

impl SmokeProbe {
    /// Starts the watchdog that fails the smoke run if the UI never reaches the engine.
    pub fn start(timeout: Duration) -> Self {
        std::thread::spawn(move || {
            std::thread::sleep(timeout);
            eprintln!("smoke: no IPC round trip within {}s", timeout.as_secs());
            std::process::exit(1);
        });
        Self {
            bootstrapped: AtomicBool::new(false),
        }
    }

    /// The UI requests `get_workspace_bootstrap` first and only asks for ribbon commands after it
    /// has parsed that response, so a successful second step proves a full round trip.
    pub fn observe(&self, command: &str, ok: bool) {
        match smoke_step(command, ok, self.bootstrapped.load(Ordering::Acquire)) {
            SmokeStep::Bootstrapped => self.bootstrapped.store(true, Ordering::Release),
            SmokeStep::Passed => {
                println!("smoke: WebUI loaded and completed an IPC round trip");
                std::process::exit(0);
            }
            SmokeStep::Failed => {
                eprintln!("smoke: IPC command {command} failed");
                std::process::exit(1);
            }
            SmokeStep::Waiting => {}
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SmokeStep {
    Waiting,
    Bootstrapped,
    Passed,
    Failed,
}

fn smoke_step(command: &str, ok: bool, bootstrapped: bool) -> SmokeStep {
    match (command, ok, bootstrapped) {
        (_, false, _) => SmokeStep::Failed,
        ("get_workspace_bootstrap", true, false) => SmokeStep::Bootstrapped,
        ("get_ribbon_category_commands", true, true) => SmokeStep::Passed,
        _ => SmokeStep::Waiting,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_requires_bootstrap_before_ribbon_round_trip() {
        assert_eq!(
            smoke_step("get_ribbon_category_commands", true, false),
            SmokeStep::Waiting
        );
        assert_eq!(
            smoke_step("get_workspace_bootstrap", true, false),
            SmokeStep::Bootstrapped
        );
        assert_eq!(
            smoke_step("get_ribbon_category_commands", true, true),
            SmokeStep::Passed
        );
        assert_eq!(smoke_step("scrub_pii", false, true), SmokeStep::Failed);
    }
}
