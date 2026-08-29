//! Surface-protocol matrix: every arm of the typed surface-protocol table
//! ([`crate::command_palette::TUI_SURFACE_PROTOCOL`]) is driven through the
//! real `handle_key` path while its owning surface is open and proven to
//! produce its observable effect, and the modal-masking invariant is locked:
//! a chord the surface consumes never also fires the global command layer
//! (the hard layer-3 vs layers-1/2 boundary declared in the registry header).
//!
//! Deliberately NOT duplicated here: the settings form's structural
//! navigation (Tab/arrows/Enter/Esc — `settings_export.rs` and
//! `ui/settings_tests.rs`), the panel's structural `q`/Esc close
//! (`service_control.rs`), and the bare-page overlay toggles
//! (`binding_matrix.rs`, `overlays.rs`).

use super::super::*;
use ratatui::crossterm::event::KeyModifiers;
use taskmanager_application::{AppAction, AppPage};
use taskmanager_core::core::services::{ServiceLogLevelFilter, ServiceLogTimeFilter};

use crate::command_palette::{
    TUI_SURFACE_PROTOCOL, TuiSurfaceAction, TuiSurfaceArm, TuiSurfaceScope,
};

fn press_char(app: &mut crate::TuiApp, character: char) -> Option<PlatformEffect> {
    handle_key(
        app,
        KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char(character),
            KeyModifiers::NONE,
        ),
    )
}

fn select_page(app: &mut crate::TuiApp, page: AppPage) {
    let _ = app.apply_action(AppAction::SelectPage(page));
}

fn arms_in(scope: TuiSurfaceScope) -> Vec<TuiSurfaceArm> {
    let mut arms: Vec<TuiSurfaceArm> = TUI_SURFACE_PROTOCOL
        .into_iter()
        .filter(|arm| arm.scope == scope)
        .collect();
    arms.sort_by_key(|arm| arm.chord);
    arms
}

/// The observable toggle state a protocol arm declares, for the four overlay
/// toggles.
fn overlay_open(app: &crate::TuiApp, action: TuiSurfaceAction) -> bool {
    match action {
        TuiSurfaceAction::ToggleSettings => app.settings_open(),
        TuiSurfaceAction::ToggleAbout => app.about_open(),
        TuiSurfaceAction::ToggleHealth => app.health_open(),
        TuiSurfaceAction::ToggleContainers => app.containers_open(),
        TuiSurfaceAction::ToggleServiceLogFollow
        | TuiSurfaceAction::ToggleServiceLogPaused
        | TuiSurfaceAction::CycleServiceLogLevel
        | TuiSurfaceAction::CycleServiceLogTime => {
            panic!("overlay probe called with a service-log action")
        }
    }
}

/// The whole observable feed state the panel chords drive, compared as one
/// value so an arm must move its own control.
fn feed_state(app: &crate::TuiApp) -> (bool, bool, ServiceLogLevelFilter, ServiceLogTimeFilter) {
    let feed = &app
        .shell
        .service_log
        .as_ref()
        .expect("service-log panel open")
        .feed;
    (feed.follow, feed.paused, feed.level, feed.time)
}

fn app_with_settings() -> crate::TuiApp {
    let mut app = crate::demo_app();
    let _ = press_char(&mut app, 'p');
    assert!(
        app.settings_open(),
        "setup: the global p must open settings"
    );
    app
}

fn app_with_overlay(opener: TuiSurfaceAction) -> crate::TuiApp {
    let chord = match opener {
        TuiSurfaceAction::ToggleAbout => 'i',
        TuiSurfaceAction::ToggleHealth => 'h',
        TuiSurfaceAction::ToggleContainers => 'c',
        _ => panic!("setup: only overlay toggles open overlays"),
    };
    let mut app = crate::demo_app();
    let _ = press_char(&mut app, chord);
    assert!(
        overlay_open(&app, opener),
        "setup: {chord:?} must open the overlay"
    );
    app
}

