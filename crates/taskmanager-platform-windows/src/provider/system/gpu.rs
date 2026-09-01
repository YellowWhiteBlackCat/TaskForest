//! GPU telemetry for the Windows system provider.
//!
//! Prefers the vendor-specific `nvml-wrapper` crate (dynamically loaded
//! `nvml.dll`) for rich NVIDIA telemetry (temperature, clocks, power, fans,
//! utilization). Falls back to native DXGI adapter querying
//! (`taskmanager_windows_api::enumerate_gpu_adapters`) for AMD, Intel, and
//! integrated/discrete GPUs to report authentic VRAM, shared memory, and driver
//! device facts without fabricating unsupported metrics.

use taskmanager_core::{
    DeviceGeneration, GpuMetricField, GpuMetricProvenance, GpuMetrics, GpuScalarObservations,
    GpuTelemetryObservation, GpuThrottleReason,
};

use super::*;

const MAX_NVML_GPU_DEVICES: u32 = 16;

#[derive(Debug)]
struct NvmlGpuSample {
    pci_address: taskmanager_windows_api::WindowsPciAddress,
    metrics: GpuMetrics,
}

#[derive(Debug)]
struct DxgiGpuSample {
    pci_address: Option<taskmanager_windows_api::WindowsPciAddress>,
    metrics: GpuMetrics,
}

/// GPU telemetry provider supporting NVML with native DXGI fallback.
pub struct WinGpuTelemetryProvider {
    nvml: Option<nvml_wrapper::Nvml>,
    lifecycles: taskmanager_core::DeviceLifecycleRegistry,
}

impl WinGpuTelemetryProvider {
    pub fn new() -> Self {
        let nvml = nvml_wrapper::Nvml::init().ok();
        Self {
            nvml,
            lifecycles: taskmanager_core::DeviceLifecycleRegistry::new(
                taskmanager_core::DEFAULT_DEVICE_ABSENCE_RETENTION_MS,
            ),
        }
    }

