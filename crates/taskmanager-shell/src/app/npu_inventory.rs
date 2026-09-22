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
            let aggregate = snapshot
                .devices
                .iter()
                .filter_map(|device| {
                    device
                        .utilization_pct
                        .current_value()
                        .copied()
                        .filter(|value| value.is_finite() && (0.0..=100.0).contains(value))
                })
                .reduce(|left, right| left + right)
                .and_then(|sum| {
                    let count =
                        snapshot
                            .devices
                            .iter()
                            .filter(|device| {
                                device.utilization_pct.current_value().copied().is_some_and(
                                    |value| value.is_finite() && (0.0..=100.0).contains(&value),
                                )
                            })
                            .count();
                    (count > 0).then_some(sum / count as f32)
                });
            self.npu_usage_history
                .push_back(aggregate.unwrap_or(f32::NAN));
            const MAX_NPU_HISTORY: usize = 600;
            while self.npu_usage_history.len() > MAX_NPU_HISTORY {
                let _ = self.npu_usage_history.pop_front();
            }
            self.npu_inventory = Some(snapshot);
            changed = true;
        }
        changed
    }
}
