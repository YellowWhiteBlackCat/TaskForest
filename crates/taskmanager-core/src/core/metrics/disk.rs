//! Disk telemetry metrics: capacity, available space, I/O rate, IOPS, active
//! time, and response-time scalar observations with availability, plus the
//! orthogonal connection model and per-device SMART availability/health fields.

use serde::{Deserialize, Serialize};

use crate::core::device_state::DeviceState;
use crate::core::storage::{StorageConnection, StorageIdentityStability};
use crate::core::{DeviceGeneration, FailureKind, ProviderId};

use super::ScalarObservation;

mod wire;

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
    pub smart_state: DeviceState,
    /// Runtime protocol provider selected for this SMART observation.
    pub smart_provider: Option<ProviderId>,
    /// Precise protocol/provider failure when availability is not healthy.
    pub smart_failure: Option<crate::core::smart::SmartProviderFailureKind>,
    /// Device-health temperature in °C. `None` means no trustworthy
    /// observation was available from the selected native provider.
    pub smart_temperature_c: Option<f32>,
    /// Provider-normalized device-health warning flag. `None` remains distinct
    /// from a confirmed `false`.
    pub smart_critical_warning: Option<bool>,
    /// Provider-reported critical-temperature threshold in °C.
    pub smart_temp_critical_c: Option<f32>,
    /// Provider-normalized estimated endurance used, in percent (0–100+).
    pub smart_percent_used: Option<f32>,
    /// Provider-reported power-on hours.
    pub smart_power_on_hours: Option<u64>,
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
    pub const fn scalar_observations(&self) -> &DiskScalarObservations {
        &self.scalar_observations
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
