//! Current settings choices derived from their live authorities.
use super::{CAPACITY_CHOICES, ChoiceEntry, SettingsChoice, SettingsField, capacity_choice_index};
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
