//! Budgets and the guarded runner for editing PDFs that cannot be trusted (PD-156).
//!
//! `hayro-syntax` (the reader) lists timeouts and out-of-memory protection among its open tasks, so
//! none of it comes from the crate. Everything that touches a PDF here runs on its own thread with
//! a deadline and `catch_unwind` ([`run_guarded`]), after the checks of `pdf_edit_inspect.rs` have
//! counted what the file declares against the limits below. Past a limit the request is refused
//! (fail closed); nothing is shortened, skipped or guessed.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const MIB: usize = 1_048_576;

/// Largest single PDF that is opened or merged (bytes).
pub const MAX_SOURCE_BYTES: usize = 64 * MIB;
/// Largest sum of the PDFs one window keeps open for editing (bytes).
pub const MAX_SESSION_BYTES: usize = 256 * MIB;
/// Most PDFs one window edits together (the first one plus merged ones).
pub const MAX_SOURCES: usize = 64;
/// Most objects a PDF may define (the reader's own count, repaired files included).
pub const MAX_OBJECTS: usize = 1_000_000;
/// Most pages one PDF may have, and most pages one result may have.
pub const MAX_PAGES: usize = 5_000;
/// Deepest page tree (`/Pages` inside `/Pages`) that is followed; real files stay under ten.
pub const MAX_PAGE_TREE_DEPTH: usize = 64;
/// Most page tree nodes visited while counting depth (a tree of nothing but nodes is refused).
pub const MAX_PAGE_TREE_NODES: usize = 100_000;
/// Largest decoded content of one page (bytes).
pub const MAX_PAGE_CONTENT_BYTES: usize = 64 * MIB;
/// Largest sum of the decoded page contents of one result (bytes).
pub const MAX_TOTAL_CONTENT_BYTES: usize = 512 * MIB;
/// Largest decoded size of any one compressed stream in the file (bytes); see `scan_streams`.
pub const MAX_STREAM_INFLATED_BYTES: u64 = 256 * MIB as u64;
/// Largest sum of what every compressed stream in the file inflates to (bytes).
pub const MAX_TOTAL_INFLATED_BYTES: u64 = 1024 * MIB as u64;
/// Largest result (bytes); the same number as a saved document's (`save_file.rs`).
pub const MAX_OUTPUT_BYTES: usize = 256 * MIB;
/// Most files one split writes.
pub const MAX_SPLIT_FILES: usize = 200;
/// Largest sum of the files one split writes (bytes). Pages that share a font each carry a copy of
/// it, so the files of a split can add up to more than the PDF they come from.
pub const MAX_SPLIT_TOTAL_BYTES: usize = 1024 * MIB;
/// Most distinct page sizes that get an overlay of their own (stamp and watermark).
pub const MAX_OVERLAY_SIZES: usize = 128;
/// Longest stamp or watermark text (characters).
pub const MAX_OVERLAY_CHARS: usize = 100;
/// How long one request may run before the caller is told it took too long.
pub const WORK_DEADLINE: Duration = Duration::from_secs(60);
/// Jobs that may run at once. A job that outlives its deadline keeps its place until it ends, so a
/// stuck file cannot pile up threads: after this many, new requests are refused.
pub const MAX_ACTIVE_JOBS: usize = 2;
/// The worker's stack. Parsing and copying recurse over nested objects (the reader stops at 512
/// levels), which fits well inside this.
const WORKER_STACK_BYTES: usize = 32 * MIB;

pub const MSG_TIMEOUT: &str =
    "PDF 처리 시간이 한도를 넘어 중단했습니다. 파일이 지나치게 크거나 손상됐을 수 있습니다.";
pub const MSG_BUSY: &str = "이전 PDF 작업이 아직 끝나지 않았습니다. 잠시 뒤 다시 시도해 주세요.";
pub const MSG_PANIC: &str = "PDF를 처리하는 중 내부 오류가 발생해 작업을 중단했습니다.";

static ACTIVE_JOBS: AtomicUsize = AtomicUsize::new(0);

/// When a request has to be over. The work polls it between pages and files; a single call into
/// the PDF reader cannot be interrupted, which is why the caller stops waiting at the same time.
#[derive(Clone, Copy, Debug)]
pub struct Deadline(Instant);

impl Deadline {
    pub fn after(limit: Duration) -> Self {
        Self(Instant::now() + limit)
    }

