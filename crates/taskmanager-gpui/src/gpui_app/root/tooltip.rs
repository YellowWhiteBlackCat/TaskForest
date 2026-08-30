//! Lazy process lookups shared by the root renderer (cursor tooltips and the
//! Properties modal target).

use std::collections::HashMap;
use std::{rc::Rc, sync::Arc};

use taskmanager_core::core::process::{ProcessItem, ProcessLiveKey};

use super::RootView;

/// Lazy live-key index for cursor tooltips. The process table already owns the
/// authoritative `Arc<Vec<ProcessItem>>`; retaining that same `Arc` lets the
/// index detect a new snapshot without relying on every test/capture fixture
/// to remember a generation bump. The map is built only when a process tooltip
/// is first needed, then reused for every row crossing in that snapshot.
#[derive(Default)]
pub(crate) struct ProcessTooltipIndex {
    processes: Option<Arc<Vec<ProcessItem>>>,
    by_identity: HashMap<ProcessLiveKey, usize>,
}

impl ProcessTooltipIndex {
    pub(super) fn index_for(
        &mut self,
        processes: &Arc<Vec<ProcessItem>>,
        identity: ProcessLiveKey,
    ) -> Option<usize> {
        let snapshot_changed = self
            .processes
            .as_ref()
            .is_none_or(|cached| !Arc::ptr_eq(cached, processes));
        if snapshot_changed {
            self.by_identity.clear();
            self.by_identity.reserve(processes.len());
            self.by_identity
                .extend(processes.iter().enumerate().filter_map(|(index, process)| {
                    ProcessLiveKey::from_process(process).map(|identity| (identity, index))
                }));
            self.processes = Some(processes.clone());
        }
        self.by_identity.get(&identity).copied()
    }
}

/// The Properties modal's performance-graph series, converted to shared `Rc`s
/// once per memo rebuild. The details panel re-renders every frame while the
/// modal is open; passing these stable `Rc`s (instead of re-collecting the
/// item's `Vec<f32>` histories per frame) keeps the graph scene store's
/// identity key hitting, so the four history sparklines replay instead of
/// re-tessellating.
pub(crate) struct ProcessHistories {
    pub(crate) cpu: Rc<[f32]>,
    pub(crate) memory: Rc<[f32]>,
    pub(crate) disk_read: Rc<[f32]>,
    pub(crate) disk_write: Rc<[f32]>,
}

impl RootView {
    /// Resolve a process tooltip without scanning the live snapshot for every
    /// hover transition. Clone the command line only when it is displayed.
    pub(crate) fn process_tooltip_text(&mut self, identity: ProcessLiveKey) -> Option<String> {
        let processes = Arc::clone(self.processes_arc());
        let index = self.process_tooltip_index.index_for(&processes, identity)?;
        let process = processes.get(index)?;
        (process.cmdline.len() > process.name.len()).then(|| process.cmdline.clone())
    }

    /// Resolve the Properties-modal target without a per-frame O(process-count)
    /// scan + full clone: the cached `Rc<ProcessItem>` (plus the memoized
    /// shared-series pack the Performance section consumes, see
    /// [`ProcessHistories`]) is rebuilt only when the snapshot `Rc` or the
    /// requested live identity changes. A process refreshed out of the snapshot returns
    /// `None` (the caller clears the dialog slot).
    pub(crate) fn process_details_target(
        &mut self,
        identity: ProcessLiveKey,
    ) -> Option<(Rc<ProcessItem>, Rc<ProcessHistories>)> {
        let processes = Arc::clone(self.processes_arc());
        if let Some(cached) = self.projection_caches.process_details(&processes, identity) {
            return Some(cached);
        }
        let index = self.process_tooltip_index.index_for(&processes, identity)?;
        let item = processes.get(index)?;
        let item = Rc::new(item.clone());
        let histories = Rc::new(ProcessHistories {
            cpu: Rc::from(item.cpu_history.as_slice()),
            memory: Rc::from(item.mem_history.as_slice()),
            disk_read: Rc::from(item.disk_read_history.as_slice()),
            disk_write: Rc::from(item.disk_write_history.as_slice()),
        });
        self.projection_caches.replace_process_details(
            processes,
            identity,
            Rc::clone(&item),
            Rc::clone(&histories),
        );
        Some((item, histories))
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_root_tooltip_tests.rs"]
mod tests;
