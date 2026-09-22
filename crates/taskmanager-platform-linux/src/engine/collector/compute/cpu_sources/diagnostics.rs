//! Package topology, power limits, idle counters and interrupt distribution.

use std::{fs, path::Path};
use taskmanager_core::core::metrics::{CpuIdleState, CpuInterruptSnapshot, CpuPackageMetrics};

/// Read package/socket topology and the cumulative thermal-throttle counters
/// exported by Linux sysfs. The result is deliberately package-scoped: a
/// package counter is repeated for every logical CPU, while core counters are
/// counted once per physical-core identity. Missing topology remains absent
/// instead of being collapsed into a fake single socket.
pub(in super::super) fn observe_cpu_packages(logical_cpu_count: usize) -> Vec<CpuPackageMetrics> {
    observe_cpu_packages_at(
        Path::new("/sys/devices/system/cpu"),
        Path::new("/sys/devices/system/node"),
        logical_cpu_count,
    )
}

pub(in super::super) fn observe_cpu_packages_at(
    cpu_root: &Path,
    node_root: &Path,
    logical_cpu_count: usize,
) -> Vec<CpuPackageMetrics> {
    use std::collections::{BTreeMap, BTreeSet};

    #[derive(Default)]
    struct PackageAccumulator {
        package: CpuPackageMetrics,
        physical_cores: BTreeSet<u32>,
        counted_core_throttle: BTreeSet<u32>,
        frequency_sum_mhz: u64,
        frequency_samples: u64,
    }

    let mut packages = BTreeMap::<u32, PackageAccumulator>::new();
    let mut cpu_ids = fs::read_dir(cpu_root)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.strip_prefix("cpu")?.parse::<usize>().ok()
        })
        .filter(|cpu| *cpu < logical_cpu_count)
        .collect::<Vec<_>>();
    cpu_ids.sort_unstable();
    cpu_ids.dedup();

    for cpu in cpu_ids {
        let cpu_path = cpu_root.join(format!("cpu{cpu}"));
        let topology = cpu_path.join("topology");
        let Some(package_id) = read_u32(&topology.join("physical_package_id")) else {
            continue;
        };
        let entry = packages.entry(package_id).or_default();
        entry.package.logical_core_ids.push(cpu);

        if let Some(core_id) = read_u32(&topology.join("core_id")) {
            entry.physical_cores.insert(core_id);
            let throttle_dir = cpu_path.join("thermal_throttle");
            // `core_throttle_count` is per physical core on current kernels;
            // choose the first logical sibling for a stable package total.
            if entry.counted_core_throttle.insert(core_id)
                && let Some(value) = read_u64(&throttle_dir.join("core_throttle_count"))
            {
                entry.package.core_throttle_count = Some(
                    entry
                        .package
                        .core_throttle_count
                        .unwrap_or_default()
                        .saturating_add(value),
                );
            }
        }

        if let Some(siblings) = read_trimmed(&topology.join("thread_siblings_list")) {
            let mut sibling_group = crate::engine::hardware::parse_cpulist(&siblings)
                .into_iter()
                .map(|cpu| cpu as usize)
                .collect::<Vec<_>>();
            sibling_group.sort_unstable();
            sibling_group.dedup();
            let count = sibling_group.len();
            if count > 0 {
                entry.package.smt_threads_per_core = Some(
                    entry
                        .package
                        .smt_threads_per_core
                        .unwrap_or_default()
                        .max(count),
                );
                if count > 1 && !entry.package.smt_sibling_groups.contains(&sibling_group) {
                    entry.package.smt_sibling_groups.push(sibling_group);
                }
            }
        }

        if let Some(chiplet_id) =
            read_u32(&topology.join("die_id")).or_else(|| read_u32(&topology.join("cluster_id")))
            && !entry.package.chiplet_ids.contains(&chiplet_id)
        {
            entry.package.chiplet_ids.push(chiplet_id);
        }

        let cpufreq = cpu_path.join("cpufreq");
        if let Some(frequency_khz) = read_u64(&cpufreq.join("scaling_cur_freq"))
            && frequency_khz > 0
        {
            entry.frequency_sum_mhz = entry
                .frequency_sum_mhz
                .saturating_add(frequency_khz / 1_000);
            entry.frequency_samples = entry.frequency_samples.saturating_add(1);
        }

        let throttle_dir = cpu_path.join("thermal_throttle");
        // The package counter is repeated on sibling CPUs. Maximum is the
        // deterministic de-duplication rule and also tolerates a partially
        // readable package where only one sibling exposes the file.
        if let Some(value) = read_u64(&throttle_dir.join("package_throttle_count")) {
            entry.package.package_throttle_count = Some(
                entry
                    .package
                    .package_throttle_count
                    .unwrap_or_default()
                    .max(value),
            );
        }
    }

    // Attach NUMA nodes to packages by intersecting each node's cpulist with
    // the package's logical CPU set. Memory totals are read from MemTotal in
    // kB and retained as bytes only after checked conversion.
    let mut nodes = Vec::<(u32, BTreeSet<usize>, Option<u64>, Option<(u64, u64)>)>::new();
    if let Ok(entries) = fs::read_dir(node_root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(node_id) = name
                .strip_prefix("node")
                .and_then(|v| v.parse::<u32>().ok())
            else {
                continue;
            };
            let cpulist = read_trimmed(&entry.path().join("cpulist"))
                .map(|value| {
                    crate::engine::hardware::parse_cpulist(&value)
                        .into_iter()
                        .map(|cpu| cpu as usize)
                        .collect::<BTreeSet<_>>()
                })
                .unwrap_or_default();
            let memory_bytes = read_trimmed(&entry.path().join("meminfo")).and_then(|text| {
                text.lines().find_map(|line| {
                    let mut fields = line.split_whitespace();
                    while let Some(field) = fields.next() {
                        if field == "MemTotal:" {
                            return fields
                                .next()
                                .and_then(|value| value.parse::<u64>().ok())
                                .and_then(|kb| kb.checked_mul(1024));
                        }
                    }
                    None
                })
            });
            let numa_ratio = read_trimmed(&entry.path().join("numastat")).and_then(|text| {
                let mut hit = None;
                let mut miss = None;
                for line in text.lines() {
                    let mut fields = line.split_whitespace();
                    match fields.next() {
                        Some("numa_hit") => hit = fields.next().and_then(|v| v.parse().ok()),
                        Some("numa_miss") => miss = fields.next().and_then(|v| v.parse().ok()),
                        _ => {}
                    }
                }
                Some((hit?, miss?))
            });
            nodes.push((node_id, cpulist, memory_bytes, numa_ratio));
        }
    }

    for accumulator in packages.values_mut() {
        accumulator.package.physical_core_count =
            (!accumulator.physical_cores.is_empty()).then_some(accumulator.physical_cores.len());
        accumulator.package.logical_core_ids.sort_unstable();
        accumulator.package.chiplet_ids.sort_unstable();
        accumulator.package.smt_sibling_groups.sort();
        accumulator.package.frequency_mhz = accumulator
            .frequency_sum_mhz
            .checked_div(accumulator.frequency_samples);
        let logical = accumulator
            .package
            .logical_core_ids
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let mut hit_total = 0_u64;
        let mut miss_total = 0_u64;
        for (node_id, node_cpus, memory_bytes, numa_ratio) in &nodes {
            if logical.is_disjoint(node_cpus) {
                continue;
            }
            accumulator.package.numa_node_ids.push(*node_id);
            if accumulator.package.numa_node_id.is_none() {
                accumulator.package.numa_node_id = Some(*node_id);
            }
            if let Some(memory_bytes) = memory_bytes {
                accumulator.package.local_memory_bytes = Some(
                    accumulator
                        .package
                        .local_memory_bytes
                        .unwrap_or_default()
                        .saturating_add(*memory_bytes),
                );
            }
            if let Some((hit, miss)) = numa_ratio {
                hit_total = hit_total.saturating_add(*hit);
                miss_total = miss_total.saturating_add(*miss);
            }
        }
        accumulator.package.numa_node_ids.sort_unstable();
        if hit_total.saturating_add(miss_total) > 0 {
            accumulator.package.numa_hit_ratio_pct =
                Some(hit_total as f32 / (hit_total + miss_total) as f32 * 100.0);
        }
    }

    packages
        .into_values()
        .map(|accumulator| accumulator.package)
        .collect()
}