    fn refresh_nvml(
        nvml: &nvml_wrapper::Nvml,
        observed_at_ms: u64,
    ) -> Result<(Vec<NvmlGpuSample>, Option<FailureKind>), ProviderFailure> {
        let Ok(count) = nvml.device_count() else {
            return Err(ProviderFailure::TemporarilyUnavailable);
        };
        let driver_version = nvml.sys_driver_version().ok();
        let mut gpus = Vec::new();
        let (bounded_count, mut identity_failure) = nvml_enumeration_bound(count);
        for index in 0..bounded_count {
            let Ok(device) = nvml.device_by_index(index) else {
                continue;
            };
            let Ok(pci) = device.pci_info() else {
                identity_failure = Some(FailureKind::Unsupported);
                continue;
            };
            let Some(function) = pci_function(&pci.bus_id) else {
                identity_failure = Some(FailureKind::Unsupported);
                continue;
            };
            // D3DKMT adapter addresses are expressed in the host PCI segment.
            // A non-zero NVML domain cannot be joined exactly to that typed
            // identity, so do not enrich an arbitrary sibling.
            if pci.domain != 0 {
                identity_failure = Some(FailureKind::Unsupported);
                continue;
            }
            let pci_address = taskmanager_windows_api::WindowsPciAddress {
                bus: pci.bus,
                device: pci.device,
                function,
            };
            let mut row =
                GpuMetrics::new("", device.name().unwrap_or_else(|_| "NVIDIA GPU".into()));
            row.device_generation = DeviceGeneration::INITIAL;
            row.device_state = DeviceState::healthy(observed_at_ms);
            row.driver_version = driver_version.clone();
            row.pci_vendor_id = Some((pci.pci_device_id & 0xffff) as u16);
            row.pci_device_id = Some((pci.pci_device_id >> 16) as u16);
            row.pci_slot = Some(pci.bus_id.clone());
            let mut observations = GpuScalarObservations::default();
            let mut provenance = Vec::new();
            if row.driver_version.is_some() {
                provenance.push(GpuMetricProvenance {
                    field: GpuMetricField::DriverVersion,
                    provider: GPU_TELEMETRY_PROVIDER,
                });
            }
            if let Ok(utilization) = device.utilization_rates() {
                observations.utilization_pct =
                    ScalarObservation::available(utilization.gpu as f32, observed_at_ms);
                provenance.push(GpuMetricProvenance {
                    field: GpuMetricField::Utilization,
                    provider: GPU_TELEMETRY_PROVIDER,
                });
            }
            if let Ok(memory) = device.memory_info() {
                observations.memory_used_bytes =
                    ScalarObservation::available(memory.used, observed_at_ms);
                observations.memory_total_bytes =
                    ScalarObservation::available(memory.total, observed_at_ms);
                observations.dedicated_vram_used_bytes =
                    ScalarObservation::available(memory.used, observed_at_ms);
                observations.dedicated_vram_total_bytes =
                    ScalarObservation::available(memory.total, observed_at_ms);
                provenance.push(GpuMetricProvenance {
                    field: GpuMetricField::Memory,
                    provider: GPU_TELEMETRY_PROVIDER,
                });
            }
            if let Ok(temp) =
                device.temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
            {
                observations.temperature_c =
                    ScalarObservation::available(temp as f32, observed_at_ms);
                provenance.push(GpuMetricProvenance {
                    field: GpuMetricField::Temperature,
                    provider: GPU_TELEMETRY_PROVIDER,
                });
            }
            if let Ok(power) = device.power_usage() {
                let watts = power as f32 / 1000.0;
                observations.power_w = ScalarObservation::available(watts, observed_at_ms);
                provenance.push(GpuMetricProvenance {
                    field: GpuMetricField::Power,
                    provider: GPU_TELEMETRY_PROVIDER,
                });
            }
            if let Ok(freq) = device.clock_info(nvml_wrapper::enum_wrappers::device::Clock::SM) {
                observations.frequency_mhz =
                    ScalarObservation::available(freq as u64, observed_at_ms);
                provenance.push(GpuMetricProvenance {
                    field: GpuMetricField::Frequency,
                    provider: GPU_TELEMETRY_PROVIDER,
                });
            }
            if let Ok(fan) = device.fan_speed(0) {
                let pct = fan as f32;
                observations.fan_speed_pct = ScalarObservation::available(pct, observed_at_ms);
                provenance.push(GpuMetricProvenance {
                    field: GpuMetricField::Fan,
                    provider: GPU_TELEMETRY_PROVIDER,
                });
            }
            let throttle_observation = nvml_throttle_observation(&device, observed_at_ms);
            if throttle_observation.current_value().is_some() {
                provenance.push(GpuMetricProvenance {
                    field: GpuMetricField::Throttle,
                    provider: GPU_TELEMETRY_PROVIDER,
                });
            }
            row.apply_scalar_observations(observations);
            row.apply_throttle_observation(throttle_observation);
            row.provenance = provenance;
            gpus.push(NvmlGpuSample {
                pci_address,
                metrics: row,
            });
        }
        Ok((gpus, identity_failure))
    }

