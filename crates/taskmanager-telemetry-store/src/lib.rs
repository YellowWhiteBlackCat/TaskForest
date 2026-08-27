//! Shared in-process telemetry history state.
//!
//! Snapshot models remain in `taskmanager-core`. This crate owns only the
//! concurrency and bounded-history mechanism populated after application
//! correlation and observed by frontends.

#![forbid(unsafe_code)]

use std::sync::Arc;

pub mod live_graph;
mod system_history;

pub use system_history::{
    CorrelatedDomainReceipt, CorrelatedIngestionError, CorrelatedIngestionReport,
    CorrelatedMetricHistory, CorrelatedMetricSample, CorrelatedSystemTelemetryHistory,
    CorrelatedSystemTelemetryIngestor, CorrelatedTelemetryStamp, DeviceMetricHistory,
    DynamicHistoryDomain, DynamicIngestionError, DynamicIngestionReport, DynamicTelemetryHistory,
    GpuMetricPoint, MAX_DYNAMIC_HISTORY_IDENTITIES, SystemHistoryDomain,
};

/// Platform-neutral UI history populated from correlated domain outcomes.
pub struct TelemetryStore {
    /// Six-domain history populated only through the correlation-capability
    /// returned by [`Self::shared_with_correlated_ingestion`].
    pub system_history: CorrelatedSystemTelemetryHistory,
    /// Runtime battery/fan/temperature histories. Kept outside both static
    /// hardware inventory and the six fixed system domains.
    pub dynamic_history: DynamicTelemetryHistory,
}

impl TelemetryStore {
    /// Build distinct read-store and correlated-ingestion capabilities.
    ///
    /// Native providers receive neither the ingestion capability nor a method
    /// on the read store that can append samples. The application/frontend
    /// composition edge may retain the capability and invoke it only for
    /// outcomes accepted into `PlatformEventBatch::system_telemetry_outcomes`.
    #[must_use]
    pub fn shared_with_correlated_ingestion(
        history_capacity: usize,
    ) -> (Arc<Self>, CorrelatedSystemTelemetryIngestor) {
        let (system_history, system_ingestor) =
            CorrelatedSystemTelemetryHistory::shared(history_capacity);
        let dynamic_history = system_history.dynamic_history();
        let store = Arc::new(Self {
            system_history,
            dynamic_history,
        });
        (store, system_ingestor)
    }
}

#[cfg(test)]
#[path = "../tests/headless/telemetry_lib.rs"]
mod tests;
