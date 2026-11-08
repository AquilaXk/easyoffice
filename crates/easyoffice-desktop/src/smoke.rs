//! `EasyOffice.exe --smoke`: opens the real window and passes only when every step of
//! [`smoke_steps::required_steps`] has reported success (PD-088 item 12).
//!
//! Each step reports once, and the run prints one line per report: `EASYOFFICE_SMOKE_STEP_OK <name>
//! <detail>` on stdout or `EASYOFFICE_SMOKE_STEP_FAIL <name> <reason>` on stderr. Steps come from
//! three places:
//! * `ipc`: the page's first engine call (`get_workspace_bootstrap`) went through the bridge.
//! * Page steps (`hwp`, `pdf`, `print`, `window_loaded`): the page reports them through an ordinary
//!   engine call (`scrub_pii` with a marker text) that this probe observes, so no extra IPC
//!   permission exists. Only the names of `smoke_steps::PAGE_STEPS` are accepted.
//! * Shell steps (`window_state`, `preview`, `update`, and `menu` on macOS) run in Rust and report
//!   directly; `window` completes when the second document window (which reported `window_loaded`)
//!   is destroyed.
//!
//! Exits 0 when all steps passed, 1 on the first failed step, failed command or timeout. Used by CI
//! to prove the shell loads its bundled assets (including the editor bundle) and reaches the engine
//! without a server.

use crate::smoke_steps::{self as steps, PAGE_STEPS};
use crate::windows::PRIMARY_LABEL;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager};

pub const SMOKE_TIMEOUT: Duration = Duration::from_secs(75);
const STEP_OK_MARKER: &str = "EASYOFFICE_SMOKE_STEP_OK ";
const STEP_FAIL_MARKER: &str = "EASYOFFICE_SMOKE_STEP_FAIL ";
/// A report longer than this is cut (the text comes from the page and is printed).
const MAX_DETAIL_CHARS: usize = 200;

/// Injected before the page scripts run, in smoke mode only; the page then drives its steps.
pub const SMOKE_INIT_SCRIPT: &str = "window.__EASYOFFICE_SMOKE__ = true;";

pub struct SmokeProbe {
    progress: Mutex<SmokeProgress>,
}

impl SmokeProbe {
    /// Starts the watchdog that fails the smoke run if the steps never all complete.
    pub fn start(timeout: Duration) -> Self {
        std::thread::spawn(move || {
            std::thread::sleep(timeout);
            eprintln!("smoke: not completed within {}s", timeout.as_secs());
            std::process::exit(1);
        });
        Self {
            progress: Mutex::new(SmokeProgress::new(steps::required_steps())),
        }
    }

    /// Observes every engine call the UI makes through the shell bridge.
    pub fn observe(&self, command: &str, payload: &str, ok: bool) {
        self.apply(|progress| progress.observe(command, payload, ok));
    }

    /// Observes that a window was destroyed (the second window closing cleanly).
    pub fn window_closed(&self, label: &str) {
        self.apply(|progress| progress.window_closed(label));
    }

    /// A shell step reports its result.
    pub fn report(&self, name: &str, outcome: Result<String, String>) {
        self.apply(|progress| progress.record(name, outcome));
    }

    fn apply(&self, update: impl FnOnce(&mut SmokeProgress) -> SmokeStep) {
        let (step, log) = match self.progress.lock() {
            Ok(mut progress) => (update(&mut progress), progress.take_log()),
            Err(_) => (
                SmokeStep::Failed("smoke state unavailable".to_string()),
                Vec::new(),
            ),
        };
        for line in log {
            match line {
                LogLine::Out(text) => println!("{text}"),
                LogLine::Err(text) => eprintln!("{text}"),
            }
        }
        Self::finish(step);
    }

