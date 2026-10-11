use super::*;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
#[cfg(unix)]
use std::sync::atomic::{AtomicUsize, Ordering};
use taskmanager_core::SessionId;
use taskmanager_core::core::session::{SessionControlAction, SessionIdentity, SessionIncarnation};

const FIXTURE_BOOT_ID: &str = "01234567-89ab-cdef-0123-456789abcdef";

fn fixture_boot_id() -> Result<String, ProviderFailure> {
    Ok(FIXTURE_BOOT_ID.to_string())
}

#[cfg(unix)]
fn unavailable_boot_id() -> Result<String, ProviderFailure> {
    Err(ProviderFailure::Unsupported)
}

fn fixture_identity(boot_id: &str, created_monotonic_usec: u64) -> SessionIdentity {
    SessionIdentity {
        session_id: SessionId::new("fixture"),
        incarnation: SessionIncarnation::LinuxLogind {
            boot_id: boot_id.to_string(),
            created_monotonic_usec,
        },
    }
}

#[cfg(unix)]
fn successful_output(stdout: &[u8]) -> BoundedOutput {
    BoundedOutput {
        status: std::process::ExitStatus::from_raw(0),
        stdout: stdout.to_vec(),
        stderr: Vec::new(),
    }
}

#[cfg(unix)]
fn failed_output(stderr: &[u8]) -> BoundedOutput {
    BoundedOutput {
        status: std::process::ExitStatus::from_raw(1 << 8),
        stdout: Vec::new(),
        stderr: stderr.to_vec(),
    }
}

#[cfg(unix)]
fn successful_session_scan(
    command: &mut Command,
    timeout: Duration,
) -> Result<BoundedOutput, BoundedCommandError> {
    assert_eq!(command.get_program(), "loginctl");
    let arguments = command
        .get_args()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    match arguments.as_slice() {
        [command, flag] if command == "list-sessions" && flag == "--no-legend" => {
            assert_eq!(timeout, SESSION_CONTROL_TIMEOUT);
            Ok(successful_output(
                b"fixture 1000 alice seat0 tty2 no 2026-08-22 12:00:00\n",
            ))
        }
        [id_property, timestamp_property, command, id]
            if id_property == "--property=Id"
                && timestamp_property == "--property=TimestampMonotonic"
                && command == "show-session"
                && id == "fixture" =>
        {
            assert!(!timeout.is_zero());
            assert!(timeout <= SESSION_IDENTITY_ENRICHMENT_BUDGET);
            Ok(successful_output(
                b"Id=fixture\nTimestampMonotonic=123456789\n",
            ))
        }
        _ => panic!("unexpected loginctl arguments: {arguments:?}"),
    }
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
    match arguments.as_slice() {
        [id_property, timestamp_property, command, id]
            if id_property == "--property=Id"
                && timestamp_property == "--property=TimestampMonotonic"
                && command == "show-session"
                && id == "fixture" =>
        {
            assert_eq!(timeout, SESSION_CONTROL_TIMEOUT);
            Ok(successful_output(
                b"Id=fixture\nTimestampMonotonic=123456789\n",
            ))
        }
        [verb, id] if verb.ends_with("-session") && id == "fixture" => {
            assert_eq!(timeout, SESSION_CONTROL_TIMEOUT);
            Ok(failed_output(b"fixture access denied"))
        }
        _ => panic!("unexpected loginctl arguments: {arguments:?}"),
    }
}

#[cfg(unix)]
static CONTROL_CALLS: AtomicUsize = AtomicUsize::new(0);

