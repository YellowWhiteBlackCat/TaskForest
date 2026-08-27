//! Process control operations: sending signals, adjusting nice priority,
//! and getting/setting CPU affinity.

#[cfg(unix)]
use nix::sys::signal::{Signal, kill};
#[cfg(unix)]
use nix::unistd::Pid;

use taskmanager_core::FailureKind;
use tracing::{info, warn};

use super::ProcessManager;

impl ProcessManager {
    // ── process control: Unix (real nix/rustix syscalls) ───────────────────

    /// Sends a Unix signal to a process identified by PID.
    #[cfg(unix)]
    pub fn send_signal(pid: u32, sig: Signal) -> Result<(), FailureKind> {
        info!("Sending signal {:?} to process PID {}", sig, pid);
        let nix_pid = Pid::from_raw(validated_raw_pid(pid)?);
        kill(nix_pid, sig).map_err(|err| {
            warn!("Failed to send signal to PID {}: {}", pid, err);
            classify_nix_process_errno(err)
        })
    }

    /// Terminates a process gracefully via SIGTERM.
    #[cfg(unix)]
    pub fn terminate_process(pid: u32) -> Result<(), FailureKind> {
        Self::send_signal(pid, Signal::SIGTERM)
    }

    /// Forcefully kills a process via SIGKILL.
    #[cfg(unix)]
    pub fn kill_process(pid: u32) -> Result<(), FailureKind> {
        Self::send_signal(pid, Signal::SIGKILL)
    }

    /// Pauses a process via SIGSTOP.
    #[cfg(unix)]
    pub fn pause_process(pid: u32) -> Result<(), FailureKind> {
        Self::send_signal(pid, Signal::SIGSTOP)
    }

    /// Resumes a paused process via SIGCONT.
    #[cfg(unix)]
    pub fn resume_process(pid: u32) -> Result<(), FailureKind> {
        Self::send_signal(pid, Signal::SIGCONT)
    }

    /// Sets the nice (priority) value of a process. Lower values = higher priority.
    #[cfg(unix)]
    pub fn set_process_nice(pid: u32, nice_val: i32) -> Result<(), FailureKind> {
        info!("Setting nice value {} for process PID {}", nice_val, pid);
        let rustix_pid = rustix::process::Pid::from_raw(validated_raw_pid(pid)?);
        rustix::process::setpriority_process(rustix_pid, nice_val).map_err(|err| {
            warn!("Failed to set nice value for PID {}: {}", pid, err);
            classify_rustix_process_errno(err)
        })
    }

    /// Returns the CPUs the process is allowed to run on, via
    /// `sched_getaffinity(2)`. Failure is typed at the syscall boundary; an
    /// empty successful mask remains distinct from unavailable observation.
    #[cfg(unix)]
    pub fn get_process_affinity(pid: u32) -> Result<Vec<u32>, FailureKind> {
        let rustix_pid = rustix::process::Pid::from_raw(validated_raw_pid(pid)?);
        rustix::thread::sched_getaffinity(rustix_pid)
            .map(|set| cpuset_to_cpus(&set))
            .map_err(|err| {
                warn!("Failed to get CPU affinity for PID {}: {}", pid, err);
                classify_rustix_process_errno(err)
            })
    }

    /// Sets the process's CPU affinity mask via `sched_setaffinity(2)`. CPU ids must be
    /// `< CpuSet::MAX_CPU`; out-of-range ids are rejected before the syscall. Mirrors
    /// `set_process_nice`.
    #[cfg(unix)]
    pub fn set_process_affinity(pid: u32, cpus: &[u32]) -> Result<(), FailureKind> {
        info!("Setting CPU affinity for PID {} to {:?} cores", pid, cpus);
        let set = cpus_to_cpuset(cpus)?;
        let rustix_pid = rustix::process::Pid::from_raw(validated_raw_pid(pid)?);
        rustix::thread::sched_setaffinity(rustix_pid, &set).map_err(|err| {
            warn!("Failed to set CPU affinity for PID {}: {}", pid, err);
            classify_rustix_process_errno(err)
        })
    }

    // ── process control: non-Unix stubs (identical signatures; unsupported) ──

    /// Non-Unix stub: sending signals is unsupported. The parameter takes the
    /// platform-neutral [`taskmanager_core::ProcessSignal`] because nix's
    /// `Signal` type does not exist off Unix.
    #[cfg(not(unix))]
    pub fn send_signal(
        _pid: u32,
        _sig: taskmanager_core::ProcessSignal,
    ) -> Result<(), FailureKind> {
        Err(FailureKind::Unsupported)
    }
    /// Non-Unix stub: terminating a process is unsupported.
    #[cfg(not(unix))]
    pub fn terminate_process(_pid: u32) -> Result<(), FailureKind> {
        Err(FailureKind::Unsupported)
    }
    /// Non-Unix stub: killing a process is unsupported.
    #[cfg(not(unix))]
    pub fn kill_process(_pid: u32) -> Result<(), FailureKind> {
        Err(FailureKind::Unsupported)
    }
    /// Non-Unix stub: pausing a process is unsupported.
    #[cfg(not(unix))]
    pub fn pause_process(_pid: u32) -> Result<(), FailureKind> {
        Err(FailureKind::Unsupported)
    }
    /// Non-Unix stub: resuming a process is unsupported.
    #[cfg(not(unix))]
    pub fn resume_process(_pid: u32) -> Result<(), FailureKind> {
        Err(FailureKind::Unsupported)
    }
    /// Non-Unix stub: setting process priority is unsupported.
    #[cfg(not(unix))]
    pub fn set_process_nice(_pid: u32, _nice_val: i32) -> Result<(), FailureKind> {
        Err(FailureKind::Unsupported)
    }
    /// Non-Unix stub: affinity query is unsupported.
    #[cfg(not(unix))]
    pub fn get_process_affinity(_pid: u32) -> Result<Vec<u32>, FailureKind> {
        Err(FailureKind::Unsupported)
    }
    /// Non-Unix stub: affinity setting is unsupported.
    #[cfg(not(unix))]
    pub fn set_process_affinity(_pid: u32, _cpus: &[u32]) -> Result<(), FailureKind> {
        Err(FailureKind::Unsupported)
    }
}

