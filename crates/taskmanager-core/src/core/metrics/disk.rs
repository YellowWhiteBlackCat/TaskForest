//! Disk telemetry metrics: capacity, available space, I/O rate, IOPS, active
//! time, and response-time scalar observations with availability, plus the
//! orthogonal connection model and per-device SMART availability/health fields.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::core::device_state::DeviceState;
use crate::core::smart::{SmartProviderFailureKind, SmartSelfTestCapabilities};
use crate::core::storage::{StorageConnection, StorageIdentityStability};
use crate::core::storage_health::{FilesystemHealth, FilesystemHealthSnapshot};
use crate::core::{DeviceGeneration, FailureKind, ProviderId};

use super::ScalarObservation;

mod wire;

/// The platform-neutral probe command for a missing SMART tool.
///
/// The smartmontools project ships the same `smartctl` binary on every
/// platform, so this command belongs to the contract: a frontend copies it
/// from the shared fact instead of composing a command of its own.
pub const SMART_TOOL_PROBE_COMMAND: &str = "smartctl --scan";

/// Which guidance paragraph a missing SMART tool needs.
///
/// The paragraph is a typed, localizable choice, not a free-form string: the
/// localization key stays the contract's, and a frontend asks for the text
/// through the shared i18n catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SmartToolGuidance {
    /// Install the missing telemetry tool, then refresh the device list.
    #[default]
    InstallTool,
}

impl SmartToolGuidance {
    /// The localization key of this guidance paragraph.
    #[must_use]
    pub const fn i18n_key(self) -> &'static str {
        match self {
            Self::InstallTool => "device.action_missing_tool",
        }
    }
}

/// The remediation for a missing SMART tool.
///
/// The probe command and the guidance paragraph are platform-neutral; the
/// install command belongs to the platform layer, which fills it only where
/// it can name the real package-manager command. A platform without one
/// leaves the install command absent rather than inventing a package
/// manager.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmartToolRemediation {
    /// The command the user copies to verify the tool and list the
    /// SMART-capable devices.
    pub probe_command: String,
    /// Which guidance paragraph explains the remediation.
    pub guidance: SmartToolGuidance,
    /// The platform's concrete install command, when the platform knows one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_command: Option<String>,
}

impl SmartToolRemediation {
    /// The platform-neutral remediation: the smartmontools probe plus the
    /// install guidance, without an install command.
    #[must_use]
    pub fn platform_neutral() -> Self {
        Self {
            probe_command: SMART_TOOL_PROBE_COMMAND.to_owned(),
            guidance: SmartToolGuidance::InstallTool,
            install_command: None,
        }
    }

    /// The same remediation with the platform's concrete install command.
    #[must_use]
    pub fn with_install_command(mut self, command: impl Into<String>) -> Self {
        self.install_command = Some(command.into());
        self
    }
}

/// Whether a disk SMART provider can currently supply health telemetry.
/// Serialized as stable snake-case strings in exported snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SmartAvailability {
    /// At least one trustworthy SMART field was collected.
    Available,
    /// This device family or operating system has no implemented provider.
    Unsupported,
    /// A supported provider could not return usable data for an unspecified
    /// reason, such as device access, command failure, or malformed output.
    #[default]
    Unavailable,
    /// The required external provider executable was definitively absent.
    MissingTool,
    /// The provider executable itself could not be launched due to EACCES.
    PermissionDenied,
}

