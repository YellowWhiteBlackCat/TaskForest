//! Capture-only shell seed for the deterministic demo window.
//!
//! The visual-contract capture needs a populated curve window, so the demo
//! composition records a short adjacent telemetry sequence on top of the
//! shared `demo_app()` fixture. This module is a pure data fixture: it owns
//! no UI tree and paints nothing — it only feeds the shared shell the same
//! typed observations a real collection frame would, which keeps it out of
//! the renderer/fold boundary the display gate pins (ARCH.md §8.1).
//!
//! Production collection and the shared fixture's semantics are untouched:
//! `demo_app` starts with one honest sample and the sequence below is
//! appended only inside the capture composition.

use taskmanager_application::{
    AppAction, InteractionEvent, PendingConfirmation, ProcessInsightsProjection,
    ProcessInsightsRevision,
};
use taskmanager_core::core::DeviceGeneration;
use taskmanager_core::core::StorageDeviceKey;
use taskmanager_core::core::alerts::{
    Alert, AlertEvent, AlertEventKind, AlertMetric, AlertSeverity,
};
use taskmanager_core::core::device_state::{DeviceState, DeviceStatus};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::identity::DeviceId;
use taskmanager_core::core::metrics::{
    CpuMetrics, DiskPartition, DiskPartitionScalarObservations, GpuEngine, GpuEngineKind,
    ScalarObservation, ScalarObservationGroup, SmartAvailability,
};
use taskmanager_core::core::npu::{
    NpuDevice, NpuEngineKind, NpuEngineUsage, NpuInventorySnapshot, NpuMemoryReport,
};
use taskmanager_core::core::process::{
    FrozenProcessIdentity, ProcessBatchAction, ProcessBatchIntent, ProcessGroupScope, ProcessItem,
};
use taskmanager_core::core::sensors::{
    SensorCenterSnapshot, SensorDescriptor, SensorMagnitude, SensorMeasurementObservation,
    SensorReading, SensorScale,
};
use taskmanager_core::core::services::{
    ServiceLogEntry, ServiceLogLevel, ServiceLogLevelFilter, ServiceLogQuery,
    ServiceLogStreamSnapshot, ServiceLogStreamState, ServiceLogTimeFilter,
};
use taskmanager_core::core::smart::SmartSelfTestKind;
use taskmanager_core::core::system_health::SmartSelfTestIntent;
use taskmanager_shell::ShellApp;
use taskmanager_shell::demo_app;
use taskmanager_shell::fixture::{
    ProjectionSeedFact, record_demo_history_frame, seed_projection_fact,
};

/// Build the capture-only shell with a warm, deterministic graph window.
pub(crate) fn demo_shell() -> ShellApp {
    let mut shell = demo_app();
    if let Some(seed) = shell.projection().snapshot.clone() {
        for offset in 1..=24_u64 {
            let mut next = seed.clone();
            next.timestamp_ms = next
                .timestamp_ms
                .saturating_add(offset.saturating_mul(1_000));
            next.cpu = demo_cpu_frame(&next.cpu, next.timestamp_ms, offset);
            record_demo_history_frame(&mut shell, &next, None, None);
        }
    }
    shell
}

/// One adjacent CPU frame: the seed's identity and shape with a deterministic
/// utilization sequence applied to the global and per-core observations.
fn demo_cpu_frame(seed: &CpuMetrics, timestamp_ms: u64, offset: u64) -> CpuMetrics {
    let usage = if offset == 24 {
        37.4
    } else {
        24.0 + f32::from((offset.saturating_mul(7) % 26) as u8)
    };
    let mut observations = seed.scalar_observations().clone();
    observations.global_usage_pct = ScalarObservation::available(usage, timestamp_ms);
    let core_values = (0..seed.current_core_usage_len())
        .map(|index| (usage + (index as f32 * 4.0) - (offset % 3) as f32 * 2.0).clamp(0.0, 100.0))
        .collect();
    observations.core_usage_group = ScalarObservationGroup::available(core_values, timestamp_ms);
    let mut frame = CpuMetrics::from_observations(observations);
    frame.brand = seed.brand.clone();
    frame.frequency_source = seed.frequency_source;
    frame.temperature_source = seed.temperature_source;
    frame.physical_cores = seed.physical_cores;
    frame.logical_cores = seed.logical_cores;
    frame.l1d_cache_kb = seed.l1d_cache_kb;
    frame.l1i_cache_kb = seed.l1i_cache_kb;
    frame.l2_cache_kb = seed.l2_cache_kb;
    frame.l3_cache_kb = seed.l3_cache_kb;
    frame.performance_policy = seed.performance_policy.clone();
    frame.packages = seed.packages.clone();
    frame.idle_states = seed.idle_states.clone();
    frame.interrupts = seed.interrupts.clone();
    frame
}

