//! macOS system-domain providers built exclusively on safe wrapper crates.
//!
//! Safety policy (ADR-019): `sysinfo` provides CPU/memory/host/disks/network
//! behind safe APIs; `system_profiler` (via the bounded command runner) adds
//! hardware identity and firmware facts (`SPHardwareDataType` -> model name,
//! marketing name, and boot ROM version). Per-interface link speed, carrier
//! state, and Wi-Fi SSID helpers live in the [`network`] submodule (bounded
//! `ifconfig`/`networksetup` shell-outs cached ~10 s; on hosts without those
//! tools the scalars degrade honestly to typed unavailable states).

mod msr_readout;
mod network;
mod rapl_power;
mod smbios_memory;

pub use msr_readout::PendingMsrReadoutProvider;
pub use network::MacNetworkTelemetryProvider;
pub use rapl_power::PendingRaplPowerProvider;
pub use smbios_memory::PendingSmbiosMemoryProvider;

use std::time::{Duration, Instant};

use taskmanager_application::{
    ContainerRollupRequest, CpuTelemetryRequest, GpuEngineRowsRequest, GpuTelemetryRequest,
    HardwareInventoryRequest, HostTelemetryRequest, MemoryTelemetryRequest, MsrReadoutRequest,
    NetworkTelemetryRequest, NpuInventoryRequest, RaplPowerRequest, SmbiosMemoryRequest,
    StorageTelemetryRequest,
};
use taskmanager_core::core::source::{SourceOutcome, SourceStatus};
use taskmanager_core::{
    ComputeTopology, ContainerRollup, CpuMetrics, CpuTelemetryObservation, DeviceId, DeviceState,
    DeviceStatus, FailureKind, FirmwareInfo, GpuEngineRowsSnapshot, HardwareInfo, HostIdentity,
    HostRuntimeFacts, HostRuntimeObservation, KernelInfo, MemoryMetrics,
    MemoryTelemetryObservation, NpuInventorySnapshot, ProviderId, ScalarObservation,
    ScalarObservationGroup,
};
use taskmanager_platform_contract::{CompositeSourceSnapshot, ProviderFailure};
use taskmanager_platform_provider::{
    ContainerRollupProvider, CpuTelemetryProvider, GpuEngineRowsProvider, GpuTelemetryProvider,
    HardwareInventoryProvider, HostTelemetryProvider, MemoryTelemetryProvider, MsrReadoutProvider,
    NetworkTelemetryProvider, NpuInventoryProvider, RaplPowerProvider, SmbiosMemoryProvider,
    StorageTelemetryProvider,
};
use taskmanager_platform_runtime::{
    ProviderRegistration, SystemAuxiliaryExecutors, SystemExecutors, SystemObservationExecutors,
    SystemProviderBindings, SystemProviderBindingsInput,
};

use crate::provider::process_facts::ProcessFactsCache;
use taskmanager_platform_portable::run_with_timeout;

type HostRegistration = ProviderRegistration<HostTelemetryRequest, Box<dyn HostTelemetryProvider>>;
type CpuRegistration = ProviderRegistration<CpuTelemetryRequest, Box<dyn CpuTelemetryProvider>>;
type MemoryRegistration =
    ProviderRegistration<MemoryTelemetryRequest, Box<dyn MemoryTelemetryProvider>>;
type StorageRegistration =
    ProviderRegistration<StorageTelemetryRequest, Box<dyn StorageTelemetryProvider>>;
type NetworkRegistration =
    ProviderRegistration<NetworkTelemetryRequest, Box<dyn NetworkTelemetryProvider>>;
type GpuRegistration = ProviderRegistration<GpuTelemetryRequest, Box<dyn GpuTelemetryProvider>>;
type HardwareInventoryRegistration =
    ProviderRegistration<HardwareInventoryRequest, Box<dyn HardwareInventoryProvider>>;
type ContainerRegistration =
    ProviderRegistration<ContainerRollupRequest, Box<dyn ContainerRollupProvider>>;

const HOST_TELEMETRY_PROVIDER: ProviderId = ProviderId::borrowed("macos.telemetry.host.sysinfo");
const CPU_TELEMETRY_PROVIDER: ProviderId = ProviderId::borrowed("macos.telemetry.cpu.sysinfo");
const MEMORY_TELEMETRY_PROVIDER: ProviderId =
    ProviderId::borrowed("macos.telemetry.memory.sysinfo");
