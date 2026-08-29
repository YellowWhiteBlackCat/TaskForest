//! Aggregate row construction and sorting for canonical category/application roots.

use std::collections::HashMap;
use std::rc::Rc;

use taskmanager_core::core::process::{AppGroup, ProcessItem, sort_apps};

use super::projection::memory_for_display;
use super::{Toggle, VisibleRow};
use taskmanager_shell::ProcessRowId;
use taskmanager_shell::SortCol;

fn sum_opt_u64(acc: Option<u64>, value: Option<u64>) -> Option<u64> {
    match (acc, value) {
        (Some(left), Some(right)) => Some(left.saturating_add(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn sum_opt_u32(acc: Option<u32>, value: Option<u32>) -> Option<u32> {
    match (acc, value) {
        (Some(left), Some(right)) => Some(left.saturating_add(right)),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

fn sum_opt_f32(acc: Option<f32>, value: Option<f32>) -> Option<f32> {
    match (acc, value) {
        (Some(left), Some(right)) => Some(left + right),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    }
}

/// Fold one group's members into an aggregate [`VisibleRow`] (summed CPU% /
/// memory / disk / threads / cpu-time / fds, representative user/status
/// fields from `main_pid`, `×N` instance badge). Category roots are structural;
/// application roots carry a PID-less [`ProcessRowId::Application`].
pub(super) fn aggregate_row(
    group: &AppGroup,
    by_pid: &HashMap<u32, &ProcessItem>,
    toggle: Toggle,
) -> VisibleRow {
    let members: Vec<&ProcessItem> = group
        .pids
        .iter()
        .filter_map(|pid| by_pid.get(pid).copied())
        .collect();
    let (disk_read, disk_write, threads, cpu_time_secs, fds, cpu, mem, swap) = members.iter().fold(
        (None, None, None, None, None, None, None, None),
        |(read, write, threads, cpu_time, fds, cpu, mem, swap), process| {
            (
                sum_opt_u64(read, process.current_disk_read_bytes_per_sec()),
                sum_opt_u64(write, process.current_disk_write_bytes_per_sec()),
                sum_opt_u32(threads, process.current_threads()),
                sum_opt_u64(cpu_time, process.current_cpu_time_secs()),
                sum_opt_u32(fds, process.current_fds()),
                sum_opt_f32(cpu, process.current_cpu_percentage()),
                sum_opt_u64(mem, memory_for_display(process)),
                sum_opt_u64(swap, process.current_swap_bytes()),
            )
        },
    );
    let (user, status, start_time_secs, nice) = by_pid
        .get(&group.main_pid)
        .map(|process| {
            (
                process.current_user().unwrap_or_default(),
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
        Toggle::None | Toggle::TreePid(_) | Toggle::GroupCategory(_) => None,
    };
    let mut row = VisibleRow {
        name: group.name.clone(),
        selection_key,
        process_pid: None,
        pid: 0,
        application_identity: group.application_identity.clone(),
        user,
        status,
        cpu,
        mem,
        swap,
        disk_read,
        disk_write,
        threads,
        start_time_secs,
        cpu_time_secs,
        fds,
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
    row.cell_text = super::projection::RowCellText::build_additive(&row, &members);
    row
}

fn group_metric(
    group: &AppGroup,
    by_pid: &HashMap<u32, &ProcessItem>,
    column: SortCol,
) -> Option<u64> {
    group
        .pids
        .iter()
        .filter_map(|pid| by_pid.get(pid).copied())
        .fold(None, |total, process| {
            let value = match column {
                // PSS-preferred like iced's `group_metric` (the shell `Pss`
                // variant cannot be activated from GPUI chrome, but the
                // memory-shaped family stays uniform here).
                SortCol::Memory | SortCol::Pss => memory_for_display(process),
                SortCol::Swap => process.current_swap_bytes(),
                _ => None,
            };
            sum_opt_u64(total, value)
        })
}

pub(super) fn sort_groups(
    groups: &mut [AppGroup],
    processes: &[&ProcessItem],
    column: SortCol,
    ascending: bool,
) {
    if !matches!(column, SortCol::Memory | SortCol::Pss | SortCol::Swap) {
        // The shell's canonical `SortCol → ProcessSortKey` fallback for
        // aggregate headers (single documented rule; the same translation
        // iced's `sort_groups` consumes).
        sort_apps(
            groups,
            taskmanager_shell::aggregate_sort_key(column),
            ascending,
        );
        return;
    }
    let by_pid: HashMap<_, _> = processes
        .iter()
        .map(|process| (process.pid, *process))
        .collect();
    groups.sort_by(|left, right| {
        let ordering = group_metric(left, &by_pid, column)
            .cmp(&group_metric(right, &by_pid, column))
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.main_pid.cmp(&right.main_pid));
        if ascending {
            ordering
        } else {
            ordering.reverse()
        }
    });
}
