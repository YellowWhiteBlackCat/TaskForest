//! Audited `SCM_RIGHTS` fd-passing boundary crate — the workspace's third
//! `unsafe` trust root (ADR-025), the safe-Rust seam for passing a file
//! descriptor between processes over a Unix domain socket.
//!
//! This is ONE of FOUR places in the product tree allowed to contain `unsafe`
//! (the others being `taskmanager-perf-ioctl`, ADR-022,
//! `taskmanager-afpacket`, ADR-024, and `taskmanager-windows-api`, ADR-031).
//! It is the OS ABI seam for `sendmsg`/
//! `recvmsg` with `SCM_RIGHTS` ancillary data — the only portable way to
//! transfer an open file descriptor from one process to another on Linux. The
//! CAP_NET_RAW launcher uses it to hand an `AF_PACKET` socket fd (which it
//! opened with privilege) to the unprivileged app, which then runs the capture
//! loop on the inherited fd without ever holding `CAP_NET_RAW` itself.
//!
//! Besides the fd-passing primitive the crate owns two more audited kernel
//! seams, exposed through the same typed-only surface:
//! * `SO_PEERCRED` peer credentials — the receiver of
//!   a handed-off fd admits only a uid-0 peer, so a local unprivileged process
//!   that guesses the handoff address can never inject a self-chosen fd;
//! * Linux pidfd (`pidfd_open` / `pidfd_send_signal`) for the
//!   process-control helper — race-free signal targets, typed fallback on
//!   kernels without pidfd (Linux < 5.1).
//!
//! Refined safe-Rust principle (same as ADR-022/024):
//! * every business crate stays `#![forbid(unsafe_code)]`;
//! * OS/ABI work lives HERE, in ONE minimal, audited boundary crate per kernel
//!   surface; and
//! * the boundary crate exposes ONLY safe APIs.
//!
//! Trust-root invariants enforced by the workspace architecture test on every
//! change (`audited_boundary_crate_carries_its_own_unsafe_contract`):
//! * the crate root carries `#![deny(unsafe_op_in_unsafe_fn)]` (NOT `forbid`);
//! * every `unsafe {` block has a `// SAFETY:` comment citing the invariant;
//! * no raw pointer or `RawFd`/`AsRawFd` crosses the PUBLIC API — the only
//!   `unsafe` is the audited `sendmsg`/`recvmsg` + `CMSG_*` walk (plus the
//!   `getsockopt`/`syscall` seams above) on owned/borrowed fds, and all
//!   pointer casts use the `.cast()` method (never the `as *const`/`as *mut`
//!   keyword form the seam test forbids). The received fd is wrapped in an
//!   `OwnedFd` (with `MSG_CMSG_CLOEXEC` so it does not leak across a later
//!   `fork`+`exec`) before it crosses back to the caller.
//!
//! Protocol hardening (fail-closed) owned by this crate:
//! * `EINTR` from `sendmsg`/`recvmsg` is retried, never reported as failure;
//! * a message whose `SCM_RIGHTS` data does not carry EXACTLY one fd — a
//!   multi-fd cmsg, several rights cmsgs, or `MSG_CTRUNC` truncation — is a
//!   protocol violation: every fd the kernel already installed for it is
//!   closed and a typed `InvalidData` error is returned.

// Linux-only kernel surface (SCM_RIGHTS over Unix sockets); the crate is empty
// on other targets so the workspace still compiles there. Consumers reach it
// only through Linux-gated dependency edges (platform-linux escalation,
// net-launcher).
#![cfg(target_os = "linux")]
#![deny(unsafe_op_in_unsafe_fn)]

mod pidfd;

pub use pidfd::{is_pidfd_unsupported, pidfd_open, pidfd_send_signal};

use std::io;
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};

/// Capacity of the product handoff control slab: two `cmsghdr`s always cover
/// `CMSG_SPACE(size_of::<c_int>())` (≤ n + sizeof(cmsghdr) + align-of(cmsghdr),
/// which fits when sizeof ≥ align + 4 — true on every supported target). The
/// slack also lets the kernel write a two-fd cmsg into the slab so a multi-fd
/// violation is fully VISIBLE to the walk instead of arriving truncated.
const HANDOFF_CONTROL_LEN: usize = 2 * std::mem::size_of::<libc::cmsghdr>();