    pub fn check(&self) -> Result<(), String> {
        if Instant::now() >= self.0 {
            Err(MSG_TIMEOUT.to_string())
        } else {
            Ok(())
        }
    }
}

/// A claim on one of the [`MAX_ACTIVE_JOBS`] places; the thread that holds it gives it back when it
/// ends, however it ends.
struct Slot;

impl Slot {
    fn claim() -> Option<Self> {
        ACTIVE_JOBS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < MAX_ACTIVE_JOBS).then_some(n + 1)
            })
            .ok()
            .map(|_| Self)
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        ACTIVE_JOBS.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Runs `work` on a thread of its own and waits for it at most `limit`. A panic of the work is
/// caught and reported as [`MSG_PANIC`] (the details go to the log); a work that does not return in
/// time is reported as [`MSG_TIMEOUT`] and left to end by itself at its next [`Deadline::check`].
pub fn run_guarded<T: Send + 'static>(
    limit: Duration,
    work: impl FnOnce(&Deadline) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let slot = Slot::claim().ok_or_else(|| MSG_BUSY.to_string())?;
    let deadline = Deadline::after(limit);
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("pdf-edit".to_string())
        .stack_size(WORKER_STACK_BYTES)
        .spawn(move || {
            let _slot = slot;
            let outcome = catch_unwind(AssertUnwindSafe(|| work(&deadline)));
            let _ = tx.send(outcome);
        })
        .map_err(|err| {
            eprintln!("pdf edit: the worker thread did not start: {err}");
            MSG_PANIC.to_string()
        })?;
    match rx.recv_timeout(limit + Duration::from_millis(250)) {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(MSG_PANIC.to_string()),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(MSG_TIMEOUT.to_string()),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(MSG_PANIC.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The jobs counter is process-wide, so the tests that fill it take turns.
    static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn turn() -> std::sync::MutexGuard<'static, ()> {
        TURN.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[test]
    fn a_result_comes_back_and_the_place_is_given_up() {
        let _turn = turn();
        let out = run_guarded(Duration::from_secs(5), |_| Ok(7)).unwrap();
        assert_eq!(out, 7);
        assert_eq!(ACTIVE_JOBS.load(Ordering::Acquire), 0);
    }

    #[test]
    fn a_panic_is_reported_not_propagated() {
        let _turn = turn();
        let err = run_guarded::<()>(Duration::from_secs(5), |_| panic!("bad pdf")).unwrap_err();
        assert_eq!(err, MSG_PANIC);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(ACTIVE_JOBS.load(Ordering::Acquire), 0, "the place is back");
    }

    #[test]
    fn a_work_that_never_checks_in_is_reported_as_too_slow_and_holds_its_place() {
        let _turn = turn();
        let release = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let held = std::sync::Arc::clone(&release);
        let err = run_guarded::<()>(Duration::from_millis(30), move |_| {
            while !held.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(())
        })
        .unwrap_err();
        assert_eq!(err, MSG_TIMEOUT);
        assert_eq!(ACTIVE_JOBS.load(Ordering::Acquire), 1, "still running");
        release.store(true, Ordering::Release);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(ACTIVE_JOBS.load(Ordering::Acquire), 0);
    }

    #[test]
    fn jobs_past_the_limit_are_refused_until_one_ends() {
        let _turn = turn();
        let release = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut stuck = Vec::new();
        for _ in 0..MAX_ACTIVE_JOBS {
            let held = std::sync::Arc::clone(&release);
            stuck.push(std::thread::spawn(move || {
                run_guarded::<()>(Duration::from_millis(20), move |_| {
                    while !held.load(Ordering::Acquire) {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Ok(())
                })
            }));
        }
        for handle in stuck {
            assert_eq!(handle.join().unwrap().unwrap_err(), MSG_TIMEOUT);
        }
        assert_eq!(
            run_guarded(Duration::from_secs(1), |_| Ok(1)).unwrap_err(),
            MSG_BUSY
        );
        release.store(true, Ordering::Release);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(run_guarded(Duration::from_secs(1), |_| Ok(1)).unwrap(), 1);
    }

    #[test]
    fn a_deadline_that_has_passed_fails_the_check() {
        let past = Deadline::after(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(2));
        assert_eq!(past.check().unwrap_err(), MSG_TIMEOUT);
        assert!(Deadline::after(Duration::from_secs(60)).check().is_ok());
    }
}
