//! Platform-neutral process scheduling policy representation.

use serde::{Deserialize, Serialize};

/// Neutral process scheduling policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProcessSchedulingPolicy {
    /// Standard round-robin time-sharing (Linux `SCHED_OTHER` / Windows standard priority / macOS standard).
    #[default]
    Other,
    /// Batch-style execution for CPU-intensive background tasks (Linux `SCHED_BATCH`).
    Batch,
    /// Very low priority background processing (Linux `SCHED_IDLE` / Windows `IDLE_PRIORITY_CLASS`).
    Idle,
    /// Real-time First-In First-Out (Linux `SCHED_FIFO` / Windows Realtime).
    Fifo,
    /// Real-time Round-Robin (Linux `SCHED_RR`).
    RoundRobin,
    /// Earliest Deadline First (Linux `SCHED_DEADLINE`).
    Deadline,
    /// Platform-specific or unrecognized policy code.
    Custom(u32),
}

impl ProcessSchedulingPolicy {
    #[must_use]
    pub const fn from_linux_policy(policy: u32) -> Self {
        match policy {
            0 => Self::Other,
            1 => Self::Fifo,
            2 => Self::RoundRobin,
            3 => Self::Batch,
            5 => Self::Idle,
            6 => Self::Deadline,
            other => Self::Custom(other),
        }
    }

    #[must_use]
    pub const fn to_linux_policy(self) -> u32 {
        match self {
            Self::Other => 0,
            Self::Fifo => 1,
            Self::RoundRobin => 2,
            Self::Batch => 3,
            Self::Idle => 5,
            Self::Deadline => 6,
            Self::Custom(other) => other,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Other => "Normal (SCHED_OTHER)",
            Self::Batch => "Batch (SCHED_BATCH)",
            Self::Idle => "Idle (SCHED_IDLE)",
            Self::Fifo => "Realtime (FIFO)",
            Self::RoundRobin => "Realtime (RR)",
            Self::Deadline => "Deadline (SCHED_DEADLINE)",
            Self::Custom(_) => "Custom",
        }
    }

    #[must_use]
    pub const fn is_realtime(self) -> bool {
        matches!(self, Self::Fifo | Self::RoundRobin | Self::Deadline)
    }
}
