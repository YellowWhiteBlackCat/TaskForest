//! Deterministic, non-destructive data used by headless tests and visual capture.

use std::collections::BTreeMap;

use taskmanager_application::{
    CpuMetrics, CpuTelemetryObservation, DeviceGeneration, DeviceId, DeviceLifecycle,
    DevicePresence, DeviceState, DiskMetrics, DiskScalarObservations, GpuMetrics,
    GpuScalarObservations, GpuTelemetryObservation, HardwareInfo, MemoryCompositionObservations,
    MemoryMetrics, MemoryOptionalObservations, MemoryScalarObservations,
    MemoryTelemetryObservation, NetworkAdapterType, NetworkMetrics, NetworkScalarObservations,
    NetworkTelemetryObservation, NetworkWirelessObservations, NpuInventorySnapshot,
    OptionalObservation, PowerSupplySnapshot, ProcessItem, ProcessMetadataObservation,
    ProcessMetadataObservations, ProcessOwner, ProcessOwnerIdentity, ProcessScalarObservations,
    ScalarObservation, SensorCenterSnapshot, ServiceId, ServiceItem, ServiceStatus, SessionItem,
    StartupControlPolicy, StartupEntry, StartupImpact, StartupImpactEvidence,
    StartupImpactUnknownReason, StartupScope, StartupSource, StorageTelemetryObservation,
    SystemSnapshot,
};
use taskmanager_telemetry_store::CorrelatedTelemetryStamp;

use crate::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ShellApp};

const GIB: u64 = 1024 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;

/// Typed deterministic facts accepted by the shell's fixture boundary.
///
/// This is deliberately not a mutable `SystemProjectionStore`: tests and
/// capture scenes describe the facts they need, while the shell remains the
/// only code that can install them into its canonical projection.
#[derive(Clone, Debug, Default)]
pub struct ProjectionSeed {
    pub snapshot: Option<SystemSnapshot>,
    pub hardware: Option<HardwareInfo>,
    pub processes: Option<Vec<ProcessItem>>,
    pub services: Option<Vec<ServiceItem>>,
    pub startup_entries: Option<Vec<StartupEntry>>,
    pub sessions: Option<Vec<SessionItem>>,
    pub services_source: Option<Vec<taskmanager_application::SourceStatus>>,
    pub startup_source: Option<Vec<taskmanager_application::SourceStatus>>,
    pub sessions_source: Option<Vec<taskmanager_application::SourceStatus>>,
}

