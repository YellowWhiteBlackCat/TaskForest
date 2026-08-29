//! Pure process-details action projection.

use taskmanager_core::core::process::ProcessItem;

/// Executable path suitable for the clipboard action. Missing and non-UTF-8
/// observations remain absent instead of becoming an empty button payload.
#[must_use]
pub(super) fn executable_path(process: Option<&ProcessItem>) -> Option<String> {
    process?
        .current_exe_path()?
        .to_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}
