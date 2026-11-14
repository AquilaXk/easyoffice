//! Runs one request on its own thread under a deadline and a worker limit.
//!
//! Finder hands the extension arbitrary files, and the parser and layout code cannot be cancelled
//! from outside. The request runs on a worker thread with a generous stack; the caller waits at most
//! `deadline` and answers `STATUS_TIMEOUT` if the worker is still busy (the worker is left to finish
//! or to die with the process). A panic never crosses the C boundary: it is caught on the worker and
//! becomes `STATUS_RENDER_FAILED`.
//!
//! Finder asks for many thumbnails at once, and each request copies its file (up to 50 MiB) and
//! decodes it, so the number of requests in flight is limited: a worker holds a permit from before
//! its input is copied until it ends, whether it is busy or stuck past its deadline. A request that
//! finds no free permit waits for one inside its own deadline and then gives up, so a stream of
//! hostile files cannot pile up threads and memory. The same gate the desktop shell's card
//! previews use (PD-142).

use crate::status::{STATUS_RENDER_FAILED, STATUS_TIMEOUT};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{mpsc, Condvar, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

/// Layout recurses with the document's nesting; the platform's default thread stack is small.
const WORKER_STACK_BYTES: usize = 32 * 1024 * 1024;
/// Requests worked on at once in this process.
pub const MAX_CONCURRENT_WORKERS: usize = 2;

/// A counting semaphore with a timed wait: how many workers may exist at once.
pub struct Limiter {
    free: Mutex<usize>,
    released: Condvar,
}

/// One slot of a [`Limiter`]; dropping it frees the slot.
struct Permit(&'static Limiter);

static PROCESS_LIMIT: Limiter = Limiter::new(MAX_CONCURRENT_WORKERS);

impl Limiter {
    pub const fn new(size: usize) -> Self {
        Self {
            free: Mutex::new(size),
            released: Condvar::new(),
        }
    }

    /// A permit, waiting at most `wait` for one; `None` when none came free.
    fn acquire(&'static self, wait: Duration) -> Option<Permit> {
        let free = self.free.lock().unwrap_or_else(PoisonError::into_inner);
        let (mut free, _) = self
            .released
            .wait_timeout_while(free, wait, |free| *free == 0)
            .unwrap_or_else(PoisonError::into_inner);
        if *free == 0 {
            return None;
        }
        *free -= 1;
        Some(Permit(self))
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        *self.0.free.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        self.0.released.notify_one();
    }
}

/// The value of `work`, or the status that stopped it. `input` runs on the caller's thread once a
/// permit is held, and what it makes goes to `work`: the copy of a large file is not made for a
/// request that has to wait.
pub fn run_with<I: Send + 'static, T: Send + 'static>(
    deadline: Duration,
    input: impl FnOnce() -> Result<I, i32>,
    work: impl FnOnce(I) -> Result<T, i32> + Send + 'static,
) -> Result<T, i32> {
    run_limited_with(&PROCESS_LIMIT, deadline, input, work)
}

/// [`run_limited_with`] with no input of its own, on a limiter of the caller's.
#[cfg(test)]
pub fn run_limited<T: Send + 'static>(
    limiter: &'static Limiter,
    deadline: Duration,
    work: impl FnOnce() -> Result<T, i32> + Send + 'static,
) -> Result<T, i32> {
    run_limited_with(limiter, deadline, || Ok(()), |()| work())
}

pub fn run_limited_with<I: Send + 'static, T: Send + 'static>(
    limiter: &'static Limiter,
    deadline: Duration,
    input: impl FnOnce() -> Result<I, i32>,
    work: impl FnOnce(I) -> Result<T, i32> + Send + 'static,
) -> Result<T, i32> {
    let started = Instant::now();
    let permit = limiter.acquire(deadline).ok_or(STATUS_TIMEOUT)?;
    let input = input()?;
    let (sender, receiver) = mpsc::channel();
    thread::Builder::new()
        .name("eo-quicklook".to_string())
        .stack_size(WORKER_STACK_BYTES)
        .spawn(move || {
            let _permit = permit;
            let outcome = catch_unwind(AssertUnwindSafe(move || work(input)))
                .unwrap_or(Err(STATUS_RENDER_FAILED));
            // The caller may have given up already.
            let _ = sender.send(outcome);
        })
        .map_err(|_| STATUS_RENDER_FAILED)?;
    receiver
        .recv_timeout(deadline.saturating_sub(started.elapsed()))
        .unwrap_or(Err(STATUS_TIMEOUT))
}