// ── free-function aliases (convenience for callers that don't want ProcessManager:: prefix) ──

/// Sends SIGTERM to a process (free-function alias).
#[cfg(feature = "test-support")]
pub fn terminate_process(pid: u32) -> Result<(), FailureKind> {
    ProcessManager::terminate_process(pid)
}

/// Sends SIGKILL to a process (free-function alias).
#[cfg(feature = "test-support")]
pub fn kill_process(pid: u32) -> Result<(), FailureKind> {
    ProcessManager::kill_process(pid)
}

/// Sends SIGSTOP to a process (free-function alias).
#[cfg(feature = "test-support")]
pub fn pause_process(pid: u32) -> Result<(), FailureKind> {
    ProcessManager::pause_process(pid)
}

/// Sends SIGCONT to a process (free-function alias).
#[cfg(feature = "test-support")]
pub fn resume_process(pid: u32) -> Result<(), FailureKind> {
    ProcessManager::resume_process(pid)
}

// ── CPU affinity helpers ───────────────────────────────────────────────────

/// Build a rustix `CpuSet` from a list of CPU ids. Rejects ids `>= CpuSet::MAX_CPU`
/// before they reach the kernel. Pure (no syscall); unit-tested via round-trip.
#[cfg(unix)]
pub(crate) fn cpus_to_cpuset(cpus: &[u32]) -> Result<rustix::thread::CpuSet, FailureKind> {
    let mut set = rustix::thread::CpuSet::new();
    let max = rustix::thread::CpuSet::MAX_CPU;
    for &cpu in cpus {
        let cpu = usize::try_from(cpu).map_err(|_| FailureKind::Rejected)?;
        if cpu >= max {
            return Err(FailureKind::Rejected);
        }
        set.set(cpu);
    }
    Ok(set)
}

fn validated_raw_pid(pid: u32) -> Result<i32, FailureKind> {
    i32::try_from(pid)
        .ok()
        .filter(|raw_pid| *raw_pid > 0)
        .ok_or(FailureKind::Rejected)
}

#[cfg(unix)]
pub(crate) const fn classify_nix_process_errno(error: nix::errno::Errno) -> FailureKind {
    match error {
        nix::errno::Errno::EACCES | nix::errno::Errno::EPERM => FailureKind::PermissionDenied,
        nix::errno::Errno::ESRCH | nix::errno::Errno::ENOENT => FailureKind::IdentityChanged,
        nix::errno::Errno::ENOSYS | nix::errno::Errno::EOPNOTSUPP => FailureKind::Unsupported,
        nix::errno::Errno::ETIMEDOUT => FailureKind::TimedOut,
        nix::errno::Errno::EAGAIN | nix::errno::Errno::EINTR | nix::errno::Errno::EBUSY => {
            FailureKind::TemporarilyUnavailable
        }
        nix::errno::Errno::EINVAL => FailureKind::Rejected,
        _ => FailureKind::ProviderFault,
    }
}

#[cfg(unix)]
pub(crate) const fn classify_rustix_process_errno(error: rustix::io::Errno) -> FailureKind {
    match error {
        rustix::io::Errno::ACCESS | rustix::io::Errno::PERM => FailureKind::PermissionDenied,
        rustix::io::Errno::SRCH | rustix::io::Errno::NOENT => FailureKind::IdentityChanged,
        rustix::io::Errno::NOSYS | rustix::io::Errno::NOTSUP => FailureKind::Unsupported,
        rustix::io::Errno::TIMEDOUT => FailureKind::TimedOut,
        rustix::io::Errno::AGAIN | rustix::io::Errno::INTR | rustix::io::Errno::BUSY => {
            FailureKind::TemporarilyUnavailable
        }
        rustix::io::Errno::INVAL => FailureKind::Rejected,
        _ => FailureKind::ProviderFault,
    }
}

/// Read the set CPUs out of a rustix `CpuSet` as a sorted `Vec<u32>`. Pure.
#[cfg(unix)]
pub(crate) fn cpuset_to_cpus(set: &rustix::thread::CpuSet) -> Vec<u32> {
    (0..rustix::thread::CpuSet::MAX_CPU)
        .filter(|i| set.is_set(*i))
        .filter_map(|i| u32::try_from(i).ok())
        .collect()
}

#[cfg(unix)]
#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_process_signal_tests.rs"]
mod tests;
