//! Private serde compatibility boundary for disk and partition rows.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{
    DiskMetrics, DiskPartition, DiskPartitionScalarObservations, DiskScalarObservations,
    SmartAvailability,
};
use crate::core::device_state::DeviceState;
use crate::core::smart::SmartProviderFailureKind;
use crate::core::storage::{
    StorageConnection, StorageDeviceKind, StorageIdentityStability, StorageInterconnect,
    StorageProtocol,
};
use crate::core::{DeviceGeneration, ProviderId, ScalarAvailability, ScalarObservation};

const LEGACY_DISK_OBSERVED_AT_MS: u64 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum StorageTransportWire {
    Nvme,
    Sata,
    Sas,
    Scsi,
    Usb,
    Virtio,
    Mmc,
    Ufs,
    Ide,
    DeviceMapper,
    SoftwareRaid,
    #[default]
    Unknown,
}

impl StorageTransportWire {
    const fn from_connection(connection: StorageConnection) -> Self {
        match (
            connection.device_kind,
            connection.interconnect,
            connection.protocol,
        ) {
            (StorageDeviceKind::Virtual, StorageInterconnect::Platform, _) => Self::DeviceMapper,
            (StorageDeviceKind::Aggregate, StorageInterconnect::Platform, _) => Self::SoftwareRaid,
            (_, StorageInterconnect::Virtio, _) => Self::Virtio,
            (StorageDeviceKind::Virtual | StorageDeviceKind::Aggregate, _, _) => Self::Unknown,
            (_, StorageInterconnect::Usb, _) => Self::Usb,
            (_, StorageInterconnect::Sata, _) => Self::Sata,
            (_, StorageInterconnect::Sas, _) => Self::Sas,
            (_, StorageInterconnect::Mmc | StorageInterconnect::Sd, _) => Self::Mmc,
            (_, StorageInterconnect::Ufs, _) => Self::Ufs,
            (_, StorageInterconnect::Ide, _) => Self::Ide,
            (_, _, StorageProtocol::Nvme) => Self::Nvme,
            (_, _, StorageProtocol::Ata) => Self::Sata,
            (_, _, StorageProtocol::Scsi) => Self::Scsi,
            (_, _, StorageProtocol::Mmc | StorageProtocol::Sd) => Self::Mmc,
            (_, _, StorageProtocol::Ufs) => Self::Ufs,
            _ => Self::Unknown,
        }
    }

