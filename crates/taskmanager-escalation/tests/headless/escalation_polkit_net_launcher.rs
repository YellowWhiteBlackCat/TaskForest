//! Headless behavior tests for the Linux net-launcher driver's hardening
//! pieces: the abstract handoff name, the `SO_PEERCRED` uid gate, and the RAII
//! child-reap guard. The live pkexec chain itself stays on-box-only (no test
//! may drive pkexec); these tests exercise the real kernel surfaces (abstract
//! namespace, SO_PEERCRED, process reap) without any privilege.

use super::*;
use std::io::Read;
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixStream};
use std::time::{Duration, Instant};

/// This process's uid, learned through the same kernel seam production uses
/// (`SO_PEERCRED` on a self-connected pair). fd-bridge's own tests anchor that
/// seam against `libc::getuid()`/`std::process::id()`; escalation is
/// `forbid(unsafe_code)` with no libc dependency, so the pair is the available
/// oracle here.
fn own_uid() -> u32 {
    let (ours, _theirs) = UnixStream::pair().expect("unix pair");
    taskmanager_fd_bridge::peer_credentials(&ours)
        .expect("self credentials")
        .uid
}

/// Spawn `sleep`-style stuck child, or `None` when the host lacks the binary
/// (optional-host-tool skip, matching the xmllint precedent in escalation
/// tests; the exited-child halves below never need it).
fn stuck_child() -> Option<std::process::Child> {
    match std::process::Command::new("sleep")
        .arg("30")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(child) => Some(child),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            eprintln!("skip kill-path half: `sleep` is not installed");
            None
        }
        Err(error) => panic!("could not spawn the stuck-child stub: {error}"),
    }
}

#[test]
fn handoff_name_is_abstract_random_and_arg_safe() {
    let a = HandoffName::generate().expect("generate handoff name");
    let b = HandoffName::generate().expect("generate second handoff name");
    assert_ne!(
        a.as_bytes(),
        b.as_bytes(),
        "two names must differ (16 urandom bytes each)"
    );
    assert!(a.as_bytes().starts_with(b"tm-netl-"));
    assert_eq!(a.as_bytes().len(), b"tm-netl-".len() + 16);

    // The CLI-arg encoding must be NUL-free printable hex (pkexec passes argv
    // as C strings).
    let hex = a.hex_name();
    assert_eq!(hex.len(), (b"tm-netl-".len() + 16) * 2);
    assert!(
        hex.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "hex_name must stay argv-safe: {hex}"
    );

    // The name must round-trip through the kernel's abstract namespace —
    // abstract means as_abstract_name() Some and as_pathname() None, i.e. no
    // enumerable /tmp artifact exists for either name or address.
    let addr = SocketAddr::from_abstract_name(a.as_bytes()).expect("abstract addr");
    assert_eq!(addr.as_abstract_name(), Some(a.as_bytes()));
    assert!(
        addr.as_pathname().is_none(),
        "an abstract address is never a filesystem path"
    );
    let listener = bind_handoff_listener(&a).expect("bind abstract listener");
    let local = listener.local_addr().expect("local addr");
    assert_eq!(
        local.as_abstract_name().map(<[u8]>::to_vec),
        Some(a.as_bytes().to_vec())
    );
    assert!(
        local.as_pathname().is_none(),
        "the bound handoff socket must not appear on any filesystem"
    );
}

#[test]
fn non_root_peer_is_disconnected_and_root_peer_is_admitted() {
    // Limitation (single-user CI): a genuinely unprivileged PEER cannot be
    // staged without a second uid. The test self-connects, then asserts the
    // gate's decision for the process's OWN uid — rejection + disconnect when
    // unprivileged (the CI case), admission when run as root. The predicate
    // under test is the same uid==0 branch production evaluates per peer.
    let handoff = HandoffName::generate().expect("handoff name");
    let listener = bind_handoff_listener(&handoff).expect("bind abstract listener");
    listener.set_nonblocking(true).expect("set nonblocking");

    let addr_bytes = handoff.as_bytes().to_vec();
    let connector = std::thread::spawn(move || {
        let addr = SocketAddr::from_abstract_name(&addr_bytes).expect("abstract addr");
        let mut stream = UnixStream::connect_addr(&addr).expect("connect");
        // Block until the gate decides: admission means EOF when the accepted
        // stream drops at test end; rejection means EOF immediately.
        let mut sink = [0u8; 1];
        stream.read(&mut sink).expect("read after gate decision")
    });

    let outcome = accept_privileged_peer(
        &listener,
        Instant::now() + Duration::from_millis(400),
        || Ok(false),
    );

    if own_uid() == 0 {
        let admitted = outcome.expect("a uid-0 peer must be admitted");
        drop(admitted);
    } else {
        let error = outcome.expect_err("a non-root peer must never be admitted");
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(
            error.to_string().contains("unprivileged"),
            "the typed error must record the rejection: {error}"
        );
    }
    let eof = connector.join().expect("connector thread");
    assert_eq!(eof, 0, "the gated-off peer must observe EOF, never data");
}

#[test]
fn guard_wait_bounded_reaps_an_exited_child_and_kills_a_stuck_one() {
    // Exited child: the test harness itself, invoked with a filter that
    // matches nothing, runs zero tests and exits promptly — no pkexec, no
    // prompt, no side effect.
    let exited_spawn = std::process::Command::new(std::env::current_exe().expect("current exe"))
        .arg("--exact")
        .arg("tm-net-launcher-no-such-test")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn harness stub");
    let mut exited = PkexecChild::wrapping(exited_spawn);
    let _status = exited
        .wait_bounded(Duration::from_secs(5))
        .expect("bounded wait on an exited child returns its status");

    let Some(stuck_spawn) = stuck_child() else {
        return;
    };
    let pid = stuck_spawn.id();
    let mut stuck = PkexecChild::wrapping(stuck_spawn);
    let started = Instant::now();
    let error = stuck
        .wait_bounded(Duration::from_millis(150))
        .expect_err("a stuck child must surface TimedOut");
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "kill+wait after the deadline must be prompt"
    );
    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "the killed child must be fully reaped (no /proc entry)"
    );
}

#[test]
fn guard_drop_alone_reaps_a_stuck_child() {
    // The error-path fix: returning from obtain_fd via `?` drops the guard
    // WITHOUT wait_bounded — Drop alone must still boundedly kill + reap the
    // child, or the root launcher would hang on its ACK read forever. This is
    // the exact scenario of a recv_fd failure.
    let Some(stuck_spawn) = stuck_child() else {
        return;
    };
    let pid = stuck_spawn.id();
    let guard = PkexecChild::wrapping(stuck_spawn);
    drop(guard);
    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "Drop must have reaped the stuck child (no /proc entry)"
    );
}
