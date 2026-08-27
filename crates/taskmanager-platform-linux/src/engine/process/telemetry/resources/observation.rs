//! Read-only Linux cgroup and rlimit observation.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use taskmanager_core::{
    FailureKind, LimitValue, ProcessIdentity, ProcessResourceObservations, ProcessResourceSnapshot,
    ProviderId, ResourceGroupMembership, ResourceObservation, SourceOutcome, SourceStatus,
};

use super::{parse_limit_value, parse_proc_cgroup, parse_proc_limits};
use crate::engine::process::telemetry::{parse_start_time_ticks, safe_cgroup_path};

const LIMITS_PROVIDER: ProviderId = ProviderId::borrowed("linux.process.limits");
const MEMBERSHIP_PROVIDER: ProviderId = ProviderId::borrowed("linux.process.cgroup.membership");
const MEMORY_PROVIDER: ProviderId = ProviderId::borrowed("linux.process.cgroup.memory");
const CPU_PROVIDER: ProviderId = ProviderId::borrowed("linux.process.cgroup.cpu");
const PIDS_PROVIDER: ProviderId = ProviderId::borrowed("linux.process.cgroup.pids");

/// Stateful per-process resource retention.
///
/// The key is the frozen process identity, not a PID. Cgroup values retain a
/// prior success only when the current membership proves the exact same group
/// locator set.
#[derive(Debug, Default)]
pub struct ProcessResourceTracker {
    previous: Option<(ProcessIdentity, ProcessResourceSnapshot)>,
}

impl ProcessResourceTracker {
    pub fn collect(
        &mut self,
        proc_dir: &Path,
        cgroup_root: &Path,
        identity: ProcessIdentity,
        now_ms: u64,
    ) -> ProcessResourceSnapshot {
        let mut snapshot = collect_from_roots(proc_dir, cgroup_root, now_ms);
        if !identity_is_current(proc_dir, identity) {
            snapshot.apply_observations(unavailable_observations(FailureKind::IdentityChanged));
            publish_source_truth(&mut snapshot, now_ms);
            self.previous = None;
            return snapshot;
        }
        revalidate_membership(&mut snapshot, proc_dir, now_ms);
        if !identity_is_current(proc_dir, identity) {
            snapshot.apply_observations(unavailable_observations(FailureKind::IdentityChanged));
            publish_source_truth(&mut snapshot, now_ms);
            self.previous = None;
            return snapshot;
        }

        if let Some((known_identity, previous)) = self.previous.take()
            && known_identity == identity
        {
            let current_groups = current_group_identity(&snapshot);
            let same_resource_groups =
                current_groups.is_some() && current_groups == current_group_identity(&previous);
            snapshot = snapshot.retain_previous(previous, same_resource_groups);
        }
        publish_source_truth(&mut snapshot, now_ms);
        self.previous = Some((identity, snapshot.clone()));
        snapshot
    }
}

fn revalidate_membership(snapshot: &mut ProcessResourceSnapshot, proc_dir: &Path, now_ms: u64) {
    let rechecked = observe_memberships(proc_dir.join("cgroup"), now_ms);
    if rechecked == snapshot.observations().resource_groups {
        return;
    }
    let failure = match &rechecked {
        ResourceObservation::Current { .. }
        | ResourceObservation::Partial { .. }
        | ResourceObservation::Absent { .. } => FailureKind::IdentityChanged,
        observation => observation.failure().unwrap_or(FailureKind::ProviderFault),
    };
    let mut observations = snapshot.observations().clone();
    observations.resource_groups = rechecked;
    set_all_cgroup_unavailable(&mut observations, failure);
    snapshot.apply_observations(observations);
}

pub(in crate::engine::process::telemetry) fn collect_from_roots(
    proc_dir: &Path,
    cgroup_root: &Path,
    now_ms: u64,
) -> ProcessResourceSnapshot {
    let limits = observe_limits(proc_dir.join("limits"), now_ms);
    let resource_groups = observe_memberships(proc_dir.join("cgroup"), now_ms);
    let mut observations = ProcessResourceObservations {
        limits,
        resource_groups,
        ..Default::default()
    };
    observe_cgroup_fields(cgroup_root, &mut observations, now_ms);

    let mut snapshot = ProcessResourceSnapshot::default();
    snapshot.apply_observations(observations);
    publish_source_truth(&mut snapshot, now_ms);
    snapshot
}