/// Control buffer for one or more `SCM_RIGHTS` cmsgs. The kernel and the
/// `CMSG_*` macros dereference `struct cmsghdr*` into this buffer, so its
/// alignment must match `struct cmsghdr` — a plain `Vec<u8>` (alignment 1) is
/// undefined behavior here (cmsg(3) requires the caller's buffer to be
/// cmsghdr-aligned; Miri flags the misaligned deref). The union's `cmsghdr`
/// member pins the alignment; that proof is part of the TYPE, which is why
/// [`find_scm_rights`] accepts a `&CmsgBuffer<N>` and never a bare byte slice.
#[repr(C)]
union CmsgBuffer<const N: usize> {
    align: libc::cmsghdr,
    bytes: [u8; N],
}

impl<const N: usize> CmsgBuffer<N> {
    /// A zeroed, cmsghdr-aligned slab.
    fn zeroed() -> Self {
        Self { bytes: [0u8; N] }
    }

    /// The control payload pointer (any `CMSG_SPACE` length that fits `N`).
    fn control_ptr(&mut self) -> *mut libc::c_void {
        // SAFETY: union field access is the unsafe surface here; the pointer
        // is used only inside the caller's audited sendmsg/recvmsg block.
        unsafe { self.bytes.as_mut_ptr().cast() }
    }

    /// The whole aligned byte slab (as a slice of the capacity).
    fn bytes(&self) -> &[u8] {
        // SAFETY: reading the union as bytes is always sound (u8 has no
        // validity constraints beyond being initialized, which every
        // construction path guarantees).
        unsafe { &self.bytes[..] }
    }

    /// The whole aligned byte slab, mutable. Fill path for the fuzz seam
    /// ([`find_scm_rights_in_control`]), which writes arbitrary bytes in as
    /// if `recvmsg` had received them.
    #[cfg(feature = "test-support")]
    fn bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: reading-or-writing the union as bytes is always sound (u8
        // has no validity constraints beyond being initialized, which every
        // construction path guarantees).
        unsafe { &mut self.bytes[..] }
    }
}

/// Retry an `EINTR`-interruptible socket operation to completion. A signal
/// arriving mid-`sendmsg`/`recvmsg` is not a protocol failure; reporting it as
/// one (the pre-hardening behavior) produced spurious handoff failures.
fn retry_on_eintr<T>(mut operation: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    loop {
        match operation() {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            outcome => return outcome,
        }
    }
}

