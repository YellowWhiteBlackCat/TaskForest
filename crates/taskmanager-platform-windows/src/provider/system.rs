//! Windows system-domain providers built on mature safe crates plus the small,
//! audited native API boundary in `taskmanager-windows-api` (ADR-031).
//!
//! `sysinfo` owns CPU/memory/host/storage/network facts, `raw-cpuid` owns
//! advertised CPU frequency facts, and `nvml-wrapper` owns NVIDIA telemetry.
//! Host thread totals, processor topology/cache, NIC metadata, and user locale
//! use the typed native boundary. Domains without an accepted safe accessor
//! publish typed `Unsupported` observations; they never invoke a command
//! interpreter or fabricate a value. The one bounded-command consumer here is
//! the WSL rollup lane, which runs the fixed `wsl.exe` management binary (see
//! `provider::system::wsl`) — never a shell and never an interpreter.

use taskmanager_application::{
    ContainerRollupRequest, CpuTelemetryRequest, GpuEngineRowsRequest, GpuTelemetryRequest,
    HardwareInventoryRequest, HostTelemetryRequest, MemoryTelemetryRequest,
    NetworkTelemetryRequest, NpuInventoryRequest, StorageTelemetryRequest,
};
use taskmanager_core::{
    ContainerRollup, DeviceId, DeviceState, FailureKind, GpuEngineKind, GpuEngineMetric,
    GpuEngineRowsSnapshot, HostRuntimeFacts, HostRuntimeObservation, MemoryMetrics,
    MemoryTelemetryObservation, NpuDevice, NpuInventorySnapshot, NpuMemoryReport, ProviderId,
    ScalarObservation,
};
use taskmanager_platform_contract::{ProviderFailure, SourceOutcome, SourceStatus};
use taskmanager_platform_provider::{
    ContainerRollupProvider, CpuTelemetryProvider, GpuEngineRowsProvider, GpuTelemetryProvider,
    HardwareInventoryProvider, HostTelemetryProvider, MemoryTelemetryProvider,
    NetworkTelemetryProvider, NpuInventoryProvider, StorageTelemetryProvider,
};
use taskmanager_platform_runtime::{
    ProviderRegistration, SystemAuxiliaryExecutors, SystemExecutors, SystemObservationExecutors,
    SystemProviderBindings, SystemProviderBindingsInput,
};

mod cpu_freq;
mod cpu_info;
mod disk;
mod gpu;
mod hardware_inventory;
mod network;
mod smbios_info;
mod virtualization;
mod wsl;

pub use cpu_freq::WinCpuTelemetryProvider;
pub use disk::WinStorageTelemetryProvider;
pub use gpu::WinGpuTelemetryProvider;
pub use hardware_inventory::WinHardwareInventoryProvider;
pub use network::WinNetworkTelemetryProvider;

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

const HOST_TELEMETRY_PROVIDER: ProviderId = ProviderId::borrowed("windows.telemetry.host.sysinfo");
const HOST_PERFORMANCE_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.telemetry.host.performance-api");
const CPU_TELEMETRY_PROVIDER: ProviderId = ProviderId::borrowed("windows.telemetry.cpu.sysinfo");
const GPU_TELEMETRY_PROVIDER: ProviderId = ProviderId::borrowed("windows.telemetry.gpu.nvml");
const MEMORY_TELEMETRY_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.telemetry.memory.sysinfo");
const STORAGE_TELEMETRY_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.telemetry.storage.sysinfo");
const NETWORK_TELEMETRY_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.telemetry.network.sysinfo");
const HARDWARE_INVENTORY_PROVIDER: ProviderId =
    ProviderId::borrowed("windows.hardware.inventory.sysinfo");

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

/// Host runtime facts from the native performance-information API plus the
/// process-independent `sysinfo` uptime query. The host lane never scans the
/// process table merely to count rows: the bounded native seam already returns
/// process and thread totals, while detailed process metadata remains in the
/// process-list lane.
pub struct WinHostTelemetryProvider;

