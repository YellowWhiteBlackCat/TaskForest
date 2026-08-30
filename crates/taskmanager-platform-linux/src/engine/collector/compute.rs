//! Native CPU and memory telemetry collectors.
//!
//! These collectors own counter-to-rate conversion details that are specific
//! to compute telemetry. The snapshot assembler only schedules them through
//! the corresponding capability traits in `providers`.

use super::*;
use sysinfo::System;
use taskmanager_core::core::metrics::{
    CpuFrequencySource, CpuScalarObservations, ScalarAvailability, ScalarObservationGroup,
    ScalarObservationSlot, cpu_usage_pct_observation,
};
use taskmanager_platform_contract::CompositeSourceSnapshot;

mod cpu_sources;
mod memory_sources;

// Pure zram mm_stat parser seam for the fuzz workspace; reachable only
// through the gated re-export chain up to the crate root (same pattern as
// the procfs parser exports).
#[cfg(feature = "test-support")]
pub use memory_sources::parse_zram_mm_stat;

use cpu_sources::{
    bogomips_to_frequency_value, observe_bogomips, observe_cpufreq, observe_rapl,
    observe_temperatures,
};
use memory_sources::{observe_compressed_swap, observe_dmi_memory, observe_meminfo};

/// Used by the memory MB/s rate. RAPL power keeps its wrap-aware arithmetic in
/// `collect_cpu`; disk and network rates have different counter semantics.
fn delta_rate(
    prev: Option<(u64, Instant)>,
    curr: u64,
    now: Instant,
) -> (Option<f64>, Option<(u64, Instant)>) {
    let new_prev = Some((curr, now));
    let Some((prev_val, prev_t)) = prev else {
        return (None, new_prev);
    };
    let dt = now.duration_since(prev_t).as_secs_f32() as f64;
    let rate = (dt > 0.0).then(|| (curr as f64 - prev_val as f64) / dt);
    (rate, new_prev)
}

fn observed_sysinfo_frequency_mhz(frequency_mhz: u64) -> Option<u64> {
    // sysinfo documents zero as unavailable on platforms/providers that cannot
    // report frequency. Keep a real cpufreq-file zero distinct, but never wrap
    // this compatibility sentinel in `Some`.
    (frequency_mhz > 0).then_some(frequency_mhz)
}

/// Build the per-core usage group from raw sysinfo percentages.
///
/// Idle-phantom-spike guard (Mission Center v1.2.0 !484 class): sysinfo can
/// surface non-finite, negative, or impossible `> 100%` percentages on some
/// hosts and ticks while the system is actually idle. Every raw percentage
/// passes the core-owned [`cpu_usage_pct_observation`] gate so a phantom
/// slot becomes a typed per-slot gap (degrading the group to an honest
/// `Partial`) instead of a full-scale spike that poisons graphs and rolling
/// histories. Healthy slots stay current; a fully healthy refresh stays
/// `Available` with measured zeros preserved.
fn usage_group_from_percentages(usages: &[f32], now_ms: u64) -> ScalarObservationGroup<f32> {
    let observations = usages
        .iter()
        .map(|&usage| cpu_usage_pct_observation(usage, now_ms))
        .collect::<Vec<_>>();
    if observations
        .iter()
        .all(|observation| observation.availability() == ScalarAvailability::Available)
    {
        ScalarObservationGroup::available(
            observations
                .into_iter()
                .filter_map(ScalarObservation::into_last_known_value)
                .collect(),
            now_ms,
        )
    } else {
        ScalarObservationGroup::partial(
            current_refresh_slots(observations),
            now_ms,
            FailureKind::ProviderFault,
        )
    }
}

