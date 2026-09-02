//! Platform-fold side effects into shell history, feedback and view hygiene.

use super::*;
use crate::presentation::process_batch_action_label;
use taskmanager_platform_contract::RequestId;

impl ShellApp {
    /// Attach or detach the process-owned durable-history sink. The shell
    /// keeps the live graph store intact when the preference changes; only
    /// the optional persistence mirror is swapped.
    pub fn set_history_persistence_sink(
        &mut self,
        sink: Option<std::sync::Arc<dyn taskmanager_core::core::history::HistoryRecordSink>>,
    ) {
        if self.persistent_application_history.is_some() == sink.is_some() {
            return;
        }
        self.persistent_application_history = sink.as_ref().map(|sink| {
            taskmanager_application::PersistentApplicationHistoryRecorder::new(
                std::sync::Arc::clone(sink),
            )
        });
        if let Some(sink) = sink {
            let ingestor = self.ensure_history_ingestor().with_record_sink(sink);
            self.history_ingestor = Some(ingestor);
        } else if let Some(ingestor) = self.history_ingestor.take() {
            self.history_ingestor = Some(ingestor.without_record_sink());
        }
    }

    /// Apply one platform batch and reduce its side outputs through the shell's
    /// typed feedback, history and selection authorities.
    pub fn apply_platform_batch(&mut self, mut batch: PlatformEventBatch) {
        self.advance_feedback_platform_batch();
        self.request_sessions.filter_platform_terminals(&mut batch);
        let mut output = self.data.apply_platform_batch(batch);
        self.request_sessions.accept_fold_terminals(&mut output);
        self.ingest_live_history(&output);
        self.sync_from_platform_fold(output);
    }

    fn ingest_live_history(&mut self, output: &BatchFoldOutput) {
        let ingestor = self.ensure_history_ingestor();
        for correlated in &output.system_telemetry_outcomes {
            if let Err(error) =
                crate::history::ingest_correlated_system_outcome(&ingestor, correlated)
            {
                self.report_notice(
                    FeedbackSource::Platform,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    format!("History ingestion failed: {error:?}"),
                );
            }
        }
        for correlated in &output.sensor_events {
            if let Err(error) =
                crate::history::ingest_correlated_sensor_event(&ingestor, correlated)
            {
                self.report_notice(
                    FeedbackSource::Platform,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    format!("Sensor history ingestion failed: {error:?}"),
                );
            }
        }
        for correlated in &output.power_supply_events {
            if let Err(error) =
                crate::history::ingest_correlated_power_supply_event(&ingestor, correlated)
            {
                self.report_notice(
                    FeedbackSource::Platform,
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    format!("Power history ingestion failed: {error:?}"),
                );
            }
        }
        if let Some(recorder) = self.persistent_application_history.as_mut() {
            for correlated in &output.process_events {
                let processes = match &correlated.event {
                    taskmanager_application::ProcessEvent::Snapshot(processes) => processes,
                    _ => continue,
                };
                let _ = recorder.record_process_snapshot(
                    processes,
                    correlated.sequence.get(),
                    correlated.observed_at_ms,
                );
            }
        }
    }

    pub(crate) fn ensure_history_ingestor(
        &mut self,
    ) -> taskmanager_telemetry_store::CorrelatedSystemTelemetryIngestor {
        if self.history_ingestor.is_none() {
            let (history, ingestor) =
                taskmanager_telemetry_store::live_graph::LiveGraphHistory::shared(
                    self.history.capacity(),
                );
            self.history = history;
            self.history_ingestor = Some(ingestor);
        }
        if let Some(ingestor) = self.history_ingestor.as_ref() {
            return ingestor.clone();
        }
        let (history, ingestor) = taskmanager_telemetry_store::live_graph::LiveGraphHistory::shared(
            self.history.capacity(),
        );
        self.history = history;
        self.history_ingestor = Some(ingestor.clone());
        ingestor
    }

    pub(super) fn begin_process_control(
        &mut self,
        request_id: RequestId,
        target: FrozenProcessIdentity,
        kind: ProcessControlKind,
    ) {
        self.data.begin_process_control(request_id, target, kind);
    }

