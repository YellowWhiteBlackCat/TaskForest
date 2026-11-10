//! Deterministic, non-destructive data used by headless tests and visual capture.

use std::collections::BTreeMap;

use taskforest_contract::{
    CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus, EventSequence,
    RequestId,
};
use taskforest_core::core::device_state::{DeviceLifecycle, DevicePresence, DeviceState};
use taskforest_core::core::hardware::HardwareInfo;
use taskforest_core::core::identity::{DeviceGeneration, DeviceId, ProviderId};
use taskforest_core::core::metrics::{
    CpuTelemetryObservation, DiskMetrics, GpuMetrics, GpuTelemetryObservation,
    MemoryTelemetryObservation, MsrReadoutSnapshot, MsrThermalStatusReadout, NetworkMetrics,
    NetworkTelemetryObservation, ScalarObservation, StorageTelemetryObservation, SystemSnapshot,
};
use taskforest_core::core::npu::NpuInventorySnapshot;
use taskforest_core::core::power::PowerSupplySnapshot;
use taskforest_core::core::process::ProcessItem;
use taskforest_core::core::sensors::SensorCenterSnapshot;
use taskforest_core::core::services::{ServiceItem, ServiceStatus};
use taskforest_core::core::session::SessionItem;
use taskforest_core::core::source::{SourceOutcome, SourceStatus};
use taskforest_core::core::startup::{
    StartupControlPolicy, StartupEntry, StartupImpact, StartupImpactEvidence,
    StartupImpactUnknownReason, StartupScope, StartupSource,
};
use taskforest_core::core::target::ServiceId;
use taskmanager_telemetry_store::{
    CorrelatedSystemTelemetryIngestor, CorrelatedTelemetryStamp, live_graph::LiveGraphHistory,
};

use crate::{DirectTrackState, FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ShellApp};

pub mod alerts;
mod cpu_topology;
pub mod dashboard_history;
mod demo_history;
pub mod health;
mod inventory;
pub mod setup;
pub mod smbios_memory;
mod snapshot;
pub mod startup;

use inventory::{processes, services, sessions, startup};
use snapshot::{hardware, snapshot};

pub use cpu_topology::{CpuClusterSpec, CpuTopologySpec, demo_cpu_topology};
use taskforest_core::core::directory_usage::DirectoryUsageSnapshot;
use taskforest_core::core::process::ProcessBatchIntent;
use taskforest_core::core::process_telemetry::ContainerRollup;
use taskforest_core::core::startup::StartupBootEvidenceSnapshot;
use taskmanager_application::i18n::t;
use taskmanager_application::{
    CorrelatedEvent, MsrReadoutEvent, PlatformEventBatch, PlatformEventContext,
    ProcessAffinityState, ProjectedProcessInsights, RequestAttemptId,
};
use taskmanager_telemetry_store::TelemetryStore;
use taskmanager_telemetry_store::live_graph::MAX_HISTORY_CAPACITY;

use taskforest_core::core::units::BYTES_PER_MIB;

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
    ProcessAffinity(Option<ProcessAffinityState>),
    ProcessInsights(Box<Option<ProjectedProcessInsights>>),
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
    /// Install a deterministic login-session projection for direct-track
    /// headless and capture scenes.
    Sessions(Option<Vec<SessionItem>>),
    /// Install the paired session-source status used by direct-track
    /// headless and capture scenes.
    SessionsSource(Option<Vec<SourceStatus>>),
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
    seed_demo_history(&mut app);
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
    track.report_notice(
        FeedbackSource::Demo,
        FeedbackSeverity::Info,
        FeedbackLifecycle::UntilReplaced,
        t("status.demo_snapshot"),
    );
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
    seed_demo_history_into(&ingestor);
    (history.store().clone(), ingestor)
}

/// Fill the composed track's history with the shared capture sequence.
fn seed_demo_history(app: &mut ShellApp) {
    let Some(seed) = app.projection().snapshot.clone() else {
        return;
    };
    record_demo_history_frame(app, &seed, None, None);
    let first_revision = app.history.store().system_history.revision();
    for offset in 1..=demo_history::DEMO_HISTORY_FRAMES {
        let mut next = seed.clone();
        next.timestamp_ms = next
            .timestamp_ms
            .saturating_add(offset.saturating_mul(1_000));
        next.cpu = demo_history::demo_cpu_frame(&next.cpu, next.timestamp_ms, offset);
        record_demo_history_frame_into(
            &app.ensure_history_ingestor(),
            first_revision.saturating_add(offset),
            &next,
            None,
            None,
        );
    }
}

/// Fill a standalone store with exactly the same sequence the composed track
/// receives, including its one honest seed frame and 24 adjacent frames.
fn seed_demo_history_into(ingestor: &CorrelatedSystemTelemetryIngestor) {
    let seed = snapshot();
    record_demo_history_frame_into(ingestor, 1, &seed, None, None);
    for offset in 1..=demo_history::DEMO_HISTORY_FRAMES {
        let mut next = seed.clone();
        next.timestamp_ms = next
            .timestamp_ms
            .saturating_add(offset.saturating_mul(1_000));
        next.cpu = demo_history::demo_cpu_frame(&next.cpu, next.timestamp_ms, offset);
        record_demo_history_frame_into(ingestor, offset.saturating_add(1), &next, None, None);
    }
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

#[cfg(test)]
#[path = "../tests/headless/fixture_demo.rs"]
mod tests;

/// Complete bounded process-insight observations for native capture and port tests.
pub mod process_insights;
pub mod process_tree;
