//! Deterministic, non-destructive data used by headless tests and visual capture.

use std::collections::BTreeMap;

use taskmanager_core::core::device_state::{DeviceLifecycle, DevicePresence, DeviceState};
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_core::core::identity::{DeviceGeneration, DeviceId, ProviderId};
use taskmanager_core::core::metrics::{
    CpuMetrics, CpuScalarObservations, CpuTelemetryObservation, DiskMetrics,
    DiskScalarObservations, GpuMetrics, GpuScalarObservations, GpuTelemetryObservation,
    MemoryCompositionObservations, MemoryMetrics, MemoryOptionalObservations,
    MemoryScalarObservations, MemoryTelemetryObservation, NetworkAdapterType, NetworkMetrics,
    NetworkScalarObservations, NetworkTelemetryObservation, NetworkWirelessObservations,
    OptionalObservation, ScalarObservation, ScalarObservationGroup,
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
    CapabilityDescriptor, CapabilityId, CapabilitySnapshot, CapabilityStatus, RequestId,
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
    Containers(Option<taskmanager_core::core::process_telemetry::ContainerRollup>),
    PowerSupplies(Option<PowerSupplySnapshot>),
    Sensors(Option<SensorCenterSnapshot>),
    NpuInventory(Option<taskmanager_core::core::npu::NpuInventorySnapshot>),
    DirectoryUsage(Option<taskmanager_core::core::directory_usage::DirectoryUsageSnapshot>),
    StartupBootEvidence(Option<taskmanager_core::core::startup::StartupBootEvidenceSnapshot>),
    ServicesSource(Option<Vec<SourceStatus>>),
    StartupSource(Option<Vec<SourceStatus>>),
    SessionsSource(Option<Vec<SourceStatus>>),
    ProcessAffinity(Option<taskmanager_application::ProcessAffinityReady>),
    ProcessInsights(Box<Option<taskmanager_application::ProjectedProcessInsights>>),
    ActiveAlerts(Vec<taskmanager_core::core::alerts::Alert>),
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
    edit: impl FnOnce(&mut Option<taskmanager_core::core::process_telemetry::ContainerRollup>),
) {
    app.edit_fixture_containers(edit);
}

/// Install an already-admitted synthetic batch request for a correlated
/// completion fixture. This preserves the same attempt → request transition
/// as a real platform submission.
pub fn seed_process_batch_loading(
    app: &mut ShellApp,
    intent: taskmanager_core::core::process::ProcessBatchIntent,
    request_id: RequestId,
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
            status: CapabilityStatus::PermissionRequired,
            providers: vec![ProviderId::borrowed("fixture.gpu-engines")],
            observed_at_ms: 0,
            last_success_at_ms: None,
        },
    ]));
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

/// One CPU cluster: physical cores of ONE class sharing one SMT shape.
/// Free composition across any number of clusters is what makes the model
/// cover the market: homogeneous AMD/server parts are ONE cluster with SMT
/// on every core, Intel P+E is two, Intel P+E+LP-E is three, Apple/ARM
/// big.LITTLE and Snapdragon-style 1X+5P+2LP shapes are two–three, and a
/// fourth cluster costs one more entry — never a new model.
#[derive(Clone, Copy, Debug)]
pub struct CpuClusterSpec {
    /// The class every core in this cluster reports.
    pub kind: taskmanager_core::core::hardware::CpuType,
    /// Physical cores in this cluster.
    pub physical_cores: usize,
    /// Logical CPUs per physical core: 1 = no SMT, 2 = SMT/Hyper-Threading.
    /// ANY cluster may carry SMT — the model does not bake Intel's
    /// "E-cores have no SMT" marketplace accident into a law (AMD Zen runs
    /// SMT on every core).
    pub threads_per_core: usize,
}

/// A CPU topology spec: an ordered cluster list. This is the fixture-side
/// GENERATOR only. The core-ized truth every frontend consumes is its
/// DERIVATION — the per-logical-CPU `hardware.cpu_types` array plus the
/// declared counts (`cpu_types`/`cpu_cores`/`physical_cores`/
/// `logical_cores`) — so GPUI, Iced and TUI all read the same elastic fields
/// and none of them hardcodes a topology.
#[derive(Clone, Debug)]
pub struct CpuTopologySpec {
    pub clusters: Vec<CpuClusterSpec>,
}

