//! RAPL package-energy sampling — pure safe `/sys` reads, parameterized by
//! the powercap root so tests run against fixture trees instead of the live
//! (0400 root-owned) counters.
//!
//! The walk samples every top-level `/sys/class/powercap/intel-rapl:N`
//! package TWICE, `sample_ms` apart, and reduces the two energy readings to
//! per-package watts:
//!
//! * plain delta when the counter moved forward;
//! * `max_energy_range_uj - e1 + e2` when the counter wrapped (the Intel RAPL
//!   energy counter is a wrapping counter with an advertised range);
//! * a typed SKIP (package dropped) when the counter wrapped but no range is
//!   advertised — the true delta is unknowable and is never guessed as zero;
//! * a typed SKIP for any non-finite watt result (e.g. a zero window).
//!
//! Honesty rules: a missing powercap root AND a present-but-empty one are
//! both `no_rapl` (there is no RAPL on this host); any permission denial is
//! `permission_denied` (the escalatable gap this helper exists to cross);
//! every other read/parse failure is a typed error — never a fabricated
//! zero-watt package.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;
use std::thread;
use std::time::Duration;

use crate::json::ErrorKindJson;

/// Directory-name prefix of a top-level RAPL package (`intel-rapl:N`).
const PACKAGE_PREFIX: &str = "intel-rapl:";

/// One package's counter snapshot at a point in time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageState {
    /// The numeric suffix `N` of `intel-rapl:N` (sort/identity key).
    pub index: u32,
    /// The package's sysfs `name` content, trimmed.
    pub name: String,
    /// Advertised counter wraparound range in microjoules (0 = unknown).
    pub max_energy_range_uj: u64,
    /// Cumulative energy consumed in microjoules.
    pub energy_uj: u64,
}

/// One package's reduced power result. `index` drives sorting; it is not part
/// of the JSON contract.
#[derive(Debug, Clone, PartialEq)]
pub struct PackagePower {
    pub index: u32,
    pub name: String,
    /// Average watts over the sample window (finite, >= 0.0).
    pub power_w: f32,
    /// The microjoule delta the watts were derived from.
    pub energy_delta_uj: u64,
}

/// The sampling pass's terminal result: per-package power, or a typed error.
pub enum ReadOutcome {
    Packages { packages: Vec<PackagePower> },
    Error(ReadError),
}

/// A typed sampling failure, already carrying the contract error kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadError {
    pub kind: ErrorKindJson,
    pub detail: String,
}

/// Sample every package twice `sample_ms` apart (real sleep) and compute the
/// per-package watts. The main path entry point.
pub fn sample_packages(powercap_root: &Path, sample_ms: u64) -> ReadOutcome {
    sample_packages_with_pause(powercap_root, sample_ms, &mut || {
        thread::sleep(Duration::from_millis(sample_ms));
    })
}

/// The orchestrator with the wait injected: `pause` runs between the two
/// counter reads. Production passes a sleeping closure; tests pass a no-op
/// closure that rewrites fixture counter files, keeping the two-read contract
/// deterministic.
pub fn sample_packages_with_pause(
    powercap_root: &Path,
    sample_ms: u64,
    pause: &mut dyn FnMut(),
) -> ReadOutcome {
    let first = match read_package_states(powercap_root) {
        Ok(states) => states,
        Err(error) => return ReadOutcome::Error(error),
    };
    pause();
    let second = match read_package_states(powercap_root) {
        Ok(states) => states,
        Err(error) => return ReadOutcome::Error(error),
    };
    // Pair the two snapshots by package index (hotplug cannot change the
    // RAPL package set, but an honest miss skips a package rather than
    // mispairing counters).
    let second_by_index: HashMap<u32, &PackageState> =
        second.iter().map(|state| (state.index, state)).collect();
    let mut packages = Vec::new();
    for before in &first {
        if let Some(after) = second_by_index.get(&before.index)
            && let Some(power) = compute_package_power(before, after, sample_ms)
        {
            packages.push(power);
        }
    }
    packages.sort_by_key(|package| package.index);
    ReadOutcome::Packages { packages }
}

