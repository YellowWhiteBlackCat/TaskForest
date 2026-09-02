//! Shared read model for durable per-application history.
//!
//! The active frontend session persists three scalar series per stable
//! [`ApplicationHistoryIdentity`]. This module is the only place those replay
//! rows are correlated into one application row; frontends render this model
//! and never join metrics, infer identity, or consult the current process list.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;

use taskmanager_core::{ApplicationHistoryIdentity, HistoryMetric, HistoryWindow};

use crate::{HistoryReplayError, HistoryReplayRequestId, HistoryReplayRow};

/// Reader capability fixed at frontend composition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplicationHistoryCapability {
    /// The canonical continuous-history preference is off.
    Disabled,
    /// History was requested but the in-process writer/query session is unavailable.
    Unavailable(ApplicationHistoryUnavailableReason),
    /// A non-blocking connector is starting or stopping the read capability.
    Connecting,
    /// A read-only replay session is available.
    Available,
}

/// Stable visible reason a durable-history reader could not become active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplicationHistoryUnavailableReason {
    ConnectorStart,
    ConnectorBusy,
    ConnectorStopped,
    RequestSpaceExhausted,
    PersistenceWriter,
    ReplayWorker,
    ReplayRead,
}

impl ApplicationHistoryUnavailableReason {
    #[must_use]
    pub const fn stable_code(self) -> &'static str {
        match self {
            Self::ConnectorStart => "connector_start",
            Self::ConnectorBusy => "connector_busy",
            Self::ConnectorStopped => "connector_stopped",
            Self::RequestSpaceExhausted => "request_space_exhausted",
            Self::PersistenceWriter => "persistence_writer",
            Self::ReplayWorker => "replay_worker",
            Self::ReplayRead => "replay_read",
        }
    }
}

/// User-visible durable-history lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplicationHistoryStatus {
    Disabled,
    Unavailable,
    Connecting,
    /// The reader is active but no persisted application series exists yet.
    Collecting,
    Ready,
}

/// One persisted metric series and its fact-only window summary.
#[derive(Clone, Debug, PartialEq)]
pub struct ApplicationHistoryMetricSeries {
    pub samples: Arc<[f32]>,
    pub sample_times_ms: Arc<[u64]>,
    pub peak_value: Option<f64>,
    pub peak_measured_at_ms: Option<u64>,
    pub observed: usize,
    pub gaps: usize,
    pub clock_jumps: u32,
}

impl From<&HistoryReplayRow> for ApplicationHistoryMetricSeries {
    fn from(row: &HistoryReplayRow) -> Self {
        Self {
            samples: Arc::clone(&row.samples),
            sample_times_ms: Arc::clone(&row.sample_times_ms),
            peak_value: row.peak_value,
            peak_measured_at_ms: row.peak_measured_at_ms,
            observed: row.observed,
            gaps: row.gaps,
            clock_jumps: row.clock_jumps,
        }
    }
}

impl ApplicationHistoryMetricSeries {
    /// Samples with explicit discontinuities inserted for recording downtime,
    /// clock reversal, or another interval far beyond the replay series'
    /// normal downsampled cadence. The original persisted gaps remain `NaN`.
    #[must_use]
    pub fn gap_aware_samples(&self) -> Arc<[f32]> {
        if self.samples.len() < 2 || self.samples.len() != self.sample_times_ms.len() {
            return Arc::clone(&self.samples);
        }
        let mut intervals = self
            .sample_times_ms
            .windows(2)
            .filter_map(|times| times[1].checked_sub(times[0]))
            .filter(|interval| *interval > 0)
            .collect::<Vec<_>>();
        intervals.sort_unstable();
        // Lower median: with one normal interval and one downtime interval,
        // the larger outage must not become the inferred cadence.
        let discontinuity = intervals
            .get((intervals.len().saturating_sub(1)) / 2)
            .copied()
            .map_or(u64::MAX, |cadence| cadence.saturating_mul(3));
        let absolute_discontinuity = u64::try_from(crate::MAX_TELEMETRY_INTERVAL.as_millis())
            .unwrap_or(u64::MAX)
            .saturating_mul(3);
        let mut projected = Vec::with_capacity(self.samples.len().saturating_mul(2));
        projected.push(self.samples[0]);
        for index in 1..self.samples.len() {
            let previous = self.sample_times_ms[index - 1];
            let current = self.sample_times_ms[index];
            let interval = current.saturating_sub(previous);
            if current <= previous || interval > discontinuity || interval > absolute_discontinuity
            {
                projected.push(f32::NAN);
            }
            projected.push(self.samples[index]);
        }
        Arc::from(projected)
    }
}

