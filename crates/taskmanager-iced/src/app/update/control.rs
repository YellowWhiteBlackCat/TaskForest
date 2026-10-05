//! Platform refresh and typed control-intent message reducer.

use taskmanager_application::{AppAction, MsrReadoutRequest, PlatformEffect, RaplPowerRequest};

use taskmanager_shell::ShellApp;

use super::super::{ContextMenuKind, IcedApp, Message};
use super::dispatch::UpdateDispatch;

impl IcedApp {
    /// Handle a typed control message, returning any emitted platform effect.
    pub fn handle_control_message(&mut self, message: Message) -> Option<PlatformEffect> {
        match message {
            Message::RefreshSource(request) => Some(PlatformEffect::Refresh(request)),
            Message::AuthorizeRaplPower => {
                Some(PlatformEffect::RaplPower(RaplPowerRequest::Refresh))
            }
            Message::AuthorizeMsrReadouts => {
                Some(PlatformEffect::MsrReadout(MsrReadoutRequest::Refresh))
            }
            Message::RequestEndTask => self.shell.apply_action(AppAction::RequestEndTask),
            Message::RequestProcessBatch(action) => self.shell.request_process_batch(action),
            Message::ConfirmEndTask => self.shell.confirm_end_task(),
            Message::ConfirmProcessBatch => self.shell.confirm_process_batch(),
            Message::RequestSessionControl(action) => {
                if self.user_menu_session().is_some() {
                    self.request_user_menu_action(action)
                } else {
                    self.close_context_menus();
                    self.shell.request_session_control(action)
                }
            }
            Message::RequestProcessNetworkEscalation => {
                self.queue(ShellApp::request_process_network_escalation());
                None
            }
            Message::OpenUserRowMenu(index) => {
                self.open_user_row_menu(index);
                None
            }
            Message::CloseUserRowMenu => {
                self.close_user_row_menu();
                None
            }
            Message::OpenStartupRowMenu { visual_index } => {
                self.open_startup_row_menu(visual_index);
                None
            }
            Message::CloseStartupRowMenu => {
                self.close_startup_row_menu();
                None
            }
            Message::RequestStartupControl(enabled) => {
                self.dismiss_context_menu_kind(ContextMenuKind::Startup);
                self.shell.request_startup_control(enabled)
            }
            Message::RequestStartupControlFor { index: _, enabled } => {
                self.apply_startup_menu_action(enabled)
            }
            Message::ConfirmStartupControl => self.shell.confirm_startup_control(),
            Message::RequestSmartSelfTest(intent) => {
                let observed = self
                    .shell
                    .projection()
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| {
                        snapshot.disks.iter().any(|disk| {
                            disk.device_id == intent.device_id.as_str()
                                && disk.device_generation == intent.device_generation
                                && disk.device_generation.is_valid()
                        })
                    });
                if observed {
                    self.shell.arm_smart_self_test(intent);
                }
                None
            }
            Message::ConfirmSmartSelfTest => self.shell.confirm_smart_self_test(),
            Message::OpenProcessLocation => self.process_location_effect(),
            Message::SearchProcessOnline => self.process_search_effect(),
            _ => None,
        }
    }

    pub(crate) fn reduce_control_message(&mut self, message: Message) -> UpdateDispatch {
        let effect = self.handle_control_message(message);
        UpdateDispatch::effect(effect)
    }
}