#[cfg(unix)]
fn ordered_session_control(
    command: &mut Command,
    timeout: Duration,
) -> Result<BoundedOutput, BoundedCommandError> {
    let call = CONTROL_CALLS.fetch_add(1, Ordering::SeqCst);
    assert_eq!(command.get_program(), "loginctl");
    let arguments = command
        .get_args()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    match (call, arguments.as_slice()) {
        (0, [id_property, timestamp_property, command, id])
            if id_property == "--property=Id"
                && timestamp_property == "--property=TimestampMonotonic"
                && command == "show-session"
                && id == "fixture" =>
        {
            assert_eq!(timeout, SESSION_CONTROL_TIMEOUT);
            Ok(successful_output(
                b"Id=fixture\nTimestampMonotonic=123456789\n",
            ))
        }
        (1, [verb, id]) if verb == "terminate-session" && id == "fixture" => {
            assert_eq!(timeout, SESSION_CONTROL_TIMEOUT);
            Ok(successful_output(b""))
        }
        (2, [id_property, timestamp_property, command, id])
            if id_property == "--property=Id"
                && timestamp_property == "--property=TimestampMonotonic"
                && command == "show-session"
                && id == "fixture" =>
        {
            assert_eq!(timeout, SESSION_CONTROL_TIMEOUT);
            Ok(successful_output(
                b"Id=fixture\nTimestampMonotonic=123456789\n",
            ))
        }
        (3, [verb, id]) if verb == "lock-session" && id == "fixture" => {
            assert_eq!(timeout, SESSION_CONTROL_TIMEOUT);
            Ok(successful_output(b""))
        }
        _ => panic!("unexpected loginctl call {call}: {arguments:?}"),
    }
}

#[cfg(unix)]
fn stale_session_identity(
    command: &mut Command,
    timeout: Duration,
) -> Result<BoundedOutput, BoundedCommandError> {
    assert_eq!(command.get_program(), "loginctl");
    assert_eq!(
        command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        [
            "--property=Id",
            "--property=TimestampMonotonic",
            "show-session",
            "fixture"
        ]
    );
    assert_eq!(timeout, SESSION_CONTROL_TIMEOUT);
    Ok(successful_output(
        b"Id=fixture\nTimestampMonotonic=999999999\n",
    ))
}

#[cfg(unix)]
fn different_session_id_identity(
    command: &mut Command,
    timeout: Duration,
) -> Result<BoundedOutput, BoundedCommandError> {
    assert_eq!(command.get_program(), "loginctl");
    assert_eq!(
        command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        [
            "--property=Id",
            "--property=TimestampMonotonic",
            "show-session",
            "fixture"
        ]
    );
    assert_eq!(timeout, SESSION_CONTROL_TIMEOUT);
    Ok(successful_output(
        b"Id=replaced\nTimestampMonotonic=123456789\n",
    ))
}

#[cfg(unix)]
fn missing_session_identity_property(
    command: &mut Command,
    timeout: Duration,
) -> Result<BoundedOutput, BoundedCommandError> {
    assert_eq!(command.get_program(), "loginctl");
    assert!(
        command
            .get_args()
            .any(|argument| argument == "show-session")
    );
    assert_eq!(timeout, SESSION_CONTROL_TIMEOUT);
    Ok(successful_output(b"Id=fixture\n"))
}

#[cfg(unix)]
fn unexpected_session_action(
    command: &mut Command,
    _timeout: Duration,
) -> Result<BoundedOutput, BoundedCommandError> {
    let arguments = command
        .get_args()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(arguments.get(2).map(String::as_str), Some("show-session"));
    Ok(successful_output(
        b"Id=fixture\nTimestampMonotonic=123456789\n",
    ))
}

#[cfg(unix)]
fn zero_creation_session_identity(
    command: &mut Command,
    timeout: Duration,
) -> Result<BoundedOutput, BoundedCommandError> {
    assert_eq!(command.get_program(), "loginctl");
    assert_eq!(
        command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        [
            "--property=Id",
            "--property=TimestampMonotonic",
            "show-session",
            "fixture"
        ]
    );
    assert_eq!(timeout, SESSION_CONTROL_TIMEOUT);
    Ok(successful_output(b"Id=fixture\nTimestampMonotonic=0\n"))
}

