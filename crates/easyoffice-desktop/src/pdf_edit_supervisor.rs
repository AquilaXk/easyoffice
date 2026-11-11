//! The shell's side of the PDF worker process (PD-164): start it, give it the request, answer its
//! requests for overlays, and kill it the moment it is too slow or holds too much memory.
//!
//! The worker is the shell's own binary started as `EasyOffice --pdf-edit-worker` with its
//! standard input and output connected to the shell (`pdf_edit_wire.rs`), no window, no IPC and
//! nothing but the bytes it is sent. A reader and a writer thread move the bytes; this thread looks
//! at the clock and at the worker's resident memory every [`MEMORY_POLL`] and ends the worker with a
//! kill, so a PDF that makes the reader allocate gigabytes or run for minutes costs the user one
//! sentence and nothing else: the shell and the other windows are not touched. Windows (job object)
//! and Linux (address space limit) also have the kernel enforce a memory limit.

use crate::pdf_edit_budget::{
    Jobs, JOBS, MAX_CHILD_RSS_BYTES, MAX_OVERLAY_SIZES, MEMORY_POLL, MSG_BUSY, MSG_MEMORY,
    MSG_TIMEOUT, MSG_WORKER_DIED, WORK_DEADLINE,
};
use crate::pdf_edit_os::{confine, resident_bytes};
use crate::pdf_edit_wire::{
    decode_overlay_need, read_frame, write_frame, BuildHead, OverlayMaker, EXIT_DEADLINE,
    EXIT_MEMORY, NEED_OVERLAY, OVERLAY_FAIL, OVERLAY_REPLY, REQ_BUILD, REQ_INSPECT, RES_ERR,
    RES_OK, SOURCE, WORKER_ARG,
};
use std::io::{self, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Longest sentence taken from a worker as it is (characters); it only ever sends its own fixed
/// sentences, anything longer is cut.
const MAX_SENTENCE_CHARS: usize = 300;
/// How long a worker that closed its output is given to exit before it is judged by its code.
const EXIT_GRACE: Duration = Duration::from_millis(500);
/// Windows: do not create a console window for the worker.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Starts and watches workers.
pub struct Supervisor {
    exe: PathBuf,
    jobs: &'static Jobs,
    deadline: Duration,
    memory: u64,
}

impl Supervisor {
    /// A supervisor that starts `exe` (the shell's own binary) as the worker, under the shell's
    /// limits and with the process-wide places for jobs.
    pub fn new(exe: PathBuf) -> Self {
        Self::with_limits(exe, &JOBS, WORK_DEADLINE, MAX_CHILD_RSS_BYTES)
    }

    /// The same with other limits and places (tests).
    pub fn with_limits(exe: PathBuf, jobs: &'static Jobs, deadline: Duration, memory: u64) -> Self {
        Self {
            exe,
            jobs,
            deadline,
            memory,
        }
    }

    /// The number of pages of `pdf` once the worker has checked it against every budget.
    pub fn inspect(&self, pdf: &Arc<Vec<u8>>) -> Result<usize, String> {
        let answer = self.run(
            &|out| write_frame(out, REQ_INSPECT, &[pdf.as_slice()]),
            &mut |_, _, _| Err(MSG_WORKER_DIED.to_string()),
        )?;
        let bytes: [u8; 4] = answer.try_into().map_err(|_| MSG_WORKER_DIED.to_string())?;
        Ok(u32::from_le_bytes(bytes) as usize)
    }

    /// The PDF of the plan in `head`, built from `sources`; `make_overlay` is asked for the stamp
    /// and watermark forms the worker needs.
    pub fn build(
        &self,
        sources: &[Arc<Vec<u8>>],
        head: &BuildHead,
        make_overlay: &mut OverlayMaker<'_>,
    ) -> Result<Vec<u8>, String> {
        let encoded = head.encode();
        self.run(
            &|out| {
                write_frame(out, REQ_BUILD, &[&encoded])?;
                sources
                    .iter()
                    .try_for_each(|source| write_frame(out, SOURCE, &[source.as_slice()]))
            },
            make_overlay,
        )
    }

    fn run(
        &self,
        request: &(dyn Fn(&mut dyn Write) -> io::Result<()> + Sync),
        make_overlay: &mut OverlayMaker<'_>,
    ) -> Result<Vec<u8>, String> {
        let _place = self.jobs.claim().ok_or_else(|| MSG_BUSY.to_string())?;
        let mut command = Command::new(&self.exe);
        command
            .arg(WORKER_ARG)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn().map_err(|err| {
            eprintln!("pdf edit: the worker did not start: {err}");
            MSG_WORKER_DIED.to_string()
        })?;
        let _confinement = confine(&child, usize::try_from(self.memory).unwrap_or(usize::MAX));
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(MSG_WORKER_DIED.to_string());
        };
        let started = Instant::now();
        let pid = child.id();
        std::thread::scope(|scope| {
            let (frames, frames_in) = mpsc::channel();
            let (replies, replies_out) = mpsc::channel::<(u8, Vec<u8>)>();
            scope.spawn(move || {
                let mut out = BufReader::new(stdout);
                loop {
                    let frame = read_frame(&mut out);
                    let last = !matches!(&frame, Ok((tag, _)) if *tag == NEED_OVERLAY);
                    if frames.send(frame).is_err() || last {
                        break;
                    }
                }
            });
            scope.spawn(move || {
                let mut input = BufWriter::new(stdin);
                if request(&mut input).and_then(|()| input.flush()).is_err() {
                    return;
                }
                while let Ok((tag, payload)) = replies_out.recv() {
                    if write_frame(&mut input, tag, &[&payload]).is_err() {
                        break;
                    }
                }
            });
            let mut asked = 0usize;
            let outcome = loop {
                match frames_in.recv_timeout(MEMORY_POLL) {
                    Ok(Ok((RES_OK, payload))) => break Ok(payload),
                    Ok(Ok((RES_ERR, payload))) => break Err(sentence(&payload)),
                    Ok(Ok((NEED_OVERLAY, payload))) => {
                        asked += 1;
                        let Some((width, height, stamp)) = decode_overlay_need(&payload) else {
                            break Err(MSG_WORKER_DIED.to_string());
                        };
                        if asked > MAX_OVERLAY_SIZES + 1 {
                            break Err(MSG_WORKER_DIED.to_string());
                        }
                        let reply = match make_overlay(width, height, stamp) {
                            Ok(pdf) => (OVERLAY_REPLY, pdf),
                            Err(message) => (OVERLAY_FAIL, message.into_bytes()),
                        };
                        if replies.send(reply).is_err() {
                            break Err(MSG_WORKER_DIED.to_string());
                        }
                    }
                    Ok(Ok(_)) => break Err(MSG_WORKER_DIED.to_string()),
                    Ok(Err(_)) | Err(RecvTimeoutError::Disconnected) => {
                        break Err(why_it_ended(&mut child));
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                }
                if started.elapsed() > self.deadline {
                    break Err(MSG_TIMEOUT.to_string());
                }
                if resident_bytes(pid).is_some_and(|rss| rss > self.memory) {
                    break Err(MSG_MEMORY.to_string());
                }
            };
            let _ = child.kill();
            let _ = child.wait();
            drop(replies);
            outcome
        })
    }
}

/// A sentence the worker sent, as text of bounded length.
fn sentence(payload: &[u8]) -> String {
    String::from_utf8_lossy(payload)
        .chars()
        .take(MAX_SENTENCE_CHARS)
        .collect()
}

/// Why a worker that closed its output has no answer: it stopped itself at the memory or time
/// limit (exit code), or it died (a crash, a signal, the kernel's memory limit).
fn why_it_ended(child: &mut std::process::Child) -> String {
    // It closed its output, so it is on its way out; give it a moment to be gone.
    let end = Instant::now() + EXIT_GRACE;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < end => std::thread::sleep(Duration::from_millis(5)),
            _ => break None,
        }
    };
    match status.and_then(|status| status.code()) {
        Some(EXIT_MEMORY) => MSG_MEMORY,
        Some(EXIT_DEADLINE) => MSG_TIMEOUT,
        _ => MSG_WORKER_DIED,
    }
    .to_string()
}
