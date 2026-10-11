//! Owner facts that make a device row identity stable across refreshes.

use taskmanager_application::DeviceLifecyclePartition;
use taskmanager_core::core::identity::{DeviceGeneration, DeviceId};
use taskmanager_core::core::metrics::{DiskMetrics, DiskPartition, GpuMetrics, NetworkMetrics};
use taskmanager_core::core::npu::NpuDevice;
use taskmanager_core::core::power::BatteryInfo;
use taskmanager_core::core::sensors::SensorReading;

/// Device family whose owner facts supply a renderer-neutral row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DeviceRowFamily {
    Storage,
    Network,
    Gpu,
    PowerSupply,
    Sensor,
    Npu,
}

impl DeviceRowFamily {
    pub(super) const fn lifecycle_partition(self) -> DeviceLifecyclePartition {
        match self {
            Self::Storage => DeviceLifecyclePartition::SystemStorage,
            Self::Network => DeviceLifecyclePartition::SystemNetwork,
            Self::Gpu => DeviceLifecyclePartition::SystemGpu,
            Self::PowerSupply => DeviceLifecyclePartition::PowerSupplies,
            Self::Sensor => DeviceLifecyclePartition::Sensors,
            Self::Npu => DeviceLifecyclePartition::NpuInventory,
        }
    }
}

/// Stable identity of a row owned by one typed device family.
///
/// Every physical device row includes the owner `DeviceId` and its confirmed
/// incarnation. A storage partition is a child of its parent device
/// incarnation; a sensor channel also includes its local channel ID.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DeviceRowId {
    StorageDisk {
        device_id: DeviceId,
        generation: DeviceGeneration,
    },
    StoragePartition {
        parent_device_id: DeviceId,
        generation: DeviceGeneration,
        partition_id: DeviceId,
    },
    Network {
        device_id: DeviceId,
        generation: DeviceGeneration,
    },
    Gpu {
        device_id: DeviceId,
        generation: DeviceGeneration,
    },
    PowerSupply {
        device_id: DeviceId,
        generation: DeviceGeneration,
    },
    SensorChannel {
        device_id: DeviceId,
        generation: DeviceGeneration,
        channel_id: String,
    },
    Npu {
        device_id: DeviceId,
        generation: DeviceGeneration,
    },
}

impl DeviceRowId {
    #[must_use]
    pub const fn family(&self) -> DeviceRowFamily {
        match self {
            Self::StorageDisk { .. } | Self::StoragePartition { .. } => DeviceRowFamily::Storage,
            Self::Network { .. } => DeviceRowFamily::Network,
            Self::Gpu { .. } => DeviceRowFamily::Gpu,
            Self::PowerSupply { .. } => DeviceRowFamily::PowerSupply,
            Self::SensorChannel { .. } => DeviceRowFamily::Sensor,
            Self::Npu { .. } => DeviceRowFamily::Npu,
        }
    }

    #[must_use]
    pub fn from_disk(disk: &DiskMetrics) -> Option<Self> {
        let device_id = nonempty_device_id(&disk.device_id)?;
        disk.device_generation
            .is_valid()
            .then_some(Self::StorageDisk {
                device_id,
                generation: disk.device_generation,
            })
    }

    #[must_use]
    pub fn from_partition(disk: &DiskMetrics, partition: &DiskPartition) -> Option<Self> {
        if partition.parent_device_id != disk.device_id
            || partition.device_generation != disk.device_generation
        {
            return None;
        }
        let parent_device_id = nonempty_device_id(&partition.parent_device_id)?;
        let partition_id = nonempty_device_id(&partition.device_id)?;
        partition
            .device_generation
            .is_valid()
            .then_some(Self::StoragePartition {
                parent_device_id,
                generation: partition.device_generation,
                partition_id,
            })
    }

    #[must_use]
    pub fn from_network(network: &NetworkMetrics) -> Option<Self> {
        let device_id = nonempty_device_id(&network.device_id)?;
        network
            .device_generation
            .is_valid()
            .then_some(Self::Network {
                device_id,
                generation: network.device_generation,
            })
    }

    #[must_use]
    pub fn from_gpu(gpu: &GpuMetrics) -> Option<Self> {
        let device_id = nonempty_device_id(&gpu.device_id)?;
        gpu.device_generation.is_valid().then_some(Self::Gpu {
            device_id,
            generation: gpu.device_generation,
        })
    }

    #[must_use]
    pub fn from_power_supply(battery: &BatteryInfo) -> Option<Self> {
        let device_id = nonempty_device_id(&battery.id)?;
        battery
            .device_generation
            .is_valid()
            .then_some(Self::PowerSupply {
                device_id,
                generation: battery.device_generation,
            })
    }

    #[must_use]
    pub fn from_sensor(reading: &SensorReading) -> Option<Self> {
        let device_id = nonempty_device_id(reading.device_id().as_str())?;
        let channel_id = nonempty_string(reading.id())?;
        reading
            .device_generation()
            .is_valid()
            .then_some(Self::SensorChannel {
                device_id,
                generation: reading.device_generation(),
                channel_id,
            })
    }

    #[must_use]
    pub fn from_npu(device: &NpuDevice) -> Option<Self> {
        let device_id = nonempty_device_id(device.device_id.as_str())?;
        device.device_generation.is_valid().then_some(Self::Npu {
            device_id,
            generation: device.device_generation,
        })
    }

    pub(super) fn physical_owner(&self) -> (&DeviceId, DeviceGeneration) {
        match self {
            Self::StorageDisk {
                device_id,
                generation,
            }
            | Self::Network {
                device_id,
                generation,
            }
            | Self::Gpu {
                device_id,
                generation,
            }
            | Self::PowerSupply {
                device_id,
                generation,
            }
            | Self::Npu {
                device_id,
                generation,
            } => (device_id, *generation),
            Self::StoragePartition {
                parent_device_id,
                generation,
                ..
            }
            | Self::SensorChannel {
                device_id: parent_device_id,
                generation,
                ..
            } => (parent_device_id, *generation),
        }
    }
}

fn nonempty_device_id(value: &str) -> Option<DeviceId> {
    nonempty_string(value).map(DeviceId::new)
}

fn nonempty_string(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_owned())
}

impl Default for DeviceRowFamily {
    fn default() -> Self {
        Self::Storage
    }
}