fn scalar_from_source<T>(
    value: Option<T>,
    source: &SourceStatus,
    now_ms: u64,
) -> ScalarObservation<T> {
    match (value, source.outcome) {
        (Some(value), SourceOutcome::Available) => ScalarObservation::available(value, now_ms),
        (Some(value), SourceOutcome::Partial(failure)) => {
            ScalarObservation::partial(value, now_ms, failure)
        }
        (Some(value), SourceOutcome::Empty | SourceOutcome::Unavailable(_)) => {
            ScalarObservation::partial(value, now_ms, FailureKind::ProviderFault)
        }
        (None, SourceOutcome::Partial(failure) | SourceOutcome::Unavailable(failure)) => {
            ScalarObservation::unavailable(failure)
        }
        (None, SourceOutcome::Available | SourceOutcome::Empty) => {
            ScalarObservation::unavailable(FailureKind::Unsupported)
        }
    }
}

fn scalar_group_from_source<T>(
    observations: Vec<ScalarObservation<T>>,
    source: &SourceStatus,
    now_ms: u64,
) -> ScalarObservationGroup<T> {
    match source.outcome {
        SourceOutcome::Available
            if observations
                .iter()
                .all(|observation| observation.availability() == ScalarAvailability::Available) =>
        {
            ScalarObservationGroup::available(
                observations
                    .into_iter()
                    .filter_map(ScalarObservation::into_last_known_value)
                    .collect(),
                now_ms,
            )
        }
        SourceOutcome::Available => ScalarObservationGroup::partial(
            current_refresh_slots(observations),
            now_ms,
            FailureKind::ProviderFault,
        ),
        SourceOutcome::Empty if observations.is_empty() => {
            ScalarObservationGroup::available(Vec::new(), now_ms)
        }
        SourceOutcome::Empty => ScalarObservationGroup::partial(
            current_refresh_slots(observations),
            now_ms,
            FailureKind::ProviderFault,
        ),
        SourceOutcome::Partial(failure) => {
            ScalarObservationGroup::partial(current_refresh_slots(observations), now_ms, failure)
        }
        SourceOutcome::Unavailable(failure) => {
            let slot_failures = observations
                .iter()
                .map(|observation| observation.availability().failure().unwrap_or(failure))
                .collect();
            ScalarObservationGroup::unavailable_slots(slot_failures, failure)
        }
    }
}

fn current_refresh_slots<T>(
    observations: Vec<ScalarObservation<T>>,
) -> Vec<ScalarObservationSlot<T>> {
    observations
        .into_iter()
        .map(|observation| {
            let availability = observation.availability();
            match (availability, observation.into_last_known_value()) {
                (ScalarAvailability::Available, Some(value)) => {
                    ScalarObservationSlot::Current(value)
                }
                (ScalarAvailability::Partial(failure), Some(value)) => {
                    ScalarObservationSlot::Partial(value, failure)
                }
                (availability, _) => ScalarObservationSlot::Unavailable(
                    availability.failure().unwrap_or(FailureKind::ProviderFault),
                ),
            }
        })
        .collect()
}

fn optional_from_source<T>(
    value: Option<T>,
    source: &SourceStatus,
    now_ms: u64,
) -> OptionalObservation<T> {
    match (value, source.outcome) {
        (Some(value), SourceOutcome::Available) => OptionalObservation::present(value, now_ms),
        (Some(value), SourceOutcome::Partial(failure) | SourceOutcome::Unavailable(failure)) => {
            OptionalObservation::partial_present(value, now_ms, failure)
        }
        (Some(value), SourceOutcome::Empty) => {
            OptionalObservation::partial_present(value, now_ms, FailureKind::ProviderFault)
        }
        (None, SourceOutcome::Partial(failure) | SourceOutcome::Unavailable(failure)) => {
            OptionalObservation::unavailable(failure)
        }
        (None, SourceOutcome::Available | SourceOutcome::Empty) => {
            OptionalObservation::absent(now_ms)
        }
    }
}

