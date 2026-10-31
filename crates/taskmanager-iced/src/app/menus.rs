//! Shared reset for Iced's renderer-local menu surfaces.

use super::*;
use taskforest_core::core::session::SessionControlAction;
use taskmanager_application::{AppPage, KeyCode};

impl IcedApp {
    pub(super) fn close_context_menus(&mut self) {
        self.dismiss_context_menu();
    }

    /// Open a Users row menu from the current canonical visual order and
    /// freeze the exact provider session before any later refresh can reorder
    /// the projection.
    pub(super) fn open_user_row_menu(&mut self, visual_index: usize) {
        let session = self
            .shell
            .sorted_session_indices()
            .get(visual_index)
            .and_then(|source_index| {
                self.shell
                    .projection()
                    .sessions
                    .as_deref()
                    .and_then(|sessions| sessions.get(*source_index))
            })
            .cloned();
        let Some(session) = session else {
            return;
        };
        let _ = self.shell.select_row(visual_index);
        self.open_context_menu(ContextMenu::User {
            visual_index,
            session,
        });
    }

    pub(super) fn close_user_row_menu(&mut self) {
        if self.context_menu_kind() == Some(ContextMenuKind::User) {
            self.dismiss_context_menu();
        }
    }

    /// Arm the shared confirmation from the Users menu's frozen provider
    /// payload. A later inventory reorder cannot retarget this pending action.
    pub(super) fn arm_user_menu_session_control(&mut self, action: SessionControlAction) -> bool {
        let Some(session) = self.user_menu_session().cloned() else {
            return false;
        };
        self.close_context_menus();
        self.shell.select_session_control(&session, action)
    }

    /// Arm the shared confirmation from the current canonical Users row.
    pub(super) fn arm_selected_session_control(&mut self, action: SessionControlAction) -> bool {
        let session = self
            .shell
            .sorted_session_indices()
            .get(self.shell.selected)
            .and_then(|source_index| {
                self.shell
                    .projection()
                    .sessions
                    .as_deref()
                    .and_then(|sessions| sessions.get(*source_index))
            })
            .cloned();
        session.is_some_and(|session| self.shell.select_session_control(&session, action))
    }

    pub fn focus_request_for(&self, message: &Message) -> Option<FocusTarget> {
        match message {
            Message::Focus(target) => Some(*target),
            Message::SelectPage(page) => Some(FocusTarget::PageTab(*page)),
            Message::SelectPerfDevice(device) => Some(FocusTarget::PerfDeviceTab(*device)),
            Message::SelectProcessStatusFilter(filter) => {
                Some(FocusTarget::ProcessStatusFilterTab(*filter))
            }
            Message::OpenProcessRowMenu { .. } => Some(FocusTarget::ProcessMenuEndTask),
            Message::OpenProcessAffinity => Some(FocusTarget::ProcessAffinityCpu(0)),
            Message::OpenProcessColumnsMenu => Some(FocusTarget::ProcessColumnToggle(SortCol::Pid)),
            Message::OpenServiceRowMenu { source_index, .. } => {
                Some(FocusTarget::ServiceMenuAction {
                    index: *source_index,
                    action: ServiceAction::Start,
                })
            }
            Message::OpenServiceLogFor { .. } => Some(FocusTarget::ServiceLogPause),
            Message::OpenServiceDetailsFor { index } => {
                Some(FocusTarget::ServiceDetailsOpen { index: *index })
            }
            Message::OpenDiskSmart { index } => Some(FocusTarget::DiskSmartOpen { index: *index }),
            Message::CloseSearch if self.shell.search_active() => Some(FocusTarget::SearchTrigger),
            Message::Key(IcedKey::Fixed(event))
                if event.key == KeyCode::Escape
                    && self.context_menu_kind() == Some(ContextMenuKind::ProcessColumns) =>
            {
                Some(FocusTarget::ProcessColumnsTrigger)
            }
            Message::Key(IcedKey::Fixed(event))
                if event.key == KeyCode::Escape
                    && self.shell.page() == AppPage::Services
                    && !self.services_query().is_empty() =>
            {
                Some(FocusTarget::TableRow {
                    page: AppPage::Services,
                    index: 0,
                })
            }
            Message::AuthorizeRaplPower => Some(FocusTarget::AuthorizeRaplPower),
            Message::AuthorizeMsrReadouts => Some(FocusTarget::AuthorizeMsrReadouts),
            Message::OpenUserRowMenu(_) => Some(FocusTarget::UserRowMenuDisconnect),
            Message::OpenStartupRowMenu { visual_index } => Some(FocusTarget::StartupMenuAction {
                index: self
                    .startup_source_index(*visual_index)
                    .unwrap_or(*visual_index),
                enabled: true,
            }),
            _ => None,
        }
    }
}
