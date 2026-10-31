//! NPU accelerator inventory fold + the visible-processes projection cache,
//! split out of the parent app module so the file stays under the
//! source-line ceiling (same precedent as `effect_dispatch.rs`).

use taskmanager_application::{
    CorrelatedNpuInventoryEvent, DeviceLifecycleApplyResult, DeviceLifecyclePartition,
    DeviceLifecycleSnapshotRevision, NpuInventoryEvent,
};

use super::SystemProjectionStore;
use super::device_rows::DeviceRowFamily;
use taskforest_core::core::npu::NpuInventorySnapshot;

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
            changed |= self.apply_npu_inventory_snapshot(
                snapshot,
                DeviceLifecycleSnapshotRevision::new(correlated.sequence.get()),
            );
        }
        changed
    }

    pub(crate) fn apply_npu_inventory_snapshot(
        &mut self,
        mut snapshot: NpuInventorySnapshot,
        revision: DeviceLifecycleSnapshotRevision,
    ) -> bool {
        let result = self
            .device_lifecycle_projection
            .apply_npu_inventory_snapshot(revision, &mut snapshot);
        let accepted = matches!(&result, DeviceLifecycleApplyResult::Applied(_));
        self.device_lifecycle_diagnostics.record(result);
        if !accepted {
            return false;
        }

        let aggregate =
            snapshot
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
        self.note_npu_power_history(&snapshot);
        self.npu_inventory = Some(snapshot);
        self.note_device_row_family_changed(DeviceRowFamily::Npu);
        true
    }

    /// Append the aggregate accelerator power draw to the bounded power
    /// history.
    ///
    /// The aggregate is the sum of the accelerators that REPORT the
    /// capability: an accelerator without a power counter contributes
    /// nothing, and an inventory where no accelerator reports the capability
    /// appends NOTHING at all — the buffer stays empty, so a chart drawn
    /// from it has no series to draw. The energy counter is never derived
    /// from the power draw or from the utilization curve.
    fn note_npu_power_history(&mut self, snapshot: &NpuInventorySnapshot) {
        let watts: Option<f32> = snapshot
            .power
            .iter()
            .filter_map(|observation| {
                observation
                    .report
                    .power_w
                    .current_value()
                    .copied()
                    .filter(|value| value.is_finite())
            })
            .reduce(|left, right| left + right);
        let Some(watts) = watts else {
            return;
        };
        self.npu_power_history
            .push_back((snapshot.observed_at_ms, watts));
        const MAX_NPU_POWER_HISTORY: usize = 600;
        while self.npu_power_history.len() > MAX_NPU_POWER_HISTORY {
            let _ = self.npu_power_history.pop_front();
        }
    }

    pub(crate) fn seed_npu_inventory_fixture(&mut self, snapshot: NpuInventorySnapshot) {
        let next_revision = self
            .device_lifecycle_projection
            .accepted_revision_for(DeviceLifecyclePartition::NpuInventory)
            .map_or(1, |revision| revision.get().saturating_add(1));
        let _ = self.apply_npu_inventory_snapshot(
            snapshot,
            DeviceLifecycleSnapshotRevision::new(next_revision),
        );
    }
}

#[cfg(test)]
#[path = "../../tests/headless/shell_app_npu_power_history.rs"]
mod tests;
