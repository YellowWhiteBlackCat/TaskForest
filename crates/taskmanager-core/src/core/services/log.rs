//! Platform-neutral service-log contracts and feed state.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::core::{FailureKind, ServiceId};

/// Maximum number of structured entries retained by one open follow feed.
/// Provider batches remain small, while this cap keeps a noisy long-lived
/// service-log panel from turning the shell and every frontend into an
/// unbounded append-only buffer.
const SERVICE_LOG_FEED_CAPACITY: usize = 2_000;

/// Coarse availability of a service-log provider. `Stale` means previously
/// collected entries remain usable after a failed refresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ServiceLogAvailability {
    #[default]
    Loading,
    Available,
    Empty,
    CaughtUp,
    Disconnected,
    Unavailable,
    Stale,
}

/// Stable, presentation-independent reason a service-log provider is
/// unavailable. Provider diagnostics belong in [`ServiceLogFailure::detail`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceLogErrorKind {
    MissingTool,
    PermissionDenied,
    Unsupported,
    TimedOut,
    TemporarilyUnavailable,
    ProviderFailed,
}

impl ServiceLogErrorKind {
    #[must_use]
    pub const fn from_failure(failure: FailureKind) -> Self {
        match failure {
            // RequiresEscalation is an escalatable denial; the service-log
            // vocabulary has no escalation token, so fold it into PermissionDenied.
            FailureKind::PermissionDenied | FailureKind::RequiresEscalation => {
                Self::PermissionDenied
            }
            FailureKind::MissingDependency => Self::MissingTool,
            FailureKind::TimedOut => Self::TimedOut,
            FailureKind::Unsupported => Self::Unsupported,
            FailureKind::TemporarilyUnavailable => Self::TemporarilyUnavailable,
            FailureKind::Rejected | FailureKind::IdentityChanged | FailureKind::ProviderFault => {
                Self::ProviderFailed
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceLogFailure {
    pub kind: ServiceLogErrorKind,
    pub detail: Option<String>,
}

impl ServiceLogFailure {
    #[must_use]
    pub fn with_detail(kind: ServiceLogErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: Some(detail.into()),
        }
    }
}

/// Refresh lifecycle for an incremental service-log feed. A failure preserves
/// `last_success_ms`; callers can therefore keep the last trustworthy entries
/// and cursor while exposing a typed stale state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ServiceLogProviderState {
    pub availability: ServiceLogAvailability,
    pub failure: Option<ServiceLogFailure>,
    #[serde(default)]
    pub stream_end: Option<ServiceLogStreamEnd>,
    pub last_success_ms: Option<u64>,
}

impl ServiceLogProviderState {
    pub fn observe_success(&mut self, is_empty: bool, now_ms: u64) {
        self.availability = if is_empty {
            ServiceLogAvailability::Empty
        } else {
            ServiceLogAvailability::Available
        };
        self.failure = None;
        self.stream_end = None;
        self.last_success_ms = Some(now_ms);
    }

    pub fn observe_failure(&mut self, failure: ServiceLogFailure) {
        self.availability = if self.last_success_ms.is_some() {
            ServiceLogAvailability::Stale
        } else {
            ServiceLogAvailability::Unavailable
        };
        self.failure = Some(failure);
        self.stream_end = None;
    }

    pub fn observe_stream_end(&mut self, end: ServiceLogStreamEnd, now_ms: u64) {
        match &end {
            ServiceLogStreamEnd::CaughtUp => {
                self.availability = ServiceLogAvailability::CaughtUp;
                self.failure = None;
                self.last_success_ms = Some(now_ms);
            }
            ServiceLogStreamEnd::Disconnected { .. } => {
                self.availability = if self.last_success_ms.is_some() {
                    ServiceLogAvailability::Stale
                } else {
                    ServiceLogAvailability::Disconnected
                };
                self.failure = None;
            }
        }
        self.stream_end = Some(end);
    }
}

/// A non-empty batch of human-readable service-log lines.
///
/// The inner vector is private so `ServiceLogState::Ready` can never carry a
/// believable success with no data. Providers must use
/// [`ServiceLogState::from_lines`] to classify a trustworthy empty result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceLogLines(Vec<String>);

impl ServiceLogLines {
    #[must_use]
    pub fn new(lines: Vec<String>) -> Option<Self> {
        (!lines.is_empty()).then_some(Self(lines))
    }

    #[must_use]
    pub fn as_slice(&self) -> &[String] {
        &self.0
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn iter(&self) -> std::slice::Iter<'_, String> {
        self.0.iter()
    }
}

impl std::ops::Deref for ServiceLogLines {
    type Target = [String];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

/// Result state displayed by the service-details Logs section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceLogState {
    Loading,
    Ready(ServiceLogLines),
    Empty,
    Unavailable(ServiceLogFailure),
}

impl ServiceLogState {
    #[must_use]
    pub fn from_lines(lines: Vec<String>) -> Self {
        ServiceLogLines::new(lines).map_or(Self::Empty, Self::Ready)
    }

    #[must_use]
    pub const fn availability(&self) -> ServiceLogAvailability {
        match self {
            Self::Loading => ServiceLogAvailability::Loading,
            Self::Ready(_) => ServiceLogAvailability::Available,
            Self::Empty => ServiceLogAvailability::Empty,
            Self::Unavailable(_) => ServiceLogAvailability::Unavailable,
        }
    }

    pub fn copy_text(&self) -> Option<String> {
        match self {
            Self::Ready(lines) => Some(lines.as_slice().join("\n")),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceLogSnapshot {
    pub service_id: ServiceId,
    pub state: ServiceLogState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceLogLevel {
    Error,
    Warning,
    Info,
    Debug,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ServiceLogLevelFilter {
    #[default]
    All,
    Errors,
    WarningsAndErrors,
    InfoAndAbove,
}

impl ServiceLogLevelFilter {
    #[must_use]
    pub fn matches(self, priority: Option<u8>) -> bool {
        match self {
            Self::All => true,
            Self::Errors => priority.is_some_and(|priority| priority <= 3),
            Self::WarningsAndErrors => priority.is_some_and(|priority| priority <= 4),
            Self::InfoAndAbove => priority.is_some_and(|priority| priority <= 6),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ServiceLogTimeFilter {
    #[default]
    All,
    LastHour,
    LastDay,
}

impl ServiceLogTimeFilter {
    #[must_use]
    pub fn matches(self, realtime_timestamp_micros: Option<u64>, now_micros: u64) -> bool {
        let Some(timestamp) = realtime_timestamp_micros else {
            return self == Self::All;
        };
        let window = match self {
            Self::All => return true,
            Self::LastHour => 60 * 60 * 1_000_000,
            Self::LastDay => 24 * 60 * 60 * 1_000_000,
        };
        now_micros.saturating_sub(timestamp) <= window
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceLogEntry {
    pub cursor: String,
    pub realtime_timestamp_micros: Option<u64>,
    pub priority: Option<u8>,
    pub level: ServiceLogLevel,
    pub message: String,
}

/// A non-empty batch of structured service-log entries.
///
/// Keeping the vector private prevents `Ready(empty)` at every provider,
/// runtime, and frontend boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceLogEntries(Vec<ServiceLogEntry>);

impl ServiceLogEntries {
    #[must_use]
    pub fn new(entries: Vec<ServiceLogEntry>) -> Option<Self> {
        (!entries.is_empty()).then_some(Self(entries))
    }

    #[must_use]
    pub fn as_slice(&self) -> &[ServiceLogEntry] {
        &self.0
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn into_vec(self) -> Vec<ServiceLogEntry> {
        self.0
    }
}

impl std::ops::Deref for ServiceLogEntries {
    type Target = [ServiceLogEntry];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceLogQuery {
    pub service_id: ServiceId,
    pub level: ServiceLogLevelFilter,
    pub time: ServiceLogTimeFilter,
    pub after_cursor: Option<String>,
}

/// Why an incremental stream ended without returning another entry batch.
///
/// `CaughtUp` is a successful journal EOF for an already-positioned cursor.
/// `Disconnected` means the transport/worker vanished and is not a provider
/// success or evidence that the service has no logs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum ServiceLogStreamEnd {
    CaughtUp,
    Disconnected { detail: Option<String> },
}

impl ServiceLogStreamEnd {
    #[must_use]
    pub fn disconnected(detail: impl Into<String>) -> Self {
        Self::Disconnected {
            detail: Some(detail.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceLogStreamState {
    Loading,
    Ready(ServiceLogEntries),
    Empty,
    Ended(ServiceLogStreamEnd),
    Unavailable(ServiceLogFailure),
}

impl ServiceLogStreamState {
    #[must_use]
    pub fn from_query_entries(query: &ServiceLogQuery, entries: Vec<ServiceLogEntry>) -> Self {
        match ServiceLogEntries::new(entries) {
            Some(entries) => Self::Ready(entries),
            None if query.after_cursor.is_some() => Self::Ended(ServiceLogStreamEnd::CaughtUp),
            None => Self::Empty,
        }
    }

    #[must_use]
    pub const fn availability(&self) -> ServiceLogAvailability {
        match self {
            Self::Loading => ServiceLogAvailability::Loading,
            Self::Ready(_) => ServiceLogAvailability::Available,
            Self::Empty => ServiceLogAvailability::Empty,
            Self::Ended(ServiceLogStreamEnd::CaughtUp) => ServiceLogAvailability::CaughtUp,
            Self::Ended(ServiceLogStreamEnd::Disconnected { .. }) => {
                ServiceLogAvailability::Disconnected
            }
            Self::Unavailable(_) => ServiceLogAvailability::Unavailable,
        }
    }

    #[must_use]
    pub fn failure(&self) -> Option<ServiceLogFailure> {
        match self {
            Self::Unavailable(failure) => Some(failure.clone()),
            Self::Ended(ServiceLogStreamEnd::Disconnected { detail }) => Some(ServiceLogFailure {
                kind: ServiceLogErrorKind::TemporarilyUnavailable,
                detail: detail.clone(),
            }),
            Self::Loading
            | Self::Ready(_)
            | Self::Empty
            | Self::Ended(ServiceLogStreamEnd::CaughtUp) => None,
        }
    }

    /// Resolve a structured stream and a bounded text snapshot into the single
    /// state consumed by any frontend. Formatting entries into `stream_lines`
    /// remains presentation-owned; lifecycle precedence does not.
    #[must_use]
    pub fn resolve_lines(
        &self,
        snapshot: &ServiceLogState,
        stream_lines: Vec<String>,
    ) -> ServiceLogState {
        if let Some(lines) = ServiceLogLines::new(stream_lines) {
            return ServiceLogState::Ready(lines);
        }
        if matches!(self, Self::Loading) && matches!(snapshot, ServiceLogState::Empty) {
            ServiceLogState::Loading
        } else if let Some(failure) = self.failure() {
            ServiceLogState::Unavailable(failure)
        } else {
            snapshot.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceLogStreamSnapshot {
    pub query: ServiceLogQuery,
    pub state: ServiceLogStreamState,
}

/// UI-owned incremental feed state, independent of the provider that produced
/// each snapshot.
#[derive(Debug, Clone)]
pub struct ServiceLogFeed {
    entries: Vec<ServiceLogEntry>,
    seen_cursors: HashSet<String>,
    last_cursor: Option<String>,
    pub provider: ServiceLogProviderState,
    pub follow: bool,
    pub paused: bool,
    pub level: ServiceLogLevelFilter,
    pub time: ServiceLogTimeFilter,
}

impl Default for ServiceLogFeed {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            seen_cursors: HashSet::new(),
            last_cursor: None,
            provider: ServiceLogProviderState::default(),
            follow: true,
            paused: false,
            level: ServiceLogLevelFilter::All,
            time: ServiceLogTimeFilter::All,
        }
    }
}

impl ServiceLogFeed {
    #[must_use]
    pub fn next_follow_query(&self, service_id: &ServiceId) -> Option<ServiceLogQuery> {
        if !self.follow || self.paused {
            return None;
        }
        Some(ServiceLogQuery {
            service_id: service_id.clone(),
            level: self.level,
            time: self.time,
            after_cursor: self
                .last_cursor
                .clone()
                .or_else(|| self.entries.last().map(|entry| entry.cursor.clone())),
        })
    }

    /// Retained entries in chronological provider order. The slice is bounded
    /// to the feed's fixed display capacity.
    #[must_use]
    pub fn entries(&self) -> &[ServiceLogEntry] {
        &self.entries
    }

    /// The newest provider cursor, retained independently from the bounded
    /// display window so trimming old rows never causes a follow request to
    /// replay them.
    #[must_use]
    pub fn last_cursor(&self) -> Option<&str> {
        self.last_cursor
            .as_deref()
            .or_else(|| self.entries.last().map(|entry| entry.cursor.as_str()))
    }

    pub fn apply_at(&mut self, snapshot: ServiceLogStreamSnapshot, now_ms: u64) {
        if self.paused {
            return;
        }
        match snapshot.state {
            ServiceLogStreamState::Ready(incoming) => {
                // `entries` used to be the dedup index, which rebuilt and
                // cloned every retained cursor for every follow batch. Keep a
                // bounded live index instead; evicted cursors leave the index
                // together with their rows.
                if self.seen_cursors.is_empty() {
                    self.seen_cursors
                        .extend(self.entries.iter().map(|entry| entry.cursor.clone()));
                }
                if self.last_cursor.is_none() {
                    self.last_cursor = self.entries.last().map(|entry| entry.cursor.clone());
                }
                for entry in incoming.into_vec() {
                    let cursor = entry.cursor.clone();
                    if self.seen_cursors.insert(cursor.clone()) {
                        self.last_cursor = Some(cursor);
                        self.entries.push(entry);
                    }
                }
                let overflow = self.entries.len().saturating_sub(SERVICE_LOG_FEED_CAPACITY);
                if overflow > 0 {
                    for entry in self.entries.drain(..overflow) {
                        self.seen_cursors.remove(entry.cursor.as_str());
                    }
                }
                self.provider
                    .observe_success(self.entries.is_empty(), now_ms);
            }
            ServiceLogStreamState::Empty => self
                .provider
                .observe_success(self.entries.is_empty(), now_ms),
            ServiceLogStreamState::Ended(end) => {
                self.provider.observe_stream_end(end, now_ms);
            }
            ServiceLogStreamState::Unavailable(failure) => {
                self.provider.observe_failure(failure);
            }
            ServiceLogStreamState::Loading if self.provider.last_success_ms.is_none() => {
                self.provider = ServiceLogProviderState::default();
            }
            ServiceLogStreamState::Loading => {}
        }
    }

    #[must_use]
    pub fn visible_entries(&self, now_micros: u64) -> Vec<&ServiceLogEntry> {
        self.entries
            .iter()
            .filter(|entry| self.level.matches(entry.priority))
            .filter(|entry| {
                self.time
                    .matches(entry.realtime_timestamp_micros, now_micros)
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "../../../tests/headless/core_core_services_log_tests.rs"]
mod tests;
