//! Frame, viewport and native-window lifecycle message reducer.

use super::super::{IcedApp, Message};
use super::dispatch::UpdateDispatch;

impl IcedApp {
    pub(super) fn reduce_window_message(&mut self, message: Message) -> UpdateDispatch {
        self.handle_window_message(message)
            .map_or_else(UpdateDispatch::none, UpdateDispatch::task)
    }
}
