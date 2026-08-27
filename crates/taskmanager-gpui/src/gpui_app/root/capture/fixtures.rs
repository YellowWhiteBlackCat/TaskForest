//! Deterministic hardware fixtures used only by capture scenarios.

use crate::core::device_state::{DeviceState, DeviceStatus};
use crate::core::metrics::{
    DiskMetrics, DiskPartition, DiskPartitionScalarObservations, DiskScalarObservations, GpuEngine,
    GpuEngineKind, GpuGraphicsApi, GpuMetrics, GpuScalarObservations, SmartAvailability,
    SystemSnapshot,
};
use crate::core::{
    BatteryInfo, BatteryScalarObservations, DeviceGeneration, DeviceId, FailureKind, NpuDevice,
    NpuEngineKind, NpuEngineUsage, NpuInventorySnapshot, NpuMemoryReport, PowerSupplySnapshot,
    ScalarObservation, SensorCenterSnapshot, SensorDescriptor, SensorMagnitude,
    SensorMeasurementObservation, SensorReading, SensorScale,
};

fn clear_smart_values(disk: &mut DiskMetrics) {
    disk.smart_temperature_c = None;
    disk.smart_critical_warning = None;
    disk.smart_temp_critical_c = None;
    disk.smart_percent_used = None;
    disk.smart_power_on_hours = None;
}

pub(super) const GPU_ENGINE_CAPTURE_DEVICE_ID: &str = "gpu:capture:engine-inventory";
pub(super) const GPU_ENGINE_CAPTURE_LAST_SAMPLE_INDEX: u64 = 4;

/// One stable capture-only GPU frame. Both current projection and correlated
/// history use this builder, so engine identity/generation cannot drift
/// between the rendered card and its graph rings.
pub(super) fn gpu_engine_inventory_frame(index: u64, observed_at_ms: u64) -> GpuMetrics {
    let observed_at_ms = observed_at_ms.max(1);
    let phase = (index % 5) as f32;
    let mut gpu = GpuMetrics::new(GPU_ENGINE_CAPTURE_DEVICE_ID, "TaskForest Capture GPU");
    gpu.marketing_name = Some("Capture GPU".into());
    gpu.driver = Some("capture-gpu".into());
    gpu.pci_slot = Some("0000:01:00.0".into());
    gpu.graphics_api = Some(GpuGraphicsApi {
        opengl_version: Some("4.6".into()),
        vulkan_version: Some("1.4.354".into()),
    });
    gpu.device_generation = DeviceGeneration::new(1);
    gpu.device_state = DeviceState::healthy(observed_at_ms);
    gpu.apply_scalar_observations(GpuScalarObservations {
        utilization_pct: ScalarObservation::available(38.0 + phase * 6.0, observed_at_ms),
        temperature_c: ScalarObservation::available(54.0 + phase, observed_at_ms),
        dedicated_vram_used_bytes: ScalarObservation::available(
            (3 + index % 2) * 1024 * 1024 * 1024,
            observed_at_ms,
        ),
        dedicated_vram_total_bytes: ScalarObservation::available(
            8 * 1024 * 1024 * 1024,
            observed_at_ms,
        ),
        shared_vram_used_bytes: ScalarObservation::available(
            (512 + index * 32) * 1024 * 1024,
            observed_at_ms,
        ),
        shared_vram_total_bytes: ScalarObservation::available(
            16 * 1024 * 1024 * 1024,
            observed_at_ms,
        ),
        frequency_mhz: ScalarObservation::available(1_650 + index * 45, observed_at_ms),
        max_frequency_mhz: ScalarObservation::available(2_250, observed_at_ms),
        idle_residency_pct: ScalarObservation::available(62.0, observed_at_ms),
        power_w: ScalarObservation::available(74.0 + phase * 2.5, observed_at_ms),
        ..Default::default()
    });
    gpu.engines = vec![
        GpuEngine {
            name: "Render/3D".into(),
            kind: GpuEngineKind::Render,
            usage_pct: 29.0 + phase * 8.0,
        },
        GpuEngine {
            name: "Video Decode".into(),
            kind: GpuEngineKind::VideoDecode,
            usage_pct: 7.0 + phase * 3.0,
        },
    ];
    gpu
}

pub(super) fn prepare_gpu_engine_inventory(snapshot: &mut SystemSnapshot) {
    snapshot.gpu = vec![gpu_engine_inventory_frame(
        GPU_ENGINE_CAPTURE_LAST_SAMPLE_INDEX,
        snapshot.timestamp_ms,
    )];
}