fn app_with_service_log() -> crate::TuiApp {
    let mut app = crate::demo_app();
    select_page(&mut app, AppPage::Services);
    let _ = press_char(&mut app, 'o');
    assert!(
        app.shell.service_log.is_some(),
        "setup: the global o must open the service-log panel"
    );
    app
}

// ── table integrity ───────────────────────────────────────────────────────

/// Each scope declares exactly its historical chord set — no more, no less.
/// A new protocol chord must extend this pin deliberately.
#[test]
fn protocol_declares_exactly_the_historical_chords_per_scope() {
    let expected: [(TuiSurfaceScope, &[char]); 3] = [
        (TuiSurfaceScope::Settings, &['c', 'h', 'i', 'p']),
        (TuiSurfaceScope::StatusOverlay, &['c', 'h', 'i']),
        (TuiSurfaceScope::ServiceLogPanel, &['f', 'l', 'p', 't']),
    ];
    for (scope, chords) in expected {
        let declared: Vec<char> = arms_in(scope).iter().map(|arm| arm.chord).collect();
        assert_eq!(
            declared,
            chords.to_vec(),
            "{scope:?} must declare exactly its historical chords"
        );
    }
}

/// The table carries only action-semantic bare letters: structural surface
/// keys (Esc, Enter, Tab/arrows, the panel's `q` close) are hand-written
/// dispatch and must never migrate in.
#[test]
fn protocol_chords_stay_action_semantics_only() {
    for arm in TUI_SURFACE_PROTOCOL {
        assert!(
            arm.chord.is_ascii_lowercase(),
            "protocol chords are bare action letters; {:?} is structural",
            arm.chord
        );
    }
    assert!(
        !arms_in(TuiSurfaceScope::ServiceLogPanel)
            .iter()
            .any(|arm| arm.chord == 'q'),
        "the panel's structural close must stay out of the protocol table"
    );
}

/// The explicit cross-layer declaration relation: the overlay chords are
/// intentionally declared in BOTH the global registry and the surface
/// protocol; modal precedence makes that single-routed (proven by the
/// masking tests below). Pin the exact overlap so any new chord collision is
/// a reviewed fact, never an accident.
#[test]
fn protocol_chords_overlapping_the_registry_are_declared_deliberately() {
    let registry_chords: Vec<String> = crate::command_palette::TUI_LOCAL_COMMANDS
        .iter()
        .map(|command| command.binding.shortcut.to_owned())
        .collect();
    let mut overlap: Vec<char> = TUI_SURFACE_PROTOCOL
        .into_iter()
        .map(|arm| arm.chord)
        .filter(|chord| registry_chords.contains(&chord.to_string()))
        .collect();
    overlap.sort_unstable();
    overlap.dedup();
    // `t` (TUI-013) is deliberate: the service-log panel consumes its `t`
    // (cycle time filter) first while it owns input, and the registry's `t`
    // arm is scoped to the Performance·Disk page — the two can never route
    // one press twice (same masking invariant the `c h i p` overlaps ride).
    assert_eq!(
        overlap,
        ['c', 'h', 'i', 'p', 't'],
        "every chord declared in both layers must be listed here on purpose"
    );
}

// ── settings-form protocol arms ───────────────────────────────────────────

#[test]
fn every_settings_protocol_arm_runs_its_declared_toggle() {
    for arm in arms_in(TuiSurfaceScope::Settings) {
        let mut app = app_with_settings();
        let before = overlay_open(&app, arm.action);
        let effect = press_char(&mut app, arm.chord);
        assert!(
            effect.is_none(),
            "{:?} produces no platform work",
            arm.chord
        );
        assert_eq!(
            overlay_open(&app, arm.action),
            !before,
            "{:?} inside the settings form must toggle its declared overlay",
            arm.chord
        );
        if arm.action != TuiSurfaceAction::ToggleSettings {
            assert!(
                !app.settings_open(),
                "{:?} must replace the settings surface, not stack on it",
                arm.chord
            );
        }
    }
}

// ── status-overlay protocol arms ──────────────────────────────────────────