const NETWORK_TELEMETRY_PROVIDER: ProviderId =
    ProviderId::borrowed("macos.telemetry.network.sysinfo");
const HARDWARE_INVENTORY_PROVIDER: ProviderId =
    ProviderId::borrowed("macos.hardware.inventory.system-profiler");

/// macOS container rollup provider. cgroup-v2 — the rollup's only data source
/// — is a Linux-only facility; Docker Desktop for Mac runs containers in an
/// embedded Linux VM whose cgroup tree is invisible to the macOS host process,
/// and macOS itself has no cgroup concept. The capability is registered so the
/// catalog and request lane exist, and the provider always returns a
/// typed-unavailable rollup (`DeviceStatus::Unsupported`) that rides the
/// snapshot lane so the page shows the honest "containers.unsupported" reason.
/// Returning `Err(Unsupported)` would route the failure into `batch.failures`
/// (not `batch.containers_events`), leaving `RootView.containers` at its
/// `empty_healthy` default and rendering a doubly-dishonest "no containers
/// detected" hint on a host where no host-side container view is possible.
pub(super) struct MacContainerRollupProvider;

impl ContainerRollupProvider for MacContainerRollupProvider {
    fn refresh(&mut self, now_ms: u64) -> Result<ContainerRollup, ProviderFailure> {
        // cgroup-v2 is unavailable on macOS by construction: mirror the Linux
        // cgroup-v1 host path — an Unsupported DeviceState inside an
        // `Ok(ContainerRollup::unavailable(..))` — so the typed reason reaches
        // the snapshot lane instead of the failure lane.
        let unsupported = DeviceState::default().transition(DeviceStatus::Unsupported, now_ms);
        Ok(ContainerRollup::unavailable(unsupported))
    }
}

fn available_source(provider: ProviderId, item_count: usize) -> SourceStatus {
    SourceStatus {
        provider,
        outcome: SourceOutcome::Available,
        item_count,
    }
}

fn unavailable_source(provider: ProviderId, failure: FailureKind) -> SourceStatus {
    SourceStatus {
        provider,
        outcome: SourceOutcome::Unavailable(failure),
        item_count: 0,
    }
}

/// Host runtime facts from `sysinfo`: uptime and process count are available;
/// the aggregate macOS thread count is the sum of every per-process thcount
/// in the cached `ps -Ao pid,nice,thcount` snapshot (sysinfo has no aggregate
/// thread count on macOS). When the cache is empty the scalar degrades
/// honestly to typed Unsupported (recorded in ADR-019).
pub struct MacHostTelemetryProvider {
    system: sysinfo::System,
    /// Per-host thread count = sum of every per-process thcount in the cached
    /// `ps` snapshot. The host lane owns its own cache instance, independent of
    /// the process-list lane (each lane refreshes on its own cadence).
    process_facts: ProcessFactsCache,
}

impl MacHostTelemetryProvider {
    pub fn new() -> Self {
        Self {
            system: sysinfo::System::new(),
            process_facts: ProcessFactsCache::new(),
        }
    }
}

impl HostTelemetryProvider for MacHostTelemetryProvider {
    fn refresh(&mut self, observed_at_ms: u64) -> Result<HostRuntimeObservation, ProviderFailure> {
        let mut sources = Vec::new();
        self.system
            .refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let process_count = u64::try_from(self.system.processes().len()).unwrap_or(u64::MAX);
        let uptime_secs = sysinfo::System::uptime();
        // Clone the cached PID -> (nice, threads) map before building the
        // observation (fresh borrows self mutably on a miss).
        let facts_map = self.process_facts.fresh(Instant::now()).clone();
        let mut facts = HostRuntimeFacts {
            uptime_secs: ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
            processes: ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
            threads: ScalarObservation::unavailable(FailureKind::Unsupported),
        };
        if uptime_secs > 0 {
            facts.uptime_secs = ScalarObservation::available(uptime_secs, observed_at_ms);
            sources.push(available_source(HOST_TELEMETRY_PROVIDER, 1));
        }
        if process_count < u64::MAX {
            facts.processes = ScalarObservation::available(process_count, observed_at_ms);
            sources.push(available_source(HOST_TELEMETRY_PROVIDER, 1));
        }
        // Publish a host thread count only when at least one process
        // contributed a real thcount — never fabricate a 0 from an all-None
        // or empty cache.
        let thread_counts: Vec<u64> = facts_map
            .values()
            .filter_map(|(_, threads)| *threads)
            .map(u64::from)
            .collect();
        if !thread_counts.is_empty() {
            let thread_sum: u64 = thread_counts.iter().sum();
            facts.threads = ScalarObservation::available(thread_sum, observed_at_ms);
            sources.push(available_source(HOST_TELEMETRY_PROVIDER, 1));
        }
        if sources.is_empty() {
            return Err(ProviderFailure::TemporarilyUnavailable);
        }
        Ok(HostRuntimeObservation::current(
            facts,
            observed_at_ms,
            sources,
        ))
    }
}