pub(crate) fn seed_service_log_fixture(shell: &mut ShellApp) {
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
        "device (wlan0): state change: activated -> deactivating",
        "device (wlan0): state change: deactivating -> disconnected",
        "wlan0: link is not ready",
        "device (wlan0): state change: disconnected -> prepare",
        "device (wlan0): supplicant interface state: scanning -> authenticating",
        "device (wlan0): supplicant interface state: authenticating -> associating",
        "device (wlan0): supplicant interface state: associating -> 4way_handshake",
        "device (wlan0): supplicant interface state: 4way_handshake -> completed",
        "wlan0: link becomes ready",
        "device (wlan0): state change: config -> activated",
        "dhcp: request granted",
        "address added: 192.168.1.42/24",
        "route added: default via 192.168.1.1",
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

pub(crate) fn seed_capture_confirmation_fixture(shell: &mut ShellApp) {
    let raw = std::env::var("TM_BEVY_CAPTURE_PAGE").unwrap_or_default();
    seed_capture_confirmation_scenario(shell, raw.trim());
}

pub(crate) fn seed_capture_confirmation_scenario(shell: &mut ShellApp, scenario: &str) {
    match scenario.to_ascii_lowercase().as_str() {
        "process-force-kill" => {
            let process = shell
                .projection()
                .processes
                .as_ref()
                .and_then(|p| p.first())
                .cloned();
            if let Some(process) = process
                && let Some(target) = FrozenProcessIdentity::from_process(&process)
            {
                let intent = ProcessBatchIntent {
                    action: ProcessBatchAction::Kill,
                    scope: ProcessGroupScope::PidAdjacency,
                    targets: vec![target],
                };
                let _ = shell
                    .application
                    .interaction
                    .reduce(InteractionEvent::ArmConfirmation(
                        PendingConfirmation::ProcessBatch(intent),
                    ));
            }
        }
        "process-tree-confirm" => {
            let process = shell
                .projection()
                .processes
                .as_ref()
                .and_then(|p| p.first())
                .cloned();
            if let Some(process) = process
                && let Some(target) = FrozenProcessIdentity::from_process(&process)
            {
                let intent = ProcessBatchIntent {
                    action: ProcessBatchAction::EndProcessTree,
                    scope: ProcessGroupScope::PidAdjacency,
                    targets: vec![target],
                };
                let _ = shell
                    .application
                    .interaction
                    .reduce(InteractionEvent::ArmConfirmation(
                        PendingConfirmation::ProcessBatch(intent),
                    ));
            }
        }
        "process-batch-confirm" => {
            let targets: Vec<_> = shell
                .projection()
                .processes
                .as_ref()
                .map(|p| {
                    p.iter()
                        .take(3)
                        .filter_map(FrozenProcessIdentity::from_process)
                        .collect()
                })
                .unwrap_or_default();
            if !targets.is_empty() {
                let intent = ProcessBatchIntent {
                    action: ProcessBatchAction::Kill,
                    scope: ProcessGroupScope::PidAdjacency,
                    targets,
                };
                let _ = shell
                    .application
                    .interaction
                    .reduce(InteractionEvent::ArmConfirmation(
                        PendingConfirmation::ProcessBatch(intent),
                    ));
            }
        }
        "smart-self-test-confirm" => {
            let intent = SmartSelfTestIntent {
                device_id: DeviceId::new("disk:demo:nvme0"),
                device_generation: DeviceGeneration::new(1),
                device_key: StorageDeviceKey::new("nvme0n1"),
                display_name: "TiPro9000 2TB".into(),
                kind: SmartSelfTestKind::Short,
            };
            shell.arm_smart_self_test(intent);
        }
        "process-network-details"
        | "process-gpu-details"
        | "process-resource-limits"
        | "process-isolation"
        | "process-properties-performance"
        | "process-memory-pss-swap" => {
            let process = shell
                .projection()
                .processes
                .as_ref()
                .and_then(|p| p.first())
                .cloned();
            if let Some(process) = process
                && let Some(target) = FrozenProcessIdentity::from_process(&process)
            {
                shell.application.selected_process = Some(target.clone());
                let revision = ProcessInsightsRevision::new(1);
                let mut tracker = ProcessInsightsProjection::default();
                tracker.begin(target, revision);
                if let Some(projection) = tracker.snapshot() {
                    seed_projection_fact(
                        shell,
                        ProjectionSeedFact::ProcessInsights(Box::new(Some(projection))),
                    );
                }
            }
        }
        "services-search-highlight" => {
            shell.query = "Network".into();
        }
        "apps-search-highlight" => {
            shell.query = "zed".into();
        }
        "telemetry-paused" => {
            let _ = shell.apply_action(AppAction::TogglePause);
        }
        "system-npu" => {
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
                        shared_total_bytes: ScalarObservation::unavailable(
                            FailureKind::Unsupported,
                        ),
                        sram_total_bytes: ScalarObservation::available(
                            32 * 1024 * 1024,
                            observed_at_ms,
                        ),
                    },
                    ..Default::default()
                }],
                observed_at_ms,
            );
            seed_projection_fact(shell, ProjectionSeedFact::NpuInventory(Some(inventory)));
        }
        "sensor-center" => {
            const OBSERVED_AT_MS: u64 = 7_000;
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
            if let (Ok(fan), Ok(package), Ok(acpitz)) = (fan, package, acpitz) {
                let reading = |device: &str, id: &str, label: &str, obs| {
                    SensorReading::from_measurement_observation(
                        device.into(),
                        id.into(),
                        label.into(),
                        obs,
                    )
                    .with_device_generation(DeviceGeneration::new(1))
                };
                let readings = vec![
                    reading("hwmon:cpu", "cpu_fan", "cpu_fan", fan),
                    reading("hwmon:cpu", "cpu_package", "Package", package),
                    reading("thermal:acpitz", "acpitz", "acpitz", acpitz),
                ];
                seed_projection_fact(
                    shell,
                    ProjectionSeedFact::Sensors(Some(SensorCenterSnapshot {
                        state: DeviceState::healthy(OBSERVED_AT_MS),
                        timestamp_ms: OBSERVED_AT_MS,
                        readings,
                        ..Default::default()
                    })),
                );
            }
        }
        "active-alert" | "alert-rules-manager" => {
            let alert = Alert {
                instance_id: "cpu-high:all".into(),
                rule_id: "cpu-high".into(),
                target: "all".into(),
                metric: AlertMetric::CpuUsagePercent,
                severity: AlertSeverity::Warning,
                value: 94.0,
                threshold: 90.0,
                active_since_ms: 1_700_000_000_000,
            };
            seed_projection_fact(shell, ProjectionSeedFact::ActiveAlerts(vec![alert]));
        }
        "smart-missing-tool"
        | "smart-permission"
        | "partition-disk-usage"
        | "partition-live-usage" => {
            if let Some(snapshot) = shell.projection().snapshot.as_ref() {
                let mut s = (*snapshot).clone();
                if scenario == "smart-missing-tool" {
                    if let Some(disk) = s.disks.first_mut() {
                        disk.smart_availability = SmartAvailability::MissingTool;
                        disk.smart_state = disk
                            .smart_state
                            .transition(DeviceStatus::MissingTool, s.timestamp_ms);
                    }
                } else if scenario == "smart-permission" {
                    if let Some(disk) = s.disks.first_mut() {
                        disk.smart_availability = SmartAvailability::PermissionDenied;
                        disk.smart_state = disk
                            .smart_state
                            .transition(DeviceStatus::PermissionDenied, s.timestamp_ms);
                    }
                } else if let Some(disk) = s.disks.first_mut() {
                    let now = s.timestamp_ms;
                    let mut p1 = DiskPartition::new("nvme0n1p1");
                    p1.mount_point = "/".into();
                    p1.fs_type = "ext4".into();
                    p1.apply_scalar_observations(DiskPartitionScalarObservations {
                        capacity_bytes: ScalarObservation::available(900 * 1024 * 1024 * 1024, now),
                        used_bytes: ScalarObservation::available(600 * 1024 * 1024 * 1024, now),
                        free_bytes: ScalarObservation::available(300 * 1024 * 1024 * 1024, now),
                    });
                    disk.partitions = vec![p1];
                }
                seed_projection_fact(shell, ProjectionSeedFact::Snapshot(Box::new(Some(s))));
            }
        }
        "gpu-engine-inventory" | "intel-gpu-telemetry" => {
            if let Some(snapshot) = shell.projection().snapshot.as_ref() {
                let mut s = (*snapshot).clone();
                if let Some(gpu) = s.gpu.first_mut() {
                    gpu.brand = "Intel(R) Arc(TM) Graphics".into();
                    gpu.engines = vec![GpuEngine {
                        name: "Render/3D".into(),
                        kind: GpuEngineKind::Render,
                        usage_pct: 42.0,
                    }];
                }
                seed_projection_fact(shell, ProjectionSeedFact::Snapshot(Box::new(Some(s))));
            }
        }
        "apps-identity-matrix" => {
            let mut procs = shell
                .projection()
                .processes
                .as_ref()
                .map(|p| (**p).clone())
                .unwrap_or_default();
            let mut p1 = ProcessItem::new(93401, "chrome-mail");
            p1.cmdline =
                "/opt/google/chrome/chrome --profile-directory=Default --app-id=abc123".into();
            let mut p2 = ProcessItem::new(93402, "snap-core");
            p2.cmdline = "/snap/core/current/usr/lib/snapd/snapd".into();
            procs.insert(0, p1);
            procs.insert(1, p2);
            seed_projection_fact(shell, ProjectionSeedFact::Processes(Some(procs)));
        }
        "event-center" => {
            shell.replace_alert_event_history(capture_event_fixture());
        }
        _ => {}
    }
}

pub(crate) fn capture_event_fixture() -> Vec<AlertEvent> {
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
