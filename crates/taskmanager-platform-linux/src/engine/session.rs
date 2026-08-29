//! Login sessions — one row per `loginctl` session (Win11 TM / Mission Center's
//! Users tab shows sessions, not a per-user rollup).
//!
//! Data source: `loginctl list-sessions --no-legend`. Its column layout has
//! drifted across systemd versions (classic: `SESSION UID USER SEAT TTY REMOTE
//! TIMESTAMP`; systemd 256+: `SESSION UID USER SEAT LEADER CLASS TTY IDLE
//! SINCE`). Rather than parse by fixed column index, [`parse_loginctl_sessions`]
//! reads the three stable leading columns (SESSION / UID / USER) positionally
//! and detects SEAT / TTY / remote / timestamp by token pattern — so it stays
//! correct across layouts without churn.
//!
//! This module is a pure data layer — no gpui/UI deps — mirroring the layout of
//! the engine's `services` and `startup` modules: a [`SessionItem`] row, a pure
//! [`parse_loginctl_sessions`] parser (unit-tested in isolation), and a
//! [`SessionManager`] whose `scan()` runs `loginctl` (cfg(unix)) and whose
//! actions `terminate_session` / `lock_session` / `unlock_session` shell out via
//! `loginctl <verb>-session <id>` (cfg(unix); cfg(not(unix)) stubs).

use std::process::Command;
use std::time::Duration;
use taskmanager_core::core::session::SessionItem;
use tracing::{debug, info};

use taskmanager_platform_portable::{BoundedCommandError, BoundedOutput, run_with_timeout};

const SESSION_CONTROL_TIMEOUT: Duration = Duration::from_secs(2);
type SessionCommandRunner =
    fn(&mut Command, Duration) -> Result<BoundedOutput, BoundedCommandError>;

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
            id,
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
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self::with_command_runner(run_with_timeout)
    }

    fn with_command_runner(command_runner: SessionCommandRunner) -> Self {
        Self { command_runner }
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
            let sessions = parse_loginctl_sessions(&String::from_utf8_lossy(&output.stdout));
            debug!("Scanned {} login sessions.", sessions.len());
            Ok(sessions)
        }
        #[cfg(not(unix))]
        {
            Err(SessionScanFailure::Unavailable)
        }
    }

    /// Terminate (log off) a session — `loginctl terminate-session <id>`.
    /// Wired to the Users-tab "Disconnect" button.
    pub fn terminate_session(&self, id: &str) -> Result<(), String> {
        info!("Terminating login session {}", id);
        #[cfg(unix)]
        {
            Self::exec_loginctl(&["terminate-session", id], self.command_runner)
        }
        #[cfg(not(unix))]
        {
            let _ = id;
            Err("terminate-session not supported on this platform".into())
        }
    }

    /// Lock a session's active VT — `loginctl lock-session <id>`.
    pub fn lock_session(&self, id: &str) -> Result<(), String> {
        info!("Locking login session {}", id);
        #[cfg(unix)]
        {
            Self::exec_loginctl(&["lock-session", id], self.command_runner)
        }
        #[cfg(not(unix))]
        {
            let _ = id;
            Err("lock-session not supported on this platform".into())
        }
    }

    /// Unlock a session's active VT — `loginctl unlock-session <id>`.
    #[cfg(any(feature = "test-support", test))]
    pub fn unlock_session(&self, id: &str) -> Result<(), String> {
        info!("Unlocking login session {}", id);
        #[cfg(unix)]
        {
            Self::exec_loginctl(&["unlock-session", id], self.command_runner)
        }
        #[cfg(not(unix))]
        {
            let _ = id;
            Err("unlock-session not supported on this platform".into())
        }
    }

    /// Run `loginctl <args>` and mirror `crate::engine::services::ServiceManager`'s
    /// Ok/Err shape: `Ok(())` on success, `Err(stderr)` on non-zero exit. Direct
    /// invocation (no `sh -c`) keeps the session id out of a shell — injection-
    /// safe and consistent with how `services.rs` drives `systemctl`.
    #[cfg(unix)]
    fn exec_loginctl(args: &[&str], command_runner: SessionCommandRunner) -> Result<(), String> {
        let mut command = Command::new("loginctl");
        command.args(args);
        let output =
            command_runner(&mut command, SESSION_CONTROL_TIMEOUT).map_err(|error| match error {
                BoundedCommandError::Spawn(error) => error.to_string(),
                BoundedCommandError::TimedOut | BoundedCommandError::ReaderTimedOut => {
                    "loginctl request timed out".to_string()
                }
                BoundedCommandError::ReaderStart(_)
                | BoundedCommandError::ReaderFailed
                | BoundedCommandError::ProcessTree => {
                    "loginctl request failed while waiting for the provider".to_string()
                }
                BoundedCommandError::OutputTooLarge => {
                    "loginctl output exceeded the hard capture limit".to_string()
                }
            })?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(stderr.to_string())
        }
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