    #[must_use]
    pub(super) fn begin_process_affinity_read(
        &mut self,
        target: FrozenProcessIdentity,
    ) -> taskmanager_application::RequestAttemptId {
        self.request_sessions.begin_affinity(target)
    }

    #[must_use]
    pub fn take_process_refresh_request(&mut self) -> Option<PlatformEffect> {
        self.data.take_process_refresh_request()
    }

    fn sync_from_platform_fold(&mut self, output: BatchFoldOutput) {
        if let Some(activity) = output.activity.as_ref() {
            self.set_feedback_activity(activity);
        }
        for failure in &output.failures {
            self.report_notice(
                FeedbackSource::Platform,
                FeedbackSeverity::Error,
                FeedbackLifecycle::UntilReplaced,
                format!("Capability {}: {:?}", failure.capability, failure.kind),
            );
        }
        if let Some(feedback) = output.process_feedback.as_ref() {
            let (text, succeeded) = process_control_notice_text(feedback);
            let severity = if succeeded {
                FeedbackSeverity::Success
            } else {
                FeedbackSeverity::Error
            };
            let lifecycle = if succeeded {
                FeedbackLifecycle::SHORT
            } else {
                FeedbackLifecycle::UntilReplaced
            };
            self.report_notice(FeedbackSource::Control, severity, lifecycle, text);
        }
        for (_, result) in &output.batch_results {
            let (text, complete) = process_batch_notice_text(result);
            self.report_notice(
                FeedbackSource::Control,
                if complete {
                    FeedbackSeverity::Success
                } else {
                    FeedbackSeverity::Error
                },
                if complete {
                    FeedbackLifecycle::SHORT
                } else {
                    FeedbackLifecycle::UntilReplaced
                },
                text,
            );
        }
        for outcome in &output.session_control_outcomes {
            let (severity, lifecycle, text) = match &outcome.result {
                Ok(()) => (
                    FeedbackSeverity::Success,
                    FeedbackLifecycle::SHORT,
                    format!(
                        "Session {} {:?} completed",
                        outcome.session_id, outcome.action
                    ),
                ),
                Err(error) => (
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    format!(
                        "Session {} {:?} failed: {error:?}",
                        outcome.session_id, outcome.action
                    ),
                ),
            };
            self.report_notice(FeedbackSource::Control, severity, lifecycle, text);
        }
        for outcome in &output.startup_control_outcomes {
            let intent = if outcome.enabled { "enable" } else { "disable" };
            let (severity, lifecycle, text) = match &outcome.result {
                Ok(()) => (
                    FeedbackSeverity::Success,
                    FeedbackLifecycle::SHORT,
                    format!("Startup {} {intent} completed", outcome.target_name),
                ),
                Err(error) => (
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    format!("Startup {} {intent} failed: {error:?}", outcome.target_name),
                ),
            };
            self.report_notice(FeedbackSource::Control, severity, lifecycle, text);
        }
        for outcome in &output.service_control_outcomes {
            let (severity, lifecycle, text) = match &outcome.result {
                Ok(()) => (
                    FeedbackSeverity::Success,
                    FeedbackLifecycle::SHORT,
                    format!(
                        "Service {} {:?} completed",
                        outcome.service_id, outcome.action
                    ),
                ),
                Err(error) => (
                    FeedbackSeverity::Error,
                    FeedbackLifecycle::UntilReplaced,
                    format!(
                        "Service {} {:?} failed: {error:?}",
                        outcome.service_id, outcome.action
                    ),
                ),
            };
            self.report_notice(FeedbackSource::Control, severity, lifecycle, text);
        }
        if output.changes.snapshot_recorded
            && let Some(snapshot) = self.data.snapshot.as_ref()
        {
            self.alert_suggestions.record_snapshot(snapshot);
        }
        if output.changes.processes {
            self.prune_stale_selection();
        }
        if !output.changes.is_empty() {
            self.clamp_selection();
        }
        for update in output.service_log_updates {
            match update {
                ServiceUpdate::Logs {
                    request_id,
                    snapshot,
                } => {
                    self.apply_service_log_snapshot(
                        request_id,
                        snapshot.service_id,
                        snapshot.state,
                    );
                }
                ServiceUpdate::LogStream {
                    request_id,
                    observed_at_ms,
                    snapshot,
                } => self.apply_service_log_update(request_id, snapshot, observed_at_ms),
                _ => {}
            }
        }
        for update in output.service_updates {
            match update {
                ServiceUpdate::Dependencies {
                    request_id,
                    service_id,
                    deps,
                } => {
                    self.service_dependencies
                        .resolve(request_id, service_id, deps);
                }
                ServiceUpdate::DependenciesUnavailable {
                    request_id,
                    service_id,
                    error,
                } => {
                    self.service_dependencies
                        .fail(request_id, service_id, error);
                }
                _ => {}
            }
        }
    }
}

