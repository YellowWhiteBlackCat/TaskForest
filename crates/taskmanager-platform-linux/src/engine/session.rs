//! Login sessions — one row per `loginctl` session (Win11 TM / Mission Center's
//! Users tab shows sessions, not a per-user rollup).
//!
//! Display data source: `loginctl list-sessions --no-legend`. Its column layout has
//! drifted across systemd versions (classic: `SESSION UID USER SEAT TTY REMOTE
//! TIMESTAMP`; systemd 256+: `SESSION UID USER SEAT LEADER CLASS TTY IDLE
//! SINCE`). Rather than parse by fixed column index, [`parse_loginctl_sessions`]
//! reads the three stable leading columns (SESSION / UID / USER) positionally
//! and detects SEAT / TTY / remote / timestamp by token pattern — so it stays
//! correct across layouts without churn.
//!
//! This module is a renderer-neutral platform layer — no gpui/UI deps — mirroring
//! the layout of the engine's `services` and `startup` modules: a [`SessionItem`]
//! row, a pure [`parse_loginctl_sessions`] parser (unit-tested in isolation), and a
//! [`SessionManager`] whose `scan()` runs `loginctl` (cfg(unix)) and whose
//! Session incarnation comes from the logind Session object's immutable
//! `Id` and `TimestampMonotonic` properties plus the kernel boot ID. Controls
//! re-read those facts immediately before submitting one bounded
//! `loginctl <verb>-session <id>` action.

use std::fs;
use std::process::Command;
use std::time::{Duration, Instant};
use taskforest_core::core::session::{
    SessionControlAction, SessionIdentity, SessionIncarnation, SessionItem,
};
use tracing::{debug, info};

use taskforest_contract::ProviderFailure;
use taskmanager_platform_portable::{BoundedCommandError, BoundedOutput, run_with_timeout};

const SESSION_CONTROL_TIMEOUT: Duration = Duration::from_secs(2);
const SESSION_IDENTITY_ENRICHMENT_BUDGET: Duration = Duration::from_secs(2);
const BOOT_ID_PATH: &str = "/proc/sys/kernel/random/boot_id";
type SessionCommandRunner =
    fn(&mut Command, Duration) -> Result<BoundedOutput, BoundedCommandError>;
type BootIdReader = fn() -> Result<String, ProviderFailure>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SessionScanFailure {
    Unavailable,
    PermissionDenied,
    TimedOut,
    ProviderFailed,
}

/// One login session row shown in the Users tab. Fields mirror the columns of
/// `loginctl list-sessions` (modulo layout drift — see [`parse_loginctl_sessions`]).
/// Parse `loginctl list-sessions --no-legend` output into [`SessionItem`]s.
///
/// The three leading columns — SESSION / UID / USER — are positional (stable in
/// every systemd version). The remaining columns are matched by token pattern
/// so the parser tolerates layout drift (LEADER / CLASS / IDLE columns added in
/// newer systemd; REMOTE / TIMESTAMP dropped) without mis-assigning fields:
///
/// - **seat**: first token starting with `seat`.
/// - **tty**: first token starting with `tty`, `pts/`, or equal to `console`.
/// - **remote**: an explicit `yes`/`no` token wins; otherwise inferred (no seat
///   → remote).
/// - **timestamp**: leftover tokens that look date/time-like (`HH:MM:SS` or
///   `<digits>-<digits>-…`); `None` when no such token exists (systemd 256+).
///
/// Blank lines and a stray header row (`SESSION …`, which `--no-legend`
/// suppresses but a caller might pass) are skipped. Rows with fewer than 3
/// columns are dropped (not enough to identify a session). uid parse failures
/// fall back to 0.
pub fn parse_loginctl_sessions(output: &str) -> Vec<SessionItem> {
    let mut out = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        // Need at least SESSION UID USER to identify a session.
        if parts.len() < 3 {
            continue;
        }
        // Defensive: skip a stray header row if a caller forgot --no-legend.
        if parts[0].eq_ignore_ascii_case("SESSION") {
            continue;
        }
        let id = parts[0].to_string();
        let uid = parts[1].parse::<u32>().unwrap_or(0);
        let user = parts[2].to_string();
        let rest = &parts[3..];

        let seat = rest
            .iter()
            .find(|t| t.starts_with("seat"))
            .map(|s| s.to_string());
        let tty = rest
            .iter()
            .find(|t| t.starts_with("tty") || t.starts_with("pts/") || **t == "console")
            .map(|s| s.to_string());
        // Explicit yes/no flag (classic layout) wins; else infer from seat.
        let remote = match rest.iter().find(|t| **t == "yes" || **t == "no") {
            Some(flag) => *flag == "yes",
            None => seat.is_none(),
        };
        // Best-effort timestamp: date/time-shaped tokens only, so LEADER pids
        // / CLASS words / `-` placeholders never leak into the Logon column.
        let ts: Vec<&str> = rest.iter().copied().filter(|t| looks_datetime(t)).collect();
        let timestamp = if ts.is_empty() {
            None
        } else {
            Some(ts.join(" "))
        };

        out.push(SessionItem {
            id: id.into(),
            incarnation: None,
            uid,
            user,
            seat,
            tty,
            remote,
            timestamp,
        });
    }
    out
}

