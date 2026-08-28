use super::*;
use std::fs::File;
use std::io::{Read, Write};

/// Create a connected `AF_UNIX` `SOCK_STREAM` socketpair as owned fds (test
/// helper; the unsafe lives in tests, which the boundary contract does not
/// scan).
fn unix_pair() -> (OwnedFd, OwnedFd) {
    let mut fds = [0i32; 2];
    // SAFETY: socketpair writes two valid fds into `fds` on success.
    let rc = unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()) };
    assert_eq!(rc, 0, "socketpair failed: {}", io::Error::last_os_error());
    // SAFETY: on success socketpair returned two fresh, exclusively-owned fds.
    unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) }
}

/// The set of descriptors currently open in this test process, from
/// `/proc/self/fd`. Used to prove that a rejected multi-fd message leaves no
/// leaked descriptor behind (the kernel installs received fds into our table
/// the moment recvmsg returns).
fn open_fd_set() -> std::collections::BTreeSet<i32> {
    std::fs::read_dir("/proc/self/fd")
        .expect("read /proc/self/fd")
        .map(|entry| {
            entry
                .expect("readdir entry")
                .file_name()
                .to_string_lossy()
                .parse::<i32>()
                .expect("fd entries are numeric")
        })
        .collect()
}

/// Test-only raw `sendmsg` carrying `fds` in one `SCM_RIGHTS` cmsg — exactly
/// how a violating peer would try to inject several descriptors at once (the
/// safe `send_fd` API cannot construct this message).
fn send_fds_raw(channel: &OwnedFd, fds: &[libc::c_int]) {
    let mut carrier = 0u8;
    let carrier_ptr: *mut u8 = &mut carrier;
    let mut iov = libc::iovec {
        iov_base: carrier_ptr.cast(),
        iov_len: 1,
    };
    let payload_len = std::mem::size_of_val(fds) as u32;
    let cmsg_len =
        // SAFETY: pure size computation (CMSG macros are libc-unsafe by nature).
        unsafe { libc::CMSG_LEN(payload_len) } as usize;
    let cmsg_space =
        // SAFETY: pure size computation.
        unsafe { libc::CMSG_SPACE(payload_len) } as usize;
    assert!(cmsg_space <= 96, "test slab must fit the cmsg");
    let mut slab = CmsgBuffer::<96>::zeroed();
    // SAFETY: zeroed msghdr is the valid POD starting point.
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = slab.control_ptr();
    msg.msg_controllen = cmsg_space;
    // SAFETY: aligned slab; the header and the c_int payload are fully
    // written below before the sendmsg call.
    unsafe {
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len = cmsg_len;
        let data: *mut u8 = libc::CMSG_DATA(cmsg);
        for (index, fd) in fds.iter().enumerate() {
            let fd_ptr: *const libc::c_int = fd;
            std::ptr::copy_nonoverlapping(
                fd_ptr.cast::<u8>(),
                data.add(index * std::mem::size_of::<libc::c_int>()),
                std::mem::size_of::<libc::c_int>(),
            );
        }
    }
    // SAFETY: `channel` is a valid owned fd; `msg` and its buffers are valid
    // and outlive the call.
    let rc = unsafe { libc::sendmsg(channel.as_raw_fd(), &msg, 0) };
    assert!(
        rc >= 0,
        "raw multi-fd sendmsg failed: {}",
        io::Error::last_os_error()
    );
}

#[test]
fn send_fd_and_recv_fd_round_trip() {
    let (chan_a, chan_b) = unix_pair();
    // The recovered fd is a duplicate of `payload_a`, so bytes written to
    // `payload_b` come out of it — proving it is the same open file description.
    let (payload_a, payload_b) = unix_pair();
    send_fd(&chan_a, &payload_a).expect("send_fd");
    let recovered = recv_fd(&chan_b).expect("recv_fd");

    let mut writer = File::from(payload_b);
    writer.write_all(b"hello-fd-bridge").unwrap();
    drop(writer);
    let mut reader = File::from(recovered);
    let mut got = String::new();
    reader.read_to_string(&mut got).unwrap();
    assert_eq!(got, "hello-fd-bridge");
}

