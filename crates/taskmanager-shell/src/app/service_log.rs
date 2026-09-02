//! Open service-log stream state machine for the shell (ADR-027).
//!
//! Owns the renderer-neutral service-log lifecycle: opening/closing the one
//! active stream, throttled follow polling, follow/pause toggles, level/time
//! filter cycles, and folding one-shot/stream log snapshots into the shared
//! feed. The feed itself lives in `taskmanager-core`; this module is the shell
//! adapter that every frontend drives, so the TUI/Iced/gpui log panels never
//! drift apart.
use super::{ShellApp, submission_time_ms};
use taskmanager_application::{PlatformEffect, ServiceLogStreamLifecycle, ServiceLogStreamRequest};
use taskmanager_core::core::services::{
    ServiceLogEntry, ServiceLogFeed, ServiceLogLevelFilter, ServiceLogProviderState,
    ServiceLogQuery, ServiceLogState, ServiceLogStreamSnapshot, ServiceLogStreamState,
    ServiceLogTimeFilter,
};
use taskmanager_core::core::target::ServiceId;
use taskmanager_platform_contract::RequestId;

/// Minimum wall-clock gap between two incremental service-log follow requests.
/// Mirrors the gpui details panel's 1 Hz throttle; the shell owns the cadence
/// so the frontends never need a clock of their own.
pub const SERVICE_LOG_POLL_INTERVAL_MS: u64 = 1_000;

/// Cycle the open stream's level filter (All → Errors → WarningsAndErrors →
/// InfoAndAbove → All). Single renderer-neutral source for the filter cycle.
#[must_use]
pub const fn next_log_level(level: ServiceLogLevelFilter) -> ServiceLogLevelFilter {
    match level {
        ServiceLogLevelFilter::All => ServiceLogLevelFilter::Errors,
        ServiceLogLevelFilter::Errors => ServiceLogLevelFilter::WarningsAndErrors,
        ServiceLogLevelFilter::WarningsAndErrors => ServiceLogLevelFilter::InfoAndAbove,
        ServiceLogLevelFilter::InfoAndAbove => ServiceLogLevelFilter::All,
    }
}

/// Cycle a log time window (All → LastHour → LastDay → All). The single
/// source for every frontend's time control.
#[must_use]
pub const fn next_log_time(time: ServiceLogTimeFilter) -> ServiceLogTimeFilter {
    match time {
        ServiceLogTimeFilter::All => ServiceLogTimeFilter::LastHour,
        ServiceLogTimeFilter::LastHour => ServiceLogTimeFilter::LastDay,
        ServiceLogTimeFilter::LastDay => ServiceLogTimeFilter::All,
    }
}

/// One open service-log stream: the frozen service identity plus the
/// core-owned [`ServiceLogFeed`] state machine (bounded entries, follow/pause,
/// level/time filters, cursor-deduped merges). The feed lives in
/// `taskmanager-core` and is the same single source the gpui details panel
/// drives, so the TUI/Iced log panels can never drift from it.
#[derive(Clone, Debug)]
pub struct OpenServiceLog {
    pub feed: ServiceLogFeed,
    pub lifecycle: ServiceLogStreamLifecycle,
    snapshot_request_id: Option<RequestId>,
}

impl OpenServiceLog {
    /// Open a stream for `service_id` with the default follow-on filters.
    #[must_use]
    pub fn new(service_id: ServiceId) -> Self {
        Self {
            lifecycle: ServiceLogStreamLifecycle::open(service_id.clone()),
            feed: ServiceLogFeed::default(),
            snapshot_request_id: None,
        }
    }

    #[must_use]
    pub fn service_id(&self) -> Option<&ServiceId> {
        self.lifecycle.target()
    }

    pub(super) fn begin_snapshot(&mut self) {
        self.snapshot_request_id = None;
    }

    pub(super) fn accept_snapshot(&mut self, request_id: RequestId) {
        self.snapshot_request_id = Some(request_id);
    }