/// Renderer-neutral message projection for a correlated process-control
/// completion. Direct and composed frontend tracks consume the same mapping.
#[must_use]
pub fn process_control_notice_text(feedback: &ProcessControlFeedback) -> (String, bool) {
    let action = process_control_action_label(&feedback.kind);
    match feedback.result {
        Ok(()) => (
            taskmanager_application::i18n::t("feedback.process_action_succeeded")
                .replace("{action}", &action)
                .replace("{pid}", &feedback.target.pid.to_string()),
            true,
        ),
        Err(kind) => (
            taskmanager_application::i18n::t("feedback.process_action_failed")
                .replace("{action}", &action)
                .replace("{pid}", &feedback.target.pid.to_string())
                .replace("{reason}", crate::presentation::control_error_detail(kind)),
            false,
        ),
    }
}

fn process_batch_notice_text(
    result: &taskmanager_core::core::process::ProcessBatchResult,
) -> (String, bool) {
    use taskmanager_core::core::process::ProcessBatchTargetResult;

    let action = process_batch_action_label(result.intent.action);
    let total = result.targets.len();
    let applied = result.applied_count();
    let mut text = taskmanager_application::i18n::t("proc_control.batch_result")
        .replace("{action}", &action)
        .replace("{applied}", &applied.to_string())
        .replace("{total}", &total.to_string());
    if let Some((identity, outcome)) = result
        .targets
        .iter()
        .find(|(_, outcome)| !matches!(outcome, ProcessBatchTargetResult::Applied))
    {
        let reason = match outcome {
            ProcessBatchTargetResult::Failed(kind) => {
                crate::presentation::control_error_detail(*kind)
            }
            ProcessBatchTargetResult::IdentityUnavailable
            | ProcessBatchTargetResult::IdentityChanged => {
                taskmanager_application::i18n::t("feedback.target_changed")
            }
            ProcessBatchTargetResult::Applied => {
                taskmanager_application::i18n::t("feedback.unknown_error")
            }
        };
        let item = taskmanager_application::i18n::t("proc_control.batch_failure_item")
            .replace("{name}", &identity.name)
            .replace("{pid}", &identity.pid.to_string())
            .replace("{reason}", reason);
        text = format!("{text} · {item}");
    }
    (text, total > 0 && applied == total)
}

fn process_control_action_label(kind: &ProcessControlKind) -> String {
    match kind {
        ProcessControlKind::EndTask => taskmanager_application::i18n::t("proc.end_task").to_owned(),
        ProcessControlKind::Signal(signal) => format!(
            "{} {signal:?}",
            taskmanager_application::i18n::t("common.signal")
        ),
        ProcessControlKind::Suspend => taskmanager_application::i18n::t("proc.suspend").to_owned(),
        ProcessControlKind::Resume => taskmanager_application::i18n::t("proc.resume").to_owned(),
        ProcessControlKind::Affinity(_) => {
            taskmanager_application::i18n::t("proc.affinity").to_owned()
        }
        ProcessControlKind::ResourceLimits(_) => {
            taskmanager_application::i18n::t("proc_insights.resource_limits").to_owned()
        }
    }
}
