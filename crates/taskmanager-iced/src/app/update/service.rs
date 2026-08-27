//! Services inventory, details and log message reducer adapter.

use super::super::{IcedApp, Message};
use super::dispatch::UpdateDispatch;

impl IcedApp {
    pub(super) fn reduce_service_message(&mut self, message: Message) -> UpdateDispatch {
        let mut clipboard = None;
        let effect = self.handle_service_message(message, &mut clipboard);
        let mut dispatch = UpdateDispatch::effect(effect);
        if let Some(task) = clipboard {
            dispatch = dispatch.with_task(task);
        }
        dispatch
    }
}
