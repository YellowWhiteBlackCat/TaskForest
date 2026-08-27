// test-intent: behavior
//! Upgrade regressions for the grouped Settings surface: every control
//! callback maps to the sanctioned [`SettingsChange`] (the same messages the
//! legacy pill rows published), the slider grids keep every legacy choosable
//! value exactly reachable, the quiet-hour selects reproduce the persisted
//! quiet-hours token, and the shortcut legend derives from the Iced binding
//! declaration rather than a copied key table.

use taskmanager_shell::command_help;
use taskmanager_theme::tokens::{MotionPolicy, UiSize};
use taskmanager_theme::{FontChoice, Skin};
use taskmanager_ui_contract::BindingEntry;

use super::shortcuts::shortcut_rows;
use super::*;
use crate::app::ModeChoice;
use crate::ui::overlays::binding_declaration;

/// Every segmented/switch callback mapper resolves the control's value to
/// the exact enum the legacy pill published, and unknown persisted tokens
/// light no segment instead of being coerced.
#[test]
fn control_value_mappers_reproduce_legacy_choices() {
    // Skin: values follow Skin::ALL order; unknown tokens select nothing.
    for (index, skin) in Skin::ALL.into_iter().enumerate() {
        assert_eq!(skin_for_value(index), skin);
        assert_eq!(skin_value(skin.label()), index);
    }
    assert_eq!(skin_value("not-a-skin"), SEGMENT_NONE);

    // Mode: the four segments and the empty→System first-launch sentinel.
    assert_eq!(mode_for_value(0), ModeChoice::Light);
    assert_eq!(mode_for_value(1), ModeChoice::Dark);
    assert_eq!(mode_for_value(2), ModeChoice::EyeForest);
    assert_eq!(mode_for_value(3), ModeChoice::System);
    assert_eq!(mode_for_value(usize::MAX), ModeChoice::System);
    assert_eq!(mode_value("Light"), 0);
    assert_eq!(mode_value("Dark"), 1);
    assert_eq!(mode_value("EyeForest"), 2);
    assert_eq!(mode_value("System"), 3);
    assert_eq!(mode_value(""), 3);
    assert_eq!(mode_value("nope"), SEGMENT_NONE);

    // Interface size follows UiSize::ALL; the fallback is the Standard
    // default, never a panic.
    for (index, size) in UiSize::ALL.into_iter().enumerate() {
        assert_eq!(ui_size_for_value(index), size);
        assert_eq!(ui_size_value(size), index);
    }
    assert_eq!(ui_size_for_value(usize::MAX), UiSize::Standard);

    // Density and the two-state unit axes: 1 = Compact / Bits / Base 10.
    assert_eq!(density_value(true), 1);
    assert_eq!(density_value(false), 0);
    assert_eq!(unit_toggle_value(true), 0);
    assert_eq!(unit_toggle_value(false), 1);

    // Font source: 0 = System, 1 = Bundled, custom families select nothing.
    assert_eq!(font_choice_for_value(0), FontChoice::System);
    assert_eq!(font_choice_for_value(1), FontChoice::Bundled);
    assert_eq!(font_choice_value(true, false), 0);
    assert_eq!(font_choice_value(false, true), 1);
    assert_eq!(font_choice_value(false, false), SEGMENT_NONE);

    // Motion: the three segments follow MotionPolicy::ALL over the persisted
    // "normal"/"reduced"/"none" wire tokens; the empty sentinel and unknown
    // tokens light the Normal segment because that is the policy the
    // snapshot seam actually installs for them — the control shows what is
    // really in effect.
    for (index, (policy, token)) in [
        (MotionPolicy::Normal, "normal"),
        (MotionPolicy::Reduced, "reduced"),
        (MotionPolicy::NoMotion, "none"),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(motion_for_value(index), policy);
        assert_eq!(motion_value(token), index);
    }
    assert_eq!(motion_for_value(usize::MAX), MotionPolicy::Normal);
    assert_eq!(motion_value(""), 0);
    assert_eq!(motion_value("  REDUCED "), 1);
    assert_eq!(motion_value("warp-speed"), 0);
}

/// The slider step grids keep every legacy pill value exactly reachable and
/// round-trip it into the identical persisted token (the component snaps
/// onto the same `start + round((v - start) / step) * step` grid).
#[test]
fn slider_grids_keep_legacy_values_exactly_reachable() {
    let refresh_legacy: [u64; 4] = [500, 1_000, 2_000, 5_000];
    for millis in refresh_legacy {
        let secs = millis as f32 / 1000.0;
        let steps = ((secs - REFRESH_MIN_S) / REFRESH_STEP_S).round();
        assert!(
            (secs - REFRESH_MIN_S - steps * REFRESH_STEP_S).abs() < 1e-6,
            "refresh {millis} ms must sit on the 0.1 s grid"
        );
        assert_eq!(refresh_value_to_ms(secs), millis);
    }
    // Endpoints are exact (the snap keeps both ends precise).
    assert_eq!(refresh_value_to_ms(REFRESH_MIN_S), 500);
    assert_eq!(refresh_value_to_ms(REFRESH_MAX_S), 5_000);

    let points_legacy: [usize; 5] = [10, 60, 120, 300, 600];
    for points in points_legacy {
        let value = points as f32;
        let steps = ((value - GRAPH_POINTS_MIN) / GRAPH_POINTS_STEP).round();
        assert!(
            (value - GRAPH_POINTS_MIN - steps * GRAPH_POINTS_STEP).abs() < 1e-6,
            "data points {points} must sit on the 10-point grid"
        );
        assert_eq!(graph_points_for_value(value), points);
    }
    assert_eq!(graph_points_for_value(GRAPH_POINTS_MIN), 10);
    assert_eq!(graph_points_for_value(GRAPH_POINTS_MAX), 600);
}

