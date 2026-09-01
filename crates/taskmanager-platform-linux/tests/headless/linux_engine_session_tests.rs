use super::*;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use taskmanager_core::SessionId;

#[cfg(unix)]
fn successful_session_scan(
    command: &mut Command,
    timeout: Duration,
) -> Result<BoundedOutput, BoundedCommandError> {
    assert_eq!(command.get_program(), "loginctl");
    assert_eq!(
        command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        ["list-sessions", "--no-legend"]
    );
    assert_eq!(timeout, SESSION_CONTROL_TIMEOUT);
    Ok(BoundedOutput {
        status: std::process::ExitStatus::from_raw(0),
        stdout: b"fixture 1000 alice seat0 tty2 no 2026-08-22 12:00:00\n".to_vec(),
        stderr: Vec::new(),
    })
}

#[cfg(unix)]
fn denied_session_control(
    command: &mut Command,
    timeout: Duration,
) -> Result<BoundedOutput, BoundedCommandError> {
    assert_eq!(command.get_program(), "loginctl");
    let arguments = command
        .get_args()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(arguments.len(), 2);
    assert!(arguments[0].ends_with("-session"));
    assert_eq!(arguments[1], "fixture");
    assert_eq!(timeout, SESSION_CONTROL_TIMEOUT);
    Ok(BoundedOutput {
        status: std::process::ExitStatus::from_raw(1 << 8),
        stdout: Vec::new(),
        stderr: b"fixture access denied".to_vec(),
    })
}

#[test]
#[cfg(unix)]
fn scan_executes_the_bounded_provider_and_returns_its_parsed_session() {
    let manager = SessionManager::with_command_runner(successful_session_scan);

    let sessions = manager.try_scan().expect("fixture provider should succeed");

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, SessionId::new("fixture"));
    assert_eq!(sessions[0].uid, 1000);
    assert_eq!(sessions[0].user, "alice");
    assert_eq!(sessions[0].seat.as_deref(), Some("seat0"));
    assert_eq!(sessions[0].tty.as_deref(), Some("tty2"));
}

#[test]
#[cfg(unix)]
fn every_public_session_control_propagates_a_nonzero_provider_result() {
    type SessionControl = fn(&SessionManager, &str) -> Result<(), String>;
    let manager = SessionManager::with_command_runner(denied_session_control);

    for control in [
        SessionManager::terminate_session as SessionControl,
        SessionManager::lock_session as SessionControl,
        SessionManager::unlock_session as SessionControl,
    ] {
        assert_eq!(
            control(&manager, "fixture"),
            Err("fixture access denied".to_string())
        );
    }
}

// ── classic layout (SESSION UID USER SEAT TTY REMOTE TIMESTAMP) ──────────

#[test]
fn parse_classic_layout_local_session() {
    let out = "2 1000 alice seat0 tty2 no Thu 2026-07-28 10:00:00 +0800";
    let s = parse_loginctl_sessions(out);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].id, SessionId::new("2"));
    assert_eq!(s[0].uid, 1000);
    assert_eq!(s[0].user, "alice");
    assert_eq!(s[0].seat.as_deref(), Some("seat0"));
    assert_eq!(s[0].tty.as_deref(), Some("tty2"));
    assert!(!s[0].remote);
    // Weekday/time tokens are kept; bare "+0800" (no `:`/`-`) is dropped.
    assert!(s[0].timestamp.as_deref().unwrap().contains("2026-07-28"));
    assert!(s[0].timestamp.as_deref().unwrap().contains("10:00:00"));
}

#[test]
fn scan_failure_classification_preserves_permission_denial() {
    assert_eq!(
        classify_session_scan_failure("Failed: Permission denied"),
        SessionScanFailure::PermissionDenied
    );
    assert_eq!(
        classify_session_scan_failure("System has not been booted with systemd"),
        SessionScanFailure::Unavailable
    );
}

#[test]
fn parse_classic_layout_remote_ssh_session() {
    // No seat, pts/0 TTY, remote=yes.
    let out = "3 1000 bob - pts/0 yes Thu 2026-07-28 11:30:00";
    let s = parse_loginctl_sessions(out);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].seat, None);
    assert_eq!(s[0].tty.as_deref(), Some("pts/0"));
    assert!(s[0].remote, "explicit `yes` flag ⇒ remote");
}