#[cfg(unix)]
fn partial_session_identity_scan(
    command: &mut Command,
    timeout: Duration,
) -> Result<BoundedOutput, BoundedCommandError> {
    assert_eq!(command.get_program(), "loginctl");
    let arguments = command
        .get_args()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    match arguments.as_slice() {
        [command, flag] if command == "list-sessions" && flag == "--no-legend" => {
            assert_eq!(timeout, SESSION_CONTROL_TIMEOUT);
            Ok(successful_output(
                b"present 1000 alice seat0 tty2 no\nmissing 1001 bob - pts/0 yes\n",
            ))
        }
        [id_property, timestamp_property, command, id]
            if id_property == "--property=Id"
                && timestamp_property == "--property=TimestampMonotonic"
                && command == "show-session" =>
        {
            assert!(!timeout.is_zero());
            match id.as_str() {
                "present" => Ok(successful_output(b"Id=present\nTimestampMonotonic=700\n")),
                "missing" => Ok(failed_output(b"No session 'missing' known")),
                _ => panic!("unexpected session id: {id}"),
            }
        }
        _ => panic!("unexpected loginctl arguments: {arguments:?}"),
    }
}

#[cfg(unix)]
fn session_listing_without_identity_query(
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
    Ok(successful_output(b"fixture 1000 alice seat0 tty2 no\n"))
}

#[test]
#[cfg(unix)]
fn scan_executes_the_bounded_provider_and_returns_its_parsed_session() {
    let manager = SessionManager::with_sources(successful_session_scan, fixture_boot_id);

    let sessions = manager.try_scan().expect("fixture provider should succeed");

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, SessionId::new("fixture"));
    assert_eq!(sessions[0].uid, 1000);
    assert_eq!(sessions[0].user, "alice");
    assert_eq!(sessions[0].seat.as_deref(), Some("seat0"));
    assert_eq!(sessions[0].tty.as_deref(), Some("tty2"));
    assert_eq!(
        sessions[0].incarnation,
        Some(SessionIncarnation::LinuxLogind {
            boot_id: FIXTURE_BOOT_ID.to_string(),
            created_monotonic_usec: 123_456_789,
        })
    );
}

#[test]
#[cfg(unix)]
fn scan_keeps_rows_when_logind_cannot_supply_their_incarnation() {
    let manager = SessionManager::with_sources(partial_session_identity_scan, fixture_boot_id);

    let sessions = manager.try_scan().expect("session listing should succeed");

    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0].id, SessionId::new("present"));
    assert_eq!(
        sessions[0].incarnation,
        Some(SessionIncarnation::LinuxLogind {
            boot_id: FIXTURE_BOOT_ID.to_string(),
            created_monotonic_usec: 700,
        })
    );
    assert_eq!(sessions[1].id, SessionId::new("missing"));
    assert_eq!(sessions[1].user, "bob");
    assert_eq!(sessions[1].incarnation, None);
}

#[test]
#[cfg(unix)]
fn missing_boot_id_keeps_session_row_without_identity() {
    let manager =
        SessionManager::with_sources(session_listing_without_identity_query, unavailable_boot_id);

    let sessions = manager.try_scan().expect("session listing should succeed");

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, SessionId::new("fixture"));
    assert_eq!(sessions[0].user, "alice");
    assert_eq!(sessions[0].incarnation, None);
}

#[test]
#[cfg(unix)]
fn control_revalidates_before_one_native_action_and_preserves_provider_denial() {
    let manager = SessionManager::with_sources(denied_session_control, fixture_boot_id);
    let target = fixture_identity(FIXTURE_BOOT_ID, 123_456_789);

    assert_eq!(
        manager.control(&target, SessionControlAction::Lock),
        Err(ProviderFailure::PermissionDenied)
    );
}

#[test]
#[cfg(unix)]
fn control_rechecks_full_identity_and_sends_no_action_when_creation_changes() {
    let manager = SessionManager::with_sources(stale_session_identity, fixture_boot_id);
    let target = fixture_identity(FIXTURE_BOOT_ID, 123_456_789);

    assert_eq!(
        manager.control(&target, SessionControlAction::Disconnect),
        Err(ProviderFailure::IdentityChanged)
    );
}