impl WinHostTelemetryProvider {
    pub fn new() -> Self {
        Self
    }
}

impl HostTelemetryProvider for WinHostTelemetryProvider {
    fn refresh(&mut self, observed_at_ms: u64) -> Result<HostRuntimeObservation, ProviderFailure> {
        let mut processes = ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable);
        let mut threads = ScalarObservation::unavailable(FailureKind::Unsupported);
        let mut sources = Vec::new();
        let uptime_secs = sysinfo::System::uptime();
        let uptime = if uptime_secs < u64::MAX {
            sources.push(available_source(HOST_TELEMETRY_PROVIDER, 1));
            ScalarObservation::available(uptime_secs, observed_at_ms)
        } else {
            sources.push(unavailable_source(
                HOST_TELEMETRY_PROVIDER,
                FailureKind::TemporarilyUnavailable,
            ));
            ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable)
        };
        match taskmanager_windows_api::system_performance() {
            Ok(performance) => {
                processes = ScalarObservation::available(
                    u64::from(performance.process_count),
                    observed_at_ms,
                );
                threads = ScalarObservation::available(
                    u64::from(performance.thread_count),
                    observed_at_ms,
                );
                sources.push(available_source(HOST_PERFORMANCE_PROVIDER, 2));
            }
            Err(taskmanager_windows_api::WindowsApiError::Unsupported) => {
                sources.push(unavailable_source(
                    HOST_PERFORMANCE_PROVIDER,
                    FailureKind::Unsupported,
                ));
            }
            Err(_) => {
                processes = ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable);
                threads = ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable);
                sources.push(unavailable_source(
                    HOST_PERFORMANCE_PROVIDER,
                    FailureKind::TemporarilyUnavailable,
                ));
            }
        }
        let facts = HostRuntimeFacts {
            uptime_secs: uptime,
            processes,
            threads,
        };
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

/// Memory telemetry from `sysinfo` (GlobalMemoryStatusEx behind a safe API).
pub struct WinMemoryTelemetryProvider {
    system: sysinfo::System,
    /// Previous (used_bytes, observed_at_ms) for the pressure-rate delta.
    prev_used: Option<(u64, u64)>,
}

impl WinMemoryTelemetryProvider {
    pub fn new() -> Self {
        Self {
            system: sysinfo::System::new(),
            prev_used: None,
        }
    }
}

/// Memory pressure rate (MiB/s) from the in-process `sysinfo` delta — pure
/// arithmetic, no shell-out. The first/zero interval is temporarily
/// unavailable, a backwards observation clock is an identity change, and
/// signed values remain valid because negative means memory was freed.
fn used_rate_mib_per_sec(
    prev: Option<(u64, u64)>,
    cur_used: u64,
    observed_at_ms: u64,
) -> ScalarObservation<f32> {
    let Some((prev_used, prev_ms)) = prev else {
        return ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable);
    };
    let Some(elapsed_ms) = observed_at_ms.checked_sub(prev_ms) else {
        return ScalarObservation::unavailable(FailureKind::IdentityChanged);
    };
    if elapsed_ms == 0 {
        return ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable);
    }
    let delta_bytes = cur_used as f64 - prev_used as f64;
    let rate_bytes_per_sec = delta_bytes / (elapsed_ms as f64 / 1000.0);
    let rate = rate_bytes_per_sec / (1024.0 * 1024.0);
    if !rate.is_finite() || rate < f32::MIN as f64 || rate > f32::MAX as f64 {
        return ScalarObservation::unavailable(FailureKind::ProviderFault);
    }
    ScalarObservation::available(rate as f32, observed_at_ms)
}

impl MemoryTelemetryProvider for WinMemoryTelemetryProvider {
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

        let perf_info = taskmanager_windows_api::system_performance().ok();
        let smbios_facts = self::smbios_info::query_memory_hardware_info();

