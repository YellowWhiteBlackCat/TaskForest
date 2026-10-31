//! Per-process open file descriptors as a typed insight facet.
//!
//! Mirrors the connection/isolation facets: every field is a fact procfs can
//! prove, and absent information stays `None` rather than being fabricated.

use std::fmt;

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
    /// A path-like target (file, directory, ...) that is neither a
    /// kernel socket nor a pipe.
    File,
    /// A kernel socket identified by inode, e.g. `socket:[12345]`.
    Socket,
    /// An anonymous or named pipe identified by inode, e.g. `pipe:[12345]`.
    Pipe,
    /// A directory descriptor.
    Directory,
    /// A character or block device node, e.g. `/dev/dri/*`, `/dev/null`.
    Device,
    /// An epoll instance descriptor (`anon_inode:[eventpoll]`).
    Epoll,
    /// An eventfd notification primitive (`anon_inode:[eventfd]`).
    Eventfd,
    /// A signalfd descriptor (`anon_inode:[signalfd]`).
    Signalfd,
    /// A timerfd descriptor (`anon_inode:[timerfd]`).
    Timerfd,
    /// A pidfd process descriptor (`anon_inode:[pidfd]`).
    Pidfd,
    /// An anonymous memory file descriptor (`memfd:*`).
    Memfd,
    /// Any other, currently unclassifiable target.
    Other,
}

impl OpenFileKind {
    /// Returns a static string representation of this kind.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Socket => "socket",
            Self::Pipe => "pipe",
            Self::Directory => "directory",
            Self::Device => "device",
            Self::Epoll => "epoll",
            Self::Eventfd => "eventfd",
            Self::Signalfd => "signalfd",
            Self::Timerfd => "timerfd",
            Self::Pidfd => "pidfd",
            Self::Memfd => "memfd",
            Self::Other => "other",
        }
    }

    /// Returns `true` if this kind represents a socket.
    #[must_use]
    pub const fn is_socket(&self) -> bool {
        matches!(self, Self::Socket)
    }

    /// Returns `true` if this kind represents a pipe.
    #[must_use]
    pub const fn is_pipe(&self) -> bool {
        matches!(self, Self::Pipe)
    }

    /// Returns `true` if this kind represents an eventfd primitive.
    #[must_use]
    pub const fn is_eventfd(&self) -> bool {
        matches!(self, Self::Eventfd)
    }

    /// Returns `true` if this kind represents a regular file or path-like descriptor.
    #[must_use]
    pub const fn is_regular_file(&self) -> bool {
        matches!(self, Self::File)
    }

    /// Alias for [`Self::is_regular_file`].
    #[must_use]
    pub const fn is_file(&self) -> bool {
        self.is_regular_file()
    }

    /// Returns `true` if this kind represents a directory descriptor.
    #[must_use]
    pub const fn is_directory(&self) -> bool {
        matches!(self, Self::Directory)
    }

    /// Returns `true` if this kind represents a character or block device.
    #[must_use]
    pub const fn is_device(&self) -> bool {
        matches!(self, Self::Device)
    }

    /// Returns `true` if this kind represents an epoll instance.
    #[must_use]
    pub const fn is_epoll(&self) -> bool {
        matches!(self, Self::Epoll)
    }

    /// Returns `true` if this kind represents a signalfd descriptor.
    #[must_use]
    pub const fn is_signalfd(&self) -> bool {
        matches!(self, Self::Signalfd)
    }

    /// Returns `true` if this kind represents a timerfd descriptor.
    #[must_use]
    pub const fn is_timerfd(&self) -> bool {
        matches!(self, Self::Timerfd)
    }

    /// Returns `true` if this kind represents a pidfd process descriptor.
    #[must_use]
    pub const fn is_pidfd(&self) -> bool {
        matches!(self, Self::Pidfd)
    }

    /// Returns `true` if this kind represents an anonymous memory file (`memfd`).
    #[must_use]
    pub const fn is_memfd(&self) -> bool {
        matches!(self, Self::Memfd)
    }

    /// Returns `true` if this kind represents an unclassified target.
    #[must_use]
    pub const fn is_other(&self) -> bool {
        matches!(self, Self::Other)
    }
}

impl fmt::Display for OpenFileKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
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
    /// Whether the file was unlinked from disk while held by this descriptor (`(deleted)`).
    #[serde(default)]
    pub deleted: bool,
}

/// Convenience alias for [`OpenFileEntry`] consistent with `ProcessItem` / `ServiceItem` naming conventions.
pub type OpenFileItem = OpenFileEntry;

impl OpenFileEntry {
    /// Creates a new open file entry.
    #[must_use]
    pub const fn new(fd: u32, kind: OpenFileKind, target: Option<String>, deleted: bool) -> Self {
        Self {
            fd,
            kind,
            target,
            deleted,
        }
    }

    /// Returns `true` if this descriptor represents a socket.
    ///
    /// Recognizes sockets by either their classified [`OpenFileKind::Socket`]
    /// or a verbatim target prefix matching `socket:[`.
    #[must_use]
    pub fn is_socket(&self) -> bool {
        self.kind.is_socket()
            || self
                .target
                .as_deref()
                .is_some_and(|t| t.starts_with("socket:"))
    }

