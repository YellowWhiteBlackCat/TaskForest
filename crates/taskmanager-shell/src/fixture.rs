//! Deterministic, non-destructive data used by headless tests and visual capture.

use std::collections::BTreeMap;

use taskmanager_core::core::device_state::{DeviceLifecycle, DevicePresence, DeviceState};
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_core::core::identity::{DeviceGeneration, DeviceId, ProviderId};
use taskmanager_core::core::metrics::{
    CpuIdleState, CpuInterruptSnapshot, CpuMetrics, CpuPackageMetrics, CpuPerformancePolicy,
    CpuScalarObservations, CpuTelemetryObservation, DiskMetrics, DiskScalarObservations,
    GpuGraphicsApi, GpuMetrics, GpuScalarObservations, GpuTelemetryObservation,
    MemoryCompositionObservations, MemoryMetrics, MemoryOptionalObservations,
    MemoryScalarObservations, MemoryTelemetryObservation, MsrReadoutSnapshot,
    MsrThermalStatusReadout, NetworkAdapterType, NetworkMetrics, NetworkScalarObservations,
    NetworkTelemetryObservation, NetworkWirelessObservations, OptionalObservation,
    ScalarObservation, ScalarObservationGroup, SystemLoadAverage,
};
use taskmanager_core::core::metrics::{StorageTelemetryObservation, SystemSnapshot};
use taskmanager_core::core::npu::NpuInventorySnapshot;
use taskmanager_core::core::power::PowerSupplySnapshot;
use taskmanager_core::core::process::{
    ProcessItem, ProcessMetadataObservation, ProcessMetadataObservations, ProcessOwner,
    ProcessOwnerIdentity, ProcessScalarObservations,
};
use taskmanager_core::core::sensors::SensorCenterSnapshot;
use taskmanager_core::core::services::{ServiceItem, ServiceStatus};
use taskmanager_core::core::session::SessionItem;
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::core::startup::{
    StartupControlPolicy, StartupEntry, StartupImpact, StartupImpactEvidence,
    StartupImpactUnknownReason, StartupScope, StartupSource,
};
use taskmanager_core::core::target::ServiceId;
use taskmanager_platform_contract::{
    CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus, EventSequence,
    RequestId,
};
use taskmanager_telemetry_store::{
    CorrelatedSystemTelemetryIngestor, CorrelatedTelemetryStamp, live_graph::LiveGraphHistory,
};

use crate::{DirectTrackState, FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ShellApp};

mod cpu_topology;
mod inventory;

use inventory::{services, sessions, startup};

pub use cpu_topology::{CpuClusterSpec, CpuTopologySpec, demo_cpu_topology};
use cpu_topology::{
    core_usage_seed, cpu_types_seed, per_core_frequency_seed, per_core_temperature_seed,
};
use taskmanager_application::i18n::t;
use taskmanager_application::{
    CorrelatedEvent, MsrReadoutEvent, PlatformEventBatch, PlatformEventContext,
    ProcessAffinityReady, ProjectedProcessInsights, RequestAttemptId,
};
use taskmanager_core::core::alerts::Alert;
use taskmanager_core::core::directory_usage::DirectoryUsageSnapshot;
use taskmanager_core::core::process::ProcessBatchIntent;
use taskmanager_core::core::process_telemetry::ContainerRollup;
use taskmanager_core::core::startup::StartupBootEvidenceSnapshot;
use taskmanager_telemetry_store::TelemetryStore;
use taskmanager_telemetry_store::live_graph::MAX_HISTORY_CAPACITY;

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
    pub services_source: Option<Vec<SourceStatus>>,
    pub startup_source: Option<Vec<SourceStatus>>,
    pub sessions_source: Option<Vec<SourceStatus>>,
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
    Containers(Option<ContainerRollup>),
    PowerSupplies(Option<PowerSupplySnapshot>),
    Sensors(Option<SensorCenterSnapshot>),
    NpuInventory(Option<NpuInventorySnapshot>),
    DirectoryUsage(Option<DirectoryUsageSnapshot>),
    StartupBootEvidence(Option<StartupBootEvidenceSnapshot>),
    ServicesSource(Option<Vec<SourceStatus>>),
    StartupSource(Option<Vec<SourceStatus>>),
    SessionsSource(Option<Vec<SourceStatus>>),
    ProcessAffinity(Option<ProcessAffinityReady>),
    ProcessInsights(Box<Option<ProjectedProcessInsights>>),
    ActiveAlerts(Vec<Alert>),
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

pub fn edit_containers(app: &mut ShellApp, edit: impl FnOnce(&mut Option<ContainerRollup>)) {
    app.edit_fixture_containers(edit);
}

