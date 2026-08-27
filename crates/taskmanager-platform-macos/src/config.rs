//! macOS selection of the user configuration path.

use std::path::PathBuf;

/// Resolve the TaskForest configuration path using the per-user macOS
/// Application Support directory.
#[must_use]
pub fn user_config_path() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|home| {
            home.join("Library")
                .join("Application Support")
                .join("TaskForest")
                .join("config.json")
        })
        .unwrap_or_else(|| PathBuf::from("taskmanager-config.json"))
}

/// Resolve the persistent telemetry-history directory (roadmap #4, ADR-028)
/// using macOS conventions: `~/Library/Application Support/TaskForest/history`,
/// else a relative directory so a restricted environment stays non-fatal.
#[must_use]
pub fn user_history_dir() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|home| {
            home.join("Library")
                .join("Application Support")
                .join("TaskForest")
                .join("history")
        })
        .unwrap_or_else(|| PathBuf::from("taskmanager-history"))
}

#[cfg(test)]
#[path = "../tests/headless/macos_config.rs"]
mod tests;
