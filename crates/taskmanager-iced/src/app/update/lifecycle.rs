//! Before/after systems surrounding one Iced message reduction.

use std::time::Instant;

use iced::Task;
use taskmanager_application::{FocusDirection, KeyCode};

use super::super::surface::InteractionSnapshot;
use super::super::{
    DetailsSection, FocusTarget, IcedApp, IcedKey, Message, ModalAppear, PresenceTransition,
};
use super::dispatch::UpdateDispatch;
use super::window_events::{close_latest_window, restore_latest_window};

/// Immutable facts derived before the message mutates interaction state.
pub(super) struct UpdatePrelude {
    interaction: InteractionSnapshot,
    paused: bool,
    focus_requested: Option<FocusTarget>,
    selection_key: bool,
    focus_cycle: Option<FocusDirection>,
}

impl UpdatePrelude {
    pub(super) fn capture(app: &IcedApp, message: &Message) -> Self {
        Self {
            interaction: app.interaction_snapshot(),
            paused: app.shell.paused(),
            focus_requested: app.focus_request_for(message),
            selection_key: matches!(
                message,
                Message::Key(IcedKey::Fixed(event))
                    if matches!(
                        event.key,
                        KeyCode::ArrowUp
                            | KeyCode::ArrowDown
                            | KeyCode::PageUp
                            | KeyCode::PageDown
                    )
            ),
            focus_cycle: match message {
                Message::Key(IcedKey::Fixed(event)) if event.key == KeyCode::Tab => {
                    Some(if event.modifiers.shift {
                        FocusDirection::Previous
                    } else {
                        FocusDirection::Next
                    })
                }
                _ => None,
            },
        }
    }

    pub(super) const fn interaction(&self) -> InteractionSnapshot {
        self.interaction
    }
}

impl IcedApp {
    /// Apply the cross-cutting systems after the message-specific reducer:
    /// effect queueing, surface convergence, tray sync, activation, modal
    /// transitions, focus restoration, row reveal, and auxiliary tasks.
    pub(super) fn finish_update(
        &mut self,
        mut prelude: UpdatePrelude,
        dispatch: UpdateDispatch,
    ) -> Task<Message> {
        if let Some(effect) = dispatch.effect {
            self.queue(effect);
        }
        self.converge_shared_interaction_surface();
        self.assert_surface_invariants();
        if prelude.paused != self.shell.paused() {
            crate::tray::sync_tray_pause_checkmark(self, self.shell.paused());
        }
        if self.shell.should_quit() {
            return close_latest_window();
        }
        let activation = if self.take_activation_request() {
            restore_latest_window()
        } else {
            Task::none()
        };
        let interaction_after = self.interaction_snapshot();
        let modal = prelude.interaction.modal_transition(interaction_after);
        self.apply_modal_presence(modal);
        self.apply_process_properties_presence(
            prelude
                .interaction
                .process_properties_transition(interaction_after),
        );
        let restore_target = if matches!(modal, PresenceTransition::Closed) {
            self.input.modal_restore.take()
        } else {
            None
        };
        if prelude.selection_key
            && matches!(modal, PresenceTransition::StableClosed)
            && !self.shell.search_active()
        {
            prelude.focus_requested = self.selected_table_focus_target();
        }
        let reveal = self.selection_reveal_task(prelude.selection_key, modal);
        let focus = self.focus_task(
            modal,
            prelude.focus_requested,
            prelude.focus_cycle,
            restore_target,
        );
        let mut tasks = vec![reveal, focus, activation];
        tasks.extend(dispatch.tasks);
        Task::batch(tasks)
    }

    fn converge_shared_interaction_surface(&mut self) {
        if self.shell.interaction_surface().is_some() {
            self.dismiss_local_surface();
            self.dismiss_context_menu();
            self.shell.close_service_log();
            self.shell.dismiss_informational_overlay();
        }
    }

    fn apply_modal_presence(&mut self, transition: PresenceTransition) {
        match transition {
            PresenceTransition::Opened => {
                self.input.modal_restore = self.input.focused_control;
                self.input.modal_appear =
                    Some(ModalAppear::new(self.motion_policy(), Instant::now()));
            }
            PresenceTransition::Closed => self.input.modal_appear = None,
            PresenceTransition::StableClosed | PresenceTransition::StableOpen => {}
        }
    }

    fn apply_process_properties_presence(&mut self, transition: PresenceTransition) {
        if !matches!(transition, PresenceTransition::Opened) {
            return;
        }
        self.process_presentation.details_section = DetailsSection::default();
        self.process_presentation.env_filter.clear();
        self.seed_process_perf_history_from_provider();
    }

    fn selection_reveal_task(
        &mut self,
        selection_key: bool,
        modal: PresenceTransition,
    ) -> Task<Message> {
        if selection_key
            && matches!(modal, PresenceTransition::StableClosed)
            && !self.shell.search_active()
        {
            self.reveal_selected_table_row()
        } else {
            Task::none()
        }
    }
}
