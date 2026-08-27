//! Latest-wins folds for renderer-neutral on-demand data lanes.

use super::*;

pub(super) fn apply_directory_usage(
    store: &mut SystemProjectionStore,
    events: Vec<taskmanager_application::CorrelatedDirectoryUsageEvent>,
    fold: &mut FoldState,
) {
    for correlated in events {
        let DirectoryUsageEvent::Update(snapshot) = correlated.event;
        store.directory_usage = Some(snapshot);
        fold.output.changes.directory_usage = true;
        fold.mark_updated();
    }
}

pub(super) fn apply_gpu_engine_rows(
    _store: &mut SystemProjectionStore,
    events: Vec<taskmanager_application::CorrelatedGpuEngineRowsEvent>,
    fold: &mut FoldState,
) {
    if !events.is_empty() {
        fold.output.changes.gpu_engine_rows = true;
        fold.mark_updated();
    }
}
