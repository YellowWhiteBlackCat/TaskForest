//! Linux namespace inode comparison as a typed process-isolation fact.

use serde::{Deserialize, Serialize};

use crate::core::device_state::DeviceState;
use crate::core::failure::FailureKind;

/// Namespace classes exposed by the Linux procfs namespace directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinuxNamespaceKind {
    Pid,
    Mount,
    Network,
    Ipc,
    Uts,
    User,
    Cgroup,
}

impl LinuxNamespaceKind {
    /// The `/proc/<pid>/ns` entry name.
    #[must_use]
    pub const fn proc_name(self) -> &'static str {
        match self {
            Self::Pid => "pid",
            Self::Mount => "mnt",
            Self::Network => "net",
            Self::Ipc => "ipc",
            Self::Uts => "uts",
            Self::User => "user",
            Self::Cgroup => "cgroup",
        }
    }

    /// Stable display label independent of the native filename abbreviation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pid => "PID",
            Self::Mount => "Mount",
            Self::Network => "Net",
            Self::Ipc => "IPC",
            Self::Uts => "UTS",
            Self::User => "User",
            Self::Cgroup => "Cgroup",
        }
    }

    /// All seven namespace classes in stable UI order.
    pub const ALL: [Self; 7] = [
        Self::Pid,
        Self::Mount,
        Self::Network,
        Self::Ipc,
        Self::Uts,
        Self::User,
        Self::Cgroup,
    ];
}

/// Result of comparing one process namespace inode with the host PID 1 inode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NamespaceAuditStatus {
    Host { inode: u64 },
    Isolated { inode: u64, host_inode: u64 },
    Unavailable(FailureKind),
}

impl NamespaceAuditStatus {
    /// Whether the process is in a namespace different from PID 1.
    #[must_use]
    pub const fn is_isolated(&self) -> bool {
        matches!(self, Self::Isolated { .. })
    }

    /// The process-side inode when it was read successfully.
    #[must_use]
    pub const fn inode(&self) -> Option<u64> {
        match self {
            Self::Host { inode } | Self::Isolated { inode, .. } => Some(*inode),
            Self::Unavailable(_) => None,
        }
    }
}

/// One namespace comparison row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamespaceAuditEntry {
    pub kind: LinuxNamespaceKind,
    pub status: NamespaceAuditStatus,
}

/// Complete seven-class namespace audit for one process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LinuxNamespaceAudit {
    pub state: DeviceState,
    pub entries: Vec<NamespaceAuditEntry>,
}

impl LinuxNamespaceAudit {
    /// Construct an audit from provider-owned comparison rows.
    #[must_use]
    pub fn from_entries(state: DeviceState, mut entries: Vec<NamespaceAuditEntry>) -> Self {
        entries.sort_by_key(|entry| entry.kind);
        Self { state, entries }
    }

    /// Count namespace classes whose inode differs from PID 1.
    #[must_use]
    pub fn isolated_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.status.is_isolated())
            .count()
    }

    /// Whether at least one namespace proves a container-like isolation.
    #[must_use]
    pub fn is_container_like(&self) -> bool {
        self.isolated_count() > 0
    }

    /// Return one entry by its semantic namespace class.
    #[must_use]
    pub fn entry(&self, kind: LinuxNamespaceKind) -> Option<&NamespaceAuditEntry> {
        self.entries.iter().find(|entry| entry.kind == kind)
    }
}

/// Parse the inode number from a Linux namespace symlink target such as
/// `net:[4026531993]`.
#[must_use]
pub fn parse_namespace_inode(target: &str) -> Option<u64> {
    let open = target.rfind('[')?;
    let close = target.get(open + 1..)?.find(']')? + open + 1;
    let value = target.get(open + 1..close)?;
    (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.parse::<u64>().ok())
        .flatten()
}