/// CPU telemetry from `sysinfo` (per-core usage + brand + frequency behind a
/// safe API). Cache/temperature/power scalars stay typed unavailable here;
/// temperatures are served by the sensor provider (sysinfo Components).
pub struct MacCpuTelemetryProvider {
    system: sysinfo::System,
}

impl MacCpuTelemetryProvider {
    pub fn new() -> Self {
        Self {
            system: sysinfo::System::new(),
        }
    }
}

impl CpuTelemetryProvider for MacCpuTelemetryProvider {
    fn refresh(&mut self, observed_at_ms: u64) -> Result<CpuTelemetryObservation, ProviderFailure> {
        self.system.refresh_cpu_all();
        self.system.refresh_cpu_usage();
        let cpus = self.system.cpus();
        if cpus.is_empty() {
            return Err(ProviderFailure::TemporarilyUnavailable);
        }
        let core_usages: Vec<f32> = cpus.iter().map(sysinfo::Cpu::cpu_usage).collect();
        let brand = cpus
            .first()
            .map(|cpu| cpu.brand().trim().to_string())
            .filter(|brand| !brand.is_empty());
        let frequency_mhz = cpus
            .first()
            .and_then(|cpu| {
                let freq = cpu.frequency();
                (freq > 0).then_some(freq)
            })
            .filter(|freq| *freq > 0);

        let observations =
            CpuScalarObservationFactory::build(&core_usages, frequency_mhz, observed_at_ms);
        let mut metrics = CpuMetrics::from_observations(observations);
        metrics.brand = brand;
        metrics.physical_cores = sysinfo::System::physical_core_count();
        metrics.logical_cores = Some(core_usages.len());
        let sources = vec![
            available_source(CPU_TELEMETRY_PROVIDER, core_usages.len()),
            unavailable_source(CPU_TELEMETRY_PROVIDER, FailureKind::Unsupported),
        ];
        Ok(CpuTelemetryObservation::current(
            metrics,
            observed_at_ms,
            sources,
        ))
    }
}

struct CpuScalarObservationFactory;

impl CpuScalarObservationFactory {
    fn build(
        core_usages: &[f32],
        frequency_mhz: Option<u64>,
        observed_at_ms: u64,
    ) -> taskmanager_core::CpuScalarObservations {
        let global = core_usages.iter().sum::<f32>() / core_usages.len() as f32;
        taskmanager_core::CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(global, observed_at_ms),
            core_usage_group: ScalarObservationGroup::available(
                core_usages.to_vec(),
                observed_at_ms,
            ),
            frequency_mhz: frequency_mhz.map_or_else(
                || ScalarObservation::unavailable(FailureKind::Unsupported),
                |frequency| ScalarObservation::available(frequency, observed_at_ms),
            ),
            max_frequency_mhz: ScalarObservation::unavailable(FailureKind::Unsupported),
            per_core_frequency_group: ScalarObservationGroup::unavailable(FailureKind::Unsupported),
            temperature_c: ScalarObservation::unavailable(FailureKind::Unsupported),
            per_core_temperature_group: ScalarObservationGroup::unavailable(
                FailureKind::Unsupported,
            ),
            power_w: ScalarObservation::unavailable(FailureKind::Unsupported),
        }
    }
}

