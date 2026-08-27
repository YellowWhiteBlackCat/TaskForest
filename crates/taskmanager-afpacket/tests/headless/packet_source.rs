use super::*;
use std::fs::File;
use std::io::Write;

/// Create a connected `AF_UNIX` `SOCK_DGRAM` socketpair as owned fds (test
/// helper; the unsafe lives in tests, which the boundary contract does not
/// scan — same shape as fd-bridge's `unix_pair`).
fn datagram_pair() -> (OwnedFd, OwnedFd) {
    let mut fds = [0i32; 2];
    // SAFETY: socketpair writes two valid fds into `fds` on success.
    let rc = unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_DGRAM, 0, fds.as_mut_ptr()) };
    assert_eq!(rc, 0, "socketpair failed: {}", io::Error::last_os_error());
    // SAFETY: on success socketpair returned two fresh, exclusively-owned fds.
    unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) }
}

/// `open()` without `CAP_NET_RAW` returns `Err` (EPERM). CI and dev hosts
/// run unprivileged, so — like perf-ioctl's PMU-open failure test — we
/// assert the FAILURE PATH only and never claim success. This exercises the
/// `fd < 0 → last_os_error` branch of the audited `socket` site.
#[test]
fn open_without_cap_net_raw_returns_err() {
    let result = PacketSource::open(1);
    assert!(
        result.is_err(),
        "expected Err without CAP_NET_RAW (unprivileged CI), got {result:?}"
    );
}

/// The privileged launcher's `open_packet_fd` must fail closed under the
/// same `CAP_NET_RAW` gate as `open` — an unprivileged caller can never
/// obtain a raw socket fd to pass along.
#[test]
fn open_packet_fd_without_cap_net_raw_returns_err() {
    let result = PacketSource::open_packet_fd(1);
    assert!(
        result.is_err(),
        "expected Err without CAP_NET_RAW (unprivileged CI), got {result:?}"
    );
}

/// `recv` must return exactly the datagram the kernel delivered on the
/// wrapped descriptor — the frame slice comes from the kernel's byte count,
/// never the caller's whole buffer — and a non-`PACKET_OUTGOING` packet
/// (here: a plain unix datagram, whose ingress address leaves the direction
/// bit at 0) must be reported as received (`outgoing = false`).
#[test]
fn recv_returns_the_delivered_frame_and_rx_direction() {
    let (send_side, recv_side) = datagram_pair();
    let frame: Vec<u8> = (0..64u8).collect();
    let mut writer = File::from(send_side);
    writer.write_all(&frame).expect("write the probe datagram");
    drop(writer);

    let source = PacketSource::from_owned_fd(recv_side);
    let mut buf = [0u8; 1500];
    let captured = source.recv(&mut buf).expect("recv on the wrapped fd");
    assert_eq!(
        captured.frame,
        &frame[..],
        "recv must return exactly the delivered bytes, not the whole buffer"
    );
    assert!(!captured.outgoing, "a non-PACKET_OUTGOING packet is rx");
}

/// `recv` on a wrapped descriptor that is not a socket must surface the OS
/// error verbatim (ENOTSOCK) — the fail-closed honesty the unprivileged app
/// needs if the launcher ever hands over a wrong fd; no fabricated frame,
/// no panic.
#[test]
fn recv_surfaces_the_os_error_for_a_non_socket_fd() {
    let null = File::open("/dev/null").expect("/dev/null");
    let source = PacketSource::from_owned_fd(null.into());
    let mut buf = [0u8; 64];
    let error = source.recv(&mut buf).expect_err("recv on a non-socket fd");
    assert_eq!(error.raw_os_error(), Some(libc::ENOTSOCK));
}
