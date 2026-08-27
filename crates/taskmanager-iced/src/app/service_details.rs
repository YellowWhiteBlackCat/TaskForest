//! Iced-owned service-details state and request routing.
//!
//! The dependency query is a typed application/platform effect. This module
//! owns the renderer-local log feed and projects the shared dependency/log
//! lifecycles; provider I/O never runs from the Iced view.

use iced::Task;
use taskmanager_application::{
    FailureKind, RequestId, ServiceDependenciesLifecycle, ServiceDeps, ServiceId,
    ServiceLogEntries, ServiceLogEntry, ServiceLogErrorKind, ServiceLogFailure, ServiceLogFeed,
    ServiceLogLevel, ServiceLogLevelFilter, ServiceLogQuery, ServiceLogState,
    ServiceLogStreamLifecycle, ServiceLogStreamRequest, ServiceLogStreamSnapshot,
    ServiceLogStreamState, ServiceLogTimeFilter, ServiceRelationEdge, ServiceRelationGraph,
    ServiceRelationKind, ServiceUpdate,
};
use taskmanager_shell::app::service_log::{
    SERVICE_LOG_POLL_INTERVAL_MS, next_log_level, next_log_time,
};
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ShellApp};

use super::{IcedApp, LocalSurface, Message, PlatformEffect};

#[derive(Clone, Debug, PartialEq)]
pub struct ServiceDetailsSnapshot {
    pub(crate) dependencies: ServiceDependenciesLifecycle,
    /// The merged log panel's resolved state (stream lines preferred, the
    /// one-shot snapshot state as fallback) — GPUI `resolve_lines` parity.
    pub(crate) logs: ServiceLogState,
    /// The merged log panel's feed controls (paused + active filters), so the
    /// view captions the controls without touching live state.
    pub(crate) log_paused: bool,
    pub(crate) log_level: ServiceLogLevelFilter,
    pub(crate) log_time: ServiceLogTimeFilter,
}

#[derive(Clone, Debug)]
pub(crate) struct ServiceDetailsState {
    /// One-shot log snapshot state (fallback when the stream has no lines).
    logs: ServiceLogState,
    /// The merged log panel's live stream feed (bounded entries, filters,
    /// pause) — the same core state machine the shell overlay owns.
    feed: ServiceLogFeed,
    stream: ServiceLogStreamLifecycle,
    last_stream_poll_ms: u64,
}

impl Default for ServiceDetailsState {
    fn default() -> Self {
        Self {
            logs: ServiceLogState::Empty,
            feed: ServiceLogFeed::default(),
            stream: ServiceLogStreamLifecycle::default(),
            last_stream_poll_ms: 0,
        }
    }
}

impl ServiceDetailsState {
    pub(crate) fn select(&mut self, service_id: &ServiceId) -> bool {
        if self.stream.target() == Some(service_id) {
            return false;
        }
        self.logs = ServiceLogState::Loading;
        self.feed = ServiceLogFeed::default();
        self.stream = ServiceLogStreamLifecycle::open(service_id.clone());
        self.last_stream_poll_ms = 0;
        true
    }

    pub(crate) fn close(&mut self) {
        self.stream.close();
    }

    pub(crate) fn begin_refresh(&mut self) -> Option<ServiceId> {
        let service_id = self.stream.target()?.clone();
        Some(service_id)
    }

    /// Reset the merged log panel for an immediate re-request (the Refresh
    /// control): drop the retained feed + filters and go back to Loading.
    pub(crate) fn begin_log_refresh(&mut self) -> Option<ServiceId> {
        let service_id = self.stream.target()?.clone();
        self.logs = ServiceLogState::Loading;
        self.feed = ServiceLogFeed::default();
        self.stream = ServiceLogStreamLifecycle::open(service_id.clone());
        self.last_stream_poll_ms = 0;
        Some(service_id)
    }

    /// The 1s-gated follow query for the merged log panel (the same cadence
    /// and inflight discipline as the shell overlay's pump; GPUI
    /// `next_follow_request` parity). `None` when no service is active, a
    /// request is in flight, the panel is paused, or the interval has not
    /// elapsed.
    pub(crate) fn poll_log(&mut self, now_ms: u64) -> Option<(ServiceId, ServiceLogQuery)> {
        let service_id = self.stream.target()?.clone();
        if self.stream.is_loading() {
            return None;
        }
        if now_ms.saturating_sub(self.last_stream_poll_ms) < SERVICE_LOG_POLL_INTERVAL_MS {
            return None;
        }
        let query = self.feed.next_follow_query(&service_id)?;
        self.last_stream_poll_ms = now_ms;
        Some((service_id, query))
    }

