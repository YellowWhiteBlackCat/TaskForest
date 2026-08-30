//! Aggregate row construction and sorting for canonical category/application roots.

use std::collections::HashMap;
use std::rc::Rc;

use taskmanager_application::process_category_projection::category_buckets;
use taskmanager_application::process_category_projection::process_memory_observation_for_display;
use taskmanager_core::core::process::aggregate::AggregateMetric;
use taskmanager_core::core::process::aggregate::aggregate_u32_widened;
use taskmanager_core::core::process::{ProcessApplicationIdentity, ProcessCategory, ProcessItem};
use taskmanager_core::core::units::UnitPreferences;

use super::{Toggle, VisibleRow};
use taskmanager_shell::ProcessRowId;
use taskmanager_shell::SortCol;
use taskmanager_shell::presentation::missing_value;

/// GPUI-owned aggregate input. The member list stays as process identities and
/// the numeric facts stay availability-bearing, so an aggregate row cannot
/// accidentally collapse unavailable members into a zero-fold total.
pub(super) struct AggregateGroup {
    pub(super) name: String,
    pub(super) main_pid: u32,
    pub(super) application_identity: Option<ProcessApplicationIdentity>,
    pub(super) cpu: AggregateMetric<f32>,
    pub(super) memory: AggregateMetric<u64>,
    pub(super) swap: AggregateMetric<u64>,
    pub(super) disk_read: AggregateMetric<u64>,
    pub(super) disk_write: AggregateMetric<u64>,
    pub(super) threads: AggregateMetric<u64>,
    pub(super) cpu_time: AggregateMetric<u64>,
    pub(super) fds: AggregateMetric<u64>,
    pub(super) process_count: usize,
}

/// Build one aggregate group from the shared process members. Every additive
/// field remains availability-bearing; a missing member is not silently
/// discarded from a successful numeric total.
pub(super) fn aggregate_group_from_members(
    name: String,
    main_pid: u32,
    application_identity: Option<ProcessApplicationIdentity>,
    members: &[&ProcessItem],
    observed_at_ms: u64,
) -> Option<AggregateGroup> {
    let bucket = category_buckets(members, |_| ProcessCategory::Application)
        .into_iter()
        .next()?;
    Some(AggregateGroup {
        name,
        main_pid,
        application_identity,
        cpu: bucket.aggregate_f32(observed_at_ms, |process| {
            &process.scalar_observations().cpu_percentage
        })?,
        memory: bucket.aggregate_u64(observed_at_ms, |process| {
            process_memory_observation_for_display(process)
        })?,
        swap: bucket.aggregate_u64(observed_at_ms, |process| {
            &process.scalar_observations().swap_bytes
        })?,
        disk_read: bucket.aggregate_u64(observed_at_ms, |process| {
            &process.scalar_observations().disk_read_bytes_per_sec
        })?,
        disk_write: bucket.aggregate_u64(observed_at_ms, |process| {
            &process.scalar_observations().disk_write_bytes_per_sec
        })?,
        threads: aggregate_u32_widened(
            members
                .iter()
                .map(|process| &process.scalar_observations().threads),
            observed_at_ms,
        )?,
        cpu_time: bucket.aggregate_u64(observed_at_ms, |process| {
            &process.scalar_observations().cpu_time_secs
        })?,
        fds: aggregate_u32_widened(
            members
                .iter()
                .map(|process| &process.scalar_observations().fds),
            observed_at_ms,
        )?,
        process_count: members.len(),
    })
}