/// Memory telemetry from `sysinfo` (host_statistics behind a safe API).
pub struct MacMemoryTelemetryProvider {
    system: sysinfo::System,
    /// Previous (used_bytes, observed_at_ms) for the pressure-rate delta.
    prev_used: Option<(u64, u64)>,
}

impl MacMemoryTelemetryProvider {
    pub fn new() -> Self {
        Self {
            system: sysinfo::System::new(),
            prev_used: None,
        }
    }
}

/// Memory pressure rate (MiB/s) from the in-process `sysinfo` delta — pure
/// arithmetic, no shell-out, honest `None` on the first sample or a zero
/// elapsed window (mirrors the network provider's delta-rate pattern). The
/// rate is signed: negative means memory was freed.
fn used_rate_mib_per_sec(
    prev: Option<(u64, u64)>,
    cur_used: u64,
    observed_at_ms: u64,
) -> Option<f32> {
    let (prev_used, prev_ms) = prev?;
    let elapsed_ms = observed_at_ms.saturating_sub(prev_ms);
    if elapsed_ms == 0 {
        return None;
    }
    let delta_bytes = cur_used as f64 - prev_used as f64;
    let rate_bytes_per_sec = delta_bytes / (elapsed_ms as f64 / 1000.0);
    Some((rate_bytes_per_sec / (1024.0 * 1024.0)) as f32)
}

