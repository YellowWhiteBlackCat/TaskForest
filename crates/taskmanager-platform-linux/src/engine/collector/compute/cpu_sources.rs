//! Independently fallible Linux CPU telemetry source probes.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use taskmanager_core::core::metrics::CpuTemperatureSource;
use taskmanager_platform_contract::{FailureKind, ProviderId, SourceOutcome, SourceStatus};

const SYSINFO_PROVIDER: &str = "linux.telemetry.cpu.sysinfo";
const CPUFREQ_PROVIDER: &str = "linux.telemetry.cpu.cpufreq";
const BOGOMIPS_PROVIDER: &str = "linux.telemetry.cpu.bogomips";
const TEMPERATURE_PROVIDER: &str = "linux.telemetry.cpu.hwmon-temperature";
const RAPL_PROVIDER: &str = "linux.telemetry.cpu.rapl";

/// Package-temperature selection tiers; a strictly lower tier number wins and
/// equal tiers keep the first reading (`retain_preferred_temperature`).
const PACKAGE_TIER_EXACT_CHIP: u8 = 0;
const PACKAGE_TIER_LABELED_HWMON: u8 = 1;
/// ACPI thermal zones are the last-resort tier. The historical intra-zone
/// priority order from [`temperature_priority`] is preserved by adding it on
/// top of this base, so no zone can ever outrank a hwmon chip tier.
const PACKAGE_TIER_THERMAL_ZONE_BASE: u8 = 2;

/// Case-insensitive label tokens that identify a temperature channel as
/// CPU-package scoped for the labeled fallback tier. A channel on a chip
/// other than coretemp/k10temp/zenpower qualifies only when its effective
/// label — the `tempN_label` contents, or the chip `name` when the channel
/// is unlabeled — contains one of these tokens.
const CPU_PACKAGE_LABEL_TOKENS: [&str; 5] = ["tctl", "tdie", "package", "apu", "cpu"];

/// GPU/board sensor label tokens that must never feed the CPU package
/// readout, even when a CPU-ish token co-occurs: amdgpu exposes `edge` /
/// `junction` / `mem` for the GPU die, hotspot, and VRAM, and board chips
/// expose `VRM` power-stage temperatures. Rejection dominates acceptance —
/// a hypothetical "CPU junction" label is still not trusted as package
/// truth from an unknown chip.
const NON_CPU_LABEL_TOKENS: [&str; 4] = ["edge", "junction", "mem", "vrm"];

/// Labeled-fallback admission rule: the effective label must carry a
/// CPU-package token and must not carry any GPU/board token.
fn is_cpu_package_labeled_fallback(effective_label: &str) -> bool {
    let label = effective_label.to_ascii_lowercase();
    if NON_CPU_LABEL_TOKENS
        .iter()
        .any(|token| label.contains(token))
    {
        return false;
    }
    CPU_PACKAGE_LABEL_TOKENS
        .iter()
        .any(|token| label.contains(token))
}

#[derive(Debug)]
pub(super) struct CpuFreqObservation {
    pub current_mhz: Option<u64>,
    pub max_mhz: Option<u64>,
    pub per_core_mhz: Vec<Option<u64>>,
    pub driver: Option<String>,
    pub governor: Option<String>,
    pub power_preference: Option<String>,
    pub status: SourceStatus,
}

#[derive(Debug)]
pub(super) struct BogoMipsObservation {
    pub value: Option<f32>,
    pub status: SourceStatus,
}

