//! Per-container aggregated CPU + memory rollup from cgroup-v2.
//!
//! Container *detection* (per-process [`IsolationKind`] + container id) lives
//! in `super::isolation`. This module owns the *rollup*: it discovers
//! container cgroups by walking the unified cgroup tree, then aggregates
//! `cpu.stat` (`usage_usec`, cumulative microseconds) and `memory.current`
//! (bytes) per cgroup, plus the member pids from `cgroup.procs`.
//!
//! CPU% is derived as a single-core-equivalent rate (mirrors the per-process
//! CPU facet: a container burning two whole cores reports `200.0`). The
//! delta-over-elapsed conversion reuses the established
//! `crate::engine::process::rates`-style baseline contract: the first sample
//! for a cgroup is a typed gap, a counter rollback (cgroup recreated) resets
//! the baseline, and a vanished cgroup between snapshot phases is typed rather
//! than panicked.
//!
//! Honesty contract: a cgroup-v1 host (no `cgroup.controllers` at the unified
//! mount) yields a typed `DeviceStatus::Unsupported` rollup; a permission
//! failure on the mount yields `DeviceStatus::PermissionDenied`; a healthy
//! host with no container cgroups yields an empty *healthy* list. Per-field
//! read failures land as typed [`ScalarObservation::unavailable`], never a
//! fabricated zero.

use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use taskmanager_core::{
    ContainerRollup, ContainerSummary, DeviceStatus, FailureKind, IsolationKind, ScalarObservation,
};

use super::isolation;
use super::state_for_status;

/// Maximum cgroup-tree depth walked from the unified root. Container runtimes
/// place workloads within a handful of levels (`/docker/<id>`,
/// `/system.slice/docker-<id>.scope`, `/kubepods/pod<id>/<id>`), so eight
/// levels reaches every realistic placement while bounding a hostile or
/// pathological tree.
const MAX_WALK_DEPTH: usize = 8;
/// Safety cap on the total directories visited during one discovery pass, so a
/// runaway cgroup hierarchy cannot stall a refresh. Container hosts rarely
/// exceed a few hundred cgroups.
const MAX_VISITED_DIRS: usize = 4096;

/// Parse the cumulative `usage_usec` field out of a cgroup-v2 `cpu.stat` file.
///
/// `cpu.stat` is a space-separated `key value` table whose first line is
/// normally `usage_usec <micros>`. Only `usage_usec` is needed for the CPU%
/// delta; unknown keys are ignored so future kernels adding rows do not break
/// the parse.
pub fn parse_cpu_stat_usage_usec(text: &str) -> Option<u64> {
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        if parts.next() == Some("usage_usec")
            && let Some(value) = parts.next()
            && parts.next().is_none()
        {
            return value.parse().ok();
        }
    }
    None
}

/// Parse a `memory.current` payload (a single unsigned byte count).
pub fn parse_memory_current(text: &str) -> Option<u64> {
    text.trim().parse().ok()
}

/// Parse a cgroup-v2 `cgroup.procs` payload (one pid per line). Unparseable
/// lines are skipped rather than failing the whole membership list.
pub fn parse_cgroup_procs(text: &str) -> Vec<u32> {
    text.lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect()
}

/// Classify a cgroup relative path as a container runtime placement. Returns
/// the inferred runtime family when the path matches a known signature, `None`
/// for a plain non-container cgroup (a systemd service scope without a
/// container marker is intentionally NOT treated as a container).
///
/// Delegates to `isolation::detect_isolation` on the path text so the
/// rollup and the per-process detection share one signature vocabulary. The
/// runtime *engine's own* daemon/service cgroup (`docker.service`,
/// `containerd.service`, ...) is explicitly excluded first — it is the engine,
/// not a workload container, and the shared substring signature (`docker`,
/// `podman`, ...) must not roll it up as a phantom container.
pub fn classify_container_cgroup(relative_path: &str) -> Option<IsolationKind> {
    if is_runtime_daemon_cgroup(relative_path) {
        return None;
    }
    isolation::detect_isolation(relative_path, b"", false).0
}