fn observe_limits(
    path: PathBuf,
    now_ms: u64,
) -> ResourceObservation<Vec<taskmanager_core::ResourceLimit>> {
    match fs::read_to_string(path) {
        Ok(text) => {
            let values = parse_proc_limits(&text);
            let expected = text
                .lines()
                .skip(1)
                .filter(|line| !line.trim().is_empty())
                .count();
            match (values.len(), expected) {
                (actual, expected) if actual == expected => {
                    ResourceObservation::current(values, now_ms)
                }
                (0, _) => ResourceObservation::unavailable(FailureKind::ProviderFault),
                _ => ResourceObservation::partial(values, now_ms, FailureKind::ProviderFault),
            }
        }
        Err(error) => ResourceObservation::unavailable(proc_io_failure(error.kind())),
    }
}

fn observe_memberships(
    path: PathBuf,
    now_ms: u64,
) -> ResourceObservation<Vec<ResourceGroupMembership>> {
    match fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => ResourceObservation::absent(now_ms),
        Ok(text) => {
            let values = parse_proc_cgroup(&text);
            let expected = text.lines().filter(|line| !line.trim().is_empty()).count();
            match (values.len(), expected) {
                (actual, expected) if actual == expected => {
                    ResourceObservation::current(values, now_ms)
                }
                (0, _) => ResourceObservation::unavailable(FailureKind::ProviderFault),
                _ => ResourceObservation::partial(values, now_ms, FailureKind::ProviderFault),
            }
        }
        Err(error) => ResourceObservation::unavailable(proc_io_failure(error.kind())),
    }
}

fn observe_cgroup_fields(
    cgroup_root: &Path,
    observations: &mut ProcessResourceObservations,
    now_ms: u64,
) {
    let groups = match &observations.resource_groups {
        ResourceObservation::Current { value, .. } | ResourceObservation::Partial { value, .. } => {
            value.clone()
        }
        ResourceObservation::Absent { .. } => {
            set_all_cgroup_absent(observations, now_ms);
            return;
        }
        observation => {
            let failure = observation.failure().unwrap_or(FailureKind::ProviderFault);
            set_all_cgroup_unavailable(observations, failure);
            return;
        }
    };
    if let Some(unified) = groups.iter().find(is_unified_membership) {
        observe_v2(cgroup_root, unified, observations, now_ms);
    } else {
        observe_v1(cgroup_root, &groups, observations, now_ms);
    }
}

fn observe_v2(
    cgroup_root: &Path,
    membership: &ResourceGroupMembership,
    observations: &mut ProcessResourceObservations,
    now_ms: u64,
) {
    let Some(dir) = safe_cgroup_path(cgroup_root, &membership.native_locator) else {
        set_all_cgroup_unavailable(observations, FailureKind::ProviderFault);
        return;
    };
    let directory_failure = group_directory_failure(&dir);
    observations.memory_usage_bytes =
        observe_u64(dir.join("memory.current"), directory_failure, now_ms);
    observations.memory_limit =
        observe_limit(dir.join("memory.max"), directory_failure, false, now_ms);
    let (quota, period) = observe_v2_cpu(dir.join("cpu.max"), directory_failure, now_ms);
    observations.cpu_time_quota_micros = quota;
    observations.cpu_time_period_micros = period;
    observations.process_count = observe_u64(dir.join("pids.current"), directory_failure, now_ms);
    observations.process_limit =
        observe_limit(dir.join("pids.max"), directory_failure, false, now_ms);
}

