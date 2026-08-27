//! CPU telemetry for the Windows system domain (split out of `system.rs`;
//! pure mechanical extraction). `sysinfo` supplies per-core usage and live
//! frequency through its safe Windows backend. Static base/max frequency facts
//! come from the safe `raw-cpuid` reader. Temperature and power remain typed
//! unavailable until a bounded, trustworthy Windows source is selected; no
//! command interpreter is used.

use taskmanager_core::{
    CpuMetrics, CpuTelemetryObservation, CpuTemperatureSource, FailureKind, ScalarObservation,
    ScalarObservationGroup, ScalarObservationSlot,
};
use taskmanager_platform_contract::ProviderFailure;
use taskmanager_platform_provider::CpuTelemetryProvider;

use super::{CPU_TELEMETRY_PROVIDER, available_source, unavailable_source};

/// CPU telemetry from `sysinfo` (per-core usage and live frequency via safe
/// Windows wrappers). Temperature and power stay typed unavailable until a safe
/// native provider is available.
pub struct WinCpuTelemetryProvider {
    system: sysinfo::System,
    advertised_max_mhz: Option<u64>,
    topology: Option<taskmanager_windows_api::WindowsProcessorTopology>,
}

impl WinCpuTelemetryProvider {
    pub fn new() -> Self {
        let (_, advertised_max_mhz) = super::cpu_info::advertised_frequencies_mhz();
        let topology = taskmanager_windows_api::processor_topology().ok();
        Self {
            system: sysinfo::System::new(),
            advertised_max_mhz,
            topology,
        }
    }
}

impl CpuTelemetryProvider for WinCpuTelemetryProvider {
    fn refresh(&mut self, observed_at_ms: u64) -> Result<CpuTelemetryObservation, ProviderFailure> {
        self.system.refresh_cpu_all();
        let cpus = self.system.cpus();
        if cpus.is_empty() {
            return Err(ProviderFailure::TemporarilyUnavailable);
        }
        let core_usages: Vec<f32> = cpus.iter().map(sysinfo::Cpu::cpu_usage).collect();
        let brand = cpus
            .first()
            .map(|cpu| cpu.brand().trim().to_string())
            .filter(|brand| !brand.is_empty());
        let core_count = core_usages.len();

        let pdh_sample = taskmanager_windows_api::query_cpu_dynamic_frequencies().ok();
        let per_core_freq: Vec<Option<u64>> = if let Some(sample) = pdh_sample {
            if !sample.per_core_frequency_mhz.is_empty() {
                sample
                    .per_core_frequency_mhz
                    .into_iter()
                    .map(|f_opt| {
                        f_opt.map(|f| {
                            if let Some(max_mhz) = self.advertised_max_mhz {
                                f.min(max_mhz)
                            } else {
                                f.min(6000)
                            }
                        })
                    })
                    .collect()
            } else {
                cpus.iter()
                    .map(|cpu| {
                        let frequency = cpu.frequency();
                        (frequency > 0).then_some(frequency)
                    })
                    .collect()
            }
        } else {
            cpus.iter()
                .map(|cpu| {
                    let frequency = cpu.frequency();
                    (frequency > 0).then_some(frequency)
                })
                .collect()
        };

        let physical_cores = self
            .topology
            .as_ref()
            .and_then(|facts| {
                facts
                    .core_breakdown
                    .map(|b| b.total_physical_cores as usize)
            })
            .or_else(sysinfo::System::physical_core_count);

        let active_policy = taskmanager_windows_api::active_power_scheme_name().ok();
        // The effective power overlay (performance power slider) is the
        // Windows counterpart of cpufreq's energy_performance_preference; an
        // unmapped or unqueryable overlay stays an honest None, never a guess.
        let energy_preference = taskmanager_windows_api::effective_power_overlay_name()
            .ok()
            .flatten();
        let performance_policy = taskmanager_core::CpuPerformancePolicy {
            frequency_implementation: Some("Windows Power Manager (powrprof)".into()),
            active_policy,
            energy_preference,
        };

        let mut observations = CpuScalarObservationFactory::build(
            core_usages.clone(),
            &per_core_freq,
            self.advertised_max_mhz,
            observed_at_ms,
        );
        let thermal_zones = taskmanager_windows_api::query_acpi_thermal_zones().ok();
        let mut thermal_temp = thermal_zones.as_ref().and_then(|zones| {
            let temps: Vec<f32> = zones.iter().map(|z| z.temperature_c).collect();
            if temps.is_empty() {
                None
            } else {
                Some(temps.iter().sum::<f32>() / temps.len() as f32)
            }
        });
        if thermal_temp.is_none() {
            let components = sysinfo::Components::new_with_refreshed_list();
            let temps: Vec<f32> = components
                .iter()
                .filter_map(sysinfo::Component::temperature)
                .filter(|&t| t > 0.0 && t < 120.0)
                .collect();
            if !temps.is_empty() {
                thermal_temp = Some(temps.iter().sum::<f32>() / temps.len() as f32);
            }
        }
        if let Some(temp) = thermal_temp {
            observations.temperature_c = ScalarObservation::available(temp, observed_at_ms);
            observations.per_core_temperature_group =
                ScalarObservationGroup::available(vec![temp; core_count], observed_at_ms);
        }
        let mut metrics = CpuMetrics::from_observations(observations);
        metrics.brand = brand;
        metrics.physical_cores = physical_cores;
        metrics.logical_cores = Some(core_count);
        // The temperature above came from ACPI thermal zones (averaged), or
        // from sysinfo's component sensors when the zones are absent — the
        // typed thermal-zone fallback tier, never a dedicated CPU sensor
        // chip, so the UI qualifier stays honest on Windows too.
        if thermal_temp.is_some() {
            metrics.temperature_source = CpuTemperatureSource::ThermalZone;
        }
        metrics.l1_cache_kb = self.topology.as_ref().and_then(|facts| facts.l1_cache_kb);
        metrics.l2_cache_kb = self.topology.as_ref().and_then(|facts| facts.l2_cache_kb);
        metrics.l3_cache_kb = self.topology.as_ref().and_then(|facts| facts.l3_cache_kb);
        metrics.performance_policy = performance_policy;

        let sources = vec![
            available_source(CPU_TELEMETRY_PROVIDER, core_count),
            unavailable_source(CPU_TELEMETRY_PROVIDER, FailureKind::Unsupported),
        ];
        Ok(CpuTelemetryObservation::current(
            metrics,
            observed_at_ms,
            sources,
        ))
    }
}

