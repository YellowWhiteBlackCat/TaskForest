//! Frontend Alerts route and local event-history message reducer.

use super::super::{IcedApp, Message};
use super::dispatch::UpdateDispatch;

impl IcedApp {
    pub(super) fn reduce_alerts_message(&mut self, message: Message) -> UpdateDispatch {
        let effect = match message {
            Message::ClearAlertEvents => {
                self.alert_center.events.clear();
                None
            }
            Message::ExportAlertEvents => None,
            Message::Alerts(message) => self.update_alerts(message),
            _ => None,
        };
        UpdateDispatch::effect(effect)
    }
}