impl CpuTopologySpec {
    /// Physical cores summed over all clusters.
    pub fn physical_cores(&self) -> usize {
        self.clusters
            .iter()
            .map(|cluster| cluster.physical_cores)
            .sum()
    }

    /// Logical CPUs summed over all clusters
    /// (Σ physical × threads-per-core).
    pub fn logical_cores(&self) -> usize {
        self.clusters
            .iter()
            .map(|cluster| cluster.physical_cores * cluster.threads_per_core)
            .sum()
    }

    /// Per-logical-CPU type in cluster order (the order every consumer —
    /// grid grouping, captions — must preserve).
    pub fn cpu_types(&self) -> Vec<taskmanager_core::core::hardware::CpuType> {
        self.clusters
            .iter()
            .flat_map(|cluster| {
                tiled(
                    &[cluster.kind],
                    cluster.physical_cores * cluster.threads_per_core,
                )
            })
            .collect()
    }

    /// Per-logical-CPU utilization seed: Performance clusters run busy,
    /// Efficient clusters moderate, LowPower clusters near idle, Unknown
    /// falls back to the moderate band. Tiling continues across same-kind
    /// clusters (the offset walks forward), keeping values deterministic and
    /// plausible for any composition.
    pub fn core_usage(&self) -> Vec<f32> {
        let mut values = Vec::with_capacity(self.logical_cores());
        for cluster in &self.clusters {
            let (pattern, offset) = (
                usage_pattern(cluster.kind),
                self.painted_logical_of_kind(cluster.kind),
            );
            values.extend(tiled_offset(pattern, cluster.logical_cpus(), offset));
        }
        values
    }

    /// Per-logical-CPU clock seed (MHz): Performance clusters boost highest,
    /// LowPower clusters sit at their floor.
    pub fn frequencies_mhz(&self) -> Vec<u64> {
        let mut values = Vec::with_capacity(self.logical_cores());
        for cluster in &self.clusters {
            let (pattern, offset) = (
                frequency_pattern(cluster.kind),
                self.painted_logical_of_kind(cluster.kind),
            );
            values.extend(tiled_offset(pattern, cluster.logical_cpus(), offset));
        }
        values
    }

    /// Per-logical-CPU temperature seed (°C), tracking the utilization shape.
    pub fn temperatures_c(&self) -> Vec<f32> {
        let mut values = Vec::with_capacity(self.logical_cores());
        for cluster in &self.clusters {
            let (pattern, offset) = (
                temperature_pattern(cluster.kind),
                self.painted_logical_of_kind(cluster.kind),
            );
            values.extend(tiled_offset(pattern, cluster.logical_cpus(), offset));
        }
        values
    }

    /// How many logical CPUs of `kind` precede this cluster — the tiling
    /// offset so same-kind clusters continue the pattern instead of
    /// restarting it.
    fn painted_logical_of_kind(&self, kind: taskmanager_core::core::hardware::CpuType) -> usize {
        self.clusters
            .iter()
            .take_while(|cluster| cluster.kind != kind)
            .map(|cluster| cluster.logical_cpus())
            .sum()
    }
}

impl CpuClusterSpec {
    /// Logical CPUs this cluster paints.
    pub const fn logical_cpus(&self) -> usize {
        self.physical_cores * self.threads_per_core
    }
}

/// The demo host profile: an Ultra 7 358H-class hybrid part — 6 P-cores with
/// SMT (12 logical) + 8 E-cores + 2 LP-E-cores = 16 physical / 22 logical.
/// One profile INSTANCE of the cluster-list generator; the shape itself is
/// not baked into any model, and any other market topology (homogeneous
/// AMD/server with SMT on every core, Snapdragon-style 1X+5P+2LP,
/// Apple-style big.LITTLE, a fourth cluster…) is one literal swap away.
pub fn demo_cpu_topology() -> CpuTopologySpec {
    use taskmanager_core::core::hardware::CpuType;
    CpuTopologySpec {
        clusters: vec![
            CpuClusterSpec {
                kind: CpuType::Performance,
                physical_cores: 6,
                threads_per_core: 2,
            },
            CpuClusterSpec {
                kind: CpuType::Efficient,
                physical_cores: 8,
                threads_per_core: 1,
            },
            CpuClusterSpec {
                kind: CpuType::LowPower,
                physical_cores: 2,
                threads_per_core: 1,
            },
        ],
    }
}

