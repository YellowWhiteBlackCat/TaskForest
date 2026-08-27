//! Shared bounded child-process runner for the fixed `pkexec` crossings.
//!
//! Every production crossing in this module tree spawns exactly one helper
//! child and must honor three bounds at once:
//!
//! 1. **A deadline.** `std::process::Command::output()` blocks until the child
//!    exits, and the polkit authentication dialog can be left open forever.
//!    An unbounded `.output()` therefore parks the calling thread (including
//!    the "end process" path) indefinitely. The runner polls `try_wait` until
//!    the deadline, then `SIGKILL`s the child and reaps it.
//! 2. **Bounded streams.** `.output()` reads both pipes to EOF into memory; a
//!    misbehaving helper emitting gigabytes would balloon the process. Each
//!    captured stream is drained on its own thread with a hard byte cap; the
//!    capped stream is dropped (closing our end), so the child's next write
//!    fails instead of being buffered. A truncated stdout no longer parses as
//!    the contract and flows through the existing `NotContract` /
//!    `classify_pkexec_no_contract` semantics — no new public vocabulary.
//! 3. **Reaping.** Every exit path — success, deadline, or wait failure —
//!    either observes the child's exit status or kills and waits it, so no
//!    zombie survives the call.
//!
//! Safe std-only implementation: the drain threads cannot be interrupted mid
//! `read`, so a pathological grandchild holding the pipe open past the grace
//! window would leave its (tiny, cap-bounded) drain thread parked until the
//! pipe closes. That is strictly better than the unbounded read it replaces.
//!
//! `taskmanager-setup-helper` is a standalone root binary that must not depend
//! on this crate; it carries an equivalent inline implementation.

#![forbid(unsafe_code)]

use std::io::{self, Read};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Hard cap for one captured stream. The helper contract is one small flat
/// JSON object (well under 1 KiB); 64 KiB leaves orders of magnitude of slack
/// for diagnostics while making a runaway stream impossible.
pub(super) const STREAM_CAP_BYTES: usize = 64 * 1024;

/// Deadline for the interactive `pkexec` crossings. Polkit's dialog may wait
/// for a slow human to find and type credentials, so this is deliberately
/// wide; past it the dialog is treated as abandoned and the child is killed.
pub(super) const INTERACTIVE_PKEXEC_DEADLINE: Duration = Duration::from_secs(120);

/// How long to wait for the drain threads after the child is gone. They
/// normally observe EOF immediately when the kernel closes the pipes.
const DRAIN_GRACE: Duration = Duration::from_secs(2);

/// `try_wait` poll interval while waiting for the child to exit.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// One bounded child result: exit status plus whatever the bounded drains
/// captured (each stream at most [`STREAM_CAP_BYTES`] long).
#[derive(Debug)]
pub(super) struct BoundedChildOutput {
    /// The process exit code, or `None` if the platform did not report one
    /// (e.g. terminated by signal).
    pub status_code: Option<i32>,
    /// Captured stdout, truncated at the stream cap.
    pub stdout: Vec<u8>,
    /// Captured stderr, truncated at the stream cap.
    pub stderr: Vec<u8>,
}

/// Typed failure of one bounded child run.
#[derive(Debug)]
pub(super) enum BoundedChildError {
    /// The child could not be spawned (e.g. `pkexec` not on `PATH`).
    Spawn(io::Error),
    /// The deadline elapsed: the child was `SIGKILL`ed and reaped. Carries
    /// the kill status and whatever partial output was drained, so callers
    /// can log honestly instead of discarding evidence.
    TimedOut {
        /// Exit status after the kill (`None` when killed by signal).
        status_code: Option<i32>,
        /// Partial stdout drained before the deadline, still stream-capped.
        stdout: Vec<u8>,
        /// Partial stderr drained before the deadline, still stream-capped.
        stderr: Vec<u8>,
    },
    /// `wait`/`try_wait` failed after the child had already spawned. The
    /// runner still kills and reaps the child best-effort before returning.
    Wait(io::Error),
}

impl BoundedChildError {
    /// Flatten into the `io::Error` the process seams return, mapping the
    /// typed deadline outcome onto `io::ErrorKind::TimedOut` so generic
    /// invoke layers can branch on it without new public vocabulary. The
    /// deadline message keeps a bounded prefix of whatever the child managed
    /// to print before it was killed, so a hung crossing still leaves a
    /// diagnostic trail.
    pub(super) fn into_io_error(self, what: &str) -> io::Error {
        match self {
            Self::Spawn(error) | Self::Wait(error) => error,
            Self::TimedOut {
                status_code,
                stdout,
                stderr,
            } => {
                let mut detail =
                    format!("{what} did not finish within the bounded deadline and was killed");
                if let Some(code) = status_code {
                    detail.push_str(&format!(" (exit {code} after the kill)"));
                }
                append_partial(&mut detail, "stdout", &stdout);
                append_partial(&mut detail, "stderr", &stderr);
                io::Error::new(io::ErrorKind::TimedOut, detail)
            }
        }
    }
}

