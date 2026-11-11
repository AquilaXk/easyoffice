//! Budgets, deadlines and the places for jobs of the PDF edit pipeline (PD-170).
//!
//! `hayro-syntax` (the reader) lists timeouts and out-of-memory protection among its open tasks, so
//! none of it comes from the crate, and counting what a file declares is not enough: a file of a
//! few hundred kilobytes can make the reader allocate gigabytes without breaking any count. So the
//! reader and the writer run in a worker process (`pdf_edit_worker.rs`) that the shell kills at the
//! deadline or when it holds too much memory (`pdf_edit_supervisor.rs`). The limits below are the
//! counts the worker applies on top (fail closed: nothing is shortened, skipped or guessed).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const MIB: usize = 1_048_576;

/// Largest single PDF that is opened or merged (bytes). The PDF window opens a file up to the same
/// size (`WindowKind::max_open_bytes`; a test in `windows.rs` keeps the two equal; this file is
/// also included by path in a test crate, so it cannot name that constant).
pub const MAX_SOURCE_BYTES: usize = 10 * MIB;
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
/// How long one request may run before the worker is killed.
pub const WORK_DEADLINE: Duration = Duration::from_secs(60);
/// How long a split may take in all (each file has the deadline of a request of its own).
pub const SPLIT_DEADLINE: Duration = Duration::from_secs(600);
/// A worker that is still alive this long after the deadline stops itself (the shell normally
/// kills it first; this is for a shell that is gone).
pub const WORKER_GRACE: Duration = Duration::from_secs(5);
/// Resident memory at which a worker is killed (bytes).
pub const MAX_CHILD_RSS_BYTES: u64 = 1536 * MIB as u64;
/// Address space of a worker on Linux (bytes); larger than the resident limit because the shell
/// binary maps its libraries too.
pub const MAX_CHILD_ADDRESS_SPACE_BYTES: u64 = 8 * 1024 * MIB as u64;
/// How long the worker of a start window card may draw one first page before it is killed. A
/// little less than the card's own deadline (`preview/mod.rs`), so the kill comes first.
pub const PREVIEW_DEADLINE: Duration = Duration::from_secs(8);
/// Resident memory at which the worker of a card is killed (bytes): a card is not worth the
/// 1.5 GiB an edit may use.
pub const PREVIEW_MAX_RSS_BYTES: u64 = 1024 * MIB as u64;
/// How often the memory of a worker is looked at.
pub const MEMORY_POLL: Duration = Duration::from_millis(10);
/// Workers that may run at once. A worker past its deadline is killed at once, so this is the
/// number of requests in flight.
pub const MAX_ACTIVE_JOBS: usize = 2;
/// Stack of the thread the worker does its work on. Parsing and copying recurse over nested objects
/// (the reader stops at 512 levels), which fits well inside this.
pub const WORKER_STACK_BYTES: usize = 32 * MIB;
/// Most entries of the resource dictionaries (fonts, pictures, ...) of the chosen pages together.
/// The library writes every entry again for every page, so this bounds the size of the result
/// before the copy starts.
pub const MAX_RESOURCE_ENTRIES: usize = 4_000_000;
/// Most items of a page's `/Contents` array.
pub const MAX_CONTENT_STREAMS: usize = 10_000;

pub const MSG_TIMEOUT: &str =
    "PDF 처리 시간이 한도를 넘어 중단했습니다. 파일이 지나치게 크거나 손상됐을 수 있습니다.";
pub const MSG_BUSY: &str = "이전 PDF 작업이 아직 끝나지 않았습니다. 잠시 뒤 다시 시도해 주세요.";
pub const MSG_PANIC: &str = "PDF를 처리하는 중 내부 오류가 발생해 작업을 중단했습니다.";
pub const MSG_MEMORY: &str =
    "PDF 처리에 필요한 메모리가 한도를 넘어 중단했습니다. 파일이 지나치게 크거나 손상됐을 수 있습니다.";
pub const MSG_WORKER_DIED: &str = "PDF 처리 프로세스가 비정상 종료해 작업을 중단했습니다.";

/// When a request has to be over. The work polls it between pages and files; the supervisor kills
/// the worker at the same moment, so a single long call into the PDF reader is cut off too.
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

/// The places for workers: [`MAX_ACTIVE_JOBS`] at a time in the process ([`JOBS`]); tests make
/// their own.
pub struct Jobs {
    active: AtomicUsize,
    max: usize,
}

pub static JOBS: Jobs = Jobs::new(MAX_ACTIVE_JOBS);
/// The places of the card previews: their own, so that a card never takes the place of an edit
/// (the card's own gate keeps at most this many in flight).
pub static PREVIEW_JOBS: Jobs = Jobs::new(MAX_ACTIVE_JOBS);

impl Jobs {
    pub const fn new(max: usize) -> Self {
        Self {
            active: AtomicUsize::new(0),
            max,
        }
    }

    /// A place, or `None` when all are taken. The place is given back when the claim is dropped.
    pub fn claim(&'static self) -> Option<Slot> {
        self.active
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < self.max).then_some(n + 1)
            })
            .ok()
            .map(|_| Slot(self))
    }

    #[cfg(test)]
    pub fn held(&self) -> usize {
        self.active.load(Ordering::Acquire)
    }
}

/// A claim on one of the places.
pub struct Slot(&'static Jobs);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jobs(max: usize) -> &'static Jobs {
        Box::leak(Box::new(Jobs::new(max)))
    }

    #[test]
    fn places_are_limited_and_given_back() {
        let jobs = jobs(MAX_ACTIVE_JOBS);
        let first = jobs.claim().expect("first");
        let second = jobs.claim().expect("second");
        assert!(jobs.claim().is_none(), "past the limit");
        assert_eq!(jobs.held(), 2);
        drop(first);
        let third = jobs.claim().expect("a place came back");
        drop((second, third));
        assert_eq!(jobs.held(), 0);
    }

    #[test]
    fn a_deadline_that_has_passed_fails_the_check() {
        let past = Deadline::after(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(2));
        assert_eq!(past.check().unwrap_err(), MSG_TIMEOUT);
        assert!(Deadline::after(Duration::from_secs(60)).check().is_ok());
    }
}
