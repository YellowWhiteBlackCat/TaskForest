//! Linux sysfs readers for CPU topology, cache sizing, and Intel hybrid
//! core classification, plus the shared `read_sysfs_string`/`read_sysfs_u64`
//! foundation readers.
use std::fs;

/// Helper functions to read Linux sysfs hardware details
pub fn read_sysfs_string(path: &str) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

pub fn read_sysfs_u64(path: &str) -> Option<u64> {
    read_sysfs_string(path).and_then(|s| s.parse::<u64>().ok())
}

/// Total L1, L2, L3 cache sizes in Kilobytes, summed over every DISTINCT cache
/// instance on the package. A missing level remains `None`; zero is never used
/// as an absence sentinel. We dedupe by `(level, id, type)`.
///
/// The cache `type` MUST be part of the key: on a core with split L1 caches the
/// L1-data and L1-instruction entries share the same `(level, id)` — without the
/// type, dedupe collapses them and only one is counted, under-reporting L1 by
/// exactly the L1i size (on Arrow Lake-H: 576 KiB reported instead of the true
/// 1600 KiB = 576 KiB L1d + 1024 KiB L1i). Shared caches (L2/L3) are still
/// counted exactly once: every sharer reports the same `(level, id, type)`.
#[cfg(target_os = "linux")]
pub fn detect_cpu_cache() -> (Option<u64>, Option<u64>, Option<u64>) {
    let mut totals: [Option<u64>; 3] = [None; 3]; // index level-1
    let mut seen: std::collections::HashSet<(String, String, String)> =
        std::collections::HashSet::new();
    for cpu in 0..num_cpu_dirs() {
        for idx in 0..=4 {
            let base = format!("/sys/devices/system/cpu/cpu{cpu}/cache/index{idx}");
            let (Some(level), Some(size), Some(id), Some(typ)) = (
                read_sysfs_string(&format!("{base}/level")),
                read_sysfs_string(&format!("{base}/size")),
                read_sysfs_string(&format!("{base}/id")),
                read_sysfs_string(&format!("{base}/type")),
            ) else {
                continue;
            };
            let Ok(n) = level.parse::<usize>() else {
                continue;
            };
            let size_kb = parse_size_to_kb(&size);
            if (1..=3).contains(&n) && size_kb > 0 && seen.insert((level, id, typ)) {
                totals[n - 1] = Some(totals[n - 1].unwrap_or(0).saturating_add(size_kb));
            }
        }
    }
    (totals[0], totals[1], totals[2])
}

/// Parse a kernel CPU list like "0-3,5,7-9" into a vec of CPU ids.
///
/// Tolerates surrounding whitespace on the whole string and on each
/// comma-separated token. Inverted ranges (`"3-1"`) and unparseable tokens are
/// silently skipped (the kernel never emits them, but defensive parsing keeps
/// a malformed node from panicking the collector). Returns an empty vec for an
/// empty/blank input.
///
/// The total output is capped at [`MAX_TRACKED_LOGICAL_CPUS`]: a malformed
/// sysfs range such as `0-4294967295` truncates at the shared ceiling instead
/// of materializing billions of ids. Downstream `i < ncpu` bounds checks keep
/// their semantics — real topologies never reach the cap.
pub fn parse_cpulist(s: &str) -> Vec<u32> {
    use taskmanager_core::MAX_TRACKED_LOGICAL_CPUS;

    let mut out = Vec::new();
    for part in s.trim().split(',') {
        if out.len() >= MAX_TRACKED_LOGICAL_CPUS {
            break;
        }
        let part = part.trim();
        if let Some((a, b)) = part.split_once('-') {
            if let (Ok(a), Ok(b)) = (a.trim().parse::<u32>(), b.trim().parse::<u32>())
                && a <= b
            {
                for id in a..=b {
                    if out.len() >= MAX_TRACKED_LOGICAL_CPUS {
                        break;
                    }
                    out.push(id);
                }
            }
        } else if let Ok(n) = part.parse::<u32>() {
            out.push(n);
        }
    }
    out
}

#[cfg(target_os = "linux")]
fn cpu_max_freq_khz(cpu: u32) -> Option<u64> {
    read_sysfs_u64(&format!(
        "/sys/devices/system/cpu/cpu{cpu}/cpufreq/cpuinfo_max_freq"
    ))
}

/// Per-logical-CPU type on a hybrid part. cpu_core/cpus → Performance;
/// cpu_atom/cpus split by cpuinfo_max_freq (highest = Efficient, strictly-lower =
/// LowPower LP-E). Missing classification nodes remain `Unknown`.
#[cfg(target_os = "linux")]
pub fn detect_cpu_types(ncpu: usize) -> Vec<super::CpuType> {
    let mut out = vec![super::CpuType::Unknown; ncpu];
    let core = read_sysfs_string("/sys/devices/cpu_core/cpus");
    let atom = read_sysfs_string("/sys/devices/cpu_atom/cpus");
    if let Some(atom) = atom {
        for cpu in parse_cpulist(&atom) {
            let i = cpu as usize;
            if i >= ncpu {
                continue;
            }
            let is_lp = match (cpu_max_freq_khz(cpu), atom_max_freq(&atom)) {
                (Some(f), Some(mf)) => f != mf,
                _ => false,
            };
            out[i] = if is_lp {
                super::CpuType::LowPower
            } else {
                super::CpuType::Efficient
            };
        }
    }
    if let Some(core) = core {
        for cpu in parse_cpulist(&core) {
            let index = cpu as usize;
            if index < ncpu {
                out[index] = super::CpuType::Performance;
            }
        }
    }
    out
}