/// Read one snapshot of every top-level package under `powercap_root`.
/// Returns the packages sorted by index; an empty set is a typed `no_rapl`.
pub fn read_package_states(powercap_root: &Path) -> Result<Vec<PackageState>, ReadError> {
    let entries =
        fs::read_dir(powercap_root).map_err(|error| classify_root_error(&error, powercap_root))?;
    let mut states = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| ReadError {
            kind: ErrorKindJson::OpenFailed,
            detail: format!("iterating {}: {error}", powercap_root.display()),
        })?;
        let Some(dir_name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(index) = package_index(&dir_name) else {
            continue;
        };
        let dir = entry.path();
        let name = read_name(&dir)?;
        let max_energy_range_uj = read_counter(&dir, "max_energy_range_uj")?;
        let energy_uj = read_counter(&dir, "energy_uj")?;
        states.push(PackageState {
            index,
            name,
            max_energy_range_uj,
            energy_uj,
        });
    }
    if states.is_empty() {
        return Err(ReadError {
            kind: ErrorKindJson::NoRapl,
            detail: format!(
                "no top-level intel-rapl:* packages under {}",
                powercap_root.display()
            ),
        });
    }
    states.sort_by_key(|state| state.index);
    Ok(states)
}

/// Reduce two snapshots of one package into average watts over `sample_ms`.
/// `None` (skip the package) when the delta is unknowable — a wrapped counter
/// with no advertised range — or the watt result is not finite. See the
/// module docs for the wraparound rule.
pub fn compute_package_power(
    first: &PackageState,
    second: &PackageState,
    sample_ms: u64,
) -> Option<PackagePower> {
    let energy_delta_uj = if second.energy_uj >= first.energy_uj {
        second.energy_uj - first.energy_uj
    } else if first.max_energy_range_uj > 0 {
        // Wrapped: distance from the first reading to the top of the
        // advertised range, plus the second reading.
        first.max_energy_range_uj.saturating_sub(first.energy_uj) + second.energy_uj
    } else {
        return None;
    };
    let power_w = energy_delta_uj as f32 / (sample_ms as f32 / 1000.0) / 1_000_000.0;
    if !power_w.is_finite() {
        return None;
    }
    Some(PackagePower {
        index: first.index,
        name: first.name.clone(),
        power_w,
        energy_delta_uj,
    })
}

/// The package index of a top-level `intel-rapl:N` directory name — `None`
/// for sub-domain directories (`intel-rapl:N:M`, the core/uncore/dram
/// children) and anything that is not a package.
fn package_index(dir_name: &str) -> Option<u32> {
    let suffix = dir_name.strip_prefix(PACKAGE_PREFIX)?;
    if suffix.contains(':') {
        return None;
    }
    suffix.parse().ok()
}

/// Read a package's `name` file, trimmed.
fn read_name(dir: &Path) -> Result<String, ReadError> {
    fs::read_to_string(dir.join("name"))
        .map(|text| text.trim().to_owned())
        .map_err(|error| classify_file_error(&error, "name", dir))
}

/// Read and parse one u64 counter file of a package.
fn read_counter(dir: &Path, file: &str) -> Result<u64, ReadError> {
    let path = dir.join(file);
    let text = fs::read_to_string(&path).map_err(|error| classify_file_error(&error, file, dir))?;
    text.trim().parse().map_err(|error| ReadError {
        kind: ErrorKindJson::ReadFailed,
        detail: format!("parse {}: {error}", path.display()),
    })
}

/// Classify a powercap-root open failure: missing root → `no_rapl`;
/// `EACCES`/`EPERM` → `permission_denied`; anything else → `open_failed`.
fn classify_root_error(error: &io::Error, root: &Path) -> ReadError {
    let kind = match error.kind() {
        io::ErrorKind::NotFound => ErrorKindJson::NoRapl,
        io::ErrorKind::PermissionDenied => ErrorKindJson::PermissionDenied,
        _ => ErrorKindJson::OpenFailed,
    };
    ReadError {
        kind,
        detail: format!("open {}: {error}", root.display()),
    }
}

/// Classify a package-file read failure: `EACCES`/`EPERM` →
/// `permission_denied` (the escalatable denial — `energy_uj` is 0400
/// root-owned); anything else (missing file, I/O error) → `read_failed`.
fn classify_file_error(error: &io::Error, file: &str, dir: &Path) -> ReadError {
    let kind = if error.kind() == io::ErrorKind::PermissionDenied {
        ErrorKindJson::PermissionDenied
    } else {
        ErrorKindJson::ReadFailed
    };
    ReadError {
        kind,
        detail: format!("read {}/{}: {error}", dir.display(), file),
    }
}

#[cfg(test)]
#[path = "../tests/headless/rapl_helper_rapl_read.rs"]
mod tests;