    fn refresh_dxgi(
        observed_at_ms: u64,
    ) -> Result<(Vec<DxgiGpuSample>, Option<FailureKind>), FailureKind> {
        let inventory =
            taskmanager_windows_api::enumerate_gpu_adapters().map_err(windows_gpu_failure_kind)?;
        let adapters = inventory.adapters;
        // WDDM 2.0+ exposes per-adapter dedicated/shared usage through PDH
        // `\GPU Adapter Memory(*)` — the same source Task Manager reads. DXGI
        // `QueryVideoMemoryInfo(NON_LOCAL)` is unreliable on Intel/AMD drivers
        // (fails or reports 0), so PDH values fill the shared-usage gap while
        // DXGI local usage remains the dedicated-usage preference.
        let (adapter_memory, memory_failure) =
            match taskmanager_windows_api::query_gpu_adapter_memory() {
                Ok(samples) => (samples, None),
                Err(error) => (Vec::new(), Some(windows_gpu_failure_kind(error))),
            };
        // Prefer hardware adapters if present.
        let has_hardware = adapters.iter().any(|a| !a.is_software);
        let eligible = adapters
            .into_iter()
            .enumerate()
            .filter(|(_, a)| !has_hardware || !a.is_software);

        let (engine_samples, engine_failure) =
            match taskmanager_windows_api::query_gpu_engine_utilization() {
                Ok(samples) => (samples, None),
                Err(error) => (Vec::new(), Some(windows_gpu_failure_kind(error))),
            };
        let mut gpus = Vec::new();
        for (_index, adapter) in eligible {
            let brand = adapter.name.clone();
            let memory_sample = find_adapter_memory_sample(&brand, adapter.luid, &adapter_memory);
            let mut row =
                GpuMetrics::new(dxgi_adapter_identity(adapter.luid, adapter.is_npu), brand);
            row.device_generation = DeviceGeneration::INITIAL;
            row.device_state = DeviceState::healthy(observed_at_ms);
            row.pci_vendor_id = u16::try_from(adapter.vendor_id).ok();
            row.pci_device_id = u16::try_from(adapter.device_id).ok();
            let mut observations = GpuScalarObservations::default();
            let mut provenance = Vec::new();

            // GPU Engine utilization from Windows PDH counters.
            let matching_sample = engine_samples.iter().find(|s| s.luid == adapter.luid);
            let util_opt = matching_sample.map(|s| s.utilization_pct);

            if let Some(sample) = matching_sample {
                row.engines = sample
                    .engines
                    .iter()
                    .filter(|e| !e.engine_name.is_empty())
                    .map(|e| taskmanager_core::GpuEngine {
                        name: e.engine_name.clone(),
                        kind: taskmanager_core::GpuEngineKind::from_display_name(&e.engine_name),
                        usage_pct: e.utilization_pct,
                    })
                    .collect();
                if !row.engines.is_empty() {
                    provenance.push(GpuMetricProvenance {
                        field: GpuMetricField::Engines,
                        provider: GPU_TELEMETRY_PROVIDER,
                    });
                }
            }

            if let Some(version) = adapter.driver_version {
                row.driver_version = Some(version);
                provenance.push(GpuMetricProvenance {
                    field: GpuMetricField::DriverVersion,
                    provider: GPU_TELEMETRY_PROVIDER,
                });
            }

            if let Some(util) = util_opt {
                observations.utilization_pct = ScalarObservation::available(util, observed_at_ms);
                provenance.push(GpuMetricProvenance {
                    field: GpuMetricField::Utilization,
                    provider: GPU_TELEMETRY_PROVIDER,
                });
            }

            let is_integrated = adapter.dedicated_video_memory <= 512 * 1024 * 1024
                && adapter.shared_system_memory > 0;
            // Match the DXGI adapter to the PDH adapter-memory instance by
            // name (exact case-insensitive, then tolerant normalization).
            let dedicated_used = adapter
                .dedicated_used_bytes
                .or(memory_sample.and_then(|s| s.dedicated_usage_bytes));
            let shared_used = memory_sample
                .and_then(|s| s.shared_usage_bytes)
                .or(adapter.shared_used_bytes);

            if is_integrated {
                let total_vram = adapter.dedicated_video_memory + adapter.shared_system_memory;
                observations.memory_total_bytes =
                    ScalarObservation::available(total_vram, observed_at_ms);
                // DXGI does not expose shared-memory usage on every driver;
                // a fabricated 0 would read as "0 used" forever. Only report
                // used totals when both halves are actually observed.
                if let (Some(dedicated), Some(shared)) = (dedicated_used, shared_used) {
                    let used_vram = dedicated.saturating_add(shared);
                    observations.memory_used_bytes =
                        ScalarObservation::available(used_vram, observed_at_ms);
                }

                observations.dedicated_vram_total_bytes =
                    ScalarObservation::available(adapter.dedicated_video_memory, observed_at_ms);
                if let Some(used) = dedicated_used {
                    let used = used.min(adapter.dedicated_video_memory);
                    observations.dedicated_vram_used_bytes =
                        ScalarObservation::available(used, observed_at_ms);
                }

                observations.shared_vram_total_bytes =
                    ScalarObservation::available(adapter.shared_system_memory, observed_at_ms);
                if let Some(used) = shared_used {
                    observations.shared_vram_used_bytes =
                        ScalarObservation::available(used, observed_at_ms);
                }

                provenance.push(GpuMetricProvenance {
                    field: GpuMetricField::Memory,
                    provider: GPU_TELEMETRY_PROVIDER,
                });
                provenance.push(GpuMetricProvenance {
                    field: GpuMetricField::DedicatedVram,
                    provider: GPU_TELEMETRY_PROVIDER,
                });
                provenance.push(GpuMetricProvenance {
                    field: GpuMetricField::SharedVram,
                    provider: GPU_TELEMETRY_PROVIDER,
                });
            } else {
                // Discrete GPU with dedicated VRAM
                if adapter.dedicated_video_memory > 0 {
                    observations.dedicated_vram_total_bytes = ScalarObservation::available(
                        adapter.dedicated_video_memory,
                        observed_at_ms,
                    );
                    observations.memory_total_bytes = ScalarObservation::available(
                        adapter.dedicated_video_memory,
                        observed_at_ms,
                    );

                    if let Some(used) = dedicated_used {
                        observations.dedicated_vram_used_bytes =
                            ScalarObservation::available(used, observed_at_ms);
                        observations.memory_used_bytes =
                            ScalarObservation::available(used, observed_at_ms);
                    }
                    provenance.push(GpuMetricProvenance {
                        field: GpuMetricField::DedicatedVram,
                        provider: GPU_TELEMETRY_PROVIDER,
                    });
                    provenance.push(GpuMetricProvenance {
                        field: GpuMetricField::Memory,
                        provider: GPU_TELEMETRY_PROVIDER,
                    });
                }

                // Shared system memory
                if adapter.shared_system_memory > 0 {
                    observations.shared_vram_total_bytes =
                        ScalarObservation::available(adapter.shared_system_memory, observed_at_ms);
                    if let Some(used) = shared_used {
                        observations.shared_vram_used_bytes =
                            ScalarObservation::available(used, observed_at_ms);
                    }
                    provenance.push(GpuMetricProvenance {
                        field: GpuMetricField::SharedVram,
                        provider: GPU_TELEMETRY_PROVIDER,
                    });
                }
            }

            row.apply_scalar_observations(observations);
            row.apply_throttle_observation(ScalarObservation::unavailable(
                FailureKind::Unsupported,
            ));
            row.provenance = provenance;
            gpus.push(DxgiGpuSample {
                pci_address: adapter.pci_address,
                metrics: row,
            });
        }
        Ok((
            gpus,
            inventory
                .truncated
                .then_some(FailureKind::Unsupported)
                .or(memory_failure)
                .or(engine_failure),
        ))
    }
}