        let cached_bytes = perf_info.map(|perf| {
            (perf.system_cache_pages as u64).saturating_mul(perf.page_size_bytes as u64)
        });
        let committed_bytes = perf_info.map(|perf| {
            (perf.commit_total_pages as u64).saturating_mul(perf.page_size_bytes as u64)
        });
        let commit_limit_bytes = perf_info.map(|perf| {
            (perf.commit_limit_pages as u64).saturating_mul(perf.page_size_bytes as u64)
        });
        let paged_pool_bytes = perf_info.map(|perf| {
            (perf.kernel_paged_pages as u64).saturating_mul(perf.page_size_bytes as u64)
        });
        let nonpaged_pool_bytes = perf_info.map(|perf| {
            (perf.kernel_nonpaged_pages as u64).saturating_mul(perf.page_size_bytes as u64)
        });
        let free_bytes = perf_info.map(|perf| {
            (perf.physical_available_pages as u64).saturating_mul(perf.page_size_bytes as u64)
        });
        let physical_total_bytes = perf_info.map(|perf| {
            (perf.physical_total_pages as u64).saturating_mul(perf.page_size_bytes as u64)
        });
        let hardware_reserved_bytes = smbios_facts
            .as_ref()
            .and_then(|facts| facts.total_installed_bytes)
            .and_then(|installed| physical_total_bytes.map(|phys| installed.saturating_sub(phys)));

        let used_rate = used_rate_mib_per_sec(self.prev_used, used, observed_at_ms);
        self.prev_used = Some((used, observed_at_ms));
        let observations = MemoryScalarObservationFactory::build(
            total,
            used,
            available,
            swap_total,
            swap_used,
            used_rate,
            observed_at_ms,
        );
        let mut optional_obs = taskmanager_core::MemoryOptionalObservations::default();
        if let Some(active) = Some(used) {
            optional_obs.composition.active_bytes =
                taskmanager_core::OptionalObservation::present(active, observed_at_ms);
        }
        if let Some(inactive) = nonpaged_pool_bytes {
            optional_obs.composition.inactive_bytes =
                taskmanager_core::OptionalObservation::present(inactive, observed_at_ms);
        }
        if let Some(free) = free_bytes {
            optional_obs.composition.free_bytes =
                taskmanager_core::OptionalObservation::present(free, observed_at_ms);
        }
        if let Some(cached) = cached_bytes {
            optional_obs.composition.cached_bytes =
                taskmanager_core::OptionalObservation::present(cached, observed_at_ms);
        }
        if let Some(reclaimable) = paged_pool_bytes {
            optional_obs.composition.reclaimable_bytes =
                taskmanager_core::OptionalObservation::present(reclaimable, observed_at_ms);
        }
        if let Some(committed) = committed_bytes {
            optional_obs.virtual_memory_commit.committed_bytes =
                taskmanager_core::OptionalObservation::present(committed, observed_at_ms);
        }
        if let Some(limit) = commit_limit_bytes {
            optional_obs.virtual_memory_commit.limit_bytes =
                taskmanager_core::OptionalObservation::present(limit, observed_at_ms);
        }
        // Compressed-memory store: the kernel process snapshot's "Memory
        // Compression" working set (hidden from the `sysinfo` table). An
        // absent store (`Ok(None)`, e.g. server SKUs) and a query failure
        // degrade exactly like the other optional native facts above — the
        // slot keeps its never-observed default rather than a fabricated
        // zero or an error failing the whole memory observation.
        if let Ok(Some(compressed_used)) =
            taskmanager_windows_api::query_memory_compression_used_bytes()
        {
            optional_obs.compression.compressed_memory_used_bytes =
                taskmanager_core::OptionalObservation::present(compressed_used, observed_at_ms);
        }
        if let Some(reserved) = hardware_reserved_bytes {
            optional_obs.hardware_reserved_bytes =
                taskmanager_core::OptionalObservation::present(reserved, observed_at_ms);
        }
        if let Some(ref smbios) = smbios_facts {
            if let Some(speed) = smbios.speed_mhz {
                optional_obs.modules.speed_mhz =
                    taskmanager_core::OptionalObservation::present(speed, observed_at_ms);
            }
            if let Some(used) = smbios.slots_used {
                optional_obs.modules.slots_used =
                    taskmanager_core::OptionalObservation::present(used, observed_at_ms);
            }
            if let Some(total) = smbios.slots_total {
                optional_obs.modules.slots_total =
                    taskmanager_core::OptionalObservation::present(total, observed_at_ms);
            }
            if let Some(ref t) = smbios.module_type {
                optional_obs.modules.module_type =
                    taskmanager_core::OptionalObservation::present(t.clone(), observed_at_ms);
            }
            if let Some(ref m) = smbios.module_manufacturer {
                optional_obs.modules.manufacturer =
                    taskmanager_core::OptionalObservation::present(m.clone(), observed_at_ms);
            }
            if let Some(ref f) = smbios.module_form_factor {
                optional_obs.modules.form_factor =
                    taskmanager_core::OptionalObservation::present(f.clone(), observed_at_ms);
            }
        }
        let metrics = MemoryMetrics::from_observations(observations, optional_obs);

