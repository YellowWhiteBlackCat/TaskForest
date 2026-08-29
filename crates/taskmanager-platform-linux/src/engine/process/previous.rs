//! Compact previous-process state used by the Linux refresh delta pass.

use std::collections::HashMap;

use taskmanager_core::core::process::{
    ProcessApplicationIdentity, ProcessHistorySnapshot, ProcessItem, ProcessMetadataObservation,
    ProcessMetadataObservations, ProcessScalarObservations,
};
use taskmanager_core::{FailureKind, ProcessHistoryStore};

/// Read-only subset of the previous process row needed to retain typed values
/// across a provider refresh. Histories deliberately stay in
/// [`ProcessHistoryStore`]; keeping them here as five cloned `Vec`s doubled
/// the largest part of every process snapshot.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PreviousProcessState {
    pub(super) pid: u32,
    parent_pid: Option<u32>,
    pub(super) name: String,
    cmdline: String,
    status: String,
    metadata_observations: ProcessMetadataObservations,
    application_identity: ProcessMetadataObservation<ProcessApplicationIdentity>,
    scalar_observations: ProcessScalarObservations,
}

impl From<&ProcessItem> for PreviousProcessState {
    fn from(item: &ProcessItem) -> Self {
        Self {
            pid: item.pid,
            parent_pid: item.parent_pid,
            name: item.name.clone(),
            cmdline: item.cmdline.clone(),
            status: item.status.clone(),
            metadata_observations: item.metadata_observations().clone(),
            application_identity: item.application_identity_observation().clone(),
            scalar_observations: *item.scalar_observations(),
        }
    }
}

impl PreviousProcessState {
    fn replace_from(&mut self, item: &ProcessItem) {
        self.pid = item.pid;
        self.parent_pid = item.parent_pid;
        self.name.clone_from(&item.name);
        self.cmdline.clone_from(&item.cmdline);
        self.status.clone_from(&item.status);
        self.metadata_observations
            .clone_from(item.metadata_observations());
        self.application_identity
            .clone_from(item.application_identity_observation());
        self.scalar_observations = *item.scalar_observations();
    }

    fn to_item(&self, history: ProcessHistorySnapshot) -> ProcessItem {
        let mut item = ProcessItem::new(self.pid, self.name.clone());
        item.parent_pid = self.parent_pid;
        item.cmdline = self.cmdline.clone();
        item.status = self.status.clone();
        item.apply_metadata_observations(self.metadata_observations.clone());
        item.apply_application_identity(self.application_identity.clone());
        item.apply_scalar_observations(self.scalar_observations);
        item.cpu_history = history.cpu;
        item.mem_history = history.memory;
        item.disk_history = history.disk;
        item.disk_read_history = history.disk_read;
        item.disk_write_history = history.disk_write;
        item
    }
}

/// The helper modules accept both a live `ProcessItem` (for tests and other
/// callers) and this compact retained state without making the full row part
/// of the provider's steady-state cache.
pub(super) trait PreviousProcessView {
    fn current_start_token(&self) -> Option<u64>;
    fn scalar_observations(&self) -> &ProcessScalarObservations;
    fn metadata_observations(&self) -> &ProcessMetadataObservations;
    fn application_identity_observation(
        &self,
    ) -> &ProcessMetadataObservation<ProcessApplicationIdentity>;
}

impl PreviousProcessView for ProcessItem {
    fn current_start_token(&self) -> Option<u64> {
        ProcessItem::current_start_token(self)
    }

    fn scalar_observations(&self) -> &ProcessScalarObservations {
        ProcessItem::scalar_observations(self)
    }

    fn metadata_observations(&self) -> &ProcessMetadataObservations {
        ProcessItem::metadata_observations(self)
    }

    fn application_identity_observation(
        &self,
    ) -> &ProcessMetadataObservation<ProcessApplicationIdentity> {
        ProcessItem::application_identity_observation(self)
    }
}

impl PreviousProcessView for PreviousProcessState {
    fn current_start_token(&self) -> Option<u64> {
        self.scalar_observations
            .start_token
            .current_value()
            .copied()
    }

    fn scalar_observations(&self) -> &ProcessScalarObservations {
        &self.scalar_observations
    }

    fn metadata_observations(&self) -> &ProcessMetadataObservations {
        &self.metadata_observations
    }

    fn application_identity_observation(
        &self,
    ) -> &ProcessMetadataObservation<ProcessApplicationIdentity> {
        &self.application_identity
    }
}

/// Last tick's retained state plus a pid → index lookup, rebuilt together by
/// [`Self::sync_from`] so the previous-tick lookup is O(1) while preserving
/// the `Vec::iter().find()` semantics (first occurrence wins) it replaced.
#[derive(Default)]
pub(super) struct PreviousItems {
    pub(super) items: Vec<PreviousProcessState>,
    pub(super) by_pid: HashMap<u32, usize>,
    seen: Vec<bool>,
}

impl PreviousItems {
    pub(super) fn find(&self, pid: u32) -> Option<&PreviousProcessState> {
        self.by_pid
            .get(&pid)
            .and_then(|&index| self.items.get(index))
    }

    pub(super) fn stale_items(
        &self,
        histories: &ProcessHistoryStore,
        failure: FailureKind,
    ) -> Vec<ProcessItem> {
        self.items
            .iter()
            .map(|previous| {
                let mut item = previous.to_item(histories.snapshot_for(previous.pid));
                super::observation::mark_retained_item_stale(&mut item, failure);
                item
            })
            .collect()
    }

    pub(super) fn sync_from(&mut self, items: &[ProcessItem]) {
        // Reuse the retained rows in place. This keeps the previous-value
        // cache's String/metadata allocations stable across the 1 Hz process
        // refresh instead of cloning and freeing the whole cache every tick.
        self.seen.resize(self.items.len(), false);
        self.seen.fill(false);
        for item in items {
            match self.by_pid.get(&item.pid).copied() {
                Some(index) if !self.seen[index] => {
                    self.items[index].replace_from(item);
                    self.seen[index] = true;
                }
                Some(_) => {}
                None => {
                    self.by_pid.insert(item.pid, self.items.len());
                    self.items.push(PreviousProcessState::from(item));
                    self.seen.push(true);
                }
            }
        }
        let mut index = 0;
        self.items.retain(|_| {
            let keep = self.seen[index];
            index += 1;
            keep
        });
        self.seen.clear();
        self.by_pid.clear();
        for (index, item) in self.items.iter().enumerate() {
            self.by_pid.entry(item.pid).or_insert(index);
        }
    }
}