/// Typed scalar observations for CPU: usage rows and sysinfo's live frequency
/// are available; temperature and power stay typed unavailable until a safe
/// native source is adopted.
struct CpuScalarObservationFactory;

impl CpuScalarObservationFactory {
    fn build(
        core_usages: Vec<f32>,
        per_core_freq: &[Option<u64>],
        advertised_max_mhz: Option<u64>,
        observed_at_ms: u64,
    ) -> taskmanager_core::CpuScalarObservations {
        let global = core_usages.iter().sum::<f32>() / core_usages.len().max(1) as f32;
        let live_freqs: Vec<u64> = per_core_freq.iter().copied().flatten().collect();
        let max_freq = live_freqs.iter().copied().max();
        let live_count = live_freqs.len();
        let total_count = core_usages.len();
        let per_core_frequency_mhz: Vec<ScalarObservation<u64>> = core_usages
            .iter()
            .enumerate()
            .map(|(idx, _)| match per_core_freq.get(idx).copied().flatten() {
                Some(mhz) => ScalarObservation::available(mhz, observed_at_ms),
                None => ScalarObservation::unavailable(FailureKind::Unsupported),
            })
            .collect();
        // Wire validator requires an `Available` group to contain ONLY
        // Available items. When some cores lack a live counter instance we
        // emit a `Partial` group (mixed Some/None slots) so a snapshot still
        // round-trips; only a fully-covered refresh is `Available`.
        let per_core_frequency_group = if live_count == 0 {
            ScalarObservationGroup::unavailable(FailureKind::Unsupported)
        } else if live_count == total_count {
            ScalarObservationGroup::available(
                per_core_frequency_mhz
                    .iter()
                    .filter_map(|observation| observation.current_value().copied())
                    .collect(),
                observed_at_ms,
            )
        } else {
            ScalarObservationGroup::partial(
                per_core_frequency_mhz
                    .iter()
                    .map(|observation| {
                        observation.current_value().copied().map_or(
                            ScalarObservationSlot::Unavailable(FailureKind::Unsupported),
                            ScalarObservationSlot::Current,
                        )
                    })
                    .collect(),
                observed_at_ms,
                FailureKind::Unsupported,
            )
        };
        taskmanager_core::CpuScalarObservations {
            global_usage_pct: ScalarObservation::available(global, observed_at_ms),
            core_usage_group: ScalarObservationGroup::available(
                core_usages.clone(),
                observed_at_ms,
            ),
            frequency_mhz: match max_freq {
                Some(mhz) => ScalarObservation::available(mhz, observed_at_ms),
                None => ScalarObservation::unavailable(FailureKind::Unsupported),
            },
            max_frequency_mhz: advertised_max_mhz.map_or_else(
                || ScalarObservation::unavailable(FailureKind::Unsupported),
                |mhz| ScalarObservation::available(mhz, observed_at_ms),
            ),
            per_core_frequency_group,
            temperature_c: ScalarObservation::unavailable(FailureKind::Unsupported),
            per_core_temperature_group: ScalarObservationGroup::unavailable(
                FailureKind::Unsupported,
            ),
            power_w: ScalarObservation::unavailable(FailureKind::Unsupported),
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/platform_windows_provider_system_cpu_freq.rs"]
mod tests;