/// Fold one group's members into an aggregate [`VisibleRow`] (summed CPU% /
/// memory / disk / threads / cpu-time / fds, representative user/status
/// fields from `main_pid`, `×N` instance badge). Category roots are structural;
/// application roots carry a PID-less [`ProcessRowId::Application`].
pub(super) fn aggregate_row(
    group: &AggregateGroup,
    by_pid: &HashMap<u32, &ProcessItem>,
    toggle: Toggle,
    units: UnitPreferences,
) -> VisibleRow {
    let (user, status, start_time_secs, nice) = by_pid
        .get(&group.main_pid)
        .map(|process| {
            (
                process.current_user().unwrap_or_else(missing_value),
                process.status.clone(),
                process.current_start_time_secs(),
                process.current_nice(),
            )
        })
        .unwrap_or_default();
    let selection_key = match &toggle {
        Toggle::GroupApp(_) => by_pid
            .get(&group.main_pid)
            .and_then(|root| ProcessRowId::application_of(root)),
        Toggle::None | Toggle::Tree(_) | Toggle::GroupCategory(_) => None,
    };
    let mut row = VisibleRow {
        name: group.name.clone(),
        selection_key,
        process_identity: None,
        application_identity: group.application_identity.clone(),
        user,
        status,
        cpu: group.cpu.current_value().copied(),
        mem: group.memory.current_value().copied(),
        cpu_aggregate: Some(group.cpu.clone()),
        memory_aggregate: Some(group.memory.clone()),
        swap: group.swap.current_value().copied(),
        disk_read: group.disk_read.current_value().copied(),
        disk_write: group.disk_write.current_value().copied(),
        threads: group
            .threads
            .current_value()
            .and_then(|value| u32::try_from(*value).ok()),
        start_time_secs,
        cpu_time_secs: group.cpu_time.current_value().copied(),
        fds: group
            .fds
            .current_value()
            .and_then(|value| u32::try_from(*value).ok()),
        nice,
        cpu_history: Rc::from([]),
        name_highlights: Vec::new(),
        cell_text: super::projection::RowCellText::default(),
        depth: 0,
        // Category/application roots remain hierarchy nodes for singletons.
        has_children: true,
        collapsed: true,
        // An aggregate row's only parent is a structural category header (or
        // nothing) — never a selectable ancestor.
        parent_key: None,
        badge: Some(format!("\u{00d7}{}", group.process_count)),
        toggle,
    };
    row.cell_text = super::projection::RowCellText::build(&row, units);
    row
}

fn group_metric(group: &AggregateGroup, column: SortCol) -> Option<u64> {
    match column {
        // The typed memory aggregate is computed by the category projection
        // and is shared by sorting, row cells, and availability semantics.
        SortCol::Memory | SortCol::Pss => group.memory.current_value().copied(),
        SortCol::Swap => group.swap.current_value().copied(),
        SortCol::DiskRead => group.disk_read.current_value().copied(),
        SortCol::DiskWrite => group.disk_write.current_value().copied(),
        _ => None,
    }
}

fn optional_f32_cmp(left: Option<f32>, right: Option<f32>) -> std::cmp::Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left
            .partial_cmp(&right)
            .unwrap_or(std::cmp::Ordering::Equal),
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn optional_u64_cmp(left: Option<u64>, right: Option<u64>) -> std::cmp::Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

pub(super) fn sort_groups(groups: &mut [AggregateGroup], column: SortCol, ascending: bool) {
    groups.sort_by(|left, right| {
        let ordering = match taskmanager_shell::aggregate_sort_key(column) {
            taskmanager_core::core::process::ProcessSortKey::Pid => {
                left.main_pid.cmp(&right.main_pid)
            }
            taskmanager_core::core::process::ProcessSortKey::Name => {
                left.name.to_lowercase().cmp(&right.name.to_lowercase())
            }
            taskmanager_core::core::process::ProcessSortKey::CpuUsage => optional_f32_cmp(
                left.cpu.current_value().copied(),
                right.cpu.current_value().copied(),
            ),
            taskmanager_core::core::process::ProcessSortKey::Memory
            | taskmanager_core::core::process::ProcessSortKey::DiskRead
            | taskmanager_core::core::process::ProcessSortKey::DiskWrite => {
                optional_u64_cmp(group_metric(left, column), group_metric(right, column))
            }
        }
        .then_with(|| left.name.cmp(&right.name))
        .then_with(|| left.main_pid.cmp(&right.main_pid));
        if ascending {
            ordering
        } else {
            ordering.reverse()
        }
    });
}
