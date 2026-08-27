//! Platform-neutral SMART self-test contracts.

use serde::{Deserialize, Serialize};

use crate::core::device_state::DeviceState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartSelfTestKind {
    Short,
    Extended,
    Conveyance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SmartSelfTestPhase {
    #[default]
    Idle,
    Running,
    Completed,
    Aborted,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartSelfTestFailure {
    InvalidDevice,
    MissingTool,
    RequiresEscalation,
    PermissionDenied,
    TimedOut,
    ProviderUnavailable,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SmartSelfTestReport {
    pub state: DeviceState,
    pub phase: SmartSelfTestPhase,
    pub kind: Option<SmartSelfTestKind>,
    pub progress_pct: Option<f32>,
    pub lifetime_hours: Option<u64>,
    pub first_error_lba: Option<u64>,
    pub failure: Option<SmartSelfTestFailure>,
}