/// Independently fallible capacity, filesystem-space, and I/O measurements.
///
/// Schema-v1 numbers are handled only by the private serde DTO. Consumers use
/// these observations so failure and measured zero remain distinct.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct DiskScalarObservations {
    pub capacity_bytes: ScalarObservation<u64>,
    pub available_bytes: ScalarObservation<u64>,
    pub read_bytes_per_sec: ScalarObservation<u64>,
    pub write_bytes_per_sec: ScalarObservation<u64>,
    pub iops: ScalarObservation<u64>,
    pub active_time_pct: ScalarObservation<f32>,
    pub response_time_ms: ScalarObservation<f32>,
    /// Mean number of in-flight I/O operations over the sampling interval.
    /// Linux derives this from `/proc/diskstats` weighted I/O time; other
    /// platforms keep the observation explicitly unavailable until they have
    /// an equivalent native counter.
    pub average_queue_depth: ScalarObservation<f32>,
    /// Busy-time per completed I/O, a lower-bound service-time estimate. The
    /// diskstats contract does not expose per-request completion latency, so
    /// this is intentionally separate from queue-inclusive response time.
    pub service_time_ms: ScalarObservation<f32>,
    /// Read requests merged per second by the block layer (`rrqm/s`).
    #[serde(default)]
    pub read_merges_per_sec: ScalarObservation<u64>,
    /// Write requests merged per second by the block layer (`wrqm/s`).
    #[serde(default)]
    pub write_merges_per_sec: ScalarObservation<u64>,
}

impl DiskScalarObservations {
    /// Retain prior successful values only within the same lifecycle
    /// generation, and only as stale when the new field observation fails.
    #[must_use]
    pub fn retain_previous(self, previous: Self) -> Self {
        Self {
            capacity_bytes: self.capacity_bytes.retain_previous(previous.capacity_bytes),
            available_bytes: self
                .available_bytes
                .retain_previous(previous.available_bytes),
            read_bytes_per_sec: self
                .read_bytes_per_sec
                .retain_previous(previous.read_bytes_per_sec),
            write_bytes_per_sec: self
                .write_bytes_per_sec
                .retain_previous(previous.write_bytes_per_sec),
            iops: self.iops.retain_previous(previous.iops),
            active_time_pct: self
                .active_time_pct
                .retain_previous(previous.active_time_pct),
            response_time_ms: self
                .response_time_ms
                .retain_previous(previous.response_time_ms),
            average_queue_depth: self
                .average_queue_depth
                .retain_previous(previous.average_queue_depth),
            service_time_ms: self
                .service_time_ms
                .retain_previous(previous.service_time_ms),
            read_merges_per_sec: self
                .read_merges_per_sec
                .retain_previous(previous.read_merges_per_sec),
            write_merges_per_sec: self
                .write_merges_per_sec
                .retain_previous(previous.write_merges_per_sec),
        }
    }

    #[must_use]
    pub fn unavailable(failure: FailureKind) -> Self {
        Self {
            capacity_bytes: ScalarObservation::unavailable(failure),
            available_bytes: ScalarObservation::unavailable(failure),
            read_bytes_per_sec: ScalarObservation::unavailable(failure),
            write_bytes_per_sec: ScalarObservation::unavailable(failure),
            iops: ScalarObservation::unavailable(failure),
            active_time_pct: ScalarObservation::unavailable(failure),
            response_time_ms: ScalarObservation::unavailable(failure),
            average_queue_depth: ScalarObservation::unavailable(failure),
            service_time_ms: ScalarObservation::unavailable(failure),
            read_merges_per_sec: ScalarObservation::unavailable(failure),
            write_merges_per_sec: ScalarObservation::unavailable(failure),
        }
    }
}

/// Typed filesystem-space observations for one partition child.
///
/// Capacity may remain available for an unmounted partition while free/used
/// space is explicitly unsupported until a filesystem mount provider supplies
/// trustworthy values. A provider failure never becomes a believable zero.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct DiskPartitionScalarObservations {
    pub capacity_bytes: ScalarObservation<u64>,
    pub used_bytes: ScalarObservation<u64>,
    pub free_bytes: ScalarObservation<u64>,
}

impl DiskPartitionScalarObservations {
    /// Retain prior successful values only within the same parent lifecycle.
    #[must_use]
    pub fn retain_previous(self, previous: Self) -> Self {
        Self {
            capacity_bytes: self.capacity_bytes.retain_previous(previous.capacity_bytes),
            used_bytes: self.used_bytes.retain_previous(previous.used_bytes),
            free_bytes: self.free_bytes.retain_previous(previous.free_bytes),
        }
    }