#[test]
#[cfg(unix)]
fn control_rejects_a_replaced_logind_id_without_dispatch() {
    let manager = SessionManager::with_sources(different_session_id_identity, fixture_boot_id);
    let target = fixture_identity(FIXTURE_BOOT_ID, 123_456_789);

    assert_eq!(
        manager.control(&target, SessionControlAction::Disconnect),
        Err(ProviderFailure::IdentityChanged)
    );
}

#[test]
#[cfg(unix)]
fn control_fails_closed_when_creation_property_is_missing() {
    let manager = SessionManager::with_sources(missing_session_identity_property, fixture_boot_id);
    let target = fixture_identity(FIXTURE_BOOT_ID, 123_456_789);

    assert_eq!(
        manager.control(&target, SessionControlAction::Disconnect),
        Err(ProviderFailure::Unsupported)
    );
}

#[test]
#[cfg(unix)]
fn zero_creation_timestamp_cannot_become_a_linux_incarnation() {
    let manager = SessionManager::with_sources(zero_creation_session_identity, fixture_boot_id);

    assert_eq!(
        manager.query_session_incarnation("fixture", FIXTURE_BOOT_ID, SESSION_CONTROL_TIMEOUT),
        Err(ProviderFailure::Unsupported)
    );
}

#[test]
#[cfg(unix)]
fn control_rejects_a_different_boot_before_querying_or_dispatching() {
    let manager = SessionManager::with_sources(unexpected_session_action, fixture_boot_id);
    let target = fixture_identity("fedcba98-7654-3210-fedc-ba9876543210", 123_456_789);

    assert_eq!(
        manager.control(&target, SessionControlAction::Disconnect),
        Err(ProviderFailure::IdentityChanged)
    );
}

#[test]
#[cfg(unix)]
fn control_rejects_invalid_incarnation_before_querying_or_dispatching() {
    let manager = SessionManager::with_sources(unexpected_session_action, fixture_boot_id);
    let target = fixture_identity(FIXTURE_BOOT_ID, 0);

    assert_eq!(
        manager.control(&target, SessionControlAction::Disconnect),
        Err(ProviderFailure::Unsupported)
    );
}

#[test]
#[cfg(unix)]
fn matching_identity_dispatches_each_action_once_after_revalidation() {
    CONTROL_CALLS.store(0, Ordering::SeqCst);
    let manager = SessionManager::with_sources(ordered_session_control, fixture_boot_id);
    let target = fixture_identity(FIXTURE_BOOT_ID, 123_456_789);

    assert_eq!(
        manager.control(&target, SessionControlAction::Disconnect),
        Ok(())
    );
    assert_eq!(manager.control(&target, SessionControlAction::Lock), Ok(()));
    assert_eq!(CONTROL_CALLS.load(Ordering::SeqCst), 4);
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

#[test]
fn logind_identity_properties_require_exact_keys_and_unsigned_creation_time() {
    assert_eq!(
        parse_session_identity_properties("Id=7\nTimestampMonotonic=42\n"),
        Ok(SessionIdentityProperties {
            id: "7".into(),
            created_monotonic_usec: 42,
        })
    );
    assert_eq!(
        parse_session_identity_properties("Id=7\nTimestampMonotonic=-1\n"),
        Err(ProviderFailure::Unsupported)
    );
    assert_eq!(
        parse_session_identity_properties("Id=7\n"),
        Err(ProviderFailure::Unsupported)
    );
    assert_eq!(
        parse_session_identity_properties("Id=7\nId=8\nTimestampMonotonic=42\n"),
        Err(ProviderFailure::Unsupported)
    );
}

#[test]
fn boot_id_normalization_accepts_only_kernel_uuid_shape() {
    assert_eq!(
        normalize_boot_id("01234567-89AB-CDEF-0123-456789ABCDEF\n"),
        Some(FIXTURE_BOOT_ID.to_string())
    );
    assert_eq!(normalize_boot_id("not-a-boot-id"), None);
    assert_eq!(normalize_boot_id("0123456789abcdef0123456789abcdef"), None);
    assert_eq!(
        normalize_boot_id("00000000-0000-0000-0000-000000000000"),
        None
    );
}
