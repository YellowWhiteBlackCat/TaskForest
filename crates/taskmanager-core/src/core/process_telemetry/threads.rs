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
}

/// The per-thread facet.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProcessThreads {
    /// Aggregated collection state.
    pub state: DeviceState,
    /// Threads ordered by ascending tid.
    pub threads: Vec<ProcessThreadInfo>,
}
