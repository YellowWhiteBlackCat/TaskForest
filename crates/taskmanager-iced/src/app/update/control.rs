//! Platform refresh and typed control-intent message reducer.

use taskmanager_application::{AppAction, PlatformEffect};

use taskmanager_shell::ShellApp;

use super::super::{ContextMenuKind, IcedApp, Message};
use super::dispatch::UpdateDispatch;

impl IcedApp {
    pub(super) fn reduce_control_message(&mut self, message: Message) -> UpdateDispatch {
        let effect = match message {
            Message::RefreshSource(request) => Some(PlatformEffect::Refresh(request)),
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
            Message::OpenProcessLocation => self.process_location_effect(),
            Message::SearchProcessOnline => self.process_search_effect(),
            _ => None,
        };
        UpdateDispatch::effect(effect)
    }
}
