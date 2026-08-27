//! Audited `AF_PACKET` boundary crate — the workspace's second `unsafe` trust
//! root (ADR-024), the safe-Rust seam for per-process network byte accounting.
//!
//! This is ONE of TWO places in the product tree allowed to contain `unsafe`
//! (the other being `taskmanager-perf-ioctl`, ADR-022). It is the OS ABI seam
//! for an `AF_PACKET` `SOCK_RAW` socket — the only kernel interface that
//! observes per-packet bytes for attribution to a process. eBPF (the prior
//! approach) was removed by ADR-021 as too large a trust root; a single
//! `socket`/`bind`/`recvfrom` boundary qualifies, mirroring the
//! `perf_event_open` carve-out.
//!
//! Refined safe-Rust principle (same as ADR-022):
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
//!   `unsafe` is forming the kernel fd into an `OwnedFd` we own, the audited
//!   `bind`/`recvfrom` on that owned fd, and all pointer casts use the `.cast()`
//!   method (never the `as *const`/`as *mut` keyword form the seam test forbids).
//!
//! The CAP_NET_RAW capability this socket requires is NOT granted to the
//! unprivileged app binary; a dedicated launcher obtains the fd and passes it
//! via `SCM_RIGHTS` (the per-feature-escalation seam, ADR-023). `PacketSource`
//! can also be constructed directly by that launcher or by tests running with
//! the capability. Packet parsing + per-pid attribution are PURE SAFE RUST
//! (`five_tuple`, and the consuming layer) — this crate's `unsafe` ends at
//! `recvfrom`. (Plain code span, not an intra-doc link: the crate root is
//! `cfg(target_os = "linux")`-gated, so the doc build on other targets must
//! still resolve.)

// Linux-only kernel surface (AF_PACKET); the crate is empty on other targets so
// the workspace still compiles there. Consumers reach it only through
// Linux-gated dependency edges (taskmanager-platform-linux, net-launcher).
#![cfg(target_os = "linux")]
#![deny(unsafe_op_in_unsafe_fn)]

mod parse;

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

pub use parse::{FiveTuple, five_tuple};