        Ok(MemoryTelemetryObservation::current(
            metrics,
            observed_at_ms,
            vec![available_source(MEMORY_TELEMETRY_PROVIDER, 1)],
        ))
    }
}

struct MemoryScalarObservationFactory;

impl MemoryScalarObservationFactory {
    fn build(
        total: u64,
        used: u64,
        available: u64,
        swap_total: u64,
        swap_used: u64,
        used_rate: ScalarObservation<f32>,
        observed_at_ms: u64,
    ) -> taskmanager_core::metrics::MemoryScalarObservations {
        taskmanager_core::metrics::MemoryScalarObservations {
            total_bytes: ScalarObservation::available(total, observed_at_ms),
            used_bytes: ScalarObservation::available(used, observed_at_ms),
            available_bytes: ScalarObservation::available(available, observed_at_ms),
            swap_total_bytes: ScalarObservation::available(swap_total, observed_at_ms),
            swap_used_bytes: ScalarObservation::available(swap_used, observed_at_ms),
            used_rate_mib_per_sec: used_rate,
        }
    }
}

/// Windows container rollup provider: one row per registered WSL
/// distribution. cgroup-v2 — the Linux rollup's data source — does not exist
/// on Windows, so the host-side view is the LXss registry inventory plus the
/// `wsl.exe` fixed-program channel (`provider::system::wsl`): running
/// distributions are sampled for member pids, thread-leader CPU% and
/// member-RSS aggregates, while stopped distributions keep typed-unavailable
/// metrics — sampling them would cold-boot the utility VM. The rollup rides
/// the snapshot lane so the page renders registry rows even when every
/// metric is a typed gap; returning `Err` would route the failure into
/// `batch.failures` and hide the honest "registered but stopped" rows.
pub(super) struct WinContainerRollupProvider {
    wsl: wsl::WslRollupCollector,
}

impl WinContainerRollupProvider {
    pub fn new() -> Self {
        Self {
            wsl: wsl::WslRollupCollector::default(),
        }
    }
}

impl Default for WinContainerRollupProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ContainerRollupProvider for WinContainerRollupProvider {
    fn refresh(&mut self, now_ms: u64) -> Result<ContainerRollup, ProviderFailure> {
        let outcome = self.wsl.rollup(now_ms);
        let status = if outcome.complete {
            taskmanager_core::DeviceStatus::Healthy
        } else {
            taskmanager_core::DeviceStatus::Stale
        };
        let state = DeviceState::default().transition(status, now_ms);
        Ok(ContainerRollup {
            state,
            containers: outcome.containers,
        })
    }
}

/// Six independently scheduled Windows observation providers.
pub struct WinSystemObservationProviders {
    host: HostRegistration,
    cpu: CpuRegistration,
    memory: MemoryRegistration,
    storage: StorageRegistration,
    network: NetworkRegistration,
    gpu: GpuRegistration,
    containers: ContainerRegistration,
}

