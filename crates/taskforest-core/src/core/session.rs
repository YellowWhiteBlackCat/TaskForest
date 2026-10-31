//! Platform-neutral login-session contract.

use serde::{Deserialize, Serialize};

use super::target::SessionId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SessionControlAction {
    Disconnect,
    Lock,
}

/// OS-owned facts that identify one login lifetime independently of a
/// renderer refresh or display field.
///
/// The providers re-read these facts immediately before control dispatch.
/// They are equality tokens rather than authorization secrets; an OS API that
/// accepts only a reusable session number can still have a small check/action
/// race after the preflight.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum SessionIncarnation {
    /// systemd logind's per-boot session id plus the session object's
    /// immutable monotonic creation timestamp.
    LinuxLogind {
        boot_id: String,
        created_monotonic_usec: u64,
    },
    /// WTSINFOEX_LEVEL1_W.LogonTime, in 100 ns intervals since 1601 UTC.
    WindowsWts { logon_time_100ns: u64 },
}

impl SessionIncarnation {
    /// Reject missing source evidence instead of constructing an identity from
    /// a zero timestamp or empty Linux boot id.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        match self {
            Self::LinuxLogind {
                boot_id,
                created_monotonic_usec,
            } => !boot_id.trim().is_empty() && *created_monotonic_usec > 0,
            Self::WindowsWts { logon_time_100ns } => *logon_time_100ns > 0,
        }
    }
}

/// Exact provider-issued session id and its OS-owned login-lifetime evidence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SessionIdentity {
    pub session_id: SessionId,
    pub incarnation: SessionIncarnation,
}

impl SessionIdentity {
    #[must_use]
    pub fn new(session_id: SessionId, incarnation: SessionIncarnation) -> Option<Self> {
        let identity = Self {
            session_id,
            incarnation,
        };
        identity.is_valid().then_some(identity)
    }

    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self.session_id.as_str().is_empty() && self.incarnation.is_valid()
    }
}

/// The terminal family that owns a login session, in the one vocabulary every
/// frontend wears: a display server, a console terminal, a session reached over
/// the network, or one that nothing identified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionFamily {
    /// A display server (Wayland or X11) owns the seat.
    Graphical,
    /// A console terminal (virtual or physical) owns the session.
    Console,
    /// The session was reached from another host; the transport is the
    /// stronger fact whatever terminal carries it.
    Remote,
    /// The identifying facts were observed, but they name no family.
    Unknown,
}

/// The session-family fact together with its availability.
///
/// Keeping availability inside the fact is what stops a renderer from
/// borrowing a neighbouring plate: "nothing observed" and "this provider
/// cannot observe families" are typed outcomes, never a default that reads as
/// a real family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionFamilyFact {
    /// The provider observed the identifying facts and classified them.
    Observed(SessionFamily),
    /// The provider reported the session but nothing identified its family.
    #[default]
    Unobserved,
    /// The provider cannot observe the identifying facts at all.
    Unavailable,
}

impl SessionFamilyFact {
    /// The one rule that turns observed session facts into a family.
    ///
    /// Providers call this rather than re-deriving a family of their own, so
    /// every platform answers the same question the same way: the transport
    /// wins over the terminal, a display tty is a display-server session, and
    /// a terminal tty is a console session. Facts that identify nothing stay
    /// unobserved instead of being guessed.
    #[must_use]
    pub fn classify(seat: Option<&str>, tty: Option<&str>, remote: bool) -> Self {
        if remote {
            return Self::Observed(SessionFamily::Remote);
        }
        if let Some(tty) = tty.map(str::trim) {
            if is_display_tty(tty) {
                return Self::Observed(SessionFamily::Graphical);
            }
            if is_terminal_tty(tty) {
                return Self::Observed(SessionFamily::Console);
            }
        }
        if tty.is_some_and(|tty| !tty.trim().is_empty()) {
            // Something was observed, but it names no family: that is an
            // observed unknown, not an absence of observation.
            return Self::Observed(SessionFamily::Unknown);
        }
        if seat.is_some_and(|seat| !seat.trim().is_empty()) {
            // A seat without a terminal belongs to a display session.
            return Self::Observed(SessionFamily::Graphical);
        }
        Self::Unobserved
    }

    /// The identified family, or `None` when the fact carries no family.
    #[must_use]
    pub const fn family(self) -> Option<SessionFamily> {
        match self {
            Self::Observed(family) => Some(family),
            Self::Unobserved | Self::Unavailable => None,
        }
    }

    /// Whether the identifying facts were observed at all.
    #[must_use]
    pub const fn is_observed(self) -> bool {
        matches!(self, Self::Observed(_))
    }
}

/// A compositor-owned terminal: an X11 display (`:0`) or a Wayland seat
/// (`wayland-0`).
fn is_display_tty(tty: &str) -> bool {
    tty.starts_with(':') || tty.contains("wayland")
}

/// A terminal-owned line: a pseudo terminal (`pts/3`, `/dev/pts/3`) or a
/// physical console (`tty1`, `/dev/tty1`).
fn is_terminal_tty(tty: &str) -> bool {
    tty.eq_ignore_ascii_case("console")
        || tty.starts_with("pts")
        || tty.contains("/dev/pts/")
        || tty.starts_with("tty")
        || tty.contains("/dev/tty")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SessionItem {
    pub id: SessionId,
    /// Absent when this provider cannot obtain and revalidate a trusted
    /// lifecycle token. Such a row remains displayable but has no stable row
    /// anchor or control target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incarnation: Option<SessionIncarnation>,
    pub uid: u32,
    pub user: String,
    pub seat: Option<String>,
    pub tty: Option<String>,
    /// The terminal family this session belongs to, with its availability.
    ///
    /// Providers classify through [`SessionFamilyFact::classify`] so the rule
    /// has one authority; a provider that cannot observe the identifying facts
    /// at all states [`SessionFamilyFact::Unavailable`] instead of guessing.
    /// An old payload without this key means the family was never observed.
    #[serde(default)]
    pub family: SessionFamilyFact,
    pub timestamp: Option<String>,
}

impl SessionItem {
    /// Return the complete control/anchor identity only when both owner facts
    /// are present and valid.
    #[must_use]
    pub fn identity(&self) -> Option<SessionIdentity> {
        SessionIdentity::new(self.id.clone(), self.incarnation.clone()?)
    }
}
