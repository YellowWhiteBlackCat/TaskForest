//! GPUI adapter for effects emitted by the shared primary-interaction machine.
//!
//! Confirmation payloads never cross this seam directly. Application
//! `InteractionState` validates the expected branch, consumes the frozen
//! intent and produces one `PlatformEffect`; this system is the only GPUI
//! confirmation path allowed to translate that effect into a platform request.

use gpui::Context;
use taskmanager_application::{PlatformEffect, ProcessControlRequest, SmartControlRequest};

use super::RootView;
use super::process_feedback::ProcessControlAction;

impl RootView {
    pub(crate) fn dispatch_confirmed_effect(
        &mut self,
        effect: PlatformEffect,
        cx: &mut Context<Self>,
    ) -> bool {
        match effect {
            PlatformEffect::EndTask(target) => {
                let pid = target.pid;
                self.submit_process_control(
                    ProcessControlRequest::EndTask(target),
                    ProcessControlAction::EndTask,
                    pid,
                    cx,
                )
            }
            PlatformEffect::ExecuteBatch(intent) => {
                self.submit_process_batch_intent(intent, cx);
                true
            }
            PlatformEffect::ServiceControl(target) => {
                self.request_service_action(target.service_id, target.action);
                true
            }
            PlatformEffect::StartupControl(request) => {
                self.submit_startup_control_request(request);
                true
            }
            PlatformEffect::SessionControl(target) => {
                self.submit_session_control_target(target);
                true
            }
            PlatformEffect::SmartControl(SmartControlRequest::StartSelfTest(intent)) => {
                self.submit_smart_self_test_intent(intent)
            }
            // StopTracking is an immediate lifecycle action, not a dangerous
            // confirmation output. Every other effect family is likewise
            // outside the shared primary-interaction reducer.
            PlatformEffect::SmartControl(SmartControlRequest::StopTracking(_))
            | PlatformEffect::Refresh(_)
            | PlatformEffect::ProcessSignal { .. }
            | PlatformEffect::RevealResource(_)
            | PlatformEffect::OpenUrl(_)
            | PlatformEffect::ProcessInsights(_)
            | PlatformEffect::ProcessNetworkEscalation
            | PlatformEffect::ServiceLogStream(_)
            | PlatformEffect::DesktopNotification(_)
            | PlatformEffect::DirectoryUsage(_)
            | PlatformEffect::GpuEngineRows(_)
            | PlatformEffect::NpuInventory(_)
            | PlatformEffect::ServiceDependencies(_)
            | PlatformEffect::ServiceLogSnapshot(_)
            | PlatformEffect::ProcessAffinity(_)
            | PlatformEffect::ProcessAffinityControl(_)
            | PlatformEffect::CommandLaunch(_)
            | PlatformEffect::SetupScript(_)
            | PlatformEffect::ResourceGroupControl(_) => false,
        }
    }
}
