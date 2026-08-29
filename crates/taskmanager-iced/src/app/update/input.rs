//! Keyboard, modifier, search-edit and focus message reducer.

use super::super::surface::InteractionSnapshot;
use super::super::{FocusTarget, IcedApp, IcedKey, InputScope, LocalSurface, Message};
use super::dispatch::UpdateDispatch;

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
            Message::ModifiersChanged(modifiers) => {
                self.input.modifiers = modifiers;
                self.shell.set_control_held(modifiers.control());
                UpdateDispatch::none()
            }
            Message::Key(IcedKey::Fixed(event)) => {
                crate::input_modality::observe_keyboard();
                if let Some(copy) = self.copy_selected_row_summary(&event) {
                    UpdateDispatch::task(copy)
                } else {
                    UpdateDispatch::effect(self.handle_fixed_message(event))
                }
            }
            Message::Key(IcedKey::Character(character, modifiers)) => {
                crate::input_modality::observe_keyboard();
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
                    InputScope::SharedSurface(
                        taskmanager_application::SurfaceKind::Confirmation(_),
                    )
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
                    InputScope::SharedSurface(
                        taskmanager_application::SurfaceKind::ProcessProperties,
                    )
                    | InputScope::LocalSurface(_)
                    | InputScope::ContextMenu(_) => None,
                };
                UpdateDispatch::effect(effect)
            }
            Message::Key(IcedKey::Other) => {
                // Bare modifiers and other unmapped keys are still keyboard
                // input to the focus-visible tracker.
                crate::input_modality::observe_keyboard();
                UpdateDispatch::none()
            }
            Message::PointerPressed => {
                crate::input_modality::observe_pointer();
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
            _ => UpdateDispatch::none(),
        }
    }
}
