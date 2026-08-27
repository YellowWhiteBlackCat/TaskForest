use super::*;

#[test]
fn operation_parser_accepts_only_fixed_actions() {
    assert_eq!(Operation::parse("end"), Ok(Operation::End));
    assert_eq!(Operation::parse("kill"), Ok(Operation::Kill));
    assert_eq!(
        Operation::parse("priority:-10"),
        Ok(Operation::Priority(-10))
    );
    assert_eq!(
        Operation::parse("signal:user2"),
        Ok(Operation::Signal(SignalName::User2))
    );
    assert_eq!(
        Operation::parse("affinity:3,1,3"),
        Ok(Operation::Affinity(vec![1, 3]))
    );
    assert!(matches!(
        Operation::parse("sh -c kill"),
        Err(HelperError::Rejected(_))
    ));
    assert!(matches!(
        Operation::parse("priority:21"),
        Err(HelperError::Rejected(_))
    ));
}

#[cfg(unix)]
#[test]
fn stat_start_token_parser_handles_spaces_in_comm() {
    let mut fields = vec!["0"; 20];
    fields[0] = "S";
    fields[19] = "987654";
    let stat = format!("42 (name with spaces) {}", fields.join(" "));
    assert_eq!(parse_start_token(&stat), Some(987_654));
    assert_eq!(parse_start_token("malformed"), None);
}

#[test]
fn error_contracts_keep_distinct_exit_meanings() {
    let errors = [
        HelperError::ArgError("a".to_owned()),
        HelperError::IdentityChanged("b".to_owned()),
        HelperError::PermissionDenied("c".to_owned()),
        HelperError::Unsupported("d".to_owned()),
        HelperError::Rejected("e".to_owned()),
        HelperError::OperationFailed("f".to_owned()),
    ];
    let codes: std::collections::HashSet<_> = errors.iter().map(HelperError::exit_code).collect();
    assert_eq!(codes.len(), errors.len());
    assert_eq!(
        HelperError::PermissionDenied("x".to_owned()).kind(),
        "permission_denied"
    );
}

// Behavior tests for the check-to-act identity seam. A deterministic
// "pid was recycled into an innocent successor" cannot be constructed in a
// test, so these assert the equivalent invariants the TOCTOU fix must keep:
// a live own child IS reached (through the pinned pidfd), a mismatched token
// NEVER reaches it, and a reaped target is typed IdentityChanged — never a
// stray success — for every operation family.

#[cfg(target_os = "linux")]
mod signal_identity {
    use super::*;
    use std::process::Child;
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

    fn spawn_own_child() -> Child {
        Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn an own short-lived sleep child")
    }

    fn child_start_token(pid: u32) -> u64 {
        let text = std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .expect("read the child's /proc stat");
        parse_start_token(&text).expect("child stat carries a start token")
    }

    fn child_is_stopped(pid: u32) -> bool {
        let text = std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .expect("read the child's /proc stat");
        // Field 3 (first after the comm paren) is the scheduler state; a
        // job-control stop shows 'T'. This is the kernel's own observation,
        // not just the helper's return value.
        let after_comm = text
            .rfind(')')
            .map(|at| &text[at + 1..])
            .unwrap_or_default();
        after_comm.split_whitespace().next() == Some("T")
    }

    fn wait_until(pid: u32, want_stopped: bool, what: &str) {
        for _ in 0..400 {
            if child_is_stopped(pid) == want_stopped {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("child never reached the expected state: {what}");
    }

    fn cleanup(mut child: Child) {
        // SIGKILL reaps through even a SIGSTOPped child; wait prevents
        // zombies, so the test leaves no OS-visible litter behind.
        let _ = send_signal(child.id(), nix::sys::signal::Signal::SIGKILL);
        let _ = child.wait();
    }

    #[test]
    fn pidfd_signal_path_stops_and_resumes_a_live_own_child() {
        if let Err(error) = taskmanager_fd_bridge::pidfd_open(std::process::id()) {
            assert!(
                taskmanager_fd_bridge::is_pidfd_unsupported(&error),
                "unexpected pidfd_open failure: {error}"
            );
            eprintln!("skipping: this kernel has no pidfd (Linux < 5.1)");
            return;
        }
        let child = spawn_own_child();
        let pid = child.id();
        let token = child_start_token(pid);

        send_signal_checked(pid, token, SignalName::Stop).expect("SIGSTOP via the pidfd path");
        wait_until(pid, true, "SIGSTOP must reach the pinned child");
        send_signal_checked(pid, token, SignalName::Continue).expect("SIGCONT via the pidfd path");
        wait_until(pid, false, "SIGCONT must release the child");

        cleanup(child);
    }

    #[test]
    fn reaped_target_is_typed_identity_changed_for_every_operation_family() {
        let mut child = spawn_own_child();
        let pid = child.id();
        let token = child_start_token(pid);
        let _ = send_signal(pid, nix::sys::signal::Signal::SIGKILL);
        let _ = child.wait();

        // The pid is now reaped (worst case already recycled — the assertion
        // holds either way: a successor cannot carry the dead child's start
        // token). On a pidfd kernel this exercises the pidfd_open ESRCH ->
        // IdentityChanged mapping; on Linux < 5.1 it exercises the fallback's
        // /proc NotFound -> IdentityChanged mapping.
        for operation in [
            Operation::Suspend,
            Operation::Priority(5),
            Operation::Affinity(vec![0]),
        ] {
            let outcome = apply_operation(pid, token, &operation);
            assert!(
                matches!(outcome, Err(HelperError::IdentityChanged(_))),
                "reaped target must be IdentityChanged for {operation:?}, got {outcome:?}"
            );
        }
    }

    #[test]
    fn mismatched_token_never_signals_the_live_target() {
        let child = spawn_own_child();
        let pid = child.id();
        let token = child_start_token(pid);
        let wrong_token = token.wrapping_add(1);

        let outcome = send_signal_checked(pid, wrong_token, SignalName::Stop);
        assert!(
            matches!(outcome, Err(HelperError::IdentityChanged(_))),
            "mismatched token must fail typed, got {outcome:?}"
        );
        wait_until(pid, false, "a mismatched token must not stop the child");

        cleanup(child);
    }

    #[test]
    fn legacy_fallback_rechecks_identity_adjacent_to_the_signal() {
        // `send_signal_legacy_checked` is exactly the function the ENOSYS
        // (Linux < 5.1) branch and the non-Linux Unix build delegate to;
        // exercising it directly proves the fallback's behavior without
        // faking a kernel capability.
        let child = spawn_own_child();
        let pid = child.id();
        let token = child_start_token(pid);
        let wrong_token = token.wrapping_add(1);

        let rejected = send_signal_legacy_checked(pid, wrong_token, SignalName::Stop);
        assert!(
            matches!(rejected, Err(HelperError::IdentityChanged(_))),
            "fallback must reject a mismatched token, got {rejected:?}"
        );

        send_signal_legacy_checked(pid, token, SignalName::Stop).expect("legacy SIGSTOP");
        wait_until(pid, true, "legacy SIGSTOP must stop the child");
        send_signal_legacy_checked(pid, token, SignalName::Continue).expect("legacy SIGCONT");
        wait_until(pid, false, "legacy SIGCONT must release the child");

        cleanup(child);
    }
}