    const fn into_connection(self) -> StorageConnection {
        match self {
            Self::Nvme => StorageConnection::new(
                StorageProtocol::Nvme,
                StorageInterconnect::Pcie,
                StorageDeviceKind::Physical,
            ),
            Self::Sata => StorageConnection::new(
                StorageProtocol::Ata,
                StorageInterconnect::Sata,
                StorageDeviceKind::Physical,
            ),
            Self::Sas => StorageConnection::new(
                StorageProtocol::Scsi,
                StorageInterconnect::Sas,
                StorageDeviceKind::Physical,
            ),
            Self::Scsi => StorageConnection::new(
                StorageProtocol::Scsi,
                StorageInterconnect::Unknown,
                StorageDeviceKind::Physical,
            ),
            Self::Usb => StorageConnection::new(
                StorageProtocol::Unknown,
                StorageInterconnect::Usb,
                StorageDeviceKind::Physical,
            ),
            Self::Virtio => StorageConnection::new(
                StorageProtocol::Unknown,
                StorageInterconnect::Virtio,
                StorageDeviceKind::Virtual,
            ),
            Self::Mmc => StorageConnection::new(
                StorageProtocol::Mmc,
                StorageInterconnect::Mmc,
                StorageDeviceKind::Physical,
            ),
            Self::Ufs => StorageConnection::new(
                StorageProtocol::Ufs,
                StorageInterconnect::Ufs,
                StorageDeviceKind::Physical,
            ),
            Self::Ide => StorageConnection::new(
                StorageProtocol::Ata,
                StorageInterconnect::Ide,
                StorageDeviceKind::Physical,
            ),
            Self::DeviceMapper => StorageConnection::new(
                StorageProtocol::Unknown,
                StorageInterconnect::Platform,
                StorageDeviceKind::Virtual,
            ),
            Self::SoftwareRaid => StorageConnection::new(
                StorageProtocol::Unknown,
                StorageInterconnect::Platform,
                StorageDeviceKind::Aggregate,
            ),
            Self::Unknown => StorageConnection::new(
                StorageProtocol::Unknown,
                StorageInterconnect::Unknown,
                StorageDeviceKind::Unknown,
            ),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct DiskPartitionWire {
    #[serde(default)]
    device_id: String,
    #[serde(default)]
    parent_device_id: String,
    #[serde(default)]
    device_generation: DeviceGeneration,
    #[serde(default)]
    device_state: DeviceState,
    #[serde(default)]
    name: String,
    #[serde(default)]
    mount_point: String,
    #[serde(default)]
    fs_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    total_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    used_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    available_bytes: Option<u64>,
    #[serde(default)]
    scalar_observations: DiskPartitionScalarObservations,
}

#[derive(Serialize, Deserialize)]
struct DiskMetricsWire {
    #[serde(default)]
    device_id: String,
    #[serde(default)]
    device_generation: DeviceGeneration,
    #[serde(default)]
    device_state: DeviceState,
    #[serde(default)]
    name: String,
    #[serde(default)]
    disk_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    transport: Option<StorageTransportWire>,
    #[serde(default)]
    connection: StorageConnection,
    #[serde(default)]
    identity_stability: StorageIdentityStability,
    #[serde(default)]
    model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    serial: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    revision: Option<String>,
    #[serde(default)]
    mount_point: String,
    #[serde(default)]
    fs_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    total_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    available_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    read_bytes_per_sec: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    write_bytes_per_sec: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    iops: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active_time_pct: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    response_time_ms: Option<f32>,
    #[serde(default)]
    partitions: Vec<DiskPartition>,
    #[serde(default)]
    scalar_observations: DiskScalarObservations,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    removable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    media_removable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hotplug_capable: Option<bool>,
    #[serde(default)]
    smart_availability: SmartAvailability,
    #[serde(default)]
    smart_state: DeviceState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    smart_provider: Option<ProviderId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    smart_failure: Option<SmartProviderFailureKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    smart_temperature_c: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    smart_critical_warning: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    smart_temp_critical_c: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    smart_percent_used: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    smart_power_on_hours: Option<u64>,
}

impl Serialize for DiskPartition {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        DiskPartitionWire {
            device_id: self.device_id.clone(),
            parent_device_id: self.parent_device_id.clone(),
            device_generation: self.device_generation,
            device_state: self.device_state,
            name: self.name.clone(),
            mount_point: self.mount_point.clone(),
            fs_type: self.fs_type.clone(),
            total_bytes: self.current_capacity_bytes(),
            used_bytes: self.current_used_bytes(),
            available_bytes: self.current_free_bytes(),
            scalar_observations: self.scalar_observations,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DiskPartition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = DiskPartitionWire::deserialize(deserializer)?;
        let mut observations = wire.scalar_observations;
        if trustworthy_partition_identity(&wire) {
            hydrate_nonzero_unknown(&mut observations.capacity_bytes, wire.total_bytes);
            if !wire.mount_point.trim().is_empty() {
                hydrate_unknown(&mut observations.used_bytes, wire.used_bytes);
                hydrate_unknown(&mut observations.free_bytes, wire.available_bytes);
            }
        }
        Ok(Self {
            device_id: wire.device_id,
            parent_device_id: wire.parent_device_id,
            device_generation: wire.device_generation,
            device_state: wire.device_state,
            name: wire.name,
            mount_point: wire.mount_point,
            fs_type: wire.fs_type,
            scalar_observations: observations,
        })
    }
}

impl Serialize for DiskMetrics {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let legacy_transport = StorageTransportWire::from_connection(self.connection);
        DiskMetricsWire {
            device_id: self.device_id.clone(),
            device_generation: self.device_generation,
            device_state: self.device_state,
            name: self.name.clone(),
            disk_type: self.disk_type.clone(),
            transport: (legacy_transport != StorageTransportWire::Unknown)
                .then_some(legacy_transport),
            connection: self.connection,
            identity_stability: self.identity_stability,
            model: self.model.clone(),
            serial: self.serial.clone(),
            revision: self.revision.clone(),
            mount_point: self.mount_point.clone(),
            fs_type: self.fs_type.clone(),
            total_bytes: self.current_capacity_bytes(),
            available_bytes: self.current_available_bytes(),
            read_bytes_per_sec: self.current_read_bytes_per_sec(),
            write_bytes_per_sec: self.current_write_bytes_per_sec(),
            iops: self.current_iops(),
            active_time_pct: self.current_active_time_pct(),
            response_time_ms: self.current_response_time_ms(),
            partitions: self.partitions.clone(),
            scalar_observations: self.scalar_observations,
            removable: self.media_removable,
            media_removable: self.media_removable,
            hotplug_capable: self.hotplug_capable,
            smart_availability: self.smart_availability,
            smart_state: self.smart_state,
            smart_provider: self.smart_provider.clone(),
            smart_failure: self.smart_failure,
            smart_temperature_c: self.smart_temperature_c,
            smart_critical_warning: self.smart_critical_warning,
            smart_temp_critical_c: self.smart_temp_critical_c,
            smart_percent_used: self.smart_percent_used,
            smart_power_on_hours: self.smart_power_on_hours,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DiskMetrics {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = DiskMetricsWire::deserialize(deserializer)?;
        let trusted_identity = trustworthy_disk_identity(&wire);
        let mut connection = wire.connection;
        if trusted_identity
            && connection == StorageConnection::default()
            && let Some(transport) = wire
                .transport
                .filter(|transport| *transport != StorageTransportWire::Unknown)
        {
            connection = transport.into_connection();
        }

        let mut observations = wire.scalar_observations;
        if trusted_identity {
            hydrate_nonzero_unknown(&mut observations.capacity_bytes, wire.total_bytes);
            if !wire.mount_point.trim().is_empty() {
                hydrate_unknown(&mut observations.available_bytes, wire.available_bytes);
            }
            hydrate_nonzero_unknown(
                &mut observations.read_bytes_per_sec,
                wire.read_bytes_per_sec,
            );
            hydrate_nonzero_unknown(
                &mut observations.write_bytes_per_sec,
                wire.write_bytes_per_sec,
            );
            hydrate_nonzero_unknown(&mut observations.iops, wire.iops);
            hydrate_positive_finite_unknown(
                &mut observations.active_time_pct,
                wire.active_time_pct,
            );
            hydrate_positive_finite_unknown(
                &mut observations.response_time_ms,
                wire.response_time_ms,
            );
        }

        let media_removable = wire.media_removable.or_else(|| {
            trusted_identity
                .then_some(wire.removable)
                .flatten()
                .filter(|value| *value)
        });

        Ok(Self {
            device_id: wire.device_id,
            device_generation: wire.device_generation,
            device_state: wire.device_state,
            name: wire.name,
            disk_type: wire.disk_type,
            connection,
            identity_stability: wire.identity_stability,
            model: wire.model,
            serial: wire.serial,
            revision: wire.revision,
            mount_point: wire.mount_point,
            fs_type: wire.fs_type,
            partitions: wire.partitions,
            scalar_observations: observations,
            media_removable,
            hotplug_capable: wire.hotplug_capable,
            smart_availability: wire.smart_availability,
            smart_state: wire.smart_state,
            smart_provider: wire.smart_provider,
            smart_failure: wire.smart_failure,
            smart_temperature_c: wire.smart_temperature_c,
            smart_critical_warning: wire.smart_critical_warning,
            smart_temp_critical_c: wire.smart_temp_critical_c,
            smart_percent_used: wire.smart_percent_used,
            smart_power_on_hours: wire.smart_power_on_hours,
        })
    }
}

fn hydrate_unknown<T>(observation: &mut ScalarObservation<T>, value: Option<T>) {
    if observation.availability() == ScalarAvailability::Unknown
        && let Some(value) = value
    {
        *observation = ScalarObservation::available(value, LEGACY_DISK_OBSERVED_AT_MS);
    }
}

fn hydrate_nonzero_unknown(observation: &mut ScalarObservation<u64>, value: Option<u64>) {
    hydrate_unknown(observation, value.filter(|value| *value > 0));
}

fn hydrate_positive_finite_unknown(observation: &mut ScalarObservation<f32>, value: Option<f32>) {
    hydrate_unknown(
        observation,
        value.filter(|value| *value > 0.0 && value.is_finite()),
    );
}

fn trustworthy_disk_identity(wire: &DiskMetricsWire) -> bool {
    !wire.device_id.trim().is_empty() || !wire.name.trim().is_empty()
}

fn trustworthy_partition_identity(wire: &DiskPartitionWire) -> bool {
    !wire.device_id.trim().is_empty()
        || (!wire.parent_device_id.trim().is_empty() && !wire.name.trim().is_empty())
}