fn observe_v1(
    cgroup_root: &Path,
    groups: &[ResourceGroupMembership],
    observations: &mut ProcessResourceObservations,
    now_ms: u64,
) {
    match v1_controller_dir(cgroup_root, groups, "memory") {
        Ok(Some(dir)) => {
            let directory_failure = group_directory_failure(&dir);
            observations.memory_usage_bytes =
                observe_u64(dir.join("memory.usage_in_bytes"), directory_failure, now_ms);
            observations.memory_limit = observe_limit(
                dir.join("memory.limit_in_bytes"),
                directory_failure,
                false,
                now_ms,
            );
        }
        Ok(None) => {
            observations.memory_usage_bytes =
                ResourceObservation::unavailable(FailureKind::Unsupported);
            observations.memory_limit = ResourceObservation::unavailable(FailureKind::Unsupported);
        }
        Err(failure) => {
            observations.memory_usage_bytes = ResourceObservation::unavailable(failure);
            observations.memory_limit = ResourceObservation::unavailable(failure);
        }
    }

    match v1_controller_dir(cgroup_root, groups, "cpu") {
        Ok(Some(dir)) => {
            let directory_failure = group_directory_failure(&dir);
            observations.cpu_time_quota_micros = observe_limit(
                dir.join("cpu.cfs_quota_us"),
                directory_failure,
                true,
                now_ms,
            );
            observations.cpu_time_period_micros =
                observe_u64(dir.join("cpu.cfs_period_us"), directory_failure, now_ms);
        }
        Ok(None) => {
            observations.cpu_time_quota_micros =
                ResourceObservation::unavailable(FailureKind::Unsupported);
            observations.cpu_time_period_micros =
                ResourceObservation::unavailable(FailureKind::Unsupported);
        }
        Err(failure) => {
            observations.cpu_time_quota_micros = ResourceObservation::unavailable(failure);
            observations.cpu_time_period_micros = ResourceObservation::unavailable(failure);
        }
    }

    match v1_controller_dir(cgroup_root, groups, "pids") {
        Ok(Some(dir)) => {
            let directory_failure = group_directory_failure(&dir);
            observations.process_count =
                observe_u64(dir.join("pids.current"), directory_failure, now_ms);
            observations.process_limit =
                observe_limit(dir.join("pids.max"), directory_failure, false, now_ms);
        }
        Ok(None) => {
            observations.process_count = ResourceObservation::unavailable(FailureKind::Unsupported);
            observations.process_limit = ResourceObservation::unavailable(FailureKind::Unsupported);
        }
        Err(failure) => {
            observations.process_count = ResourceObservation::unavailable(failure);
            observations.process_limit = ResourceObservation::unavailable(failure);
        }
    }
}

fn v1_controller_dir(
    cgroup_root: &Path,
    groups: &[ResourceGroupMembership],
    controller: &str,
) -> Result<Option<PathBuf>, FailureKind> {
    let membership = groups.iter().find(|membership| {
        membership
            .capabilities
            .iter()
            .any(|value| value == controller)
    });
    let Some(membership) = membership else {
        return Ok(None);
    };
    let combined = membership.capabilities.join(",");
    for mount in std::iter::once(combined.as_str())
        .chain(membership.capabilities.iter().map(String::as_str))
        .map(|name| cgroup_root.join(name))
    {
        match fs::metadata(&mount) {
            Ok(metadata) if metadata.is_dir() => {
                return safe_cgroup_path(&mount, &membership.native_locator)
                    .map(Some)
                    .ok_or(FailureKind::ProviderFault);
            }
            Ok(_) => continue,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(field_io_failure(error.kind(), FailureKind::IdentityChanged));
            }
        }
    }
    Ok(None)
}