pub(super) fn dynamic_power_fixture() -> PowerSupplySnapshot {
    let mut battery = BatteryInfo::new("power-supply:capture-battery", DeviceState::healthy(1_000));
    battery.display_name = "Internal battery".into();
    battery.device_generation = DeviceGeneration::new(1);
    battery.status = "Discharging".into();
    battery.technology = "Li-ion".into();
    battery.model_name = "Capture Battery".into();
    battery.manufacturer = "TaskForest".into();
    battery.apply_scalar_observations(BatteryScalarObservations {
        capacity_pct: ScalarObservation::available(78, 1_000),
        voltage_uv: ScalarObservation::available(12_100_000, 1_000),
        power_w: ScalarObservation::available(14.2, 1_000),
        cycle_count: ScalarObservation::available(142, 1_000),
        ..Default::default()
    });
    PowerSupplySnapshot {
        timestamp_ms: 1_000,
        batteries: vec![battery],
        ..Default::default()
    }
}

pub(super) fn dynamic_sensor_fixture() -> SensorCenterSnapshot {
    let device_id = DeviceId::new("hwmon:capture-fan");
    SensorCenterSnapshot {
        state: DeviceState::healthy(1_000),
        timestamp_ms: 1_000,
        readings: vec![
            sensor_reading(
                device_id.clone(),
                "hwmon:capture-fan:fan1_input",
                "CPU fan",
                SensorDescriptor::fan_speed(SensorScale::IDENTITY),
                SensorMagnitude::Unsigned(1_420),
            )
            .with_device_generation(DeviceGeneration::new(1)),
            sensor_reading(
                device_id,
                "hwmon:capture-fan:temp1_input",
                "CPU package",
                SensorDescriptor::temperature(SensorScale::IDENTITY),
                SensorMagnitude::Decimal(48.5),
            )
            .with_device_generation(DeviceGeneration::new(1)),
        ],
        ..Default::default()
    }
}

pub(super) fn npu_inventory_fixture() -> NpuInventorySnapshot {
    NpuInventorySnapshot::discovered(
        vec![NpuDevice {
            device_id: DeviceId::new("accel:capture-npu0"),
            device_generation: DeviceGeneration::new(1),
            brand: Some("TaskForest Neural Accelerator".into()),
            driver: Some("capture_npu".into()),
            utilization_pct: ScalarObservation::available(42.0, 1_000),
            engines: vec![
                NpuEngineUsage {
                    kind: NpuEngineKind::Matrix,
                    utilization_pct: ScalarObservation::available(37.0, 1_000),
                },
                NpuEngineUsage {
                    kind: NpuEngineKind::Copy,
                    utilization_pct: ScalarObservation::unavailable(FailureKind::Unsupported),
                },
            ],
            memory: NpuMemoryReport {
                dedicated_total_bytes: ScalarObservation::available(0, 1_000),
                shared_total_bytes: ScalarObservation::unavailable(FailureKind::Unsupported),
            },
        }],
        1_000,
    )
}

fn sensor_reading(
    device_id: DeviceId,
    id: &str,
    label: &str,
    descriptor: SensorDescriptor,
    magnitude: SensorMagnitude,
) -> SensorReading {
    let observation = SensorMeasurementObservation::available(descriptor.clone(), magnitude, 1_000)
        .unwrap_or_else(|_| {
            SensorMeasurementObservation::unavailable(descriptor, FailureKind::ProviderFault)
        });
    SensorReading::from_measurement_observation(device_id, id.into(), label.into(), observation)
}

pub(super) fn prepare_missing_tool_disk(snapshot: &mut SystemSnapshot) {
    let now = snapshot.timestamp_ms;
    let disk = ensure_disk(snapshot);
    disk.smart_availability = SmartAvailability::MissingTool;
    disk.smart_state = disk.smart_state.transition(DeviceStatus::MissingTool, now);
    clear_smart_values(disk);
}

fn ensure_disk(snapshot: &mut SystemSnapshot) -> &mut DiskMetrics {
    if snapshot.disks.is_empty() {
        let mut disk = DiskMetrics::new("/dev/capture-smart-fixture");
        disk.device_id = "disk:wwid:capture-fixture".into();
        disk.disk_type = "Capture fixture".into();
        disk.model = "Controlled SMART state".into();
        disk.apply_scalar_observations(DiskScalarObservations {
            capacity_bytes: ScalarObservation::available(512 * 1024 * 1024 * 1024, 1),
            available_bytes: ScalarObservation::available(320 * 1024 * 1024 * 1024, 1),
            ..Default::default()
        });
        snapshot.disks.push(disk);
    }
    &mut snapshot.disks[0]
}

pub(super) fn prepare_permission_disk(snapshot: &mut SystemSnapshot) {
    let now = snapshot.timestamp_ms;
    let disk = ensure_disk(snapshot);
    disk.smart_availability = SmartAvailability::PermissionDenied;
    disk.smart_state = DeviceState::healthy(now.saturating_sub(5_000))
        .transition(DeviceStatus::PermissionDenied, now);
    clear_smart_values(disk);
}

