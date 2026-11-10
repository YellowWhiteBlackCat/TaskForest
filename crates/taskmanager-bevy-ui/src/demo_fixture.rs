//! Capture-only shell seeds for the deterministic demo window.
//!
//! The warm graph history now belongs to the shared `demo_app()` fixture, so
//! Bevy and GPUI capture the same accepted frames through the same correlated
//! ingestor. This module owns only scenario-specific shell state (alerts,
//! confirmations, service logs, dynamic device snapshots); it no longer
//! appends a renderer-private telemetry sequence.

use taskmanager_shell::fixture::alerts::seed_shell_active_alert;
use taskmanager_shell::fixture::health::{
    seed_shell_health, seed_smart_self_test_capabilities, seed_wear_critical_disk,
};
use taskmanager_shell::fixture::process_insights::{
    process_insights_projection, seed_process_properties_history,
};
use taskmanager_shell::fixture::process_tree::seed_shell_process_tree;
use taskmanager_shell::fixture::startup::startup_failure_evidence;
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

use taskforest_core::core::DeviceGeneration;
use taskforest_core::core::alerts::{
    Alert, AlertEventKind, AlertInstanceId, AlertMetric, AlertRuleId, AlertSeverity,
    AlertTargetIdentity,
};
use taskforest_core::core::device_state::{DeviceState, DeviceStatus};
use taskforest_core::core::failure::FailureKind;
use taskforest_core::core::identity::DeviceId;
use taskforest_core::core::identity::ProviderId;
use taskforest_core::core::metrics::{
    DiskMetrics, DiskPartition, DiskPartitionScalarObservations, GpuEngine, GpuEngineKind,
    ScalarObservation, SmartAvailability,
};
use taskforest_core::core::npu::{
    NpuDevice, NpuEngineKind, NpuEngineUsage, NpuInventorySnapshot, NpuMemoryReport,
};
use taskforest_core::core::power::{BatteryInfo, BatteryScalarObservations, PowerSupplySnapshot};
use taskmanager_application::AlertEventSeed;
use taskmanager_application::{AppAction, AppPage, InteractionEvent, PendingConfirmation};
mod dynamic_history;
use taskforest_core::core::process::{
    ContainerMetadata, ContainerRuntime, ProcessMetadataObservations, ProcessOwner,
};
use taskforest_core::core::process::{
    FrozenProcessIdentity, ProcessBatchAction, ProcessBatchIntent, ProcessGroupScope, ProcessItem,
};
use taskforest_core::core::sensors::{
    SensorCenterSnapshot, SensorDescriptor, SensorMagnitude, SensorMeasurementObservation,
    SensorReading, SensorScale,
};
use taskforest_core::core::services::{
    ServiceLogEntry, ServiceLogLevel, ServiceLogLevelFilter, ServiceLogQuery,
    ServiceLogStreamSnapshot, ServiceLogStreamState, ServiceLogTimeFilter,
};
use taskforest_core::core::session::{
    SessionControlAction, SessionFamily, SessionFamilyFact, SessionIncarnation, SessionItem,
};
use taskforest_core::core::smart::SmartSelfTestKind;
use taskforest_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_shell::demo_app;
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};
use taskmanager_shell::presentation::smart::self_test_intent;
use taskmanager_shell::{ProcessRowId, ShellApp};