/// Per-kind base patterns. A cluster of size *n* tiles the *n* entries of its
/// kind's pattern starting at the kind's running offset, so the demo profile
/// reproduces the original hand-written values exactly while any other
/// topology stays deterministic and plausible. Patterns live per KIND (not
/// per full vector) precisely so topology changes never require re-writing
/// literal vectors.
fn usage_pattern(kind: taskmanager_core::core::hardware::CpuType) -> &'static [f32] {
    use taskmanager_core::core::hardware::CpuType;
    match kind {
        CpuType::Performance => &[
            52.0, 41.0, 34.0, 22.0, 57.5, 33.0, 48.5, 39.0, 44.5, 28.0, 61.5, 36.0,
        ],
        CpuType::Efficient => &[18.0, 25.5, 12.0, 31.0, 9.5, 22.5, 15.0, 27.0],
        CpuType::LowPower => &[4.5, 7.0],
        CpuType::Unknown => &[21.0, 33.0],
    }
}

fn frequency_pattern(kind: taskmanager_core::core::hardware::CpuType) -> &'static [u64] {
    use taskmanager_core::core::hardware::CpuType;
    match kind {
        CpuType::Performance => &[
            4_820, 4_760, 4_910, 4_640, 4_750, 4_690, 4_880, 4_710, 4_800, 4_655, 4_940, 4_725,
        ],
        CpuType::Efficient => &[3_380, 3_450, 3_360, 3_420, 3_310, 3_470, 3_390, 3_440],
        CpuType::LowPower => &[1_250, 1_180],
        CpuType::Unknown => &[2_800, 2_650],
    }
}

fn temperature_pattern(kind: taskmanager_core::core::hardware::CpuType) -> &'static [f32] {
    use taskmanager_core::core::hardware::CpuType;
    match kind {
        CpuType::Performance => &[
            58.0, 56.5, 61.0, 54.0, 57.5, 55.5, 59.5, 56.0, 58.5, 54.5, 62.0, 57.0,
        ],
        CpuType::Efficient => &[49.0, 50.5, 48.0, 51.0, 47.5, 50.0, 48.5, 49.5],
        CpuType::LowPower => &[43.0, 42.5],
        CpuType::Unknown => &[47.0, 48.0],
    }
}

fn tiled_offset<T: Copy>(pattern: &[T], len: usize, offset: usize) -> Vec<T> {
    (0..len)
        .map(|index| pattern[(index + offset) % pattern.len()])
        .collect()
}

fn tiled<T: Copy>(pattern: &[T], len: usize) -> Vec<T> {
    tiled_offset(pattern, len, 0)
}

/// Every per-core seed vector in the demo snapshot derives from
/// [`demo_cpu_topology`], so the fixture can never contradict its own
/// topology declaration: vector lengths, `cpu_types` and the declared
/// physical/logical counts are one derivation apart.
fn core_usage_seed() -> Vec<f32> {
    demo_cpu_topology().core_usage()
}

fn per_core_frequency_seed() -> Vec<u64> {
    demo_cpu_topology().frequencies_mhz()
}

fn per_core_temperature_seed() -> Vec<f32> {
    demo_cpu_topology().temperatures_c()
}

