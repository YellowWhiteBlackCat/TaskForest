//! Zh-locale whole-frame honesty: the same overlay surfaces the leak scan
//! visits must paint their localized titles under `Zh` (sampled through
//! `t()`, never hardcoded copy), and the English frame must keep painting the
//! English catalog value for the same key — one surface drifting to a
//! hardcoded string breaks one side of this pair.

use taskmanager_application::i18n::{self, Language};
use taskmanager_application::{AppAction, AppPage};

use super::acceptance_support::{
    REFERENCE_HEIGHT, REFERENCE_WIDTH, battery_surfaces, surface_title_key, with_frame_in_language,
};
use crate::TuiApp;

#[test]
fn every_titled_overlay_surface_paints_its_localized_title_in_both_locales() {
    for surface in battery_surfaces() {
        let Some(key) = surface_title_key(surface.name) else {
            continue;
        };
        let mut app = TuiApp::demo();
        assert!(
            (surface.open)(&mut app),
            "{} must open on the demo fixture",
            surface.name
        );

        // Render first, resolve the expectation inside the same language
        // scope the frame was painted with (`t()` reads the process-global
        // language, so the closure runs while the guard is held).
        let (en_frame, en) = with_frame_in_language(
            &app,
            REFERENCE_WIDTH,
            REFERENCE_HEIGHT,
            Language::En,
            |frame| (frame.to_owned(), i18n::t(key)),
        );
        let (zh_frame, zh) = with_frame_in_language(
            &app,
            REFERENCE_WIDTH,
            REFERENCE_HEIGHT,
            Language::Zh,
            |frame| (frame.to_owned(), i18n::t(key)),
        );

        // Catalog sanity: both locales must carry the key with distinct copy,
        // or the two frame assertions could not detect a hardcoded string.
        assert_ne!(en, zh, "{key} must translate to distinct En/Zh copy");
        assert_ne!(en, key, "{key} must exist in the en catalog");
        assert_ne!(zh, key, "{key} must exist in the zh catalog");
        assert!(
            en_frame.contains(en),
            "{} must paint its English title {en:?} (resolved from {key})",
            surface.name
        );
        assert!(
            zh_frame.contains(zh),
            "{} must paint its Chinese title {zh:?} (resolved from {key})",
            surface.name
        );
    }
}

#[test]
fn the_settings_form_body_is_translated_under_zh_not_just_its_title() {
    let mut app = TuiApp::demo();
    app.toggle_settings();
    with_frame_in_language(
        &app,
        REFERENCE_WIDTH,
        REFERENCE_HEIGHT,
        Language::Zh,
        |frame| {
            let skin = i18n::t("settings.skin");
            assert_ne!(
                skin, "settings.skin",
                "settings.skin must exist in the zh catalog"
            );
            assert!(
                frame.contains(skin),
                "the settings form rows must paint localized labels under Zh; expected {skin:?}"
            );
        },
    );
}

#[test]
fn the_zh_end_task_confirmation_names_the_frozen_process_in_chinese() {
    let mut app = TuiApp::demo();
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Applications));
    assert!(
        app.open_process_menu(),
        "demo fixture exposes a process menu"
    );
    let _ = app.process_menu_select();
    // The gate's frozen target is the authority for which row was armed; the
    // behavior under test here is the Zh rendering of THAT identity, so the
    // expectation is derived from the gate itself rather than a hardcoded
    // row assumption (selection anchoring is owned by the selection layer).
    let frozen = match app.shell.pending_confirmation() {
        Some(taskmanager_application::PendingConfirmation::EndTask(target)) => target,
        other => panic!("an end-task confirmation must be pending, got {other:?}"),
    };
    with_frame_in_language(
        &app,
        REFERENCE_WIDTH,
        REFERENCE_HEIGHT,
        Language::Zh,
        |frame| {
            // The headline template resolves with the frozen identity, so build
            // the expected painted text through the same substitution the
            // renderer performs rather than a hardcoded sentence.
            let headline = i18n::t("confirm.end_headline")
                .replacen("{name}", frozen.name.as_str(), 1)
                .replacen("{pid}", &frozen.pid.to_string(), 1);
            assert!(
                frame.contains(&headline),
                "the Zh confirmation must paint the localized frozen-target headline {headline:?}"
            );
        },
    );
}

#[test]
fn every_top_level_page_paints_its_localized_tab_label_in_both_locales() {
    let pages = [
        (AppPage::Performance, "tab.performance"),
        (AppPage::Applications, "tab.apps"),
        (AppPage::Services, "tab.services"),
        (AppPage::System, "tab.system"),
        (AppPage::Startup, "tab.startup"),
        (AppPage::Users, "tab.users"),
        (AppPage::AppHistory, "tab.apphistory_short"),
    ];
    for (page, key) in pages {
        let mut app = TuiApp::demo();
        let _ = app.apply_action(AppAction::SelectPage(page));
        for language in [Language::En, Language::Zh] {
            let (frame, label) = with_frame_in_language(
                &app,
                REFERENCE_WIDTH,
                REFERENCE_HEIGHT,
                language,
                |frame| (frame.to_owned(), i18n::t(key)),
            );
            assert_ne!(
                label, key,
                "{key} must exist in the catalog for {language:?}"
            );
            assert!(
                frame.contains(label),
                "{page:?} must paint its {language:?} tab label {label:?} (resolved from {key})"
            );
        }
    }
}