fn observe_v2_cpu(
    path: PathBuf,
    missing_failure: FailureKind,
    now_ms: u64,
) -> (ResourceObservation<LimitValue>, ResourceObservation<u64>) {
    match fs::read_to_string(path) {
        Ok(text) => {
            let mut fields = text.split_whitespace();
            let quota = fields.next().and_then(parse_limit_value);
            let period = fields.next().and_then(|value| value.parse().ok());
            if fields.next().is_some() {
                return (
                    ResourceObservation::unavailable(FailureKind::ProviderFault),
                    ResourceObservation::unavailable(FailureKind::ProviderFault),
                );
            }
            (
                quota.map_or_else(
                    || ResourceObservation::unavailable(FailureKind::ProviderFault),
                    |value| ResourceObservation::current(value, now_ms),
                ),
                period.map_or_else(
                    || ResourceObservation::unavailable(FailureKind::ProviderFault),
                    |value| ResourceObservation::current(value, now_ms),
                ),
            )
        }
        Err(error) => {
            let failure = field_io_failure(error.kind(), missing_failure);
            (
                ResourceObservation::unavailable(failure),
                ResourceObservation::unavailable(failure),
            )
        }
    }
}

fn observe_u64(
    path: PathBuf,
    missing_failure: FailureKind,
    now_ms: u64,
) -> ResourceObservation<u64> {
    match fs::read_to_string(path) {
        Ok(text) => text.trim().parse().map_or_else(
            |_| ResourceObservation::unavailable(FailureKind::ProviderFault),
            |value| ResourceObservation::current(value, now_ms),
        ),
        Err(error) => {
            ResourceObservation::unavailable(field_io_failure(error.kind(), missing_failure))
        }
    }
}

fn observe_limit(
    path: PathBuf,
    missing_failure: FailureKind,
    negative_one_is_unlimited: bool,
    now_ms: u64,
) -> ResourceObservation<LimitValue> {
    match fs::read_to_string(path) {
        Ok(text) if negative_one_is_unlimited && text.trim() == "-1" => {
            ResourceObservation::current(LimitValue::Unlimited, now_ms)
        }
        Ok(text) => parse_limit_value(text.trim()).map_or_else(
            || ResourceObservation::unavailable(FailureKind::ProviderFault),
            |value| ResourceObservation::current(value, now_ms),
        ),
        Err(error) => {
            ResourceObservation::unavailable(field_io_failure(error.kind(), missing_failure))
        }
    }
}

fn group_directory_failure(dir: &Path) -> FailureKind {
    match fs::metadata(dir) {
        Ok(metadata) if metadata.is_dir() => FailureKind::Unsupported,
        Ok(_) => FailureKind::IdentityChanged,
        Err(error) => field_io_failure(error.kind(), FailureKind::IdentityChanged),
    }
}

fn field_io_failure(kind: ErrorKind, missing_failure: FailureKind) -> FailureKind {
    match kind {
        ErrorKind::NotFound => missing_failure,
        ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        ErrorKind::Interrupted | ErrorKind::WouldBlock | ErrorKind::TimedOut => {
            FailureKind::TemporarilyUnavailable
        }
        _ => FailureKind::ProviderFault,
    }
}

fn proc_io_failure(kind: ErrorKind) -> FailureKind {
    match kind {
        ErrorKind::NotFound => FailureKind::IdentityChanged,
        ErrorKind::PermissionDenied => FailureKind::PermissionDenied,
        ErrorKind::Interrupted | ErrorKind::WouldBlock | ErrorKind::TimedOut => {
            FailureKind::TemporarilyUnavailable
        }
        _ => FailureKind::ProviderFault,
    }
}

fn set_all_cgroup_absent(observations: &mut ProcessResourceObservations, now_ms: u64) {
    observations.memory_usage_bytes = ResourceObservation::absent(now_ms);
    observations.memory_limit = ResourceObservation::absent(now_ms);
    observations.cpu_time_quota_micros = ResourceObservation::absent(now_ms);
    observations.cpu_time_period_micros = ResourceObservation::absent(now_ms);
    observations.process_count = ResourceObservation::absent(now_ms);
    observations.process_limit = ResourceObservation::absent(now_ms);
}

fn set_all_cgroup_unavailable(
    observations: &mut ProcessResourceObservations,
    failure: FailureKind,
) {
    observations.memory_usage_bytes = ResourceObservation::unavailable(failure);
    observations.memory_limit = ResourceObservation::unavailable(failure);
    observations.cpu_time_quota_micros = ResourceObservation::unavailable(failure);
    observations.cpu_time_period_micros = ResourceObservation::unavailable(failure);
    observations.process_count = ResourceObservation::unavailable(failure);
    observations.process_limit = ResourceObservation::unavailable(failure);
}

