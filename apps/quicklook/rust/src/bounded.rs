//! Runs one request on its own thread under a deadline.
//!
//! Finder hands the extension arbitrary files, and the parser and layout code cannot be cancelled
//! from outside. The request runs on a worker thread with a generous stack; the caller waits at most
//! `deadline` and answers `STATUS_TIMEOUT` if the worker is still busy (the worker is left to finish
//! or to die with the process). A panic never crosses the C boundary: it is caught on the worker and
//! becomes `STATUS_RENDER_FAILED`. Requests may overlap (Finder asks for many thumbnails at once),
//! but once [`MAX_ABANDONED_WORKERS`] timed-out workers are still running, new requests are refused
//! until one finishes, so a stream of hostile files cannot pile up threads and memory.

use crate::status::{STATUS_RENDER_FAILED, STATUS_TIMEOUT};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

/// Layout recurses with the document's nesting; the platform's default thread stack is small.
const WORKER_STACK_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_ABANDONED_WORKERS: usize = 3;

const RUNNING: u8 = 0;
const ABANDONED: u8 = 1;
const FINISHED: u8 = 2;

/// How many timed-out workers may still be running.
pub struct Limiter {
    abandoned: AtomicUsize,
    max: usize,
}

static PROCESS_LIMIT: Limiter = Limiter::new(MAX_ABANDONED_WORKERS);

impl Limiter {
    pub const fn new(max: usize) -> Self {
        Self {
            abandoned: AtomicUsize::new(0),
            max,
        }
    }
}

/// The value of `work`, or the status that stopped it.
pub fn run<T: Send + 'static>(
    deadline: Duration,
    work: impl FnOnce() -> Result<T, i32> + Send + 'static,
) -> Result<T, i32> {
    run_limited(&PROCESS_LIMIT, deadline, work)
}

pub fn run_limited<T: Send + 'static>(
    limiter: &'static Limiter,
    deadline: Duration,
    work: impl FnOnce() -> Result<T, i32> + Send + 'static,
) -> Result<T, i32> {
    if limiter.abandoned.load(Ordering::Acquire) >= limiter.max {
        return Err(STATUS_TIMEOUT);
    }
    let state = Arc::new(AtomicU8::new(RUNNING));
    let (sender, receiver) = mpsc::channel();
    let worker_state = Arc::clone(&state);
    let spawned = thread::Builder::new()
        .name("eo-quicklook".to_string())
        .stack_size(WORKER_STACK_BYTES)
        .spawn(move || {
            let outcome = catch_unwind(AssertUnwindSafe(work)).unwrap_or(Err(STATUS_RENDER_FAILED));
            // The caller may have given up already.
            let _ = sender.send(outcome);
            if worker_state.swap(FINISHED, Ordering::AcqRel) == ABANDONED {
                limiter.abandoned.fetch_sub(1, Ordering::AcqRel);
            }
        });
    if spawned.is_err() {
        return Err(STATUS_RENDER_FAILED);
    }
    match receiver.recv_timeout(deadline) {
        Ok(outcome) => outcome,
        Err(_) => {
            // Count first: the worker may finish between the two steps.
            limiter.abandoned.fetch_add(1, Ordering::AcqRel);
            if state
                .compare_exchange(RUNNING, ABANDONED, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                limiter.abandoned.fetch_sub(1, Ordering::AcqRel);
                // It finished just now: its answer is waiting.
                return receiver.try_recv().unwrap_or(Err(STATUS_TIMEOUT));
            }
            Err(STATUS_TIMEOUT)
        }
    }
}
