//! Renderer-local focus command policy for the Iced adapter.

use super::{FocusTarget, IcedApp, Message, PresenceTransition};
use iced::Task;
use taskmanager_application::FocusDirection;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FocusCommand {
    ModalClose,
    Restore(FocusTarget),
    Target(FocusTarget),
    Previous,
    Next,
    None,
}

pub(super) fn focus_command(
    modal: PresenceTransition,
    focus_requested: Option<FocusTarget>,
    focus_cycle: Option<FocusDirection>,
    restore_target: Option<FocusTarget>,
) -> FocusCommand {
    if matches!(modal, PresenceTransition::Closed) {
        restore_target.map_or(FocusCommand::None, FocusCommand::Restore)
    } else if modal.is_open()
        && (matches!(modal, PresenceTransition::Opened)
            || matches!(focus_requested, Some(FocusTarget::ModalClose))
            || focus_cycle.is_some())
    {
        FocusCommand::ModalClose
    } else if let Some(target) = focus_requested {
        FocusCommand::Target(target)
    } else if let Some(direction) = focus_cycle {
        match direction {
            FocusDirection::Next => FocusCommand::Next,
            FocusDirection::Previous => FocusCommand::Previous,
        }
    } else {
        FocusCommand::None
    }
}

pub(super) fn pending_end_focus_target(focused_control: Option<FocusTarget>) -> FocusTarget {
    match focused_control {
        Some(FocusTarget::ConfirmEndTask) => FocusTarget::CancelEndTask,
        Some(FocusTarget::CancelEndTask) => FocusTarget::ConfirmEndTask,
        _ => FocusTarget::ConfirmEndTask,
    }
}

/// Tab-cycle the pending service-control confirmation scope between its two
/// actions, mirroring the end-task confirmation scope.
pub(super) fn service_control_focus_target(focused_control: Option<FocusTarget>) -> FocusTarget {
    match focused_control {
        Some(FocusTarget::ConfirmServiceControl) => FocusTarget::CancelServiceControl,
        Some(FocusTarget::CancelServiceControl) => FocusTarget::ConfirmServiceControl,
        _ => FocusTarget::ConfirmServiceControl,
    }
}

pub(super) fn smart_self_test_focus_target(focused_control: Option<FocusTarget>) -> FocusTarget {
    match focused_control {
        Some(FocusTarget::ConfirmSmartSelfTest) => FocusTarget::CancelSmartSelfTest,
        Some(FocusTarget::CancelSmartSelfTest) => FocusTarget::ConfirmSmartSelfTest,
        _ => FocusTarget::ConfirmSmartSelfTest,
    }
}

impl IcedApp {
    /// Resolve one update's focus operation into an Iced focus task. The modal
    /// scopes (end-task / service-control / local modal) Tab-cycle their own
    /// controls first; otherwise the shared [`focus_command`] policy decides.
    pub(super) fn focus_task(
        &mut self,
        modal: PresenceTransition,
        focus_requested: Option<FocusTarget>,
        focus_cycle: Option<FocusDirection>,
        restore_target: Option<FocusTarget>,
    ) -> Task<Message> {
        if self.shell.pending_end().is_some() && modal.was_open() && focus_cycle.is_some() {
            let target = pending_end_focus_target(self.input.focused_control);
            self.input.focused_control = Some(target);
            return iced::widget::operation::focus(crate::focus::focus_id(target));
        }
        if self.shell.pending_service_control().is_some()
            && modal.was_open()
            && focus_cycle.is_some()
        {
            let target = service_control_focus_target(self.input.focused_control);
            self.input.focused_control = Some(target);
            return iced::widget::operation::focus(crate::focus::focus_id(target));
        }
        if self.shell.pending_smart_self_test().is_some()
            && modal.was_open()
            && focus_cycle.is_some()
        {
            let target = smart_self_test_focus_target(self.input.focused_control);
            self.input.focused_control = Some(target);
            return iced::widget::operation::focus(crate::focus::focus_id(target));
        }
        if self.local_modal_open()
            && modal.was_open()
            && let Some(direction) = focus_cycle
        {
            // Tab cycles through the modal's own controls (settings pills,
            // toolbar-adjacent choosers) instead of snapping to Close.
            return match direction {
                FocusDirection::Next => iced::widget::operation::focus_next(),
                FocusDirection::Previous => iced::widget::operation::focus_previous(),
            };
        }
        match focus_command(modal, focus_requested, focus_cycle, restore_target) {
            FocusCommand::ModalClose => {
                let target = self.modal_focus_target();
                self.input.focused_control = Some(target);
                iced::widget::operation::focus(crate::focus::focus_id(target))
            }
            FocusCommand::Restore(target) => {
                self.input.focused_control = Some(target);
                iced::widget::operation::focus(crate::focus::focus_id(target))
            }
            FocusCommand::Target(target) => {
                self.input.focused_control = Some(target);
                iced::widget::operation::focus(crate::focus::focus_id(target))
            }
            FocusCommand::Previous => iced::widget::operation::focus_previous(),
            FocusCommand::Next => iced::widget::operation::focus_next(),
            FocusCommand::None => Task::none(),
        }
    }

    /// The focus target a pending modal should land on: the confirm action of
    /// an open end-task / service-control bar, else the generic modal close.
    pub(super) fn modal_focus_target(&self) -> FocusTarget {
        match self.input_scope() {
            super::InputScope::SharedSurface(
                taskmanager_application::SurfaceKind::Confirmation(
                    taskmanager_application::ConfirmationKind::EndTask,
                ),
            ) => FocusTarget::ConfirmEndTask,
            super::InputScope::SharedSurface(
                taskmanager_application::SurfaceKind::Confirmation(
                    taskmanager_application::ConfirmationKind::ServiceControl,
                ),
            ) => FocusTarget::ConfirmServiceControl,
            super::InputScope::SharedSurface(
                taskmanager_application::SurfaceKind::Confirmation(
                    taskmanager_application::ConfirmationKind::SmartSelfTest,
                ),
            ) => FocusTarget::ConfirmSmartSelfTest,
            super::InputScope::LocalSurface(super::LocalSurfaceKind::RunTask) => {
                FocusTarget::RunTaskCommandInput
            }
            super::InputScope::LocalSurface(super::LocalSurfaceKind::ProcessAffinity) => {
                FocusTarget::ProcessAffinityCpu(0)
            }
            _ => FocusTarget::ModalClose,
        }
    }
}