/// True for a token shaped like a date or time: `HH:MM(:SS)` (contains `:`) or a
/// dashed date containing a digit (`2026-07-28`). Pure; used by
/// [`parse_loginctl_sessions`] to keep structural tokens out of the timestamp.
fn looks_datetime(t: &str) -> bool {
    if t.contains(':') {
        return true;
    }
    t.contains('-') && t.chars().any(|c| c.is_ascii_digit())
}

/// Scans + controls login sessions. Stateless across calls (each `scan` re-runs
/// `loginctl`); the UI snapshots a `Vec<SessionItem>` per render, the same way
/// it does for processes/services/startup.
#[derive(Debug, Clone)]
pub struct SessionManager {
    command_runner: SessionCommandRunner,
    boot_id_reader: BootIdReader,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self::with_sources(run_with_timeout, read_boot_id)
    }

    fn with_sources(command_runner: SessionCommandRunner, boot_id_reader: BootIdReader) -> Self {
        Self {
            command_runner,
            boot_id_reader,
        }
    }

    /// All current login sessions. cfg(unix): runs `loginctl list-sessions
    /// --no-legend` + parses; cfg(not(unix)): empty (no loginctl on Windows).
    #[cfg(feature = "test-support")]
    pub fn scan(&self) -> Vec<SessionItem> {
        self.try_scan().unwrap_or_default()
    }

    /// Typed scan used by the application adapter. A missing or denied
    /// `loginctl` must not be presented as a believable empty session list.
    pub(crate) fn try_scan(&self) -> Result<Vec<SessionItem>, SessionScanFailure> {
        #[cfg(unix)]
        {
            let mut command = Command::new("loginctl");
            command.args(["list-sessions", "--no-legend"]);
            let output =
                (self.command_runner)(&mut command, SESSION_CONTROL_TIMEOUT).map_err(|error| {
                    match error {
                        BoundedCommandError::Spawn(_) => SessionScanFailure::Unavailable,
                        BoundedCommandError::TimedOut | BoundedCommandError::ReaderTimedOut => {
                            SessionScanFailure::TimedOut
                        }
                        BoundedCommandError::ReaderStart(_)
                        | BoundedCommandError::ReaderFailed
                        | BoundedCommandError::ProcessTree
                        | BoundedCommandError::OutputTooLarge => SessionScanFailure::ProviderFailed,
                    }
                })?;
            if !output.status.success() {
                return Err(classify_session_scan_failure(&String::from_utf8_lossy(
                    &output.stderr,
                )));
            }
            let mut sessions = parse_loginctl_sessions(&String::from_utf8_lossy(&output.stdout));
            let boot_id = (self.boot_id_reader)().ok();
            if let Some(boot_id) = boot_id {
                let deadline = Instant::now() + SESSION_IDENTITY_ENRICHMENT_BUDGET;
                for session in &mut sessions {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        break;
                    }
                    session.incarnation = self
                        .query_session_incarnation(session.id.as_str(), &boot_id, remaining)
                        .ok();
                }
            }
            debug!("Scanned {} login sessions.", sessions.len());
            Ok(sessions)
        }
        #[cfg(not(unix))]
        {
            Err(SessionScanFailure::Unavailable)
        }
    }

    /// Submit one login-session action only after the captured identity has
    /// been re-read from logind and the current kernel boot.
    pub fn control(
        &self,
        target: &SessionIdentity,
        action: SessionControlAction,
    ) -> Result<(), ProviderFailure> {
        let SessionIncarnation::LinuxLogind {
            boot_id: expected_boot_id,
            ..
        } = &target.incarnation
        else {
            return Err(ProviderFailure::Unsupported);
        };
        if target.session_id.as_str().is_empty() {
            return Err(ProviderFailure::IdentityChanged);
        }
        if !target.incarnation.is_valid() {
            return Err(ProviderFailure::Unsupported);
        }

        let current_boot_id = (self.boot_id_reader)()?;
        if &current_boot_id != expected_boot_id {
            return Err(ProviderFailure::IdentityChanged);
        }

        let current_incarnation = self.query_session_incarnation(
            target.session_id.as_str(),
            &current_boot_id,
            SESSION_CONTROL_TIMEOUT,
        )?;
        if current_incarnation != target.incarnation {
            return Err(ProviderFailure::IdentityChanged);
        }

        let verb = match action {
            SessionControlAction::Disconnect => "terminate-session",
            SessionControlAction::Lock => "lock-session",
        };
        info!(
            "Submitting login session action {} for {}",
            verb,
            target.session_id.as_str()
        );
        let id = target.session_id.as_str();
        #[cfg(unix)]
        {
            Self::exec_loginctl(&[verb, id], self.command_runner)
        }
        #[cfg(not(unix))]
        {
            let _ = (verb, id);
            Err(ProviderFailure::Unsupported)
        }
    }

    fn query_session_incarnation(
        &self,
        session_id: &str,
        boot_id: &str,
        timeout: Duration,
    ) -> Result<SessionIncarnation, ProviderFailure> {
        if session_id.is_empty() || timeout.is_zero() {
            return Err(ProviderFailure::IdentityChanged);
        }
        let mut command = Command::new("loginctl");
        command.args([
            "--property=Id",
            "--property=TimestampMonotonic",
            "show-session",
            session_id,
        ]);
        let output = (self.command_runner)(&mut command, timeout).map_err(|error| match error {
            BoundedCommandError::Spawn(_) => ProviderFailure::Unsupported,
            BoundedCommandError::TimedOut | BoundedCommandError::ReaderTimedOut => {
                ProviderFailure::TimedOut
            }
            BoundedCommandError::ReaderStart(_)
            | BoundedCommandError::ReaderFailed
            | BoundedCommandError::ProcessTree
            | BoundedCommandError::OutputTooLarge => ProviderFailure::ProviderFault,
        })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(classify_session_identity_error(&stderr));
        }
        let properties =
            parse_session_identity_properties(&String::from_utf8_lossy(&output.stdout))?;
        if properties.id != session_id {
            return Err(ProviderFailure::IdentityChanged);
        }
        if properties.created_monotonic_usec == 0 {
            return Err(ProviderFailure::Unsupported);
        }
        Ok(SessionIncarnation::LinuxLogind {
            boot_id: boot_id.to_string(),
            created_monotonic_usec: properties.created_monotonic_usec,
        })
    }

    /// Run `loginctl <args>` and preserve its typed failure. Direct
    /// invocation (no `sh -c`) keeps the session id out of a shell — injection-
    /// safe and consistent with how `services.rs` drives `systemctl`.
    #[cfg(unix)]
    fn exec_loginctl(
        args: &[&str],
        command_runner: SessionCommandRunner,
    ) -> Result<(), ProviderFailure> {
        let mut command = Command::new("loginctl");
        command.args(args);
        let output =
            command_runner(&mut command, SESSION_CONTROL_TIMEOUT).map_err(|error| match error {
                BoundedCommandError::Spawn(_) => ProviderFailure::Unsupported,
                BoundedCommandError::TimedOut | BoundedCommandError::ReaderTimedOut => {
                    ProviderFailure::TimedOut
                }
                BoundedCommandError::ReaderStart(_)
                | BoundedCommandError::ReaderFailed
                | BoundedCommandError::ProcessTree => ProviderFailure::ProviderFault,
                BoundedCommandError::OutputTooLarge => ProviderFailure::ProviderFault,
            })?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(classify_session_control_error(&stderr))
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct SessionIdentityProperties {
    id: String,
    created_monotonic_usec: u64,
}

fn parse_session_identity_properties(
    output: &str,
) -> Result<SessionIdentityProperties, ProviderFailure> {
    let mut id = None;
    let mut created_monotonic_usec = None;
    for line in output.lines() {
        let Some((key, value)) = line.split_once('=') else {
            return Err(ProviderFailure::Unsupported);
        };
        match key {
            "Id" => {
                if id.replace(value.to_string()).is_some() {
                    return Err(ProviderFailure::Unsupported);
                }
            }
            "TimestampMonotonic" => {
                let parsed = value
                    .parse::<u64>()
                    .map_err(|_| ProviderFailure::Unsupported)?;
                if created_monotonic_usec.replace(parsed).is_some() {
                    return Err(ProviderFailure::Unsupported);
                }
            }
            _ => {}
        }
    }
    Ok(SessionIdentityProperties {
        id: id.ok_or(ProviderFailure::Unsupported)?,
        created_monotonic_usec: created_monotonic_usec.ok_or(ProviderFailure::Unsupported)?,
    })
}

fn read_boot_id() -> Result<String, ProviderFailure> {
    let value = fs::read_to_string(BOOT_ID_PATH).map_err(|_| ProviderFailure::Unsupported)?;
    normalize_boot_id(&value).ok_or(ProviderFailure::Unsupported)
}

fn normalize_boot_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.len() != 36 {
        return None;
    }
    for (index, byte) in trimmed.bytes().enumerate() {
        if matches!(index, 8 | 13 | 18 | 23) {
            if byte != b'-' {
                return None;
            }
        } else if !byte.is_ascii_hexdigit() {
            return None;
        }
    }
    let normalized = trimmed.to_ascii_lowercase();
    let compact = normalized.replace('-', "");
    if compact.chars().all(|character| character == '0')
        || compact.chars().all(|character| character == 'f')
    {
        return None;
    }
    Some(normalized)
}