impl WinSystemObservationProviders {
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

/// Per-engine GPU utilization rows (capability `telemetry.gpu.engines`) from
/// the unprivileged PDH `\GPU Engine(*)` counters — no helper crossing on
/// Windows. Each read is one request/response crossing: DXGI resolves the
/// requested `device_id` to exactly one adapter LUID, then only that LUID's
/// aggregated engine breakdown becomes rows. A device id that is not one of
/// this provider's DXGI identities is a typed failure — never a sibling
/// adapter's rows and never a fabricated empty success.
pub struct WinGpuEngineRowsProvider;

impl WinGpuEngineRowsProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WinGpuEngineRowsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuEngineRowsProvider for WinGpuEngineRowsProvider {
    fn read_engine_rows(
        &mut self,
        device_id: &DeviceId,
    ) -> Result<GpuEngineRowsSnapshot, ProviderFailure> {
        let inventory = taskmanager_windows_api::enumerate_gpu_adapters()
            .map_err(|error| ProviderFailure::from_kind(gpu::windows_gpu_failure_kind(error)))?;
        let Some(adapter) = inventory.adapters.iter().find(|adapter| {
            gpu::dxgi_adapter_identity(adapter.luid, adapter.is_npu) == device_id.as_str()
        }) else {
            return Err(ProviderFailure::Unsupported);
        };
        let samples = taskmanager_windows_api::query_gpu_engine_utilization()
            .map_err(|error| ProviderFailure::from_kind(gpu::windows_gpu_failure_kind(error)))?;
        // A PDH answer without this LUID is the honest "no active engine
        // rows right now" success; only the query itself can fail.
        let engines = samples
            .iter()
            .find(|sample| sample.luid == adapter.luid)
            .map(engine_rows_from_pdh_sample)
            .unwrap_or_default();
        Ok(GpuEngineRowsSnapshot::success(device_id.clone(), engines))
    }
}

/// Map one PDH per-adapter engine breakdown into typed engine rows. The
/// boundary already sums sibling engine instances per type and clamps to
/// 0–100; unmapped display labels stay `Unknown` rather than receiving a
/// guessed semantic.
fn engine_rows_from_pdh_sample(
    sample: &taskmanager_windows_api::WindowsGpuEngineSample,
) -> Vec<GpuEngineMetric> {
    sample
        .engines
        .iter()
        .filter(|engine| !engine.engine_name.is_empty())
        .map(|engine| GpuEngineMetric {
            name: engine.engine_name.clone(),
            kind: GpuEngineKind::from_display_name(&engine.engine_name),
            utilization_pct: engine.utilization_pct,
        })
        .collect()
}

/// Windows NPU accelerator inventory provider (capability `accelerator.npu`):
/// SetupAPI enumeration of the MCDM compute-accelerator device-setup class
/// through the audited native boundary. Discovery is real — identity, brand,
/// and driver description per device — while utilization and memory stay the
/// documented typed-unavailable facts (Task Manager's NPU counter set is not
/// public, ADR-018), mirroring the Linux `/sys/class/accel` provider.
pub struct WinNpuInventoryProvider;

impl WinNpuInventoryProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WinNpuInventoryProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl NpuInventoryProvider for WinNpuInventoryProvider {
    fn read_inventory(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<NpuInventorySnapshot, ProviderFailure> {
        let accelerators = taskmanager_windows_api::enumerate_compute_accelerators()
            .map_err(|error| ProviderFailure::from_kind(gpu::windows_gpu_failure_kind(error)))?;
        // An empty accelerator list with no failure is the honest "no NPU on
        // this host" success; a dormant (non-Windows) boundary surfaces as
        // the typed `Unsupported` failure above.
        Ok(NpuInventorySnapshot::discovered(
            accelerators
                .into_iter()
                .map(npu_device_from_setupapi)
                .collect(),
            observed_at_ms,
        ))
    }
}

/// Map one SetupAPI compute accelerator onto the core NPU contract. The
/// device id embeds the sanitized instance path — stable across boots and
/// unique per device, mirroring the Linux sysfs-path identity — and the
/// utilization/engine/memory facts stay typed-unavailable per the core
/// contract until a stable public interface exists.
fn npu_device_from_setupapi(
    accelerator: taskmanager_windows_api::WindowsComputeAccelerator,
) -> NpuDevice {
    NpuDevice {
        device_id: DeviceId::new(format!(
            "windows:npu:setupapi:{}",
            sanitize_setupapi_identity(&accelerator.instance_path)
        )),
        brand: accelerator.friendly_name,
        driver: accelerator.driver_desc,
        utilization_pct: ScalarObservation::unavailable(FailureKind::Unsupported),
        engines: Vec::new(),
        memory: NpuMemoryReport {
            dedicated_total_bytes: ScalarObservation::unavailable(FailureKind::Unsupported),
            shared_total_bytes: ScalarObservation::unavailable(FailureKind::Unsupported),
        },
        ..NpuDevice::default()
    }
}

/// Instance-path identity sanitizer for device ids: lowercase ASCII
/// alphanumerics, `-`, `_`, and `.` pass through; every other character folds
/// to `-`. Windows device instance paths (`ACPI\INTC1070\1`,
/// `PCI\VEN_8086&DEV_1170&...\3&11583659&0&A1`) are case-insensitive, so
/// lowercasing merges only identical devices while the per-character fold
/// keeps sibling devices distinct.
fn sanitize_setupapi_identity(instance_path: &str) -> String {
    instance_path
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

/// Hardware operations outside domain observation ownership.
pub struct WinSystemAuxiliaryProviders {
    hardware_inventory: HardwareInventoryRegistration,
    gpu_engine_rows: GpuEngineRowsRegistration,
    npu_inventory: NpuInventoryRegistration,
}

impl WinSystemAuxiliaryProviders {
    #[must_use]
    pub fn new<P, E, N>(
        hardware_inventory: ProviderRegistration<HardwareInventoryRequest, P>,
        gpu_engine_rows: ProviderRegistration<GpuEngineRowsRequest, E>,
        npu_inventory: ProviderRegistration<NpuInventoryRequest, N>,
    ) -> Self
    where
        P: HardwareInventoryProvider,
        E: GpuEngineRowsProvider,
        N: NpuInventoryProvider,
    {
        Self {
            hardware_inventory: hardware_inventory
                .map_provider(|provider| Box::new(provider) as Box<dyn HardwareInventoryProvider>),
            gpu_engine_rows: gpu_engine_rows
                .map_provider(|provider| Box::new(provider) as Box<dyn GpuEngineRowsProvider>),
            npu_inventory: npu_inventory
                .map_provider(|provider| Box::new(provider) as Box<dyn NpuInventoryProvider>),
        }
    }

    pub(crate) fn into_runtime(self) -> SystemAuxiliaryExecutors {
        let Self {
            hardware_inventory,
            gpu_engine_rows,
            npu_inventory,
        } = self;
        let mut hardware_inventory = hardware_inventory.into_provider();
        let mut gpu_engine_rows = gpu_engine_rows.into_provider();
        let mut npu_inventory = npu_inventory.into_provider();
        SystemAuxiliaryExecutors::new(move || hardware_inventory.refresh())
            .with_gpu_engine_rows(move |request| {
                gpu_engine_rows.read_engine_rows(&request.device_id)
            })
            .with_npu_inventory(move |observed_at_ms| npu_inventory.read_inventory(observed_at_ms))
    }
}

/// Windows system provider composition grouped by scheduling responsibility.
pub struct WinSystemProviders {
    observations: WinSystemObservationProviders,
    auxiliary: WinSystemAuxiliaryProviders,
}

impl WinSystemProviders {
    #[must_use]
    pub const fn new(
        observations: WinSystemObservationProviders,
        auxiliary: WinSystemAuxiliaryProviders,
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
    }

    pub(crate) fn into_runtime(self) -> SystemExecutors {
        SystemExecutors::new(
            self.observations.into_runtime(),
            self.auxiliary.into_runtime(),
        )
    }
}

#[cfg(test)]
#[path = "../../tests/headless/platform_windows_provider_system.rs"]
mod tests;