fn unavailable_observations(failure: FailureKind) -> ProcessResourceObservations {
    ProcessResourceObservations {
        limits: ResourceObservation::unavailable(failure),
        resource_groups: ResourceObservation::unavailable(failure),
        memory_usage_bytes: ResourceObservation::unavailable(failure),
        memory_limit: ResourceObservation::unavailable(failure),
        cpu_time_quota_micros: ResourceObservation::unavailable(failure),
        cpu_time_period_micros: ResourceObservation::unavailable(failure),
        process_count: ResourceObservation::unavailable(failure),
        process_limit: ResourceObservation::unavailable(failure),
    }
}

fn identity_is_current(proc_dir: &Path, identity: ProcessIdentity) -> bool {
    fs::read_to_string(proc_dir.join("stat"))
        .ok()
        .and_then(|text| parse_start_time_ticks(&text))
        == Some(identity.start_token)
}

fn current_group_identity(
    snapshot: &ProcessResourceSnapshot,
) -> Option<Vec<ResourceGroupMembership>> {
    match &snapshot.observations().resource_groups {
        ResourceObservation::Current { value, .. } => Some(value.clone()),
        ResourceObservation::Unknown
        | ResourceObservation::Partial { .. }
        | ResourceObservation::Absent { .. }
        | ResourceObservation::Stale { .. }
        | ResourceObservation::Unavailable { .. } => None,
    }
}

fn is_unified_membership(membership: &&ResourceGroupMembership) -> bool {
    membership.provider.as_str() == "linux.cgroup"
        && membership.native_hierarchy_id == Some(0)
        && membership.capabilities.is_empty()
}

fn publish_source_truth(snapshot: &mut ProcessResourceSnapshot, now_ms: u64) {
    let observations = snapshot.observations();
    let mut sources = vec![
        list_source_status(LIMITS_PROVIDER, &observations.limits),
        list_source_status(MEMBERSHIP_PROVIDER, &observations.resource_groups),
        source_status_pair(
            MEMORY_PROVIDER,
            &observations.memory_usage_bytes,
            &observations.memory_limit,
        ),
        source_status_pair(
            CPU_PROVIDER,
            &observations.cpu_time_quota_micros,
            &observations.cpu_time_period_micros,
        ),
        source_status_pair(
            PIDS_PROVIDER,
            &observations.process_count,
            &observations.process_limit,
        ),
    ];
    sources.sort_by(|left, right| left.provider.cmp(&right.provider));
    let state = source_state(&sources, now_ms);
    snapshot.apply_source_truth(state, sources);
}

fn list_source_status<T>(
    provider: ProviderId,
    observation: &ResourceObservation<Vec<T>>,
) -> SourceStatus {
    let (outcome, item_count) = match observation {
        ResourceObservation::Current { value, .. } if value.is_empty() => (SourceOutcome::Empty, 0),
        ResourceObservation::Current { value, .. } => (SourceOutcome::Available, value.len()),
        ResourceObservation::Partial { value, failure, .. } => {
            (SourceOutcome::Partial(*failure), value.len())
        }
        ResourceObservation::Absent { .. } => (SourceOutcome::Empty, 0),
        ResourceObservation::Stale { failure, .. }
        | ResourceObservation::Unavailable { failure } => (SourceOutcome::Unavailable(*failure), 0),
        ResourceObservation::Unknown => (SourceOutcome::Unavailable(FailureKind::ProviderFault), 0),
    };
    SourceStatus {
        provider,
        outcome,
        item_count,
    }
}