pub(super) fn prepare_partition_disk(snapshot: &mut SystemSnapshot) {
    let disk = ensure_disk(snapshot);
    disk.device_id = "disk:wwid:capture-partition-fixture".into();
    disk.device_generation = DeviceGeneration::new(1);
    disk.name = "/dev/nvme0n1".into();
    disk.disk_type = "NVMe SSD".into();
    disk.model = "Partitioned NVMe fixture".into();
    disk.mount_point = "/".into();
    disk.fs_type = "ext4".into();
    let mut observations = *disk.scalar_observations();
    observations.capacity_bytes = ScalarObservation::available(
        2_000 * 1024 * 1024 * 1024,
        disk.device_state.last_success_ms.unwrap_or(1),
    );
    observations.available_bytes = ScalarObservation::available(
        700 * 1024 * 1024 * 1024,
        disk.device_state.last_success_ms.unwrap_or(1),
    );
    disk.apply_scalar_observations(observations);
    disk.partitions = vec![
        capture_partition(disk, "nvme0n1p1", "/", "ext4", 900, 600, 300),
        capture_partition(
            disk,
            "nvme0n1p2",
            "/mnt/capture/long-mount-point-for-layout-regression/home",
            "btrfs",
            1_000,
            420,
            580,
        ),
        capture_partition(disk, "nvme0n1p3", "", "", 100, 0, 0),
    ];
}

fn capture_partition(
    disk: &DiskMetrics,
    name: &str,
    mount_point: &str,
    fs_type: &str,
    total_gib: u64,
    used_gib: u64,
    free_gib: u64,
) -> DiskPartition {
    let now = disk.device_state.last_success_ms.unwrap_or(1_000);
    let parent_device_id = disk.device_id.clone();
    let mut partition = DiskPartition::new(name);
    partition.device_id = DiskPartition::stable_id(&parent_device_id, name);
    partition.parent_device_id = parent_device_id;
    partition.device_generation = disk.device_generation;
    partition.device_state = DeviceState::healthy(now);
    partition.mount_point = mount_point.into();
    partition.fs_type = fs_type.into();
    let scale = 1024 * 1024 * 1024;
    let observations = if mount_point.is_empty() {
        DiskPartitionScalarObservations {
            capacity_bytes: ScalarObservation::available(total_gib * scale, now),
            ..DiskPartitionScalarObservations::unavailable(crate::core::FailureKind::Unsupported)
        }
    } else {
        DiskPartitionScalarObservations {
            capacity_bytes: ScalarObservation::available(total_gib * scale, now),
            used_bytes: ScalarObservation::available(used_gib * scale, now),
            free_bytes: ScalarObservation::available(free_gib * scale, now),
        }
    };
    partition.apply_scalar_observations(observations);
    partition
}

pub(super) fn prepare_hotplug(snapshot: &mut SystemSnapshot, snapshot_count: u8) {
    let timestamp_ms = snapshot.timestamp_ms;
    if snapshot_count == 1 {
        let disk = ensure_disk(snapshot);
        disk.device_id = "disk:wwid:capture-hotplug".into();
        disk.device_generation = DeviceGeneration::new(1);
        disk.device_state = DeviceState::healthy(timestamp_ms);
        disk.model = "Hot-plug NVMe fixture".into();
    } else if snapshot_count < 6 {
        snapshot.disks.clear();
    } else {
        let disk = ensure_disk(snapshot);
        disk.device_id = "disk:wwid:capture-hotplug".into();
        disk.device_generation = DeviceGeneration::new(2);
        disk.device_state = DeviceState::healthy(timestamp_ms);
        disk.model = "Hot-plug NVMe fixture (reconnected)".into();
    }
}

pub(super) fn prepare_intel_gpu(snapshot: &mut SystemSnapshot) {
    let now = snapshot.timestamp_ms;
    if snapshot.gpu.is_empty() {
        snapshot.gpu.push(GpuMetrics::default());
    }
    let gpu = &mut snapshot.gpu[0];
    gpu.device_id = "gpu:pci:0000:00:02.0".into();
    gpu.device_state = DeviceState::healthy(now);
    gpu.brand = "Intel Arc Graphics (xe)".into();
    gpu.apply_scalar_observations(GpuScalarObservations {
        utilization_pct: ScalarObservation::available(37.0, now),
        frequency_mhz: ScalarObservation::available(1_850, now),
        max_frequency_mhz: ScalarObservation::available(2_250, now),
        idle_residency_pct: ScalarObservation::available(62.0, now),
        ..Default::default()
    });
}

pub(super) fn prepare_active_alert(snapshot: &mut SystemSnapshot) {
    let now = snapshot.timestamp_ms;
    let disk = ensure_disk(snapshot);
    disk.smart_availability = SmartAvailability::Available;
    disk.smart_state = DeviceState::healthy(now);
    disk.smart_critical_warning = Some(true);
}