fn cpu_types_seed() -> Vec<taskmanager_core::core::hardware::CpuType> {
    demo_cpu_topology().cpu_types()
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

#[cfg(test)]
mod topology_tests {
    use super::{CpuClusterSpec, CpuTopologySpec};
    use taskmanager_core::core::hardware::CpuType;

    fn cluster(kind: CpuType, physical_cores: usize, threads_per_core: usize) -> CpuClusterSpec {
        CpuClusterSpec {
            kind,
            physical_cores,
            threads_per_core,
        }
    }

    /// The market's real shapes must all be expressible as free cluster
    /// composition — and every composition must generate self-consistent
    /// seeds (all per-core vectors exactly `logical_cores` long, types
    /// agreeing with the cluster arithmetic, declared counts derived).
    /// Covers: Intel hybrid 3-cluster, homogeneous AMD/server with SMT on
    /// EVERY core (one cluster), Snapdragon-style 1X+5P+2LP three-cluster,
    /// Apple-style two-cluster, and a FOUR-cluster shape (the model has no
    /// cluster-count ceiling).
    #[test]
    fn market_cpu_shapes_are_expressible_and_self_consistent() {
        let shapes: Vec<(&str, CpuTopologySpec, usize, usize)> = vec![
            (
                "Intel Ultra 7 358H (P+SMT, E, LP-E)",
                CpuTopologySpec {
                    clusters: vec![
                        cluster(CpuType::Performance, 6, 2),
                        cluster(CpuType::Efficient, 8, 1),
                        cluster(CpuType::LowPower, 2, 1),
                    ],
                },
                16,
                22,
            ),
            (
                "AMD Ryzen 9 7950X (homogeneous, SMT on EVERY core)",
                CpuTopologySpec {
                    clusters: vec![cluster(CpuType::Performance, 16, 2)],
                },
                16,
                32,
            ),
            (
                "Snapdragon-style 1X+5P+2LP three-cluster",
                CpuTopologySpec {
                    clusters: vec![
                        cluster(CpuType::Performance, 1, 1),
                        cluster(CpuType::Performance, 5, 1),
                        cluster(CpuType::Efficient, 2, 1),
                    ],
                },
                8,
                8,
            ),
            (
                "Apple-style big.LITTLE two-cluster",
                CpuTopologySpec {
                    clusters: vec![
                        cluster(CpuType::Performance, 4, 1),
                        cluster(CpuType::Efficient, 4, 1),
                    ],
                },
                8,
                8,
            ),
            (
                "four clusters (no model ceiling)",
                CpuTopologySpec {
                    clusters: vec![
                        cluster(CpuType::Performance, 2, 2),
                        cluster(CpuType::Performance, 4, 1),
                        cluster(CpuType::Efficient, 4, 1),
                        cluster(CpuType::LowPower, 2, 1),
                    ],
                },
                12,
                14,
            ),
        ];

        for (name, topology, physical, logical) in shapes {
            assert_eq!(topology.physical_cores(), physical, "{name}");
            assert_eq!(topology.logical_cores(), logical, "{name}");
            assert_eq!(topology.core_usage().len(), logical, "{name}");
            assert_eq!(topology.frequencies_mhz().len(), logical, "{name}");
            assert_eq!(topology.temperatures_c().len(), logical, "{name}");
            assert_eq!(topology.cpu_types().len(), logical, "{name}");
            // SMT factor is per-cluster: a cluster with 2 threads paints two
            // logical CPUs of its kind for every physical core.
            for cluster in &topology.clusters {
                let painted = topology
                    .cpu_types()
                    .iter()
                    .filter(|kind| **kind == cluster.kind)
                    .count();
                assert!(
                    painted >= cluster.logical_cpus(),
                    "{name}: cluster {cluster:?} must fit inside the painted types"
                );
            }
        }
    }

    /// SMT is per-cluster and NOT limited to P-cores: an Efficiency cluster
    /// with `threads_per_core = 2` (a hypothetical AMD-style homogeneous
    /// efficiency part, or any future shape) paints two logical CPUs per
    /// physical core.
    #[test]
    fn any_cluster_may_carry_smt() {
        let topology = CpuTopologySpec {
            clusters: vec![cluster(CpuType::Efficient, 4, 2)],
        };
        assert_eq!(topology.logical_cores(), 8);
        let usage = topology.core_usage();
        // The kind's pattern tiles across BOTH threads of the first physical
        // core (18.0 / 25.5), proving per-thread values instead of a copied
        // pair.
        assert_eq!(usage[..2], [18.0, 25.5]);
    }

    /// The demo profile keeps reproducing the original hand-written demo
    /// values byte-for-byte (16 physical / 22 logical, C00 = 52% · 4.82 GHz ·
    /// 58 °C), so evidence captures stay comparable across the refactor.
    #[test]
    fn demo_profile_preserves_the_original_seed_values() {
        let demo = super::demo_cpu_topology();
        assert_eq!(demo.physical_cores(), 16);
        assert_eq!(demo.logical_cores(), 22);
        let usage = demo.core_usage();
        assert_eq!(usage[..4], [52.0, 41.0, 34.0, 22.0]);
        let frequencies = demo.frequencies_mhz();
        assert_eq!(frequencies[0], 4_820);
        let temperatures = demo.temperatures_c();
        assert_eq!(temperatures[0], 58.0);
    }
}
