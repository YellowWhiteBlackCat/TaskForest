//! Typed process-group aggregation.

use std::collections::{HashMap, HashSet};

use super::aggregate::{AggregateMetric, aggregate_f32, aggregate_u64};
use super::{
    ProcessApplicationIdentity, ProcessItem, ProcessLiveKey, application_group_name,
    classify_process_type, process_type_label,
};

/// Availability-preserving aggregate for one non-empty process group.
///
/// This is the canonical group projection for business code. Its numeric
/// totals cannot explain a missing member as a bare value: use the accessors
/// on this type when rendering, sorting, or persisting current group facts.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessGroupAggregate {
    name: String,
    main_identity: Option<ProcessLiveKey>,
    application_identity: Option<ProcessApplicationIdentity>,
    member_identities: Vec<ProcessLiveKey>,
    cpu: AggregateMetric<f32>,
    memory: AggregateMetric<u64>,
    process_count: usize,
}

impl ProcessGroupAggregate {
    /// Stable group label.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The exact live identity of the selected root when its provider token is
    /// available. A group can remain displayable when its provider did not
    /// expose identity authority for the root.
    #[must_use]
    pub const fn main_identity(&self) -> Option<ProcessLiveKey> {
        self.main_identity
    }

    /// Verified desktop-entry identity when this is an application group.
    #[must_use]
    pub fn application_identity(&self) -> Option<&ProcessApplicationIdentity> {
        self.application_identity.as_ref()
    }

    /// Exact live identities for the members whose provider tokens are
    /// available, in deterministic PID order. A missing token is omitted from
    /// this authority list while the aggregate count and metrics remain honest.
    #[must_use]
    pub fn member_identities(&self) -> &[ProcessLiveKey] {
        &self.member_identities
    }

    /// Availability-bearing CPU aggregate.
    #[must_use]
    pub const fn cpu(&self) -> &AggregateMetric<f32> {
        &self.cpu
    }

    /// Availability-bearing resident-memory aggregate.
    #[must_use]
    pub const fn memory(&self) -> &AggregateMetric<u64> {
        &self.memory
    }

    /// Number of process members, including members whose metric is not
    /// currently available.
    #[must_use]
    pub const fn process_count(&self) -> usize {
        self.process_count
    }
}

/// Build one typed aggregate from an arbitrary non-empty process member set.
///
/// This is the owner-side entry point for process-tree roots and other
/// projections that already resolved their members. The caller supplies the
/// display label and optional exact root identity, while core computes the
/// member identity list and all CPU/memory aggregate semantics. A supplied
/// root identity that is not part of `members` is rejected.
#[must_use]
pub fn aggregate_process_group_typed(
    name: impl Into<String>,
    main_identity: Option<ProcessLiveKey>,
    application_identity: Option<ProcessApplicationIdentity>,
    members: &[&ProcessItem],
    observed_at_ms: u64,
) -> Option<ProcessGroupAggregate> {
    if members.is_empty()
        || main_identity.is_some_and(|identity| {
            !members
                .iter()
                .any(|process| ProcessLiveKey::from_process(process) == Some(identity))
        })
    {
        return None;
    }
    let mut member_identities: Vec<_> = members
        .iter()
        .filter_map(|process| ProcessLiveKey::from_process(process))
        .collect();
    member_identities.sort_unstable();
    process_group_aggregate(
        name.into(),
        main_identity,
        application_identity,
        member_identities,
        members.to_vec(),
        observed_at_ms,
    )
}

/// Availability-preserving aggregate for one user bucket.
#[derive(Debug, Clone, PartialEq)]
pub struct UserProcessAggregate {
    user: Option<String>,
    cpu: AggregateMetric<f32>,
    memory: AggregateMetric<u64>,
    process_count: usize,
}

impl UserProcessAggregate {
    /// Current owner label. `None` means the source did not provide a current
    /// owner identity; the absence remains typed instead of becoming an empty
    /// legacy string.
    #[must_use]
    pub fn user(&self) -> Option<&str> {
        self.user.as_deref()
    }

    /// Availability-bearing CPU aggregate.
    #[must_use]
    pub const fn cpu(&self) -> &AggregateMetric<f32> {
        &self.cpu
    }

    /// Availability-bearing resident-memory aggregate.
    #[must_use]
    pub const fn memory(&self) -> &AggregateMetric<u64> {
        &self.memory
    }

    /// Number of process members, including unavailable members.
    #[must_use]
    pub const fn process_count(&self) -> usize {
        self.process_count
    }
}

