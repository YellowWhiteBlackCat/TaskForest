//! Frontend Alerts route and shared event-history message reducer.

use taskmanager_application::i18n::t;
use taskmanager_core::core::alerts::export_alert_events_json;
use taskmanager_shell::{FeedbackLifecycle, FeedbackSeverity, FeedbackSource};

use super::super::{IcedApp, Message};
use super::dispatch::UpdateDispatch;

impl IcedApp {
    pub(super) fn reduce_alerts_message(&mut self, message: Message) -> UpdateDispatch {
        let effect = match message {
            Message::ClearAlertEvents => {
                self.shell.clear_alert_event_history();
                None
            }
            Message::ExportAlertEvents => {
                let events = self.shell.projection().alert_center.event_history();
                match export_alert_events_json(events) {
                    Ok(json) => {
                        self.shell.report_notice(
                            FeedbackSource::Clipboard,
                            FeedbackSeverity::Success,
                            FeedbackLifecycle::SHORT,
                            format!("{} {}", t("events.title"), t("common.copied")),
                        );
                        return UpdateDispatch::task(iced::clipboard::write(json));
                    }
                    Err(error) => {
                        self.shell.report_notice(
                            FeedbackSource::Clipboard,
                            FeedbackSeverity::Error,
                            FeedbackLifecycle::SHORT,
                            format!("{}: {error}", t("events.title")),
                        );
                    }
                }
                None
            }
            Message::Alerts(message) => self.update_alerts(message),
            _ => None,
        };
        UpdateDispatch::effect(effect)
    }
}
