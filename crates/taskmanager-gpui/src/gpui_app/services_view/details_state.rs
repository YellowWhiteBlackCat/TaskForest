//! Background log/export state for the service details dialog.
//!
//! The state is **per-window**: it lives on the owning window's `RootView`
//! (`RootView.service_details`), so opening the details dialog in one window
//! never leaks the other window's selected service / logs / pause state into
//! it. (A shared `thread_local` used to cross window boundaries.)

use std::path::PathBuf;

use taskmanager_app_host::DiagnosticBundleClient;
use taskmanager_application::{DiagnosticBundleSession, DiagnosticBundleTarget};

use taskmanager_application::{
    ServiceDependenciesLifecycle, ServiceLogStreamLifecycle, ServiceUpdate,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::services::{
    ServiceDeps, ServiceLogErrorKind, ServiceLogFailure, ServiceLogFeed, ServiceLogLevelFilter,
    ServiceLogQuery, ServiceLogState, ServiceLogStreamState, ServiceLogTimeFilter,
    ServiceRelationEdge, ServiceRelationGraph, ServiceRelationKind,
};
use taskmanager_core::core::target::ServiceId;
use taskmanager_core::core::{DiagnosticBundleErrorKind, DiagnosticBundlePlan, DiagnosticSource};
use taskmanager_platform_contract::RequestId;

#[derive(Debug, Clone, Copy)]
pub enum ServiceLogCopyFeedback {
    Copied,
    NoData,
}

#[derive(Debug)]
pub(crate) enum ServiceLogExportNotice {
    NothingToExport,
    Exporting(PathBuf),
    Exported(PathBuf),
    Failed(String),
}

#[derive(Debug, Default)]
enum ServiceLogExportRuntime {
    #[default]
    Unavailable,
    Active(DiagnosticBundleSession<DiagnosticBundleClient>),
}

#[derive(Debug, Clone)]
pub struct ServiceDetailsSnapshot {
    pub dependencies: ServiceDependenciesLifecycle,
    pub logs: ServiceLogState,
    pub copy_feedback: Option<ServiceLogCopyFeedback>,
    pub feed: ServiceLogFeed,
    pub log_stream: ServiceLogStreamState,
}

pub struct ServiceDetailsState {
    logs: ServiceLogState,
    copy_feedback: Option<ServiceLogCopyFeedback>,
    last_stream_request_ms: u64,
    feed: ServiceLogFeed,
    stream: ServiceLogStreamLifecycle,
    export: ServiceLogExportRuntime,
}

impl ServiceDetailsState {
    pub(crate) fn new() -> Self {
        Self {
            logs: ServiceLogState::Empty,
            copy_feedback: None,
            last_stream_request_ms: 0,
            feed: ServiceLogFeed::default(),
            stream: ServiceLogStreamLifecycle::default(),
            export: ServiceLogExportRuntime::default(),
        }
    }

    pub(crate) fn install_export_client(&mut self, client: DiagnosticBundleClient) {
        self.export = ServiceLogExportRuntime::Active(DiagnosticBundleSession::new(client));
    }

    pub fn select(&mut self, service_id: &ServiceId) -> bool {
        if self.stream.target() != Some(service_id) {
            self.logs = ServiceLogState::Loading;
            self.copy_feedback = None;
            self.feed = ServiceLogFeed::default();
            self.stream = ServiceLogStreamLifecycle::open(service_id.clone());
            self.last_stream_request_ms = 0;
            true
        } else {
            false
        }
    }

    pub(crate) fn close(&mut self) {
        self.stream.close();
        if let ServiceLogExportRuntime::Active(session) = &mut self.export {
            session.close();
        }
    }

    pub fn apply(&mut self, update: ServiceUpdate) {
        match update {
            ServiceUpdate::Action(_) => {}
            ServiceUpdate::Logs(result) if self.stream.target() == Some(&result.service_id) => {
                self.logs = result.state;
                self.copy_feedback = None;
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
            ServiceUpdate::Dependencies { .. }
            | ServiceUpdate::DependenciesUnavailable { .. }
            | ServiceUpdate::Logs(_)
            | ServiceUpdate::LogStream { .. } => {}
        }
    }

    pub(crate) fn poll_export(&mut self) -> Option<ServiceLogExportNotice> {
        let ServiceLogExportRuntime::Active(session) = &mut self.export else {
            return None;
        };
        let result = session.drain().into_iter().next()?;
        Some(match result.result {
            Ok(()) => ServiceLogExportNotice::Exported(result.destination),
            Err(error) => ServiceLogExportNotice::Failed(error.to_string()),
        })
    }

    pub(crate) fn next_follow_request(
        &mut self,
        service_id: &ServiceId,
        now_ms: u64,
    ) -> Option<ServiceLogQuery> {
        let follow_due = now_ms.saturating_sub(self.last_stream_request_ms) >= 1_000;
        if !self.stream.is_loading()
            && follow_due
            && let Some(query) = self.feed.next_follow_query(service_id)
        {
            self.last_stream_request_ms = now_ms;
            Some(query)
        } else {
            None
        }
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
        request_id: taskmanager_platform_contract::RequestId,
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

    pub fn snapshot(&self, dependencies: &ServiceDependenciesLifecycle) -> ServiceDetailsSnapshot {
        ServiceDetailsSnapshot {
            dependencies: dependencies.clone(),
            logs: self.logs.clone(),
            copy_feedback: self.copy_feedback,
            feed: self.feed.clone(),
            log_stream: self.stream.projected_state(),
        }
    }

    /// Snapshot the details dialog state for `service_id`, selecting it on
    /// first use (same contract as the old `details_for` free fn: render
    /// selects + drains the export worker, then renders the snapshot).
    pub(crate) fn details_for(
        &mut self,
        service_id: &ServiceId,
        dependencies: &ServiceDependenciesLifecycle,
    ) -> ServiceDetailsSnapshot {
        if std::env::var("TM_CAPTURE_SCENARIO").as_deref() == Ok("service-details-logs") {
            let deps = ServiceDeps::from_relations(ServiceRelationGraph::from_edges([
                ServiceRelationEdge::new(ServiceRelationKind::Requires, "sysinit.target"),
                ServiceRelationEdge::new(ServiceRelationKind::Requires, "basic.target"),
                ServiceRelationEdge::new(ServiceRelationKind::Wants, "network-online.target"),
                ServiceRelationEdge::new(ServiceRelationKind::WantedBy, "multi-user.target"),
                ServiceRelationEdge::new(ServiceRelationKind::After, "network.target"),
                ServiceRelationEdge::new(ServiceRelationKind::After, "systemd-journald.socket"),
            ]));
            let mut dependencies = ServiceDependenciesLifecycle::default();
            dependencies.begin(RequestId::MIN, service_id.clone());
            dependencies.resolve(RequestId::MIN, service_id.clone(), deps);
            return ServiceDetailsSnapshot {
                dependencies,
                logs: ServiceLogState::from_lines(vec![
                    "Jul 29 08:41:02 taskmanager systemd[1]: Started telemetry service.".into(),
                    "Jul 29 08:41:03 taskmanager daemon[842]: Collectors ready: cpu memory disk network gpu".into(),
                    "Jul 29 08:41:04 taskmanager daemon[842]: Health check passed".into(),
                    "Jul 29 08:41:05 taskmanager daemon[842]: Waiting for next refresh".into(),
                ]),
                copy_feedback: None,
                feed: ServiceLogFeed::default(),
                log_stream: ServiceLogStreamState::Empty,
            };
        }
        self.select(service_id);
        self.snapshot(dependencies)
    }

    pub(crate) fn begin_log_refresh(&mut self, service_id: &ServiceId) -> bool {
        if self.stream.target() == Some(service_id) {
            self.logs = ServiceLogState::Loading;
            self.copy_feedback = None;
            true
        } else {
            false
        }
    }

    pub(crate) fn toggle_pause(&mut self, service_id: &ServiceId) {
        if self.stream.target() == Some(service_id) {
            self.feed.paused = !self.feed.paused;
            self.last_stream_request_ms = 0;
        }
    }

    pub(crate) fn cycle_level(&mut self, service_id: &ServiceId) {
        if self.stream.target() == Some(service_id) {
            self.feed.level = match self.feed.level {
                ServiceLogLevelFilter::All => ServiceLogLevelFilter::Errors,
                ServiceLogLevelFilter::Errors => ServiceLogLevelFilter::WarningsAndErrors,
                ServiceLogLevelFilter::WarningsAndErrors => ServiceLogLevelFilter::InfoAndAbove,
                ServiceLogLevelFilter::InfoAndAbove => ServiceLogLevelFilter::All,
            };
            self.stream = ServiceLogStreamLifecycle::open(service_id.clone());
            self.last_stream_request_ms = 0;
        }
    }

    pub(crate) fn cycle_time(&mut self, service_id: &ServiceId) {
        if self.stream.target() == Some(service_id) {
            self.feed.time = match self.feed.time {
                ServiceLogTimeFilter::All => ServiceLogTimeFilter::LastHour,
                ServiceLogTimeFilter::LastHour => ServiceLogTimeFilter::LastDay,
                ServiceLogTimeFilter::LastDay => ServiceLogTimeFilter::All,
            };
            self.stream = ServiceLogStreamLifecycle::open(service_id.clone());
            self.last_stream_request_ms = 0;
        }
    }

    pub(crate) fn export_logs(
        &mut self,
        service_id: &ServiceId,
        display_name: &str,
        now_micros: u64,
    ) -> Option<ServiceLogExportNotice> {
        if self.stream.target() != Some(service_id) {
            return None;
        }
        let entries: Vec<_> = self
            .feed
            .visible_entries(now_micros)
            .into_iter()
            .cloned()
            .collect();
        if entries.is_empty() {
            return Some(ServiceLogExportNotice::NothingToExport);
        }
        let contents = match serde_json::to_string_pretty(&entries) {
            Ok(contents) => contents,
            Err(error) => return Some(ServiceLogExportNotice::Failed(error.to_string())),
        };
        let plan = match DiagnosticBundlePlan::prepare(
            vec![DiagnosticSource {
                name: "service-logs.json".into(),
                contents,
            }],
            [],
        ) {
            Ok(plan) => plan,
            Err(error) => return Some(ServiceLogExportNotice::Failed(error.to_string())),
        };
        let safe_name: String = display_name
            .chars()
            .map(|character| match character {
                character if character.is_ascii_alphanumeric() || "-_".contains(character) => {
                    character
                }
                _ => '-',
            })
            .collect();
        let file_name = format!("taskmanager-{safe_name}-logs.json");
        let ServiceLogExportRuntime::Active(session) = &mut self.export else {
            return Some(ServiceLogExportNotice::Failed(
                DiagnosticBundleErrorKind::Unavailable.stable_code().into(),
            ));
        };
        match session.submit(
            plan,
            DiagnosticBundleTarget::current_directory(file_name.clone()),
        ) {
            Ok(_) => Some(ServiceLogExportNotice::Exporting(PathBuf::from(file_name))),
            Err(error) => Some(ServiceLogExportNotice::Failed(error.to_string())),
        }
    }

    pub(crate) fn set_copy_feedback(&mut self, feedback: ServiceLogCopyFeedback) {
        self.copy_feedback = Some(feedback);
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/gpui_gpui_app_services_view_details_state_tests.rs"]
mod tests;
