use super::event_kind_label;
use taskmanager_application::i18n::{self, Language};
use taskmanager_core::core::alerts::AlertEventKind;

#[test]
fn alert_event_kinds_use_the_shared_locale_catalog() {
    let prior = i18n::current_language();

    i18n::set_language(Language::En);
    assert_eq!(
        event_kind_label(AlertEventKind::Activated),
        "Alert activated"
    );
    assert_eq!(event_kind_label(AlertEventKind::Cleared), "Cleared");

    i18n::set_language(Language::Zh);
    assert_eq!(event_kind_label(AlertEventKind::Activated), "告警已触发");
    assert_eq!(event_kind_label(AlertEventKind::Cleared), "已恢复");

    i18n::set_language(prior);
}