    /// The merged log panel's follow-poll effect for the Tick pump.
    #[must_use]
    pub(crate) fn log_poll_effect(&mut self, now_ms: u64) -> Option<PlatformEffect> {
        let (_, query) = self.poll_log(now_ms)?;
        Some(PlatformEffect::ServiceLogStream(ServiceLogStreamRequest {
            query,
        }))
    }

    /// The immediate re-request effect for the Refresh control (bypasses the
    /// 1s gate by construction: `begin_log_refresh` zeroes the watermark and
    /// the inflight flag).
    #[must_use]
    pub(crate) fn log_refresh_effect(&mut self) -> Option<PlatformEffect> {
        let service_id = self.begin_log_refresh()?;
        let query = self.feed.next_follow_query(&service_id)?;
        Some(PlatformEffect::ServiceLogStream(ServiceLogStreamRequest {
            query,
        }))
    }

    pub(crate) fn begin_stream_attempt(
        &mut self,
        query: ServiceLogQuery,
    ) -> Option<taskmanager_application::ServiceAttemptId> {
        self.stream.begin_attempt(query)
    }

    pub(crate) fn accept_stream(
        &mut self,
        attempt_id: taskmanager_application::ServiceAttemptId,
        request_id: RequestId,
    ) {
        self.stream.accept_attempt(attempt_id, request_id);
    }

    pub(crate) fn reject_stream(
        &mut self,
        attempt_id: taskmanager_application::ServiceAttemptId,
        failure: FailureKind,
    ) {
        let failure = ServiceLogFailure::with_detail(
            ServiceLogErrorKind::from_failure(failure),
            format!("service log request was rejected: {failure:?}"),
        );
        self.stream.reject_attempt(attempt_id, failure);
    }

    /// Pause/resume the merged log panel's feed (the shared core pause: a
    /// paused feed stops both follow requests and entry merges).
    pub(crate) fn toggle_log_paused(&mut self) {
        self.feed.paused = !self.feed.paused;
    }

    /// Cycle the merged panel's level filter through the shell's single
    /// source order.
    pub(crate) fn cycle_log_level(&mut self) {
        self.feed.level = next_log_level(self.feed.level);
        if let Some(target) = self.stream.target().cloned() {
            self.stream = ServiceLogStreamLifecycle::open(target);
            self.last_stream_poll_ms = 0;
        }
    }

    /// Cycle the merged panel's time filter through the shell's single
    /// source order.
    pub(crate) fn cycle_log_time(&mut self) {
        self.feed.time = next_log_time(self.feed.time);
        if let Some(target) = self.stream.target().cloned() {
            self.stream = ServiceLogStreamLifecycle::open(target);
            self.last_stream_poll_ms = 0;
        }
    }

    /// The resolved log state the panel renders: live stream lines preferred
    /// (through the shared `resolve_lines`), the one-shot snapshot state as
    /// fallback — the exact merge GPUI's details view applies.
    #[must_use]
    pub(crate) fn resolved_logs(&self, now_micros: u64) -> ServiceLogState {
        let stream_lines: Vec<String> = self
            .feed
            .visible_entries(now_micros)
            .into_iter()
            .map(|entry| format!("[{:?}] {}", entry.level, entry.message))
            .collect();
        self.stream
            .projected_state()
            .resolve_lines(&self.logs, stream_lines)
    }

    /// The log text the Copy control writes; `None` when nothing is
    /// visible — never an empty clipboard write.
    #[must_use]
    pub(crate) fn log_copy_text(&self, now_micros: u64) -> Option<String> {
        self.resolved_logs(now_micros)
            .copy_text()
            .filter(|joined| !joined.trim().is_empty())
    }

    pub(crate) fn apply(&mut self, update: ServiceUpdate) {
        match update {
            ServiceUpdate::Logs(result) if self.stream.target() == Some(&result.service_id) => {
                self.logs = result.state;
            }
            ServiceUpdate::LogStream {
                request_id,
                observed_at_ms,
                snapshot,
            } if self.stream.target() == Some(&snapshot.query.service_id) => {
                if self.stream.resolve(request_id, snapshot.clone()) {
                    self.feed.apply_at(snapshot, observed_at_ms);
                }
            }
            ServiceUpdate::Action(_)
            | ServiceUpdate::Dependencies { .. }
            | ServiceUpdate::DependenciesUnavailable { .. }
            | ServiceUpdate::Logs(_)
            | ServiceUpdate::LogStream { .. } => {}
        }
    }