fn observe_rapl_power(
    energy_uj: Option<u64>,
    previous: &mut Option<(u64, Instant)>,
    max_energy_uj: u64,
    now: Instant,
    now_ms: u64,
    source: &SourceStatus,
) -> ScalarObservation<f32> {
    let power_w = match (energy_uj, *previous) {
        (Some(current_uj), Some((previous_uj, previous_at))) => {
            let elapsed = now.duration_since(previous_at).as_secs_f32();
            if elapsed > 0.0 {
                let delta_uj = if current_uj >= previous_uj {
                    current_uj - previous_uj
                } else {
                    max_energy_uj
                        .saturating_sub(previous_uj)
                        .saturating_add(current_uj)
                };
                Some((delta_uj as f32 / 1_000_000.0) / elapsed)
            } else {
                None
            }
        }
        _ => None,
    };
    *previous = energy_uj.map(|energy| (energy, now));

    power_w.map_or_else(
        || {
            if energy_uj.is_some() {
                ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable)
            } else {
                scalar_from_source(None, source, now_ms)
            }
        },
        |power| scalar_from_source(Some(power), source, now_ms),
    )
}

/// Gather all CPU telemetry for one tick: per-core + global utilization, live
/// frequency, package temperature, native performance policy, cache layout,
/// and RAPL package power.
pub(super) fn collect_cpu(
    sys: &System,
    prev_rapl: &mut Option<(u64, Instant)>,
    cache: (Option<u64>, Option<u64>, Option<u64>, Option<u64>),
    rapl_max_energy_uj: u64,
    now: Instant,
    now_ms: u64,
) -> CompositeSourceSnapshot<CpuMetrics> {
    let cpus = sys.cpus();
    let global_usage = sys.global_cpu_usage();
    let core_usages: Vec<f32> = cpus.iter().map(|cpu| cpu.cpu_usage()).collect();
    let brand = cpus
        .first()
        .map(|cpu| cpu.brand().trim())
        .filter(|brand| !brand.is_empty())
        .map(str::to_string);

    let physical_core_count = sysinfo::System::physical_core_count().filter(|count| *count > 0);
    let sysinfo_status = cpu_sources::sysinfo_status(cpus.len(), physical_core_count.is_some());
    let cpufreq = observe_cpufreq(cpus.len());
    let bogomips = observe_bogomips();
    let temperatures = observe_temperatures(cpus.len());
    let rapl = observe_rapl();
    let sysinfo_frequency_mhz = cpus
        .first()
        .and_then(|cpu| observed_sysinfo_frequency_mhz(cpu.frequency()));
    let (frequency_mhz, frequency_source) =
        cpufreq.current_mhz.or(sysinfo_frequency_mhz).map_or_else(
            || {
                bogomips
                    .value
                    .and_then(bogomips_to_frequency_value)
                    .map_or((None, CpuFrequencySource::Native), |frequency| {
                        (Some(frequency), CpuFrequencySource::BogoMips)
                    })
            },
            |frequency| (Some(frequency), CpuFrequencySource::Native),
        );
    let max_freq_mhz = cpufreq.max_mhz;

    // RAPL is a wrapping monotonic energy counter. A failed read must not seed
    // the next delta or the following successful sample would report a spike.
    let power_observation = observe_rapl_power(
        rapl.energy_uj,
        prev_rapl,
        rapl_max_energy_uj,
        now,
        now_ms,
        &rapl.status,
    );
    let usage_failure = if cpus.is_empty() {
        Some(FailureKind::ProviderFault)
    } else {
        None
    };
    // Value-sanity gate only: zero-window/rollback counter discipline lives in
    // core `counter.rs`; this stops phantom percentages at the sysinfo border.
    let global_usage_observation = usage_failure.map_or_else(
        || cpu_usage_pct_observation(global_usage, now_ms),
        ScalarObservation::unavailable,
    );
    let core_usage_group = usage_failure.map_or_else(
        || usage_group_from_percentages(&core_usages, now_ms),
        ScalarObservationGroup::unavailable,
    );
    let frequency_observation = if cpufreq.current_mhz.is_some() {
        scalar_from_source(cpufreq.current_mhz, &cpufreq.status, now_ms)
    } else if let Some(frequency) = sysinfo_frequency_mhz {
        ScalarObservation::available(frequency, now_ms)
    } else if let Some(frequency) = frequency_mhz {
        scalar_from_source(Some(frequency), &bogomips.status, now_ms)
    } else {
        scalar_from_source(None, &bogomips.status, now_ms)
    };
    let max_frequency_observation = scalar_from_source(max_freq_mhz, &cpufreq.status, now_ms);
    let per_core_frequency_observations = cpufreq
        .per_core_mhz
        .iter()
        .copied()
        .map(|frequency| scalar_from_source(frequency, &cpufreq.status, now_ms))
        .collect::<Vec<_>>();
    let per_core_frequency_group = scalar_group_from_source(
        per_core_frequency_observations.clone(),
        &cpufreq.status,
        now_ms,
    );
    let temperature_observation =
        scalar_from_source(temperatures.package_c, &temperatures.status, now_ms);
    let per_core_temperature_observations = temperatures
        .per_core_c
        .iter()
        .map(|reading| match reading {
            Some(cpu_sources::LogicalCoreTemperature {
                temperature_c,
                provenance: cpu_sources::TemperatureProvenance::DirectlyMeasured,
            }) => scalar_from_source(Some(*temperature_c), &temperatures.status, now_ms),
            Some(cpu_sources::LogicalCoreTemperature {
                temperature_c,
                provenance: cpu_sources::TemperatureProvenance::PhysicalSiblingShared,
            }) => ScalarObservation::partial(*temperature_c, now_ms, FailureKind::Unsupported),
            None => scalar_from_source(None, &temperatures.status, now_ms),
        })
        .collect::<Vec<_>>();
    let per_core_temperature_group = scalar_group_from_source(
        per_core_temperature_observations.clone(),
        &temperatures.status,
        now_ms,
    );
    let scalar_observations = CpuScalarObservations {
        global_usage_pct: global_usage_observation,
        core_usage_group,
        frequency_mhz: frequency_observation,
        max_frequency_mhz: max_frequency_observation,
        per_core_frequency_group,
        temperature_c: temperature_observation,
        per_core_temperature_group,
        power_w: power_observation,
    };

    let (l1d_cache_kb, l1i_cache_kb, l2_cache_kb, l3_cache_kb) = cache;
    let mut sources = vec![
        sysinfo_status,
        cpufreq.status,
        temperatures.status,
        rapl.status,
    ];
    // BogoMIPS is an optional fallback source. Its absence must not degrade a
    // healthy native-frequency CPU domain; once it is actually selected (or is
    // the only remaining frequency source), publish its typed outcome.
    if frequency_source.is_bogomips() || frequency_mhz.is_none() {
        sources.push(bogomips.status);
    }

    let mut metrics = CpuMetrics::from_observations(scalar_observations);
    metrics.brand = brand;
    metrics.frequency_source = frequency_source;
    metrics.temperature_source = temperatures.package_source;
    metrics.physical_cores = physical_core_count;
    metrics.logical_cores = (!cpus.is_empty()).then_some(cpus.len());
    metrics.l1d_cache_kb = l1d_cache_kb;
    metrics.l1i_cache_kb = l1i_cache_kb;
    metrics.l2_cache_kb = l2_cache_kb;
    metrics.l3_cache_kb = l3_cache_kb;
    metrics.performance_policy = CpuPerformancePolicy {
        frequency_implementation: cpufreq.driver,
        active_policy: cpufreq.governor,
        energy_preference: cpufreq.power_preference,
    };
    CompositeSourceSnapshot::new(metrics, sources)
}

