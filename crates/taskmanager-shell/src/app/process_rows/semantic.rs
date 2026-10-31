//! Ordered adapter from the canonical process tree rows to the shared
//! semantic snapshot input.

use taskforest_core::core::process::ProcessItem;
use taskmanager_ui_contract::{
    ProcessAggregateSemantics, ProcessGroupRowInput, ProcessRowBehavior, ProcessRowInput,
    ProcessRowObservations, SemanticSnapshotBuilder,
};

use super::{ProcessProjectionGeneration, ProcessRowAggregate, ProcessRowId, ProcessTreeRow};

/// Append semantic inputs for the exact canonical row slice rendered by a
/// caller.
///
/// The callback resolves the same visible-list indexes that the renderer
/// uses. Name localization and selection policy stay with the caller, while
/// row kind, ordering, stable identity, tree state, and typed aggregate facts
/// come from the shared shell projection. Rows without live identity receive
/// generation-and-row-local semantic IDs and have no control actions.
pub fn append_process_semantic_rows<'process>(
    mut builder: SemanticSnapshotBuilder,
    generation: ProcessProjectionGeneration,
    rows: &[ProcessTreeRow],
    mut process_at: impl FnMut(usize) -> Option<&'process ProcessItem>,
    mut name_for: impl FnMut(&ProcessTreeRow, Option<&'process ProcessItem>) -> String,
    mut is_selected: impl FnMut(ProcessRowId) -> bool,
    memory_total_bytes: Option<u64>,
) -> SemanticSnapshotBuilder {
    for (row_index, row) in rows.iter().enumerate() {
        let process = row.visible_index().and_then(&mut process_at);
        match row {
            ProcessTreeRow::Category {
                category,
                expansion_key,
                expanded,
                aggregate,
                ..
            } => {
                let id = ProcessRowId::Category(*category).stable_key();
                builder = builder.process_group_row_with_semantics(
                    ProcessGroupRowInput {
                        id,
                        name: name_for(row, None),
                        expanded: *expanded,
                        selected: false,
                    },
                    Some(expansion_key.clone()),
                    false,
                    true,
                    aggregate_semantics(aggregate),
                );
            }
            ProcessTreeRow::Application {
                visible_index,
                row_key,
                expansion_key,
                expanded,
                aggregate,
                has_children,
                ..
            } => {
                let id = row_key.map_or_else(
                    || projection_local_id("application", generation, row_index, *visible_index),
                    ProcessRowId::stable_key,
                );
                let selectable = row_key.is_some();
                let selected = row_key.is_some_and(&mut is_selected);
                builder = builder.process_group_row_with_semantics(
                    ProcessGroupRowInput {
                        id,
                        name: name_for(row, process),
                        expanded: *expanded,
                        selected,
                    },
                    Some(expansion_key.clone()),
                    selectable,
                    selectable && *has_children,
                    aggregate_semantics(aggregate),
                );
            }
            ProcessTreeRow::Process {
                visible_index,
                row_key,
                has_children,
                collapsed,
                ..
            } => {
                let id = row_key.map_or_else(
                    || projection_local_id("process", generation, row_index, *visible_index),
                    ProcessRowId::stable_key,
                );
                let selectable = row_key.is_some();
                let selected = row_key.is_some_and(&mut is_selected);
                let name = name_for(row, process);
                let (cpu_percent, memory_percent, observations) = process.map_or_else(
                    || (None, None, None),
                    |process| {
                        let observations = ProcessRowObservations::new(
                            process.scalar_observations().cpu_percentage,
                            process.scalar_observations().memory_bytes,
                        );
                        let cpu_percent = process.current_cpu_percentage().map(f64::from);
                        let memory_percent = process
                            .current_memory_bytes()
                            .zip(memory_total_bytes.filter(|total| *total > 0))
                            .map(|(memory, total)| memory as f64 / total as f64 * 100.0);
                        (cpu_percent, memory_percent, Some(observations))
                    },
                );
                let behavior = if *has_children {
                    ProcessRowBehavior::TreeItem {
                        expanded: !collapsed,
                        selectable,
                        expansion_key: row_key
                            .and_then(ProcessRowId::live_key)
                            .map(|identity| identity.stable_key()),
                    }
                } else if selectable {
                    ProcessRowBehavior::Selectable
                } else {
                    ProcessRowBehavior::NonActionable
                };
                builder = builder.process_row_with_semantics(
                    ProcessRowInput {
                        id,
                        name,
                        cpu_percent,
                        memory_percent,
                        selected,
                    },
                    behavior,
                    observations,
                );
            }
        }
    }
    builder
}

fn projection_local_id(
    family: &str,
    generation: ProcessProjectionGeneration,
    row_index: usize,
    visible_index: usize,
) -> String {
    format!(
        "{family}:projection:{}:row:{row_index}:visible:{visible_index}:unknown",
        generation.get()
    )
}

fn aggregate_semantics(aggregate: &ProcessRowAggregate) -> ProcessAggregateSemantics {
    ProcessAggregateSemantics {
        cpu: aggregate.cpu().clone(),
        memory: aggregate.memory().clone(),
        memory_pss: aggregate.memory_pss().clone(),
        swap: aggregate.swap().clone(),
        disk_read: aggregate.disk_read().clone(),
        disk_write: aggregate.disk_write().clone(),
        cpu_time: aggregate.cpu_time().clone(),
        threads: aggregate.threads().clone(),
        fds: aggregate.fds().clone(),
    }
}
