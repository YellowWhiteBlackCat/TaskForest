//! Keyboard, modifier, search-edit and focus message reducer.

use super::super::surface::InteractionSnapshot;
use super::super::{
    ContextMenuKind, FocusTarget, IcedApp, IcedKey, InputScope, LocalSurface, Message,
};
use super::dispatch::UpdateDispatch;
use taskmanager_application::KeyCode;
use taskmanager_application::SurfaceKind;
use taskmanager_shell::ShellKeyEvent;
use taskmanager_ui_contract::InputModality;

impl IcedApp {
    /// The selectable value that owns the window's one active text selection;
    /// the view resolves each widget's `selection_owner` flag against it.
    pub(crate) fn text_selection_owner(&self) -> Option<iced::advanced::widget::Id> {
        self.input.text_selection_owner.clone()
    }

    pub(super) fn reduce_input_message(
        &mut self,
        message: Message,
        interaction_before: InteractionSnapshot,
    ) -> UpdateDispatch {
        match message {
            Message::Tick => UpdateDispatch::effect(self.handle_tick_message()),
            Message::DismissToast => {
                self.toast.dismiss(&self.shell);
                UpdateDispatch::none()
            }
            Message::ModifiersChanged(modifiers) => {
                self.input.modifiers = modifiers;
                self.shell.set_control_held(modifiers.control());
                UpdateDispatch::none()
            }
            Message::Key(IcedKey::Fixed(event)) => {
                self.input.modality = InputModality::Keyboard;
                self.configuration.set_focus_visible(true);
                if self.context_menu_kind().is_some() {
                    match self.context_menu_key(event) {
                        ContextMenuKeyOutcome::Focus(task) => UpdateDispatch::task(task),
                        ContextMenuKeyOutcome::NoTask => UpdateDispatch::none(),
                        ContextMenuKeyOutcome::Continue => self.continue_context_menu_key(event),
                    }
                } else if let Some(copy) = self.copy_selected_row_summary(&event) {
                    UpdateDispatch::task(copy)
                } else {
                    UpdateDispatch::effect(self.handle_fixed_message(event))
                }
            }
            Message::Key(IcedKey::Character(character, modifiers)) => {
                self.input.modality = InputModality::Keyboard;
                self.configuration.set_focus_visible(true);
                let effect = match self.input_scope() {
                    InputScope::ServiceLog => {
                        match character {
                            'q' | 'Q' => self.shell.close_service_log(),
                            'f' | 'F' => self.shell.toggle_service_log_follow(),
                            'p' | 'P' => self.shell.toggle_service_log_paused(),
                            'l' | 'L' => self.shell.cycle_service_log_level(),
                            't' | 'T' => self.shell.cycle_service_log_time(),
                            _ => {}
                        }
                        None
                    }
                    InputScope::SharedSurface(SurfaceKind::Confirmation(_))
                    | InputScope::Help
                    | InputScope::Suggestions
                    | InputScope::Search => self
                        .shell
                        .handle_local_char(character, modifiers)
                        .into_effect(),
                    InputScope::Content if matches!(character, 'a' | 'A') && modifiers.control => {
                        self.open_local_surface(LocalSurface::About);
                        None
                    }
                    InputScope::Content => self
                        .shell
                        .handle_local_char(character, modifiers)
                        .into_effect(),
                    InputScope::SharedSurface(SurfaceKind::ProcessProperties)
                    | InputScope::LocalSurface(_)
                    | InputScope::ContextMenu(_) => None,
                };
                UpdateDispatch::effect(effect)
            }
            Message::Key(IcedKey::Other) => {
                // Bare modifiers and other unmapped keys are still keyboard
                // input to the focus-visible tracker.
                self.input.modality = InputModality::Keyboard;
                self.configuration.set_focus_visible(true);
                UpdateDispatch::none()
            }
            Message::PointerPressed => {
                self.input.modality = InputModality::Pointer;
                self.configuration.set_focus_visible(false);
                UpdateDispatch::none()
            }
            Message::TextSelectionClaimed(id) => {
                // The reference selection registry, collapsed to one owner
                // slot: beginning a selection anywhere moves ownership, and
                // every other selectable value clears its highlight.
                self.input.text_selection_owner = Some(id);
                UpdateDispatch::none()
            }
            Message::SearchBackspace => {
                if self.shell.search_active() {
                    self.shell.pop_search_char();
                }
                UpdateDispatch::none()
            }
            Message::Focus(target) => {
                if !interaction_before.opaque_modal_open()
                    && !matches!(target, FocusTarget::ModalClose)
                {
                    self.input.focused_control = Some(target);
                }
                UpdateDispatch::none()
            }
            Message::FocusedTargetUnmounted(target) => {
                if self.input.focused_control == Some(target) {
                    self.input.focused_control = None;
                    self.reset_focus_visible();
                }
                UpdateDispatch::none()
            }
            _ => UpdateDispatch::none(),
        }
    }

    /// Return focus-visible state to the programmatic baseline after the
    /// window or its tracked target stops being a valid keyboard focus owner.
    pub(super) fn reset_focus_visible(&mut self) {
        self.input.modality = InputModality::Programmatic;
        self.configuration.set_focus_visible(false);
    }
}

enum ContextMenuKeyOutcome {
    Focus(iced::Task<Message>),
    NoTask,
    Continue,
}

impl IcedApp {
    fn continue_context_menu_key(&mut self, event: ShellKeyEvent) -> UpdateDispatch {
        if let Some(copy) = self.copy_selected_row_summary(&event) {
            UpdateDispatch::task(copy)
        } else {
            UpdateDispatch::effect(self.handle_fixed_message(event))
        }
    }

    fn context_menu_key(&mut self, event: ShellKeyEvent) -> ContextMenuKeyOutcome {
        match event.key {
            KeyCode::ArrowUp | KeyCode::ArrowDown
                if self.context_menu_kind() == Some(ContextMenuKind::ProcessActions)
                    && self.shell.process_control_availability().is_ready() =>
            {
                ContextMenuKeyOutcome::NoTask
            }
            KeyCode::ArrowUp | KeyCode::ArrowDown => self
                .context_menu_focus_menu_key(event.key == KeyCode::ArrowUp)
                .map_or(ContextMenuKeyOutcome::Continue, |target| {
                    ContextMenuKeyOutcome::Focus(iced::widget::operation::focus(
                        crate::focus::focus_id(target),
                    ))
                }),
            _ => ContextMenuKeyOutcome::Continue,
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/gui/app/menu_focus_input_tests.rs"]
mod tests;