#[test]
fn send_fd_keeps_the_original_fd_open() {
    // Ownership is not transferred: prove payload_a is still a LIVE open file
    // description after the send (not merely a non-negative integer) by
    // round-tripping a byte through the payload pair — payload_a↔payload_b.
    let (chan_a, _chan_b) = unix_pair();
    let (payload_a, payload_b) = unix_pair();
    send_fd(&chan_a, &payload_a).expect("send_fd");
    let mut writer = File::from(payload_b);
    writer.write_all(b"x").unwrap();
    drop(writer);
    let mut reader = File::from(payload_a);
    let mut buf = [0u8; 1];
    reader.read_exact(&mut buf).unwrap();
    assert_eq!(buf[0], b'x', "sender's fd still usable after send_fd");
}

#[test]
fn find_scm_rights_accepts_exactly_scm_rights_and_skips_other_sol_socket_ancillary() {
    // SCM_RIGHTS alone → the fd.
    let mut rights = CmsgBuffer::<HANDOFF_CONTROL_LEN>::zeroed();
    {
        let msg = header_for(&mut rights);
        // SAFETY: audited boundary-crate code; justification below is kept adjacent to the block it guards.
        let cmsg = unsafe { libc::CMSG_FIRSTHDR(&msg) };
        // SAFETY: audited boundary-crate code; justification below is kept adjacent to the block it guards.
        unsafe {
            (*cmsg).cmsg_level = libc::SOL_SOCKET;
            (*cmsg).cmsg_type = libc::SCM_RIGHTS;
            (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<libc::c_int>() as u32) as usize;
            *libc::CMSG_DATA(cmsg).cast::<libc::c_int>() = 4242;
        }
    }
    let fd = find_scm_rights(&rights, CMSG_SPACE_UCRED);
    assert_eq!(fd, Some(vec![4242]));

    // SOL_SOCKET + SCM_CREDENTIALS (a real peer's credentials, not an fd):
    // must be SKIPPED, never interpreted as an fd (a `&&`→`||` mutation of
    // the accept predicate would fabricate one from credential bytes).
    let mut creds = CmsgBuffer::<HANDOFF_CONTROL_LEN>::zeroed();
    {
        let msg = header_for(&mut creds);
        // SAFETY: audited boundary-crate code; justification below is kept adjacent to the block it guards.
        let cmsg = unsafe { libc::CMSG_FIRSTHDR(&msg) };
        // SAFETY: audited boundary-crate code; justification below is kept adjacent to the block it guards.
        unsafe {
            (*cmsg).cmsg_level = libc::SOL_SOCKET;
            (*cmsg).cmsg_type = libc::SCM_CREDENTIALS;
            (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<libc::ucred>() as u32) as usize;
            *libc::CMSG_DATA(cmsg).cast::<libc::ucred>() = libc::ucred {
                pid: 1234,
                uid: 1000,
                gid: 1000,
            };
        }
    }
    assert_eq!(
        find_scm_rights(&creds, CMSG_SPACE_UCRED),
        None,
        "credential ancillary data must not be received as an fd"
    );

    // Empty / junk control data → None (no panic, no fabricated fd). The
    // clamped/lying length variants must stay bounded: a controllen beyond
    // the slab neither panics nor extends the walk past it.
    let junk = CmsgBuffer::<HANDOFF_CONTROL_LEN> {
        bytes: [0xFFu8; HANDOFF_CONTROL_LEN],
    };
    assert_eq!(find_scm_rights(&rights, 0), None);
    assert_eq!(find_scm_rights(&junk, 4096), None);
    assert_eq!(find_scm_rights(&rights, 4096), Some(vec![4242]));
    assert_eq!(find_scm_rights(&junk, HANDOFF_CONTROL_LEN), None);
}

