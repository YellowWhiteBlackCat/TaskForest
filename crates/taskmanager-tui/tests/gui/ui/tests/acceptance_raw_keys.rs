//! Full-surface raw catalog-key leak scan.
//!
//! A leak is a call site that painted an i18n key literal without wrapping it
//! in `t()` — or a key missing from the catalogs that fell back to itself.
//! Both render as dotted lowercase key text ("settings.language", "tui.x")
//! somewhere on the surface. The battery opens every overlay surface in both
//! shipped locales at the reference size and asserts the painted frame never
//! contains an exact catalog key nor any key-shaped run over the exact prefix
//! list ([`super::acceptance_support::RAW_KEY_PREFIXES`]).

use ratatui::layout::Rect;
use taskmanager_application::i18n::{Language, current_language};
use taskmanager_application::{AppAction, AppPage};

use super::acceptance_support::{
    REFERENCE_HEIGHT, REFERENCE_WIDTH, assert_frame_has_no_raw_catalog_keys, battery_surfaces,
    frame_in_language,
};
use crate::{TuiApp, ui::TuiFramePlan};

fn set_language_blocking(language: Language) {
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    taskmanager_application::i18n::set_language(language);
    assert_eq!(
        current_language(),
        language,
        "language pin must take effect"
    );
}

/// Open one battery surface and fail loudly when it did not actually install
/// its popup (a silent no-op would make the frame scan vacuous).
fn open_and_install(surface: &super::acceptance_support::BatterySurface, app: &mut TuiApp) {
    assert!(
        (surface.open)(app),
        "{} must open on the demo fixture",
        surface.name
    );
    if surface.popup {
        let plan = TuiFramePlan::build(app, Rect::new(0, 0, REFERENCE_WIDTH, REFERENCE_HEIGHT));
        assert!(
            plan.overlay().is_some(),
            "{} must own an overlay popup once open",
            surface.name
        );
    } else {
        assert!(
            app.shell.service_log.is_some(),
            "the service log panel must be open"
        );
    }
}

#[test]
fn every_overlay_surface_paints_no_raw_catalog_keys_in_english() {
    set_language_blocking(Language::En);
    for surface in battery_surfaces() {
        let mut app = TuiApp::demo();
        open_and_install(&surface, &mut app);
        let frame = frame_in_language(&app, REFERENCE_WIDTH, REFERENCE_HEIGHT, Language::En);
        assert_frame_has_no_raw_catalog_keys(&frame, surface.name, Language::En);
    }
}

#[test]
fn every_overlay_surface_paints_no_raw_catalog_keys_in_chinese() {
    set_language_blocking(Language::Zh);
    for surface in battery_surfaces() {
        let mut app = TuiApp::demo();
        open_and_install(&surface, &mut app);
        let frame = frame_in_language(&app, REFERENCE_WIDTH, REFERENCE_HEIGHT, Language::Zh);
        assert_frame_has_no_raw_catalog_keys(&frame, surface.name, Language::Zh);
    }
}

#[test]
fn every_top_level_page_paints_no_raw_catalog_keys_in_either_locale() {
    let pages = [
        AppPage::Performance,
        AppPage::Applications,
        AppPage::Services,
        AppPage::System,
        AppPage::Startup,
        AppPage::Users,
        AppPage::AppHistory,
    ];
    for language in [Language::En, Language::Zh] {
        set_language_blocking(language);
        for page in pages {
            let mut app = TuiApp::demo();
            let _ = app.apply_action(AppAction::SelectPage(page));
            let frame = frame_in_language(&app, REFERENCE_WIDTH, REFERENCE_HEIGHT, language);
            assert_frame_has_no_raw_catalog_keys(&frame, &format!("page {page:?}"), language);
        }
    }
}