const fn nvml_enumeration_bound(count: u32) -> (u32, Option<FailureKind>) {
    if count > MAX_NVML_GPU_DEVICES {
        (MAX_NVML_GPU_DEVICES, Some(FailureKind::Unsupported))
    } else {
        (count, None)
    }
}

/// The one DXGI adapter identity for a Windows LUID. The periodic telemetry
/// lane and the engine-rows request lane must derive the same string, or a
/// frontend device id stops addressing the adapter its rows came from.
pub(super) fn dxgi_adapter_identity(luid: u64, is_npu: bool) -> String {
    let device_prefix = if is_npu { "npu" } else { "gpu" };
    format!("windows:{device_prefix}:dxgi:{luid:016x}")
}

fn nvml_throttle_observation(
    device: &nvml_wrapper::Device<'_>,
    observed_at_ms: u64,
) -> ScalarObservation<Vec<GpuThrottleReason>> {
    use nvml_wrapper::error::{Bits, NvmlError};

    match device.current_throttle_reasons_strict() {
        Ok(reasons) => {
            ScalarObservation::available(map_nvml_throttle_bits(reasons.bits()), observed_at_ms)
        }
        Err(NvmlError::IncorrectBits(Bits::U64(raw))) => {
            ScalarObservation::available(map_nvml_throttle_bits(raw), observed_at_ms)
        }
        Err(_) => ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
    }
}

fn map_nvml_throttle_bits(raw: u64) -> Vec<GpuThrottleReason> {
    use nvml_wrapper::bitmasks::device::ThrottleReasons;

    let reasons = ThrottleReasons::from_bits_truncate(raw);
    let mut mapped = [
        (ThrottleReasons::GPU_IDLE, GpuThrottleReason::Idle),
        (
            ThrottleReasons::APPLICATIONS_CLOCKS_SETTING,
            GpuThrottleReason::ApplicationClockLimit,
        ),
        (
            ThrottleReasons::SW_POWER_CAP,
            GpuThrottleReason::SoftwarePowerLimit,
        ),
        (
            ThrottleReasons::HW_SLOWDOWN,
            GpuThrottleReason::HardwareSlowdown,
        ),
        (ThrottleReasons::SYNC_BOOST, GpuThrottleReason::SyncBoost),
        (
            ThrottleReasons::SW_THERMAL_SLOWDOWN,
            GpuThrottleReason::SoftwareThermalLimit,
        ),
        (
            ThrottleReasons::HW_THERMAL_SLOWDOWN,
            GpuThrottleReason::HardwareThermalLimit,
        ),
        (
            ThrottleReasons::HW_POWER_BRAKE_SLOWDOWN,
            GpuThrottleReason::ExternalPowerBrake,
        ),
        (
            ThrottleReasons::DISPLAY_CLOCK_SETTING,
            GpuThrottleReason::DisplayClockLimit,
        ),
    ]
    .into_iter()
    .filter_map(|(flag, reason)| reasons.contains(flag).then_some(reason))
    .collect::<Vec<_>>();
    if raw & !ThrottleReasons::all().bits() != 0 {
        mapped.push(GpuThrottleReason::Other);
    }
    mapped
}

