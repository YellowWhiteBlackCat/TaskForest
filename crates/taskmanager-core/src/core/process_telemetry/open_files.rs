//! Per-process open file descriptors as a typed insight facet.
//!
//! Mirrors the connection/isolation facets: every field is a fact procfs can
//! prove, and absent information stays `None` rather than being fabricated.

use serde::{Deserialize, Serialize};

use crate::core::device_state::DeviceState;

/// Coarse classification of an open file-descriptor target.
///
/// Derived solely from the readlink of `/proc/<pid>/fd/<n>`; the raw target
/// string is preserved verbatim on [`OpenFileEntry::target`] so a caller never
/// loses information to the classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenFileKind {
    /// A path-like target (file, device node, TTY, ...) that is neither a
    /// kernel socket nor a pipe.
    File,
    /// A kernel socket identified by inode, e.g. `socket:[12345]`.
    Socket,
    /// An anonymous or named pipe identified by inode, e.g. `pipe:[12345]`.
    Pipe,
    /// Any other, currently unclassifiable target.
    Other,
}

/// One open file descriptor belonging to a process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenFileEntry {
    /// The file descriptor number (e.g. `0`, `1`, `2`, ...).
    pub fd: u32,
    /// Coarse target classification.
    pub kind: OpenFileKind,
    /// Verbatim readlink target (e.g. `/dev/null`, `socket:[12345]`).
    /// `None` only when the descriptor exists but its target could not be read
    /// (for example the link vanished between enumeration and readlink).
    pub target: Option<String>,
}

/// The open-files facet: every readable descriptor plus a typed device state.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProcessOpenFiles {
    /// Aggregated collection state. `Healthy` when the fd directory was listed
    /// successfully, `PermissionDenied` when procfs refused access, `Stale`
    /// when the process vanished.
    pub state: DeviceState,
    /// All readable descriptors, ordered by ascending fd.
    pub entries: Vec<OpenFileEntry>,
    /// Number of descriptors that existed but whose target readlink failed.
    /// Each is also present in [`Self::entries`] with `target: None`; this
    /// scalar lets a caller surface "X unreadable" without re-scanning.
    pub unreadable_count: u32,
}