/// SAFETY boundary: open an `AF_PACKET` `SOCK_RAW` socket and return the owned
/// kernel fd. Private — the safe public API ([`PacketSource::open`]) is the only
/// caller. Combining the syscall and the `OwnedFd` ownership transfer in one
/// audited block keeps the socket `unsafe` to exactly this site.
fn packet_socket() -> io::Result<OwnedFd> {
    // SAFETY: `AF_PACKET`, `SOCK_RAW`, and `htons(ETH_P_ALL)` (the protocol
    // ethertype in network byte order) are by-value integer arguments matching
    // socket(2)'s signature. libc::socket returns a non-negative fd on success
    // or -1 with errno set otherwise; the error case becomes
    // io::Error::last_os_error() and never reaches from_raw_fd. On success the
    // integer is a freshly kernel-allocated file descriptor we exclusively own,
    // so OwnedFd::from_raw_fd is the one unavoidable unsafe — OwnedFd closes the
    // descriptor on drop and no raw fd ever crosses the safe public API.
    let fd = unsafe {
        libc::socket(
            libc::AF_PACKET,
            libc::SOCK_RAW,
            libc::htons(libc::ETH_P_ALL as u16) as libc::c_int,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: see above — `fd` is a freshly allocated, exclusively-owned kernel
    // descriptor returned by socket(2) on the success path.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// SAFETY boundary: bind the socket to one interface index so it only captures
/// that interface's traffic (not every NIC — the cost control the design
/// requires). Private; only [`PacketSource::open`] calls it.
fn bind_interface(fd: &OwnedFd, iface_index: u32) -> io::Result<()> {
    let addr = libc::sockaddr_ll {
        sll_family: libc::AF_PACKET as u16,
        sll_protocol: libc::htons(libc::ETH_P_ALL as u16),
        sll_ifindex: iface_index as libc::c_int,
        sll_hatype: 0,
        sll_pkttype: 0,
        sll_halen: 0,
        sll_addr: [0; 8],
    };
    let addr_ptr: *const libc::sockaddr_ll = &addr;
    // SAFETY: `fd` is a valid OwnedFd borrowed for the duration of the call;
    // `addr_ptr` is a valid pointer to a fully-initialized #[repr(C)]
    // sockaddr_ll (libc-defined, matches the kernel ABI) borrowed for the call
    // and outliving it; the length is sizeof(sockaddr_ll). bind(2) reads the
    // struct read-only; the raw fd from as_raw_fd is read-only and never
    // escapes this function. The `addr_ptr.cast()` widens the typed pointer to
    // the `*const sockaddr` bind expects — this is the audited pointer cast,
    // expressed via the `.cast()` method (not the `as *const` keyword form the
    // architecture test forbids).
    let rc = unsafe {
        libc::bind(
            fd.as_raw_fd(),
            addr_ptr.cast(),
            core::mem::size_of::<libc::sockaddr_ll>() as libc::socklen_t,
        )
    };
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// SAFETY boundary: receive one packet frame into `buf`, returning the byte
/// count the kernel filled AND whether the packet was *sent by this host*
/// (`outgoing = true`, i.e. tx) vs received (`outgoing = false`, i.e. rx) — read
/// from the `sll_pkttype` field of the ingress `sockaddr_ll` the kernel fills.
/// This direction bit is the only L2 metadata attribution needs (the 5-tuple
/// comes from the frame bytes). Private; only [`PacketSource::recv`] calls it.
fn recv_packet(fd: &OwnedFd, buf: &mut [u8]) -> io::Result<(usize, bool)> {
    let buf_ptr: *mut libc::c_void = buf.as_mut_ptr().cast();
    let mut addr = libc::sockaddr_ll {
        sll_family: 0,
        sll_protocol: 0,
        sll_ifindex: 0,
        sll_hatype: 0,
        sll_pkttype: 0,
        sll_halen: 0,
        sll_addr: [0; 8],
    };
    let addr_ptr: *mut libc::sockaddr_ll = &mut addr;
    let mut addr_len = core::mem::size_of::<libc::sockaddr_ll>() as libc::socklen_t;
    // SAFETY: `fd` is a valid OwnedFd borrowed for the call; `buf_ptr` is a
    // valid writable pointer derived from `buf`'s exclusive &mut [u8] borrow,
    // with `buf.len()` bytes the kernel may write (recvfrom writes at most `len`
    // and returns the count actually written). `addr_ptr` is a valid writable
    // pointer to a fully-initialized #[repr(C)] sockaddr_ll borrowed for the
    // call, with `addr_len` initialized to its size; recvfrom writes the ingress
    // L2 metadata there read-only-after-the-call. The flags argument is 0. The
    // raw fd from as_raw_fd is read-only and never escapes. `buf_ptr` and
    // `addr_ptr` are obtained via the `.cast()` method (not the `as *mut`
    // keyword form the architecture test forbids). With SO_RCVTIMEO set, a
    // timeout surfaces as EAGAIN/EWOULDBLOCK (becomes io::Error::WouldBlock).
    let n = unsafe {
        libc::recvfrom(
            fd.as_raw_fd(),
            buf_ptr,
            buf.len(),
            0,
            addr_ptr.cast(),
            &mut addr_len,
        )
    };
    if n < 0 {
        return Err(io::Error::last_os_error());
    }
    // PACKET_OUTGOING (4) marks frames this host transmitted; everything else
    // (PACKET_HOST/OTHER/BROADCAST/MULTICAST) was received.
    Ok((n as usize, addr.sll_pkttype == libc::PACKET_OUTGOING))
}

/// SAFETY boundary: set `SO_RCVTIMEO` so [`recv_packet`] returns periodically
/// with `EAGAIN` instead of blocking forever — the capture worker checks its
/// shutdown flag on each return. Private; only [`PacketSource::open`] calls it.
fn set_recv_timeout(fd: &OwnedFd, ms: i64) -> io::Result<()> {
    let timeout = libc::timeval {
        tv_sec: ms / 1000,
        tv_usec: ((ms % 1000) * 1000) as libc::suseconds_t,
    };
    let timeout_ptr: *const libc::timeval = &timeout;
    // SAFETY: `fd` is a valid OwnedFd borrowed for the call; `timeout_ptr` is a
    // valid pointer to a fully-initialized #[repr(C)] timeval borrowed for the
    // call (setsockopt copies it). SOL_SOCKET/SO_RCVTIMEO are by-value integer
    // constants. The raw fd from as_raw_fd is read-only and never escapes.
    // `timeout_ptr.cast()` widens the typed pointer to the `*const c_void`
    // setsockopt expects via the `.cast()` method (not the `as *const` keyword
    // form the architecture test forbids).
    let rc = unsafe {
        libc::setsockopt(
            fd.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            timeout_ptr.cast(),
            core::mem::size_of::<libc::timeval>() as libc::socklen_t,
        )
    };
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// A safe handle to one bound `AF_PACKET` raw socket.
///
/// Owns the kernel file descriptor via [`OwnedFd`] (closes on drop). The public
/// API never exposes the underlying fd, a `RawFd`, or any raw pointer — callers
/// can only [`PacketSource::open`] (open+bind to an interface index) and
/// [`PacketSource::recv`] (read one frame for safe parsing).
///
/// Constructing this requires `CAP_NET_RAW`; on the unprivileged app binary
/// `open` returns `Err(PermissionDenied)` (the per-process-network feature then
/// reports `RequiresEscalation(PerProcessNet)` and the user is offered the
/// OS-native prompt, per ADR-023).
#[derive(Debug)]
pub struct PacketSource {
    fd: OwnedFd,
}

impl PacketSource {
    /// Open an `AF_PACKET` raw socket bound to `iface_index` (the `ifindex`
    /// from `/sys/class/net/<iface>/ifindex`). Binds to ONE interface so the
    /// capture cost scales with one NIC's traffic, not the whole host. A 200 ms
    /// recv timeout is set so [`PacketSource::recv`] returns periodically (as
    /// [`io::ErrorKind::WouldBlock`]) — the capture worker checks its shutdown
    /// flag on each return rather than blocking forever.
    ///
    /// Returns `Err` without `CAP_NET_RAW` — the caller escalates via the
    /// per-feature gate rather than blanket-setcap'ing the app.
    pub fn open(iface_index: u32) -> io::Result<Self> {
        Ok(Self {
            fd: open_bound(iface_index)?,
        })
    }

    /// Open the `AF_PACKET` socket bound to `iface_index` and yield the owned fd
    /// — the privileged launcher's path: it obtains the fd (with `CAP_NET_RAW`,
    /// granted by the OS-native prompt) and passes it to the unprivileged app
    /// via `SCM_RIGHTS`; the app then wraps it with [`PacketSource::from_owned_fd`].
    /// Returns `Err` without `CAP_NET_RAW` (the launcher runs privileged; the app
    /// never calls this).
    pub fn open_packet_fd(iface_index: u32) -> io::Result<OwnedFd> {
        open_bound(iface_index)
    }

    /// Wrap an `AF_PACKET` socket fd received from the privileged launcher (via
    /// `SCM_RIGHTS`) — the unprivileged app's path. The fd must be a bound
    /// `AF_PACKET` raw socket the launcher created with
    /// [`PacketSource::open_packet_fd`]; no validation is performed here, matching
    /// the trust the audited fd-bridge + launcher established when handing it over.
    /// The OwnedFd (never a RawFd) crosses this safe public API per ADR-024.
    #[must_use]
    pub fn from_owned_fd(fd: OwnedFd) -> Self {
        Self { fd }
    }

    /// Receive one packet frame into `buf`. Returns a [`CapturedPacket`] whose
    /// `frame` is the bytes the kernel filled (Ethernet header included — feed
    /// it to [`five_tuple`]) and `outgoing` is the direction (true = sent by this
    /// host / tx, false = received / rx). A recv timeout surfaces as
    /// [`io::ErrorKind::WouldBlock`] (no packet this interval); other read
    /// failures (e.g. the interface went down) are returned verbatim.
    pub fn recv<'a>(&self, buf: &'a mut [u8]) -> io::Result<CapturedPacket<'a>> {
        let (n, outgoing) = recv_packet(&self.fd, buf)?;
        Ok(CapturedPacket {
            frame: &buf[..n],
            outgoing,
        })
    }
}

/// Open + bind + set the recv timeout, returning the owned fd. Shared by
/// [`PacketSource::open`] (wraps it) and [`PacketSource::open_packet_fd`]
/// (yields it for a privileged launcher to pass via `SCM_RIGHTS`). Private — the
/// audited `socket`/`bind`/`setsockopt` sequence stays in one place.
fn open_bound(iface_index: u32) -> io::Result<OwnedFd> {
    let fd = packet_socket()?;
    bind_interface(&fd, iface_index)?;
    set_recv_timeout(&fd, 200)?;
    Ok(fd)
}

/// One captured frame + its direction. `outgoing = true` means this host sent
/// the frame (charge to tx); `false` means it was received (charge to rx).
#[derive(Debug, Clone, Copy)]
pub struct CapturedPacket<'a> {
    /// The frame bytes the kernel filled (Ethernet header included).
    pub frame: &'a [u8],
    /// Whether this host transmitted (`true`) or received (`false`) the frame.
    pub outgoing: bool,
}

#[cfg(test)]
#[path = "../tests/headless/packet_source.rs"]
mod tests;