/// Leaf cgroup names that belong to the container *runtime engine itself*
/// rather than to any workload. Rolling these up would phantom a container that
/// is really the host-side daemon. Sourced from the standard systemd unit name
/// each runtime ships under `system.slice` (`docker.service`,
/// `containerd.service`, ...).
const RUNTIME_DAEMON_UNIT_LEAVES: &[&str] = &[
    "docker.service",
    "dockerd.service",
    "containerd.service",
    "podman.service",
    "lxcfs.service",
];

/// True when `relative_path` is the runtime engine's own daemon cgroup (for
/// example `/system.slice/docker.service`). Such a cgroup is the engine, not a
/// container, and must never be classified as a workload — see
/// [`classify_container_cgroup`].
fn is_runtime_daemon_cgroup(relative_path: &str) -> bool {
    let leaf = relative_path.rsplit('/').next().unwrap_or(relative_path);
    RUNTIME_DAEMON_UNIT_LEAVES.contains(&leaf)
}

/// A readable cgroup field's typed outcome: either a value or the failure that
/// prevented reading it.
enum FieldRead<T> {
    Value(T),
    Failed(FailureKind),
}

fn read_u64_field(path: &Path) -> FieldRead<u64> {
    match fs::read_to_string(path) {
        Ok(text) => match text.trim().parse() {
            Ok(value) => FieldRead::Value(value),
            Err(_) => FieldRead::Failed(FailureKind::ProviderFault),
        },
        Err(error) => FieldRead::Failed(field_io_failure(error.kind())),
    }
}

fn read_cpu_stat_usage(path: &Path) -> FieldRead<u64> {
    match fs::read_to_string(path) {
        Ok(text) => {
            if let Some(value) = parse_cpu_stat_usage_usec(&text) {
                FieldRead::Value(value)
            } else {
                FieldRead::Failed(FailureKind::ProviderFault)
            }
        }
        Err(error) => FieldRead::Failed(field_io_failure(error.kind())),
    }
}

fn read_cgroup_procs(path: &Path) -> FieldRead<Vec<u32>> {
    match fs::read_to_string(path) {
        Ok(text) => FieldRead::Value(parse_cgroup_procs(&text)),
        Err(error) => FieldRead::Failed(field_io_failure(error.kind())),
    }
}

fn field_io_failure(kind: ErrorKind) -> FailureKind {
    match kind {
        ErrorKind::NotFound => FailureKind::IdentityChanged,
        ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        ErrorKind::Interrupted | ErrorKind::WouldBlock | ErrorKind::TimedOut => {
            FailureKind::TemporarilyUnavailable
        }
        _ => FailureKind::ProviderFault,
    }
}

#[derive(Debug, Clone, Copy)]
struct CounterPoint {
    usage_usec: u64,
    observed_at_ms: u64,
}

/// Per-cgroup CPU delta→rate state. Mirrors the baseline contract of
/// `crate::engine::process::rates::ProcessRateState`: the first observation
/// for a cgroup is a typed gap, a counter rollback resets the baseline, and a
/// missing current sample clears only that cgroup's baseline.
#[derive(Debug, Default)]
pub struct ContainerCpuRateTracker {
    baselines: HashMap<String, CounterPoint>,
}