    fn resolve_snapshot(&mut self, request_id: RequestId) -> bool {
        if self.snapshot_request_id != Some(request_id) {
            return false;
        }
        self.snapshot_request_id = None;
        true
    }
}

impl ShellApp {
    /// Open the log stream for the selected service. The page cursor indexes
    /// the shared sorted projection (the order every frontend renders), so
    /// the visual row resolves through [`ShellApp::sorted_service_at`] — the
    /// one place "row N" becomes a service identity. Returns the initial
    /// follow request to submit; the shell keeps the frozen service identity
    /// so later updates only apply to this stream.
    #[must_use]
    pub fn open_service_log(&mut self) -> Option<PlatformEffect> {
        let service_id = self.sorted_service_at(self.selected)?.id.clone();
        self.open_service_log_for(service_id)
    }

    /// Open a log stream for a provider-order service identity. Inventory
    /// frontends use this when sorting/filtering means the visual index is not
    /// the provider index held by the shell cursor.
    #[must_use]
    pub fn open_service_log_for(&mut self, service_id: ServiceId) -> Option<PlatformEffect> {
        self.service_log = Some(OpenServiceLog::new(service_id.clone()));
        self.last_service_log_poll_ms = 0;
        Some(PlatformEffect::ServiceLogStream(ServiceLogStreamRequest {
            query: self.service_log_query(&service_id),
        }))
    }

    /// Close the open log stream. Returns nothing to submit; the provider's
    /// worker notices the open query set shrinking on its next poll.
    pub fn close_service_log(&mut self) {
        self.service_log = None;
    }

    /// The query for the open (or about-to-open) service: current filters plus
    /// the feed cursor so a follow request never re-reads entries the feed
    /// already merged.
    fn service_log_query(&self, service_id: &ServiceId) -> ServiceLogQuery {
        let (level, time, cursor) = self.service_log.as_ref().map_or(
            (ServiceLogLevelFilter::All, ServiceLogTimeFilter::All, None),
            |open| {
                (
                    open.feed.level,
                    open.feed.time,
                    open.feed.last_cursor().map(str::to_owned),
                )
            },
        );
        ServiceLogQuery {
            service_id: service_id.clone(),
            level,
            time,
            after_cursor: cursor,
        }
    }

    /// Toggle the open stream between follow-on and follow-off.
    pub fn toggle_service_log_follow(&mut self) {
        if let Some(open) = self.service_log.as_mut() {
            open.feed.follow = !open.feed.follow;
        }
    }

    /// Toggle the open stream between running and paused (paused keeps the
    /// merged entries but stops both polling and provider delivery).
    pub fn toggle_service_log_paused(&mut self) {
        if let Some(open) = self.service_log.as_mut() {
            open.feed.paused = !open.feed.paused;
        }
    }

    /// Cycle the open stream's level filter (All → Errors → WarningsAndErrors
    /// → InfoAndAbove → All). Cycles are renderer-neutral single source; the
    /// frontend only maps the active filter to its caption.
    pub fn cycle_service_log_level(&mut self) {
        if let Some(open) = self.service_log.as_mut() {
            open.feed.level = next_log_level(open.feed.level);
            if let Some(target) = open.lifecycle.target().cloned() {
                open.lifecycle = ServiceLogStreamLifecycle::open(target);
                self.last_service_log_poll_ms = 0;
            }
        }
    }

    /// Cycle the open stream's time window (All → LastHour → LastDay → All).
    pub fn cycle_service_log_time(&mut self) {
        if let Some(open) = self.service_log.as_mut() {
            open.feed.time = next_log_time(open.feed.time);
            if let Some(target) = open.lifecycle.target().cloned() {
                open.lifecycle = ServiceLogStreamLifecycle::open(target);
                self.last_service_log_poll_ms = 0;
            }
        }
    }