/// All durable series for one stable application identity.
#[derive(Clone, Debug, PartialEq)]
pub struct ApplicationHistoryRow {
    pub identity: ApplicationHistoryIdentity,
    pub cpu_usage: Option<ApplicationHistoryMetricSeries>,
    pub memory: Option<ApplicationHistoryMetricSeries>,
    pub process_count: Option<ApplicationHistoryMetricSeries>,
}

impl ApplicationHistoryRow {
    #[must_use]
    pub fn display_name(&self) -> &str {
        self.identity.value()
    }

    #[must_use]
    pub fn peak_cpu_usage_pct(&self) -> Option<f64> {
        self.cpu_usage.as_ref().and_then(|series| series.peak_value)
    }

    #[must_use]
    pub fn peak_memory_bytes(&self) -> Option<f64> {
        self.memory.as_ref().and_then(|series| series.peak_value)
    }

    #[must_use]
    pub fn peak_process_count(&self) -> Option<f64> {
        self.process_count
            .as_ref()
            .and_then(|series| series.peak_value)
    }
}

/// Immutable page projection shared by GPUI, Iced, TUI and Bevy.
#[derive(Clone, Debug, PartialEq)]
pub struct ApplicationHistoryProjection {
    pub status: ApplicationHistoryStatus,
    pub selected_window: HistoryWindow,
    /// Window that actually produced `rows`; differs during a refresh failure
    /// when last-good evidence remains visible.
    pub rows_window: Option<HistoryWindow>,
    pub rows: Arc<[ApplicationHistoryRow]>,
    pub source_request: Option<HistoryReplayRequestId>,
    pub refreshing: bool,
    pub failure: Option<HistoryReplayError>,
    pub unavailable_reason: Option<ApplicationHistoryUnavailableReason>,
    pub loaded_at_ms: Option<u64>,
}

impl ApplicationHistoryProjection {
    #[must_use]
    pub(crate) fn from_replay(snapshot: ApplicationHistoryReplaySnapshot) -> Self {
        let ApplicationHistoryReplaySnapshot {
            capability,
            selected_window,
            rows_window,
            rows: replay_rows,
            source_request,
            refreshing,
            failure,
            loaded_at_ms,
        } = snapshot;
        let (status, rows, unavailable_reason) = match capability {
            ApplicationHistoryCapability::Disabled => (
                ApplicationHistoryStatus::Disabled,
                Arc::<[ApplicationHistoryRow]>::from([]),
                None,
            ),
            ApplicationHistoryCapability::Unavailable(reason) => (
                ApplicationHistoryStatus::Unavailable,
                Arc::<[ApplicationHistoryRow]>::from([]),
                Some(reason),
            ),
            ApplicationHistoryCapability::Connecting => (
                ApplicationHistoryStatus::Connecting,
                Arc::<[ApplicationHistoryRow]>::from([]),
                None,
            ),
            ApplicationHistoryCapability::Available if !replay_rows.is_empty() => {
                (ApplicationHistoryStatus::Ready, replay_rows, None)
            }
            ApplicationHistoryCapability::Available if failure.is_some() => (
                ApplicationHistoryStatus::Unavailable,
                Arc::<[ApplicationHistoryRow]>::from([]),
                Some(ApplicationHistoryUnavailableReason::ReplayRead),
            ),
            ApplicationHistoryCapability::Available => (
                ApplicationHistoryStatus::Collecting,
                Arc::<[ApplicationHistoryRow]>::from([]),
                None,
            ),
        };
        Self {
            status,
            selected_window,
            rows_window,
            rows,
            source_request,
            refreshing,
            failure,
            unavailable_reason,
            loaded_at_ms,
        }
    }
}

