//! Provider-neutral startup impact and boot evidence.

use serde::{Deserialize, Serialize};

use crate::core::device_state::DeviceState;

use super::timeline::StartupTimelineSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StartupImpactEvidence {
    Measured { duration_ms: u64 },
    Unknown { reason: StartupImpactUnknownReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StartupImpactUnknownReason {
    NotInstrumented,
    NoRecordForThisBoot,
    ProviderUnavailable,
    TimedOut,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartupFailedUnit {
    pub unit: String,
    pub load_state: String,
    pub active_state: String,
    pub sub_state: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartupCriticalChainNode {
    pub unit: String,
    pub activated_at_ms: Option<u64>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupEvidenceFailure {
    MissingTool,
    PermissionDenied,
    TimedOut,
    Unavailable,
    Unsupported,
}

/// The boot evidence of one machine.
///
/// Every source inside the snapshot keeps its own typed state and typed
/// failure, so one unavailable source never hides the others and never turns
/// into a fabricated value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StartupBootEvidenceSnapshot {
    pub state: DeviceState,
    pub failed_units_state: DeviceState,
    pub critical_chain_state: DeviceState,
    pub failed_units_failure: Option<StartupEvidenceFailure>,
    pub critical_chain_failure: Option<StartupEvidenceFailure>,
    pub failed_units: Vec<StartupFailedUnit>,
    pub critical_chain: Vec<StartupCriticalChainNode>,
    /// Typed state of the per-phase boot-duration source.
    ///
    /// This source is independent of the failed-unit and critical-chain
    /// sources: a platform may publish a phase split while one of the other
    /// sources is unavailable, and vice versa.
    #[serde(default)]
    pub phase_timeline_state: DeviceState,
    /// Typed failure of the per-phase boot-duration source.
    ///
    /// `None` = the source was read (or the platform publishes no phase
    /// split at all).
    #[serde(default)]
    pub phase_timeline_failure: Option<StartupEvidenceFailure>,
    /// The measured boot-phase split (firmware / loader / kernel / user space).
    ///
    /// `None` = this boot published no phase split. A phase the source did
    /// not measure stays absent inside the snapshot — never zero.
    #[serde(default)]
    pub phase_timeline: Option<StartupTimelineSnapshot>,
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_startup_phase_evidence_tests.rs"]
mod phase_tests;
