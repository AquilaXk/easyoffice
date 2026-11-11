//! The PDF worker process (PD-166): `EasyOffice --pdf-edit-worker`.
//!
//! It is started by `pdf_edit_supervisor.rs`, before anything of the shell exists: no window, no
//! IPC, no plugin. It reads one request from its standard input (`pdf_edit_wire.rs`), does it with
//! the PDF reader and writer (`pdf_edit_inspect.rs`, `pdf_edit_build.rs`), writes the answer to its
//! standard output and exits. It is given bytes and returns bytes: a path never reaches it, and
//! the stamp and watermark forms it needs come from the shell as bytes too (the shell owns the
//! fonts).
//!
//! The shell kills it at the deadline or at the memory limit. It also watches itself, so that a
//! shell that is slow to look, or gone, does not leave it running: past the deadline or the
//! resident memory limit it exits with a code that says which.

use crate::pdf_edit_budget::{
    Deadline, MAX_CHILD_ADDRESS_SPACE_BYTES, MAX_CHILD_RSS_BYTES, MAX_PAGES, MAX_SESSION_BYTES,
    MAX_SOURCES, MAX_SOURCE_BYTES, MEMORY_POLL, MSG_PANIC, WORKER_GRACE, WORKER_STACK_BYTES,
    WORK_DEADLINE,
};
use crate::pdf_edit_build::build;
use crate::pdf_edit_inspect::inspect;
use crate::pdf_edit_os::{limit_own_address_space, resident_bytes};
use crate::pdf_edit_overlay::Overlay;
use crate::pdf_edit_wire::{
    read_frame, write_frame, BuildHead, EXIT_DEADLINE, EXIT_MEMORY, NEED_OVERLAY, OVERLAY_REPLY,
    REQ_BUILD, REQ_INSPECT, RES_ERR, RES_OK, SOURCE,
};
use std::io::{self, Read, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;
use std::time::Instant;

/// Runs the worker and returns the process exit code.
pub fn run() -> i32 {
    limit_own_address_space(MAX_CHILD_ADDRESS_SPACE_BYTES);
    watch_self();
    let work = std::thread::Builder::new()
        .name("pdf-edit-worker".to_string())
        .stack_size(WORKER_STACK_BYTES)
        .spawn(|| {
            let (stdin, stdout) = (io::stdin(), io::stdout());
            let (mut input, mut output) = (stdin.lock(), io::BufWriter::new(stdout.lock()));
            let outcome = catch_unwind(AssertUnwindSafe(|| serve(&mut input, &mut output)));
            if outcome.is_err() {
                let _ = write_frame(&mut output, RES_ERR, &[MSG_PANIC.as_bytes()]);
            }
            let _ = output.flush();
            outcome.is_ok_and(|done| done.is_ok())
        });
    match work.map(std::thread::JoinHandle::join) {
        Ok(Ok(true)) => 0,
        _ => 1,
    }
}

/// Ends the process when it has run past the deadline or holds too much memory.
fn watch_self() {
    let started = Instant::now();
    let _ = std::thread::Builder::new()
        .name("pdf-edit-watch".to_string())
        .spawn(move || loop {
            std::thread::sleep(MEMORY_POLL);
            if started.elapsed() > WORK_DEADLINE + WORKER_GRACE {
                std::process::exit(EXIT_DEADLINE);
            }
            if resident_bytes(std::process::id()).is_some_and(|rss| rss > MAX_CHILD_RSS_BYTES) {
                std::process::exit(EXIT_MEMORY);
            }
        });
}

/// Reads one request and writes its answer.
fn serve(input: &mut impl Read, output: &mut impl Write) -> io::Result<()> {
    let deadline = Deadline::after(WORK_DEADLINE);
    let (tag, payload) = read_frame(input)?;
    let answer = match tag {
        REQ_INSPECT => inspect(&Arc::new(payload), &deadline).map(|info| {
            u32::try_from(info.pages)
                .unwrap_or(u32::MAX)
                .to_le_bytes()
                .to_vec()
        }),
        REQ_BUILD => build_request(&payload, input, output, &deadline),
        _ => Err(MSG_PANIC.to_string()),
    };
    match answer {
        Ok(bytes) => write_frame(output, RES_OK, &[&bytes]),
        Err(message) => write_frame(output, RES_ERR, &[message.as_bytes()]),
    }
}

/// A build: the head is in `head`, the sources follow on the input. The overlays are asked of the
/// shell on the output and answered on the input.
fn build_request(
    head: &[u8],
    input: &mut impl Read,
    output: &mut impl Write,
    deadline: &Deadline,
) -> Result<Vec<u8>, String> {
    let head = BuildHead::decode(head, MAX_SOURCES, MAX_PAGES).ok_or_else(bad_request)?;
    let mut sources = Vec::with_capacity(head.sources);
    let mut held = 0usize;
    for _ in 0..head.sources {
        let (tag, bytes) = read_frame(input).map_err(|_| bad_request())?;
        held = held.saturating_add(bytes.len());
        if tag != SOURCE || bytes.len() > MAX_SOURCE_BYTES || held > MAX_SESSION_BYTES {
            return Err(bad_request());
        }
        sources.push(Arc::new(bytes));
    }
    let overlay = Overlay::new(head.watermark.as_deref(), head.stamp.as_deref())?;
    let mut ask = |width: f32, height: f32, stamp: bool| -> Result<Vec<u8>, String> {
        let need = crate::pdf_edit_wire::encode_overlay_need(width, height, stamp);
        write_frame(output, NEED_OVERLAY, &[&need]).map_err(|_| bad_request())?;
        match read_frame(input) {
            Ok((OVERLAY_REPLY, pdf)) => Ok(pdf),
            Ok((_, message)) => Err(String::from_utf8_lossy(&message).into_owned()),
            Err(_) => Err(bad_request()),
        }
    };
    build(&sources, &head.pages, &overlay, &mut ask, deadline)
}

fn bad_request() -> String {
    MSG_PANIC.to_string()
}