pub(crate) struct ApplicationHistoryReplaySnapshot {
    pub capability: ApplicationHistoryCapability,
    pub selected_window: HistoryWindow,
    pub rows_window: Option<HistoryWindow>,
    pub rows: Arc<[ApplicationHistoryRow]>,
    pub source_request: Option<HistoryReplayRequestId>,
    pub refreshing: bool,
    pub failure: Option<HistoryReplayError>,
    pub loaded_at_ms: Option<u64>,
}

#[derive(Default)]
struct RowBuilder {
    cpu_usage: Option<ApplicationHistoryMetricSeries>,
    memory: Option<ApplicationHistoryMetricSeries>,
    process_count: Option<ApplicationHistoryMetricSeries>,
}

/// Correlate canonical replay rows into one stable-identity application row.
/// Non-application metrics and malformed application scopes are ignored.
#[must_use]
pub(crate) fn project_application_history_rows(
    replay_rows: &[HistoryReplayRow],
) -> Arc<[ApplicationHistoryRow]> {
    let mut relevant = replay_rows
        .iter()
        .filter_map(|row| {
            let identity = row.key.application()?;
            let metric_order = match row.key.metric() {
                HistoryMetric::ApplicationCpuUsagePct => 0_u8,
                HistoryMetric::ApplicationMemoryBytes => 1,
                HistoryMetric::ApplicationProcessCount => 2,
                _ => return None,
            };
            Some((identity, metric_order, row))
        })
        .collect::<Vec<_>>();
    relevant.sort_by(
        |(left_identity, left_metric, _), (right_identity, right_metric, _)| {
            identity_order(left_identity)
                .cmp(&identity_order(right_identity))
                .then_with(|| left_metric.cmp(right_metric))
        },
    );

    let mut order = Vec::<ApplicationHistoryIdentity>::new();
    let mut builders = HashMap::<ApplicationHistoryIdentity, RowBuilder>::new();
    for (identity, _, replay) in relevant {
        let builder = match builders.entry(identity.clone()) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                order.push(identity.clone());
                entry.insert(RowBuilder::default())
            }
        };
        let series = ApplicationHistoryMetricSeries::from(replay);
        match replay.key.metric() {
            HistoryMetric::ApplicationCpuUsagePct => builder.cpu_usage = Some(series),
            HistoryMetric::ApplicationMemoryBytes => builder.memory = Some(series),
            HistoryMetric::ApplicationProcessCount => builder.process_count = Some(series),
            _ => {}
        }
    }

    let mut rows = order
        .into_iter()
        .filter_map(|identity| {
            builders
                .remove(&identity)
                .map(|builder| ApplicationHistoryRow {
                    identity,
                    cpu_usage: builder.cpu_usage,
                    memory: builder.memory,
                    process_count: builder.process_count,
                })
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        compare_optional_desc(left.peak_cpu_usage_pct(), right.peak_cpu_usage_pct())
            .then_with(|| identity_order(&left.identity).cmp(&identity_order(&right.identity)))
    });
    Arc::from(rows)
}

fn compare_optional_desc(left: Option<f64>, right: Option<f64>) -> Ordering {
    match (
        left.filter(|value| value.is_finite()),
        right.filter(|value| value.is_finite()),
    ) {
        (Some(left), Some(right)) => right.partial_cmp(&left).unwrap_or(Ordering::Equal),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn identity_order(identity: &ApplicationHistoryIdentity) -> (u8, &str) {
    (if identity.is_verified() { 0 } else { 1 }, identity.value())
}

#[cfg(test)]
#[path = "../tests/headless/application_application_history_projection_tests.rs"]
mod tests;
