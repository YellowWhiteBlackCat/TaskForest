//! Process-launch and desktop-opening control methods.

use std::process::Child;

use super::ProcessManager;

/// Hand a fire-and-forget child to a one-shot reaper thread that blocks in
/// `wait()` until the child exits and then ends with it.
///
/// Dropping a [`Child`] without waiting leaves the exited process as a zombie
/// until the parent process itself exits. One bounded thread per launch keeps
/// the reap timely without touching the global `SIGCHLD` disposition, which the
/// portable command runner depends on for its own wait semantics.
pub(super) fn spawn_reaper(mut child: Child, thread_name: String) {
    let _ = std::thread::Builder::new()
        .name(thread_name)
        .spawn(move || {
            let _ = child.wait();
        });
}

impl ProcessManager {
    #[cfg(unix)]
    pub fn run(command: &str) -> Result<u32, String> {
        use std::process::{Command, Stdio};

        let trimmed = command.trim();
        if trimmed.is_empty() {
            return Err("no command given".into());
        }
        let child = Command::new("sh")
            .arg("-c")
            .arg(trimmed)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("failed to launch: {error}"))?;
        let pid = child.id();
        spawn_reaper(child, format!("run-reap-{pid}"));
        tracing::info!("Launched \"{}\" as PID {}", trimmed, pid);
        Ok(pid)
    }

    #[cfg(not(unix))]
    pub fn run(_command: &str) -> Result<u32, String> {
        Err("launching processes is not supported on this platform".into())
    }

    #[cfg(unix)]
    pub fn xdg_open(target: &str) -> Result<(), String> {
        use std::process::{Command, Stdio};

        let child = Command::new("xdg-open")
            .arg(target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("failed to launch xdg-open: {error}"))?;
        let pid = child.id();
        spawn_reaper(child, format!("xdg-open-reap-{pid}"));
        tracing::info!("xdg-open \"{}\"", target);
        Ok(())
    }

    #[cfg(not(unix))]
    pub fn xdg_open(_target: &str) -> Result<(), String> {
        Err("opening URLs is not supported on this platform".into())
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_process_control_tests.rs"]
mod tests;