    pub(crate) fn snapshot(
        &self,
        dependencies: &ServiceDependenciesLifecycle,
        now_micros: u64,
    ) -> ServiceDetailsSnapshot {
        ServiceDetailsSnapshot {
            dependencies: dependencies.clone(),
            logs: self.resolved_logs(now_micros),
            log_paused: self.feed.paused,
            log_level: self.feed.level,
            log_time: self.feed.time,
        }
    }

    /// Seed the demo details modal's merged log panel (no host I/O): three
    /// plausible journal entries applied through the real `apply` path so
    /// demo screenshots exercise the same fold as production.
    pub(crate) fn seed_demo_logs(&mut self, service_id: &ServiceId) {
        let entries = ServiceLogEntries::new(vec![
            ServiceLogEntry {
                cursor: "demo-1".into(),
                realtime_timestamp_micros: None,
                priority: Some(6),
                level: ServiceLogLevel::Info,
                message: "systemd[1]: Started telemetry service.".into(),
            },
            ServiceLogEntry {
                cursor: "demo-2".into(),
                realtime_timestamp_micros: None,
                priority: Some(6),
                level: ServiceLogLevel::Info,
                message: "daemon[842]: Collectors ready: cpu memory disk network gpu".into(),
            },
            ServiceLogEntry {
                cursor: "demo-3".into(),
                realtime_timestamp_micros: None,
                priority: Some(6),
                level: ServiceLogLevel::Info,
                message: "daemon[842]: Health check passed".into(),
            },
        ])
        .expect("demo entries are non-empty");
        let snapshot = ServiceLogStreamSnapshot {
            query: ServiceLogQuery {
                service_id: service_id.clone(),
                level: ServiceLogLevelFilter::All,
                time: ServiceLogTimeFilter::All,
                after_cursor: None,
            },
            state: ServiceLogStreamState::Ready(entries),
        };
        self.stream.begin(RequestId::MIN, snapshot.query.clone());
        self.apply(ServiceUpdate::LogStream {
            request_id: RequestId::MIN,
            observed_at_ms: 1,
            snapshot,
        });
    }
}

impl IcedApp {
    pub(super) fn handle_service_message(
        &mut self,
        message: Message,
        clipboard_task: &mut Option<Task<Message>>,
    ) -> Option<PlatformEffect> {
        match message {
            Message::OpenServiceRowMenu {
                visual_index,
                source_index,
            } => {
                self.open_service_row_menu(visual_index, source_index);
                None
            }
            Message::CloseServiceRowMenu => {
                self.close_service_row_menu();
                None
            }
            Message::OpenServiceLog => self.open_service_log_effect(),
            Message::OpenServiceLogFor { index } => self.open_service_log_for_effect(index),
            Message::OpenServiceDetailsFor { index } => self.open_service_details_for_effect(index),
            Message::RefreshServiceDetails => self.refresh_service_details_effect(),
            Message::ToggleServiceDetailsLogPaused => {
                self.service_details.toggle_log_paused();
                None
            }
            Message::CycleServiceDetailsLogLevel => {
                self.service_details.cycle_log_level();
                None
            }
            Message::CycleServiceDetailsLogTime => {
                self.service_details.cycle_log_time();
                None
            }
            Message::CopyServiceDetailsLog => {
                self.copy_service_details_log(clipboard_task);
                None
            }
            Message::RefreshServiceDetailsLogs => {
                if self.is_demo() {
                    if let Some(service_id) = self.service_details.begin_log_refresh() {
                        self.service_details.seed_demo_logs(&service_id);
                    }
                    None
                } else {
                    self.service_details.log_refresh_effect()
                }
            }
            Message::CloseServiceLog => self.close_service_log_effect(),
            Message::ToggleLogFollow => self.toggle_service_log_follow_effect(),
            Message::ToggleLogPaused => self.toggle_service_log_paused_effect(),
            Message::CycleLogLevel => self.cycle_service_log_level_effect(),
            Message::CycleLogTime => self.cycle_service_log_time_effect(),
            Message::CopyServiceLog => {
                self.copy_service_log(clipboard_task);
                None
            }
            Message::ExportServiceLog => self.export_service_log(),
            Message::RequestServiceAction { index, action } => {
                let menu_target = self.service_menu_target().cloned();
                self.close_service_row_menu();
                match menu_target {
                    Some(service) => self.request_service_action_for(service, action),
                    None => self.request_service_action_at(index, action),
                }
            }
            Message::ConfirmServiceControl => self
                .shell
                .apply_action(taskmanager_application::AppAction::ConfirmServiceControl),
            _ => None,
        }
    }

