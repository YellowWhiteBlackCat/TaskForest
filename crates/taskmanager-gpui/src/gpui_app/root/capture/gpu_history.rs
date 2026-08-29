//! Capture-only correlated history for the fixed multi-engine GPU scene.

use std::collections::BTreeMap;

use taskmanager_core::core::{
    DeviceId, DeviceLifecycle, DevicePresence, DeviceState, GpuTelemetryObservation,
};
use taskmanager_telemetry_store::{
    CorrelatedSystemTelemetryHistory, CorrelatedSystemTelemetryIngestor, CorrelatedTelemetryStamp,
    SystemHistoryDomain,
};

use super::fixtures::{
    GPU_ENGINE_CAPTURE_DEVICE_ID, GPU_ENGINE_CAPTURE_LAST_SAMPLE_INDEX, gpu_engine_inventory_frame,
};

pub(super) const GPU_ENGINE_CAPTURE_SAMPLE_COUNT: u64 = GPU_ENGINE_CAPTURE_LAST_SAMPLE_INDEX + 1;

/// Feed five consecutive accepted GPU frames through the same typed store used
/// by live correlation. No renderer-owned history or mutable production seam
/// is introduced by the capture scenario.
pub(super) fn seed(
    history: &CorrelatedSystemTelemetryHistory,
    ingestor: &CorrelatedSystemTelemetryIngestor,
    anchor_timestamp_ms: u64,
) -> bool {
    let base_revision = history
        .receipts(SystemHistoryDomain::Gpu)
        .last()
        .map_or(0, |receipt| receipt.stamp.revision());
    let first_timestamp_ms = anchor_timestamp_ms.max(1).saturating_add(1);

    for index in 0..GPU_ENGINE_CAPTURE_SAMPLE_COUNT {
        let Some(revision) = base_revision.checked_add(index.saturating_add(1)) else {
            return false;
        };
        let timestamp_ms = first_timestamp_ms.saturating_add(index);
        let Some(stamp) = CorrelatedTelemetryStamp::from_accepted_event(revision, timestamp_ms)
        else {
            return false;
        };
        let gpu = gpu_engine_inventory_frame(index, timestamp_ms);
        let lifecycles = BTreeMap::from([(
            DeviceId::new(GPU_ENGINE_CAPTURE_DEVICE_ID),
            DeviceLifecycle {
                presence: DevicePresence::Present,
                state: DeviceState::healthy(timestamp_ms),
                generation: 1,
                first_seen_ms: Some(first_timestamp_ms),
                last_seen_ms: Some(timestamp_ms),
                absent_since_ms: None,
            },
        )]);
        if ingestor
            .ingest_correlated_gpu(
                stamp,
                &GpuTelemetryObservation::current(
                    vec![gpu],
                    timestamp_ms,
                    Vec::new(),
                    Vec::new(),
                    lifecycles,
                ),
            )
            .is_err()
        {
            return false;
        }
    }
    true
}
