//! Fixtures and helpers for Iced capture scenes.

use taskmanager_application::{AppPage, ProcessInsightsProjection, ProcessInsightsRevision};
use taskmanager_core::SystemSnapshot;
use taskmanager_core::core::alerts::{
    Alert, AlertEvent, AlertEventKind, AlertMetric, AlertSeverity,
};
use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::{DeviceGeneration, DeviceId};
use taskmanager_core::core::metrics::{
    DiskPartition, DiskPartitionScalarObservations, GpuEngine, GpuEngineKind, ScalarObservation,
    SmartAvailability,
};
use taskmanager_core::core::npu::{
    NpuDevice, NpuEngineKind, NpuEngineUsage, NpuInventorySnapshot, NpuMemoryReport,
};
use taskmanager_core::core::power::{BatteryInfo, BatteryScalarObservations, PowerSupplySnapshot};
use taskmanager_core::core::process::{
    FrozenProcessIdentity, ProcessItem, ProcessMetadataObservation, ProcessMetadataObservations,
};
use taskmanager_core::core::sensors::{
    SensorCenterSnapshot, SensorDescriptor, SensorMagnitude, SensorMeasurementObservation,
    SensorReading, SensorScale,
};
use taskmanager_core::core::services::{
    ServiceLogEntry, ServiceLogLevel, ServiceLogLevelFilter, ServiceLogQuery,
    ServiceLogStreamSnapshot, ServiceLogStreamState, ServiceLogTimeFilter,
};
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};
use taskmanager_shell::{ProcessRowId, ShellApp};

use super::{DetailsSection, IcedApp, PerfDevice};

pub(super) fn seed_capture_storage_scenario(snapshot: &mut SystemSnapshot, target: &str) {
    if target == "smart-missing-tool" {
        if let Some(disk) = snapshot.disks.first_mut() {
            disk.smart_availability = SmartAvailability::MissingTool;
            disk.smart_state = disk
                .smart_state
                .transition(DeviceStatus::MissingTool, snapshot.timestamp_ms);
        }
    } else if target == "smart-permission" {
        if let Some(disk) = snapshot.disks.first_mut() {
            disk.smart_availability = SmartAvailability::PermissionDenied;
            disk.smart_state = disk
                .smart_state
                .transition(DeviceStatus::PermissionDenied, snapshot.timestamp_ms);
        }
    } else if target == "partition-disk-usage" || target == "partition-live-usage" {
        if let Some(disk) = snapshot.disks.first_mut() {
            let now = snapshot.timestamp_ms;
            let mut p1 = DiskPartition::new("nvme0n1p1");
            p1.mount_point = "/".into();
            p1.fs_type = "ext4".into();
            p1.apply_scalar_observations(DiskPartitionScalarObservations {
                capacity_bytes: ScalarObservation::available(900 * 1024 * 1024 * 1024, now),
                used_bytes: ScalarObservation::available(600 * 1024 * 1024 * 1024, now),
                free_bytes: ScalarObservation::available(300 * 1024 * 1024 * 1024, now),
            });
            let mut p2 = DiskPartition::new("nvme0n1p2");
            p2.mount_point = "/home".into();
            p2.fs_type = "btrfs".into();
            p2.apply_scalar_observations(DiskPartitionScalarObservations {
                capacity_bytes: ScalarObservation::available(1000 * 1024 * 1024 * 1024, now),
                used_bytes: ScalarObservation::available(420 * 1024 * 1024 * 1024, now),
                free_bytes: ScalarObservation::available(580 * 1024 * 1024 * 1024, now),
            });
            disk.partitions = vec![p1, p2];
        }
    } else if (target == "gpu-engine-inventory" || target == "intel-gpu-telemetry")
        && let Some(gpu) = snapshot.gpu.first_mut()
    {
        gpu.brand = "Intel(R) Arc(TM) Graphics".into();
        gpu.engines = vec![
            GpuEngine {
                name: "Render/3D".into(),
                kind: GpuEngineKind::Render,
                usage_pct: 42.0,
            },
            GpuEngine {
                name: "Video Decode".into(),
                kind: GpuEngineKind::VideoDecode,
                usage_pct: 18.0,
            },
            GpuEngine {
                name: "Compute".into(),
                kind: GpuEngineKind::Compute,
                usage_pct: 35.0,
            },
        ];
    }
}