#[test]
fn find_scm_rights_rejects_a_cmsg_overrunning_the_walked_window() {
    // Regression (fuzz-found 2026-08-26): CMSG_NXTHDR only guarantees the
    // next cmsg HEADER fits msg_controllen (glibc's __cmsg_nxthdr checks
    // exactly that), so a garbage chain can park a rights header inside the
    // walked window while its declared payload runs past the window end.
    // The old guard compared cmsg_len against the WHOLE window, letting the
    // fd-copy loop read past it — on the 32-byte product slab the first
    // shape below read 16 bytes past the slab itself. The walk must stop at
    // the violation; no fd may be fabricated from outside the window.
    fn header(bytes: &mut [u8], offset: usize, len: u64, level: i32, kind: i32) {
        bytes[offset..offset + 8].copy_from_slice(&len.to_le_bytes());
        bytes[offset + 8..offset + 12].copy_from_slice(&level.to_le_bytes());
        bytes[offset + 12..offset + 16].copy_from_slice(&kind.to_le_bytes());
    }

    // Product-slab shape (32-byte window): a header-only credentials cmsg at
    // 0, then a "rights" cmsg at offset 16 whose header fits the window but
    // whose declared length 32 claims four fds at 32..48 — past the slab.
    let mut overrun = [0u8; HANDOFF_CONTROL_LEN];
    header(&mut overrun, 0, 16, libc::SOL_SOCKET, libc::SCM_CREDENTIALS);
    header(&mut overrun, 16, 32, libc::SOL_SOCKET, libc::SCM_RIGHTS);
    let product = CmsgBuffer::<HANDOFF_CONTROL_LEN> { bytes: overrun };
    assert_eq!(
        find_scm_rights(&product, HANDOFF_CONTROL_LEN),
        None,
        "an overrunning cmsg must end the walk, never read past the slab"
    );

    // Fuzzer's exact shape (96-byte slab, 51-byte window): credentials cmsg
    // of length 28 at 0, rights cmsg at offset 32 whose 20-byte length fits
    // the whole window but overruns the 19 bytes remaining from offset 32 —
    // the old walk read one fd from 48..52, one byte past the window.
    let mut straddle = [0u8; 96];
    header(
        &mut straddle,
        0,
        28,
        libc::SOL_SOCKET,
        libc::SCM_CREDENTIALS,
    );
    header(&mut straddle, 32, 20, libc::SOL_SOCKET, libc::SCM_RIGHTS);
    straddle[48..51].copy_from_slice(&[0x00, 0x00, 0x38]);
    let fuzzed = CmsgBuffer::<96> { bytes: straddle };
    assert_eq!(
        find_scm_rights(&fuzzed, 51),
        None,
        "no fd may be fabricated from bytes outside the walked window"
    );
}

#[test]
fn find_scm_rights_walks_past_credentials_to_a_later_rights_cmsg() {
    // A message carrying credentials followed by a real SCM_RIGHTS cmsg:
    // the walk must skip the first and still find the fd. A ucred cmsg
    // alone spans 40 bytes, so this test needs a bigger aligned slab than
    // the product CmsgBuffer.
    let mut buf = CmsgBuffer::<96>::zeroed();
    // SAFETY: zeroed msghdr is the valid POD starting point.
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    // SAFETY: CmsgBuffer pins cmsghdr alignment; the walk stays within it.
    msg.msg_control = buf.control_ptr();
    // SAFETY: the first test cmsg (SCM_CREDENTIALS) spans CMSG_SPACE(12)
    // = 40 bytes; the second (SCM_RIGHTS) adds CMSG_SPACE(4) = 32.
    msg.msg_controllen = unsafe {
        libc::CMSG_SPACE(std::mem::size_of::<libc::ucred>() as u32) as usize
            + libc::CMSG_SPACE(std::mem::size_of::<libc::c_int>() as u32) as usize
    };
    // SAFETY: aligned slab; both cmsg headers are fully written
    // below before the walk.
    unsafe {
        let creds = libc::CMSG_FIRSTHDR(&msg);
        assert!(!creds.is_null());
        (*creds).cmsg_level = libc::SOL_SOCKET;
        (*creds).cmsg_type = libc::SCM_CREDENTIALS;
        (*creds).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<libc::ucred>() as u32) as usize;
        *libc::CMSG_DATA(creds).cast::<libc::ucred>() = libc::ucred {
            pid: 1,
            uid: 1,
            gid: 1,
        };
        let rights = libc::CMSG_NXTHDR(&msg, creds);
        assert!(!rights.is_null(), "second cmsg slot must exist");
        (*rights).cmsg_level = libc::SOL_SOCKET;
        (*rights).cmsg_type = libc::SCM_RIGHTS;
        (*rights).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<libc::c_int>() as u32) as usize;
        *libc::CMSG_DATA(rights).cast::<libc::c_int>() = 777;
    }
    // The walk consumes the real control length, not the whole buffer.
    assert_eq!(find_scm_rights(&buf, msg.msg_controllen), Some(vec![777]));
}

