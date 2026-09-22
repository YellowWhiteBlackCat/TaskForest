//! Per-thread breakdown of a process as a typed insight facet.

use serde::{Deserialize, Serialize};

use crate::core::device_state::DeviceState;

/// Kernel scheduler state of a single thread, parsed from field 3 of
/// `/proc/<pid>/task/<tid>/stat`.
///
/// Only the widely-deployed Linux task-state characters are enumerated;
/// unrecognized characters map to [`ThreadState::Other`] so display never
/// fabricates a state procfs did not report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadState {
    /// `R` — runnable (on or awaiting a CPU).
    Running,
    /// `S` — interruptible sleep.
    Sleep,
    /// `D` — uninterruptible disk sleep.
    UninterruptibleSleep,
    /// `T` — stopped by a job-control signal.
    Stopped,
    /// `t` — stopped by the tracer (debugger).
    Traced,
    /// `Z` — zombie.
    Zombie,
    /// `P` — paging (rare on modern kernels).
    Paging,
    /// `X` — dead.
    Dead,
    /// `I` — idle kernel thread.
    Idle,
    /// Any unrecognized state character.
    Other,
}

impl ThreadState {
    /// Parse the single-character state field (`/proc/<pid>/task/<tid>/stat`
    /// field 3) into a typed value. Unknown characters map to [`Self::Other`].
    #[must_use]
    pub fn from_char(character: char) -> Self {
        match character {
            'R' => Self::Running,
            'S' => Self::Sleep,
            'D' => Self::UninterruptibleSleep,
            'T' => Self::Stopped,
            't' => Self::Traced,
            'Z' => Self::Zombie,
            'P' => Self::Paging,
            'X' => Self::Dead,
            'I' => Self::Idle,
            _ => Self::Other,
        }
    }

    /// Canonical short label for display, mirroring `ps`/`htop` letters.
    #[must_use]
    pub const fn as_short_label(self) -> &'static str {
        match self {
            Self::Running => "R",
            Self::Sleep => "S",
            Self::UninterruptibleSleep => "D",
            Self::Stopped => "T",
            Self::Traced => "t",
            Self::Zombie => "Z",
            Self::Paging => "P",
            Self::Dead => "X",
            Self::Idle => "I",
            Self::Other => "?",
        }
    }
}

/// Best-effort classification of the kernel wait observed for a thread.
///
/// Linux exposes a wait-channel symbol and scheduler run-queue delay, but it
/// does not expose a portable "time blocked on this futex" counter through
/// the unprivileged procfs API. Keeping the class and measured run-queue
/// duration separate prevents a scheduler wait from being misreported as a
/// futex duration while still making D-state and lock-shaped wait channels
/// visible to all frontends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadWaitKind {
    /// The wait channel names a futex/semaphore/mutex-style kernel lock.
    KernelLock,
    /// The thread is in uninterruptible sleep, commonly waiting for I/O.
    UninterruptibleIo,
    /// The scheduler reported non-zero time waiting on a run queue.
    RunQueue,
    /// A provider supplied a wait channel that cannot be classified further.
    Other,
}

impl ThreadWaitKind {
    /// Infer a display class from the scheduler state, wait channel and
    /// measured run-queue delay. `None` means there is no wait evidence.
    #[must_use]
    pub fn from_observation(
        state: ThreadState,
        wchan: Option<&str>,
        run_queue_wait_ns: Option<u64>,
    ) -> Option<Self> {
        if matches!(state, ThreadState::UninterruptibleSleep) {
            return Some(Self::UninterruptibleIo);
        }
        if wchan.is_some_and(is_kernel_lock_wait_channel) {
            return Some(Self::KernelLock);
        }
        if run_queue_wait_ns.is_some_and(|wait| wait > 0) {
            return Some(Self::RunQueue);
        }
        wchan
            .filter(|value| !value.is_empty() && *value != "0")
            .map(|_| Self::Other)
    }

    /// Stable compact label for terminal and dense desktop readouts.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::KernelLock => "lock",
            Self::UninterruptibleIo => "io",
            Self::RunQueue => "runq",
            Self::Other => "wait",
        }
    }
}

fn is_kernel_lock_wait_channel(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    ["futex", "mutex", "semaphore", "sem_wait", "rwsem", "lock"]
        .iter()
        .any(|needle| value.contains(needle))
}

/// One thread of a process.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessThreadInfo {
    /// Thread (kernel task) id.
    pub tid: u32,
    /// Field 2 of `stat`: the executable name of the thread. May contain
    /// spaces and is wrapped in parentheses in the raw file.
    pub comm: String,
    /// Field 3 of `stat`: scheduler state.
    pub state: ThreadState,
    /// Cumulative CPU time in seconds (`utime + stime` converted with the
    /// provider's reported statistics-clock frequency). `None` when the
    /// counters or clock frequency are unavailable.
    pub cpu_time_secs: Option<f64>,
    /// Instantaneous CPU utilization for this thread, measured against one
    /// logical CPU between two identity-bound samples. The first sample and
    /// any counter/timestamp gap are `None`; a missing rate is never rendered
    /// as a believable zero.
    #[serde(default)]
    pub cpu_percent: Option<f32>,
    /// Kernel wait channel symbol (e.g. `futex_wait_queue_me`, `ep_poll`, `do_select`).
    #[serde(default)]
    pub wchan: Option<String>,
    /// Scheduler run-queue delay from `/proc/<pid>/task/<tid>/schedstat`, in
    /// nanoseconds. This is not a futex/D-state sleep duration; the wait kind
    /// records that distinction explicitly.
    #[serde(default)]
    pub run_queue_wait_ns: Option<u64>,
    /// Typed classification of the observed wait channel/state.
    #[serde(default)]
    pub wait_kind: Option<ThreadWaitKind>,
}

/// The per-thread facet.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProcessThreads {
    /// Aggregated collection state.
    pub state: DeviceState,
    /// Threads ordered by ascending tid.
    pub threads: Vec<ProcessThreadInfo>,
}