pub(super) fn seed_capture_identity_matrix(processes: &mut Vec<ProcessItem>) {
    let mut p1 = ProcessItem::new(93401, "chrome-mail");
    p1.cmdline = "/opt/google/chrome/chrome --profile-directory=Default --app-id=abc123".into();
    p1.apply_metadata_observations(ProcessMetadataObservations {
        executable_path: ProcessMetadataObservation::available(
            "/opt/google/chrome/chrome".into(),
            1,
        ),
        ..Default::default()
    });
    let mut p2 = ProcessItem::new(93402, "snap-core");
    p2.cmdline = "/snap/core/current/usr/lib/snapd/snapd".into();
    p2.apply_metadata_observations(ProcessMetadataObservations {
        executable_path: ProcessMetadataObservation::available(
            "/snap/core/current/usr/lib/snapd/snapd".into(),
            1,
        ),
        ..Default::default()
    });
    let mut p3 = ProcessItem::new(93403, "appimage-zed");
    p3.cmdline = "/tmp/.mount_zed123/usr/bin/zed".into();
    p3.apply_metadata_observations(ProcessMetadataObservations {
        executable_path: ProcessMetadataObservation::available(
            "/tmp/.mount_zed123/usr/bin/zed".into(),
            1,
        ),
        ..Default::default()
    });
    processes.insert(0, p1);
    processes.insert(1, p2);
    processes.insert(2, p3);
}

pub(super) fn seed_capture_process_target(app: &mut IcedApp) -> Option<FrozenProcessIdentity> {
    let first = app
        .shell
        .projection()
        .processes
        .as_ref()
        .and_then(|processes| processes.first())
        .cloned()?;
    let target = FrozenProcessIdentity::from_process(&first)?;
    app.shell
        .set_row_selection(ProcessRowId::from_process(&first), Some(&first));
    Some(target)
}

