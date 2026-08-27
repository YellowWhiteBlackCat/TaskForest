//! Linux selection of the user configuration path.

use std::path::PathBuf;

/// Resolve the TaskForest config path using Linux desktop conventions.
///
/// XDG_CONFIG_HOME wins when set and non-empty. HOME/.config is the fallback;
/// a relative filename keeps startup non-fatal in restricted environments
/// without either variable.
#[must_use]
pub fn user_config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .map(|home| home.join(".config"))
        });
    base.map_or_else(
        || PathBuf::from("taskmanager-config.json"),
        |directory| directory.join("taskmanager").join("config.json"),
    )
}

/// Resolve the persistent telemetry-history directory (roadmap #4, ADR-028)
/// using Linux desktop conventions: `XDG_DATA_HOME`, else `HOME/.local/share`,
/// else a relative directory so a restricted environment stays non-fatal.
/// The store itself only runs when the user opted in via config.
#[must_use]
pub fn user_history_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .map(|home| home.join(".local").join("share"))
        });
    base.map_or_else(
        || PathBuf::from("taskmanager-history"),
        |directory| directory.join("taskmanager").join("history"),
    )
}

/// Number of collector ticks between full `/proc/<pid>/fd` directory scans.
///
/// fd count is a low-frequency-drift column — a process rarely opens or closes
/// many fds within a second — so a full `read_dir("/proc/<pid>/fd")` every
/// ~5 ticks (≈1 s at the default ~200 ms cadence) bounds the per-process
/// syscall cost. Intermediate ticks reuse the previous value via
/// `retain_previous`, and the first tick after a pid appears always reads fd
/// so a value is established before any deferral.
pub(crate) const FD_COUNT_REFRESH_EVERY_N_TICKS: u32 = 5;

#[cfg(test)]
#[path = "../tests/headless/linux_config_tests.rs"]
mod tests;