/// Locate the PDH adapter-memory sample for a DXGI adapter by exact LUID.
/// Friendly names are presentation data and may collide or change with a
/// driver update; using them as identity can copy one adapter's readings onto
/// a sibling.
#[cfg(windows)]
fn find_adapter_memory_sample<'a>(
    _name: &str,
    luid: u64,
    samples: &'a [taskmanager_windows_api::WindowsGpuAdapterMemorySample],
) -> Option<&'a taskmanager_windows_api::WindowsGpuAdapterMemorySample> {
    samples.iter().find(|sample| sample.luid == Some(luid))
}

#[cfg(not(windows))]
fn find_adapter_memory_sample<'a>(
    _name: &str,
    _luid: u64,
    _samples: &'a [taskmanager_windows_api::WindowsGpuAdapterMemorySample],
) -> Option<&'a taskmanager_windows_api::WindowsGpuAdapterMemorySample> {
    None
}

fn pci_function(bus_id: &str) -> Option<u32> {
    let (_, function) = bus_id.rsplit_once('.')?;
    u32::from_str_radix(function, 16).ok()
}

/// Keep DXGI as the complete, exact-LUID inventory and attach NVML facts only
/// when both providers prove the same PCI function. An unmatched or ambiguous
/// enrichment is a typed partial failure; it never becomes a second GPU row or
/// gets copied to a sibling.
fn merge_gpu_samples(
    mut dxgi: Vec<DxgiGpuSample>,
    nvml: Vec<NvmlGpuSample>,
) -> (Vec<GpuMetrics>, Option<FailureKind>) {
    let mut merge_failure = None;
    for nvml_sample in nvml {
        let matches = dxgi
            .iter()
            .enumerate()
            .filter_map(|(index, sample)| {
                (sample.pci_address == Some(nvml_sample.pci_address)).then_some(index)
            })
            .collect::<Vec<_>>();
        if let [index] = matches.as_slice() {
            enrich_dxgi_row(&mut dxgi[*index].metrics, nvml_sample.metrics);
        } else {
            merge_failure = Some(FailureKind::Unsupported);
        }
    }
    (
        dxgi.into_iter().map(|sample| sample.metrics).collect(),
        merge_failure,
    )
}

fn enrich_dxgi_row(target: &mut GpuMetrics, source: GpuMetrics) {
    let mut observations = *target.scalar_observations();
    let source_observations = source.scalar_observations();
    if source.current_utilization_pct().is_some() {
        observations.utilization_pct = source_observations.utilization_pct;
    }
    if source.current_memory_used_bytes().is_some() && source.current_memory_total_bytes().is_some()
    {
        observations.memory_used_bytes = source_observations.memory_used_bytes;
        observations.memory_total_bytes = source_observations.memory_total_bytes;
        observations.dedicated_vram_used_bytes = source_observations.dedicated_vram_used_bytes;
        observations.dedicated_vram_total_bytes = source_observations.dedicated_vram_total_bytes;
    }
    if source.current_temperature_c().is_some() {
        observations.temperature_c = source_observations.temperature_c;
    }
    if source.current_power_w().is_some() {
        observations.power_w = source_observations.power_w;
    }
    if source.current_frequency_mhz().is_some() {
        observations.frequency_mhz = source_observations.frequency_mhz;
    }
    if source.current_fan_speed_pct().is_some() {
        observations.fan_speed_pct = source_observations.fan_speed_pct;
    }
    if source.current_throttle_reasons().is_some() {
        target.apply_throttle_observation(source.throttle_observation().clone());
    }
    if source.driver_version.is_some() {
        target.driver_version = source.driver_version;
    }

    for provenance in source.provenance {
        target
            .provenance
            .retain(|existing| existing.field != provenance.field);
        target.provenance.push(provenance);
    }
    target.provenance.sort_by_key(|provenance| provenance.field);
    target.apply_scalar_observations(observations);
}

