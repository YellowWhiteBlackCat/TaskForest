//! Declared container resource quotas.
//!
//! A container runtime declares the ceilings it enforces for a workload in
//! the control group that owns it. This module owns the *neutral* shape of
//! those declarations so a frontend never has to invent a cap of its own and
//! then report the invented cap as an observation.
//!
//! Honesty contract:
//!
//! - Every dimension is optional, and `None` means "this container declares
//!   no ceiling in that dimension". It never means zero, never a guessed
//!   default, and never a value derived from an unrelated measurement.
//! - A provider that cannot read the declarations reports the reason in
//!   [`ContainerQuota::failure`] and leaves every dimension absent, so
//!   "unknown" can never masquerade as "unlimited".
//!
//! Linux maps the dimensions onto the cgroup-v2 limit files a container
//! runtime writes (`memory.max`, `memory.high`, `cpu.max`); another platform
//! translates the same declarations from its native sandbox description.

use serde::{Deserialize, Serialize};

use crate::core::failure::FailureKind;

use super::resources::LimitValue;

/// The resource ceilings one container declares for its workload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ContainerQuota {
    /// The hard memory ceiling in bytes (`memory.max` on Linux).
    ///
    /// `None` = the container declares no hard memory ceiling.
    pub memory_hard_limit_bytes: Option<u64>,
    /// The soft (throttling) memory ceiling in bytes (`memory.high` on Linux).
    ///
    /// `None` = the container declares no soft memory ceiling.
    pub memory_soft_limit_bytes: Option<u64>,
    /// The declared CPU bandwidth quota (`cpu.max` on Linux).
    ///
    /// `None` = the container declares no CPU quota at all.
    pub cpu: Option<ContainerCpuQuota>,
    /// Why the declarations could not be read.
    ///
    /// `None` means the declarations were read (or the container declares
    /// nothing at all). `Some` means the declarations are UNKNOWN: every
    /// dimension stays absent rather than guessing a ceiling.
    pub failure: Option<FailureKind>,
}

impl ContainerQuota {
    /// A quota in which the container declares no ceiling in any dimension.
    ///
    /// This is the honest shape of a container whose control group carries no
    /// resource controller: "nothing is capped", not "everything is zero".
    #[must_use]
    pub const fn not_declared() -> Self {
        Self {
            memory_hard_limit_bytes: None,
            memory_soft_limit_bytes: None,
            cpu: None,
            failure: None,
        }
    }

    /// A quota whose declarations could not be read.
    ///
    /// The dimensions stay absent: an unreadable declaration is unknown, not
    /// an unlimited ceiling.
    #[must_use]
    pub const fn unavailable(failure: FailureKind) -> Self {
        Self {
            memory_hard_limit_bytes: None,
            memory_soft_limit_bytes: None,
            cpu: None,
            failure: Some(failure),
        }
    }

    /// Whether the container declares at least one ceiling.
    ///
    /// A CPU quota that names no budget declares no ceiling, so it does not
    /// count here either.
    #[must_use]
    pub fn declares_any_ceiling(&self) -> bool {
        self.memory_hard_limit_bytes.is_some()
            || self.memory_soft_limit_bytes.is_some()
            || self.cpu.is_some_and(|cpu| cpu.quota_us.is_some())
    }

    /// Current usage as a percentage of the declared soft memory ceiling.
    ///
    /// `None` when no soft ceiling is declared, when the declarations are
    /// unknown, or when the current usage is unknown. A usage above the
    /// ceiling is retained so an actual overrun stays visible.
    #[must_use]
    pub fn soft_memory_usage_percent(&self, current_bytes: Option<u64>) -> Option<f32> {
        let limit = self.memory_soft_limit_bytes?;
        LimitValue::Value(limit).usage_percent(current_bytes)
    }
}

/// The CPU bandwidth quota one container declares.
///
/// The quota is expressed the way a control group expresses it: a CPU-time
/// budget granted within a fixed period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerCpuQuota {
    /// The CPU time granted within one period, in microseconds.
    ///
    /// `None` = the container declares no CPU ceiling (the declaration exists
    /// but names no budget), while the period is still reported.
    pub quota_us: Option<u64>,
    /// The period the quota is measured over, in microseconds.
    pub period_us: u64,
}

impl ContainerCpuQuota {
    /// The quota expressed in whole CPUs.
    ///
    /// `None` when the container declares no CPU ceiling, or when the period is
    /// zero (an unusable measurement window, not a zero-CPU quota).
    #[must_use]
    pub fn cpus(&self) -> Option<f64> {
        let quota_us = self.quota_us?;
        if self.period_us == 0 {
            return None;
        }
        Some(quota_us as f64 / self.period_us as f64)
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_process_telemetry_quota_tests.rs"]
mod tests;