impl MemoryTelemetryProvider for MacMemoryTelemetryProvider {
    fn refresh(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<MemoryTelemetryObservation, ProviderFailure> {
        self.system.refresh_memory();
        let total = self.system.total_memory();
        if total == 0 {
            return Err(ProviderFailure::TemporarilyUnavailable);
        }
        let available = self.system.available_memory();
        let used = total - available.min(total);
        let swap_total = self.system.total_swap();
        let swap_used = self.system.used_swap();
        // Pressure rate from the in-process sysinfo delta (pure safe Rust);
        // honest None on the first sample.
        let used_rate = used_rate_mib_per_sec(self.prev_used, used, observed_at_ms);
        self.prev_used = Some((used, observed_at_ms));
        let scalar_observations = taskmanager_core::metrics::MemoryScalarObservations {
            total_bytes: ScalarObservation::available(total, observed_at_ms),
            used_bytes: ScalarObservation::available(used, observed_at_ms),
            available_bytes: ScalarObservation::available(available, observed_at_ms),
            swap_total_bytes: ScalarObservation::available(swap_total, observed_at_ms),
            swap_used_bytes: ScalarObservation::available(swap_used, observed_at_ms),
            used_rate_mib_per_sec: match used_rate {
                Some(rate) => ScalarObservation::available(rate, observed_at_ms),
                None => ScalarObservation::unavailable(FailureKind::Unsupported),
            },
        };
        let metrics = MemoryMetrics::from_observations(
            scalar_observations,
            taskmanager_core::MemoryOptionalObservations::unavailable(FailureKind::Unsupported),
        );
        Ok(MemoryTelemetryObservation::current(
            metrics,
            observed_at_ms,
            vec![available_source(MEMORY_TELEMETRY_PROVIDER, 1)],
        ))
    }
}

/// Hardware inventory: host/kernel/topology facts from `sysinfo`; product
/// identity and firmware facts (model identifier, marketing name, boot ROM
/// version, chip) from `system_profiler SPHardwareDataType -json` through the
/// bounded command runner. Fields the tool does not expose (firmware vendor —
/// "Apple" is implicit but not surfaced) stay honestly `None`.
pub struct MacHardwareInventoryProvider {
    system: sysinfo::System,
}

impl MacHardwareInventoryProvider {
    pub fn new() -> Self {
        Self {
            system: sysinfo::System::new(),
        }
    }
}

impl HardwareInventoryProvider for MacHardwareInventoryProvider {
    fn refresh(&mut self) -> Result<CompositeSourceSnapshot<HardwareInfo>, ProviderFailure> {
        self.system.refresh_cpu_all();
        self.system.refresh_memory();

        let hardware_facts = system_profiler_hardware();
        let host = HostIdentity {
            os_name: sysinfo::System::long_os_version(),
            os_version: sysinfo::System::os_version(),
            hostname: sysinfo::System::host_name(),
            ..HostIdentity::default()
        };
        let kernel = KernelInfo {
            version: sysinfo::System::kernel_version(),
            build: None,
            modules_count: None,
            command_line: None,
            compiler: None,
        };
        let cpu_brand = self
            .system
            .cpus()
            .first()
            .map(|cpu| cpu.brand().trim().to_string())
            .filter(|brand| !brand.is_empty())
            .or(hardware_facts.chip_type);
        let topology = ComputeTopology {
            cpu_brand,
            logical_cpu_count: Some(self.system.cpus().len()),
            socket_count: None,
            total_memory_mb: Some(self.system.total_memory() / 1024),
            base_frequency_mhz: macos_base_frequency_mhz(),
            instruction_features: sysctl_instruction_features(),
            ..ComputeTopology::default()
        };
        // Map the system_profiler facts onto FirmwareInfo: machine_model ->
        // product_name (already done pre-enrichment), machine_name (e.g.
        // "MacBook Pro") -> product_version, boot_rom_version ->
        // firmware_version. firmware_vendor has no system_profiler source and
        // stays None rather than fabricating "Apple".
        let firmware = FirmwareInfo {
            product_name: hardware_facts.machine_model,
            product_version: hardware_facts.machine_name,
            firmware_version: hardware_facts.boot_rom_version,
            ..FirmwareInfo::default()
        };
        let firmware_present = firmware.product_name.is_some()
            || firmware.product_version.is_some()
            || firmware.firmware_version.is_some();
        let hardware = HardwareInfo::from_fragments(host, kernel, topology, firmware);
        let mut sources = vec![available_source(HARDWARE_INVENTORY_PROVIDER, 1)];
        if firmware_present {
            sources.push(available_source(HARDWARE_INVENTORY_PROVIDER, 1));
        } else {
            sources.push(unavailable_source(
                HARDWARE_INVENTORY_PROVIDER,
                FailureKind::TemporarilyUnavailable,
            ));
        }
        Ok(CompositeSourceSnapshot::new(hardware, sources))
    }
}

/// Read macOS's static nominal CPU frequency. This is deliberately separate
/// from `sysinfo::Cpu::frequency()`, which is a live observation and belongs
/// to the performance surface. Unknown OIDs, malformed output, and non-macOS
/// builds remain unavailable rather than being inferred from a live sample.
fn macos_base_frequency_mhz() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        let mut command = std::process::Command::new("sysctl");
        command.args(["-n", "hw.cpufrequency"]);
        let output = run_with_timeout(&mut command, Duration::from_secs(2)).ok()?;
        if !output.status.success() {
            return None;
        }
        let hz = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u64>()
            .ok()?;
        let mhz = hz / 1_000_000;
        (mhz > 0 && mhz < 10_000).then_some(mhz)
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// The `sysctl hw.optional.*` key for one neutral instruction feature — the
/// single mapping table for this adapter. Features macOS has no
/// `hw.optional` key for stay `None` (never guessed from the CPU brand).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn sysctl_optional_key(feature: taskmanager_core::CpuInstructionFeature) -> Option<&'static str> {
    match feature {
        taskmanager_core::CpuInstructionFeature::Avx2 => Some("hw.optional.avx2_1"),
        taskmanager_core::CpuInstructionFeature::Avx512F => Some("hw.optional.avx512f"),
        taskmanager_core::CpuInstructionFeature::Neon => Some("hw.optional.neon"),
        taskmanager_core::CpuInstructionFeature::Sve => Some("hw.optional.sve"),
        _ => None,
    }
}