/// Aggregate application groups while preserving every member metric's typed
/// availability. `observed_at_ms` is the timestamp of the accepted process
/// snapshot and must come from the caller that owns that snapshot.
#[must_use]
pub fn aggregate_apps_typed(
    items: &[&ProcessItem],
    observed_at_ms: u64,
) -> Vec<ProcessGroupAggregate> {
    let mut grouped: HashMap<String, Vec<&ProcessItem>> = HashMap::new();
    for item in items {
        grouped
            .entry(application_group_name(item))
            .or_default()
            .push(*item);
    }
    let mut groups: Vec<_> = grouped
        .into_iter()
        .filter_map(|(name, processes)| {
            let mut pids: Vec<_> = processes.iter().map(|process| process.pid).collect();
            pids.sort_unstable();
            let pid_set: HashSet<_> = pids.iter().copied().collect();
            let main_pid = processes
                .iter()
                .filter(|process| process.parent_pid.is_none_or(|pid| !pid_set.contains(&pid)))
                .map(|process| process.pid)
                .min()
                .or_else(|| pids.first().copied())?;
            let main_identity = processes
                .iter()
                .find(|process| process.pid == main_pid)
                .and_then(|process| ProcessLiveKey::from_process(process));
            let application_identity = processes
                .iter()
                .find_map(|process| process.current_application_identity())
                .cloned();
            aggregate_process_group_typed(
                name,
                main_identity,
                application_identity,
                &processes,
                observed_at_ms,
            )
        })
        .collect();
    sort_typed_process_groups_default(&mut groups);
    groups
}

/// Aggregate users while retaining typed CPU and memory availability.
#[must_use]
pub fn aggregate_by_user_typed(
    processes: &[ProcessItem],
    observed_at_ms: u64,
) -> Vec<UserProcessAggregate> {
    let mut grouped: HashMap<Option<String>, Vec<&ProcessItem>> = HashMap::new();
    for process in processes {
        grouped
            .entry(process.current_user())
            .or_default()
            .push(process);
    }
    let mut rows: Vec<_> = grouped
        .into_iter()
        .filter_map(|(user, members)| {
            let (cpu, memory) = aggregate_process_metrics(&members, observed_at_ms)?;
            Some(UserProcessAggregate {
                user,
                cpu,
                memory,
                process_count: members.len(),
            })
        })
        .collect();
    rows.sort_by(compare_typed_users_default);
    rows
}

/// Aggregate kernel/userspace groups while preserving typed availability.
#[must_use]
pub fn aggregate_by_type_typed(
    items: &[&ProcessItem],
    observed_at_ms: u64,
) -> Vec<ProcessGroupAggregate> {
    let mut grouped: HashMap<&'static str, Vec<&ProcessItem>> = HashMap::new();
    for item in items {
        grouped
            .entry(process_type_label(classify_process_type(item)))
            .or_default()
            .push(*item);
    }
    let mut groups: Vec<_> = grouped
        .into_iter()
        .filter_map(|(label, members)| {
            let mut pids: Vec<_> = members.iter().map(|process| process.pid).collect();
            pids.sort_unstable();
            let main_pid = pids.first().copied()?;
            let main_identity = members
                .iter()
                .find(|process| process.pid == main_pid)
                .and_then(|process| ProcessLiveKey::from_process(process));
            aggregate_process_group_typed(
                label.to_string(),
                main_identity,
                None,
                &members,
                observed_at_ms,
            )
        })
        .collect();
    sort_typed_process_groups_default(&mut groups);
    groups
}

fn process_group_aggregate(
    name: String,
    main_identity: Option<ProcessLiveKey>,
    application_identity: Option<ProcessApplicationIdentity>,
    member_identities: Vec<ProcessLiveKey>,
    members: Vec<&ProcessItem>,
    observed_at_ms: u64,
) -> Option<ProcessGroupAggregate> {
    let (cpu, memory) = aggregate_process_metrics(&members, observed_at_ms)?;
    Some(ProcessGroupAggregate {
        name,
        main_identity,
        application_identity,
        member_identities,
        cpu,
        memory,
        process_count: members.len(),
    })
}

fn aggregate_process_metrics(
    members: &[&ProcessItem],
    observed_at_ms: u64,
) -> Option<(AggregateMetric<f32>, AggregateMetric<u64>)> {
    let cpu = aggregate_f32(
        members
            .iter()
            .copied()
            .map(|process| &process.scalar_observations().cpu_percentage),
        observed_at_ms,
    )?;
    let memory = aggregate_u64(
        members
            .iter()
            .copied()
            .map(|process| &process.scalar_observations().memory_bytes),
        observed_at_ms,
    )?;
    Some((cpu, memory))
}

fn compare_current_f32(
    left: &AggregateMetric<f32>,
    right: &AggregateMetric<f32>,
) -> std::cmp::Ordering {
    match (left.current_value(), right.current_value()) {
        (Some(left), Some(right)) => left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal),
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn compare_current_u64(
    left: &AggregateMetric<u64>,
    right: &AggregateMetric<u64>,
) -> std::cmp::Ordering {
    match (left.current_value(), right.current_value()) {
        (Some(left), Some(right)) => left.cmp(right),
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn sort_typed_process_groups_default(groups: &mut [ProcessGroupAggregate]) {
    groups.sort_by(|left, right| {
        compare_current_f32(right.cpu(), left.cpu())
            .then_with(|| compare_current_u64(right.memory(), left.memory()))
            .then_with(|| left.main_identity.cmp(&right.main_identity))
    });
}

fn compare_typed_users_default(
    left: &UserProcessAggregate,
    right: &UserProcessAggregate,
) -> std::cmp::Ordering {
    compare_current_f32(right.cpu(), left.cpu())
        .then_with(|| compare_current_u64(right.memory(), left.memory()))
        .then_with(|| left.user.cmp(&right.user))
}