pub(super) fn seed_capture_multiple_process_targets(app: &IcedApp) -> Vec<FrozenProcessIdentity> {
    app.shell
        .projection()
        .processes
        .as_ref()
        .map(|processes| {
            processes
                .iter()
                .take(3)
                .filter_map(FrozenProcessIdentity::from_process)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn seed_capture_process_details(app: &mut IcedApp, section: DetailsSection) {
    if let Some(target) = seed_capture_process_target(app) {
        let _ = app.shell.open_process_properties_for(target.clone());
        app.process_presentation.details_section = section;
        if section == DetailsSection::Insights {
            seed_capture_insights_fixture(app, &target);
        }
    }
}

pub(super) fn seed_capture_insights_fixture(app: &mut IcedApp, target: &FrozenProcessIdentity) {
    let revision = ProcessInsightsRevision::new(1);
    let mut tracker = ProcessInsightsProjection::default();
    tracker.begin(target.clone(), revision);
    if let Some(projection) = tracker.snapshot() {
        seed_projection_fact(
            &mut app.shell,
            ProjectionSeedFact::ProcessInsights(Box::new(Some(projection))),
        );
    }
}

pub(super) fn seed_service_log_fixture(shell: &mut ShellApp) {
    let Some(service) = shell.sorted_services().first().cloned() else {
        return;
    };
    let service_id = service.id.clone();
    let _ = shell.open_service_log_for(service_id.clone());
    let lines: &[&str] = &[
        "Started Network Manager.",
        "Reached target Network.",
        "wlan0: link becomes ready",
        "Starting Network Manager Script Dispatcher Service...",
        "Started Network Manager Script Dispatcher Service.",
        "dhcp: lease renewed (3600s)",
        "wlan0: Gained IPv6LL",
    ];
    let base_micros = 1_700_000_000_000_000;
    let entries: Vec<ServiceLogEntry> = lines
        .iter()
        .enumerate()
        .map(|(index, message)| ServiceLogEntry {
            cursor: format!("demo:{index:04}"),
            realtime_timestamp_micros: Some(base_micros + index as u64 * 1_500_000),
            priority: Some(6),
            level: ServiceLogLevel::Unknown,
            message: (*message).to_owned(),
        })
        .collect();
    let query = ServiceLogQuery {
        service_id: service_id.clone(),
        level: ServiceLogLevelFilter::All,
        time: ServiceLogTimeFilter::All,
        after_cursor: None,
    };
    let snapshot = ServiceLogStreamSnapshot {
        query: query.clone(),
        state: ServiceLogStreamState::from_query_entries(&query, entries),
    };
    if let Some(open) = shell.service_log.as_mut() {
        open.feed.apply_at(snapshot, 1_700_000_000_000);
    }
}

pub(super) fn seed_capture_npu_fixture(app: &mut IcedApp) {
    let observed_at_ms = 7_000;
    let inventory = NpuInventorySnapshot::discovered(
        vec![NpuDevice {
            device_id: DeviceId::new("accel0"),
            brand: Some("Intel AI Boost".into()),
            driver: Some("intel_vpu".into()),
            utilization_pct: ScalarObservation::available(38.0, observed_at_ms),
            engines: vec![NpuEngineUsage {
                kind: NpuEngineKind::Matrix,
                utilization_pct: ScalarObservation::available(61.0, observed_at_ms),
            }],
            memory: NpuMemoryReport {
                dedicated_total_bytes: ScalarObservation::available(0, observed_at_ms),
                shared_total_bytes: ScalarObservation::unavailable(FailureKind::Unsupported),
                sram_total_bytes: ScalarObservation::available(32 * 1024 * 1024, observed_at_ms),
            },
            ..Default::default()
        }],
        observed_at_ms,
    );
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::NpuInventory(Some(inventory)),
    );
}

pub(super) fn capture_device_from_name(name: &str) -> Option<PerfDevice> {
    match name {
        "cpu" => Some(PerfDevice::Cpu),
        "memory" => Some(PerfDevice::Memory),
        "disk" => Some(PerfDevice::Disk(0)),
        "network" => Some(PerfDevice::Network(0)),
        "gpu" => Some(PerfDevice::Gpu(0)),
        "npu" => Some(PerfDevice::Npu(0)),
        "battery" => Some(PerfDevice::Battery(0)),
        "fan" => Some(PerfDevice::Fan(0)),
        _ => None,
    }
}

pub(super) fn capture_page_from_name(name: &str) -> Option<AppPage> {
    match name {
        "applications" => Some(AppPage::Applications),
        "services" => Some(AppPage::Services),
        "startup" => Some(AppPage::Startup),
        "users" => Some(AppPage::Users),
        "system" => Some(AppPage::System),
        "app-history" => Some(AppPage::AppHistory),
        _ => None,
    }
}

pub(super) fn capture_event_fixture() -> Vec<AlertEvent> {
    let warning = Alert {
        instance_id: "capture-cpu:system".into(),
        rule_id: "capture-cpu".into(),
        target: "CPU".into(),
        metric: AlertMetric::CpuUsagePercent,
        severity: AlertSeverity::Warning,
        value: 93.0,
        threshold: 90.0,
        active_since_ms: 3_590_000,
    };
    let activated = AlertEvent {
        id: 1,
        kind: AlertEventKind::Activated,
        alert: warning.clone(),
        observed_at_ms: 3_590_000,
    };
    let mut cleared = warning;
    cleared.instance_id = "capture-memory:system".into();
    cleared.rule_id = "capture-memory".into();
    cleared.metric = AlertMetric::MemoryUsagePercent;
    cleared.target = "Memory".into();
    cleared.value = 74.0;
    vec![
        activated,
        AlertEvent {
            id: 2,
            kind: AlertEventKind::Cleared,
            alert: cleared,
            observed_at_ms: 3_560_000,
        },
    ]
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