/// Gather the memory breakdown and the signed used-memory delta rate for one
/// tick. Static DMI facts and dynamic `/proc/meminfo` values are assembled into
/// the shared memory model here, inside the native adapter.
pub(super) fn collect_memory(
    sys: &System,
    prev_mem_used: &mut Option<(u64, Instant)>,
    now: Instant,
    now_ms: u64,
) -> CompositeSourceSnapshot<MemoryMetrics> {
    let meminfo = observe_meminfo();
    let dmi = observe_dmi_memory();
    let compressed_swap = observe_compressed_swap();
    let module_and_compression = memory_sources::assemble_module_and_compression_observations(
        &dmi,
        &compressed_swap,
        now_ms,
    );
    let sysinfo_status = memory_sources::sysinfo_status();
    let mem_total = sys.total_memory();
    let mem_available = sys.available_memory();
    let mem_used = sys.used_memory();
    let (bytes_per_sec, new_prev) = delta_rate(*prev_mem_used, mem_used, now);
    *prev_mem_used = new_prev;
    let mem_used_rate_mbps = bytes_per_sec.map(|rate| (rate / 1_048_576.0) as f32);
    let scalar_observations = MemoryScalarObservations {
        total_bytes: ScalarObservation::available(mem_total, now_ms),
        used_bytes: ScalarObservation::available(mem_used, now_ms),
        available_bytes: ScalarObservation::available(mem_available, now_ms),
        swap_total_bytes: ScalarObservation::available(sys.total_swap(), now_ms),
        swap_used_bytes: ScalarObservation::available(sys.used_swap(), now_ms),
        used_rate_mib_per_sec: mem_used_rate_mbps.map_or_else(
            || ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
            |rate| ScalarObservation::available(rate, now_ms),
        ),
    };
    let optional_observations = MemoryOptionalObservations {
        composition: MemoryCompositionObservations {
            cached_bytes: optional_from_source(
                meminfo.fields.get("Cached").copied(),
                &meminfo.status,
                now_ms,
            ),
            buffers_bytes: optional_from_source(
                meminfo.fields.get("Buffers").copied(),
                &meminfo.status,
                now_ms,
            ),
            active_bytes: optional_from_source(
                meminfo.fields.get("Active").copied(),
                &meminfo.status,
                now_ms,
            ),
            inactive_bytes: optional_from_source(
                meminfo.fields.get("Inactive").copied(),
                &meminfo.status,
                now_ms,
            ),
            free_bytes: optional_from_source(
                meminfo.fields.get("MemFree").copied(),
                &meminfo.status,
                now_ms,
            ),
            reclaimable_bytes: optional_from_source(
                meminfo.fields.get("SReclaimable").copied(),
                &meminfo.status,
                now_ms,
            ),
            // OpenZFS publishes its ARC size as an optional `Zfs:` line.
            // Absent on non-ZFS hosts, it must stay a typed absence rather
            // than a failure; a malformed value is dropped by the parser.
            zfs_arc_bytes: optional_from_source(
                meminfo.fields.get("Zfs").copied(),
                &meminfo.status,
                now_ms,
            ),
        },
        // Linux `MemTotal - MemAvailable` is memory currently in use, not
        // firmware/device-reserved capacity. No exact provider is wired.
        hardware_reserved_bytes: OptionalObservation::unavailable(FailureKind::Unsupported),
        modules: module_and_compression.0,
        virtual_memory_commit: VirtualMemoryCommitObservations {
            committed_bytes: optional_from_source(
                meminfo.fields.get("Committed_AS").copied(),
                &meminfo.status,
                now_ms,
            ),
            limit_bytes: optional_from_source(
                meminfo.fields.get("CommitLimit").copied(),
                &meminfo.status,
                now_ms,
            ),
        },
        compression: module_and_compression.1,
    };
    let memory = MemoryMetrics::from_observations(scalar_observations, optional_observations);

    CompositeSourceSnapshot::new(
        memory,
        vec![
            sysinfo_status,
            meminfo.status,
            dmi.status,
            compressed_swap.status,
        ],
    )
}
#[cfg(test)]
#[path = "../../../tests/headless/engine/collector/compute.rs"]
mod tests;
