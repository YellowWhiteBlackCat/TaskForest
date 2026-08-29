//! Size-matrix acceptance: every top-level page and a representative overlay
//! roster must render without panicking at the four named size tiers —
//! minimum (54x16), short-wide (70x18), narrow-tall (44x60) and wide
//! (200x50) — with honest content at each tier: real page rows whenever the
//! terminal meets the 54x16 floor, and the honest too-small screen (with
//! overlays suppressed, never half-clipped) below it.

use ratatui::layout::Rect;
use taskmanager_application::i18n::{self, Language};
use taskmanager_application::{AppAction, AppPage};

use super::acceptance_support::{
    battery_surfaces, body_text, surface_title_key, visible_row_count, with_frame_in_language,
};
use crate::TuiApp;
use crate::ui::TuiFramePlan;

const MINIMUM: (u16, u16) = (54, 16);
const SHORT_WIDE: (u16, u16) = (70, 18);
// Below the 54-column floor: the honest degradation tier.
const NARROW_TALL: (u16, u16) = (44, 60);
const WIDE: (u16, u16) = (200, 50);

const PAGES: [AppPage; 7] = [
    AppPage::Performance,
    AppPage::Applications,
    AppPage::Services,
    AppPage::System,
    AppPage::Startup,
    AppPage::Users,
    AppPage::AppHistory,
];

fn page_app(page: AppPage) -> TuiApp {
    let mut app = TuiApp::demo();
    let _ = app.apply_action(AppAction::SelectPage(page));
    app
}

#[test]
fn every_page_paints_body_rows_at_every_meets_floor_size() {
    for page in PAGES {
        for (width, height) in [MINIMUM, SHORT_WIDE, WIDE] {
            let app = page_app(page);
            let frame =
                with_frame_in_language(&app, width, height, Language::En, |frame| frame.to_owned());
            let rows = visible_row_count(&body_text(&frame, height));
            assert!(
                rows > 0,
                "{page:?} painted no body rows at {width}x{height} — the page is invisible"
            );
        }
    }
}

#[test]
fn below_the_floor_every_page_degrades_to_the_honest_too_small_screen() {
    let (width, height) = NARROW_TALL;
    for page in PAGES {
        let app = page_app(page);
        with_frame_in_language(&app, width, height, Language::En, |frame| {
            let title = i18n::t("empty.terminal_too_small");
            assert_ne!(title, "empty.terminal_too_small");
            assert!(
                frame.contains(title),
                "{page:?} at {width}x{height} must show the honest too-small screen"
            );
            assert!(
                frame.contains("54×16"),
                "the too-small screen must state the 54×16 floor"
            );
            // Honest degradation: no page content leaks past the mask.
            assert!(
                !frame.contains("zed"),
                "{page:?} at {width}x{height} must not paint demo page rows below the floor"
            );
        });
    }
}

#[test]
fn key_overlays_paint_clamped_but_titled_at_every_meets_floor_size() {
    // A representative overlay roster: the largest forms (settings, command
    // palette, process properties), a mid-size reference (help) and the
    // smallest surfaces (process menu, end-task confirmation).
    let wanted = [
        "settings",
        "help",
        "command palette",
        "process menu",
        "process properties",
        "end-task confirmation",
    ];
    for name in wanted {
        let surface = battery_surfaces()
            .into_iter()
            .find(|surface| surface.name == name)
            .unwrap_or_else(|| panic!("{name} must be part of the battery roster"));
        for (width, height) in [MINIMUM, SHORT_WIDE, WIDE] {
            let mut app = TuiApp::demo();
            assert!(
                (surface.open)(&mut app),
                "{name} must open on the demo fixture"
            );
            // The committed geometry keeps the popup reachable: clamped, not
            // zeroed, at every size that meets the floor.
            let plan = TuiFramePlan::build(&app, Rect::new(0, 0, width, height));
            let popup = plan
                .overlay()
                .unwrap_or_else(|| panic!("{name} must own an overlay at {width}x{height}"))
                .popup;
            assert!(
                popup.width >= 8 && popup.height >= 4,
                "{name} popup collapsed to {popup:?} at {width}x{height}"
            );
            with_frame_in_language(&app, width, height, Language::En, |frame| {
                let title_key = surface_title_key(name).expect("roster entry has a title key");
                let title = i18n::t(title_key);
                assert!(
                    frame.contains(title),
                    "{name} must keep its title {title:?} when clamped to {width}x{height}"
                );
            });
        }
    }
}

#[test]
fn below_the_floor_overlays_are_suppressed_not_half_clipped() {
    let (width, height) = NARROW_TALL;
    for surface in battery_surfaces() {
        if !surface.popup {
            continue;
        }
        let mut app = TuiApp::demo();
        assert!(
            (surface.open)(&mut app),
            "{} must open on the demo fixture",
            surface.name
        );
        with_frame_in_language(&app, width, height, Language::En, |frame| {
            let too_small = i18n::t("empty.terminal_too_small");
            assert!(
                frame.contains(too_small),
                "{} below the floor must degrade to the too-small screen",
                surface.name
            );
            if let Some(title_key) = surface_title_key(surface.name) {
                let title = i18n::t(title_key);
                assert_ne!(title, title_key);
                assert!(
                    !frame.contains(title),
                    "{} must not paint its popup below the floor; seen title {title:?}",
                    surface.name
                );
            }
        });
    }
}

#[test]
fn the_wide_and_tall_extremes_render_the_full_chrome() {
    let app = page_app(AppPage::Applications);
    for (width, height) in [WIDE, (200, 16), (54, 50)] {
        with_frame_in_language(&app, width, height, Language::En, |frame| {
            // Chrome identity survives every legal extreme shape. Below the
            // 68-column breakpoint the header paints the compact `TF`
            // monogram instead of the full name, and the paragraph trim
            // drops the span's leading pad — so the narrow tier matches the
            // monogram on the header row itself.
            let brand = taskmanager_assets::product::NAME;
            let header = frame.lines().next().unwrap_or_default();
            assert!(
                frame.contains(brand) || header.contains("TF"),
                "{width}x{height} must paint the product identity in the header"
            );
            let body = body_text(frame, height);
            assert!(
                visible_row_count(&body) > 0,
                "{width}x{height} must paint body rows"
            );
        });
    }
}
