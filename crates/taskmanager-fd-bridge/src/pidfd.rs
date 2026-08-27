//! Linux `pidfd` typed API — an audited kernel seam of this boundary crate,
//! consumed (via the safe surface only) by the privileged process-control
//! helper path (see `docs/PERMISSION_MODEL.md` Boundary 1: the unsafe stays in
//! the audited crate; every caller sees typed owned values).
//!
//! `pidfd_open(2)` (Linux ≥ 5.1) returns a stable handle to a process that
//! does not get recycled when the pid is reaped, closing the classic
//! kill-the-wrong-pid race in signal-based process control;
//! `pidfd_send_signal(2)` delivers a signal through that handle. Both are
//! plain `libc::syscall` wrappers here — no new trust surface beyond the two
//! integer-argument syscalls.
//!
//! Kernel support is a runtime property: on Linux < 5.1 both calls fail with
//! `ENOSYS`; [`is_pidfd_unsupported`] matches that so callers can fall back to
//! their legacy (typed, honest) path instead of guessing.

use std::io;
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};

/// Open a pidfd for `pid`, returning it as a close-on-exec owned descriptor.
///
/// Fails with the OS error verbatim: `ESRCH` when no such pid exists, `EPERM`
/// when the caller may not signal that process, `ENOSYS` on kernels without
/// pidfd (match it with [`is_pidfd_unsupported`] to choose a fallback).
pub fn pidfd_open(pid: u32) -> io::Result<OwnedFd> {
    // SAFETY: pure syscall wrapper taking by-value arguments; no memory is
    // read or written. On failure it only sets errno; on success the returned
    // c_long is a fresh non-negative fd this process owns.
    let rc = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0) };
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }
    let raw = rc as libc::c_int;
    // SAFETY: `raw` (>= 0) is the fresh fd pidfd_open created and nobody else
    // owns; OwnedFd::from_raw_fd takes exclusive ownership so Drop closes it
    // on every path below (including the fcntl failure path).
    let pidfd = unsafe { OwnedFd::from_raw_fd(raw) };
    // SAFETY: `pidfd` is a valid owned descriptor for the call. F_SETFD with
    // FD_CLOEXEC only flips the descriptor flag — no memory is read or
    // written; it keeps the pidfd from surviving a later fork+exec (the same
    // privilege-hygiene rule MSG_CMSG_CLOEXEC enforces for received fds).
    if unsafe { libc::fcntl(pidfd.as_fd().as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(pidfd)
}

/// Send `signal` (a signal number: `SIGTERM`/`SIGKILL`, or 0 to probe) to the
/// process identified by `pidfd` (from [`pidfd_open`]). The handle stays
/// valid even after the target exits, so the signal either reaches the
/// intended process or fails typed — it can never hit a recycled pid.
///
/// Fails with the OS error verbatim (`EPERM`, `ESRCH` for an already-reaped
/// process, `ENOSYS` on kernels without pidfd).
pub fn pidfd_send_signal(pidfd: &impl AsFd, signal: i32) -> io::Result<()> {
    // SAFETY: pure syscall wrapper taking by-value arguments over the borrowed
    // pidfd; the null siginfo asks the kernel to synthesize the default
    // info for the signal, and flags is 0. No memory is read or written and
    // the fd is not retained.
    let rc = unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            pidfd.as_fd().as_raw_fd(),
            signal,
            std::ptr::null::<libc::c_void>(),
            0,
        )
    };
    if rc < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// True when an error from [`pidfd_open`]/[`pidfd_send_signal`] means "this
/// kernel has no pidfd" (`ENOSYS`, Linux < 5.1) — the caller's cue to use its
/// legacy typed path rather than report a generic failure.
#[must_use]
pub fn is_pidfd_unsupported(error: &io::Error) -> bool {
    error.raw_os_error() == Some(libc::ENOSYS)
}

#[cfg(test)]
#[path = "../tests/headless/pidfd.rs"]
mod tests;
