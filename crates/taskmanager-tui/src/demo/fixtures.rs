//! Deterministic fixtures for TUI demo and capture scene composition.

use taskmanager_application::{
    CorrelatedEvent, NpuInventoryEvent, PlatformEventBatch, PlatformEventContext,
};
use taskmanager_core::SystemSnapshot;
use taskmanager_core::core::alerts::{
    Alert, AlertEvent, AlertEventKind, AlertMetric, AlertSeverity,
};
use taskmanager_core::core::device_state::DeviceState;
use taskmanager_core::core::directory_usage::{
    DirectoryScanId, DirectoryScanStatus, DirectoryScanTotals, DirectoryUsageEntry,
    DirectoryUsageSnapshot,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::{DeviceGeneration, DeviceId, ProviderId};
use taskmanager_core::core::metrics::ScalarObservation;
use taskmanager_core::core::metrics::{GpuEngine, GpuEngineKind};
use taskmanager_core::core::npu::{
    NpuDevice, NpuEngineKind, NpuEngineUsage, NpuInventorySnapshot, NpuMemoryReport,
};
use taskmanager_core::core::process::ProcessItem;
use taskmanager_core::core::sensors::{
    SensorCenterSnapshot, SensorDescriptor, SensorMagnitude, SensorMeasurementObservation,
    SensorReading, SensorScale,
};
use taskmanager_core::core::startup::{StartupBootEvidenceSnapshot, StartupCriticalChainNode};
use taskmanager_platform_contract::{CapabilityId, EventSequence, RequestId};
use taskmanager_shell::ShellApp;
use taskmanager_shell::fixture::{
    ProjectionSeedFact, record_demo_history_frame, seed_projection_fact,
};

use crate::TuiApp;

pub(super) fn seed_demo_npu_inventory(app: &mut TuiApp) {
    const OBSERVED_AT_MS: u64 = 1_785_292_800_000;
    let engines = NpuEngineKind::ALL
        .iter()
        .copied()
        .enumerate()
        .map(|(index, kind)| NpuEngineUsage {
            kind,
            utilization_pct: ScalarObservation::available(
                11.0 + (index as f32 * 7.0),
                OBSERVED_AT_MS,
            ),
        })
        .collect();
    let inventory = NpuInventorySnapshot::discovered(
        vec![NpuDevice {
            device_id: DeviceId::new("accel0"),
            device_generation: DeviceGeneration::new(1),
            brand: Some("Intel AI Boost".into()),
            driver: Some("intel_vpu".into()),
            utilization_pct: ScalarObservation::available(44.0, OBSERVED_AT_MS),
            engines,
            memory: NpuMemoryReport {
                dedicated_total_bytes: ScalarObservation::available(
                    512 * 1024 * 1024,
                    OBSERVED_AT_MS,
                ),
                shared_total_bytes: ScalarObservation::available(
                    4 * 1024 * 1024 * 1024,
                    OBSERVED_AT_MS,
                ),
                sram_total_bytes: ScalarObservation::available(32 * 1024 * 1024, OBSERVED_AT_MS),
            },
        }],
        OBSERVED_AT_MS,
    );
    let context = PlatformEventContext {
        request_id: RequestId::MIN,
        capability: CapabilityId::ACCELERATOR_NPU,
        provider: Some(ProviderId::borrowed("fixture.npu")),
        sequence: EventSequence::new(1),
        observed_at_ms: OBSERVED_AT_MS,
    };
    let mut batch = PlatformEventBatch::default();
    batch.npu_inventory_events.push(CorrelatedEvent::new(
        context,
        NpuInventoryEvent::Update(inventory),
    ));
    app.apply_platform_batch(batch);
}

pub(super) fn seed_gpu_capture_history(app: &mut TuiApp) {
    let Some(mut snapshot) = app.projection().snapshot.clone() else {
        return;
    };
    let Some(gpu) = snapshot.gpu.first_mut() else {
        return;
    };
    gpu.engines = vec![
        GpuEngine {
            name: "Render/3D".into(),
            kind: GpuEngineKind::Render,
            usage_pct: 0.0,
        },
        GpuEngine {
            name: "Video Decode".into(),
            kind: GpuEngineKind::VideoDecode,
            usage_pct: 0.0,
        },
    ];
    for (index, (utilization, render, video)) in [
        (12.0, 9.0, 3.0),
        (27.0, 21.0, 8.0),
        (46.0, 39.0, 12.0),
        (34.0, 28.0, 7.0),
        (61.0, 52.0, 18.0),
    ]
    .into_iter()
    .enumerate()
    {
        let observed_at_ms = 1_785_292_800_100_u64.saturating_add(index as u64 * 1_000);
        snapshot.timestamp_ms = observed_at_ms;
        let gpu = &mut snapshot.gpu[0];
        let mut observations = *gpu.scalar_observations();
        observations.utilization_pct = ScalarObservation::available(utilization, observed_at_ms);
        gpu.apply_scalar_observations(observations);
        gpu.engines[0].usage_pct = render;
        gpu.engines[1].usage_pct = video;
        record_demo_history_frame(&mut app.shell, &snapshot, None, None);
    }
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
    );
}