    #[must_use]
    pub fn unavailable(failure: FailureKind) -> Self {
        Self {
            capacity_bytes: ScalarObservation::unavailable(failure),
            used_bytes: ScalarObservation::unavailable(failure),
            free_bytes: ScalarObservation::unavailable(failure),
        }
    }
}

/// A partition is a filesystem-space child of one physical or logical disk.
///
/// `parent_device_id` is the lifecycle association. `device_generation` is
/// copied from that parent after the application/provider projection accepts a
/// refresh, so a parent re-attach cannot blend old partition usage into the
/// new physical generation. SMART fields intentionally do not live here.
#[derive(Debug, Clone, Default)]
pub struct DiskPartition {
    /// Stable identity derived from the parent identity and native partition
    /// name, never from the current sidebar index.
    pub device_id: String,
    pub parent_device_id: String,
    pub device_generation: DeviceGeneration,
    pub device_state: DeviceState,
    /// Kernel partition name, without the `/dev/` prefix.
    pub name: String,
    /// Empty means the partition is discovered but not mounted or its mount
    /// provider is unavailable.
    pub mount_point: String,
    pub fs_type: String,
    scalar_observations: DiskPartitionScalarObservations,
}

impl DiskPartition {
    /// Construct a discovered partition. Measurement truth is attached only
    /// through [`Self::apply_scalar_observations`].
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    /// Derive a stable child identity without depending on a sidebar index or
    /// the order in which mount facts were returned by the provider.
    #[must_use]
    pub fn stable_id(parent_device_id: &str, partition_name: &str) -> String {
        format!("partition:{parent_device_id}:{partition_name}")
    }

    #[must_use]
    pub const fn current_capacity_bytes(&self) -> Option<u64> {
        self.scalar_observations
            .capacity_bytes
            .current_value()
            .copied()
    }

    #[must_use]
    pub fn current_used_bytes(&self) -> Option<u64> {
        self.scalar_observations.used_bytes.current_value().copied()
    }

    #[must_use]
    pub fn current_free_bytes(&self) -> Option<u64> {
        self.scalar_observations.free_bytes.current_value().copied()
    }

    #[must_use]
    pub const fn scalar_observations(&self) -> &DiskPartitionScalarObservations {
        &self.scalar_observations
    }

    /// The filesystem health fact observed for this partition's mount.
    ///
    /// The association is the canonical mount path both facts already carry,
    /// and it is resolved here — in the contract layer — so no consumer
    /// compares mount-point strings itself. `None` means the partition is not
    /// mounted or the filesystem provider observed no such mount; it never
    /// means a healthy filesystem.
    #[must_use]
    pub fn filesystem_health<'a>(
        &self,
        filesystems: &'a FilesystemHealthSnapshot,
    ) -> Option<&'a FilesystemHealth> {
        if self.mount_point.is_empty() {
            return None;
        }
        let mount_point = Path::new(&self.mount_point);
        filesystems
            .filesystems
            .iter()
            .find(|filesystem| filesystem.mount_point.as_path() == mount_point)
    }

    /// Replace the partition's canonical measurement group atomically.
    pub fn apply_scalar_observations(&mut self, observations: DiskPartitionScalarObservations) {
        self.scalar_observations = observations;
    }
}