/// Convert a libc call result (`< 0` = error, errno set) into an `io::Result`.
/// Must be called immediately after the failing libc call so `errno` is still
/// the caller's.
fn cvt(value: libc::ssize_t) -> io::Result<libc::ssize_t> {
    if value >= 0 {
        Ok(value)
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Send a duplicate of `fd` over the connected Unix domain socket `channel`
/// using `SCM_RIGHTS`. The kernel duplicates the file description into the
/// peer's fd table on the matching [`recv_fd`]; `fd` stays open in this process
/// (ownership is not transferred). Returns `Err` if the socket is not connected
/// or the buffer is full. An `EINTR` interruption is retried, not failed.
///
/// `channel` must be a connected `AF_UNIX` `SOCK_STREAM` (or `SOCK_SEQPACKET`)
/// socket — the app/launcher establish that out-of-band. Both arguments are
/// safe `OwnedFd` borrows; no `RawFd` crosses the public API.
pub fn send_fd(channel: &impl AsFd, fd: &impl AsFd) -> io::Result<()> {
    // SCM_RIGHTS ancillary data is only delivered alongside ≥1 byte of regular
    // payload on Linux, so send a single zero byte as the carrier.
    let mut carrier: u8 = 0;
    let carrier_ptr: *mut u8 = &mut carrier;
    let mut iov = libc::iovec {
        iov_base: carrier_ptr.cast(),
        iov_len: 1,
    };
    let cmsg_space =
        // SAFETY: CMSG_SPACE is a pure size computation (libc marks it unsafe as a C macro); one c_int is sound.
        unsafe { libc::CMSG_SPACE(std::mem::size_of::<libc::c_int>() as u32) } as usize;
    let mut cmsg_buf = CmsgBuffer::<HANDOFF_CONTROL_LEN>::zeroed();
    // SAFETY: `libc::msghdr` is a plain-old-data C struct of pointer/length
    // fields; an all-zero msghdr is the documented starting point (no union, no
    // drop). We overwrite every field we use below before the sendmsg call.
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    // SAFETY: `CmsgBuffer` is cmsghdr-aligned and `cmsg_space` bytes fit in
    // its capacity; the kernel and CMSG_* macros may deref `struct cmsghdr*`
    // into it.
    msg.msg_control = cmsg_buf.control_ptr();
    msg.msg_controllen = cmsg_space;
    // SAFETY: `msg` is fully initialized; `cmsg_buf` (cmsg_space bytes, sized by
    // CMSG_SPACE for one c_int) outlives the block and is the control buffer.
    // CMSG_FIRSTHDR returns a writable pointer to the first cmsghdr slot inside
    // it; we populate the header and copy the fd (a c_int) into the CMSG_DATA
    // payload area. `fd.as_fd().as_raw_fd()` returns the integer fd by value,
    // never escaping. The block does not retain any raw pointer past its end.
    unsafe {
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<libc::c_int>() as u32) as usize;
        let data: *mut u8 = libc::CMSG_DATA(cmsg);
        let raw: libc::c_int = fd.as_fd().as_raw_fd();
        let raw_ptr: *const libc::c_int = &raw;
        std::ptr::copy_nonoverlapping(
            raw_ptr.cast::<u8>(),
            data,
            std::mem::size_of::<libc::c_int>(),
        );
    }
    // SAFETY: `channel` is a valid borrowed fd for the call; `msg` and its
    // iov/cmsg buffers are valid and outlive the call. sendmsg reads (does not
    // retain) the iov + control data. The raw fd from as_fd().as_raw_fd() is
    // read-only. EINTR (a signal before any byte was transferred) is retried
    // by the wrapper.
    let sent = retry_on_eintr(|| {
        // SAFETY: same contract as the block above; the call is the single
        // unsafe operation this closure performs.
        cvt(unsafe { libc::sendmsg(channel.as_fd().as_raw_fd(), &msg, 0) })
    })?;
    if sent == 0 {
        // A one-byte carrier that "sent" nothing transferred no ancillary data
        // either; report it instead of letting the caller believe the fd went.
        return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
    }
    Ok(())
}

/// Receive one file descriptor sent via `SCM_RIGHTS` over the connected Unix
/// domain socket `channel`, returning it as an owned fd. Uses
/// `MSG_CMSG_CLOEXEC` so the received fd is close-on-exec (it does not leak
/// across a subsequent `fork`+`exec` — a privilege-hygiene requirement in the
/// launcher context).
///
/// Fail-closed protocol checks (the message must carry EXACTLY one fd):
/// * a `SCM_RIGHTS` cmsg with more than one fd — or several rights cmsgs —
///   closes every fd the kernel installed for the message and returns
///   `InvalidData`;
/// * `MSG_CTRUNC` (the peer sent ancillary data beyond the slab) is the same
///   violation: the message is discarded with a typed error;
/// * an orderly peer close before any fd surfaces `UnexpectedEof`; no fd in
///   the message surfaces `InvalidData`. An `EINTR` interruption is retried,
///   not failed.
pub fn recv_fd(channel: &impl AsFd) -> io::Result<OwnedFd> {
    let mut carrier: [u8; 1] = [0];
    let mut iov = libc::iovec {
        iov_base: carrier.as_mut_ptr().cast(),
        iov_len: 1,
    };
    let cmsg_space =
        // SAFETY: CMSG_SPACE is a pure size computation (libc marks it unsafe as a C macro); one c_int is sound.
        unsafe { libc::CMSG_SPACE(std::mem::size_of::<libc::c_int>() as u32) } as usize;
    let mut cmsg_buf = CmsgBuffer::<HANDOFF_CONTROL_LEN>::zeroed();
    // SAFETY: see send_fd — zeroed msghdr is the valid POD initialization.
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    // SAFETY: `CmsgBuffer` is cmsghdr-aligned and `cmsg_space` bytes fit in
    // its capacity; recvmsg fills it and CMSG_FIRSTHDR/NXTHDR deref it.
    msg.msg_control = cmsg_buf.control_ptr();
    msg.msg_controllen = cmsg_space;
    // SAFETY: `channel` is a valid OwnedFd; `msg` + iov + cmsg_buf are valid
    // writable buffers outliving the call. MSG_CMSG_CLOEXEC sets close-on-exec
    // on any received fd. recvmsg writes at most iov_len into carrier and fills
    // the control buffer up to msg_controllen, updating msg_controllen to the
    // real length. The raw fd from as_raw_fd is read-only. EINTR is retried by
    // the wrapper (nothing was consumed when recvmsg reports it).
    let n = retry_on_eintr(|| {
        // SAFETY: same contract as the block above; the call is the single
        // unsafe operation this closure performs.
        cvt(unsafe {
            libc::recvmsg(
                channel.as_fd().as_raw_fd(),
                &mut msg,
                libc::MSG_CMSG_CLOEXEC,
            )
        })
    })?;
    if n == 0 {
        // The peer performed an orderly close (or sent an empty datagram) with no
        // fd — distinguish this from a malformed message so a caller can tell a
        // launcher that connected-then-failed-and-closed from a protocol error.
        return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
    }
    // Walk the control messages for the SCM_RIGHTS fds. Bounded: AF_PACKET
    // passes exactly one fd; anything else is a protocol violation handled
    // below. Always materialize the visible fd list before checking
    // MSG_CTRUNC so the truncation path also closes every descriptor that the
    // kernel exposed in the received control slab, including the no-visible-fd
    // case.
    // SAFETY: `msg` is the kernel-filled msghdr from recvmsg; the walk inside
    // stays within the cmsghdr-aligned `cmsg_buf` slab (see find_scm_rights).
    let mut fds = find_scm_rights(&cmsg_buf, msg.msg_controllen).unwrap_or_default();
    if (msg.msg_flags & libc::MSG_CTRUNC) != 0 {
        // The kernel installed the fds that fit before truncating; close them
        // so the violation leaves no residue.
        close_received(fds);
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "control data truncated: the peer sent more ancillary data than the one-fd protocol carries",
        ));
    }
    if fds.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no SCM_RIGHTS file descriptor in the received message",
        ));
    }
    if fds.len() != 1 {
        let count = fds.len();
        close_received(fds);
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "SCM_RIGHTS message carried {count} file descriptors; the handoff protocol carries exactly one"
            ),
        ));
    }
    let raw = fds.remove(0);
    // SAFETY: `raw` is a fresh fd the kernel duplicated into our table on this
    // recvmsg; OwnedFd::from_raw_fd takes exclusive ownership (closed on drop)
    // — wrapped exactly once.
    Ok(unsafe { OwnedFd::from_raw_fd(raw) })
}