#[test]
fn every_status_overlay_protocol_arm_toggles_from_every_overlay() {
    let openers = [
        TuiSurfaceAction::ToggleAbout,
        TuiSurfaceAction::ToggleHealth,
        TuiSurfaceAction::ToggleContainers,
    ];
    for opener in openers {
        for arm in arms_in(TuiSurfaceScope::StatusOverlay) {
            let mut app = app_with_overlay(opener);
            let before = overlay_open(&app, arm.action);
            let effect = press_char(&mut app, arm.chord);
            assert!(
                effect.is_none(),
                "{:?} produces no platform work",
                arm.chord
            );
            assert_eq!(
                overlay_open(&app, arm.action),
                !before,
                "{:?} pressed from the {:?} overlay must toggle its own target",
                arm.chord,
                opener
            );
        }
    }
}

// ── service-log panel protocol arms ───────────────────────────────────────

#[test]
fn every_service_log_protocol_arm_drives_the_shared_feed() {
    for arm in arms_in(TuiSurfaceScope::ServiceLogPanel) {
        let mut app = app_with_service_log();
        let before = feed_state(&app);
        let effect = press_char(&mut app, arm.chord);
        assert!(
            effect.is_none(),
            "panel controls route no platform work: {:?}",
            arm.chord
        );
        assert_ne!(
            feed_state(&app),
            before,
            "{:?} must advance its declared feed control",
            arm.chord
        );
        assert!(
            app.local_surface_kind().is_none(),
            "{:?} is a panel control and must not open an overlay",
            arm.chord
        );
    }
}

// ── the masking invariant (hard layer-3 vs layers-1/2 boundary) ──────────

#[test]
fn settings_modal_masks_global_command_chords() {
    // Control: with no surface up, `x` reaches the export path and reports.
    let mut control = crate::demo_app();
    control.shell.clear_feedback_notice();
    let _ = press_char(&mut control, 'x');
    assert!(
        control.feedback_notice().is_some(),
        "control: the bare x chord must reach the export path"
    );

    // With the settings form up, the same chord is consumed as protocol
    // noise: the export command never fires and the modal stays up.
    let mut app = app_with_settings();
    app.shell.clear_feedback_notice();
    let effect = press_char(&mut app, 'x');
    assert!(effect.is_none(), "a masked chord routes no platform work");
    assert!(
        app.feedback_notice().is_none(),
        "the settings modal must mask the global x command"
    );
    assert!(
        app.settings_open(),
        "the modal survives the swallowed chord"
    );
}

#[test]
fn status_overlays_mask_the_global_command_and_settings_chords() {
    // `p` is declared in BOTH the global registry and the settings protocol;
    // while a status overlay owns input it is swallowed either way.
    let mut app = app_with_overlay(TuiSurfaceAction::ToggleAbout);
    let _ = press_char(&mut app, 'p');
    assert!(
        !app.settings_open(),
        "the overlay must mask the global settings chord"
    );
    assert!(app.about_open(), "the overlay survives the swallowed chord");

    // `x` is likewise consumed by the overlay without reaching the command
    // layer.
    app.shell.clear_feedback_notice();
    let _ = press_char(&mut app, 'x');
    assert!(
        app.feedback_notice().is_none(),
        "the overlay must mask the global x command"
    );
    assert!(app.about_open());
}

#[test]
fn service_log_panel_masks_claimed_chords_and_falls_through_the_rest() {
    // `p` is claimed by the panel protocol: the feed pauses, and the global
    // settings command of the same chord never fires.
    let mut app = app_with_service_log();
    let _ = press_char(&mut app, 'p');
    assert!(
        app.shell
            .service_log
            .as_ref()
            .expect("panel up")
            .feed
            .paused,
        "the panel's own p must pause the feed"
    );
    assert!(
        !app.settings_open(),
        "the panel outranks the global settings chord"
    );

    // `i` is NOT claimed by the panel: it falls through to the global layer —
    // the deliberate partial ownership of a partial-owner surface.
    let _ = press_char(&mut app, 'i');
    assert!(
        app.about_open(),
        "unclaimed chords must reach the command layer"
    );
}