/// Install an already-admitted synthetic batch request for a correlated
/// completion fixture. This preserves the same attempt → request transition
/// as a real platform submission.
pub fn seed_process_batch_loading(
    app: &mut ShellApp,
    intent: ProcessBatchIntent,
    request_id: RequestId,
) {
    app.seed_fixture_process_batch_loading(intent, request_id);
}

/// Typed direct-track fixture fact used by GPUI headless/capture scenes.
#[derive(Clone, Debug)]
pub enum DirectTrackSeedFact {
    NpuInventory(NpuInventorySnapshot),
    /// Install a deterministic process snapshot through the same projection
    /// owner used by the live direct track. This exists only for headless
    /// renderer tests and never exposes a mutable projection reference.
    Processes(Vec<ProcessItem>),
}

pub fn seed_direct_track_fact(app: &mut crate::DirectTrackState, fact: DirectTrackSeedFact) {
    app.seed_fixture_fact(fact);
}

/// The deterministic, explicitly synthetic real-time MSR readout the
/// demo/capture path seeds so the `Thermal status` row is visible in every
/// frontend's evidence frame.
///
/// This is FIXTURE DATA, not a live read: demo mode never invokes the
/// privileged `telemetry.cpu.msr` lane. It deliberately carries BOTH states in
/// one frame — CPU 0 with readable bits (`thermal_status` asserted; the
/// log/event bits clear) and CPU 1 with every bit `None` (an
/// unreadable/unimplemented register, which the shared fold renders as the
/// honest dash). The values are installed through the same request-session
/// lifecycle production uses (begin → accept → correlated platform terminal),
/// so the demo exercises the real path instead of poking projection internals.
fn demo_msr_readout_snapshot() -> MsrReadoutSnapshot {
    MsrReadoutSnapshot::success(Vec::new()).with_thermal(vec![
        MsrThermalStatusReadout {
            cpu: 0,
            thermal_status: Some(true),
            thermal_status_log: Some(false),
            prochot_event: Some(false),
            prochot_event_log: Some(false),
        },
        MsrThermalStatusReadout {
            cpu: 1,
            ..MsrThermalStatusReadout::default()
        },
    ])
}

/// The fixed nonzero request id the demo seed correlates its synthetic
/// terminal with, so the seeded session state is identical across frontends
/// and runs.
const DEMO_MSR_READOUT_REQUEST_ID: RequestId = RequestId::MIN;

/// The shell tracks a demo bootstrap can seed the synthetic MSR readout into.
/// Both expose the same application-owned request-session API, so the seeding
/// sequence lives once (in [`seed_demo_msr_readout`]) and cannot drift between
/// the composed track (`ShellApp`) and the direct track (`DirectTrackState`).
trait DemoMsrReadoutTarget {
    fn demo_msr_begin(&mut self) -> RequestAttemptId;
    fn demo_msr_accept(&mut self, attempt: RequestAttemptId, request_id: RequestId) -> bool;
    fn demo_msr_fold(&mut self, batch: PlatformEventBatch);
}

impl DemoMsrReadoutTarget for ShellApp {
    fn demo_msr_begin(&mut self) -> RequestAttemptId {
        self.begin_msr_readout_request()
    }

    fn demo_msr_accept(&mut self, attempt: RequestAttemptId, request_id: RequestId) -> bool {
        self.accept_msr_readout_request(attempt, request_id)
    }

    fn demo_msr_fold(&mut self, batch: PlatformEventBatch) {
        self.apply_platform_batch(batch);
    }
}

impl DemoMsrReadoutTarget for DirectTrackState {
    fn demo_msr_begin(&mut self) -> RequestAttemptId {
        self.begin_msr_readout_request()
    }

    fn demo_msr_accept(&mut self, attempt: RequestAttemptId, request_id: RequestId) -> bool {
        self.accept_msr_readout_request(attempt, request_id)
    }

    fn demo_msr_fold(&mut self, batch: PlatformEventBatch) {
        let _ = self.apply_platform_batch(batch);
    }
}

/// Seed the deterministic, explicitly synthetic real-time thermal-status
/// readout into one shell track through the SAME request-session lifecycle a
/// live read uses: begin the attempt, accept its request, then fold the
/// correlated platform terminal. This is the single demo/capture seeding path
/// the shared demo builders call; the live path never reaches it.
fn seed_demo_msr_readout(target: &mut impl DemoMsrReadoutTarget) {
    let attempt = target.demo_msr_begin();
    if !target.demo_msr_accept(attempt, DEMO_MSR_READOUT_REQUEST_ID) {
        return;
    }
    let mut batch = PlatformEventBatch::default();
    batch.msr_readout_events.push(CorrelatedEvent::new(
        PlatformEventContext {
            request_id: DEMO_MSR_READOUT_REQUEST_ID,
            capability: CapabilityId::TELEMETRY_CPU_MSR,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 0,
        },
        MsrReadoutEvent::Update(demo_msr_readout_snapshot()),
    ));
    target.demo_msr_fold(batch);
}

