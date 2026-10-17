//! Background telemetry persistence manager wrapping PersistentHistoryStore.

use std::fs;
#[cfg(target_os = "linux")]
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;

use taskmanager_core::{HistoricalSample, HistorySeriesKey};
use taskmanager_history_store::{
    HistoryQuery, HistoryStoreError, HistoryStoreErrorKind, PersistentHistoryStore,
    RecordSampleOutcome, RetentionPolicy,
};

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
        let store = PersistentHistoryStore::open(&root, policy, default_holder_is_gone)?;

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
        if let Ok(guard) = self.store.lock() {
            if let Some(store) = guard.as_ref() {
                return Ok(store.try_record_sample(key, sample));
            }
        }
        Err(HistoryStoreError::new(
            HistoryStoreErrorKind::Write,
            "History store closed or mutex poisoned".to_string(),
        ))
    }

    /// Flush in-memory buffered samples to disk files.
    pub fn flush(&self, now_ms: u64) -> Result<(), HistoryStoreError> {
        if let Ok(guard) = self.store.lock() {
            if let Some(store) = guard.as_ref() {
                let _report = store.flush(now_ms)?;
                return Ok(());
            }
        }
        Ok(())
    }

    /// Return a read-only query client for historical data.
    #[must_use]
    pub fn query(&self) -> HistoryQuery {
        HistoryQuery::new(self.root_dir.clone())
    }

    /// Gracefully drop and release writer lock.
    pub fn close(&self) {
        if let Ok(mut guard) = self.store.lock() {
            if let Some(store) = guard.take() {
                let _ = store.flush(0);
            }
        }
    }
}

impl Drop for DaemonHistoryManager {
    fn drop(&mut self) {
        self.close();
    }
}

/// Safe platform-neutral PID liveness check.
fn default_holder_is_gone(pid: u32) -> bool {
    if pid == 0 || pid == std::process::id() {
        return false;
    }

    #[cfg(target_os = "linux")]
    {
        // On Linux, check whether /proc/<pid> exists
        !Path::new("/proc").join(pid.to_string()).exists()
    }

    #[cfg(not(target_os = "linux"))]
    {
        // In other environments, assume holder is gone if not current pid
        true
    }
}
