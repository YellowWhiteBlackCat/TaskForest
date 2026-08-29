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
    let mut config = taskmanager_core::core::config::Config::default();
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

/// Every overlay label must come from the shared i18n catalog; a label that
/// leaks its raw catalog key means a `t()` wrapper was forgotten at a call
/// site (visible in every locale, not just English).
#[test]
fn overlay_labels_resolve_through_the_shared_catalog() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let mut app = crate::demo_app();
    app.toggle_settings();

    let backend = ratatui::backend::TestBackend::new(70, 34);
    let mut terminal = ratatui::Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| {
            render_settings_overlay_at(
                frame,
                &app.settings_form,
                crate::TuiTheme::default(),
                crate::ui::frame_plan::TuiFocusPlan {
                    target: crate::ui::frame_plan::TuiFocusTarget::LocalSurface(
                        crate::TuiSurfaceKind::Settings,
                    ),
                    order: crate::ui::frame_plan::TuiFocusOrder::None,
                    control: crate::ui::frame_plan::TuiFocusControl::SettingsField(0),
                },
                ratatui::layout::Rect::new(0, 0, 68, 32),
            );
        })
        .expect("draw");
    let text = terminal.backend().to_string();
    assert!(
        !text.contains("settings."),
        "overlay labels must resolve through the shared catalog instead of \
         leaking raw keys, got:\n{text}"
    );
}

/// TUI-002 tail: a successful settings save reconciles the Performance
/// resource anchor in the SAME call. Closing the family of the currently
/// selected resource fails closed to the first still-backed resource
/// immediately (the save must not wait for the next platform batch to
/// reconcile), and re-opening a family only adds resources, so the live
/// selection never drifts.
#[test]
fn settings_save_reconciles_the_perf_device_selection_immediately() {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let mut app = crate::demo_app();
    assert!(
        app.visible_perf_devices()
            .contains(&crate::PerfDevice::Disk),
        "the demo fixture must back the Disk resource for this scenario"
    );

    // Select Disk, close its family, save: the anchor must fall back now,
    // before any new platform batch arrives.
    app.select_perf_device(crate::PerfDevice::Disk);
    app.begin_settings_edit();
    app.settings_form.show[2] = false;
    assert!(
        app.apply_settings_form(),
        "demo-mode saves apply locally without a config client"
    );
    let visible = app.visible_perf_devices();
    assert!(
        !visible.contains(&crate::PerfDevice::Disk),
        "the closed family must be gone from the visible set"
    );
    assert_eq!(
        Some(app.perf_device),
        visible.first().copied(),
        "the selection must fail closed to the first still-backed resource"
    );

    // Re-opening the family adds resources only: no drift.
    let reconciled = app.perf_device;
    app.begin_settings_edit();
    app.settings_form.show[2] = true;
    assert!(app.apply_settings_form());
    assert_eq!(
        app.perf_device, reconciled,
        "enabling a family must not move the selection"
    );
    assert!(
        app.visible_perf_devices()
            .contains(&crate::PerfDevice::Disk),
        "the reopened family is selectable again"
    );
}
