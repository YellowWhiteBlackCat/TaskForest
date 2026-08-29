//! Directly owned process-lifetime resources for [`super::IcedApp`].
//!
//! Native clients and lifecycle handles have one owner. No renderer clone or
//! view path can dynamically re-borrow the platform client: tick systems split
//! `IcedApp` fields under ordinary Rust mutable borrowing.

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use taskmanager_application::{PlatformClient, PlatformEffect};
use taskmanager_core::core::tray::TrayEvent;
use taskmanager_platform_contract::SubmissionErrorKind;

use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource, ShellApp};

use super::IcedApp;

/// Keep one UI tick responsive even when a lifecycle producer is busier than
/// the renderer. Remaining messages stay queued for the next tick.
const MAX_LIFECYCLE_EVENTS_PER_TICK: usize = 64;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ActivationRequest {
    #[default]
    Idle,
    Pending,
}

/// The sole owner of I/O and process-lifetime handles for one Iced app.
pub(crate) struct IcedRuntime {
    platform: Option<PlatformClient>,
    instance_guard: Option<Box<dyn taskmanager_platform_contract::InstanceGuard>>,
    instance_rx: Option<Receiver<taskmanager_platform_contract::InstanceEvent>>,
    tray_controller: Option<Box<dyn taskmanager_platform_contract::TrayController>>,
    tray_events_rx: Option<Receiver<TrayEvent>>,
    activation_request: ActivationRequest,
    last_gpu_engine_rows: Instant,
}

impl IcedRuntime {
    #[must_use]
    pub(crate) fn new(platform: Option<PlatformClient>) -> Self {
        Self {
            platform,
            instance_guard: None,
            instance_rx: None,
            tray_controller: None,
            tray_events_rx: None,
            activation_request: ActivationRequest::Idle,
            last_gpu_engine_rows: Instant::now(),
        }
    }

    #[must_use]
    pub(crate) const fn is_demo(&self) -> bool {
        self.platform.is_none()
    }

    pub(crate) fn platform_mut(&mut self) -> Option<&mut PlatformClient> {
        self.platform.as_mut()
    }

    pub(crate) fn install_instance(
        &mut self,
        guard: Option<Box<dyn taskmanager_platform_contract::InstanceGuard>>,
        receiver: Option<Receiver<taskmanager_platform_contract::InstanceEvent>>,
    ) {
        self.instance_guard = guard;
        self.instance_rx = receiver;
    }

    pub(crate) fn drain_instance_events(&self) -> bool {
        let Some(receiver) = self.instance_rx.as_ref() else {
            return false;
        };
        receiver
            .try_iter()
            .take(MAX_LIFECYCLE_EVENTS_PER_TICK)
            .count()
            != 0
    }

    pub(crate) fn install_tray(
        &mut self,
        controller: Option<Box<dyn taskmanager_platform_contract::TrayController>>,
        receiver: Option<Receiver<TrayEvent>>,
    ) {
        self.tray_controller = controller;
        self.tray_events_rx = receiver;
    }

    #[must_use]
    pub(crate) fn drain_tray_events(&self) -> Vec<TrayEvent> {
        self.tray_events_rx
            .as_ref()
            .map_or_else(Vec::new, |receiver| {
                receiver
                    .try_iter()
                    .take(MAX_LIFECYCLE_EVENTS_PER_TICK)
                    .collect()
            })
    }

    #[must_use]
    pub(crate) const fn tray_available(&self) -> bool {
        self.tray_controller.is_some()
    }

    pub(crate) fn sync_tray_pause_checkmark(&self, action: u32, paused: bool) {
        if let Some(controller) = self.tray_controller.as_ref() {
            let _ = controller.set_item_checked(action, paused);
        }
    }

    pub(crate) fn request_activation(&mut self) {
        self.activation_request = ActivationRequest::Pending;
    }

    pub(crate) fn take_activation_request(&mut self) -> bool {
        matches!(
            std::mem::take(&mut self.activation_request),
            ActivationRequest::Pending
        )
    }

    #[must_use]
    pub(crate) fn gpu_engine_rows_due(&self, cadence: Duration) -> bool {
        self.last_gpu_engine_rows.elapsed() >= cadence
    }

    pub(crate) fn reset_gpu_engine_rows_cadence(&mut self) {
        self.last_gpu_engine_rows = Instant::now();
    }

    pub(crate) fn queue(
        &mut self,
        shell: &mut ShellApp,
        effect: PlatformEffect,
    ) -> Result<Vec<taskmanager_platform_contract::RequestId>, SubmissionErrorKind> {
        match self.platform.as_mut() {
            Some(platform) => taskmanager_shell::queue_effect_result(shell, platform, effect),
            None => {
                shell.report_notice(
                    FeedbackSource::Demo,
                    FeedbackSeverity::Warning,
                    FeedbackLifecycle::UntilReplaced,
                    "Demo mode suppresses platform actions",
                );
                Err(SubmissionErrorKind::RuntimeStopped)
            }
        }
    }
}

impl IcedApp {
    pub(crate) fn install_instance_runtime(
        &mut self,
        guard: Option<Box<dyn taskmanager_platform_contract::InstanceGuard>>,
        receiver: Option<Receiver<taskmanager_platform_contract::InstanceEvent>>,
    ) {
        self.runtime.install_instance(guard, receiver);
    }

    #[must_use]
    pub(crate) const fn tray_available(&self) -> bool {
        self.runtime.tray_available()
    }

    pub(crate) fn take_activation_request(&mut self) -> bool {
        self.runtime.take_activation_request()
    }
}

#[cfg(test)]
#[path = "../../tests/gui/app/runtime_tests.rs"]
mod tests;
