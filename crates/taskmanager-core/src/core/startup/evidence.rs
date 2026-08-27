//! Provider-neutral startup impact and boot evidence.

use serde::{Deserialize, Serialize};

use crate::core::device_state::DeviceState;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StartupBootEvidenceSnapshot {
    pub state: DeviceState,
    pub failed_units_state: DeviceState,
    pub critical_chain_state: DeviceState,
    pub failed_units_failure: Option<StartupEvidenceFailure>,
    pub critical_chain_failure: Option<StartupEvidenceFailure>,
    pub failed_units: Vec<StartupFailedUnit>,
    pub critical_chain: Vec<StartupCriticalChainNode>,
}
