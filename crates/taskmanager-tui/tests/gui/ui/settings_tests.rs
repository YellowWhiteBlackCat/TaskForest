use super::*;

#[test]
fn form_navigation_wraps_at_edges_and_changes_values() {
    let mut form = SettingsForm::default();
    form.move_field(-1);
    assert_eq!(form.field, 0, "up from the top stays at the top");
    form.move_field(99);
    assert_eq!(
        form.field,
        SETTINGS_FIELDS - 1,
        "down clamps at the last field"
    );

    form.field = 0;
    form.step_value(1);
    assert_eq!(form.skin, 1, "skin steps to KDE");
    form.step_value(-1);
    assert_eq!(form.skin, 0, "skin steps back to GNOME");
    form.step_value(-1);
    assert_eq!(form.skin, 3, "skin wraps to macOS");

    form.field = 2;
    form.step_value(1);
    assert!(form.hc, "the high-contrast toggle flips");
    form.step_value(-1);
    assert!(!form.hc, "and flips back");

    form.field = 29;
    form.step_value(1);
    assert!(form.history_persistence);
    let mut config = taskmanager_application::Config::default();
    let mut theme = crate::ThemeParams::default();
    apply_settings_to_config(&form, &mut config, &mut theme);
    assert!(config.history_persistence);
}

#[test]
fn form_tokens_match_the_config_vocabulary() {
    let mut form = SettingsForm {
        skin: 1,
        ..SettingsForm::default()
    };
    assert_eq!(form.skin_token(), "KDE");
    form.mode = 3;
    assert_eq!(form.mode_token(), "System");
    form.mode = 0;
    assert_eq!(form.mode_token(), "Light");
    form.ui_font = 1;
    assert_eq!(form.ui_font_token(), "MiSans VF");
    form.mono_font = 2;
    assert_eq!(form.mono_font_token(), "Roboto Mono");
    form.density = 1;
    assert_eq!(form.density_token(), "Compact");
}

#[test]
fn form_seeds_from_opaque_config_tokens_with_unknown_fallbacks() {
    let form = SettingsForm::from_config_tokens(
        "KDE",
        "System",
        true,
        "",
        "Roboto Mono",
        "Compact",
        (true, Some((22 * 60, 7 * 60))),
    );
    assert_eq!(form.skin, 1);
    assert_eq!(form.mode, 3);
    assert!(form.hc);
    assert_eq!(form.ui_font, 0);
    assert_eq!(form.mono_font, 2);
    assert_eq!(form.density, 1);
    assert!(form.notify_enabled);
    assert_eq!(form.quiet_start, 22);
    assert_eq!(form.quiet_end, 7);

    let defaults =
        SettingsForm::from_config_tokens("", "", false, "unknown", "unknown", "", (false, None));
    assert_eq!(defaults, SettingsForm::default());
}

/// G-22: every language index round-trips through its persisted token,
/// and only the two known tokens map back (anything else — including no
/// recorded preference — keeps English, the fallback-chain base).
#[test]
fn language_tokens_round_trip_with_unknown_falling_back_to_english() {
    for index in 0..LANGUAGE_TOKENS.len() {
        let form = SettingsForm {
            language: index,
            ..SettingsForm::default()
        };
        let token = form.language_token();
        assert_eq!(
            SettingsForm::language_index_for(Some(token)),
            index,
            "token {token} must restore index {index}"
        );
    }
    assert_eq!(SettingsForm::language_index_for(None), 0);
    assert_eq!(SettingsForm::language_index_for(Some("fr")), 0);
    assert_eq!(SettingsForm::language_index_for(Some("")), 0);

    // The bundle mapping: a recorded token resolves to its Language; an
    // unrecorded one keeps the host-detected locale (None).
    assert_eq!(
        SettingsForm::language_for_token(Some("zh")),
        Some(taskmanager_application::i18n::Language::Zh)
    );
    assert_eq!(
        SettingsForm::language_for_token(Some("en")),
        Some(taskmanager_application::i18n::Language::En)
    );
    assert_eq!(
        SettingsForm::language_for_token(Some("fr")),
        Some(taskmanager_application::i18n::Language::En)
    );
    assert_eq!(SettingsForm::language_for_token(None), None);
}