#[test]
fn find_scm_rights_accumulates_multi_fd_and_multi_cmsg_violations() {
    // One cmsg carrying TWO fds: the walk must surface both, so recv_fd can
    // count them and reject (the pre-hardening walk read only the first).
    let mut two = CmsgBuffer::<96>::zeroed();
    let msg = header_for96(&mut two);
    // SAFETY: aligned slab, header fully written below.
    unsafe {
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len = libc::CMSG_LEN(2 * std::mem::size_of::<libc::c_int>() as u32) as usize;
        *libc::CMSG_DATA(cmsg).cast::<libc::c_int>() = 111;
        *libc::CMSG_DATA(cmsg)
            .add(std::mem::size_of::<libc::c_int>())
            .cast::<libc::c_int>() = 222;
    }
    assert_eq!(
        // SAFETY: pure size computation for two c_ints.
        find_scm_rights(&two, unsafe {
            libc::CMSG_LEN(2 * std::mem::size_of::<libc::c_int>() as u32)
        } as usize),
        Some(vec![111, 222])
    );

    // TWO separate rights cmsgs: the accumulated count must still be 2.
    let mut twice = CmsgBuffer::<96>::zeroed();
    // SAFETY: zeroed msghdr is the valid POD starting point.
    let mut msg2: libc::msghdr = unsafe { std::mem::zeroed() };
    // SAFETY: aligned slab; walk stays within it.
    msg2.msg_control = twice.control_ptr();
    msg2.msg_controllen =
        // SAFETY: pure size computations.
        unsafe { 2 * libc::CMSG_SPACE(std::mem::size_of::<libc::c_int>() as u32) as usize };
    // SAFETY: both headers fully written below before the walk.
    unsafe {
        let first = libc::CMSG_FIRSTHDR(&msg2);
        (*first).cmsg_level = libc::SOL_SOCKET;
        (*first).cmsg_type = libc::SCM_RIGHTS;
        (*first).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<libc::c_int>() as u32) as usize;
        *libc::CMSG_DATA(first).cast::<libc::c_int>() = 5;
        let second = libc::CMSG_NXTHDR(&msg2, first);
        assert!(!second.is_null(), "second slot must exist");
        (*second).cmsg_level = libc::SOL_SOCKET;
        (*second).cmsg_type = libc::SCM_RIGHTS;
        (*second).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<libc::c_int>() as u32) as usize;
        *libc::CMSG_DATA(second).cast::<libc::c_int>() = 6;
    }
    assert_eq!(
        find_scm_rights(&twice, msg2.msg_controllen),
        Some(vec![5, 6])
    );
}

#[test]
fn recv_fd_distinguishes_orderly_close_from_no_fd() {
    // The peer closing its half returns n == 0: recv_fd must surface
    // UnexpectedEof, NOT the stale-errno path (n < 0) and not InvalidData.
    let (chan_a, chan_b) = unix_pair();
    drop(chan_a);

    let error = recv_fd(&chan_b).expect_err("orderly close must be an error");
    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
}

#[test]
fn recv_fd_rejects_a_two_fd_message_and_closes_both_descriptors() {
    let (chan_a, chan_b) = unix_pair();
    let (payload_a, payload_b) = unix_pair();
    let before = open_fd_set();

    send_fds_raw(&chan_a, &[payload_a.as_raw_fd(), payload_b.as_raw_fd()]);
    let error = recv_fd(&chan_b).expect_err("a two-fd message is a protocol violation");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(
        error.to_string().contains("exactly one"),
        "the typed error must name the violation: {error}"
    );
    // The kernel installed BOTH fds into this table at recvmsg time; the
    // rejection must have closed them, leaving the fd set unchanged.
    assert_eq!(
        open_fd_set(),
        before,
        "rejected two-fd message leaked a descriptor"
    );
}

