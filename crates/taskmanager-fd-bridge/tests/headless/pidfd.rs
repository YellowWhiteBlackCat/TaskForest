use super::*;

#[test]
fn pidfd_open_targets_a_live_pid_and_signal_zero_probes_it() {
    // The one process guaranteed alive: this test's own. Signal 0 performs
    // permission + existence checks without delivering anything (no side
    // effect), proving both halves of the syscall plumbing move real data.
    let pidfd = pidfd_open(std::process::id()).expect("pidfd_open on a live pid");
    pidfd_send_signal(&pidfd, 0).expect("signal 0 probes without delivering");
}

#[test]
fn pidfd_send_signal_rejects_an_invalid_signal_number() {
    // -1 is not a valid signal number: the kernel rejects it with EINVAL
    // before any delivery — the typed error path, never Ok and never a
    // panic.
    let pidfd = pidfd_open(std::process::id()).expect("pidfd_open on a live pid");
    let error = pidfd_send_signal(&pidfd, -1).expect_err("invalid signal number");
    assert_eq!(error.raw_os_error(), Some(libc::EINVAL));
}

#[test]
fn pidfd_open_on_an_impossible_pid_is_a_typed_esrch() {
    // 4_194_305 exceeds the kernel pid_max ceiling (2^22), so it can never
    // exist regardless of host configuration — the error must be the OS's
    // ESRCH, not a wrapped or fabricated kind.
    let error = pidfd_open(4_194_305).expect_err("impossible pid");
    assert_eq!(error.raw_os_error(), Some(libc::ESRCH));
}

#[test]
fn enosys_is_reported_as_unsupported_for_caller_fallback() {
    // The typed-fallback predicate: exactly ENOSYS means "kernel without
    // pidfd"; every other error keeps its own meaning.
    assert!(is_pidfd_unsupported(&io::Error::from_raw_os_error(
        libc::ENOSYS
    )));
    assert!(!is_pidfd_unsupported(&io::Error::from_raw_os_error(
        libc::ESRCH
    )));
    assert!(!is_pidfd_unsupported(&io::Error::from(
        io::ErrorKind::WouldBlock
    )));
}