fn classify_session_identity_error(error: &str) -> ProviderFailure {
    let lower = error.to_ascii_lowercase();
    if lower.contains("no session")
        || lower.contains("no such session")
        || lower.contains("unknown session")
        || lower.contains("session not found")
    {
        ProviderFailure::IdentityChanged
    } else if lower.contains("permission denied") || lower.contains("access denied") {
        ProviderFailure::PermissionDenied
    } else if lower.contains("unknown property")
        || lower.contains("failed to get property")
        || lower.contains("unknown command")
        || lower.contains("not supported")
        || lower.contains("unsupported")
    {
        ProviderFailure::Unsupported
    } else if lower.contains("timed out") || lower.contains("timeout") {
        ProviderFailure::TimedOut
    } else {
        ProviderFailure::TemporarilyUnavailable
    }
}

pub(crate) fn classify_session_control_error(error: &str) -> ProviderFailure {
    let lower = error.to_ascii_lowercase();
    if lower.contains("permission denied") || lower.contains("access denied") {
        ProviderFailure::PermissionDenied
    } else if lower.contains("timed out") || lower.contains("timeout") {
        ProviderFailure::TimedOut
    } else if lower.contains("no session")
        || lower.contains("no such session")
        || lower.contains("unknown session")
        || lower.contains("session not found")
    {
        ProviderFailure::IdentityChanged
    } else if lower.contains("no such file")
        || lower.contains("not supported")
        || lower.contains("unsupported")
    {
        ProviderFailure::Unsupported
    } else {
        ProviderFailure::Rejected
    }
}

fn classify_session_scan_failure(stderr: &str) -> SessionScanFailure {
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("permission denied") || lower.contains("access denied") {
        SessionScanFailure::PermissionDenied
    } else {
        SessionScanFailure::Unavailable
    }
}

#[cfg(test)]
#[path = "../../tests/headless/linux_engine_session_tests.rs"]
mod tests;
