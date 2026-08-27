//! Platform-neutral storage SMART contracts.

use serde::{Deserialize, Serialize};

use crate::core::ProviderId;
use crate::core::device_state::{DeviceState, DeviceStatus};
use crate::core::metrics::SmartAvailability;

pub mod self_test;
pub use self_test::{
    SmartSelfTestFailure, SmartSelfTestKind, SmartSelfTestPhase, SmartSelfTestReport,
};

/// Precise reason why a selected SMART protocol provider could not produce a
/// trustworthy observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartProviderFailureKind {
    UnsupportedProtocol,
    BridgeLimitation,
    MissingTool,
    PermissionDenied,
    TimedOut,
    MalformedResponse,
    DeviceUnavailable,
    CommandFailed,
    TemporarilyUnavailable,
}

/// A single parsed ATA/SATA SMART attribute row from the transport's
/// attribute table. Only the failure-relevant fields are kept: the attribute
/// `id`, its raw counter, and whether the source reports it as
/// failing now. IDs 5 (reallocated sectors), 197 (current pending), 198
/// (offline uncorrectable), and 199 (command timeout) are the actionable
/// precursors to disk failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtaSmartAttribute {
    pub id: u16,
    pub raw_value: u64,
    pub failing_now: bool,
}

#[derive(Debug, Clone)]
pub struct DiskSmart {
    pub availability: SmartAvailability,
    pub state: DeviceState,
    pub provider: Option<ProviderId>,
    pub failure: Option<SmartProviderFailureKind>,
    pub temperature_c: Option<f32>,
    pub critical_warning: Option<bool>,
    pub temp_critical_c: Option<f32>,
    pub percent_used: Option<f32>,
    pub power_on_hours: Option<u64>,
    /// Typed ATA/SATA SMART attribute table. `None` when the device does not
    /// expose ATA attributes (NVMe / SCSI / SAS) or the transport omitted the table.
    pub ata_attributes: Option<Vec<AtaSmartAttribute>>,
}

impl DiskSmart {
    #[must_use]
    pub fn with_availability(availability: SmartAvailability) -> Self {
        let failure = match availability {
            SmartAvailability::Available => None,
            SmartAvailability::Unsupported => Some(SmartProviderFailureKind::UnsupportedProtocol),
            SmartAvailability::Unavailable => {
                Some(SmartProviderFailureKind::TemporarilyUnavailable)
            }
            SmartAvailability::MissingTool => Some(SmartProviderFailureKind::MissingTool),
            SmartAvailability::PermissionDenied => Some(SmartProviderFailureKind::PermissionDenied),
        };
        Self {
            availability,
            state: DeviceState {
                status: match availability {
                    SmartAvailability::Available => DeviceStatus::Healthy,
                    SmartAvailability::Unsupported => DeviceStatus::Unsupported,
                    SmartAvailability::Unavailable => DeviceStatus::Stale,
                    SmartAvailability::MissingTool => DeviceStatus::MissingTool,
                    SmartAvailability::PermissionDenied => DeviceStatus::PermissionDenied,
                },
                last_success_ms: None,
            },
            provider: None,
            failure,
            temperature_c: None,
            critical_warning: None,
            temp_critical_c: None,
            percent_used: None,
            power_on_hours: None,
            ata_attributes: None,
        }
    }

    #[must_use]
    pub fn with_failure(failure: SmartProviderFailureKind) -> Self {
        let availability = match failure {
            SmartProviderFailureKind::UnsupportedProtocol
            | SmartProviderFailureKind::BridgeLimitation => SmartAvailability::Unsupported,
            SmartProviderFailureKind::MissingTool => SmartAvailability::MissingTool,
            SmartProviderFailureKind::PermissionDenied => SmartAvailability::PermissionDenied,
            SmartProviderFailureKind::TimedOut
            | SmartProviderFailureKind::MalformedResponse
            | SmartProviderFailureKind::DeviceUnavailable
            | SmartProviderFailureKind::CommandFailed
            | SmartProviderFailureKind::TemporarilyUnavailable => SmartAvailability::Unavailable,
        };
        let mut smart = Self::with_availability(availability);
        smart.failure = Some(failure);
        smart
    }
}

impl Default for DiskSmart {
    fn default() -> Self {
        Self::with_availability(SmartAvailability::Unavailable)
    }
}

pub fn refresh_state(previous: DeviceState, observed: &mut DiskSmart, now_ms: u64) {
    observed.state = previous.transition(observed.state.status, now_ms);
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_smart_smart_state_tests.rs"]
mod smart_state_tests;