    fn finish(step: SmokeStep) {
        match step {
            SmokeStep::Passed(detail) => {
                println!("smoke: all steps passed ({detail})");
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

/// Starts the shell steps: the macOS menu check now (it needs the main thread, where the menu was
/// installed) and the others on a worker thread. Each reports to the managed [`SmokeProbe`].
pub fn run_rust_steps(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    app.state::<SmokeProbe>()
        .report(steps::MENU, steps::menu_step(app));
    let app = app.clone();
    let app_version = app.package_info().version.to_string();
    std::thread::spawn(move || {
        let probe = app.state::<SmokeProbe>();
        for (name, run) in steps::WORKER_STEPS {
            probe.report(name, run());
        }
        probe.report(steps::UPDATE, steps::update_check_step(&app_version));
    });
}

#[derive(Debug, PartialEq, Eq)]
enum SmokeStep {
    Waiting,
    Passed(String),
    Failed(String),
}

#[derive(Debug, PartialEq, Eq)]
enum LogLine {
    Out(String),
    Err(String),
}

/// A page report: `EASYOFFICE_SMOKE_STEP_OK <name> [detail]` or `..._FAIL <name> [reason]`.
fn parse_report(payload: &str) -> Option<(&str, Result<&str, &str>)> {
    let (rest, ok) = match payload.strip_prefix(STEP_OK_MARKER) {
        Some(rest) => (rest, true),
        None => (payload.strip_prefix(STEP_FAIL_MARKER)?, false),
    };
    let (name, text) = rest.split_once(' ').unwrap_or((rest, ""));
    Some((name, if ok { Ok(text) } else { Err(text) }))
}

fn clipped(text: &str) -> String {
    text.chars().take(MAX_DETAIL_CHARS).collect()
}

struct SmokeProgress {
    required: &'static [&'static str],
    done: HashMap<String, String>,
    log: Vec<LogLine>,
}

impl SmokeProgress {
    fn new(required: &'static [&'static str]) -> Self {
        Self {
            required,
            done: HashMap::new(),
            log: Vec::new(),
        }
    }

    fn take_log(&mut self) -> Vec<LogLine> {
        std::mem::take(&mut self.log)
    }

    /// Records one step result and writes its marker line. A failure ends the run at once.
    fn record(&mut self, name: &str, outcome: Result<String, String>) -> SmokeStep {
        match outcome {
            Ok(detail) => {
                let detail = clipped(&detail);
                self.log
                    .push(LogLine::Out(format!("{STEP_OK_MARKER}{name} {detail}")));
                self.done.insert(name.to_string(), detail);
                self.verdict()
            }
            Err(reason) => {
                let reason = clipped(&reason);
                self.log
                    .push(LogLine::Err(format!("{STEP_FAIL_MARKER}{name} {reason}")));
                SmokeStep::Failed(format!("{name}: {reason}"))
            }
        }
    }

    /// The UI requests `get_workspace_bootstrap` first; the page steps arrive later over the same
    /// IPC path as `scrub_pii` markers.
    fn observe(&mut self, command: &str, payload: &str, ok: bool) -> SmokeStep {
        if !ok {
            return SmokeStep::Failed(format!("IPC command {command} failed"));
        }
        match command {
            "get_workspace_bootstrap" => self.record(steps::IPC, Ok(String::new())),
            "scrub_pii" => match parse_report(payload) {
                Some((name, _)) if !PAGE_STEPS.contains(&name) => {
                    SmokeStep::Failed(format!("unknown page step {}", clipped(name)))
                }
                Some((name, outcome)) => {
                    self.record(name, outcome.map(str::to_string).map_err(str::to_string))
                }
                None => SmokeStep::Waiting,
            },
            _ => SmokeStep::Waiting,
        }
    }

    /// The second window counts as closed only after it reported that it had loaded.
    fn window_closed(&mut self, label: &str) -> SmokeStep {
        match self.done.get(steps::WINDOW_LOADED) {
            Some(module) if label != PRIMARY_LABEL && !self.done.contains_key(steps::WINDOW) => {
                let module = module.clone();
                self.record(steps::WINDOW, Ok(module))
            }
            _ => SmokeStep::Waiting,
        }
    }

    fn verdict(&self) -> SmokeStep {
        if self
            .required
            .iter()
            .all(|name| self.done.contains_key(*name))
        {
            SmokeStep::Passed(self.required.join(", "))
        } else {
            SmokeStep::Waiting
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REQUIRED: &[&str] = &["ipc", "hwp", "pdf", "window", "update"];

    fn page_ok(name: &str, detail: &str) -> String {
        format!("{STEP_OK_MARKER}{name} {detail}")
    }

    fn run_all_but(skip: &str) -> (SmokeProgress, SmokeStep) {
        let mut p = SmokeProgress::new(REQUIRED);
        let mut last = SmokeStep::Waiting;
        let reports: [(&str, &str, &str); 3] = [
            ("scrub_pii", "hwp", "bytes=512"),
            ("scrub_pii", "pdf", "pages=1"),
            ("scrub_pii", "window_loaded", "DOC"),
        ];
        for (command, name, detail) in reports {
            if name != skip {
                last = p.observe(command, &page_ok(name, detail), true);
            }
        }
        if skip != "ipc" {
            last = p.observe("get_workspace_bootstrap", "", true);
        }
        if skip != "update" {
            last = p.record("update", Ok("version=999.0.0".to_string()));
        }
        if skip != "window" {
            last = p.window_closed("doc-1-a");
        }
        (p, last)
    }

    #[test]
    fn the_run_passes_only_when_every_required_step_has_reported() {
        let (_, last) = run_all_but("none");
        assert_eq!(
            last,
            SmokeStep::Passed("ipc, hwp, pdf, window, update".to_string())
        );
        for missing in ["ipc", "hwp", "pdf", "window", "update"] {
            let (p, last) = run_all_but(missing);
            assert_eq!(last, SmokeStep::Waiting, "without {missing}");
            assert!(!p.done.contains_key(missing));
        }
    }

    #[test]
    fn each_report_writes_one_marker_line() {
        let mut p = SmokeProgress::new(REQUIRED);
        p.observe("scrub_pii", &page_ok("hwp", "bytes=512"), true);
        p.record("update", Ok("version=999.0.0".to_string()));
        assert_eq!(
            p.take_log(),
            vec![
                LogLine::Out("EASYOFFICE_SMOKE_STEP_OK hwp bytes=512".to_string()),
                LogLine::Out("EASYOFFICE_SMOKE_STEP_OK update version=999.0.0".to_string()),
            ]
        );
        assert!(p.take_log().is_empty());
    }

    #[test]
    fn a_failed_step_ends_the_run_and_writes_the_fail_marker() {
        let mut p = SmokeProgress::new(REQUIRED);
        let step = p.observe(
            "scrub_pii",
            &format!("{STEP_FAIL_MARKER}hwp 편집기 준비 시간 초과"),
            true,
        );
        assert_eq!(
            step,
            SmokeStep::Failed("hwp: 편집기 준비 시간 초과".to_string())
        );
        assert_eq!(
            p.take_log(),
            vec![LogLine::Err(
                "EASYOFFICE_SMOKE_STEP_FAIL hwp 편집기 준비 시간 초과".to_string()
            )]
        );
        assert!(matches!(
            p.record("preview", Err("no picture".to_string())),
            SmokeStep::Failed(_)
        ));
        assert!(matches!(
            p.observe("scrub_pii", "x", false),
            SmokeStep::Failed(_)
        ));
    }

    #[test]
    fn a_page_cannot_report_a_shell_step_or_an_unknown_one() {
        let mut p = SmokeProgress::new(REQUIRED);
        for name in [
            "update",
            "window",
            "ipc",
            "window_state",
            "menu",
            "bogus",
            "",
        ] {
            let step = p.observe("scrub_pii", &page_ok(name, "forged"), true);
            assert!(matches!(step, SmokeStep::Failed(_)), "{name:?}");
        }
        assert!(p.done.is_empty());
    }

    #[test]
    fn other_calls_and_ordinary_texts_are_not_steps() {
        let mut p = SmokeProgress::new(REQUIRED);
        for (command, payload) in [
            ("execute_ribbon_command", ""),
            ("scrub_pii", "ordinary text"),
            ("scrub_pii", "EASYOFFICE_SMOKE_HWP_ROUNDTRIP_OK bytes=1"),
            ("audit_security", &page_ok("hwp", "x")),
        ] {
            assert_eq!(p.observe(command, payload, true), SmokeStep::Waiting);
        }
        assert!(p.done.is_empty() && p.take_log().is_empty());
    }

    #[test]
    fn the_second_window_counts_only_after_it_loaded_and_is_not_the_first_window() {
        let mut p = SmokeProgress::new(REQUIRED);
        assert_eq!(
            p.window_closed("doc-1-a"),
            SmokeStep::Waiting,
            "not loaded yet"
        );
        p.observe("scrub_pii", &page_ok("window_loaded", "DOC"), true);
        assert_eq!(p.window_closed(PRIMARY_LABEL), SmokeStep::Waiting);
        assert!(!p.done.contains_key("window"));
        p.window_closed("doc-1-a");
        assert_eq!(p.done.get("window").map(String::as_str), Some("DOC"));
        let lines = p.take_log();
        assert_eq!(
            lines.last(),
            Some(&LogLine::Out(
                "EASYOFFICE_SMOKE_STEP_OK window DOC".to_string()
            ))
        );
    }

    #[test]
    fn long_details_are_cut_and_a_missing_detail_is_empty() {
        let mut p = SmokeProgress::new(REQUIRED);
        p.observe("scrub_pii", &page_ok("hwp", &"x".repeat(5_000)), true);
        assert_eq!(p.done["hwp"].chars().count(), MAX_DETAIL_CHARS);
        assert_eq!(
            parse_report("EASYOFFICE_SMOKE_STEP_OK pdf"),
            Some(("pdf", Ok("")))
        );
        assert_eq!(parse_report("EASYOFFICE_SMOKE_STEP_OK"), None);
        assert_eq!(parse_report("hello"), None);
    }

    #[test]
    fn the_shipping_step_list_is_what_the_probe_waits_for() {
        let mut p = SmokeProgress::new(steps::required_steps());
        assert_eq!(p.record(steps::IPC, Ok(String::new())), SmokeStep::Waiting);
        assert!(steps::required_steps().len() >= 8);
    }
}
