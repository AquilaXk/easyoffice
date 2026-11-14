//! Test support: a named pipe in place of a file. Opening one for reading blocks until a program
//! writes to it, so a read path that does not look at the kind of the file first never returns.

use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

/// How long a path that opens a file may take before it counts as blocked.
const BLOCKED_AFTER: Duration = Duration::from_secs(3);

/// Makes a named pipe at `path`.
pub(crate) fn make_pipe(path: &Path) {
    let made = std::process::Command::new("mkfifo")
        .arg(path)
        .status()
        .is_ok_and(|status| status.success());
    assert!(made, "mkfifo");
}

/// The result of `work`, or `None` when it did not return in time. A blocked `work` is let go by
/// opening the pipe for writing, which ends its open, so the test leaves no thread behind.
pub(crate) fn within_time<T: Send + 'static>(
    pipe: &Path,
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    let result = rx.recv_timeout(BLOCKED_AFTER).ok();
    if result.is_none() {
        let _ = std::fs::OpenOptions::new().write(true).open(pipe);
    }
    result
}