/// One explicitly-scoped fixture mutation. Unlike a mutable projection
/// reference, this enum makes the affected domain visible at the call site.
#[derive(Clone, Debug)]
pub enum ProjectionSeedFact {
    Snapshot(Box<Option<SystemSnapshot>>),
    Hardware(Option<Box<HardwareInfo>>),
    Processes(Option<Vec<ProcessItem>>),
    Services(Option<Vec<ServiceItem>>),
    StartupEntries(Option<Vec<StartupEntry>>),
    Sessions(Option<Vec<SessionItem>>),
    Containers(Option<taskmanager_application::ContainerRollup>),
    PowerSupplies(Option<PowerSupplySnapshot>),
    Sensors(Option<SensorCenterSnapshot>),
    NpuInventory(Option<taskmanager_application::NpuInventorySnapshot>),
    DirectoryUsage(Option<taskmanager_application::DirectoryUsageSnapshot>),
    StartupBootEvidence(Option<taskmanager_application::StartupBootEvidenceSnapshot>),
    ServicesSource(Option<Vec<taskmanager_application::SourceStatus>>),
    StartupSource(Option<Vec<taskmanager_application::SourceStatus>>),
    SessionsSource(Option<Vec<taskmanager_application::SourceStatus>>),
    ProcessAffinity(Option<taskmanager_application::ProcessAffinityReady>),
    ProcessInsights(Box<Option<taskmanager_application::ProjectedProcessInsights>>),
    ActiveAlerts(Vec<taskmanager_application::alerts::Alert>),
    AdvanceRevision(ProjectionSeedDomain),
    AdvanceRefresh,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionSeedDomain {
    Processes,
    Services,
    Startup,
    Sessions,
    System,
}

/// Apply one deterministic fixture fact through the shell-owned seed reducer.
pub fn seed_projection_fact(app: &mut ShellApp, fact: ProjectionSeedFact) {
    app.seed_fixture_fact(fact);
}

/// Domain-scoped fixture editors. The closure cannot reach any other store
/// partition and no mutable reference escapes the call.
pub fn edit_snapshot(app: &mut ShellApp, edit: impl FnOnce(&mut Option<SystemSnapshot>)) {
    app.edit_fixture_snapshot(edit);
}

pub fn edit_processes(app: &mut ShellApp, edit: impl FnOnce(&mut Option<Vec<ProcessItem>>)) {
    app.edit_fixture_processes(edit);
}

pub fn edit_hardware(app: &mut ShellApp, edit: impl FnOnce(&mut Option<HardwareInfo>)) {
    app.edit_fixture_hardware(edit);
}

pub fn edit_containers(
    app: &mut ShellApp,
    edit: impl FnOnce(&mut Option<taskmanager_application::ContainerRollup>),
) {
    app.edit_fixture_containers(edit);
}

/// Install an already-admitted synthetic batch request for a correlated
/// completion fixture. This preserves the same attempt → request transition
/// as a real platform submission.
pub fn seed_process_batch_loading(
    app: &mut ShellApp,
    intent: taskmanager_application::ProcessBatchIntent,
    request_id: taskmanager_application::RequestId,
) {
    app.seed_fixture_process_batch_loading(intent, request_id);
}

/// Typed direct-track fixture fact used by GPUI headless/capture scenes.
#[derive(Clone, Debug)]
pub enum DirectTrackSeedFact {
    NpuInventory(NpuInventorySnapshot),
}

pub fn seed_direct_track_fact(app: &mut crate::DirectTrackState, fact: DirectTrackSeedFact) {
    app.seed_fixture_fact(fact);
}

/// A stable full-product frame. It contains no control intent and performs no I/O.
#[must_use]
pub fn demo_app() -> ShellApp {
    let mut app = ShellApp::new();
    app.seed_fixture_projection(ProjectionSeed {
        snapshot: Some(snapshot()),
        hardware: Some(hardware()),
        processes: Some(processes()),
        services: Some(services()),
        startup_entries: Some(startup()),
        sessions: Some(sessions()),
        services_source: Some(vec![taskmanager_application::SourceStatus {
            provider: taskmanager_application::ProviderId::borrowed("fixture"),
            outcome: taskmanager_application::SourceOutcome::Available,
            item_count: 5,
        }]),
        startup_source: Some(vec![taskmanager_application::SourceStatus {
            provider: taskmanager_application::ProviderId::borrowed("fixture"),
            outcome: taskmanager_application::SourceOutcome::Available,
            item_count: 2,
        }]),
        // The fixture answers with an Available source so the Users page renders
        // rows, never the failed-source empty state.
        sessions_source: Some(vec![taskmanager_application::SourceStatus {
            provider: taskmanager_application::ProviderId::borrowed("fixture"),
            outcome: taskmanager_application::SourceOutcome::Available,
            item_count: 2,
        }]),
    });
    // Seed the rolling suggestion window with exactly the demo snapshot, so the
    // threshold-suggestions overlay reflects the honest "just-launched" state
    // (every numeric metric below the 20-sample floor, never a fabricated
    // threshold). The demo runtime performs no live collection, so the window
    // does not grow further; this is honest demo fixture data, not a guess.
    if let Some(seeded) = app.projection().snapshot.clone() {
        app.alert_suggestions.record_snapshot(&seeded);
    }
    if let Some(seeded) = app.projection().snapshot.clone() {
        record_demo_history_frame(&mut app, &seeded, None, None);
    }
    app.apply_capability_snapshot(
        taskmanager_application::CapabilitySnapshot::from_descriptors([
            taskmanager_application::CapabilityDescriptor {
                id: taskmanager_application::CapabilityId::TELEMETRY_GPU_ENGINES,
                status: taskmanager_application::CapabilityStatus::PermissionRequired,
                providers: vec![taskmanager_application::ProviderId::borrowed(
                    "fixture.gpu-engines",
                )],
                observed_at_ms: 0,
                last_success_at_ms: None,
            },
        ]),
    );
    app.report_notice(
        FeedbackSource::Demo,
        FeedbackSeverity::Info,
        FeedbackLifecycle::UntilReplaced,
        "Demo snapshot · no host actions",
    );
    app
}

/// Feed deterministic demo/capture facts through the same typed bounded-store
/// ingestor used by live correlation. This fixture seam does not exist on
/// `LiveGraphHistory`, so production render code retains read-only authority.
pub fn record_demo_history_frame(
    app: &mut ShellApp,
    snapshot: &SystemSnapshot,
    power: Option<&PowerSupplySnapshot>,
    sensors: Option<&SensorCenterSnapshot>,
) {
    let revision = app
        .history
        .store()
        .system_history
        .revision()
        .saturating_add(1)
        .max(1);
    let Some(stamp) =
        CorrelatedTelemetryStamp::from_accepted_event(revision, snapshot.timestamp_ms)
    else {
        return;
    };
    let ingestor = app.ensure_history_ingestor();
    let _ = ingestor.ingest_correlated_cpu(
        stamp,
        &CpuTelemetryObservation::current(snapshot.cpu.clone(), snapshot.timestamp_ms, Vec::new()),
    );
    let _ = ingestor.ingest_correlated_memory(
        stamp,
        &MemoryTelemetryObservation::current(
            snapshot.memory.clone(),
            snapshot.timestamp_ms,
            Vec::new(),
        ),
    );

    let (disks, disk_lifecycles) = generation_scoped_disks(snapshot);
    let _ = ingestor.ingest_correlated_storage(
        stamp,
        &StorageTelemetryObservation::current(
            disks,
            snapshot.timestamp_ms,
            Vec::new(),
            Vec::new(),
            disk_lifecycles,
        ),
    );
    let (networks, network_lifecycles) = generation_scoped_networks(snapshot);
    let _ = ingestor.ingest_correlated_network(
        stamp,
        &NetworkTelemetryObservation::current(
            networks,
            snapshot.timestamp_ms,
            Vec::new(),
            Vec::new(),
            network_lifecycles,
        ),
    );
    let (gpus, gpu_lifecycles) = generation_scoped_gpus(snapshot);
    let _ = ingestor.ingest_correlated_gpu(
        stamp,
        &GpuTelemetryObservation::current(
            gpus,
            snapshot.timestamp_ms,
            Vec::new(),
            Vec::new(),
            gpu_lifecycles,
        ),
    );

    if let Some(power) = power {
        let mut power = power.clone();
        for battery in &mut power.batteries {
            battery.device_generation = DeviceGeneration::new(1);
        }
        let _ = ingestor.ingest_correlated_power_supplies(stamp, &power);
    }
    if let Some(sensors) = sensors {
        let mut sensors = sensors.clone();
        sensors.readings = sensors
            .readings
            .into_iter()
            .map(|reading| reading.with_device_generation(DeviceGeneration::new(1)))
            .collect();
        let _ = ingestor.ingest_correlated_sensors(stamp, &sensors);
    }
}

fn generation_scoped_disks(
    snapshot: &SystemSnapshot,
) -> (Vec<DiskMetrics>, BTreeMap<DeviceId, DeviceLifecycle>) {
    let mut values = snapshot.disks.clone();
    for disk in &mut values {
        disk.device_generation = DeviceGeneration::new(1);
        disk.device_state = DeviceState::healthy(snapshot.timestamp_ms);
    }
    let lifecycles = values
        .iter()
        .map(|disk| lifecycle_entry(disk.device_id.as_str(), snapshot.timestamp_ms))
        .collect();
    (values, lifecycles)
}

fn generation_scoped_networks(
    snapshot: &SystemSnapshot,
) -> (Vec<NetworkMetrics>, BTreeMap<DeviceId, DeviceLifecycle>) {
    let mut values = snapshot.networks.clone();
    for network in &mut values {
        network.device_generation = DeviceGeneration::new(1);
        network.device_state = DeviceState::healthy(snapshot.timestamp_ms);
    }
    let lifecycles = values
        .iter()
        .map(|network| lifecycle_entry(network.device_id.as_ref(), snapshot.timestamp_ms))
        .collect();
    (values, lifecycles)
}

fn generation_scoped_gpus(
    snapshot: &SystemSnapshot,
) -> (Vec<GpuMetrics>, BTreeMap<DeviceId, DeviceLifecycle>) {
    let mut values = snapshot.gpu.clone();
    for gpu in &mut values {
        gpu.device_generation = DeviceGeneration::new(1);
        gpu.device_state = DeviceState::healthy(snapshot.timestamp_ms);
    }
    let lifecycles = values
        .iter()
        .map(|gpu| lifecycle_entry(gpu.device_id.as_str(), snapshot.timestamp_ms))
        .collect();
    (values, lifecycles)
}

fn lifecycle_entry(device_id: &str, observed_at_ms: u64) -> (DeviceId, DeviceLifecycle) {
    (
        DeviceId::new(device_id.to_owned()),
        DeviceLifecycle {
            presence: DevicePresence::Present,
            state: DeviceState::healthy(observed_at_ms),
            generation: 1,
            first_seen_ms: Some(observed_at_ms),
            last_seen_ms: Some(observed_at_ms),
            absent_since_ms: None,
        },
    )
}

fn snapshot() -> SystemSnapshot {
    let mut cpu = CpuMetrics::from_observations(taskmanager_application::CpuScalarObservations {
        global_usage_pct: ScalarObservation::available(37.4, 1_785_292_800_000),
        core_usage_group: taskmanager_application::ScalarObservationGroup::available(
            vec![52.0, 41.0, 34.0, 22.0],
            1_785_292_800_000,
        ),
        frequency_mhz: ScalarObservation::available(3_284, 1_785_292_800_000),
        temperature_c: ScalarObservation::available(54.0, 1_785_292_800_000),
        ..Default::default()
    });
    cpu.brand = Some("Intel Core Ultra 7 358H".into());
    cpu.physical_cores = Some(16);
    cpu.logical_cores = Some(22);
    SystemSnapshot {
        timestamp_ms: 1_785_292_800_000,
        cpu,
        memory: MemoryMetrics::from_observations(
            MemoryScalarObservations {
                total_bytes: ScalarObservation::available(32 * GIB, 1_785_292_800_000),
                used_bytes: ScalarObservation::available(12 * GIB + 640 * MIB, 1_785_292_800_000),
                available_bytes: ScalarObservation::available(
                    19 * GIB + 384 * MIB,
                    1_785_292_800_000,
                ),
                swap_total_bytes: ScalarObservation::available(8 * GIB, 1_785_292_800_000),
                swap_used_bytes: ScalarObservation::available(620 * MIB, 1_785_292_800_000),
                ..Default::default()
            },
            MemoryOptionalObservations {
                composition: MemoryCompositionObservations {
                    cached_bytes: OptionalObservation::present(7 * GIB, 1_785_292_800_000),
                    ..Default::default()
                },
                ..Default::default()
            },
        ),
        disks: vec![{
            let mut disk = DiskMetrics::new("nvme0n1");
            disk.device_id = "disk:demo:nvme0".into();
            disk.disk_type = "NVMe SSD".into();
            disk.model = "TiPro9000 2TB".into();
            disk.mount_point = "/".into();
            // The same accepted observation drives the row and the seeded
            // history ring, so the demo row carries the generation its ring
            // was reset for (generation-scoped reads refuse an unbound 0).
            disk.device_generation = DeviceGeneration::new(1);
            disk.apply_scalar_observations(DiskScalarObservations {
                capacity_bytes: ScalarObservation::available(2_000 * GIB, 1_785_292_800_000),
                available_bytes: ScalarObservation::available(1_240 * GIB, 1_785_292_800_000),
                read_bytes_per_sec: ScalarObservation::available(84 * MIB, 1_785_292_800_000),
                write_bytes_per_sec: ScalarObservation::available(31 * MIB, 1_785_292_800_000),
                active_time_pct: ScalarObservation::available(12.7, 1_785_292_800_000),
                ..Default::default()
            });
            disk
        }],
        networks: vec![{
            let mut network = NetworkMetrics::new("wlan0");
            network.device_id = "network:demo:wlan0".into();
            network.ipv4_addr = Some("192.168.1.42".into());
            // Same row/ring generation contract as the disk and GPU rows.
            network.device_generation = DeviceGeneration::new(1);
            network.apply_observations(
                NetworkAdapterType::WiFi,
                NetworkScalarObservations {
                    rx_bytes_per_sec: ScalarObservation::available(12 * MIB, 1_785_292_800_000),
                    tx_bytes_per_sec: ScalarObservation::available(2 * MIB, 1_785_292_800_000),
                    ..Default::default()
                },
                NetworkWirelessObservations {
                    association: OptionalObservation::present(true, 1_785_292_800_000),
                    ssid: OptionalObservation::present("TaskForest Lab".into(), 1_785_292_800_000),
                    ..Default::default()
                },
            );
            network
        }],
        gpu: vec![{
            let mut gpu = GpuMetrics::new("gpu:demo:xe", "Intel Graphics (xe)");
            gpu.driver = Some("xe".into());
            // The same accepted observation drives the row and the seeded
            // history ring, so the demo row carries the generation its ring
            // was reset for (generation-scoped reads refuse an unbound 0).
            gpu.device_generation = DeviceGeneration::new(1);
            gpu.apply_scalar_observations(GpuScalarObservations {
                utilization_pct: ScalarObservation::available(18.0, 1_785_292_800_000),
                idle_residency_pct: ScalarObservation::available(78.0, 1_785_292_800_000),
                temperature_c: ScalarObservation::available(48.0, 1_785_292_800_000),
                frequency_mhz: ScalarObservation::available(900, 1_785_292_800_000),
                ..Default::default()
            });
            gpu
        }],
        telemetry_sources: Vec::new(),
        provider_states: Vec::new(),
        device_lifecycles: Default::default(),
        uptime_secs: 6 * 3600 + 42 * 60,
        processes: 347,
        threads: Some(2_816),
    }
}

fn hardware() -> HardwareInfo {
    HardwareInfo {
        os_name: Some("Linux".into()),
        os_version: Some("Arch Linux".into()),
        kernel_version: Some("6.18.7-arch1-1".into()),
        hostname: Some("taskforest-workstation".into()),
        cpu_brand: Some("Intel Core Ultra 7 358H".into()),
        cpu_cores: Some(22),
        sockets: Some(1),
        total_memory_mb: Some(32 * 1024),
        architecture: Some(std::env::consts::ARCH.into()),
        motherboard_vendor: Some("LENOVO".into()),
        motherboard_model: Some("21L6000CSC".into()),
        firmware_release_date: Some("2025-06-11".into()),
        secure_boot: Some(true),
        ..Default::default()
    }
}

fn processes() -> Vec<ProcessItem> {
    [
        (4201, "zed", 24.8, 2_640, "devuser", "Running"),
        (1810, "gnome-shell", 9.6, 1_120, "devuser", "Running"),
        (9312, "rust-analyzer", 6.1, 842, "devuser", "Sleeping"),
        (1550, "Xwayland", 3.7, 378, "root", "Sleeping"),
        (8842, "cargo", 2.9, 244, "devuser", "Running"),
        (732, "NetworkManager", 1.1, 96, "root", "Sleeping"),
        (1, "systemd", 0.4, 18, "root", "Sleeping"),
        (9930, "taskmanager-tui", 0.3, 14, "devuser", "Running"),
        (843, "pipewire", 0.2, 42, "devuser", "Sleeping"),
        (712, "dbus-broker", 0.1, 12, "root", "Sleeping"),
        (602, "systemd-journald", 0.1, 64, "root", "Sleeping"),
        (77, "kworker/u64:2", 0.0, 0, "root", "Idle"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (pid, name, cpu, memory_mib, user, status))| {
        let start_time_secs = 1_785_290_000 + index as u64;
        let mut process = ProcessItem::new(pid, name);
        process.status = status.into();
        process.apply_metadata_observations(ProcessMetadataObservations {
            owner: ProcessMetadataObservation::available(
                ProcessOwner {
                    identity: ProcessOwnerIdentity::Opaque(user.into()),
                    label: None,
                },
                1,
            ),
            executable_path: ProcessMetadataObservation::absent(1),
        });
        process.apply_scalar_observations(ProcessScalarObservations {
            start_token: ScalarObservation::available(
                u64::from(pid) * 10_000 + index as u64 + 1,
                1,
            ),
            cpu_percentage: ScalarObservation::available(cpu, 1),
            memory_bytes: ScalarObservation::available(memory_mib * MIB, 1),
            start_time_secs: ScalarObservation::available(start_time_secs, 1),
            ..Default::default()
        });
        process
    })
    .collect()
}

fn services() -> Vec<ServiceItem> {
    [
        (
            "NetworkManager.service",
            ServiceStatus::Active,
            "Network manager",
        ),
        (
            "bluetooth.service",
            ServiceStatus::Active,
            "Bluetooth service",
        ),
        (
            "docker.service",
            ServiceStatus::Inactive,
            "Container engine",
        ),
        (
            "systemd-timesyncd.service",
            ServiceStatus::Active,
            "Network time",
        ),
        (
            "demo-failed.service",
            ServiceStatus::Failed,
            "Recovery required",
        ),
    ]
    .into_iter()
    .map(|(name, status, description)| {
        ServiceItem::from_inventory(
            ServiceId::new(format!("fixture.service:{name}")),
            name,
            status,
            description,
            "",
            "",
            "",
        )
    })
    .collect()
}

fn startup() -> Vec<StartupEntry> {
    vec![
        StartupEntry {
            id: "user-service:ssh-agent.service".into(),
            name: "SSH Agent".into(),
            exec: "ssh-agent.service".into(),
            enabled: true,
            source: StartupSource::UserService,
            scope: StartupScope::User,
            control_policy: StartupControlPolicy::Direct,
            locator: "ssh-agent.service".into(),
            impact: StartupImpact::Low,
            impact_evidence: StartupImpactEvidence::Measured { duration_ms: 42 },
        },
        StartupEntry {
            id: "desktop:clipboard-sync.desktop".into(),
            name: "Clipboard Sync".into(),
            exec: "wl-paste --watch".into(),
            enabled: true,
            source: StartupSource::DesktopEntry,
            scope: StartupScope::User,
            control_policy: StartupControlPolicy::Direct,
            locator: "clipboard-sync.desktop".into(),
            impact: StartupImpact::None,
            impact_evidence: StartupImpactEvidence::Unknown {
                reason: StartupImpactUnknownReason::NotInstrumented,
            },
        },
    ]
}

fn sessions() -> Vec<SessionItem> {
    vec![
        SessionItem {
            id: "2".into(),
            uid: 1000,
            user: "devuser".into(),
            seat: Some("seat0".into()),
            tty: Some("tty2".into()),
            remote: false,
            timestamp: Some("2026-07-29 08:41".into()),
        },
        SessionItem {
            id: "9".into(),
            uid: 1000,
            user: "devuser".into(),
            seat: None,
            tty: Some("pts/4".into()),
            remote: true,
            timestamp: Some("2026-07-29 11:20".into()),
        },
    ]
}