/// Enhanced Disk Metrics & IOPS
#[derive(Debug, Clone, Default)]
pub struct DiskMetrics {
    /// Lifecycle correlation identity. See [`Self::identity_stability`] before
    /// treating it as persistent across detach or native-locator reorder.
    pub device_id: String,
    /// Confirmed hot-plug generation for this stable identity. Zero means the
    /// metric has not yet passed through a lifecycle assembler.
    pub device_generation: DeviceGeneration,
    pub device_state: DeviceState,
    pub name: String,
    pub disk_type: String, // e.g. "NVMe SSD", "SATA SSD", "HDD", "Virtual"
    /// Canonical protocol/interconnect/presentation classification.
    connection: StorageConnection,
    /// Whether the lifecycle identity is expected to survive native locator
    /// renumbering. Attachment-scoped IDs must not be advertised as reorder
    /// safe.
    pub identity_stability: StorageIdentityStability,
    /// Device model reported by the native adapter. Empty remains the legacy
    /// representation for unavailable model text.
    pub model: String,
    /// Device serial reported by the native adapter. `None` is unavailable,
    /// never a fabricated empty serial.
    pub serial: Option<String>,
    /// Firmware/revision string reported by the native adapter.
    pub revision: Option<String>,
    pub mount_point: String,
    pub fs_type: String,
    /// Filesystem-space children discovered below this physical/logical disk.
    /// The parent remains the owner of SMART and I/O telemetry.
    pub partitions: Vec<DiskPartition>,
    /// Authoritative typed truth for live disk scalars.
    scalar_observations: DiskScalarObservations,
    /// Whether the medium itself can be removed from the device. `None` means
    /// the native adapter could not determine it.
    media_removable: Option<bool>,
    /// Whether the attachment path supports hot-plug. This is not inferred
    /// from removable-media state.
    hotplug_capable: Option<bool>,
    /// Platform-neutral availability of SMART telemetry. This remains explicit
    /// even when every individual optional health field is absent.
    pub smart_availability: SmartAvailability,
    /// The platform's remediation for a missing SMART tool.
    ///
    /// `None` while the tool is present. A platform that knows its own
    /// install command supplies it; the contract's platform-neutral
    /// remediation applies otherwise.
    pub smart_tool_remediation: Option<SmartToolRemediation>,
    pub smart_state: DeviceState,
    /// Runtime protocol provider selected for this SMART observation.
    pub smart_provider: Option<ProviderId>,
    /// Precise protocol/provider failure when availability is not healthy.
    pub smart_failure: Option<SmartProviderFailureKind>,
    /// Device-reported self-test support, separate from SMART health
    /// availability and current-process authorization.
    pub smart_self_test_capabilities: SmartSelfTestCapabilities,
    /// Device-health temperature in °C. `None` means no trustworthy
    /// observation was available from the selected native provider.
    pub smart_temperature_c: Option<f32>,
    /// Additional controller/NAND temperature sensors, in Celsius. The
    /// primary `smart_temperature_c` remains the composite value.
    pub smart_temperature_sensors_c: Vec<f32>,
    /// Provider-normalized device-health warning flag. `None` remains distinct
    /// from a confirmed `false`.
    pub smart_critical_warning: Option<bool>,
    /// Provider-reported critical-temperature threshold in °C.
    pub smart_temp_critical_c: Option<f32>,
    /// Provider-normalized estimated endurance used, in percent (0–100+).
    pub smart_percent_used: Option<f32>,
    /// NVMe controller-reported available spare capacity, in percent.
    pub smart_available_spare_pct: Option<f32>,
    /// NVMe controller-reported available-spare warning threshold, in percent.
    pub smart_available_spare_threshold_pct: Option<f32>,
    /// Provider-reported power-on hours.
    pub smart_power_on_hours: Option<u64>,
    /// NVMe lifetime unsafe-shutdown event counter.
    pub smart_unsafe_shutdowns: Option<u64>,
}

impl DiskMetrics {
    /// Construct a discovered disk. Classification and measurement truth enter
    /// through the named typed assembly methods below.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    /// Project every child onto the accepted parent lifecycle.
    ///
    /// This keeps parent re-attach semantics in the shared read model: a
    /// partition cannot retain an old parent generation or stable ID merely
    /// because its native name was reused.
    pub fn project_partition_lifecycle(&mut self) {
        for partition in &mut self.partitions {
            partition.parent_device_id = self.device_id.clone();
            partition.device_generation = self.device_generation;
            if !partition.name.is_empty() {
                partition.device_id = DiskPartition::stable_id(&self.device_id, &partition.name);
            }
        }
    }