// ── systemd 256+ layout (LEADER / CLASS / IDLE / SINCE; no timestamp) ─────

#[test]
fn parse_systemd256_layout_local() {
    // Real output from systemd 261 on this host: extra LEADER/CLASS columns
    // and an IDLE flag instead of REMOTE/TIMESTAMP. seat + tty must still
    // resolve correctly and not pick up the LEADER pid.
    let out = "2 1000 devuser seat0 1156 user tty2 no -";
    let s = parse_loginctl_sessions(out);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].seat.as_deref(), Some("seat0"));
    assert_eq!(s[0].tty.as_deref(), Some("tty2"));
    assert_eq!(
        s[0].tty.as_deref(),
        Some("tty2"),
        "must not pick up LEADER pid `1156` as the TTY"
    );
    assert!(!s[0].remote, "`no` IDLE flag must read as not-remote");
    assert_eq!(s[0].timestamp, None, "no date/time token in this layout");
}

#[test]
fn parse_systemd256_layout_manager_session() {
    // A manager/session with no seat and no tty → inferred remote.
    let out = "3 1000 devuser - 1162 manager - no -";
    let s = parse_loginctl_sessions(out);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].seat, None);
    assert_eq!(s[0].tty, None);
    // No explicit yes/no BEFORE inference? `no` is present (IDLE) → used.
    assert!(!s[0].remote);
}

// ── tolerance: blanks, short rows, header, uid garbage ────────────────────

#[test]
fn parse_skips_blank_header_and_short_rows() {
    let out = [
        "SESSION  UID USER  SEAT  TTY",
        "2 1000 alice seat0 tty2",
        "",
        "  ",
        "7",          // < 3 columns → dropped
        "8 1000 bob", // exactly 3 → kept (seat/tty None)
    ]
    .join("\n");
    let s = parse_loginctl_sessions(&out);
    // header dropped, short row dropped → 2 real sessions.
    assert_eq!(s.len(), 2);
    assert_eq!(s[0].id, SessionId::new("2"));
    assert_eq!(s[1].id, SessionId::new("8"));
    assert_eq!(s[1].seat, None);
    assert!(s[1].remote, "no seat ⇒ inferred remote");
    assert_eq!(s[1].tty, None);
}

#[test]
fn parse_uid_garbage_falls_back_to_zero() {
    let s = parse_loginctl_sessions("9 notanumber carol seat0 tty2 no");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].uid, 0);
    assert_eq!(s[0].user, "carol");
}

#[test]
fn parse_empty_input() {
    assert!(parse_loginctl_sessions("").is_empty());
    assert!(parse_loginctl_sessions("\n  \n").is_empty());
}

#[test]
fn parse_multiple_sessions_preserve_order() {
    let out = [
        "2 1000 alice seat0 tty2 no Thu 2026-07-28 10:00:00",
        "3 1000 bob - pts/0 yes Thu 2026-07-28 11:00:00",
        "4 0 root seat0 tty1 no Thu 2026-07-28 09:00:00",
    ]
    .join("\n");
    let s = parse_loginctl_sessions(&out);
    assert_eq!(s.len(), 3);
    assert_eq!(s[0].id, SessionId::new("2"));
    assert_eq!(s[1].id, SessionId::new("3"));
    assert_eq!(s[2].id, SessionId::new("4"));
    assert!(s[1].remote);
    assert!(!s[0].remote);
}

#[test]
fn looks_datetime_drops_structural_tokens() {
    // TTY / seat / pid / yes-no / `-` must never read as a timestamp.
    assert!(!looks_datetime("seat0"));
    assert!(!looks_datetime("tty2"));
    assert!(!looks_datetime("1156"));
    assert!(!looks_datetime("yes"));
    assert!(!looks_datetime("-"));
    // Real date/time tokens are accepted.
    assert!(looks_datetime("10:00:00"));
    assert!(looks_datetime("2026-07-28"));
}