impl ContainerCpuRateTracker {
    /// Derive a single-core-equivalent CPU% for one cgroup. `usage_usec` is the
    /// cumulative `cpu.stat` value; `observed_at_ms` is the collection wall
    /// clock. Returns a typed observation: the first sample, a zero-elapsed
    /// sample, or a counter rollback never fabricate a percentage.
    pub fn percentage(
        &mut self,
        cgroup_path: &str,
        usage_usec: u64,
        observed_at_ms: u64,
    ) -> ScalarObservation<f32> {
        let baseline = self
            .baselines
            .entry(cgroup_path.to_owned())
            .or_insert(CounterPoint {
                usage_usec,
                observed_at_ms,
            });
        if baseline.observed_at_ms == observed_at_ms {
            return ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable);
        }
        let elapsed_ms = match observed_at_ms.checked_sub(baseline.observed_at_ms) {
            Some(elapsed) => elapsed,
            None => {
                *baseline = CounterPoint {
                    usage_usec,
                    observed_at_ms,
                };
                return ScalarObservation::unavailable(FailureKind::IdentityChanged);
            }
        };
        let delta = match usage_usec.checked_sub(baseline.usage_usec) {
            Some(delta) => delta,
            None => {
                *baseline = CounterPoint {
                    usage_usec,
                    observed_at_ms,
                };
                return ScalarObservation::unavailable(FailureKind::IdentityChanged);
            }
        };
        *baseline = CounterPoint {
            usage_usec,
            observed_at_ms,
        };
        // pct = (delta_secs / elapsed_secs) * 100
        //     = (delta_usec / 1_000_000) / (elapsed_ms / 1_000) * 100
        //     = delta_usec / (elapsed_ms * 10)
        let percentage = (delta as f64 * 100.0) / (elapsed_ms as f64 * 1000.0);
        let percentage = percentage as f32;
        if percentage.is_finite() && percentage >= 0.0 {
            ScalarObservation::available(percentage, observed_at_ms)
        } else {
            ScalarObservation::unavailable(FailureKind::ProviderFault)
        }
    }

    /// Drop baselines for cgroups absent from the latest discovery so the
    /// tracker cannot retain a stale entry for a destroyed container.
    pub fn retain_paths(&mut self, present: &std::collections::HashSet<String>) {
        self.baselines.retain(|path, _| present.contains(path));
    }

    /// Reset all per-cgroup state (used on a typed-unavailable rollup).
    pub fn clear(&mut self) {
        self.baselines.clear();
    }
}

/// Filesystem-bound cgroup-v2 container rollup collector. Owns the per-cgroup
/// CPU baselines across samples.
#[derive(Debug, Default)]
pub struct ContainerRollupCollector {
    rates: ContainerCpuRateTracker,
}

impl ContainerRollupCollector {
    /// Collect from the default live roots (`/proc` is unused here; the rollup
    /// is driven entirely by the unified cgroup mount).
    #[cfg(target_os = "linux")]
    pub fn collect(&mut self, now_ms: u64) -> ContainerRollup {
        self.collect_from_root(Path::new("/sys/fs/cgroup"), now_ms)
    }

    #[cfg(not(target_os = "linux"))]
    pub fn collect(&mut self, now_ms: u64) -> ContainerRollup {
        self.rates.clear();
        ContainerRollup::unavailable(state_for_status(DeviceStatus::Unsupported, now_ms))
    }

    /// Collect the rollup from an explicit unified-cgroup mount root. This is
    /// the integration entry point used by tests with a fixture tree and by the
    /// production collector (via [`Self::collect`]).
    pub fn collect_from_root(&mut self, cgroup_root: &Path, now_ms: u64) -> ContainerRollup {
        self.collect_from_root_bounded(cgroup_root, now_ms, MAX_VISITED_DIRS as u32)
    }

    /// Bounded variant of [`Self::collect_from_root`]: production passes the
    /// default [`MAX_VISITED_DIRS`] cap; tests pass a tiny cap to exercise the
    /// discovery-capped branch without materializing 4096 fixture directories.
    fn collect_from_root_bounded(
        &mut self,
        cgroup_root: &Path,
        now_ms: u64,
        max_visited: u32,
    ) -> ContainerRollup {
        // v2 detection: the unified mount exposes a `cgroup.controllers` file.
        // A v1 host (or a missing mount) is typed Unsupported rather than
        // silently treated as "no containers".
        match fs::metadata(cgroup_root.join("cgroup.controllers")) {
            Ok(_) => {}
            Err(error) => {
                self.rates.clear();
                let status = match error.kind() {
                    ErrorKind::PermissionDenied => DeviceStatus::PermissionDenied,
                    ErrorKind::NotFound => DeviceStatus::Unsupported,
                    _ => DeviceStatus::Stale,
                };
                return ContainerRollup::unavailable(state_for_status(status, now_ms));
            }
        }

        let discovered = discover_container_cgroups_bounded(cgroup_root, max_visited);
        let mut present = std::collections::HashSet::new();
        for (relative, _) in &discovered.found {
            present.insert(relative.clone());
        }
        self.rates.retain_paths(&present);

        let built: Vec<(ContainerSummary, bool)> = discovered
            .found
            .into_iter()
            .map(|(relative, abs)| {
                build_summary(cgroup_root, &relative, abs, &mut self.rates, now_ms)
            })
            .collect();
        let membership_partial = built.iter().any(|&(_, failed)| failed);
        let mut containers: Vec<ContainerSummary> =
            built.into_iter().map(|(summary, _)| summary).collect();
        // Descending CPU%: a container with a current reading ranks above one
        // whose CPU% is still a typed gap/unavailable (None sorts as -1.0, so
        // it never beats a real reading); ties break on name for a stable order.
        containers.sort_by(|left, right| {
            cpu_sort_key(right)
                .total_cmp(&cpu_sort_key(left))
                .then_with(|| left.name.cmp(&right.name))
        });

        // Honesty: a discovery that hit the visited cap (real container cgroups
        // beyond the cap remain undiscovered) or that could not enumerate a
        // discovered container's membership (`cgroup.procs` unreadable) is
        // incomplete — the rollup reports Stale/partial rather than Healthy so
        // the page never overclaims a complete, healthy view. A clean scan with
        // no containers remains a healthy empty list.
        let status = if discovered.capped || membership_partial {
            DeviceStatus::Stale
        } else {
            DeviceStatus::Healthy
        };
        ContainerRollup {
            state: state_for_status(status, now_ms),
            containers,
        }
    }
}

