//! Correlated ring reads and transaction-owned append operations.

use super::{
    BoundedHistory, CorrelatedMetricHistory, CorrelatedMetricSample, CorrelatedTelemetryStamp,
    lock_unpoisoned,
};
use ringbuf::traits::{Consumer, Observer};
use std::sync::{Arc, Mutex};

impl<T> CorrelatedMetricHistory<T> {
    pub(super) fn new(capacity: usize, commit_gate: Arc<Mutex<()>>) -> Self {
        Self {
            inner: Arc::new(BoundedHistory::new(capacity, commit_gate)),
        }
    }

    pub(super) fn push(
        &self,
        stamp: CorrelatedTelemetryStamp,
        measured_at_ms: Option<u64>,
        value: Option<T>,
    ) {
        self.inner.push_in_transaction(CorrelatedMetricSample {
            stamp,
            measured_at_ms,
            value,
        });
    }

    /// Latest real sampling time while the caller already owns this ring's
    /// domain commit gate. This is deliberately not a public read path: taking
    /// the gate again from freshness fan-out would deadlock the transaction.
    pub(super) fn latest_measured_at_in_transaction(&self) -> Option<u64> {
        self.inner
            .buffer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .rev()
            .filter_map(|sample| sample.measured_at_ms)
            .next()
    }

    #[must_use]
    pub fn samples(&self) -> Vec<CorrelatedMetricSample<T>>
    where
        T: Clone,
    {
        self.inner.samples()
    }

    /// Clone only the requested newest tail, preserving accepted order and gaps.
    #[must_use]
    pub fn tail_samples(&self, limit: usize) -> Vec<CorrelatedMetricSample<T>>
    where
        T: Clone,
    {
        self.inner.tail_samples(limit)
    }

    #[must_use]
    pub fn capacity(&self) -> usize {
        self.inner.capacity
    }

    /// Stable identity of the underlying storage (the shared buffer's
    /// address). Derived-projection caches include it so two distinct
    /// histories that happen to agree on `(len, revision)` can never serve
    /// each other's cached vector.
    #[must_use]
    pub fn ring_id(&self) -> usize {
        Arc::as_ptr(&self.inner) as usize
    }

    /// Latest accepted completion without cloning the retained sample window.
    #[must_use]
    pub fn latest_completion_ms(&self) -> Option<u64> {
        let _commit = lock_unpoisoned(&self.inner.commit_gate);
        let buffer = self
            .inner
            .buffer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        buffer
            .iter()
            .next_back()
            .map(|sample| sample.stamp.completed_at_ms())
    }

    /// Content watermark: `(len, latest_revision)`. Two histories with the
    /// same watermark cannot differ, so derived projections (graph sample
    /// vectors) can key their caches on it and skip the sample clone entirely.
    /// Reads the lock without cloning any sample.
    #[must_use]
    pub fn watermark(&self) -> (usize, Option<u64>) {
        let _commit = lock_unpoisoned(&self.inner.commit_gate);
        let buffer = self
            .inner
            .buffer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            buffer.occupied_len(),
            buffer
                .iter()
                .next_back()
                .map(|sample| sample.stamp.revision()),
        )
    }
}
