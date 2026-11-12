//! Linux session end (PD-191). A logout or shutdown reaches the app as `SIGTERM` (or `SIGHUP` when
//! the session leader hangs up), and nothing in the shell handled either: the default action ended
//! the process over unsaved documents. The handler does only what a signal handler may: it writes
//! the signal number to a pipe. A thread reads the pipe and hands the request to the main thread
//! (`app_quit::request_session_end`): nothing unsaved ends the app at once, otherwise every window
//! asks its own save prompt through the quit queue.
//!
//! Implementation review (PD-191): `libc` is already a dependency of this crate (process limits),
//! tauri and tao install no handler for these signals, and a signal-handling crate would only wrap
//! the same self-pipe, so the pipe is written here.
//!
//! The module builds on every Unix so the signal path is tested on any development machine; only
//! Linux installs it.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::os::fd::RawFd;
use std::sync::atomic::{AtomicI32, Ordering};

use libc::c_int;

/// The signals that end a Linux session.
const SESSION_SIGNALS: [c_int; 2] = [libc::SIGTERM, libc::SIGHUP];

/// The pipe's write end for the handler, which has no context pointer; -1 while none exists.
static WAKE_FD: AtomicI32 = AtomicI32::new(-1);

/// The read end of the pipe the handler writes to. Dropping it restores the default actions.
struct Signals {
    read_fd: RawFd,
    write_fd: RawFd,
}

#[cfg(target_os = "linux")]
fn errno_slot() -> *mut c_int {
    // SAFETY: returns the calling thread's errno location; no arguments.
    unsafe { libc::__errno_location() }
}

#[cfg(not(target_os = "linux"))]
fn errno_slot() -> *mut c_int {
    // SAFETY: returns the calling thread's errno location; no arguments.
    unsafe { libc::__error() }
}

/// Async-signal-safe: reads an atomic and calls `write`, nothing else; `errno` of the interrupted
/// code is kept. A full pipe (the reader is behind) drops the byte, which is fine: one pending
/// byte already means "a session end is requested".
extern "C" fn on_signal(signal: c_int) {
    let fd = WAKE_FD.load(Ordering::Relaxed);
    if fd < 0 {
        return;
    }
    let byte = signal as u8;
    let slot = errno_slot();
    // SAFETY: `slot` is this thread's errno; `write` is async-signal-safe and reads one byte from
    // a stack variable that lives through the call.
    unsafe {
        let saved = *slot;
        libc::write(fd, (&raw const byte).cast(), 1);
        *slot = saved;
    }
}

fn last_error(what: &str) -> String {
    format!("linux_signal: {what}: {}", std::io::Error::last_os_error())
}

impl Signals {
    /// Creates the pipe and sets the handler for [`SESSION_SIGNALS`]. Only one may exist.
    fn install() -> Result<Self, String> {
        let mut fds: [RawFd; 2] = [-1; 2];
        // SAFETY: `fds` has room for the two descriptors `pipe` writes.
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return Err(last_error("pipe"));
        }
        let [read_fd, write_fd] = fds;
        let signals = Self { read_fd, write_fd };
        for fd in fds {
            // SAFETY: `fd` is a descriptor this function just created.
            if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } != 0 {
                return Err(last_error("FD_CLOEXEC"));
            }
        }
        // The handler must never block on a full pipe.
        // SAFETY: `write_fd` is a descriptor this function just created.
        if unsafe { libc::fcntl(write_fd, libc::F_SETFL, libc::O_NONBLOCK) } != 0 {
            return Err(last_error("O_NONBLOCK"));
        }
        if WAKE_FD
            .compare_exchange(-1, write_fd, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err("linux_signal: installed twice".to_string());
        }
        for signal in SESSION_SIGNALS {
            // SAFETY: an all-zero `sigaction` is a valid starting value; the fields set below are
            // the handler (an `extern "C" fn(c_int)`), an empty mask and the restart flag.
            let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
            action.sa_sigaction = on_signal as extern "C" fn(c_int) as usize;
            action.sa_flags = libc::SA_RESTART;
            // SAFETY: `action.sa_mask` is a valid `sigset_t` to initialise, and `action` outlives
            // the call.
            let set = unsafe {
                libc::sigemptyset(&raw mut action.sa_mask);
                libc::sigaction(signal, &raw const action, std::ptr::null_mut())
            };
            if set != 0 {
                return Err(last_error("sigaction"));
            }
        }
        Ok(signals)
    }

    /// Blocks until a session signal arrived; its number. `None` when the pipe is closed or broken.
    fn wait(&self) -> Option<c_int> {
        let mut byte = 0_u8;
        loop {
            // SAFETY: reads at most one byte into `byte`, which lives through the call.
            let read = unsafe { libc::read(self.read_fd, (&raw mut byte).cast(), 1) };
            match read {
                1 => return Some(c_int::from(byte)),
                -1 if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted => {
                }
                _ => return None,
            }
        }
    }
}

impl Drop for Signals {
    fn drop(&mut self) {
        // A value whose install failed on a second handler never owned the handlers.
        let owned = WAKE_FD
            .compare_exchange(self.write_fd, -1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok();
        if owned {
            for signal in SESSION_SIGNALS {
                // SAFETY: restores the default action of a signal number.
                unsafe { libc::signal(signal, libc::SIG_DFL) };
            }
        }
        // SAFETY: both descriptors belong to this value and are closed once.
        unsafe {
            libc::close(self.read_fd);
            libc::close(self.write_fd);
        }
    }
}

/// Starts handling `SIGTERM` and `SIGHUP`: each one starts the session end on the main thread.
pub fn install(app: &tauri::AppHandle) -> Result<(), String> {
    let signals = Signals::install()?;
    let app = app.clone();
    std::thread::Builder::new()
        .name("session-signal".to_string())
        .spawn(move || {
            while signals.wait().is_some() {
                crate::app_quit::defer_to_main(&app, crate::app_quit::request_session_end);
            }
        })
        .map(drop)
        .map_err(|err| format!("linux_signal: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_signal_reaches_the_reader_through_the_pipe_and_only_one_handler_exists() {
        let signals = Signals::install().expect("the pipe and the handlers");
        assert!(
            Signals::install().is_err(),
            "a second install would take over the pipe"
        );
        // `raise` runs the handler on this thread before it returns.
        for signal in SESSION_SIGNALS {
            // SAFETY: `signal` is a valid signal number with the handler set above.
            assert_eq!(unsafe { libc::raise(signal) }, 0);
            assert_eq!(signals.wait(), Some(signal));
        }
        drop(signals);
        assert_eq!(
            WAKE_FD.load(Ordering::Acquire),
            -1,
            "dropped: no handler target"
        );
    }

    /// The reader hands every signal to the shared session-end entry through the main thread, and
    /// only Linux installs it, before the event loop runs.
    #[test]
    fn the_signal_thread_starts_the_shared_session_end_and_main_installs_it() {
        let own = include_str!("linux_signal.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(own.contains(
            "crate::app_quit::defer_to_main(&app, crate::app_quit::request_session_end)"
        ));
        let main = include_str!("main.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let install = main
            .find("#[cfg(target_os = \"linux\")]\n    if let Err(err) = linux_signal::install(app.handle())")
            .expect("only Linux installs the handlers");
        assert!(install < main.find("app.run(handle_run_event)").unwrap());
    }
}
