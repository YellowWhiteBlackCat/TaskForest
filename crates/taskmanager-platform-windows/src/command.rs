//! Windows injection for the shared bounded command lifecycle.
//!
//! The adapter is already an authorized consumer of the audited Windows API
//! boundary. It atomically assigns a suspended child to a kill-on-close Job
//! before resuming it, then hands only safe owned values to portable code.

use std::process::Command;
use std::time::Duration;

use taskmanager_platform_portable::{BoundedCommandError, BoundedOutput};

#[cfg(windows)]
use taskmanager_platform_portable::{
    BoundedCommandSpawner, OwnedProcessTree, SpawnedCommand, run_with_spawner,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const CREATE_SUSPENDED: u32 = 0x0000_0004;

pub(crate) fn run_with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> Result<BoundedOutput, BoundedCommandError> {
    #[cfg(windows)]
    {
        run_with_spawner(command, timeout, &WindowsCommandSpawner)
    }
    #[cfg(not(windows))]
    {
        taskmanager_platform_portable::run_with_timeout(command, timeout)
    }
}

#[cfg(windows)]
struct WindowsCommandSpawner;

#[cfg(windows)]
impl BoundedCommandSpawner for WindowsCommandSpawner {
    fn spawn(&self, command: &mut Command) -> Result<SpawnedCommand, BoundedCommandError> {
        command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
        let mut child = command.spawn().map_err(BoundedCommandError::Spawn)?;
        let job = match taskmanager_windows_api::assign_and_resume_suspended_process(child.id()) {
            Ok(job) => job,
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(BoundedCommandError::ProcessTree);
            }
        };
        Ok(SpawnedCommand::new(
            child,
            Box::new(WindowsJobTree { job: Some(job) }),
        ))
    }
}

#[cfg(windows)]
struct WindowsJobTree {
    job: Option<taskmanager_windows_api::WindowsProcessJob>,
}

#[cfg(windows)]
impl OwnedProcessTree for WindowsJobTree {
    fn terminate(&mut self) {
        if let Some(job) = self.job.take() {
            let _ = job.terminate();
        }
    }
}