/// Read detected instruction features from `sysctl -n hw.optional.*`. Each
/// probe is one bounded subprocess (`0`/`1` answer); an unknown oid or a
/// failed call contributes nothing. Non-macOS targets return an honest empty
/// list (the sysctl MIB is macOS-specific).
fn sysctl_instruction_features() -> Vec<taskmanager_core::CpuInstructionFeature> {
    #[cfg(target_os = "macos")]
    {
        taskmanager_core::CpuInstructionFeature::ALL
            .iter()
            .copied()
            .filter(|feature| {
                let Some(key) = sysctl_optional_key(*feature) else {
                    return false;
                };
                let mut command = std::process::Command::new("sysctl");
                command.args(["-n", key]);
                match run_with_timeout(&mut command, Duration::from_secs(2)) {
                    Ok(output) if output.status.success() => {
                        String::from_utf8_lossy(&output.stdout).trim() == "1"
                    }
                    _ => false,
                }
            })
            .collect()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}

/// Parsed `SPHardwareDataType` facts. Each field is `None` when that key was
/// missing or unparsable, so partial data still populates whichever fields the
/// tool actually exposed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct HardwareFacts {
    /// `machine_model` (e.g. "MacBookPro18,3") -> FirmwareInfo::product_name.
    machine_model: Option<String>,
    /// `machine_name` (e.g. "MacBook Pro") -> FirmwareInfo::product_version.
    machine_name: Option<String>,
    /// `boot_rom_version` -> FirmwareInfo::firmware_version.
    boot_rom_version: Option<String>,
    /// `chip_type` (e.g. "Apple M1 Pro") -> CPU brand fallback.
    chip_type: Option<String>,
}

/// Parse the `SPHardwareDataType` JSON body into hardware facts. Pure:
/// unit-tested. Returns the default (all-`None`) fragment when the JSON is
/// missing the expected `SPHardwareDataType` array or its first row.
fn parse_hardware_json(body: &[u8]) -> HardwareFacts {
    let Ok(root): Result<serde_json::Value, _> = serde_json::from_slice(body) else {
        return HardwareFacts::default();
    };
    let Some(row) = root
        .get("SPHardwareDataType")
        .and_then(|value| value.as_array())
        .and_then(|rows| rows.first())
    else {
        return HardwareFacts::default();
    };
    let str_field = |key: &str| {
        row.get(key)
            .and_then(|value| value.as_str())
            .map(ToString::to_string)
    };
    HardwareFacts {
        machine_model: str_field("machine_model"),
        machine_name: str_field("machine_name"),
        boot_rom_version: str_field("boot_rom_version"),
        chip_type: str_field("chip_type"),
    }
}

/// `system_profiler SPHardwareDataType -json` -> hardware facts (model name,
/// marketing name, boot ROM version, chip). Fails softly to the all-`None`
/// fragment when the tool is unavailable or output is unparsable.
fn system_profiler_hardware() -> HardwareFacts {
    let mut command = std::process::Command::new("system_profiler");
    command.args(["SPHardwareDataType", "-json"]);
    let Ok(output) = run_with_timeout(&mut command, Duration::from_secs(5)) else {
        return HardwareFacts::default();
    };
    if !output.status.success() {
        return HardwareFacts::default();
    }
    parse_hardware_json(&output.stdout)
}

/// Six independently scheduled macOS observation providers.
pub struct MacSystemObservationProviders {
    host: HostRegistration,
    cpu: CpuRegistration,
    memory: MemoryRegistration,
    storage: StorageRegistration,
    network: NetworkRegistration,
    gpu: GpuRegistration,
    containers: ContainerRegistration,
}

impl MacSystemObservationProviders {
    #[must_use]
    pub fn new<H, C, M, S, N, G, D>(
        host: ProviderRegistration<HostTelemetryRequest, H>,
        cpu: ProviderRegistration<CpuTelemetryRequest, C>,
        memory: ProviderRegistration<MemoryTelemetryRequest, M>,
        storage: ProviderRegistration<StorageTelemetryRequest, S>,
        network: ProviderRegistration<NetworkTelemetryRequest, N>,
        gpu: ProviderRegistration<GpuTelemetryRequest, G>,
        containers: ProviderRegistration<ContainerRollupRequest, D>,
    ) -> Self
    where
        H: HostTelemetryProvider,
        C: CpuTelemetryProvider,
        M: MemoryTelemetryProvider,
        S: StorageTelemetryProvider,
        N: NetworkTelemetryProvider,
        G: GpuTelemetryProvider,
        D: ContainerRollupProvider,
    {
        Self {
            host: host
                .map_provider(|provider| Box::new(provider) as Box<dyn HostTelemetryProvider>),
            cpu: cpu.map_provider(|provider| Box::new(provider) as Box<dyn CpuTelemetryProvider>),
            memory: memory
                .map_provider(|provider| Box::new(provider) as Box<dyn MemoryTelemetryProvider>),
            storage: storage
                .map_provider(|provider| Box::new(provider) as Box<dyn StorageTelemetryProvider>),
            network: network
                .map_provider(|provider| Box::new(provider) as Box<dyn NetworkTelemetryProvider>),
            gpu: gpu.map_provider(|provider| Box::new(provider) as Box<dyn GpuTelemetryProvider>),
            containers: containers
                .map_provider(|provider| Box::new(provider) as Box<dyn ContainerRollupProvider>),
        }
    }

