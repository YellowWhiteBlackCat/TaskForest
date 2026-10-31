//! Bounded kernel-error log facts.
//!
//! The native adapter owns the `dmesg`/kernel-log read. This module only
//! carries the already-filtered priority and message across the shared
//! projection boundary; a missing permission or source remains `None`.

use serde::{Deserialize, Serialize};

/// Syslog-compatible kernel message priority, from emergency (0) to error
/// (3). The shared contract intentionally stops at error for the health view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KernelLogPriority {
    Emergency,
    Alert,
    Critical,
    Error,
}

impl KernelLogPriority {
    #[must_use]
    pub const fn from_number(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Emergency,
            1 => Self::Alert,
            2 => Self::Critical,
            3 => Self::Error,
            _ => return None,
        })
    }

    #[must_use]
    pub const fn number(self) -> u8 {
        match self {
            Self::Emergency => 0,
            Self::Alert => 1,
            Self::Critical => 2,
            Self::Error => 3,
        }
    }
}

/// One sanitized, bounded kernel error line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelLogEntry {
    /// Kernel monotonic timestamp in seconds when the source exposed it.
    #[serde(default)]
    pub timestamp_seconds: Option<u64>,
    pub priority: KernelLogPriority,
    pub message: String,
}

impl KernelLogEntry {
    /// Keep one message bounded before it reaches a UI or diagnostic export.
    #[must_use]
    pub fn new(priority: KernelLogPriority, message: impl Into<String>) -> Option<Self> {
        let message = message.into();
        let message = message.trim();
        if message.is_empty() {
            return None;
        }
        let message = message.chars().take(512).collect::<String>();
        Some(Self {
            timestamp_seconds: None,
            priority,
            message,
        })
    }
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_kernel_log_tests.rs"]
mod tests;