#[cfg(target_os = "linux")]
fn atom_max_freq(atom_cpulist: &str) -> Option<u64> {
    parse_cpulist(atom_cpulist)
        .iter()
        .filter_map(|&c| cpu_max_freq_khz(c))
        .max()
}

/// Detect the heterogeneous core breakdown of an Intel hybrid CPU
/// (P-cores / E-cores / LP-E-cores), e.g. Arrow Lake-H. Returns
/// `(p_cores, e_cores, lp_cores)`.
///
/// Strategy:
/// - `/sys/devices/cpu_core/cpus` → P-cores (performance, e.g. Lion Cove).
/// - `/sys/devices/cpu_atom/cpus` → E-cores + LP-E-cores grouped together by
///   the kernel. We split the atom bucket by `cpuinfo_max_freq`: the cluster
///   with the highest max frequency is the on-package E-cores; any strictly
///   lower-frequency cluster is the LP-E-cores (separate low-power module —
///   no L3, lower cpu_capacity, e.g. 3.3 GHz vs 3.7 GHz on this host).
/// - Missing cpu_core/cpu_atom nodes: keep the classification unknown.
///
/// On this host (Intel Core Ultra X7 358H / Arrow Lake-H) it returns (4, 8, 4):
/// P = cpu0-3, E = cpu4-11 (3.7 GHz), LP-E = cpu12-15 (3.3 GHz).
#[cfg(target_os = "linux")]
pub fn detect_cpu_core_breakdown() -> (u16, u16, u16) {
    let max_freq_khz = |cpu: u32| -> Option<u64> {
        read_sysfs_u64(&format!(
            "/sys/devices/system/cpu/cpu{cpu}/cpufreq/cpuinfo_max_freq"
        ))
    };

    let core = read_sysfs_string("/sys/devices/cpu_core/cpus");
    let atom = read_sysfs_string("/sys/devices/cpu_atom/cpus");

    match (core, atom) {
        (Some(c), Some(a)) => {
            let p = parse_cpulist(&c).len() as u16;
            let atom_cpus = parse_cpulist(&a);
            let freqs: Vec<Option<u64>> = atom_cpus.iter().map(|&cpu| max_freq_khz(cpu)).collect();
            let max_f = freqs.iter().copied().flatten().max();
            let (mut e, mut lp) = (0u16, 0u16);
            for f in &freqs {
                match (f, max_f) {
                    (Some(fv), Some(mf)) if *fv == mf => e += 1,
                    (Some(_), Some(_)) => lp += 1, // strictly lower max freq → LP-E
                    _ => e += 1,                   // freq unknown → assume regular E-core
                }
            }
            (p, e, lp)
        }
        (Some(c), None) => (parse_cpulist(&c).len() as u16, 0, 0),
        (None, Some(a)) => (0, parse_cpulist(&a).len() as u16, 0),
        (None, None) => (0, 0, 0),
    }
}

/// Number of logical CPUs present on the system (count of /sys/devices/system/cpu/cpuN).
#[cfg(target_os = "linux")]
pub fn num_cpu_dirs() -> usize {
    std::fs::read_dir("/sys/devices/system/cpu")
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().to_str().map(str::to_owned))
                .filter_map(|name| {
                    name.strip_prefix("cpu")
                        .and_then(|rest| rest.parse::<usize>().ok())
                })
                .count()
        })
        .unwrap_or(1)
}

/// Parse a kernel cache `size` string ("256K", "16M", "2G", or a raw byte
/// count) into Kilobytes. Unparseable input returns 0. The suffix is matched
/// case-insensitively (lowercased first); a bare integer is treated as bytes
/// and divided by 1024 (matching how `/sys/.../cache/indexN/size` falls back
/// to a byte count on some drivers).
pub fn parse_size_to_kb(s: &str) -> u64 {
    let s = s.trim().to_lowercase();
    if s.ends_with('k') {
        s.trim_end_matches('k').parse::<u64>().unwrap_or(0)
    } else if s.ends_with('m') {
        s.trim_end_matches('m').parse::<u64>().unwrap_or(0) * 1024
    } else if s.ends_with('g') {
        s.trim_end_matches('g').parse::<u64>().unwrap_or(0) * 1024 * 1024
    } else {
        s.parse::<u64>().unwrap_or(0) / 1024
    }
}

// ── macOS / Windows stubs ────────────────────────────────────────────────────
// Every reader above is bound to Linux `/sys/devices/system/cpu/...`. Off Linux
// they return the type's
// empty/None default so the collector + `HardwareInfo::detect` stay
// cross-platform (sysinfo still supplies brand / usage / core count).
// `read_sysfs_string` / `read_sysfs_u64` (foundation fs readers) + the pure
// `parse_cpulist` / `parse_size_to_kb` helpers stay cross-platform above.
// This crate intentionally owns only the Linux provider.

#[cfg(not(target_os = "linux"))]
pub fn detect_cpu_cache() -> (Option<u64>, Option<u64>, Option<u64>) {
    (None, None, None)
}

#[cfg(not(target_os = "linux"))]
pub fn detect_cpu_types(_ncpu: usize) -> Vec<super::CpuType> {
    Vec::new()
}

#[cfg(not(target_os = "linux"))]
pub fn detect_cpu_core_breakdown() -> (u16, u16, u16) {
    (0, 0, 0)
}

#[cfg(not(target_os = "linux"))]
pub fn num_cpu_dirs() -> usize {
    1
}

#[cfg(test)]
#[path = "../../../tests/headless/linux_engine_hardware_cpu_tests.rs"]
mod tests;
