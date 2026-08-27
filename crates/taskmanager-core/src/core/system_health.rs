//! Platform-neutral aggregate health observations.
//!
//! Discovery and command execution stay in platform providers. These models
//! carry only stable device identity, presentation metadata, and typed health
//! observations so every frontend can consume the same contract.

use serde::{Deserialize, Serialize};

use crate::core::{
    DeviceGeneration, DeviceId, FilesystemHealthSnapshot, SensorCenterSnapshot, SmartSelfTestKind,
    SmartSelfTestReport, StorageDeviceKey, StorageDeviceTarget,
};

/// Provider-neutral identity and intent for a destructive-capable SMART job.
///
/// `device_id`/`device_generation` identify the physical target. `device_key`
/// stays an opaque command locator: a Linux provider may resolve it to a
/// block-device name while another platform can use its native locator.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmartSelfTestIntent {
    /// Stable physical disk identity selected from the telemetry lifecycle.
    #[serde(default)]
    pub device_id: DeviceId,
    /// Disk generation at confirmation time; never a SMART-job generation.
    #[serde(default)]
    pub device_generation: DeviceGeneration,
    /// Provider-native locator used only to address the command.
    pub device_key: StorageDeviceKey,
    pub display_name: String,
    pub kind: SmartSelfTestKind,
}

impl SmartSelfTestIntent {
    /// Return the complete mutation target supplied to a native provider.
    ///
    /// Keeping identity, lifecycle generation, and native locator together
    /// allows the adapter to reject a stale locator after hot-plug.
    #[must_use]
    pub fn target(&self) -> StorageDeviceTarget {
        StorageDeviceTarget {
            device_id: self.device_id.clone(),
            device_generation: self.device_generation,
            locator: self.device_key.clone(),
        }
    }

    #[must_use]
    pub fn into_observation(self, report: SmartSelfTestReport) -> SmartSelfTestObservation {
        SmartSelfTestObservation {
            device_id: self.device_id,
            device_generation: self.device_generation,
            device_key: self.device_key,
            display_name: self.display_name,
            report,
        }
    }
}

/// Selected device plus the newest typed self-test report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SmartSelfTestObservation {
    /// Stable physical target identity retained across native locator changes.
    #[serde(default)]
    pub device_id: DeviceId,
    /// Physical target generation copied from the confirmed intent.
    #[serde(default)]
    pub device_generation: DeviceGeneration,
    /// Provider-native locator; this is not lifecycle identity.
    pub device_key: StorageDeviceKey,
    pub display_name: String,
    pub report: SmartSelfTestReport,
}

impl SmartSelfTestObservation {
    /// Return the same generation-bound target used to start the job.
    #[must_use]
    pub fn target(&self) -> StorageDeviceTarget {
        StorageDeviceTarget {
            device_id: self.device_id.clone(),
            device_generation: self.device_generation,
            locator: self.device_key.clone(),
        }
    }
}

/// Latest cross-provider health state.
///
/// Event ordering and correlation live in the platform contract envelope, so
/// this domain snapshot deliberately carries no transport sequence number.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SystemHealthSnapshot {
    pub filesystems: FilesystemHealthSnapshot,
    pub sensors: SensorCenterSnapshot,
    /// Independently tracked jobs keyed by physical device lifecycle identity.
    /// Presentation layers may select one row without collapsing this model.
    pub self_tests: Vec<SmartSelfTestObservation>,
}

#[cfg(test)]
#[path = "../../tests/headless/core_core_system_health_tests.rs"]
mod tests;
