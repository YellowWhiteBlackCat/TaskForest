//! Open file descriptors of a process, read from `/proc/<pid>/fd`.
//!
//! Each `fd/N` directory entry is a symlink to the open target. The readlink
//! string is classified into a coarse [`OpenFileKind`] and also preserved
//! verbatim so the caller never loses information. Permission-denied on the
//! descriptor directory and a vanished process produce typed device states
//! rather than panics or fabricated zeros.

use std::path::Path;

use taskmanager_core::core::device_state::DeviceStatus;
use taskmanager_core::{OpenFileEntry, OpenFileKind, ProcessOpenFiles};

use super::{state_for_status, status_from_io_error};

/// Classify a `/proc/<pid>/fd/<n>` readlink target into a coarse kind.
///
/// `socket:[inode]` and `pipe:[inode]` are recognized by their kernel-assigned
/// prefixes; `anon_inode:*` targets are anonymous kernel inodes (eventfd,
/// inotify, signalfd, ...) and are reported as [`OpenFileKind::Other`]
/// because they are neither a path nor a socket/pipe. Every other target
/// (file path, `/dev/null`, `/run/sock`, ...) is [`OpenFileKind::File`].
#[must_use]
pub fn classify_open_file_target(target: &str) -> OpenFileKind {
    if target.starts_with("socket:") {
        OpenFileKind::Socket
    } else if target.starts_with("pipe:") {
        OpenFileKind::Pipe
    } else if target.starts_with("anon_inode:") {
        OpenFileKind::Other
    } else {
        OpenFileKind::File
    }
}

/// Read every readable descriptor of `proc_dir` (`/proc/<pid>`) into a typed
/// open-files facet. Designed to be called from a collector that has already
/// pinned the process start-time token.
pub fn collect_open_files_from_proc_dir(proc_dir: &Path, now_ms: u64) -> ProcessOpenFiles {
    let fd_dir = proc_dir.join("fd");
    let entries = match std::fs::read_dir(&fd_dir) {
        Ok(entries) => entries,
        Err(error) => {
            return ProcessOpenFiles {
                state: state_for_status(status_from_io_error(&error), now_ms),
                ..ProcessOpenFiles::default()
            };
        }
    };

    let mut found = Vec::new();
    let mut unreadable_count = 0_u32;
    for entry in entries.flatten() {
        let Some(fd) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            // Non-numeric fd directory entries are not real descriptors; skip.
            continue;
        };
        match std::fs::read_link(entry.path()) {
            Ok(target) => {
                let target = target.to_string_lossy().to_string();
                found.push(OpenFileEntry {
                    fd,
                    kind: classify_open_file_target(&target),
                    target: Some(target),
                });
            }
            Err(_error) => {
                // A single unreadable descriptor (commonly a privileged fd on a
                // non-root process, or a link that vanished between enumeration
                // and readlink) does not fail the whole facet; it is recorded
                // as an entry with an unknown target and counted as unreadable.
                unreadable_count += 1;
                found.push(OpenFileEntry {
                    fd,
                    kind: OpenFileKind::Other,
                    target: None,
                });
            }
        }
    }

    // Ordering is part of the contract so downstream diffing is stable.
    found.sort_by_key(|entry| entry.fd);

    // The directory itself was readable, so the facet is Healthy even if some
    // individual descriptors were not (`unreadable_count` surfaces that).
    ProcessOpenFiles {
        state: state_for_status(DeviceStatus::Healthy, now_ms),
        entries: found,
        unreadable_count,
    }
}

#[cfg(test)]
#[path = "../../../../tests/headless/linux_engine_process_telemetry_open_files_tests.rs"]
mod tests;
