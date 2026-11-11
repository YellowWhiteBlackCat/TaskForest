//! Shared reset for Iced's renderer-local menu surfaces.

use super::*;
use taskforest_core::core::session::SessionControlAction;
use taskmanager_application::{AppPage, KeyCode};

impl IcedApp {
    pub(super) fn close_context_menus(&mut self) {
        self.dismiss_context_menu();
    }

    /// Arrow-key traversal for the one open context menu. The targets are the
    /// menu's real focus stops in renderer order; Tab keeps Iced's native
    /// traversal and Escape keeps the existing dismiss path.
    pub(super) fn context_menu_focus_menu_key(&mut self, previous: bool) -> Option<FocusTarget> {
        let kind = self.context_menu_kind()?;
        let targets = self.context_menu_targets(kind);
        let target = next_context_menu_target(&targets, self.input.focused_control, previous)?;
        self.input.focused_control = Some(target);
        Some(target)
    }

    fn context_menu_targets(&self, kind: ContextMenuKind) -> Vec<FocusTarget> {
        match kind {
            ContextMenuKind::Process => vec![
                FocusTarget::ProcessMenuEndTask,
                FocusTarget::ProcessMenuEndTree,
                FocusTarget::ProcessMenuKill,
                FocusTarget::ProcessMenuSuspend,
                FocusTarget::ProcessMenuResume,
                FocusTarget::ProcessMenuPriorityHigh,
                FocusTarget::ProcessMenuPriorityNormal,
                FocusTarget::ProcessMenuPriorityLow,
                FocusTarget::ProcessMenuEfficiencyMode,
                FocusTarget::ProcessMenuSignalHangup,
                FocusTarget::ProcessMenuSignalInterrupt,
                FocusTarget::ProcessMenuSignalUser1,
                FocusTarget::ProcessMenuSignalUser2,
                FocusTarget::ProcessMenuOpenLocation,
                FocusTarget::ProcessMenuSearchOnline,
                FocusTarget::ProcessMenuAffinity,
                FocusTarget::ProcessMenuProperties,
                FocusTarget::ProcessMenuCopyName,
                FocusTarget::ProcessMenuCopyPid,
                FocusTarget::ProcessMenuCopyCommandLine,
                FocusTarget::ProcessMenuCopyTsv,
                FocusTarget::ProcessMenuCopyJson,
                FocusTarget::ProcessMenuClose,
            ],
            ContextMenuKind::Service => {
                let Some(index) = self.service_menu_index() else {
                    return Vec::new();
                };
                use ServiceAction::*;
                [Start, Stop, Restart, Enable, Disable, ReloadDaemon]
                    .into_iter()
                    .map(|action| FocusTarget::ServiceMenuAction { index, action })
                    .chain(std::iter::once(FocusTarget::ServiceMenuClose))
                    .collect()
            }
            ContextMenuKind::Startup => {
                let Some(index) = self.startup_menu_index() else {
                    return Vec::new();
                };
                vec![
                    FocusTarget::StartupMenuAction {
                        index,
                        enabled: true,
                    },
                    FocusTarget::StartupMenuAction {
                        index,
                        enabled: false,
                    },
                    FocusTarget::StartupMenuClose,
                ]
            }
            ContextMenuKind::User => vec![
                FocusTarget::UserRowMenuDisconnect,
                FocusTarget::UserRowMenuLock,
                FocusTarget::UserRowMenuClose,
            ],
            ContextMenuKind::ProcessColumns => {
                let mut targets: Vec<FocusTarget> = crate::ui::applications::apps_columns(true)
                    .into_iter()
                    .filter(|(column, _)| crate::ui::applications::column_hideable(*column))
                    .map(|(column, _)| FocusTarget::ProcessColumnToggle(column))
                    .collect();
                targets.extend([
                    FocusTarget::ProcessColumnsReset,
                    FocusTarget::ProcessColumnsClose,
                ]);
                targets
            }
            ContextMenuKind::ProcessActions => {
                if self.shell.process_control_availability().is_ready() {
                    vec![FocusTarget::ProcessPrioritySelect]
                } else {
                    Vec::new()
                }
            }
        }
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
            Message::SelectServiceStatusFilter(_) => Some(FocusTarget::ServiceStatusFilterSegment),
            Message::SelectStartupStatusFilter(_) => Some(FocusTarget::StartupStatusFilterSegment),
            Message::OpenProcessRowMenu { .. } => Some(FocusTarget::ProcessMenuEndTask),
            Message::OpenProcessAffinity => Some(FocusTarget::ProcessAffinityCpu(0)),
            Message::OpenProcessColumnsMenu => Some(FocusTarget::ProcessColumnToggle(SortCol::Pid)),
            Message::OpenProcessActionsMenu
                if self.shell.process_control_availability().is_ready() =>
            {
                Some(FocusTarget::ProcessPrioritySelect)
            }
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
                    && self.shell.page() == AppPage::Startup
                    && !self.startup_presentation.query().trim().is_empty() =>
            {
                let source_index = self.startup_source_index(self.shell.selected)?;
                let index = self
                    .shell
                    .sorted_startup_indices()
                    .iter()
                    .position(|source| *source == source_index)
                    .unwrap_or(0);
                Some(FocusTarget::TableRow {
                    page: AppPage::Startup,
                    index,
                })
            }
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

pub(super) fn next_context_menu_target(
    targets: &[FocusTarget],
    focused: Option<FocusTarget>,
    previous: bool,
) -> Option<FocusTarget> {
    if targets.is_empty() {
        return None;
    }

    let current =
        focused.and_then(|target| targets.iter().position(|candidate| *candidate == target));
    let next = match (current, previous) {
        (Some(0), true) | (None, true) => targets.len() - 1,
        (Some(index), true) => index - 1,
        (Some(index), false) => (index + 1) % targets.len(),
        (None, false) => 0,
    };
    targets.get(next).copied()
}

#[cfg(test)]
#[path = "../../tests/gui/app/menu_focus_tests.rs"]
mod tests;
