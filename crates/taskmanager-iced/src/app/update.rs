//! Iced message orchestration: capture once, route once, finish once.

use iced::Task;

use super::{IcedApp, Message};

mod alerts;
// `pub(super)`: `app` re-exports the sizing state types for the view layer.
pub(super) mod columns;
mod control;
mod dispatch;
pub(super) mod first_run;
mod input;
mod lifecycle;
mod navigation;
mod navigation_messages;
mod performance;
mod run_task;
mod service;
mod surfaces;
mod transfer;
mod window;
mod window_events;

use lifecycle::UpdatePrelude;

impl IcedApp {
    /// Every message crosses the same lifecycle envelope. The exhaustive
    /// domain router owns branch selection; reducers only return typed effects
    /// and auxiliary tasks, and cannot bypass the finish systems.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        let prelude = UpdatePrelude::capture(self, &message);
        let dispatch = self.dispatch_message(message, prelude.interaction());
        self.finish_update(prelude, dispatch)
    }
}
