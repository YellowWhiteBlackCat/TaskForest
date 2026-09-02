//! Free-function dispatch of a queued platform effect through the frontend's
//! platform client (ADR-027), plus the shared notification cap and wall-clock
//! helper it shares with the platform-batch fold. Split out of the parent app
//! module so the file stays under the source-line ceiling.
use super::{ProcessControlKind, ShellApp};
use taskmanager_application::{
    PlatformClient, PlatformEffect, ProcessControlRequest, ServiceControlRequest,
    SessionControlRequest, ShellUiActionIntent,
};
use taskmanager_platform_contract::{RequestId, SubmissionError, SubmissionErrorKind};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SubmissionState {
    #[default]
    NoSubmission,
    Rejected(SubmissionErrorKind),
    Accepted,
}

impl SubmissionState {
    fn record(
        self,
        app: &mut ShellApp,
        result: Result<RequestId, SubmissionError>,
        accepted: &mut Vec<RequestId>,
    ) -> Self {
        match result {
            Ok(request_id) => {
                accepted.push(request_id);
                Self::Accepted
            }
            Err(error) => {
                app.report_submission_error(&error);
                match self {
                    Self::NoSubmission => Self::Rejected(error.kind),
                    Self::Rejected(first) => Self::Rejected(first),
                    Self::Accepted => Self::Accepted,
                }
            }
        }
    }

    fn finish(
        self,
        app: &mut ShellApp,
        effect: &PlatformEffect,
        accepted: Vec<RequestId>,
    ) -> Result<Vec<RequestId>, SubmissionErrorKind> {
        match self {
            Self::NoSubmission => Ok(accepted),
            Self::Rejected(kind) => Err(kind),
            Self::Accepted => {
                app.report_effect_queued(effect);
                Ok(accepted)
            }
        }
    }
}

/// Submit one platform effect through the frontend's [`PlatformClient`],
/// updating the shell's status line with the honest outcome. Shared by every
/// frontend (ADR-027): refresh scheduling and control submissions always
/// cross the application port, never the platform directly.
pub fn queue_effect(app: &mut ShellApp, platform: &mut PlatformClient, effect: PlatformEffect) {
    let _ = queue_effect_result(app, platform, effect);
}

