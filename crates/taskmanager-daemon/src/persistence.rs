//! Background telemetry persistence manager wrapping PersistentHistoryStore.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use taskmanager_core::{HistoricalSample, HistoryMetric, HistorySeriesKey, HistoryWindow};
use taskmanager_history_store::{
    HistoryQuery, HistoryStoreError, HistoryStoreErrorKind, PersistentHistoryStore,
    RecordSampleOutcome, RetentionPolicy,
};
use taskmanager_ipc::{HistoryProvider, SubscriptionMask};
use taskmanager_platform_native::history_lock_holder_is_gone;

/// Daemon persistence manager owning the background rolling telemetry store.
pub struct DaemonHistoryManager {
    store: Mutex<Option<PersistentHistoryStore>>,
    root_dir: PathBuf,
}

impl DaemonHistoryManager {
    /// Initialize the history manager, creating the directory if missing and acquiring the single-writer lock.
    pub fn open(root_dir: impl Into<PathBuf>) -> Result<Self, HistoryStoreError> {
        let root = root_dir.into();
        fs::create_dir_all(&root).map_err(|error| {
            HistoryStoreError::new(
                HistoryStoreErrorKind::CreateDirectory,
                format!("{}: {error}", root.display()),
            )
        })?;

        let policy = RetentionPolicy::default();
        let store = PersistentHistoryStore::open(&root, policy, history_lock_holder_is_gone)?;

        Ok(Self {
            store: Mutex::new(Some(store)),
            root_dir: root,
        })
    }

    /// Record one telemetry scalar sample into the persistent rolling buffer.
    pub fn record_sample(
        &self,
        key: HistorySeriesKey,
        sample: HistoricalSample,
    ) -> Result<RecordSampleOutcome, HistoryStoreError> {
        if let Ok(guard) = self.store.lock()
            && let Some(store) = guard.as_ref()
        {
            return Ok(store.try_record_sample(key, sample));
        }
        Err(HistoryStoreError::new(
            HistoryStoreErrorKind::Write,
            "History store closed or mutex poisoned".to_string(),
        ))
    }

    /// Flush in-memory buffered samples to disk files.
    pub fn flush(&self, now_ms: u64) -> Result<(), HistoryStoreError> {
        if let Ok(guard) = self.store.lock()
            && let Some(store) = guard.as_ref()
        {
            let _report = store.flush(now_ms)?;
            return Ok(());
        }
        Ok(())
    }

    /// Return a read-only query client for historical data.
    #[must_use]
    pub fn query(&self) -> HistoryQuery {
        HistoryQuery::new(self.root_dir.clone())
    }

    /// Query historical samples for a given facet within the specified time range.
    pub fn query_facet_range(
        &self,
        facet_mask: u32,
        start_ms: u64,
        end_ms: u64,
    ) -> (usize, String) {
        let key = if facet_mask & SubscriptionMask::CPU.bits() != 0 {
            HistorySeriesKey::system(HistoryMetric::CpuUsagePct)
        } else if facet_mask & SubscriptionMask::MEMORY.bits() != 0 {
            HistorySeriesKey::system(HistoryMetric::MemoryUsedPct)
        } else {
            return (0, "[]".to_string());
        };

        let duration = end_ms.saturating_sub(start_ms);
        let window = if duration <= HistoryWindow::OneHour.duration_ms() {
            HistoryWindow::OneHour
        } else if duration <= HistoryWindow::TwentyFourHours.duration_ms() {
            HistoryWindow::TwentyFourHours
        } else {
            HistoryWindow::SevenDays
        };

        let query = self.query();
        if let Ok(Some(read)) = query.series(&key, window, end_ms) {
            let filtered: Vec<HistoricalSample> = read
                .series
                .samples
                .into_iter()
                .filter(|s| s.completed_at_ms >= start_ms && s.completed_at_ms <= end_ms)
                .collect();
            let count = filtered.len();
            let json = serde_json::to_string(&filtered).unwrap_or_else(|_| "[]".to_string());
            (count, json)
        } else {
            (0, "[]".to_string())
        }
    }

    /// Gracefully drop and release writer lock.
    pub fn close(&self) {
        if let Ok(mut guard) = self.store.lock()
            && let Some(store) = guard.take()
        {
            let _ = store.flush(0);
        }
    }
}

impl Drop for DaemonHistoryManager {
    fn drop(&mut self) {
        self.close();
    }
}

impl HistoryProvider for DaemonHistoryManager {
    fn query_history(&self, facet_mask: u32, start_ms: u64, end_ms: u64) -> (usize, String) {
        self.query_facet_range(facet_mask, start_ms, end_ms)
    }
}
