// test-intent: behavior
//
// Behavior tests for the inputs component family: the state/token mapping
// seams (keyboard navigation, step snapping, track fills, clear gating) and
// construction of every control across skins, modes, and degenerate states
// — the palette sweep exercises the style/geometry code paths where a
// broken token read would otherwise panic in a real frame.

use super::*;
use taskmanager_theme::{HighContrast, LightDark, Skin};

fn theme_for(skin: Skin, mode: LightDark) -> Theme {
    Theme::build(
        skin,
        mode,
        HighContrast::Off,
        taskmanager_theme::ResolvedFonts::system_for(skin),
    )
}

fn target() -> FocusTarget {
    FocusTarget::StartupControl
}

/// Arrow navigation wraps past both ends, never treats an unknown active
/// value as a position, and refuses to move in a choice-less or
/// single-choice group.
#[test]
fn segmented_keyboard_navigation_wraps_and_degrades() {
    let choices = [
        ("Slow".to_string(), 0),
        ("Normal".to_string(), 1),
        ("Fast".to_string(), 2),
    ];
    assert_eq!(segmented_active_index(&choices, 1), Some(1));
    assert_eq!(segmented_neighbor_index(&choices, 1, true), Some(2));
    assert_eq!(segmented_neighbor_index(&choices, 1, false), Some(0));
    assert_eq!(segmented_neighbor_index(&choices, 2, true), Some(0));
    assert_eq!(segmented_neighbor_index(&choices, 0, false), Some(2));
    assert_eq!(segmented_active_index(&choices, 42), None);
    assert_eq!(segmented_neighbor_index(&choices, 42, true), None);
    let single = [("Only".to_string(), 7)];
    assert_eq!(segmented_active_index(&single, 7), Some(0));
    assert_eq!(segmented_neighbor_index(&single, 7, false), None);
    assert_eq!(segmented_active_index(&[], 0), None);
}

/// Slider values clamp to the range, snap onto the step grid anchored at
/// the range start, and degrade safely for zero/negative/NaN steps and
/// unordered ranges (never a division by zero or an out-of-range value).
#[test]
fn slider_values_clamp_and_snap_to_the_step_grid() {
    let range = 0.0..=10.0_f32;
    assert_eq!(slider_snapped_value(&range, 3.0, 5.2), 6.0);
    assert_eq!(slider_snapped_value(&range, 3.0, 4.2), 3.0);
    assert_eq!(slider_snapped_value(&range, 3.0, 12.0), 10.0);
    assert_eq!(slider_snapped_value(&range, 3.0, -4.0), 0.0);
    assert_eq!(slider_snapped_value(&range, 0.0, 4.7), 4.7);
    assert_eq!(slider_snapped_value(&range, -2.0, 4.7), 4.7);
    assert_eq!(slider_snapped_value(&range, f32::NAN, 4.7), 4.7);
    assert_eq!(slider_snapped_value(&(10.0..=0.0), 1.0, 4.0), 4.0);
    assert_eq!(slider_snapped_value(&(5.0..=5.0), 1.0, 9.0), 5.0);
}

/// The switch track reads its fill off the palette tokens per state, and
/// the disabled status dims toward the backdrop instead of leaving the
/// accent (an unavailable control must not look actionable).
#[test]
fn switch_track_fill_maps_state_onto_palette_tokens() {
    let theme = theme_for(Skin::Gnome, LightDark::Dark);
    let palette = theme.palette();
    assert_eq!(
        switch_track_fill(&theme, true, button::Status::Active),
        taskmanager_theme::iced::color(palette.accent)
    );
    assert_eq!(
        switch_track_fill(&theme, false, button::Status::Active),
        taskmanager_theme::iced::color(palette.border)
    );
    let disabled = switch_track_fill(&theme, true, button::Status::Disabled);
    assert_ne!(disabled, taskmanager_theme::iced::color(palette.accent));
    assert_eq!(
        disabled,
        taskmanager_theme::iced::color(mix(palette.accent, palette.window_backdrop, 0.5))
    );
}

/// Every control constructs without I/O across every skin and mode, in its
/// normal and its degenerate shapes (disabled switch, degenerate slider
/// step, empty/unselected select, empty/valued search query, unknown-active
/// and empty segmented groups).
#[test]
fn input_controls_build_across_skins_modes_and_degenerate_states() {
    for skin in Skin::ALL {
        for mode in [LightDark::Light, LightDark::Dark, LightDark::EyeForest] {
            let theme = theme_for(skin, mode);

            let _ = switch(
                &theme,
                target(),
                true,
                false,
                Message::RequestStartupControl,
            );
            let _ = switch(
                &theme,
                target(),
                false,
                false,
                Message::RequestStartupControl,
            );
            let _ = switch(&theme, target(), true, true, Message::RequestStartupControl);

            let _ = slider(
                &theme,
                target(),
                0.0..=100.0,
                5.0,
                42.0,
                |value| Message::RequestStartupControl(value > 50.0),
                Some(|value| format!("{value:.0}%")),
            );
            let _ = slider(
                &theme,
                target(),
                0.0..=100.0,
                0.0,
                42.0,
                |_| Message::Tick,
                None::<fn(f32) -> String>,
            );

            let options = vec!["Five seconds".to_string(), "Sixty seconds".to_string()];
            let _ = select(
                &theme,
                target(),
                &options,
                Some(&options[0]),
                "pick one",
                |_| Message::Tick,
            );
            let _ = select(&theme, target(), &options, None, "pick one", |_| {
                Message::Tick
            });
            let empty: Vec<String> = Vec::new();
            let _ = select(&theme, target(), &empty, None, "pick one", |_| {
                Message::Tick
            });

            let _ = search_input(
                &theme,
                target(),
                "Search…",
                "",
                Message::ServicesSearchChanged,
            );
            let _ = search_input(
                &theme,
                target(),
                "Search…",
                "disk",
                Message::ServicesSearchChanged,
            );
            assert!(!search_shows_clear(""));
            assert!(search_shows_clear("disk"));

            let choices = vec![("Low".to_string(), 0), ("High".to_string(), 1)];
            let _ = segmented(&theme, target(), &choices, 1, |value| {
                Message::SelectPerformanceGraphPoints(value as u32)
            });
            let _ = segmented(&theme, target(), &choices, 42, |_| Message::Tick);
            let _ = segmented(&theme, target(), &[], 0, |_| Message::Tick);
        }
    }
}
