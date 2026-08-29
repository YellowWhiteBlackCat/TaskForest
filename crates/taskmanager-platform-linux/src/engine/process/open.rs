//! Linux "open the folder containing a path" helper for the process
//! "Open file location" context-menu action (Win11 Task Manager parity).
//!
//! Resolves the parent directory of the given executable path and spawns the
//! configured file manager through `xdg-open`. The spawn is fire-and-forget:
//! the launched file manager outlives this call, and a one-shot reaper thread
//! waits for its exit so it cannot linger as a zombie, mirroring
//! [`super::ProcessManager::xdg_open`].
//!
//! The equivalent macOS/Windows behavior belongs to those platform adapters;
//! this crate does not carry dormant SKU branches for them.

use std::path::{Path, PathBuf};

use tracing::info;

/// Resolve the absolute executable path for `pid` by following procfs. Missing
/// links, kernel threads and permission failures remain `None`.
pub fn read_exe_path(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/exe")).ok()
}

/// Resolve the parent directory of `path` as an owned [`PathBuf`], or `None`
/// when `path` has no parent (e.g. a bare filename like `kworker` for a kernel
/// thread, or `/` itself). Pure — no syscall, no spawn — so it is unit-tested
/// directly (the spawn itself is intentionally NOT tested; it launches a real
/// file-manager process that has no place in the test harness).
pub fn parent_dir(path: &Path) -> Option<PathBuf> {
    path.parent().map(Path::to_path_buf)
}

/// Open the platform file manager at the directory containing `exe`.
///
/// Computes [`parent_dir`] and spawns the OS file manager on it. Returns `Err`
/// with a human-readable message when the parent can't be resolved (kernel
/// thread / bare name) OR the file manager fails to spawn (not installed, PATH
/// missing, etc.). NO `unsafe`: spawning a child is exposed by
/// [`std::process::Command`] as a safe API.
pub fn open_file_location(exe: &Path) -> Result<(), String> {
    use std::process::{Command, Stdio};

    let dir =
        parent_dir(exe).ok_or_else(|| format!("no parent directory for \"{}\"", exe.display()))?;

    let program = "xdg-open";
    let child = Command::new(program)
        .arg(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to open file location: {e}"))?;
    let pid = child.id();
    // Fire-and-forget: the launched file manager outlives this call (no
    // kill_on_drop), while the one-shot reaper waits for its exit so it
    // cannot linger as a zombie, mirroring ProcessManager::xdg_open.
    super::control::spawn_reaper(child, format!("file-loc-reap-{pid}"));
    info!("open_file_location \"{}\" via {program}", dir.display());
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_process_open_tests.rs"]
mod tests;
