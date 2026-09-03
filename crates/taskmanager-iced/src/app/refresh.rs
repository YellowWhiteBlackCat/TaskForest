//! Named tick systems for platform drain, scheduling and view-local finish.

use super::*;
use crate::ui::first_run::FirstRunEvent;
use taskmanager_application::ServiceUpdate;
use taskmanager_core::core::identity::DeviceId;

use taskmanager_shell::{ShellApp, queue_effect};

const GPU_ENGINE_ROWS_REFRESH: std::time::Duration = std::time::Duration::from_millis(2500);

struct TickPlan {
    insight_effect: Option<PlatformEffect>,
    service_log_effect: Option<PlatformEffect>,
    details_log_effect: Option<PlatformEffect>,
    gpu_device: Option<DeviceId>,
}

impl IcedApp {
    pub(super) fn handle_tick_message(&mut self) -> Option<PlatformEffect> {
        self.shell
            .advance_feedback_time(std::time::Duration::from_millis(100));
        self.runtime_event_system();
        // The tick doubles as the deferred-stepper-commit flush point (see
        // `update::columns`): a cheap gate check per 100 ms poll.
        self.poll_process_column_persist();
        self.tick();
        None
    }

    fn runtime_event_system(&mut self) {
        self.drain_config_publications();
        self.drain_history_replay_completions();
        self.drain_snapshot_export_completions();
        let instance_activate = self.runtime.drain_instance_events();
        let tray_activate = crate::tray::drain_tray_events(self);
        if instance_activate || tray_activate {
            self.runtime.request_activation();
        }
    }

    /// Run the tick as three ordered systems. `platform_tick_system` borrows
    /// only `runtime` and `shell`; the finish system starts after that static
    /// borrow ends, so no dynamic borrow guard or manual `drop` is possible.
    /// The first-run lane's correlated events are extracted inside the
    /// platform borrow and folded after it ends (the fold mutates
    /// renderer-local surface state).
    pub(super) fn tick(&mut self) {
        let plan = self.prepare_tick_system();
        let (service_updates, first_run_events) = self.platform_tick_system(plan);
        self.fold_first_run_events(first_run_events);
        self.finish_tick_system(service_updates);
    }

    fn prepare_tick_system(&mut self) -> TickPlan {
        self.poll_service_log_export();
        let insight_effect = self.request_selected_process_insights();
        let now_ms = unix_now_ms();
        self.window_time.observe_tick_millis(now_ms);
        let service_log_effect = self.shell.poll_service_log(now_ms);
        let details_log_effect = (!self.runtime.is_demo())
            .then(|| self.service_details.log_poll_effect(now_ms))
            .flatten();

        let selected_gpu_index = match self.performance.selected_device {
            super::selectors::PerfDevice::Gpu(index) => Some(index),
            _ => None,
        };
        let gpu_device = if matches!(
            self.shell.gpu_engine_rows_state(),
            taskmanager_application::GpuEngineRowsState::Loading { .. }
                | taskmanager_application::GpuEngineRowsState::Ready(_)
        ) && selected_gpu_index.is_some()
            && self.runtime.gpu_engine_rows_due(GPU_ENGINE_ROWS_REFRESH)
        {
            self.runtime.reset_gpu_engine_rows_cadence();
            let index = selected_gpu_index.unwrap_or_default();
            self.shell
                .projection()
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.gpu.get(index))
                .and_then(|gpu| {
                    let id = gpu.device_id.trim();
                    (!id.is_empty()).then(|| DeviceId::new(id.to_owned()))
                })
        } else {
            None
        };