    pub(crate) fn into_runtime(self) -> SystemObservationExecutors {
        let Self {
            host,
            cpu,
            memory,
            storage,
            network,
            gpu,
            containers,
        } = self;
        let mut host = host.into_provider();
        let mut cpu = cpu.into_provider();
        let mut memory = memory.into_provider();
        let mut storage = storage.into_provider();
        let mut network = network.into_provider();
        let mut gpu = gpu.into_provider();
        let mut containers = containers.into_provider();
        SystemObservationExecutors::new(
            move |observed_at_ms| host.refresh(observed_at_ms),
            move |observed_at_ms| cpu.refresh(observed_at_ms),
            move |observed_at_ms| memory.refresh(observed_at_ms),
            move |observed_at_ms| storage.refresh(observed_at_ms),
            move |observed_at_ms| network.refresh(observed_at_ms),
            move |observed_at_ms| gpu.refresh(observed_at_ms),
            move |now_ms| containers.refresh(now_ms),
        )
    }
}

type GpuEngineRowsRegistration =
    ProviderRegistration<GpuEngineRowsRequest, Box<dyn GpuEngineRowsProvider>>;
type NpuInventoryRegistration =
    ProviderRegistration<NpuInventoryRequest, Box<dyn NpuInventoryProvider>>;
type SmbiosMemoryRegistration =
    ProviderRegistration<SmbiosMemoryRequest, Box<dyn SmbiosMemoryProvider>>;
type RaplPowerRegistration = ProviderRegistration<RaplPowerRequest, Box<dyn RaplPowerProvider>>;
type MsrReadoutRegistration = ProviderRegistration<MsrReadoutRequest, Box<dyn MsrReadoutProvider>>;

/// Registered-pending per-engine GPU utilization provider: macOS has no Intel
/// PMU helper crossing, so the capability publishes an honest `Unsupported`
/// descriptor and every read completes with a typed failure — never a
/// fabricated row (G-05 style, ADR-019).
pub struct PendingGpuEngineRowsProvider;

impl GpuEngineRowsProvider for PendingGpuEngineRowsProvider {
    fn read_engine_rows(
        &mut self,
        _device_id: &DeviceId,
    ) -> Result<GpuEngineRowsSnapshot, ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}

/// Registered-pending NPU accelerator inventory provider: reading the Apple
/// Neural Engine needs a powermetrics/IORegistry seam that is not implemented
/// yet, so the capability publishes an honest `Unsupported` descriptor and
/// every read completes with a typed failure — never a fabricated device row
/// (G-05 style, ADR-019).
pub struct PendingNpuInventoryProvider;

impl NpuInventoryProvider for PendingNpuInventoryProvider {
    fn read_inventory(
        &mut self,
        _observed_at_ms: u64,
    ) -> Result<NpuInventorySnapshot, ProviderFailure> {
        Err(ProviderFailure::Unsupported)
    }
}

/// Hardware operations outside domain observation ownership.
pub struct MacSystemAuxiliaryProviders {
    hardware_inventory: HardwareInventoryRegistration,
    gpu_engine_rows: GpuEngineRowsRegistration,
    npu_inventory: NpuInventoryRegistration,
    smbios_memory: SmbiosMemoryRegistration,
    rapl_power: RaplPowerRegistration,
    msr_readout: MsrReadoutRegistration,
}

