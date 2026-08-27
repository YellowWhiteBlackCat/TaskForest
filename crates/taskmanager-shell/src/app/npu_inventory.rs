//! NPU accelerator inventory fold + the visible-processes projection cache,
//! split out of the parent app module so the file stays under the
//! source-line ceiling (same precedent as `effect_dispatch.rs`).

use taskmanager_application::{CorrelatedNpuInventoryEvent, NpuInventoryEvent};

use super::SystemProjectionStore;

impl SystemProjectionStore {
    /// Fold NPU accelerator inventory publications: latest-wins in event
    /// order. The snapshot carries a sorted device list or a typed failure —
    /// an empty list is the honest no-NPU host, never an error.
    pub(crate) fn apply_npu_inventory_events(
        &mut self,
        events: Vec<CorrelatedNpuInventoryEvent>,
    ) -> bool {
        let mut changed = false;
        for correlated in events {
            let NpuInventoryEvent::Update(snapshot) = correlated.event;
            self.npu_inventory = Some(snapshot);
            changed = true;
        }
        changed
    }
}