/// Build the capture-only shell from the shared warm demo history.
pub(crate) fn demo_shell() -> ShellApp {
    demo_app()
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
        "startup-failure-evidence" => {
            seed_projection_fact(
                shell,
                ProjectionSeedFact::StartupBootEvidence(Some(startup_failure_evidence(3_600_000))),
            );
        }
        "process-selection" => {
            let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
            let process = shell
                .projection()
                .processes
                .as_ref()
                .and_then(|rows| rows.first())
                .cloned();
            if let Some(process) = process {
                shell.set_row_selection(ProcessRowId::from_process(&process), Some(&process));
            }
        }
        "process-context-menu" => {
            // Anchor the action menu on a real selected row: the menu freezes
            // that row's live identity at open time, so a capture without a
            // selection would fail closed and paint nothing.
            let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
            let process = shell
                .projection()
                .processes
                .as_ref()
                .and_then(|rows| rows.first())
                .cloned();
            if let Some(process) = process {
                shell.set_row_selection(ProcessRowId::from_process(&process), Some(&process));
            }
        }
        "feedback-overlay" => {
            shell.report_notice(
                FeedbackSource::Persistence,
                FeedbackSeverity::Success,
                FeedbackLifecycle::UntilReplaced,
                format!(
                    "Current window saved: /run/media/{}/very-long-workspace/target/capture/runs/{}/capture.png (1180x780)",
                    "capture-user", "977f44d34-bc94-4c7d-9d89-0f6a6b0a62e9",
                ),
            );
        }
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
                    collateral: Default::default(),
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
            if let Some(root) = seed_shell_process_tree(shell) {
                shell.request_process_tree_end(root);
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
                    collateral: Default::default(),
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
            seed_smart_self_test_capabilities(shell);
            if let Some(intent) = shell
                .projection()
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.disks.first())
                .and_then(|disk| self_test_intent(disk, SmartSelfTestKind::Short))
            {
                let _ = shell.arm_smart_self_test(intent);
            }
        }
        "users-session-confirmation" => {
            let session = SessionItem {
                id: "capture-session-7".into(),
                incarnation: Some(SessionIncarnation::LinuxLogind {
                    boot_id: "capture-boot-identity".into(),
                    created_monotonic_usec: 700_000,
                }),
                uid: 1000,
                user: "capture-user".into(),
                seat: Some("seat0".into()),
                tty: Some("tty7".into()),
                timestamp: Some("2026-10-12 09:30".into()),
                family: SessionFamilyFact::Observed(SessionFamily::Console),
            };
            seed_projection_fact(
                shell,
                ProjectionSeedFact::Sessions(Some(vec![session.clone()])),
            );
            seed_projection_fact(
                shell,
                ProjectionSeedFact::SessionsSource(Some(vec![SourceStatus {
                    provider: ProviderId::borrowed("capture.sessions"),
                    outcome: SourceOutcome::Available,
                    item_count: 1,
                }])),
            );
            let _ = shell.select_session_control(&session, SessionControlAction::Disconnect);
        }
        "process-network-details"
        | "process-gpu-details"
        | "process-resource-limits"
        | "process-isolation"
        | "process-properties-performance"
        | "process-memory-pss-swap" => {
            let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
            seed_process_properties_history(shell);
            let process = shell
                .projection()
                .processes
                .as_ref()
                .and_then(|p| p.first())
                .cloned();
            if let Some(process) = process
                && let Some(target) = FrozenProcessIdentity::from_process(&process)
            {
                shell.set_row_selection(ProcessRowId::from_process(&process), Some(&process));
                if let Some(projection) = process_insights_projection(target) {
                    seed_projection_fact(
                        shell,
                        ProjectionSeedFact::ProcessInsights(Box::new(Some(projection))),
                    );
                }
                let _ = shell.apply_action(AppAction::OpenProperties);
            }
        }
        "apps-container-group" | "apps-container-group-compact" => {
            let _ = shell.apply_action(AppAction::SelectPage(AppPage::Applications));
            if let Some(root) = seed_shell_process_tree(shell) {
                let _ = root;
            }
            let mut processes = shell
                .projection()
                .processes
                .as_deref()
                .cloned()
                .unwrap_or_default();
            let mut tagged = 0;
            for process in processes
                .iter_mut()
                .filter(|process| (90_001..=90_003).contains(&process.pid))
            {
                let metadata = ContainerMetadata::new(
                    ContainerRuntime::Docker,
                    "a1b2c3d4e5f678901234567890abcdef",
                )
                .with_name("capture-web");
                process.apply_metadata_observations(
                    ProcessMetadataObservations::current_with_container(
                        ProcessOwner::opaque("capture-user"),
                        None,
                        Some(metadata),
                        1,
                    ),
                );
                tagged += 1;
            }
            if tagged == 3 {
                seed_projection_fact(shell, ProjectionSeedFact::Processes(Some(processes)));
            }
        }
        "services-search-highlight" => {
            shell.set_services_query("Network");
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
        "storage-health" | "sensor-center" => {
            seed_shell_health(shell);
        }
        "active-alert" | "alert-rules-manager" => {
            let _ = seed_shell_active_alert(shell);
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
        "smart-wear-critical" => {
            // The disk whose endurance reached its rated life: the critical
            // marker rides every surface that paints the endurance, and this
            // is the disk that proves it.
            if let Some(mut snapshot) = shell.projection().snapshot.clone() {
                seed_wear_critical_disk(&mut snapshot);
                seed_projection_fact(
                    shell,
                    ProjectionSeedFact::Snapshot(Box::new(Some(snapshot))),
                );
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
        "battery-fan-performance" | "fan-performance" => {
            dynamic_history::seed(shell);
            seed_projection_fact(
                shell,
                ProjectionSeedFact::Sensors(Some(dynamic_sensor_fixture())),
            );
        }
        "battery-live-performance" => {
            dynamic_history::seed(shell);
        }
        "device-hotplug" => {
            if let Some(snapshot) = shell.projection().snapshot.as_ref() {
                let mut s = (*snapshot).clone();
                let mut disk = DiskMetrics::new("/dev/sdb");
                disk.device_id = "disk:hotplug:usb0".into();
                disk.disk_type = "USB Drive".into();
                disk.model = "TaskForest Flash".into();
                disk.device_generation = DeviceGeneration::new(2);
                s.disks.push(disk);
                seed_projection_fact(shell, ProjectionSeedFact::Snapshot(Box::new(Some(s))));
            }
        }
        _ => {}
    }
}

pub(crate) fn dynamic_power_fixture() -> PowerSupplySnapshot {
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

pub(crate) fn dynamic_sensor_fixture() -> SensorCenterSnapshot {
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

pub(crate) fn capture_event_fixture() -> Vec<AlertEventSeed> {
    let cpu_rule_id = AlertRuleId::new("capture-cpu");
    let warning = Alert {
        instance_id: AlertInstanceId::new(cpu_rule_id.clone(), AlertTargetIdentity::System),
        rule_id: cpu_rule_id,
        target: "CPU".into(),
        metric: AlertMetric::CpuUsagePercent,
        severity: AlertSeverity::Warning,
        value: 93.0,
        threshold: 90.0,
        active_since_ms: 3_590_000,
    };
    let activated = AlertEventSeed {
        kind: AlertEventKind::Activated,
        alert: warning.clone(),
        observed_at_ms: 3_590_000,
    };
    let mut cleared = warning;
    let memory_rule_id = AlertRuleId::new("capture-memory");
    cleared.instance_id = AlertInstanceId::new(memory_rule_id.clone(), AlertTargetIdentity::System);
    cleared.rule_id = memory_rule_id;
    cleared.metric = AlertMetric::MemoryUsagePercent;
    cleared.target = "Memory".into();
    cleared.value = 74.0;
    vec![
        activated,
        AlertEventSeed {
            kind: AlertEventKind::Cleared,
            alert: cleared,
            observed_at_ms: 3_560_000,
        },
    ]
}