#[derive(Debug)]
pub(super) struct CpuTemperatureObservation {
    pub package_c: Option<f32>,
    /// Which tier produced `package_c`. When no package temperature was
    /// found this stays at the enum default (`Coretemp`): with no value
    /// there is no provenance to surface, and the default is omitted from
    /// serialized snapshots.
    pub package_source: CpuTemperatureSource,
    /// Per-logical-core temperature readings. When topology is available this
    /// is padded to `logical_cpu_count`: SMT siblings inherit their parent
    /// physical core's reading (the silicon thermal sensor is shared). When
    /// topology is unavailable this degrades to a short Vec of the measured
    /// physical-core readings (honest Partial — not faked as logical-indexed).
    /// `None` marks a logical index with no sensor mapping (e.g. AMD k10temp
    /// exposes only Tdie at package scope, or a heterogeneous sensor set).
    pub per_core_c: Vec<Option<LogicalCoreTemperature>>,
    pub status: SourceStatus,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct LogicalCoreTemperature {
    pub temperature_c: f32,
    pub provenance: TemperatureProvenance,
}

/// Honest accounting of how a per-logical-core temperature was obtained. The
/// four-state `ScalarAvailability` is preserved end-to-end: a directly-measured
/// reading becomes `Available`; an SMT-shared reading becomes
/// `Partial(Unsupported)` (current value, sensor granularity at the physical
/// core); a missing mapping becomes `Unavailable`. No value is labelled as a
/// fresh per-logical-core measurement when it was actually inherited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TemperatureProvenance {
    /// The sensor exposes a reading scoped to this exact logical core. On
    /// non-SMT hosts every entry is `DirectlyMeasured`; on SMT hosts the first
    /// logical CPU mapped to each physical core carries the canonical label.
    DirectlyMeasured,
    /// The sensor is shared at physical-core granularity (Intel coretemp: one
    /// diode per physical core). SMT siblings inherit the parent physical
    /// core's current reading — accurate (one silicon die, one sensor) but
    /// not a per-logical-core measurement, so the value is published as
    /// `Partial(Unsupported)` to keep the four-state availability honest.
    PhysicalSiblingShared,
}

#[derive(Debug)]
pub(super) struct RaplObservation {
    pub energy_uj: Option<u64>,
    pub status: SourceStatus,
}

#[derive(Debug, Default)]
struct FailureSummary {
    failure: Option<FailureKind>,
}

impl FailureSummary {
    fn record(&mut self, failure: FailureKind) {
        if self
            .failure
            .is_none_or(|current| priority(failure) > priority(current))
        {
            self.failure = Some(failure);
        }
    }

    fn record_io(&mut self, error: &io::Error) {
        self.record(classify_io(error));
    }
}

const fn priority(failure: FailureKind) -> u8 {
    match failure {
        FailureKind::RequiresEscalation => 9,
        FailureKind::PermissionDenied => 8,
        FailureKind::MissingDependency => 7,
        FailureKind::TimedOut => 6,
        FailureKind::ProviderFault => 5,
        FailureKind::TemporarilyUnavailable => 4,
        FailureKind::Unsupported => 3,
        FailureKind::IdentityChanged | FailureKind::Rejected => 1,
    }
}

fn classify_io(error: &io::Error) -> FailureKind {
    match error.kind() {
        io::ErrorKind::NotFound => FailureKind::Unsupported,
        io::ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        io::ErrorKind::TimedOut => FailureKind::TimedOut,
        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock => {
            FailureKind::TemporarilyUnavailable
        }
        _ => FailureKind::ProviderFault,
    }
}

fn source_status(
    provider: &'static str,
    observed: usize,
    source_reached: bool,
    failures: FailureSummary,
) -> SourceStatus {
    let outcome = if observed > 0 {
        failures
            .failure
            .map_or(SourceOutcome::Available, SourceOutcome::Partial)
    } else if let Some(failure) = failures.failure {
        SourceOutcome::Unavailable(failure)
    } else if source_reached {
        SourceOutcome::Empty
    } else {
        SourceOutcome::Unavailable(FailureKind::Unsupported)
    };
    SourceStatus {
        provider: ProviderId::borrowed(provider),
        outcome,
        item_count: observed,
    }
}

pub(super) fn sysinfo_status(
    logical_cpu_count: usize,
    physical_topology_available: bool,
) -> SourceStatus {
    SourceStatus {
        provider: ProviderId::borrowed(SYSINFO_PROVIDER),
        outcome: if logical_cpu_count == 0 {
            SourceOutcome::Unavailable(FailureKind::ProviderFault)
        } else if !physical_topology_available {
            SourceOutcome::Partial(FailureKind::ProviderFault)
        } else {
            SourceOutcome::Available
        },
        item_count: logical_cpu_count,
    }
}

pub(super) fn observe_cpufreq(logical_cpu_count: usize) -> CpuFreqObservation {
    observe_cpufreq_at(Path::new("/sys/devices/system/cpu"), logical_cpu_count)
}

pub(super) fn observe_bogomips() -> BogoMipsObservation {
    observe_bogomips_at(Path::new("/proc/cpuinfo"))
}