        TickPlan {
            insight_effect,
            service_log_effect,
            details_log_effect,
            gpu_device,
        }
    }

    fn platform_tick_system(&mut self, plan: TickPlan) -> (Vec<ServiceUpdate>, Vec<FirstRunEvent>) {
        let Self {
            runtime,
            shell,
            service_details,
            ..
        } = self;
        let Some(platform) = runtime.platform_mut() else {
            let failure = taskmanager_application::service_submission_failure(
                taskmanager_platform_contract::SubmissionErrorKind::RuntimeStopped,
            );
            if let Some(PlatformEffect::ServiceLogStream(request)) = plan.details_log_effect
                && let Some(attempt_id) =
                    service_details.begin_stream_attempt(request.query.clone())
            {
                service_details.reject_stream(attempt_id, failure);
            }
            if let Some(PlatformEffect::ServiceLogStream(request)) = plan.service_log_effect
                && let Some(open) = shell.service_log.as_mut()
                && open.service_id() == Some(&request.query.service_id)
                && let Some(attempt_id) = open.lifecycle.begin_attempt(request.query)
            {
                open.lifecycle.reject_attempt(
                    attempt_id,
                    taskmanager_core::core::services::ServiceLogFailure::with_detail(
                        taskmanager_core::core::services::ServiceLogErrorKind::from_failure(
                            failure,
                        ),
                        "service log runtime is stopped",
                    ),
                );
            }
            return (Vec::new(), Vec::new());
        };

        shell.apply_capability_snapshot(platform.capabilities().snapshot());

        let (service_updates, first_run_events) = match platform.try_drain() {
            Ok(batch) => {
                let updates = batch
                    .service_events
                    .iter()
                    .filter_map(|event| match &event.event {
                        taskmanager_application::ServiceEvent::Update(update) => {
                            Some(update.clone())
                        }
                        taskmanager_application::ServiceEvent::Snapshot(_) => None,
                    })
                    .collect();
                // Correlate the first-run lane's own requests before the
                // shell consumes the batch; the fold itself runs after the
                // platform borrow ends (see `tick`).
                let first_run_events = super::update::first_run::extract_batch_events(
                    &batch,
                    &mut self.first_run_requests,
                );
                shell.apply_platform_batch(batch);
                for request in shell.drain_alert_notifications() {
                    queue_effect(
                        shell,
                        platform,
                        PlatformEffect::DesktopNotification(request),
                    );
                }
                (updates, first_run_events)
            }
            Err(error) => {
                shell.report_event_port_error(error);
                (Vec::new(), Vec::new())
            }
        };

        if !shell.paused() {
            platform.set_telemetry_interval(shell.telemetry_interval());
            let _ = platform.run_scheduled_refresh(unix_now_ms());
        }
        if let Some(device_id) = plan.gpu_device {
            queue_effect(
                shell,
                platform,
                ShellApp::request_gpu_engine_rows(device_id),
            );
        }
        for effect in [plan.service_log_effect, plan.insight_effect]
            .into_iter()
            .flatten()
        {
            queue_effect(shell, platform, effect);
        }
        if let Some(PlatformEffect::ServiceLogStream(request)) = plan.details_log_effect {
            let query = request.query.clone();
            let Some(attempt_id) = service_details.begin_stream_attempt(query) else {
                return (service_updates, first_run_events);
            };
            match taskmanager_shell::queue_effect_result(
                shell,
                platform,
                PlatformEffect::ServiceLogStream(request),
            ) {
                Ok(request_ids) => {
                    if let Some(request_id) = request_ids.into_iter().next() {
                        service_details.accept_stream(attempt_id, request_id);
                    }
                }
                Err(error) => service_details.reject_stream(
                    attempt_id,
                    taskmanager_application::service_submission_failure(error),
                ),
            }
        }
        (service_updates, first_run_events)
    }

    fn finish_tick_system(&mut self, service_updates: Vec<ServiceUpdate>) {
        self.apply_service_details_updates(service_updates);
        self.sample_process_history();
        self.sync_process_affinity_snapshot();
        // Same-wave fold (ADR-034 stage 2): reconcile the shared GPU
        // chart-metric selection against the viewed device's fresh facts so
        // a generation change or a family going dark falls back to the
        // default in the frame that carried the fact.
        let gate = taskmanager_shell::gpu_chart_metric_gate(self.viewed_gpu());
        self.shell.reconcile_gpu_chart_metric(&gate);
        self.advance_motion(Instant::now());
    }

    fn request_selected_process_insights(&mut self) -> Option<PlatformEffect> {
        let identity = self.shell.selected_process_identity();
        match identity {
            Some(identity)
                if self.process_presentation.last_insights_target.as_ref() != Some(&identity) =>
            {
                self.process_presentation.last_insights_target = Some(identity.clone());
                Some(PlatformEffect::ProcessInsights(identity))
            }
            Some(_) | None => None,
        }
    }
}

fn unix_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(u64::MAX)
}
