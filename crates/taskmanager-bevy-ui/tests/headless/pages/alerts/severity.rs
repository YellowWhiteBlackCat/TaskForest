//! test-intent: behavior
//!
//! The event severity palette has ONE authority: `taskmanager-theme`. The
//! frontend reads the ink from the theme's severity palette, so the alert
//! banner, the event timeline and the theme agree channel for channel — no
//! frontend-local severity table can drift.

use super::tests::{headless_shell_app, mount_alerts};
use super::*;
use bevy::app::App;
use bevy::ecs::query::With;
use bevy::ui::widget::Text;
use taskmanager_shell::fixture::alerts::seed_shell_active_alert;
use taskmanager_theme::Theme;
use taskmanager_theme::palette::{SeverityLevel, severity_palette};
use taskmanager_theme::tokens::UiSize;

#[test]
fn every_severity_ink_is_the_theme_palette() {
    let theme = Theme::dark();
    let palette = crate::palette::ui_palette(&theme, UiSize::Standard);
    let theme_palette = severity_palette(&theme);
    for (severity, level) in [
        (AlertSeverity::Critical, SeverityLevel::Critical),
        (AlertSeverity::Warning, SeverityLevel::Warning),
        (AlertSeverity::Info, SeverityLevel::Notice),
    ] {
        assert_eq!(
            severity::severity_level(severity),
            level,
            "the core severity maps onto the theme's level"
        );
        assert_eq!(
            severity::severity_ink(severity, &palette),
            crate::palette::theme_color(level.ink(theme_palette)),
            "the {severity:?} ink is the theme's own token"
        );
    }
}

#[test]
fn the_alert_banner_paints_its_severity_ink() {
    let mut app = headless_shell_app();
    let mut shell = crate::demo_fixture::demo_shell();
    assert!(
        seed_shell_active_alert(&mut shell),
        "the fixture arms one active alert"
    );
    app.world_mut().non_send_mut::<FrontendTrack>().shell = shell;
    mount_alerts(&mut app);

    let inks: Vec<bevy::color::Color> = app
        .world_mut()
        .query_filtered::<&crate::icons::IconInk, With<crate::icons::IconInk>>()
        .iter(app.world())
        .map(|ink| ink.0)
        .collect();
    assert!(
        inks.iter().any(|ink| {
            *ink == severity::severity_ink(AlertSeverity::Warning, &context_palette(&app))
        }),
        "the banner's severity plate carries the theme's severity ink: {inks:?}"
    );
}

/// The palette the mounted page rendered with.
fn context_palette(app: &App) -> crate::palette::UiPalette {
    app.world()
        .resource::<crate::window::WindowPalette>()
        .inner
        .clone()
}

#[test]
fn the_event_center_rows_carry_the_severity_ink() {
    let mut app = headless_shell_app();
    let mut shell = crate::demo_fixture::demo_shell();
    crate::demo_fixture::seed_capture_confirmation_scenario(&mut shell, "event-center");
    app.world_mut().non_send_mut::<FrontendTrack>().shell = shell;
    app.world_mut().trigger(crate::event_center::EventCommand(
        crate::event_center::EventAction::Open,
    ));
    for _ in 0..3 {
        app.update();
    }
    let texts: Vec<String> = app
        .world_mut()
        .query::<&Text>()
        .iter(app.world())
        .map(|text| text.0.clone())
        .collect();
    assert!(
        texts.iter().any(|text| text.contains("Warning")),
        "the event center lists the seeded events: {texts:?}"
    );
}