/// The settings changes every new control publishes apply through the
/// unchanged coordinator path and round-trip back through the preference
/// projection (identical persisted tokens).
#[test]
fn control_changes_round_trip_through_preferences() {
    let mut app = crate::IcedApp::demo();
    let apply = |app: &mut crate::IcedApp, change: SettingsChange| {
        let _ = app.update(Message::SettingsChanged(change));
    };

    apply(&mut app, SettingsChange::Skin(Skin::Kde));
    assert!(app.preferences().skin.eq_ignore_ascii_case("KDE"));
    apply(&mut app, SettingsChange::Mode(ModeChoice::Dark));
    assert!(app.preferences().mode.eq_ignore_ascii_case("Dark"));
    apply(&mut app, SettingsChange::UiSize(UiSize::Large));
    assert_eq!(app.ui_size(), UiSize::Large);
    apply(&mut app, SettingsChange::CompactDensity(true));
    assert!(app.preferences().density.eq_ignore_ascii_case("Compact"));
    apply(&mut app, SettingsChange::HighContrast(true));
    assert!(app.preferences().hc);
    apply(&mut app, SettingsChange::Motion(MotionPolicy::Reduced));
    assert_eq!(app.preferences().motion, "reduced");
    assert_eq!(app.motion_policy(), MotionPolicy::Reduced);

    apply(&mut app, SettingsChange::RefreshInterval(2_000));
    assert_eq!(app.preferences().refresh_ms, 2_000);
    apply(&mut app, SettingsChange::GraphDataPoints(300));
    assert_eq!(app.preferences().graph_data_points, 300);

    apply(
        &mut app,
        SettingsChange::ShowDevice(DeviceKind::Gpus, false),
    );
    assert!(!app.preferences().device_visible(DeviceKind::Gpus));
    apply(&mut app, SettingsChange::NetworkDynamicScaling(false));
    assert!(!app.preferences().network_dynamic_scaling);
    apply(&mut app, SettingsChange::GrayZeroValues(true));
    assert!(app.preferences().gray_zero_values);

    // The startup select publishes exactly the three legacy tokens.
    for choice in startup_choices(crate::i18n::Language::En) {
        apply(&mut app, SettingsChange::StartupPage(choice.token));
        assert_eq!(app.preferences().startup_page, choice.token);
    }
}

/// Quiet-hour selects cover 0..=23 with `HH:00` labels, and the hour pair
/// reproduces the persisted quiet-hours token (equal boundaries clear it).
#[test]
fn quiet_hour_selects_reproduce_persisted_tokens() {
    let hours = quiet_hours();
    assert_eq!(hours.len(), 24);
    assert_eq!(hours[0].to_string(), "00:00");
    assert_eq!(hours[7].to_string(), "07:00");
    assert_eq!(hours[23].to_string(), "23:00");

    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::SettingsChanged(SettingsChange::QuietHoursStart(
        22,
    )));
    let _ = app.update(Message::SettingsChanged(SettingsChange::QuietHoursEnd(7)));
    let prefs = app.preferences();
    assert_eq!((prefs.quiet_start, prefs.quiet_end), (22, 7));

    // Equal start/end = no quiet hours; the projection mirrors it as (0, 0).
    let _ = app.update(Message::SettingsChanged(SettingsChange::QuietHoursEnd(22)));
    let cleared = app.preferences();
    assert_eq!((cleared.quiet_start, cleared.quiet_end), (0, 0));
}

/// The settings shortcut legend derives from the Iced binding declaration:
/// one row per declared command, the key token copied from the declaration
/// (never a second key table), labels joined from the shared command
/// vocabulary, and a deliberate `Unbound` rendered as the not-bound state.
#[test]
fn shortcut_rows_derive_from_the_binding_declaration() {
    let declaration = binding_declaration();
    let rows = shortcut_rows(&declaration);
    assert_eq!(rows.len(), declaration.entries.len());
    for (row, entry) in rows.iter().zip(&declaration.entries) {
        assert_eq!(row.keys, entry.binding.key_token());
    }

    // Labels come from the shared command_help vocabulary.
    let commands = command_help();
    for row in &rows {
        assert!(
            commands.iter().any(|help| help.label == row.label),
            "row label {} must come from command_help",
            row.label
        );
    }

    // A deliberately unbound declaration entry renders the not-bound state
    // instead of a key token.
    let mut with_unbound = declaration.clone();
    let first_command = with_unbound.entries[0].command;
    with_unbound.entries[0] = BindingEntry::unbound(first_command);
    let unbound_rows = shortcut_rows(&with_unbound);
    assert_eq!(unbound_rows.len(), with_unbound.entries.len());
    assert_eq!(unbound_rows[0].keys, None);
}

/// The grouped surface still renders headless from non-default preference
/// mirrors (every control constructor runs for non-default tokens).
#[test]
fn grouped_modal_renders_from_non_default_preferences() {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::SettingsChanged(SettingsChange::Skin(Skin::Macos)));
    let _ = app.update(Message::SettingsChanged(SettingsChange::Mode(
        ModeChoice::EyeForest,
    )));
    let _ = app.update(Message::SettingsChanged(SettingsChange::CompactDensity(
        true,
    )));
    let _ = app.update(Message::SettingsChanged(SettingsChange::RefreshInterval(
        5_000,
    )));
    let _ = app.update(Message::SettingsChanged(SettingsChange::GraphDataPoints(
        600,
    )));
    let _ = app.update(Message::SettingsChanged(SettingsChange::QuietHoursStart(
        22,
    )));
    let _ = app.update(Message::SettingsChanged(SettingsChange::QuietHoursEnd(7)));
    {
        let view = render(&app);
        let _ = view;
    }
}