pub(super) fn bogomips_to_frequency_value(value: f32) -> Option<u64> {
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    // Keep the raw BogoMIPS scale. It is intentionally not converted to MHz:
    // BogoMIPS is a calibration score, not a clock measurement. The typed
    // CpuFrequencySource drives the UI qualifier and graph formatter.
    format!("{value:.0}").parse::<u64>().ok()
}

fn observe_bogomips_at(path: &Path) -> BogoMipsObservation {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) => {
            let failure = classify_io(&error);
            return BogoMipsObservation {
                value: None,
                status: source_status(
                    BOGOMIPS_PROVIDER,
                    0,
                    false,
                    FailureSummary {
                        failure: Some(failure),
                    },
                ),
            };
        }
    };

    let mut values = Vec::new();
    let mut failures = FailureSummary::default();
    for line in contents.lines() {
        let Some((key, raw)) = line.split_once(':') else {
            continue;
        };
        if !key.trim().eq_ignore_ascii_case("bogomips") {
            continue;
        }
        match raw.trim().parse::<f32>() {
            Ok(value) if value.is_finite() && value > 0.0 => values.push(value),
            Ok(_) | Err(_) => failures.record(FailureKind::ProviderFault),
        }
    }
    let value = values.first().copied();
    BogoMipsObservation {
        value,
        status: source_status(BOGOMIPS_PROVIDER, values.len(), true, failures),
    }
}

fn observe_cpufreq_at(cpu_root: &Path, logical_cpu_count: usize) -> CpuFreqObservation {
    let boot_cpufreq = cpu_root.join("cpu0").join("cpufreq");
    let mut failures = FailureSummary::default();
    let source_reached = match fs::read_dir(&boot_cpufreq) {
        Ok(_) => true,
        Err(error) => {
            failures.record_io(&error);
            false
        }
    };
    let mut observed = 0usize;

    let current_mhz = read_optional_u64(
        &boot_cpufreq.join("scaling_cur_freq"),
        &mut failures,
        &mut observed,
    )
    .map(|khz| khz / 1000);
    let max_mhz = read_optional_u64(
        &boot_cpufreq.join("cpuinfo_max_freq"),
        &mut failures,
        &mut observed,
    )
    .map(|khz| khz / 1000);
    let driver = read_optional_text(
        &boot_cpufreq.join("scaling_driver"),
        &mut failures,
        &mut observed,
    );
    let governor = read_optional_text(
        &boot_cpufreq.join("scaling_governor"),
        &mut failures,
        &mut observed,
    );
    let power_preference = read_optional_text(
        &boot_cpufreq.join("energy_performance_preference"),
        &mut failures,
        &mut observed,
    );
    let mut per_core_mhz = Vec::with_capacity(logical_cpu_count);
    // Package max boost = the highest `cpuinfo_max_freq` across ALL cores. On
    // hybrid CPUs (Intel P+E cores) cpu0 is often an E-core with a lower cap,
    // so reading only cpu0 under-reports the package's real max boost.
    let mut package_max_mhz: Option<u64> = None;
    for logical in 0..logical_cpu_count {
        let cpu_cpufreq = cpu_root.join(format!("cpu{logical}")).join("cpufreq");
        let frequency = read_optional_u64(
            &cpu_cpufreq.join("scaling_cur_freq"),
            &mut failures,
            &mut observed,
        )
        .map(|khz| khz / 1000);
        if frequency.is_none() {
            failures.record(FailureKind::Unsupported);
        }
        if let Some(khz) = read_optional_u64(
            &cpu_cpufreq.join("cpuinfo_max_freq"),
            &mut failures,
            &mut observed,
        ) {
            let mhz = khz / 1000;
            package_max_mhz = Some(package_max_mhz.map_or(mhz, |m| m.max(mhz)));
        }
        per_core_mhz.push(frequency);
    }
    // Prefer the hybrid-aware package max; fall back to the cpu0 read above when
    // no core exposed cpuinfo_max_freq (e.g. a stub cpufreq dir in tests).
    let max_mhz = package_max_mhz.or(max_mhz);

    CpuFreqObservation {
        current_mhz,
        max_mhz,
        per_core_mhz,
        driver,
        governor,
        power_preference,
        status: source_status(CPUFREQ_PROVIDER, observed, source_reached, failures),
    }
}

pub(super) fn observe_temperatures(logical_cpu_count: usize) -> CpuTemperatureObservation {
    observe_temperatures_at(
        Path::new("/sys/class/thermal"),
        Path::new("/sys/class/hwmon"),
        Path::new("/sys/devices/system/cpu"),
        logical_cpu_count,
    )
}

