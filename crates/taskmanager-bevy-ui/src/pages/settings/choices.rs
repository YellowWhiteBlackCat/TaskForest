//! Current settings choices derived from their live authorities.
use super::{
    CAPACITY_CHOICES, ChoiceEntry, REFRESH_CHOICES_MS, SettingsChoice, SettingsField,
    capacity_choice_index, refresh_choice_index,
};
use std::time::Duration;
use taskmanager_application::TelemetryInterval;
use taskmanager_application::i18n::Language;
use taskmanager_theme::LightDark;

pub(super) fn theme_entries(mode: Option<LightDark>) -> Vec<ChoiceEntry> {
    vec![
        ChoiceEntry {
            label: "System".to_owned(),
            choice: SettingsChoice(SettingsField::SystemMode),
            selected: mode.is_none(),
        },
        ChoiceEntry {
            label: "Light".to_owned(),
            choice: SettingsChoice(SettingsField::Theme(LightDark::Light)),
            selected: mode == Some(LightDark::Light),
        },
        ChoiceEntry {
            label: "Dark".to_owned(),
            choice: SettingsChoice(SettingsField::Theme(LightDark::Dark)),
            selected: mode == Some(LightDark::Dark),
        },
    ]
}

pub(super) fn language_entries(language: Language) -> Vec<ChoiceEntry> {
    [(Language::En, "English"), (Language::Zh, "中文")]
        .into_iter()
        .map(|(value, label)| ChoiceEntry {
            label: label.to_owned(),
            choice: SettingsChoice(SettingsField::Language(value)),
            selected: language == value,
        })
        .collect()
}

pub(super) fn refresh_entries(interval: TelemetryInterval) -> Vec<ChoiceEntry> {
    let selected = refresh_choice_index(interval);
    REFRESH_CHOICES_MS
        .iter()
        .enumerate()
        .map(|(index, millis)| ChoiceEntry {
            label: refresh_label(*millis),
            choice: SettingsChoice(SettingsField::Refresh(interval_for_millis(*millis))),
            selected: selected == Some(index),
        })
        .collect()
}

pub(super) fn capacity_entries(capacity: usize) -> Vec<ChoiceEntry> {
    let selected = capacity_choice_index(capacity);
    CAPACITY_CHOICES
        .iter()
        .enumerate()
        .map(|(index, samples)| ChoiceEntry {
            label: samples.to_string(),
            choice: SettingsChoice(SettingsField::HistoryCapacity(*samples)),
            selected: selected == Some(index),
        })
        .collect()
}

/// The `TelemetryInterval` for one offered cadence step. The ladder lives
/// inside the policy's clamp window, so `clamped` never deviates from the
/// requested step.
fn interval_for_millis(millis: u64) -> TelemetryInterval {
    TelemetryInterval::clamped(Duration::from_millis(millis))
}

pub(super) fn refresh_label(millis: u64) -> String {
    format!(
        "{} s",
        f64::from(u32::try_from(millis).unwrap_or(u32::MAX)) / 1000.0
    )
}