/// Append `label: <first bytes>` to `detail` when the partial stream is
/// non-empty; the prefix is char-boundary-safe and tiny.
fn append_partial(detail: &mut String, label: &str, partial: &[u8]) {
    if partial.is_empty() {
        return;
    }
    const PREFIX_BYTES: usize = 120;
    let bounded = String::from_utf8_lossy(&partial[..partial.len().min(PREFIX_BYTES)]);
    let cut = super::truncate_at_char_boundary(bounded.trim_end(), PREFIX_BYTES);
    detail.push_str(&format!("; partial {label}: {cut:?}"));
}

/// Spawn `command` and run it to completion under the three bounds above.
///
/// Only the streams the caller piped are drained (`Stdio::piped()`); nulled
/// streams spawn no drain thread and come back empty.
pub(super) fn run_bounded(
    command: &mut Command,
    timeout: Duration,
) -> Result<BoundedChildOutput, BoundedChildError> {
    let mut child = command.spawn().map_err(BoundedChildError::Spawn)?;
    let stdout_drain = child.stdout.take().map(spawn_drain);
    let stderr_drain = child.stderr.take().map(spawn_drain);
    match wait_with_deadline(&mut child, timeout) {
        Ok(status_code) => Ok(BoundedChildOutput {
            status_code,
            stdout: finish_drain(stdout_drain),
            stderr: finish_drain(stderr_drain),
        }),
        Err(error) if error.kind() == io::ErrorKind::TimedOut => {
            // Abandoned dialog / stuck helper: kill, reap, and report the
            // typed timeout with whatever partial output was drained.
            let _ = child.kill();
            let status_code = child.wait().ok().and_then(|status| status.code());
            Err(BoundedChildError::TimedOut {
                status_code,
                stdout: finish_drain(stdout_drain),
                stderr: finish_drain(stderr_drain),
            })
        }
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(BoundedChildError::Wait(error))
        }
    }
}

/// Poll `try_wait` until the child exits or `timeout` elapses.
fn wait_with_deadline(child: &mut Child, timeout: Duration) -> io::Result<Option<i32>> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status.code());
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "child did not exit within the bounded deadline",
            ));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// One background drain of a single captured stream.
struct Drain {
    /// Bytes read so far; the thread keeps appending until EOF or the cap.
    buffer: Arc<Mutex<Vec<u8>>>,
    /// Set (release) once the drain thread has stopped reading.
    done: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

/// Read `source` into `buffer` until EOF, a read error, or the stream cap.
/// Reaching the cap drops the pipe, so a writer that keeps writing sees
/// EPIPE/SIGPIPE instead of being buffered into this process's memory.
fn spawn_drain<R: Read + Send + 'static>(source: R) -> Drain {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let done = Arc::new(AtomicBool::new(false));
    let thread_buffer = Arc::clone(&buffer);
    let thread_done = Arc::clone(&done);
    let handle = thread::spawn(move || {
        let mut source = source;
        let mut chunk = [0u8; 4096];
        loop {
            match source.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => {
                    let mut guard = match thread_buffer.lock() {
                        Ok(guard) => guard,
                        // Another thread panicked while holding the lock; the
                        // buffer is still readable, keep appending.
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    let room = STREAM_CAP_BYTES.saturating_sub(guard.len());
                    guard.extend_from_slice(&chunk[..read.min(room)]);
                    if guard.len() >= STREAM_CAP_BYTES {
                        break;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        thread_done.store(true, Ordering::Release);
    });
    Drain {
        buffer,
        done,
        handle: Some(handle),
    }
}

/// Collect a drain's bytes, waiting at most [`DRAIN_GRACE`] for its thread.
fn finish_drain(drain: Option<Drain>) -> Vec<u8> {
    let Some(drain) = drain else {
        return Vec::new();
    };
    let deadline = Instant::now() + DRAIN_GRACE;
    while !drain.done.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
    }
    if drain.done.load(Ordering::Acquire)
        && let Some(handle) = drain.handle
    {
        // The thread finished: join reclaims it and cannot block.
        let _ = handle.join();
    }
    // If the thread is still parked on a pipe held open by someone else, the
    // JoinHandle is dropped here (detaching a cap-bounded reader) and the
    // snapshot taken so far is returned.
    match drain.buffer.lock() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

#[cfg(test)]
#[path = "../../tests/headless/escalation_polkit_bounded_runner.rs"]
mod tests;