    #[must_use]
    pub const fn current_capacity_bytes(&self) -> Option<u64> {
        self.scalar_observations
            .capacity_bytes
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_available_bytes(&self) -> Option<u64> {
        self.scalar_observations
            .available_bytes
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_read_bytes_per_sec(&self) -> Option<u64> {
        self.scalar_observations
            .read_bytes_per_sec
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_write_bytes_per_sec(&self) -> Option<u64> {
        self.scalar_observations
            .write_bytes_per_sec
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_iops(&self) -> Option<u64> {
        self.scalar_observations.iops.current_value().copied()
    }

    #[must_use]
    pub fn current_active_time_pct(&self) -> Option<f32> {
        self.scalar_observations
            .active_time_pct
            .current_value()
            .copied()
            .filter(|value| value.is_finite())
    }

    #[must_use]
    pub fn current_response_time_ms(&self) -> Option<f32> {
        self.scalar_observations
            .response_time_ms
            .current_value()
            .copied()
            .filter(|value| value.is_finite())
    }

    #[must_use]
    pub fn current_average_queue_depth(&self) -> Option<f32> {
        self.scalar_observations
            .average_queue_depth
            .current_value()
            .copied()
            .filter(|value| value.is_finite() && *value >= 0.0)
    }

    #[must_use]
    pub fn current_service_time_ms(&self) -> Option<f32> {
        self.scalar_observations
            .service_time_ms
            .current_value()
            .copied()
            .filter(|value| value.is_finite() && *value >= 0.0)
    }

    #[must_use]
    pub const fn current_read_merges_per_sec(&self) -> Option<u64> {
        self.scalar_observations
            .read_merges_per_sec
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn current_write_merges_per_sec(&self) -> Option<u64> {
        self.scalar_observations
            .write_merges_per_sec
            .current_value()
            .copied()
    }

    #[must_use]
    pub const fn scalar_observations(&self) -> &DiskScalarObservations {
        &self.scalar_observations
    }

    /// The remediation guidance for this device's missing SMART tool.
    ///
    /// `None` while the tool is present. The contract's platform-neutral
    /// remediation is the default, so a frontend never composes a command: it
    /// copies the probe command from this fact and renders the guidance
    /// paragraph named by [`SmartToolGuidance::i18n_key`].
    #[must_use]
    pub fn smart_tool_remediation_hint(&self) -> Option<SmartToolRemediation> {
        if self.smart_availability != SmartAvailability::MissingTool {
            return None;
        }
        Some(
            self.smart_tool_remediation
                .clone()
                .unwrap_or_else(SmartToolRemediation::platform_neutral),
        )
    }

    /// Replace typed truth atomically. Compatibility scalars are projected
    /// only while serializing the private wire DTO.
    pub fn apply_scalar_observations(&mut self, observations: DiskScalarObservations) {
        self.scalar_observations = observations;
    }

    #[must_use]
    pub const fn connection(&self) -> StorageConnection {
        self.connection
    }

    pub fn apply_connection(&mut self, connection: StorageConnection) {
        self.connection = connection;
    }

    #[must_use]
    pub const fn media_removable(&self) -> Option<bool> {
        self.media_removable
    }

    #[must_use]
    pub const fn hotplug_capable(&self) -> Option<bool> {
        self.hotplug_capable
    }

    pub fn apply_attachment_capabilities(
        &mut self,
        media_removable: Option<bool>,
        hotplug_capable: Option<bool>,
    ) {
        self.media_removable = media_removable;
        self.hotplug_capable = hotplug_capable;
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_metrics_disk_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/headless/core_core_metrics_disk_smart_tests.rs"]
mod smart_tests;
