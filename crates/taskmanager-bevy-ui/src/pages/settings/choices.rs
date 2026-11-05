//! Current settings choices derived from their live authorities.
use super::{
    CAPACITY_CHOICES, ChoiceEntry, REFRESH_CHOICES_MS, SettingsChoice, SettingsField,
    capacity_choice_index, refresh_choice_index,
};
use std::time::Duration;
use taskmanager_application::TelemetryInterval;
use taskmanager_application::i18n::{Language, t};
use taskmanager_theme::LightDark;
use taskmanager_theme::tokens::UiSize;

/// The UI-size ladder in display order — the same three steps the GPUI/Iced
/// settings rows expose, each a whole-surface type-scale choice.
pub(super) fn ui_size_entries(current: UiSize) -> Vec<ChoiceEntry> {
    UiSize::ALL
        .into_iter()
        .map(|size| ChoiceEntry {
            label: ui_size_label(size).to_owned(),
            choice: SettingsChoice(SettingsField::UiSize(size)),
            selected: current == size,
        })
        .collect()
}

/// Display labels for the size ladder, resolved from the shared catalog (the
/// config keeps the typed `config_token()` vocabulary for persistence).
pub(super) fn ui_size_label(size: UiSize) -> &'static str {
    t(match size {
        UiSize::Small => "settings.ui_size_small",
        UiSize::Standard => "settings.ui_size_standard",
        UiSize::Large => "settings.ui_size_large",
    })
}

pub(super) fn theme_entries(mode: Option<LightDark>) -> Vec<ChoiceEntry> {
    vec![
        ChoiceEntry {
            label: t("settings.system").to_owned(),
            choice: SettingsChoice(SettingsField::SystemMode),
            selected: mode.is_none(),
        },
        ChoiceEntry {
            label: t("settings.light").to_owned(),
            choice: SettingsChoice(SettingsField::Theme(LightDark::Light)),
            selected: mode == Some(LightDark::Light),
        },
        ChoiceEntry {
            label: t("settings.dark").to_owned(),
            choice: SettingsChoice(SettingsField::Theme(LightDark::Dark)),
            selected: mode == Some(LightDark::Dark),
        },
    ]
}

pub(super) fn language_entries(language: Language) -> Vec<ChoiceEntry> {
    // Each language names itself through the owner's catalog key, so the picker
    // enumerates the owner's `Language::ALL` instead of a frontend-private list.
    Language::ALL
        .iter()
        .copied()
        .map(|value| ChoiceEntry {
            label: t(value.self_name_key()).to_owned(),
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