/// Read package power limits from the primary Intel RAPL powercap domain.
/// `constraint_0/1` are classified by their kernel-provided names rather than
/// assuming an index, because firmware and the MMIO driver may expose an
/// additional peak constraint before the short-term limit.
pub(in super::super) fn observe_power_limits() -> (Option<f32>, Option<f32>, Option<u64>) {
    observe_power_limits_at(Path::new("/sys/class/powercap"))
}

pub(in super::super) fn observe_power_limits_at(
    powercap_root: &Path,
) -> (Option<f32>, Option<f32>, Option<u64>) {
    let package = ["intel-rapl:0", "intel-rapl-mmio:0"]
        .iter()
        .map(|name| powercap_root.join(name))
        .find(|path| path.is_dir());
    let Some(package) = package else {
        return (None, None, None);
    };
    let Ok(entries) = fs::read_dir(&package) else {
        return (None, None, None);
    };
    let mut constraints = Vec::<(u32, Option<String>, Option<f32>, Option<u64>)>::new();
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let Some(index) = file_name
            .strip_prefix("constraint_")
            .and_then(|name| name.strip_suffix("_power_limit_uw"))
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        let base = format!("constraint_{index}");
        let limit = read_u64(&entry.path()).map(|uw| uw as f32 / 1_000_000.0);
        let name = read_trimmed(&package.join(format!("{base}_name")));
        let window_ms =
            read_u64(&package.join(format!("{base}_time_window_us"))).map(|us| us / 1_000);
        constraints.push((index, name, limit, window_ms));
    }
    constraints.sort_by_key(|(index, ..)| *index);
    // Constraint indices are driver ordering, not power-limit semantics.
    // Unknown or unreadable names must not turn peak power into PL1/PL2.
    let pick = |name: &str| {
        constraints
            .iter()
            .find(|(_, candidate, ..)| candidate.as_deref() == Some(name))
    };
    let pl1 = pick("long_term").and_then(|(_, _, value, _)| *value);
    let pl2 = pick("short_term").and_then(|(_, _, value, _)| *value);
    let tau = pick("long_term").and_then(|(_, _, _, window)| *window);
    (pl1, pl2, tau)
}