#[test]
fn recv_fd_rejects_a_truncated_control_message_and_closes_what_installed() {
    // Three fds cannot fit the product slab (two visible + MSG_CTRUNC):
    // fail-closed, and the two the kernel did install must be closed.
    let (chan_a, chan_b) = unix_pair();
    let (payload_a, payload_b) = unix_pair();
    let (extra_a, extra_b) = unix_pair();
    let before = open_fd_set();

    send_fds_raw(
        &chan_a,
        &[
            payload_a.as_raw_fd(),
            payload_b.as_raw_fd(),
            extra_a.as_raw_fd(),
        ],
    );
    let error = recv_fd(&chan_b).expect_err("a three-fd message is a protocol violation");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(
        error.to_string().contains("truncated"),
        "the typed error must name the truncation: {error}"
    );
    assert_eq!(
        open_fd_set(),
        before,
        "truncated message leaked an installed descriptor"
    );
    drop(extra_b);
}

#[test]
fn retry_on_eintr_resumes_after_interrupted_and_propagates_other_errors() {
    // The wrapper's semantics, not the kernel's: N interrupted attempts are
    // transparently retried; any other error surfaces immediately.
    let mut attempts = 0;
    let result = retry_on_eintr(|| {
        attempts += 1;
        if attempts < 3 {
            Err(io::Error::from(io::ErrorKind::Interrupted))
        } else {
            Ok(attempts)
        }
    });
    assert_eq!(result.expect("third attempt succeeds"), 3);

    let error =
        retry_on_eintr(|| -> io::Result<()> { Err(io::Error::from(io::ErrorKind::WouldBlock)) })
            .expect_err("non-EINTR error must propagate immediately");
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
}

#[test]
fn peer_credentials_reports_the_kernel_side_of_the_socket() {
    // On a self-connected pair SO_PEERCRED must describe THIS process — the
    // values are cross-anchored against libc::getuid/getgid and
    // std::process::id, proving the getsockopt plumbing moves real kernel
    // data (not zeros). This is the seam the net-launcher receiver's uid==0
    // gate consumes.
    let (ours, _theirs) = std::os::unix::net::UnixStream::pair().expect("unix pair");
    let creds = peer_credentials(&ours).expect("peer_credentials");
    // SAFETY: getuid/getgid are pure process-attribute queries.
    assert_eq!(creds.uid, unsafe { libc::getuid() });
    // SAFETY: pure process-attribute query.
    assert_eq!(creds.gid, unsafe { libc::getgid() });
    assert_eq!(creds.pid, std::process::id() as i32);
}

#[test]
fn peer_credentials_surfaces_the_os_error_for_non_socket_fds() {
    // The documented contract: "other descriptors surface the OS error
    // verbatim" — ENOTSOCK here, never fabricated zero credentials (a
    // uid=0 fabricated answer would wrongly pass the launcher's root gate).
    let not_a_socket = File::open("/dev/null").expect("/dev/null");
    let error = peer_credentials(&not_a_socket).expect_err("getsockopt on a non-socket fd");
    assert_eq!(error.raw_os_error(), Some(libc::ENOTSOCK));
}

#[test]
fn send_fd_surfaces_the_os_error_for_a_non_socket_channel() {
    // The documented failure mode ("the socket is not connected") with the
    // most deterministic producer: a descriptor that is not a socket at all
    // → ENOTSOCK through the same cvt path every send error takes.
    let not_a_socket = File::open("/dev/null").expect("/dev/null");
    let (payload, _keep_open) = unix_pair();
    let error = send_fd(&not_a_socket, &payload).expect_err("sendmsg on a non-socket fd");
    assert_eq!(error.raw_os_error(), Some(libc::ENOTSOCK));
}