    pub(super) fn open_service_details_for_effect(
        &mut self,
        index: usize,
    ) -> Option<PlatformEffect> {
        let service_id = self
            .shell
            .projection()
            .services
            .as_deref()
            .and_then(|services| services.get(index))
            .map(|service| service.id.clone())
            .filter(|service_id| !service_id.as_str().is_empty())?;

        self.open_local_surface(LocalSurface::ServiceDetails {
            service_id: service_id.clone(),
        });
        self.service_details.select(&service_id);

        if self.is_demo() {
            let request_id = taskmanager_application::RequestId::MIN;
            self.shell
                .service_dependencies
                .begin(request_id, service_id.clone());
            self.shell.service_dependencies.resolve(
                request_id,
                service_id.clone(),
                demo_service_dependencies(),
            );
            self.service_details.seed_demo_logs(&service_id);
            None
        } else {
            Some(ShellApp::request_service_dependencies(service_id))
        }
    }

    pub(super) fn refresh_service_details_effect(&mut self) -> Option<PlatformEffect> {
        let service_id = self.service_details.begin_refresh()?;
        if self.is_demo() {
            let request_id = taskmanager_application::RequestId::MIN;
            self.shell
                .service_dependencies
                .begin(request_id, service_id.clone());
            self.shell.service_dependencies.resolve(
                request_id,
                service_id.clone(),
                demo_service_dependencies(),
            );
            self.service_details.seed_demo_logs(&service_id);
            None
        } else {
            Some(ShellApp::request_service_dependencies(service_id))
        }
    }

    pub(crate) fn apply_service_details_updates(
        &mut self,
        updates: impl IntoIterator<Item = ServiceUpdate>,
    ) {
        for update in updates {
            self.service_details.apply(update);
        }
    }

    /// Copy the details modal's merged log lines to the clipboard (the same
    /// feedback contract as the standalone overlay's copy: an honest
    /// nothing-to-copy line when no line is visible).
    pub(super) fn copy_service_details_log(&mut self, clipboard_task: &mut Option<Task<Message>>) {
        match self
            .service_details
            .log_copy_text(self.service_log_now_micros())
        {
            Some(payload) => {
                self.shell.report_notice(
                    FeedbackSource::Clipboard,
                    FeedbackSeverity::Success,
                    FeedbackLifecycle::SHORT,
                    format!(
                        "{} · {}",
                        taskmanager_application::i18n::t("hint.copied"),
                        taskmanager_application::i18n::t("svc.logs"),
                    ),
                );
                *clipboard_task = Some(iced::clipboard::write(payload));
            }
            None => {
                self.shell.report_notice(
                    FeedbackSource::Clipboard,
                    FeedbackSeverity::Warning,
                    FeedbackLifecycle::SHORT,
                    taskmanager_application::i18n::t("svc.logs_nothing_to_copy"),
                );
            }
        }
    }

    pub(crate) fn service_details_snapshot(&self) -> ServiceDetailsSnapshot {
        self.service_details.snapshot(
            &self.shell.service_dependencies,
            self.service_log_now_micros(),
        )
    }
}

fn demo_service_dependencies() -> ServiceDeps {
    ServiceDeps::from_relations(ServiceRelationGraph::from_edges([
        ServiceRelationEdge::new(ServiceRelationKind::Requires, "sysinit.target"),
        ServiceRelationEdge::new(ServiceRelationKind::Requires, "basic.target"),
        ServiceRelationEdge::new(ServiceRelationKind::Wants, "network-online.target"),
        ServiceRelationEdge::new(ServiceRelationKind::WantedBy, "multi-user.target"),
        ServiceRelationEdge::new(ServiceRelationKind::After, "network.target"),
        ServiceRelationEdge::new(ServiceRelationKind::After, "systemd-journald.socket"),
    ]))
}

#[cfg(test)]
#[path = "../../tests/gui/app/service_details_tests.rs"]
mod tests;
