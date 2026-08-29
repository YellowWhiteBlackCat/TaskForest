//! Linux CPU temperature sensors and physical-core topology mapping.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::CpuTemperatureSource;

use super::{
    CpuTemperatureObservation, FailureSummary, LogicalCoreTemperature, PACKAGE_TIER_EXACT_CHIP,
    PACKAGE_TIER_LABELED_HWMON, PACKAGE_TIER_THERMAL_ZONE_BASE, TEMPERATURE_PROVIDER,
    TemperatureProvenance, is_cpu_package_labeled_fallback, read_optional_text,
    read_optional_text_allow_empty, read_optional_u64, source_status,
};

pub(super) fn observe_temperatures_from_paths(
    thermal_root: &Path,
    hwmon_root: &Path,
    cpu_root: &Path,
    logical_cpu_count: usize,
) -> CpuTemperatureObservation {
    let mut failures = FailureSummary::default();
    let mut observed = 0usize;
    let mut source_reached = false;
    let mut best_package: Option<(u8, f32, CpuTemperatureSource)> = None;
    let mut core_temperatures = HashMap::<u32, f32>::new();

    match fs::read_dir(thermal_root) {
        Ok(entries) => {
            source_reached = true;
            for entry in entries {
                let Ok(entry) = entry else {
                    failures.record(FailureKind::ProviderFault);
                    continue;
                };
                if !entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("thermal_zone")
                {
                    continue;
                }
                let sensor_type =
                    read_optional_text_allow_empty(&entry.path().join("type"), &mut failures)
                        .unwrap_or_default()
                        .to_ascii_lowercase();
                let Some(raw) =
                    read_optional_u64(&entry.path().join("temp"), &mut failures, &mut observed)
                else {
                    continue;
                };
                let priority = PACKAGE_TIER_THERMAL_ZONE_BASE + temperature_priority(&sensor_type);
                retain_preferred_temperature(
                    &mut best_package,
                    priority,
                    raw as f32 / 1000.0,
                    CpuTemperatureSource::ThermalZone,
                );
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => failures.record_io(&error),
    }

    match fs::read_dir(hwmon_root) {
        Ok(entries) => {
            source_reached = true;
            for entry in entries {
                let Ok(entry) = entry else {
                    failures.record(FailureKind::ProviderFault);
                    continue;
                };
                let mut ignored_count = 0usize;
                let name = read_optional_text(
                    &entry.path().join("name"),
                    &mut failures,
                    &mut ignored_count,
                )
                .unwrap_or_default()
                .to_ascii_lowercase();
                if name == "coretemp" {
                    for (index, input) in temperature_inputs(&entry.path(), &mut failures) {
                        let Some(raw) = read_optional_u64(&input, &mut failures, &mut observed)
                        else {
                            continue;
                        };
                        let label = read_optional_text_allow_empty(
                            &entry.path().join(format!("temp{index}_label")),
                            &mut failures,
                        )
                        .unwrap_or_default()
                        .to_ascii_lowercase();
                        let temperature = raw as f32 / 1000.0;
                        if label.contains("package") || (index == 1 && label.is_empty()) {
                            retain_preferred_temperature(
                                &mut best_package,
                                PACKAGE_TIER_EXACT_CHIP,
                                temperature,
                                CpuTemperatureSource::Coretemp,
                            );
                            continue;
                        }
                        let core_id = label
                            .strip_prefix("core ")
                            .and_then(|value| value.trim().parse::<u32>().ok())
                            .unwrap_or(index);
                        core_temperatures.insert(core_id, temperature);
                    }
                } else if name.contains("k10temp") || name.contains("zenpower") {
                    let source = if name.contains("zenpower") {
                        CpuTemperatureSource::Zenpower
                    } else {
                        CpuTemperatureSource::K10temp
                    };
                    let mut first = None;
                    let mut tctl = None;
                    let mut tdie = None;
                    for (index, input) in temperature_inputs(&entry.path(), &mut failures) {
                        let Some(raw) = read_optional_u64(&input, &mut failures, &mut observed)
                        else {
                            continue;
                        };
                        let temperature = raw as f32 / 1000.0;
                        first.get_or_insert(temperature);
                        let label = read_optional_text_allow_empty(
                            &entry.path().join(format!("temp{index}_label")),
                            &mut failures,
                        )
                        .unwrap_or_default()
                        .to_ascii_lowercase();
                        match label.as_str() {
                            "tdie" => tdie = Some(temperature),
                            "tctl" if tctl.is_none() => tctl = Some(temperature),
                            _ => {}
                        }
                    }
                    if let Some(temperature) = tdie.or(tctl).or(first) {
                        retain_preferred_temperature(
                            &mut best_package,
                            PACKAGE_TIER_EXACT_CHIP,
                            temperature,
                            source,
                        );
                    }
                } else {
                    // Labeled fallback tier: a temperature channel on any
                    // other hwmon chip is admitted only when its effective
                    // label — the channel label, or the chip name for an
                    // unlabeled channel — carries CPU-package semantics. The
                    // label is read BEFORE the input so a rejected GPU-ish or
                    // unlabeled channel never counts as an observed item: a
                    // chip whose channels are all rejected contributes no
                    // success to the source status.
                    for (index, input) in temperature_inputs(&entry.path(), &mut failures) {
                        let label = read_optional_text_allow_empty(
                            &entry.path().join(format!("temp{index}_label")),
                            &mut failures,
                        )
                        .unwrap_or_default();
                        let effective_label = if label.is_empty() {
                            name.as_str()
                        } else {
                            label.as_str()
                        };
                        if !is_cpu_package_labeled_fallback(effective_label) {
                            continue;
                        }
                        let Some(raw) = read_optional_u64(&input, &mut failures, &mut observed)
                        else {
                            continue;
                        };
                        retain_preferred_temperature(
                            &mut best_package,
                            PACKAGE_TIER_LABELED_HWMON,
                            raw as f32 / 1000.0,
                            CpuTemperatureSource::PackageHwmon,
                        );
                    }
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => failures.record_io(&error),
    }

    let per_core_c = build_per_logical_core_temperatures(
        core_temperatures,
        cpu_root,
        logical_cpu_count,
        &mut failures,
    );
    CpuTemperatureObservation {
        package_c: best_package.map(|(_, temperature, _)| temperature),
        package_source: best_package
            .map_or(CpuTemperatureSource::Coretemp, |(_, _, source)| source),
        per_core_c,
        status: source_status(TEMPERATURE_PROVIDER, observed, source_reached, failures),
    }
}

/// Map per-physical-core temperature readings onto the per-logical-core index
/// space using `/sys/devices/system/cpu/cpuN/topology/core_id`. All-or-nothing:
/// any missing/malformed topology entry degrades the whole mapping to the short
/// physical-core Vec (honest Partial via recorded `ProviderFault`); the spec
/// forbids per-core partial success. When `core_temperatures` is empty (e.g.
/// AMD k10temp exposes only Tdie at package scope) the per-core Vec is empty,
/// mirroring the historical "no per-core channels" truth.
fn build_per_logical_core_temperatures(
    core_temperatures: HashMap<u32, f32>,
    cpu_root: &Path,
    logical_cpu_count: usize,
    failures: &mut FailureSummary,
) -> Vec<Option<LogicalCoreTemperature>> {
    if core_temperatures.is_empty() {
        return Vec::new();
    }
    let Some(topology) = read_physical_core_topology(cpu_root, logical_cpu_count, failures) else {
        // Topology unavailable — degrade to a short Vec of the measured
        // physical-core readings, sorted by core_id. All entries are
        // `DirectlyMeasured` (no SMT claim can be made); the recorded
        // `ProviderFault` flows through `source_status` as `Partial`.
        let mut sorted = core_temperatures.into_iter().collect::<Vec<_>>();
        sorted.sort_by_key(|(core_id, _)| *core_id);
        return sorted
            .into_iter()
            .map(|(_, temperature_c)| {
                Some(LogicalCoreTemperature {
                    temperature_c,
                    provenance: TemperatureProvenance::DirectlyMeasured,
                })
            })
            .collect::<Vec<_>>();
    };
    // Topology available — pad to `logical_cpu_count`, mapping each logical
    // index to its physical core's reading. The first logical CPU seen on each
    // physical core carries `DirectlyMeasured`; SMT siblings inherit the same
    // temperature as `PhysicalSiblingShared` so downstream availability can
    // distinguish "this exact logical core was sampled" from "inherited from
    // the shared physical sensor". Unmapped physical cores emit `None`.
    let mut seen_physical: HashSet<u32> = HashSet::new();
    let mut per_core = Vec::with_capacity(topology.len());
    for physical_core_id in topology {
        match core_temperatures.get(&physical_core_id) {
            Some(&temperature_c) => {
                let provenance = if seen_physical.insert(physical_core_id) {
                    TemperatureProvenance::DirectlyMeasured
                } else {
                    TemperatureProvenance::PhysicalSiblingShared
                };
                per_core.push(Some(LogicalCoreTemperature {
                    temperature_c,
                    provenance,
                }));
            }
            None => per_core.push(None),
        }
    }
    per_core
}

/// Read the logical-CPU → physical-core-id table from
/// `/sys/devices/system/cpu/cpuN/topology/core_id`. All-or-nothing: a single
/// missing or malformed entry returns `None` and records `ProviderFault` so the
/// caller can fall back to the short physical-core Vec (no per-core partial
/// success, no fabricated mapping).
fn read_physical_core_topology(
    cpu_root: &Path,
    logical_cpu_count: usize,
    failures: &mut FailureSummary,
) -> Option<Vec<u32>> {
    let mut table = Vec::with_capacity(logical_cpu_count);
    for logical in 0..logical_cpu_count {
        let path = cpu_root
            .join(format!("cpu{logical}"))
            .join("topology")
            .join("core_id");
        match fs::read_to_string(&path) {
            Ok(raw) => match raw.trim().parse::<u32>() {
                Ok(core_id) => table.push(core_id),
                Err(_) => {
                    failures.record(FailureKind::ProviderFault);
                    return None;
                }
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                failures.record(FailureKind::ProviderFault);
                return None;
            }
            Err(error) => {
                failures.record_io(&error);
                return None;
            }
        }
    }
    Some(table)
}

/// Intra-zone priority for ACPI thermal-zone sensors (0 = most specific).
/// Added on top of [`PACKAGE_TIER_THERMAL_ZONE_BASE`] so zones stay the
/// lowest package-temperature tier regardless of their specificity.
fn temperature_priority(sensor_type: &str) -> u8 {
    if sensor_type.contains("x86_pkg_temp")
        || sensor_type.contains("pkg_temp")
        || sensor_type.contains("tctl")
        || sensor_type.contains("tdie")
    {
        0
    } else if sensor_type.contains("cpu_thermal")
        || sensor_type.contains("coretemp")
        || sensor_type.contains("k10temp")
    {
        1
    } else if sensor_type.contains("acpitz") {
        2
    } else {
        3
    }
}

fn retain_preferred_temperature(
    best: &mut Option<(u8, f32, CpuTemperatureSource)>,
    priority: u8,
    temperature: f32,
    source: CpuTemperatureSource,
) {
    if best.is_none_or(|(current, _, _)| priority < current) {
        *best = Some((priority, temperature, source));
    }
}

fn temperature_inputs(dir: &Path, failures: &mut FailureSummary) -> Vec<(u32, PathBuf)> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            failures.record_io(&error);
            return Vec::new();
        }
    };
    let mut inputs = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(index) = name
            .strip_prefix("temp")
            .and_then(|value| value.strip_suffix("_input"))
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        inputs.push((index, entry.path()));
    }
    inputs.sort_by_key(|(index, _)| *index);
    inputs
}