/// Close every fd the kernel installed into this process's table for a
/// message that failed the exactly-one-fd protocol check. Fail-closed: the
/// moment `recvmsg` returned, those fds were live here, so leaving them
/// unwrapped would leak one descriptor per malformed message.
fn close_received(fds: Vec<libc::c_int>) {
    for raw in fds {
        // SAFETY: `raw` is a kernel-installed fd from this recvmsg, listed by
        // the kernel in the cmsg payload; OwnedFd::from_raw_fd takes exclusive
        // ownership so Drop closes it exactly once.
        drop(unsafe { OwnedFd::from_raw_fd(raw) });
    }
}

/// Find every `SCM_RIGHTS` fd inside a control buffer filled by `recvmsg`
/// (or hand-built by tests). Returns `None` when no well-formed rights cmsg is
/// present; `Some(fds)` accumulates one entry per received fd across all
/// rights cmsgs, so the caller can enforce its exactly-one contract.
///
/// The buffer argument is the aligned [`CmsgBuffer`] TYPE itself — the
/// cmsghdr-alignment invariant the `CMSG_*` macros rely on is encoded in the
/// type, never in a caller comment over a bare byte slice. `controllen` is the
/// length the kernel reported; it is clamped to the slab capacity (a lying
/// length must not extend the walk past the buffer).
///
/// The walk is the protocol-decision half of [`recv_fd`], split out so the
/// accept/reject predicate is testable without a socket (and under Miri).
fn find_scm_rights<const N: usize>(
    buf: &CmsgBuffer<N>,
    controllen: usize,
) -> Option<Vec<libc::c_int>> {
    let len = controllen.min(N);
    let data: &[u8] = &buf.bytes()[..len];
    let mut fds = Vec::new();
    // SAFETY: audited boundary-crate code; justification below is kept adjacent to the block it guards.
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    // SAFETY: `buf` is the aligned CmsgBuffer (union-pinned); `data` is a
    // subslice of its slab, so the same alignment proof covers it. The bytes
    // are kernel- or test-filled cmsghdr data.
    msg.msg_control = data.as_ptr().cast_mut().cast();
    msg.msg_controllen = data.len();
    // SAFETY: `msg` is zeroed-then-filled exactly like the send/recv paths;
    // CMSG_FIRSTHDR/NXTHDR walk within `data`'s bounds (cmsg_controllen).
    let mut cmsg = unsafe { libc::CMSG_FIRSTHDR(&msg) };
    while !cmsg.is_null() {
        // SAFETY: cmsg points within `data`; we read level/type/len by value
        // and copy whole c_ints out of CMSG_DATA.
        unsafe {
            // Malformed control data must end the walk, never crash it: a
            // bogus cmsg_len would make CMSG_ALIGN(cmsg_len) overflow inside
            // CMSG_NXTHDR (kernel-filled buffers can't be malformed, but this
            // walk is also the fuzz/unit-test surface for garbage buffers).
            // The length must fit the window REMAINING from this cmsg's own
            // offset: CMSG_NXTHDR (glibc's __cmsg_nxthdr) only guarantees the
            // next HEADER fits msg_controllen, so a garbage chain can park a
            // header inside the window while its declared payload runs past
            // it — checking against data.len() alone let that read cross the
            // window end (fuzz-found 2026-08-26; the byte distance below is
            // computed, never dereferenced).
            let cmsg_len = (*cmsg).cmsg_len;
            let offset = cmsg
                .cast::<u8>()
                .byte_offset_from(data.as_ptr())
                .unsigned_abs();
            let remaining = data.len().saturating_sub(offset);
            if cmsg_len < std::mem::size_of::<libc::cmsghdr>() || cmsg_len > remaining {
                break;
            }
            if (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_RIGHTS {
                // A header-only or truncated SCM_RIGHTS cmsg carries no fd
                // (CMSG_LEN(sizeof(c_int)) is the minimum valid length);
                // reading CMSG_DATA anyway would fabricate an fd from
                // whatever bytes follow the header.
                let one_fd_len = libc::CMSG_LEN(std::mem::size_of::<libc::c_int>() as u32) as usize;
                if cmsg_len < one_fd_len {
                    break;
                }
                let header_len = libc::CMSG_LEN(0) as usize;
                let payload_len = cmsg_len - header_len;
                if !payload_len.is_multiple_of(std::mem::size_of::<libc::c_int>()) {
                    break;
                }
                let count = payload_len / std::mem::size_of::<libc::c_int>();
                let payload: *mut u8 = libc::CMSG_DATA(cmsg);
                for index in 0..count {
                    let mut raw: libc::c_int = 0;
                    let raw_ptr: *mut libc::c_int = &mut raw;
                    // SAFETY: `index` is bounded by the fd count the cmsg_len
                    // declares, and cmsg_len was checked against the window
                    // remaining from this cmsg's offset, so the reads stay
                    // inside `data`.
                    std::ptr::copy_nonoverlapping(
                        payload.add(index * std::mem::size_of::<libc::c_int>()),
                        raw_ptr.cast::<u8>(),
                        std::mem::size_of::<libc::c_int>(),
                    );
                    fds.push(raw);
                }
            }
            cmsg = libc::CMSG_NXTHDR(&msg, cmsg);
        }
    }
    if fds.is_empty() { None } else { Some(fds) }
}

/// Capacity of the fuzz seam's control slab: six `cmsghdr`s (the same slab
/// size the unit tests lay chained cmsgs into), so byte-driven fuzzing can
/// reach the walk's multi-fd, multi-cmsg and walk-past-other-ancillary
/// branches that the one-fd product handoff slab is too small to carry.
#[cfg(feature = "test-support")]
const FUZZ_CONTROL_LEN: usize = 6 * std::mem::size_of::<libc::cmsghdr>();

/// Fuzz-reachable seam over the audited cmsg walk (`find_scm_rights`),
/// exposed behind the crate's `test-support` feature the same way
/// `taskmanager-platform-linux` re-exports its pure parsers: the private
/// aligned-slab type never becomes public — a raw byte slice in, the fd
/// list out.
///
/// `control` fills the leading bytes of a `FUZZ_CONTROL_LEN`-sized,
/// cmsghdr-aligned slab (the rest stays zeroed); `controllen` is the length
/// the kernel would have reported, clamped to the slab by the walk itself.
/// The contract under fuzzing: ANY bytes and ANY reported length either
/// return exactly the fds well-formed `SCM_RIGHTS` cmsgs carry or `None` —
/// never a panic, never an out-of-bounds read, never a fabricated fd.
#[cfg(feature = "test-support")]
pub fn find_scm_rights_in_control(control: &[u8], controllen: usize) -> Option<Vec<libc::c_int>> {
    let mut slab = CmsgBuffer::<FUZZ_CONTROL_LEN>::zeroed();
    let fill = control.len().min(FUZZ_CONTROL_LEN);
    slab.bytes_mut()[..fill].copy_from_slice(&control[..fill]);
    find_scm_rights(&slab, controllen)
}

/// Kernel-reported credentials of the process on the other end of a connected
/// `AF_UNIX` stream socket (`SO_PEERCRED`). Filled by the kernel at
/// `connect`/`accept` time — the peer cannot forge or alter them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ucred {
    /// The peer's process id, or 0 when the peer lives in a PID namespace
    /// this process cannot translate.
    pub pid: i32,
    /// The peer's effective user id at connection time.
    pub uid: u32,
    /// The peer's effective group id at connection time.
    pub gid: u32,
}

