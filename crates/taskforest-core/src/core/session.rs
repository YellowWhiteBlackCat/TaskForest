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
    pub remote: bool,
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
