//! A SHORT directory for a Unix-domain socket path.
//!
//! `sockaddr_un`'s `sun_path` is 108 bytes. The local quality gate exports
//! its own long `TMPDIR` (the gate's scratch root inside the repository),
//! so a temp dir under it pushes a socket path past that limit and `bind`
//! fails with "path must be shorter than SUN_LEN" — a failure that depends
//! on WHERE the repository lives, not on the behavior under test. The CI
//! checkout path is long as well, so the same test would fail there.
//!
//! The socket tests therefore anchor their temp dir at the system's short
//! temp root and keep the process temp dir only when that root is itself
//! usable. Nothing about the tested behavior changes: only the location of
//! the throwaway socket moves.

use std::path::PathBuf;

/// The shortest usable directory a Unix socket path can live in.
///
/// The system's `/tmp` is the short root on every Unix host; when it is not
/// a directory, the process temp dir is the fallback.
#[must_use]
pub fn socket_temp_root() -> PathBuf {
    let short = PathBuf::from("/tmp");
    if short.is_dir() {
        return short;
    }
    std::env::temp_dir()
}