    /// Returns `true` if this descriptor represents a pipe (FIFO or anonymous pipe).
    ///
    /// Recognizes pipes by either their classified [`OpenFileKind::Pipe`]
    /// or a verbatim target prefix matching `pipe:[` or `FIFO:`.
    #[must_use]
    pub fn is_pipe(&self) -> bool {
        self.kind.is_pipe()
            || self
                .target
                .as_deref()
                .is_some_and(|t| t.starts_with("pipe:") || t.starts_with("FIFO:"))
    }

    /// Returns `true` if this descriptor represents an eventfd notification primitive.
    ///
    /// Recognizes eventfds by either their classified [`OpenFileKind::Eventfd`]
    /// or an anonymous inode target matching `anon_inode:[eventfd]` or `anon_inode:eventfd`.
    #[must_use]
    pub fn is_eventfd(&self) -> bool {
        self.kind.is_eventfd()
            || self.target.as_deref().is_some_and(|t| {
                t == "anon_inode:[eventfd]"
                    || t == "anon_inode:eventfd"
                    || t.starts_with("anon_inode:[eventfd]")
                    || t.starts_with("anon_inode:eventfd")
            })
    }

    /// Returns `true` if this descriptor represents a regular file on disk.
    ///
    /// Returns `true` when [`OpenFileKind::File`] is present and the descriptor is
    /// neither a socket, pipe, nor eventfd.
    #[must_use]
    pub fn is_regular_file(&self) -> bool {
        if self.is_socket() || self.is_pipe() || self.is_eventfd() {
            return false;
        }
        self.kind.is_regular_file()
    }

    /// Alias for [`Self::is_regular_file`].
    #[must_use]
    pub fn is_file(&self) -> bool {
        self.is_regular_file()
    }

    /// Returns `true` if this descriptor represents a directory.
    #[must_use]
    pub fn is_directory(&self) -> bool {
        self.kind.is_directory()
    }

    /// Returns `true` if this descriptor represents a character or block device.
    #[must_use]
    pub fn is_device(&self) -> bool {
        self.kind.is_device()
    }

    /// Returns `true` if this descriptor represents an epoll instance.
    #[must_use]
    pub fn is_epoll(&self) -> bool {
        self.kind.is_epoll()
            || self.target.as_deref().is_some_and(|t| {
                t.starts_with("anon_inode:[eventpoll]") || t.starts_with("anon_inode:eventpoll")
            })
    }

    /// Returns `true` if this descriptor represents a signalfd descriptor.
    #[must_use]
    pub fn is_signalfd(&self) -> bool {
        self.kind.is_signalfd()
            || self.target.as_deref().is_some_and(|t| {
                t.starts_with("anon_inode:[signalfd]") || t.starts_with("anon_inode:signalfd")
            })
    }

    /// Returns `true` if this descriptor represents a timerfd descriptor.
    #[must_use]
    pub fn is_timerfd(&self) -> bool {
        self.kind.is_timerfd()
            || self.target.as_deref().is_some_and(|t| {
                t.starts_with("anon_inode:[timerfd]") || t.starts_with("anon_inode:timerfd")
            })
    }

    /// Returns `true` if this descriptor represents a pidfd process descriptor.
    #[must_use]
    pub fn is_pidfd(&self) -> bool {
        self.kind.is_pidfd()
            || self.target.as_deref().is_some_and(|t| {
                t.starts_with("anon_inode:[pidfd]") || t.starts_with("anon_inode:pidfd")
            })
    }

    /// Returns `true` if this descriptor represents an anonymous memory file (`memfd`).
    #[must_use]
    pub fn is_memfd(&self) -> bool {
        self.kind.is_memfd()
            || self
                .target
                .as_deref()
                .is_some_and(|t| t.starts_with("memfd:") || t.starts_with("anon_inode:memfd"))
    }

    /// Resolves the fine-grained [`OpenFileKind`], promoting anonymous kernel primitives
    /// (such as `anon_inode:[eventfd]`) that may have been coarsely collected as
    /// [`OpenFileKind::Other`].
    #[must_use]
    pub fn resolved_kind(&self) -> OpenFileKind {
        if self.is_socket() {
            OpenFileKind::Socket
        } else if self.is_pipe() {
            OpenFileKind::Pipe
        } else if self.is_eventfd() {
            OpenFileKind::Eventfd
        } else if self.is_epoll() {
            OpenFileKind::Epoll
        } else if self.is_signalfd() {
            OpenFileKind::Signalfd
        } else if self.is_timerfd() {
            OpenFileKind::Timerfd
        } else if self.is_pidfd() {
            OpenFileKind::Pidfd
        } else if self.is_memfd() {
            OpenFileKind::Memfd
        } else {
            self.kind
        }
    }

    /// Returns the target string or `fallback` if unreadable.
    #[must_use]
    pub fn target_or<'a>(&'a self, fallback: &'a str) -> &'a str {
        self.target.as_deref().unwrap_or(fallback)
    }
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

#[cfg(test)]
#[path = "../../../tests/headless/core_core_process_telemetry_open_files_tests.rs"]
mod tests;
