//! Platform-neutral SMART self-test contracts.

use serde::{Deserialize, Serialize};

use crate::core::device_state::DeviceState;

/// Evidence-backed availability of one SMART self-test kind on a device.
///
/// `Unknown` is used when the provider has not observed a device capability
/// field. A SMART health reading, protocol name, or installed command is not
/// evidence that a self-test kind is supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SmartSelfTestSupport {
    /// No device-specific evidence has been collected for this test kind.
    #[default]
    Unknown,
    /// The device explicitly reports support for this test kind.
    Supported,
    /// The device explicitly reports that this test kind is not supported.
    Unsupported,
}

/// Device-reported support for each user-visible SMART self-test kind.
///
/// This fact is separate from SMART health availability and from the current
/// process's authorization to start a test. On a disk projection it belongs to
/// that row's `(device_id, device_generation)` lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SmartSelfTestCapabilities {
    #[serde(default)]
    pub short: SmartSelfTestSupport,
    #[serde(default)]
    pub extended: SmartSelfTestSupport,
    #[serde(default)]
    pub conveyance: SmartSelfTestSupport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartSelfTestKind {
    Short,
    Extended,
    Conveyance,
}

impl SmartSelfTestCapabilities {
    /// Return the device evidence for one requested test kind.
    #[must_use]
    pub const fn support_for(self, kind: SmartSelfTestKind) -> SmartSelfTestSupport {
        match kind {
            SmartSelfTestKind::Short => self.short,
            SmartSelfTestKind::Extended => self.extended,
            SmartSelfTestKind::Conveyance => self.conveyance,
        }
    }
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
    SupportUnknown,
    Unsupported,
    MissingTool,
    RequiresEscalation,
    PermissionDenied,
    AuthorizationUnavailable,
    HelperUnavailable,
    HelperProtocolViolation,
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