/// Submit one effect and retain the submission rejection for a frontend that
/// owns a local typed loading/error projection. Existing callers should use
/// [`queue_effect`]; this result-bearing seam is for independent UI lanes.
pub fn queue_effect_result(
    app: &mut ShellApp,
    platform: &mut PlatformClient,
    effect: PlatformEffect,
) -> Result<Vec<RequestId>, SubmissionErrorKind> {
    let now_ms = submission_time_ms();
    let results = match &effect {
        PlatformEffect::Refresh(request) => platform.request_refresh(*request, now_ms),
        PlatformEffect::EndTask(target) => {
            // Record the accepted submission so a later EndTaskCompleted can
            // correlate (clear pending, record feedback, request refresh).
            match platform
                .submit_process_control(ProcessControlRequest::EndTask(target.clone()), now_ms)
            {
                Ok(request_id) => {
                    app.begin_process_control(
                        request_id,
                        target.clone(),
                        ProcessControlKind::EndTask,
                    );
                    vec![Ok(request_id)]
                }
                Err(error) => vec![Err(error)],
            }
        }
        PlatformEffect::ProcessSignal { target, signal } => {
            match platform.submit_process_control(
                ProcessControlRequest::SendSignal {
                    target: target.clone(),
                    signal: *signal,
                },
                now_ms,
            ) {
                Ok(request_id) => {
                    app.begin_process_control(
                        request_id,
                        target.clone(),
                        ProcessControlKind::Signal(*signal),
                    );
                    vec![Ok(request_id)]
                }
                Err(error) => vec![Err(error)],
            }
        }
        PlatformEffect::ExecuteBatch(intent) => {
            let attempt = app.request_sessions.begin_batch(intent.clone());
            let submission = platform.submit_process_control(
                ProcessControlRequest::ExecuteBatch(intent.clone()),
                now_ms,
            );
            match &submission {
                Ok(request_id) => {
                    app.request_sessions.accept_batch(attempt, *request_id);
                }
                Err(error) => {
                    app.request_sessions.reject_batch(
                        attempt,
                        taskmanager_application::request_submission_failure(error.kind),
                    );
                }
            }
            vec![submission]
        }
        // The ControlRequestId is caller-supplied payload: submit_service_control
        // allocates only the envelope request id, and the native adapter echoes
        // this id back in the ServiceControlOutcome. Generate it through the
        // latest-wins tracker so a superseded completion can never be accepted.
        PlatformEffect::ServiceControl(target) => {
            let request_id = app
                .data
                .service_control_requests
                .begin(target.service_id.clone(), target.action);
            let submission = platform.submit_service_control(
                ServiceControlRequest {
                    request_id,
                    service_id: target.service_id.clone(),
                    action: target.action,
                },
                now_ms,
            );
            if submission.is_err() {
                // A rejected submission never produces an outcome; drop the
                // pending tracker entry so a later action cannot correlate
                // against it.
                app.data.service_control_requests.accept(
                    request_id,
                    &target.service_id,
                    target.action,
                );
            }
            vec![submission]
        }
        PlatformEffect::SessionControl(target) => vec![platform.submit_session_control(
            SessionControlRequest {
                request_id: target.request_id,
                session_id: target.session_id.clone(),
                action: target.action,
            },
            now_ms,
        )],
        PlatformEffect::StartupControl(request) => {
            vec![platform.submit_startup_control(request.clone(), now_ms)]
        }
        PlatformEffect::RevealResource(request) => {
            let attempt = app.begin_shell_ui_action(ShellUiActionIntent::Reveal(request.clone()));
            let submission = platform.submit_resource_reveal(request.clone(), now_ms);
            match &submission {
                Ok(request_id) => {
                    app.accept_shell_ui_action(attempt, *request_id);
                }
                Err(error) => {
                    app.reject_shell_ui_action(
                        attempt,
                        taskmanager_application::request_submission_failure(error.kind),
                    );
                }
            }
            vec![submission]
        }
        PlatformEffect::OpenUrl(request) => {
            let attempt = app.begin_shell_ui_action(ShellUiActionIntent::OpenUrl(request.clone()));
            let submission = platform.submit_url_open(request.clone(), now_ms);
            match &submission {
                Ok(request_id) => {
                    app.accept_shell_ui_action(attempt, *request_id);
                }
                Err(error) => {
                    app.reject_shell_ui_action(
                        attempt,
                        taskmanager_application::request_submission_failure(error.kind),
                    );
                }
            }
            vec![submission]
        }
        PlatformEffect::ProcessInsights(target) => {
            match platform.submit_process_insights(target.clone(), now_ms) {
                Ok(submission) => vec![
                    submission.network,
                    submission.gpu,
                    submission.resources,
                    submission.isolation,
                    submission.threads,
                    // The optional open-files facet rides the same submission;
                    // collecting its result keeps a lane-absent error visible
                    // in the status line instead of silently dropped.
                    submission.open_files,
                ],
                Err(error) => {
                    app.report_process_insights_submission_error(error);
                    vec![]
                }
            }
        }
        PlatformEffect::ProcessNetworkEscalation => {
            let attempt = app.begin_network_escalation();
            let submission = platform.submit_process_network_escalation(now_ms);
            match &submission {
                Ok(request_id) => {
                    app.accept_network_escalation(attempt, *request_id);
                }
                Err(error) => {
                    app.reject_network_escalation(
                        attempt,
                        taskmanager_application::request_submission_failure(error.kind),
                    );
                }
            }
            vec![submission]
        }
        PlatformEffect::ServiceLogStream(request) => {
            let attempt = app
                .service_log
                .as_mut()
                .filter(|open| open.service_id() == Some(&request.query.service_id))
                .and_then(|open| open.lifecycle.begin_attempt(request.query.clone()));
            let submission = platform.submit_service_log_stream(request.clone(), now_ms);
            if let (Some(attempt_id), Some(open)) = (attempt, app.service_log.as_mut()) {
                match &submission {
                    Ok(request_id) => {
                        open.lifecycle.accept_attempt(attempt_id, *request_id);
                    }
                    Err(error) => {
                        open.lifecycle.reject_attempt(
                            attempt_id,
                            taskmanager_core::core::services::ServiceLogFailure::with_detail(
                                taskmanager_core::core::services::ServiceLogErrorKind::from_failure(
                                    taskmanager_application::service_submission_failure(error.kind),
                                ),
                                "service log request submission failed",
                            ),
                        );
                    }
                }
            }
            vec![submission]
        }
        PlatformEffect::DesktopNotification(request) => {
            vec![platform.submit_desktop_notification(request.clone(), now_ms)]
        }
        // Typed on-demand lanes (G-03/G-19): straight submissions mirror the
        // simple arms above; the identity-carrying control lanes also begin
        // their completion correlation like EndTask.
        PlatformEffect::DirectoryUsage(request) => {
            vec![platform.submit_directory_usage(request.clone(), now_ms)]
        }
        PlatformEffect::GpuEngineRows(request) => {
            let attempt = app.begin_gpu_engine_rows_request(request.device_id.clone());
            let submission = platform.submit_gpu_engine_rows(request.clone(), now_ms);
            match &submission {
                Ok(request_id) => {
                    app.accept_gpu_engine_rows_request(attempt, *request_id);
                }
                Err(error) => {
                    app.reject_gpu_engine_rows_request(
                        attempt,
                        taskmanager_application::request_submission_failure(error.kind),
                    );
                }
            }
            vec![submission]
        }
        PlatformEffect::NpuInventory(request) => {
            vec![platform.submit_npu_inventory(request.clone(), now_ms)]
        }
        PlatformEffect::SmbiosMemory(request) => {
            let attempt = app.begin_smbios_memory_request();
            let submission = platform.submit_smbios_memory(*request, now_ms);
            match &submission {
                Ok(request_id) => {
                    app.accept_smbios_memory_request(attempt, *request_id);
                }
                Err(error) => {
                    app.reject_smbios_memory_request(
                        attempt,
                        taskmanager_application::request_submission_failure(error.kind),
                    );
                }
            }
            vec![submission]
        }
        PlatformEffect::RaplPower(request) => {
            let attempt = app.begin_rapl_power_request();
            let submission = platform.submit_rapl_power(*request, now_ms);
            match &submission {
                Ok(request_id) => {
                    app.accept_rapl_power_request(attempt, *request_id);
                }
                Err(error) => {
                    app.reject_rapl_power_request(
                        attempt,
                        taskmanager_application::request_submission_failure(error.kind),
                    );
                }
            }
            vec![submission]
        }
        PlatformEffect::MsrReadout(request) => {
            let attempt = app.begin_msr_readout_request();
            let submission = platform.submit_msr_readout(*request, now_ms);
            match &submission {
                Ok(request_id) => {
                    app.accept_msr_readout_request(attempt, *request_id);
                }
                Err(error) => {
                    app.reject_msr_readout_request(
                        attempt,
                        taskmanager_application::request_submission_failure(error.kind),
                    );
                }
            }
            vec![submission]
        }
        PlatformEffect::SmartControl(request) => {
            let attempt = match request {
                taskmanager_application::SmartControlRequest::StartSelfTest(intent) => {
                    Some(app.request_sessions.begin_smart_self_test(intent.clone()))
                }
                taskmanager_application::SmartControlRequest::StopTracking(_) => None,
            };
            let submission = platform.submit_smart_control(request.clone(), now_ms);
            if let Some(attempt) = attempt {
                match &submission {
                    Ok(request_id) => {
                        app.request_sessions
                            .accept_smart_self_test(attempt, *request_id);
                    }
                    Err(error) => {
                        app.request_sessions.reject_smart_self_test(
                            attempt,
                            taskmanager_application::request_submission_failure(error.kind),
                        );
                    }
                }
            }
            vec![submission]
        }
        PlatformEffect::ServiceDependencies(request) => {
            let attempt_id = app
                .service_dependencies
                .begin_attempt(request.service_id.clone());
            let submission = platform.submit_service_dependencies(request.clone(), now_ms);
            match &submission {
                Ok(request_id) => app
                    .service_dependencies
                    .accept_attempt(attempt_id, *request_id),
                Err(error) => app.service_dependencies.reject_attempt(
                    attempt_id,
                    taskmanager_application::service_submission_failure(error.kind),
                ),
            };
            vec![submission]
        }
        PlatformEffect::ServiceLogSnapshot(request) => {
            let open_for_target = app
                .service_log
                .as_ref()
                .is_some_and(|open| open.service_id() == Some(&request.service_id));
            if open_for_target && let Some(open) = app.service_log.as_mut() {
                open.begin_snapshot();
            }
            let submission = platform.submit_service_log_snapshot(request.clone(), now_ms);
            if open_for_target
                && let Ok(request_id) = &submission
                && let Some(open) = app.service_log.as_mut()
            {
                open.accept_snapshot(*request_id);
            }
            vec![submission]
        }
        PlatformEffect::ProcessAffinity(request) => {
            let attempt = app.begin_process_affinity_read(request.target.clone());
            match platform.submit_process_affinity(request.clone(), now_ms) {
                Ok(request_id) => {
                    app.request_sessions.accept_affinity(attempt, request_id);
                    vec![Ok(request_id)]
                }
                Err(error) => {
                    app.request_sessions.reject_affinity(
                        attempt,
                        taskmanager_application::request_submission_failure(error.kind),
                    );
                    vec![Err(error)]
                }
            }
        }
        PlatformEffect::ProcessAffinityControl(request) => {
            match platform.submit_process_affinity_control(request.clone(), now_ms) {
                Ok(request_id) => {
                    app.begin_process_control(
                        request_id,
                        request.target.clone(),
                        ProcessControlKind::Affinity(request.cpus.clone()),
                    );
                    vec![Ok(request_id)]
                }
                Err(error) => vec![Err(error)],
            }
        }
        PlatformEffect::CommandLaunch(request) => {
            let attempt = app.begin_shell_ui_action(ShellUiActionIntent::Command(request.clone()));
            let submission = platform.submit_command_launch(request.clone(), now_ms);
            match &submission {
                Ok(request_id) => {
                    app.accept_shell_ui_action(attempt, *request_id);
                }
                Err(error) => {
                    app.reject_shell_ui_action(
                        attempt,
                        taskmanager_application::request_submission_failure(error.kind),
                    );
                }
            }
            vec![submission]
        }
        PlatformEffect::SetupScript(request) => {
            vec![platform.submit_setup_script(*request, now_ms)]
        }
        PlatformEffect::ResourceGroupControl(request) => {
            match platform.submit_process_resource_control(request.clone(), now_ms) {
                Ok(request_id) => {
                    app.begin_process_control(
                        request_id,
                        request.target.clone(),
                        ProcessControlKind::ResourceLimits(request.limits),
                    );
                    vec![Ok(request_id)]
                }
                Err(error) => vec![Err(error)],
            }
        }
    };
    let mut accepted = Vec::new();
    results
        .into_iter()
        .fold(SubmissionState::NoSubmission, |state, result| {
            state.record(app, result, &mut accepted)
        })
        .finish(app, &effect, accepted)
}

/// Bounded notification queue so a pathological evaluation burst cannot grow
/// the shell state without limit; the frontend drains every frame.
pub(super) const MAX_PENDING_NOTIFICATIONS: usize = 32;

pub(super) fn submission_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(u64::MAX)
}