/// Sort key for the descending-CPU% container ordering. A current reading maps
/// to its value; an unavailable/gap reading maps to `-1.0` (CPU% is never
/// negative) so containers with real data always rank above typed gaps.
fn cpu_sort_key(container: &ContainerSummary) -> f64 {
    container
        .cpu_percentage
        .current_value()
        .map(|value| f64::from(*value))
        .unwrap_or(-1.0)
}

/// Build one [`ContainerSummary`] from a discovered cgroup directory. Per-field
/// read failures are typed individually so a vanished `cpu.stat` (the cgroup
/// died between discovery and field read) downgrades only CPU%, never the
/// whole row. Returns the summary plus a flag that is `true` when
/// `cgroup.procs` could not be read (membership unavailable): the caller marks
/// the rollup partial rather than letting a read failure collapse into a silent
/// empty member list indistinguishable from a genuine zero-member container.
fn build_summary(
    _root: &Path,
    relative: &str,
    abs: PathBuf,
    rates: &mut ContainerCpuRateTracker,
    now_ms: u64,
) -> (ContainerSummary, bool) {
    let (runtime, container_id) = isolation::detect_isolation(relative, b"", false);
    let name = container_id
        .clone()
        .or_else(|| {
            Path::new(relative)
                .file_name()
                .and_then(|segment| segment.to_str())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| relative.to_owned());
    let cpu_percentage = match read_cpu_stat_usage(&abs.join("cpu.stat")) {
        FieldRead::Value(usage) => rates.percentage(relative, usage, now_ms),
        FieldRead::Failed(failure) => ScalarObservation::unavailable(failure),
    };
    let memory_bytes = match read_u64_field(&abs.join("memory.current")) {
        FieldRead::Value(bytes) => ScalarObservation::available(bytes, now_ms),
        FieldRead::Failed(failure) => ScalarObservation::unavailable(failure),
    };
    // Membership (`cgroup.procs`) read failure must never collapse into a silent
    // empty Vec that is indistinguishable from a genuine zero-member container:
    // keep the empty list (the model's documented "membership unavailable" form)
    // and surface the failure as a typed signal so the rollup can downgrade to
    // Stale/partial.
    let (member_pids, procs_read_failed) = match read_cgroup_procs(&abs.join("cgroup.procs")) {
        FieldRead::Value(pids) => (pids, false),
        FieldRead::Failed(_) => (Vec::new(), true),
    };
    let summary = ContainerSummary {
        id: relative.to_owned(),
        name,
        runtime,
        cgroup_path: relative.to_owned(),
        cpu_percentage,
        memory_bytes,
        member_pids,
    };
    (summary, procs_read_failed)
}

/// Result of a cgroup-tree discovery pass. `capped` is `true` when the BFS
/// visited-cap was reached before the whole tree could be walked — the caller
/// downgrades the rollup to a typed Stale/partial state so a capped (incomplete)
/// discovery never binds silently while the page reports Healthy.
#[derive(Debug)]
struct ContainerDiscovery {
    /// `(relative_path, abs_path)` for each classified container cgroup.
    found: Vec<(String, PathBuf)>,
    /// Whether the visited-cap fired before the tree was fully walked.
    capped: bool,
}

/// Walk the unified cgroup tree (bounded BFS) and collect every directory whose
/// relative path classifies as a container. Returns `(relative_path, abs_path)`
/// pairs via [`ContainerDiscovery::found`]. Discovery is tolerant: an unreadable
/// subtree is skipped rather than failing the whole pass (its containers simply
/// remain undiscovered, which the field-read phase reports as typed-unavailable
/// at the row level only when the dir itself is readable).
///
/// `max_visited` bounds the BFS so a runaway cgroup hierarchy cannot stall a
/// refresh; production passes [`MAX_VISITED_DIRS`], tests pass a small value to
/// drive the capped branch without materializing 4096 fixture directories. When
/// the cap binds, [`ContainerDiscovery::capped`] is `true` so the rollup can be
/// honest about incompleteness instead of silently dropping the tail of the tree.
fn discover_container_cgroups_bounded(root: &Path, max_visited: u32) -> ContainerDiscovery {
    let mut found = Vec::new();
    let mut visited = 0u32;
    let mut capped = false;
    // Stack of (absolute dir, relative path, depth).
    let mut stack: Vec<(PathBuf, String, usize)> = vec![(root.to_path_buf(), String::new(), 0)];
    while let Some((dir, relative, depth)) = stack.pop() {
        visited = visited.saturating_add(1);
        if visited > max_visited {
            capped = true;
            break;
        }
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if !file_type.is_dir() {
                continue;
            }
            let child_abs = entry.path();
            let segment = match entry.file_name().to_str() {
                Some(name) => name.to_owned(),
                None => continue,
            };
            let child_relative = if relative.is_empty() {
                format!("/{segment}")
            } else {
                format!("{relative}/{segment}")
            };
            if classify_container_cgroup(&child_relative).is_some() {
                found.push((child_relative.clone(), child_abs.clone()));
                // A container cgroup may still nest sub-cgroups (rare); keep
                // descending so a deeper container is not missed, subject to
                // the global depth + visited caps.
            }
            if depth + 1 < MAX_WALK_DEPTH {
                stack.push((child_abs, child_relative, depth + 1));
            }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found.dedup_by(|a, b| a.0 == b.0);
    let found = drop_ancestor_scopes(found);
    ContainerDiscovery { found, capped }
}

/// Keep only the deepest classified cgroups. The runtime's grouping scope
/// (for example `/docker` or `/kubepods.slice`) matches the same substring
/// classifier as its child containers, but only a leaf cgroup is a real
/// per-container accounting domain — rolling up the parent would double-count
/// every child. A classified cgroup is dropped when another classified cgroup
/// is a strict descendant of it.
fn drop_ancestor_scopes(found: Vec<(String, PathBuf)>) -> Vec<(String, PathBuf)> {
    let paths: Vec<&str> = found.iter().map(|(path, _)| path.as_str()).collect();
    let keep: Vec<bool> = paths
        .iter()
        .map(|&candidate| {
            !paths
                .iter()
                .any(|&other| is_strict_descendant(other, candidate))
        })
        .collect();
    found
        .into_iter()
        .zip(keep)
        .filter_map(|(entry, keep)| keep.then_some(entry))
        .collect()
}

/// True when `maybe_child` is a strict sub-cgroup of `maybe_parent`
/// (`maybe_child` starts with `maybe_parent/`).
fn is_strict_descendant(maybe_child: &str, maybe_parent: &str) -> bool {
    maybe_child.len() > maybe_parent.len()
        && maybe_child.starts_with(maybe_parent)
        && maybe_child.as_bytes().get(maybe_parent.len()) == Some(&b'/')
}

#[cfg(test)]
#[path = "../../../../tests/headless/linux_engine_process_telemetry_containers_tests.rs"]
mod tests;