fn observe_temperatures_at(
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

pub(super) fn observe_rapl() -> RaplObservation {
    observe_rapl_at(Path::new("/sys/class/powercap"))
}

fn observe_rapl_at(powercap_root: &Path) -> RaplObservation {
    let mut failures = FailureSummary::default();
    let entries = match fs::read_dir(powercap_root) {
        Ok(entries) => entries,
        Err(error) => {
            failures.record_io(&error);
            return RaplObservation {
                energy_uj: None,
                status: source_status(RAPL_PROVIDER, 0, false, failures),
            };
        }
    };

    let mut observed = 0usize;
    let mut total = 0u64;
    for entry in entries {
        let Ok(entry) = entry else {
            failures.record(FailureKind::ProviderFault);
            continue;
        };
        let entry_name = entry.file_name();
        let entry_name = entry_name.to_string_lossy();
        if !entry_name.starts_with("intel-rapl:") || entry_name.matches(':').count() != 1 {
            continue;
        }
        let mut ignored_count = 0usize;
        let package_name = read_optional_text(
            &entry.path().join("name"),
            &mut failures,
            &mut ignored_count,
        )
        .unwrap_or_default();
        if !package_name.starts_with("package") {
            continue;
        }
        // `energy_uj` is mode 0400 (root-only) on mainline, so read it
        // escalation-aware: a permission gap classifies as RequiresEscalation
        // (PackagePowerRapl) rather than a bare denial. The sibling RAPL nodes
        // (name / max_energy_range_uj) are world-readable and stay on
        // `read_optional_u64`.
        let energy = match fs::read_to_string(entry.path().join("energy_uj")) {
            Ok(raw) => match raw.trim().parse::<u64>() {
                Ok(value) => {
                    observed = observed.saturating_add(1);
                    Some(value)
                }
                Err(_) => {
                    failures.record(FailureKind::ProviderFault);
                    None
                }
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => {
                failures.record(classify_rapl_io(&error));
                None
            }
        };
        if let Some(energy) = energy {
            total = total.saturating_add(energy);
        }
    }
    RaplObservation {
        energy_uj: (observed > 0).then_some(total),
        status: source_status(RAPL_PROVIDER, observed, true, failures),
    }
}

/// Classify an I/O failure on the root-only RAPL `energy_uj` node:
/// `PermissionDenied` becomes [`FailureKind::RequiresEscalation`] when the gate
/// confirms [`EscalationFeature::PackagePowerRapl`] is escalatable. Mirrors the
/// memory SMBIOS + Intel PMU escalation-aware classification.
fn classify_rapl_io(error: &io::Error) -> FailureKind {
    use taskmanager_escalation::{
        EscalationAvailability, EscalationFeature, PrivilegeGate, UnprivilegedGate,
    };
    let denied = error.kind() == io::ErrorKind::PermissionDenied;
    if denied
        && matches!(
            UnprivilegedGate.probe(EscalationFeature::PackagePowerRapl),
            EscalationAvailability::RequiresEscalation(_)
        )
    {
        FailureKind::RequiresEscalation
    } else if denied {
        FailureKind::PermissionDenied
    } else {
        FailureKind::ProviderFault
    }
}

fn read_optional_text(
    path: &Path,
    failures: &mut FailureSummary,
    observed: &mut usize,
) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(value) => {
            let value = value.trim();
            if value.is_empty() {
                failures.record(FailureKind::ProviderFault);
                None
            } else {
                *observed = observed.saturating_add(1);
                Some(value.to_string())
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => {
            failures.record_io(&error);
            None
        }
    }
}

fn read_optional_text_allow_empty(path: &Path, failures: &mut FailureSummary) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(value) => Some(value.trim().to_string()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => {
            failures.record_io(&error);
            None
        }
    }
}

fn read_optional_u64(
    path: &Path,
    failures: &mut FailureSummary,
    observed: &mut usize,
) -> Option<u64> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
        Err(error) => {
            failures.record_io(&error);
            return None;
        }
    };
    match raw.trim().parse::<u64>() {
        Ok(value) => {
            *observed = observed.saturating_add(1);
            Some(value)
        }
        Err(_) => {
            failures.record(FailureKind::ProviderFault);
            None
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/headless/engine/collector/compute/cpu_sources.rs"]
mod tests;