/// Seed the same synthetic real-time thermal-status readout into a direct
/// track from the capture-evidence route, which runs the production shell and
/// therefore never reaches the demo builders. The privileged
/// `telemetry.cpu.msr` lane cannot run on the disposable capture host, so
/// without this seed the capture receipt would omit the real-time row
/// entirely.
///
/// This is FIXTURE DATA, not a live privileged read: the production live path
/// never calls it (the caller gates on capture evidence being enabled), and it
/// drives the real request-session lifecycle so the painted row exercises the
/// same admission path a live read uses.
pub fn seed_capture_msr_readout(track: &mut DirectTrackState) {
    seed_demo_msr_readout(track);
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
        services_source: Some(vec![SourceStatus {
            provider: ProviderId::borrowed("fixture"),
            outcome: SourceOutcome::Available,
            item_count: 5,
        }]),
        startup_source: Some(vec![SourceStatus {
            provider: ProviderId::borrowed("fixture"),
            outcome: SourceOutcome::Available,
            item_count: 2,
        }]),
        // The fixture answers with an Available source so the Users page renders
        // rows, never the failed-source empty state.
        sessions_source: Some(vec![SourceStatus {
            provider: ProviderId::borrowed("fixture"),
            outcome: SourceOutcome::Available,
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
    app.apply_capability_snapshot(CapabilitySnapshot::from_descriptors([
        CapabilityDescriptor {
            id: CapabilityId::TELEMETRY_GPU_ENGINES,
            // The demo GPU-engine lane is escalation-backed (ADR-023), so it
            // shows the honest escalatable state rather than a plain gate.
            status: CapabilityStatus::RequiresEscalation,
            providers: vec![ProviderId::borrowed("fixture.gpu-engines")],
            observed_at_ms: 0,
            last_success_at_ms: None,
        },
    ]));
    // Install the synthetic real-time thermal-status readout through the real
    // request-session path so every demo/capture frame paints the new row.
    seed_demo_msr_readout(&mut app);
    app.report_notice(
        FeedbackSource::Demo,
        FeedbackSeverity::Info,
        FeedbackLifecycle::UntilReplaced,
        t("status.demo_snapshot"),
    );
    app
}

/// Build the GPUI direct-track projection from the same canonical demo facts
/// used by the composed shell track. The conversion is intentionally kept in
/// the fixture boundary so a renderer cannot invent a second demo model.
#[must_use]
pub fn demo_direct_track() -> DirectTrackState {
    let demo = demo_app();
    let mut track = DirectTrackState::from_fixture_projection(demo.projection().clone());
    // The direct track owns its own request sessions (the projection copy does
    // not carry them), so seed the same synthetic readout through the shared
    // helper — one source of values for every frontend track.
    seed_demo_msr_readout(&mut track);
    track
}

/// Build a bounded live-graph store populated with deterministic samples for
/// the standalone GPUI demo. The returned write capability belongs to the
/// same store and is intentionally returned to the caller so the GPUI root
/// can retain the normal read/write separation even though demo mode never
/// schedules live collection.
#[must_use]
pub fn demo_telemetry() -> (
    std::sync::Arc<TelemetryStore>,
    CorrelatedSystemTelemetryIngestor,
) {
    let (history, ingestor) = LiveGraphHistory::shared(MAX_HISTORY_CAPACITY);
    let base = snapshot();
    for revision in 1..=4 {
        let mut frame = base.clone();
        frame.timestamp_ms = base.timestamp_ms.saturating_add(revision * 1_000);
        record_demo_history_frame_into(&ingestor, revision, &frame, None, None);
    }
    (history.store().clone(), ingestor)
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
    let ingestor = app.ensure_history_ingestor();
    record_demo_history_frame_into(&ingestor, revision, snapshot, power, sensors);
}

/// Feed one deterministic frame into an explicitly supplied live-history
/// writer. This is the shared fixture-side adapter used when a frontend must
/// construct a demo store before it has a shell application instance.
pub fn record_demo_history_frame_into(
    ingestor: &CorrelatedSystemTelemetryIngestor,
    revision: u64,
    snapshot: &SystemSnapshot,
    power: Option<&PowerSupplySnapshot>,
    sensors: Option<&SensorCenterSnapshot>,
) {
    let Some(stamp) =
        CorrelatedTelemetryStamp::from_accepted_event(revision.max(1), snapshot.timestamp_ms)
    else {
        return;
    };
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
            generation: DeviceGeneration::INITIAL,
            first_seen_ms: Some(observed_at_ms),
            last_seen_ms: Some(observed_at_ms),
            absent_since_ms: None,
        },
    )
}

fn snapshot() -> SystemSnapshot {
    let mut cpu = CpuMetrics::from_observations(CpuScalarObservations {
        global_usage_pct: ScalarObservation::available(37.4, 1_785_292_800_000),
        core_usage_group: ScalarObservationGroup::available(core_usage_seed(), 1_785_292_800_000),
        per_core_frequency_group: ScalarObservationGroup::available(
            per_core_frequency_seed(),
            1_785_292_800_000,
        ),
        per_core_temperature_group: ScalarObservationGroup::available(
            per_core_temperature_seed(),
            1_785_292_800_000,
        ),
        frequency_mhz: ScalarObservation::available(3_284, 1_785_292_800_000),
        temperature_c: ScalarObservation::available(54.0, 1_785_292_800_000),
        ..Default::default()
    });
    cpu.brand = Some("Intel(R) Core(TM) Ultra 7 358H".into());
    cpu.physical_cores = Some(demo_cpu_topology().physical_cores());
    cpu.logical_cores = Some(demo_cpu_topology().logical_cores());
    cpu.performance_policy = CpuPerformancePolicy {
        frequency_implementation: Some("intel_pstate".into()),
        active_policy: Some("powersave".into()),
        energy_preference: Some("balance_performance".into()),
        boost_enabled: Some(true),
        boost_max_frequency_mhz: Some(4_800),
        power_limit_1_w: Some(89.0),
        power_limit_2_w: Some(95.0),
        power_time_window_ms: Some(55_000),
    };
    cpu.packages = vec![CpuPackageMetrics {
        package_id: 0,
        numa_node_id: Some(0),
        numa_node_ids: vec![0],
        logical_core_ids: (0..cpu.logical_cores.unwrap_or_default()).collect(),
        chiplet_ids: vec![0],
        smt_threads_per_core: Some(2),
        smt_sibling_groups: vec![(0..2).collect()],
        physical_core_count: cpu.physical_cores,
        local_memory_bytes: Some(32 * GIB),
        numa_hit_ratio_pct: Some(99.8),
        is_throttled: Some(false),
        package_throttle_count: Some(0),
        core_throttle_count: Some(0),
        thermal_margin_c: Some(46.0),
        temperature_c: Some(54.0),
        power_w: Some(18.8),
        frequency_mhz: Some(3_284),
    }];
    cpu.idle_states = vec![
        CpuIdleState {
            name: "C1".into(),
            description: Some("C1-HLT".into()),
            residency_us: Some(611_700_000),
            residency_pct: Some(12.5),
            usage_count: Some(611_700),
            latency_us: Some(1),
            disabled: Some(false),
        },
        CpuIdleState {
            name: "C10".into(),
            description: Some("深度空闲".into()),
            residency_us: Some(4_300_000_000),
            residency_pct: Some(82.5),
            usage_count: Some(4_300_000),
            latency_us: Some(200),
            disabled: Some(false),
        },
    ];
    cpu.interrupts = Some(CpuInterruptSnapshot {
        total: Some(2_350_000),
        per_logical_cpu: (0..cpu.logical_cores.unwrap_or_default())
            .map(|index| 80_000 + u64::from((index % 4) as u8) * 10_000)
            .collect(),
    });
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
                average_queue_depth: ScalarObservation::available(0.42, 1_785_292_800_000),
                service_time_ms: ScalarObservation::available(1.8, 1_785_292_800_000),
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
                    link_up: ScalarObservation::available(true, 1_785_292_800_000),
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
            gpu.vbios_version = Some("101.0.0.0".into());
            gpu.graphics_api = Some(GpuGraphicsApi {
                opengl_version: Some("4.6".into()),
                vulkan_version: Some("1.4.304".into()),
                mesa_version: Some("25.1.4".into()),
            });
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
        pressure: None,
        load_average: SystemLoadAverage::from_raw(2.4, 1.8, 1.2, 8),
    }
}

fn hardware() -> HardwareInfo {
    HardwareInfo {
        os_name: Some("Linux".into()),
        os_version: Some("Arch Linux".into()),
        kernel_version: Some("6.18.7-arch1-1".into()),
        hostname: Some("taskforest-workstation".into()),
        cpu_brand: Some("Intel(R) Core(TM) Ultra 7 358H".into()),
        cpu_types: cpu_types_seed(),
        cpu_cores: Some(demo_cpu_topology().logical_cores()),
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

#[cfg(test)]
#[path = "../tests/headless/fixture_demo.rs"]
mod tests;
