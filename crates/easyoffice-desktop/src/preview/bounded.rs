//! Runs one render on its own thread under a deadline and a worker limit (PD-142).
//!
//! The recent list may hold any file, and the parser and the layout cannot be cancelled from
//! outside. So a render runs on a worker thread with a large stack, the caller waits at most the
//! deadline, and a panic is caught on the worker. A worker that outlives its deadline is left to
//! finish (or to die with the process), but it keeps its permit until then: the number of render
//! threads that exist at once never passes the gate's size, whether they are busy or stuck. A request
//! that finds no free permit waits a short while and then gives up, so a stream of hostile files
//! cannot pile up threads and memory. Ported from the Quick Look crate's `bounded.rs` (same
//! repository, `apps/quicklook/rust/src/`), which counts only the abandoned workers.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

/// Layout recurses with the document's nesting; the platform's default thread stack is small.
const WORKER_STACK_BYTES: usize = 32 * 1024 * 1024;

/// Why a render gave no picture. Only [`RenderError::Busy`] is not the file's fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderError {
    /// Every permit is taken (by running or by stuck workers); nothing was tried.
    Busy,
    /// The worker did not answer within the deadline.
    TimedOut,
    /// The worker panicked, or could not be started.
    Panicked,
    /// The file is no document the engine can lay out, or its page cannot be drawn.
    Unreadable,
    /// The document has no page.
    Empty,
    /// The page's picture is larger than the limit.
    TooLarge,
}

/// A counting semaphore with a timed wait.
pub struct Gate {
    free: Mutex<usize>,
    released: Condvar,
}

/// One slot of the gate; dropping it frees the slot.
pub struct Permit(Arc<Gate>);

impl Gate {
    pub fn new(size: usize) -> Arc<Self> {
        Arc::new(Self {
            free: Mutex::new(size),
            released: Condvar::new(),
        })
    }

    /// A permit, waiting at most `wait` for one; `None` when none came free.
    pub fn acquire(self: &Arc<Self>, wait: Duration) -> Option<Permit> {
        let free = self.free.lock().unwrap_or_else(PoisonError::into_inner);
        let (mut free, _) = self
            .released
            .wait_timeout_while(free, wait, |free| *free == 0)
            .unwrap_or_else(PoisonError::into_inner);
        if *free == 0 {
            return None;
        }
        *free -= 1;
        Some(Permit(Arc::clone(self)))
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        *self.0.free.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        self.0.released.notify_one();
    }
}

/// The value of `work`, or the reason it was not produced. `work` runs on a new thread that holds a
/// permit of `gate` until it ends.
pub fn run<T: Send + 'static>(
    gate: &Arc<Gate>,
    permit_wait: Duration,
    deadline: Duration,
    work: impl FnOnce() -> Result<T, RenderError> + Send + 'static,
) -> Result<T, RenderError> {
    let permit = gate.acquire(permit_wait).ok_or(RenderError::Busy)?;
    let (sender, receiver) = mpsc::channel();
    thread::Builder::new()
        .name("eo-home-preview".to_string())
        .stack_size(WORKER_STACK_BYTES)
        .spawn(move || {
            let _permit = permit;
            let outcome =
                catch_unwind(AssertUnwindSafe(work)).unwrap_or(Err(RenderError::Panicked));
            // The caller may have given up already.
            let _ = sender.send(outcome);
        })
        .map_err(|_| RenderError::Panicked)?;
    match receiver.recv_timeout(deadline) {
        Ok(outcome) => outcome,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(RenderError::TimedOut),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(RenderError::Panicked),
    }
}
