//! Immutable persisted system-history review over the canonical replay controller.

use crate::{
    ApplicationHistoryStatus, ApplicationHistoryUnavailableReason, HistoryReplayError,
    HistoryReplayRequestId, HistoryReplayRow,
};
use std::sync::Arc;
use taskmanager_core::core::history::HistoryWindow;

#[derive(Clone, Debug, PartialEq)]
pub struct PerformanceHistoryProjection {
    pub status: ApplicationHistoryStatus,
    pub selected_window: HistoryWindow,
    pub rows_window: Option<HistoryWindow>,
    pub rows: Arc<[HistoryReplayRow]>,
    pub source_request: Option<HistoryReplayRequestId>,
    pub refreshing: bool,
    pub failure: Option<HistoryReplayError>,
    pub loaded_at_ms: Option<u64>,
    pub unavailable_reason: Option<ApplicationHistoryUnavailableReason>,
}

impl PerformanceHistoryProjection {
    #[must_use]
    pub fn stale(&self) -> bool {
        !self.rows.is_empty()
            && (self.refreshing
                || self.rows_window != Some(self.selected_window)
                || self.failure.is_some())
    }
}