impl MacSystemAuxiliaryProviders {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new<P, E, N, S, R, M>(
        hardware_inventory: ProviderRegistration<HardwareInventoryRequest, P>,
        gpu_engine_rows: ProviderRegistration<GpuEngineRowsRequest, E>,
        npu_inventory: ProviderRegistration<NpuInventoryRequest, N>,
        smbios_memory: ProviderRegistration<SmbiosMemoryRequest, S>,
        rapl_power: ProviderRegistration<RaplPowerRequest, R>,
        msr_readout: ProviderRegistration<MsrReadoutRequest, M>,
    ) -> Self
    where
        P: HardwareInventoryProvider,
        E: GpuEngineRowsProvider,
        N: NpuInventoryProvider,
        S: SmbiosMemoryProvider,
        R: RaplPowerProvider,
        M: MsrReadoutProvider,
    {
        Self {
            hardware_inventory: hardware_inventory
                .map_provider(|provider| Box::new(provider) as Box<dyn HardwareInventoryProvider>),
            gpu_engine_rows: gpu_engine_rows
                .map_provider(|provider| Box::new(provider) as Box<dyn GpuEngineRowsProvider>),
            npu_inventory: npu_inventory
                .map_provider(|provider| Box::new(provider) as Box<dyn NpuInventoryProvider>),
            smbios_memory: smbios_memory
                .map_provider(|provider| Box::new(provider) as Box<dyn SmbiosMemoryProvider>),
            rapl_power: rapl_power
                .map_provider(|provider| Box::new(provider) as Box<dyn RaplPowerProvider>),
            msr_readout: msr_readout
                .map_provider(|provider| Box::new(provider) as Box<dyn MsrReadoutProvider>),
        }
    }

    pub(crate) fn into_runtime(self) -> SystemAuxiliaryExecutors {
        let Self {
            hardware_inventory,
            gpu_engine_rows,
            npu_inventory,
            smbios_memory,
            rapl_power,
            msr_readout,
        } = self;
        let mut hardware_inventory = hardware_inventory.into_provider();
        let mut gpu_engine_rows = gpu_engine_rows.into_provider();
        let mut npu_inventory = npu_inventory.into_provider();
        let mut smbios_memory = smbios_memory.into_provider();
        let mut rapl_power = rapl_power.into_provider();
        let mut msr_readout = msr_readout.into_provider();
        SystemAuxiliaryExecutors::new(move || hardware_inventory.refresh())
            .with_gpu_engine_rows(move |request| {
                gpu_engine_rows.read_engine_rows(&request.device_id)
            })
            .with_npu_inventory(move |observed_at_ms| npu_inventory.read_inventory(observed_at_ms))
            .with_smbios_memory(move || smbios_memory.read_memory_smbios())
            .with_rapl_power(move || rapl_power.read_package_power())
            .with_msr_readout(move || msr_readout.read_msr_readouts())
    }
}

/// macOS system provider composition grouped by scheduling responsibility.
pub struct MacSystemProviders {
    observations: MacSystemObservationProviders,
    auxiliary: MacSystemAuxiliaryProviders,
}

impl MacSystemProviders {
    #[must_use]
    pub const fn new(
        observations: MacSystemObservationProviders,
        auxiliary: MacSystemAuxiliaryProviders,
    ) -> Self {
        Self {
            observations,
            auxiliary,
        }
    }

    pub(crate) fn runtime_bindings(&self) -> SystemProviderBindings {
        SystemProviderBindings::new(SystemProviderBindingsInput {
            host: self.observations.host.binding(),
            cpu: self.observations.cpu.binding(),
            memory: self.observations.memory.binding(),
            storage: self.observations.storage.binding(),
            network: self.observations.network.binding(),
            gpu: self.observations.gpu.binding(),
            hardware_inventory: self.auxiliary.hardware_inventory.binding(),
            containers: self.observations.containers.binding(),
        })
        .with_gpu_engine_rows(&self.auxiliary.gpu_engine_rows)
        .with_npu_inventory(&self.auxiliary.npu_inventory)
        .with_smbios_memory(&self.auxiliary.smbios_memory)
        .with_rapl_power(&self.auxiliary.rapl_power)
        .with_msr_readout(&self.auxiliary.msr_readout)
    }

    pub(crate) fn into_runtime(self) -> SystemExecutors {
        SystemExecutors::new(
            self.observations.into_runtime(),
            self.auxiliary.into_runtime(),
        )
    }
}

#[cfg(test)]
#[path = "../../tests/headless/macos_provider_system.rs"]
mod tests;