    /// The visible entries of the open stream after applying the level/time
    /// filters, bounded by the feed's own cursor dedup. `None` when no stream
    /// is open.
    #[must_use]
    pub fn visible_service_log_entries(&self, now_micros: u64) -> Option<Vec<&ServiceLogEntry>> {
        self.service_log
            .as_ref()
            .map(|open| open.feed.visible_entries(now_micros))
    }

    /// The open stream's provider state (availability / last-success / failure)
    /// for honest empty/unavailable rendering. `None` when no stream is open.
    #[must_use]
    pub fn service_log_provider_state(&self) -> Option<&ServiceLogProviderState> {
        self.service_log.as_ref().map(|open| &open.feed.provider)
    }

    /// Queue at most one incremental log follow per second for the open
    /// stream. Called by the frontend runtime tick (never by rendering).
    /// Returns the follow effect to submit, or `None` when throttled, paused,
    /// follow-off, or no stream is open.
    #[must_use]
    pub fn poll_service_log(&mut self, now_ms: u64) -> Option<PlatformEffect> {
        let open = self.service_log.as_mut()?;
        if open.lifecycle.is_loading() {
            return None;
        }
        if now_ms.saturating_sub(self.last_service_log_poll_ms) < SERVICE_LOG_POLL_INTERVAL_MS {
            return None;
        }
        self.last_service_log_poll_ms = now_ms;
        let service_id = open.lifecycle.target()?.clone();
        let query = open.feed.next_follow_query(&service_id)?;
        Some(PlatformEffect::ServiceLogStream(ServiceLogStreamRequest {
            query,
        }))
    }

    /// Fold a one-shot log snapshot into the open feed by building the
    /// equivalent stream-shaped state (full-filter query, no cursor).
    pub(super) fn apply_service_log_snapshot(
        &mut self,
        request_id: RequestId,
        service_id: ServiceId,
        state: ServiceLogState,
    ) {
        let Some(open) = self.service_log.as_mut() else {
            return;
        };
        if open.lifecycle.target() != Some(&service_id) || !open.resolve_snapshot(request_id) {
            return;
        }
        let entries = match state {
            ServiceLogState::Ready(lines) => lines
                .as_slice()
                .iter()
                .cloned()
                .enumerate()
                .map(|(index, message)| ServiceLogEntry {
                    cursor: format!("snapshot:{index}"),
                    realtime_timestamp_micros: None,
                    priority: None,
                    level: taskmanager_core::core::services::ServiceLogLevel::Unknown,
                    message,
                })
                .collect(),
            _ => Vec::new(),
        };
        let state = ServiceLogStreamState::from_query_entries(
            &ServiceLogQuery {
                service_id,
                level: ServiceLogLevelFilter::All,
                time: ServiceLogTimeFilter::All,
                after_cursor: None,
            },
            entries,
        );
        let Some(open_service_id) = open.lifecycle.target().cloned() else {
            return;
        };
        open.feed.apply_at(
            ServiceLogStreamSnapshot {
                query: open
                    .feed
                    .next_follow_query(&open_service_id)
                    .unwrap_or(ServiceLogQuery {
                        service_id: open_service_id,
                        level: open.feed.level,
                        time: open.feed.time,
                        after_cursor: None,
                    }),
                state,
            },
            submission_time_ms(),
        );
    }

    /// Fold a log stream state into the open feed, keyed by the service the
    /// update names. Updates for a service that is not open are dropped (the
    /// shell opens exactly one log stream at a time).
    pub(super) fn apply_service_log_update(
        &mut self,
        request_id: RequestId,
        snapshot: ServiceLogStreamSnapshot,
        observed_at_ms: u64,
    ) {
        let Some(open) = self.service_log.as_mut() else {
            return;
        };
        if !open.lifecycle.resolve(request_id, snapshot.clone()) {
            return;
        }
        open.feed.apply_at(snapshot, observed_at_ms);
    }
}