/// Read the peer credentials of a connected `AF_UNIX` stream socket via
/// `SO_PEERCRED` (`getsockopt`). The fd-handoff receiver uses this to admit
/// only a root peer before trusting a received descriptor: a local
/// unprivileged process that connects to the handoff socket first is
/// disconnected instead of being `recv_fd`'d from.
///
/// `stream` must be a connected Unix stream socket (an accepted connection,
/// or either end of a connected `UnixStream`); other descriptors surface the
/// OS error verbatim.
pub fn peer_credentials(stream: &impl AsFd) -> io::Result<Ucred> {
    let mut ucred = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let ucred_ptr: *mut libc::ucred = &mut ucred;
    // SAFETY: `stream` is a valid borrowed socket fd for the call. `ucred` is
    // a plain struct of three integers outliving the call; getsockopt writes
    // at most `size_of::<ucred>()` bytes into it, and the written length is
    // verified below. The raw fd from as_raw_fd is read-only.
    let rc = unsafe {
        libc::getsockopt(
            stream.as_fd().as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            ucred_ptr.cast(),
            &mut len,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    if len as usize != std::mem::size_of::<libc::ucred>() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "SO_PEERCRED returned an unexpected credential length",
        ));
    }
    Ok(Ucred {
        pid: ucred.pid,
        uid: ucred.uid,
        gid: ucred.gid,
    })
}

#[cfg(test)]
#[path = "../tests/headless/fd_bridge.rs"]
mod tests;