fn source_status_pair<A, B>(
    provider: ProviderId,
    left: &ResourceObservation<A>,
    right: &ResourceObservation<B>,
) -> SourceStatus {
    let mut successful = 0usize;
    let mut item_count = 0usize;
    let mut failure = None;
    for observation in [observation_truth(left), observation_truth(right)] {
        match observation {
            (true, true, candidate) => {
                successful = successful.saturating_add(1);
                item_count = item_count.saturating_add(1);
                failure =
                    candidate.map_or(failure, |candidate| strongest_failure(failure, candidate));
            }
            (true, false, candidate) => {
                successful = successful.saturating_add(1);
                failure =
                    candidate.map_or(failure, |candidate| strongest_failure(failure, candidate));
            }
            (false, _, Some(candidate)) => {
                failure = strongest_failure(failure, candidate);
            }
            (false, _, None) => {
                failure = strongest_failure(failure, FailureKind::ProviderFault);
            }
        }
    }
    let outcome = match (successful, item_count, failure) {
        (2, 0, None) => SourceOutcome::Empty,
        (2, _, None) => SourceOutcome::Available,
        (count, _, Some(failure)) if count > 0 => SourceOutcome::Partial(failure),
        (_, _, Some(failure)) => SourceOutcome::Unavailable(failure),
        _ => SourceOutcome::Unavailable(FailureKind::ProviderFault),
    };
    SourceStatus {
        provider,
        outcome,
        item_count,
    }
}

fn observation_truth<T>(observation: &ResourceObservation<T>) -> (bool, bool, Option<FailureKind>) {
    match observation {
        ResourceObservation::Current { .. } => (true, true, None),
        ResourceObservation::Partial {
            failure: candidate, ..
        } => (true, true, Some(*candidate)),
        ResourceObservation::Absent { .. } => (true, false, None),
        ResourceObservation::Stale {
            failure: candidate, ..
        }
        | ResourceObservation::Unavailable { failure: candidate } => {
            (false, false, Some(*candidate))
        }
        ResourceObservation::Unknown => (false, false, Some(FailureKind::ProviderFault)),
    }
}

fn strongest_failure(current: Option<FailureKind>, candidate: FailureKind) -> Option<FailureKind> {
    match current {
        Some(current) if failure_priority(current) >= failure_priority(candidate) => Some(current),
        _ => Some(candidate),
    }
}

const fn failure_priority(failure: FailureKind) -> u8 {
    match failure {
        FailureKind::RequiresEscalation => 9,
        FailureKind::PermissionDenied => 8,
        FailureKind::IdentityChanged => 7,
        FailureKind::MissingDependency => 6,
        FailureKind::TimedOut => 5,
        FailureKind::ProviderFault => 4,
        FailureKind::TemporarilyUnavailable => 3,
        FailureKind::Unsupported => 2,
        FailureKind::Rejected => 1,
    }
}

fn source_state(sources: &[SourceStatus], now_ms: u64) -> taskmanager_core::DeviceState {
    let failure = sources
        .iter()
        .filter_map(|source| match source.outcome {
            SourceOutcome::Partial(failure) | SourceOutcome::Unavailable(failure) => Some(failure),
            SourceOutcome::Available | SourceOutcome::Empty => None,
        })
        .max_by_key(|failure| failure_priority(*failure));
    let status = match failure {
        Some(FailureKind::PermissionDenied) => taskmanager_core::DeviceStatus::PermissionDenied,
        Some(FailureKind::Unsupported)
            if sources.iter().all(|source| {
                matches!(
                    source.outcome,
                    SourceOutcome::Unavailable(FailureKind::Unsupported) | SourceOutcome::Empty
                )
            }) =>
        {
            taskmanager_core::DeviceStatus::Unsupported
        }
        Some(_) => taskmanager_core::DeviceStatus::Stale,
        None => taskmanager_core::DeviceStatus::Healthy,
    };
    taskmanager_core::DeviceState {
        status,
        last_success_ms: sources
            .iter()
            .any(|source| {
                matches!(
                    source.outcome,
                    SourceOutcome::Available | SourceOutcome::Empty | SourceOutcome::Partial(_)
                )
            })
            .then_some(now_ms),
    }
}

#[cfg(test)]
#[path = "../../../../../tests/headless/engine/process/telemetry/resources/observation.rs"]
mod tests;