/// Read cumulative cpuidle state counters from the first logical CPU exposing
/// a cpuidle directory. A state is retained even when one optional counter is
/// absent, because the state name itself is useful and availability is kept
/// per field.
pub(in super::super) fn observe_cpu_idle_states() -> Vec<CpuIdleState> {
    observe_cpu_idle_states_at(Path::new("/sys/devices/system/cpu"))
}

pub(in super::super) fn observe_cpu_idle_states_at(cpu_root: &Path) -> Vec<CpuIdleState> {
    let mut cpu_dirs = fs::read_dir(cpu_root)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.strip_prefix("cpu")
                .and_then(|value| value.parse::<usize>().ok())
                .map(|cpu| (cpu, entry.path()))
        })
        .collect::<Vec<_>>();
    cpu_dirs.sort_by_key(|(cpu, _)| *cpu);
    for (_, cpu_path) in cpu_dirs {
        let idle_root = cpu_path.join("cpuidle");
        let Ok(entries) = fs::read_dir(idle_root) else {
            continue;
        };
        let mut states = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                name.strip_prefix("state")?
                    .parse::<u32>()
                    .ok()
                    .map(|index| {
                        let path = entry.path();
                        let state_name =
                            read_trimmed(&path.join("name")).unwrap_or_else(|| format!("C{index}"));
                        CpuIdleState {
                            name: state_name,
                            description: read_trimmed(&path.join("desc")),
                            residency_us: read_u64(&path.join("time")),
                            residency_pct: None,
                            usage_count: read_u64(&path.join("usage")),
                            latency_us: read_u64(&path.join("latency")),
                            disabled: read_trimmed(&path.join("disable")).and_then(|value| {
                                match value.as_str() {
                                    "0" => Some(false),
                                    "1" => Some(true),
                                    _ => None,
                                }
                            }),
                        }
                    })
            })
            .collect::<Vec<_>>();
        states.sort_by(|left, right| left.name.cmp(&right.name));
        if !states.is_empty() {
            return states;
        }
    }
    Vec::new()
}

/// Read the kernel's cumulative interrupt distribution. `/proc/interrupts`
/// has a header with one `CPU<n>` column per logical CPU; each IRQ row then
/// carries one counter per column. Malformed rows are skipped, while a source
/// with no parseable rows is an honest absence.
pub(in super::super) fn observe_interrupts(
    logical_cpu_count: usize,
) -> Option<CpuInterruptSnapshot> {
    let text = fs::read_to_string("/proc/interrupts").ok()?;
    parse_interrupts(&text, logical_cpu_count)
}

pub(in super::super) fn parse_interrupts(
    text: &str,
    logical_cpu_count: usize,
) -> Option<CpuInterruptSnapshot> {
    let mut lines = text.lines();
    let header = lines.next()?;
    let header_cpus = header
        .split_whitespace()
        .filter(|field| {
            field
                .strip_prefix("CPU")
                .is_some_and(|value| value.parse::<usize>().is_ok())
        })
        .count();
    let column_count = header_cpus.min(logical_cpu_count);
    if column_count == 0 {
        return None;
    }
    let mut per_cpu = vec![0_u64; column_count];
    let mut observed_rows = 0_usize;
    for line in lines {
        let mut fields = line.split_whitespace();
        let Some(irq) = fields.next() else {
            continue;
        };
        if !irq.ends_with(':') {
            continue;
        }
        let mut row = Vec::with_capacity(column_count);
        for _ in 0..column_count {
            let Some(value) = fields.next().and_then(|value| value.parse::<u64>().ok()) else {
                row.clear();
                break;
            };
            row.push(value);
        }
        if row.len() != column_count {
            continue;
        }
        observed_rows = observed_rows.saturating_add(1);
        for (total, value) in per_cpu.iter_mut().zip(row) {
            *total = total.saturating_add(value);
        }
    }
    (observed_rows > 0).then(|| CpuInterruptSnapshot {
        total: Some(per_cpu.iter().copied().fold(0_u64, u64::saturating_add)),
        per_logical_cpu: per_cpu,
    })
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn read_u32(path: &Path) -> Option<u32> {
    read_trimmed(path)?.parse().ok()
}

fn read_u64(path: &Path) -> Option<u64> {
    read_trimmed(path)?.parse().ok()
}