impl Default for WinGpuTelemetryProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuTelemetryProvider for WinGpuTelemetryProvider {
    fn refresh(&mut self, observed_at_ms: u64) -> Result<GpuTelemetryObservation, ProviderFailure> {
        self.lifecycles.begin_refresh();
        let (dxgi, dxgi_partial_failure) = match Self::refresh_dxgi(observed_at_ms) {
            Ok(snapshot) => snapshot,
            Err(failure) => {
                let _delta = self.lifecycles.finish_refresh(
                    taskmanager_core::DeviceRefreshOutcome::Unavailable(
                        taskmanager_core::DeviceStatus::from_failure(failure),
                    ),
                    observed_at_ms,
                );
                return Ok(GpuTelemetryObservation::unavailable(
                    failure,
                    vec![unavailable_source(GPU_TELEMETRY_PROVIDER, failure)],
                    Vec::new(),
                    Default::default(),
                ));
            }
        };
        let (nvml, nvml_failure) = if let Some(nvml) = self.nvml.as_ref() {
            match Self::refresh_nvml(nvml, observed_at_ms) {
                Ok(snapshot) => snapshot,
                Err(failure) => (Vec::new(), Some(failure.kind())),
            }
        } else {
            (Vec::new(), None)
        };
        let (mut gpus, merge_failure) = merge_gpu_samples(dxgi, nvml);
        let partial_failure = merge_failure.or(nvml_failure).or(dxgi_partial_failure);

        for gpu in &mut gpus {
            let device_state = taskmanager_core::DeviceState::healthy(observed_at_ms);
            let lifecycle =
                self.lifecycles
                    .observe(gpu.device_id.as_str(), device_state, observed_at_ms);
            gpu.device_generation = lifecycle.generation;
            gpu.device_state = device_state;
        }

        let _delta = self.lifecycles.finish_refresh(
            taskmanager_core::DeviceRefreshOutcome::Complete,
            observed_at_ms,
        );
        let lifecycles = self
            .lifecycles
            .iter()
            .map(|(id, l)| (taskmanager_core::DeviceId::new(id), *l))
            .collect::<std::collections::BTreeMap<_, _>>();

        if gpus.is_empty() {
            if let Some(failure) = partial_failure {
                return Ok(GpuTelemetryObservation::unavailable(
                    failure,
                    vec![unavailable_source(GPU_TELEMETRY_PROVIDER, failure)],
                    Vec::new(),
                    lifecycles,
                ));
            }
            return Ok(GpuTelemetryObservation::current(
                Vec::new(),
                observed_at_ms,
                vec![SourceStatus {
                    provider: GPU_TELEMETRY_PROVIDER,
                    outcome: SourceOutcome::Empty,
                    item_count: 0,
                }],
                Vec::new(),
                lifecycles,
            ));
        }

        if let Some(failure) = partial_failure {
            let item_count = gpus.len();
            Ok(GpuTelemetryObservation::partial(
                gpus,
                observed_at_ms,
                failure,
                vec![SourceStatus {
                    provider: GPU_TELEMETRY_PROVIDER,
                    outcome: SourceOutcome::Partial(failure),
                    item_count,
                }],
                Vec::new(),
                lifecycles,
            ))
        } else {
            let sources = vec![available_source(GPU_TELEMETRY_PROVIDER, gpus.len())];
            Ok(GpuTelemetryObservation::current(
                gpus,
                observed_at_ms,
                sources,
                Vec::new(),
                lifecycles,
            ))
        }
    }
}

pub(super) fn windows_gpu_failure_kind(
    error: taskmanager_windows_api::WindowsApiError,
) -> FailureKind {
    match error {
        taskmanager_windows_api::WindowsApiError::Unsupported => FailureKind::Unsupported,
        taskmanager_windows_api::WindowsApiError::PermissionDenied => FailureKind::PermissionDenied,
        taskmanager_windows_api::WindowsApiError::IdentityChanged
        | taskmanager_windows_api::WindowsApiError::InvalidInput => FailureKind::IdentityChanged,
        taskmanager_windows_api::WindowsApiError::ResourceLimit
        | taskmanager_windows_api::WindowsApiError::InvalidText
        | taskmanager_windows_api::WindowsApiError::QueryFailed => {
            FailureKind::TemporarilyUnavailable
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/platform_windows_provider_system_gpu.rs"]
mod tests;
