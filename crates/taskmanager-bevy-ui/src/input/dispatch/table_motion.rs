//! The table-row motion arm of the ordered key pass.
//!
//! The inventory tables own their row cursor; the process list's cursor
//! belongs to the shell. This arm is additive: it runs after the character
//! router declined a press and always falls through to the fixed-key router
//! below.

use bevy::input::keyboard::KeyCode;
use taskmanager_application::Modifiers;

use super::{DispatchFrame, KeyPress, KeyboardOwner};
use crate::app::Page;

impl DispatchFrame<'_, '_, '_, '_, '_, '_> {
    /// Arm 3a — table row selection motion for non-process inventory tables.
    ///
    /// A focused slider owns the navigation keys, so a row never moves
    /// underneath a slider the user is adjusting.
    pub(super) fn table_motion(&mut self, press: KeyPress) {
        if !matches!(press.context, KeyboardOwner::Free) || press.modifiers != Modifiers::NONE {
            return;
        }
        if self.slider_owns(press) {
            return;
        }
        match (press.page, press.key_code) {
            (Page::Startup, KeyCode::ArrowUp) => self
                .commands
                .trigger(crate::pages::startup::StartupSelectionMoved(-1)),
            (Page::Startup, KeyCode::ArrowDown) => self
                .commands
                .trigger(crate::pages::startup::StartupSelectionMoved(1)),
            (Page::Sessions, KeyCode::ArrowUp) => self
                .commands
                .trigger(crate::pages::sessions::SessionSelectionMoved(-1)),
            (Page::Sessions, KeyCode::ArrowDown) => self
                .commands
                .trigger(crate::pages::sessions::SessionSelectionMoved(1)),
            (Page::Services, KeyCode::ArrowUp) => self
                .commands
                .trigger(crate::pages::services::ServiceSelectionMoved(-1)),
            (Page::Services, KeyCode::ArrowDown) => self
                .commands
                .trigger(crate::pages::services::ServiceSelectionMoved(1)),
            _ => return,
        }
        self.applied = true;
    }
}