#[test]
fn recv_fd_rejects_a_payload_only_message_with_no_fd() {
    // A carrier byte WITHOUT ancillary data is not a handoff: the receiver
    // must fail closed with InvalidData — distinguishable from the orderly
    // close's UnexpectedEof — never fabricate or return a stale fd.
    let (chan_a, chan_b) = unix_pair();
    let mut peer = File::from(chan_a);
    peer.write_all(&[0]).expect("write the carrier byte");
    drop(peer);
    let error = recv_fd(&chan_b).expect_err("a payload-only message is not a handoff");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(
        error.to_string().contains("no SCM_RIGHTS"),
        "the typed error must name the violation: {error}"
    );
}

#[test]
fn find_scm_rights_accepts_a_rights_cmsg_that_fills_the_buffer_exactly() {
    // A valid SCM_RIGHTS cmsg whose length equals the whole control
    // length (len > controllen is the malformed case; len == controllen
    // is a legal full-buffer message and must still parse).
    let mut buf = CmsgBuffer::<HANDOFF_CONTROL_LEN>::zeroed();
    let mut msg = header_for(&mut buf);
    // SAFETY: aligned CmsgBuffer, header fully written below.
    unsafe {
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<libc::c_int>() as u32) as usize;
        *libc::CMSG_DATA(cmsg).cast::<libc::c_int>() = 555;
        msg.msg_controllen = (*cmsg).cmsg_len;
    }
    assert_eq!(find_scm_rights(&buf, msg.msg_controllen), Some(vec![555]));
}

#[test]
fn find_scm_rights_rejects_header_only_cmsg() {
    // cmsg_len == sizeof(cmsghdr): a header with no payload is not a
    // valid SCM_RIGHTS carrier and must not yield an fd.
    let mut buf = CmsgBuffer::<HANDOFF_CONTROL_LEN>::zeroed();
    let mut msg = header_for(&mut buf);
    // SAFETY: aligned CmsgBuffer, header fully written below.
    unsafe {
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len = std::mem::size_of::<libc::cmsghdr>();
        msg.msg_controllen = (*cmsg).cmsg_len;
    }
    assert_eq!(find_scm_rights(&buf, msg.msg_controllen), None);
}

#[test]
fn find_scm_rights_rejects_a_partial_fd_payload() {
    // A rights payload must contain whole c_int values. The parser must stop
    // before CMSG_DATA is copied when the peer declares a partial word.
    let mut buf = CmsgBuffer::<HANDOFF_CONTROL_LEN>::zeroed();
    let mut msg = header_for(&mut buf);
    // SAFETY: aligned CmsgBuffer, header fully written below.
    unsafe {
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len = libc::CMSG_LEN(5) as usize;
        msg.msg_controllen = (*cmsg).cmsg_len;
    }
    assert_eq!(find_scm_rights(&buf, msg.msg_controllen), None);
}

/// `CMSG_SPACE(size_of::<ucred>())` — the claimed controllen the header_for
/// helpers lay out (largest payload the tests fill).
const CMSG_SPACE_UCRED: usize =
    // SAFETY: pure size computation.
    unsafe { libc::CMSG_SPACE(std::mem::size_of::<libc::ucred>() as u32) } as usize;

/// Zeroed msghdr pointing at the aligned product-sized CmsgBuffer, with
/// `msg_controllen` set to the real `CMSG_SPACE` for a `ucred` (the largest
/// payload the tests fill; CMSG_FIRSTHDR needs a truthful length to lay out
/// the first header).
fn header_for(buf: &mut CmsgBuffer<HANDOFF_CONTROL_LEN>) -> libc::msghdr {
    // SAFETY: audited boundary-crate code; justification below is kept adjacent to the block it guards.
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_control = buf.control_ptr();
    msg.msg_controllen = CMSG_SPACE_UCRED;
    msg
}

/// The 96-byte-slab variant used by the multi-fd walk tests.
fn header_for96(buf: &mut CmsgBuffer<96>) -> libc::msghdr {
    // SAFETY: audited boundary-crate code; justification below is kept adjacent to the block it guards.
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_control = buf.control_ptr();
    msg.msg_controllen = CMSG_SPACE_UCRED;
    msg
}