pub(crate) fn seed_fan_capture_sensors(app: &mut TuiApp) {
    const OBSERVED_AT_MS: u64 = 1_785_292_800_000;
    let fan = SensorMeasurementObservation::available(
        SensorDescriptor::fan_speed(SensorScale::IDENTITY),
        SensorMagnitude::Unsigned(2_400),
        OBSERVED_AT_MS,
    );
    let package = SensorMeasurementObservation::available(
        SensorDescriptor::temperature(SensorScale::IDENTITY),
        SensorMagnitude::Decimal(51.0),
        OBSERVED_AT_MS,
    );
    let acpitz = SensorMeasurementObservation::available(
        SensorDescriptor::temperature(SensorScale::IDENTITY),
        SensorMagnitude::Decimal(61.0),
        OBSERVED_AT_MS,
    );
    let (Ok(fan), Ok(package), Ok(acpitz)) = (fan, package, acpitz) else {
        return;
    };
    let reading =
        |device: &str, id: &str, label: &str, observation: SensorMeasurementObservation| {
            SensorReading::from_measurement_observation(
                device.into(),
                id.into(),
                label.into(),
                observation,
            )
            .with_device_generation(DeviceGeneration::new(1))
        };
    let readings = vec![
        reading("hwmon:cpu", "cpu_fan", "cpu_fan", fan),
        reading("hwmon:cpu", "cpu_package", "Package", package),
        reading("thermal:acpitz", "acpitz", "acpitz", acpitz),
        reading(
            "thermal:nvme0",
            "nvme0",
            "nvme0",
            SensorMeasurementObservation::unavailable(
                SensorDescriptor::temperature(SensorScale::IDENTITY),
                FailureKind::PermissionDenied,
            ),
        ),
    ];
    seed_projection_fact(
        &mut app.shell,
        ProjectionSeedFact::Sensors(Some(SensorCenterSnapshot {
            state: DeviceState::healthy(OBSERVED_AT_MS),
            timestamp_ms: OBSERVED_AT_MS,
            readings,
            ..Default::default()
        })),
    );
}

pub(super) fn demo_directory_usage() -> DirectoryUsageSnapshot {
    let observed_at_ms = 1_785_292_800_000;
    let readable = DirectoryUsageEntry {
        path: "lib/postgres".into(),
        depth: 1,
        size_bytes: ScalarObservation::available(2 * 1024 * 1024 * 1024, observed_at_ms),
        file_count: ScalarObservation::available(4200, observed_at_ms),
        unreadable: None,
    };
    let unreadable = DirectoryUsageEntry {
        path: "cache/private".into(),
        depth: 1,
        size_bytes: ScalarObservation::available(7 * 1024 * 1024 * 1024, observed_at_ms),
        file_count: ScalarObservation::available(900, observed_at_ms),
        unreadable: Some(FailureKind::PermissionDenied),
    };
    DirectoryUsageSnapshot {
        scan_id: DirectoryScanId::new(1),
        root: "/var".into(),
        status: DirectoryScanStatus::Completed,
        entries: vec![readable, unreadable],
        totals: DirectoryScanTotals {
            directories_visited: 7,
            files_counted: 42,
            unreadable_directories: 1,
            bytes_counted: ScalarObservation::available(2 * 1024 * 1024 * 1024, observed_at_ms),
            depth_reached: 1,
            capped: true,
        },
    }
}

pub(super) fn demo_boot_evidence() -> StartupBootEvidenceSnapshot {
    let healthy = DeviceState::healthy(1_785_292_800_000);
    StartupBootEvidenceSnapshot {
        state: healthy,
        failed_units_state: healthy,
        critical_chain_state: healthy,
        failed_units_failure: None,
        critical_chain_failure: None,
        failed_units: Vec::new(),
        critical_chain: vec![
            StartupCriticalChainNode {
                unit: "dbus.service".into(),
                activated_at_ms: Some(500),
                duration_ms: Some(1_200),
            },
            StartupCriticalChainNode {
                unit: "graphical.target".into(),
                activated_at_ms: None,
                duration_ms: None,
            },
            StartupCriticalChainNode {
                unit: "multi-user.target".into(),
                activated_at_ms: Some(2_600),
                duration_ms: Some(2_500),
            },
        ],
    }
}

pub(super) fn seed_service_log_fixture(shell: &mut ShellApp) {
    use taskmanager_core::core::services::{
        ServiceLogEntry, ServiceLogLevel, ServiceLogLevelFilter, ServiceLogQuery,
        ServiceLogStreamSnapshot, ServiceLogStreamState, ServiceLogTimeFilter,
    };
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

pub(super) fn seed_capture_storage_scenario(snapshot: &mut SystemSnapshot, target: &str) {
    use taskmanager_core::core::device_state::DeviceStatus;
    use taskmanager_core::core::metrics::{
        DiskPartition, DiskPartitionScalarObservations, GpuEngine, GpuEngineKind, SmartAvailability,
    };
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
    } else if target == "gpu-engine-inventory" || target == "intel-gpu-telemetry" {
        if let Some(gpu) = snapshot.gpu.first_mut() {
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
}

pub(super) fn seed_capture_identity_matrix(processes: &mut Vec<ProcessItem>) {
    use taskmanager_core::core::process::{
        ProcessMetadataObservation, ProcessMetadataObservations,
    };
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

pub(super) fn seed_alert_event_history_fixture(shell: &mut ShellApp) {
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
    shell.replace_alert_event_history(vec![
        activated,
        AlertEvent {
            id: 2,
            kind: AlertEventKind::Cleared,
            alert: cleared,
            observed_at_ms: 3_560_000,
        },
    ]);
}
